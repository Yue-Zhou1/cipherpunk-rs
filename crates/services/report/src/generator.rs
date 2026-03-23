use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use audit_agent_core::audit_config::ParsedPreviousAudit;
use audit_agent_core::finding::Finding;
use audit_agent_core::output::AuditManifest;
use audit_agent_core::session::AuditRecord;
use findings::json_export::to_findings_json;
use findings::pipeline::{deduplicate_findings, mark_regression_checks};
use findings::sarif::to_sarif;
use llm::{LlmProvider, LlmRole, role_aware_llm_call};
use printpdf::{BuiltinFont, Mm, PdfDocument};

pub use crate::coverage::{V3ChecklistCoverage, V3ToolInventory};
use crate::coverage::{derive_checklist_coverage, derive_tool_inventory};
use crate::executive::render_executive_report;
use crate::regression::{generate_regression_tests, write_phase1_output_layout};
use crate::typst::{render_executive_typst, render_technical_typst};

pub struct ReportGenerator {
    findings: Vec<Finding>,
    manifest: AuditManifest,
    options: ReportGeneratorOptions,
}

pub struct ReportGeneratorOptions {
    pub llm: Option<Arc<dyn LlmProvider>>,
    pub no_llm_prose: bool,
    pub evidence_pack_zip: PathBuf,
    pub previous_audit: Option<ParsedPreviousAudit>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct V3ReportBundle {
    pub manifest: AuditManifest,
    pub findings: Vec<Finding>,
    pub candidates: Vec<AuditRecord>,
    pub tool_inventory: Vec<V3ToolInventory>,
    pub checklist_coverage: Vec<V3ChecklistCoverage>,
    pub recommended_fixes: Vec<String>,
    pub regression_plan: Vec<String>,
}

pub fn render_v3_report(bundle: V3ReportBundle) -> String {
    let mut out = String::new();
    out.push_str("# Technical Audit Report\n\n");
    out.push_str("## Project Metadata and Scope\n\n");
    out.push_str(&format!("- Audit ID: `{}`\n", bundle.manifest.audit_id));
    out.push_str(&format!(
        "- Source Commit: `{}`\n",
        bundle.manifest.source.commit_hash
    ));
    out.push_str(&format!(
        "- Target Crates: {}\n",
        if bundle.manifest.scope.target_crates.is_empty() {
            "n/a".to_string()
        } else {
            bundle.manifest.scope.target_crates.join(", ")
        }
    ));
    out.push('\n');

    out.push_str("## Tool Inventory\n\n");
    if bundle.tool_inventory.is_empty() {
        out.push_str("- No tool inventory captured.\n\n");
    } else {
        for item in &bundle.tool_inventory {
            out.push_str(&format!(
                "- {} `{}` (container `{}`)\n",
                item.tool, item.version, item.container_digest
            ));
        }
        out.push('\n');
    }

    out.push_str("## Checklist Coverage\n\n");
    if bundle.checklist_coverage.is_empty() {
        out.push_str("- No checklist coverage metadata available.\n\n");
    } else {
        for coverage in &bundle.checklist_coverage {
            out.push_str(&format!(
                "- {}: {} ({})\n",
                coverage.domain, coverage.status, coverage.notes
            ));
        }
        out.push('\n');
    }

    out.push_str("## Verified Findings\n\n");
    if bundle.findings.is_empty() {
        out.push_str("No verified findings.\n\n");
    } else {
        for finding in &bundle.findings {
            out.push_str(&format!("### [{}] {}\n\n", finding.id, finding.title));
            out.push_str(&format!("- Severity: `{:?}`\n", finding.severity));
            out.push_str(&format!("- Impact: {}\n", finding.impact));
            if let Some(command) = finding.evidence.command.as_deref() {
                out.push_str(&format!("- Reproduce: `{command}`\n"));
            } else {
                out.push_str(&format!(
                    "- Reproduce: `bash evidence-pack/{}/reproduce.sh`\n",
                    finding.id
                ));
            }
            out.push_str(&format!(
                "- Evidence Digest: `{}`\n\n",
                finding.evidence.container_digest
            ));
            out.push_str(&format!(
                "#### Recommendation\n{}\n\n",
                finding.recommendation
            ));
        }
    }

    out.push_str("## Unverified Candidates\n\n");
    if bundle.candidates.is_empty() {
        out.push_str("No unverified candidates.\n\n");
    } else {
        for candidate in &bundle.candidates {
            out.push_str(&format!(
                "- **{}**: {}\n",
                candidate.title, candidate.summary
            ));
        }
        out.push('\n');
    }

    out.push_str("## Recommended Fixes\n\n");
    if bundle.recommended_fixes.is_empty() {
        out.push_str("- No fix recommendations were generated.\n\n");
    } else {
        for fix in &bundle.recommended_fixes {
            out.push_str(&format!("- {fix}\n"));
        }
        out.push('\n');
    }

    out.push_str("## Regression-Test Section\n\n");
    if bundle.regression_plan.is_empty() {
        out.push_str("- No regression plan captured.\n");
    } else {
        for step in &bundle.regression_plan {
            out.push_str(&format!("- {step}\n"));
        }
    }

    out
}

fn build_v3_report_bundle(manifest: &AuditManifest, findings: &[Finding]) -> V3ReportBundle {
    V3ReportBundle {
        manifest: manifest.clone(),
        findings: findings.to_vec(),
        candidates: vec![],
        tool_inventory: derive_tool_inventory(findings),
        checklist_coverage: derive_checklist_coverage(manifest),
        recommended_fixes: findings
            .iter()
            .map(|finding| finding.recommendation.clone())
            .collect(),
        regression_plan: findings
            .iter()
            .filter_map(|finding| finding.regression_test.clone())
            .collect(),
    }
}

impl ReportGenerator {
    pub fn new(
        findings: Vec<Finding>,
        manifest: AuditManifest,
        options: ReportGeneratorOptions,
    ) -> Self {
        Self {
            findings,
            manifest,
            options,
        }
    }

    pub async fn generate_all(&self, output_dir: &Path) -> Result<()> {
        let mut findings = deduplicate_findings(&self.findings);
        if let Some(previous_audit) = &self.options.previous_audit {
            mark_regression_checks(&mut findings, previous_audit);
        }

        let (render_findings, llm_prose_used) = self.prose_for_reporting(findings).await;

        let mut manifest = self.manifest.clone();
        manifest.optional_inputs_used.llm_prose_used = llm_prose_used;

        let v3_bundle = build_v3_report_bundle(&manifest, &render_findings);
        let report_executive_markdown = render_executive_report(&render_findings, &manifest);
        let report_technical_markdown = render_v3_report(v3_bundle.clone());
        let findings_json = to_findings_json(&render_findings)?;
        let findings_sarif = serde_json::to_string_pretty(&to_sarif(&render_findings, &manifest))?;
        let regression_suite = generate_regression_tests(&render_findings);
        let audit_manifest_json = serde_json::to_string_pretty(&manifest)?;

        write_phase1_output_layout(
            output_dir,
            &report_executive_markdown,
            &report_technical_markdown,
            &findings_json,
            &findings_sarif,
            &self.options.evidence_pack_zip,
            &audit_manifest_json,
            &regression_suite,
        )?;

        fs::write(
            output_dir.join("report-executive.typ"),
            render_executive_typst(&v3_bundle),
        )?;
        fs::write(
            output_dir.join("report-technical.typ"),
            render_technical_typst(&v3_bundle),
        )?;

        write_plain_text_pdf(
            &output_dir.join("report-executive.pdf"),
            &report_executive_markdown,
            Some(2),
        )?;
        write_plain_text_pdf(
            &output_dir.join("report-technical.pdf"),
            &report_technical_markdown,
            None,
        )?;
        Ok(())
    }

    async fn prose_for_reporting(&self, findings: Vec<Finding>) -> (Vec<Finding>, bool) {
        if self.options.no_llm_prose {
            return (findings, false);
        }
        let Some(llm) = &self.options.llm else {
            return (findings, false);
        };

        let mut used = false;
        let mut polished = Vec::with_capacity(findings.len());
        for finding in findings {
            let mut next = finding;

            let (impact, impact_used) = polish_text(llm.as_ref(), &next.impact).await;
            let (recommendation, recommendation_used) =
                polish_text(llm.as_ref(), &next.recommendation).await;
            if impact_used || recommendation_used {
                used = true;
            }
            next.impact = impact;
            next.recommendation = recommendation;
            polished.push(next);
        }

        (polished, used)
    }
}

async fn polish_text(llm: &dyn LlmProvider, text: &str) -> (String, bool) {
    let sanitized = sanitize_prompt_input(text);
    let prompt = format!(
        "Improve readability of this security text without changing technical content. \
         Output only improved text:\n\n{sanitized}"
    );
    match role_aware_llm_call(llm, LlmRole::ProseRendering, &prompt).await {
        Ok((response, provenance)) => {
            tracing::debug!(
                provider = %provenance.provider,
                model = ?provenance.model,
                role = %provenance.role,
                duration_ms = provenance.duration_ms,
                attempt = provenance.attempt,
                "captured report-polish LLM provenance"
            );
            (response, true)
        }
        Err(_) => (text.to_string(), false),
    }
}

fn sanitize_prompt_input(text: &str) -> String {
    const MAX_CHARS: usize = 4_000;
    let redacted = sandbox::redaction::redact_ai_prompt(text);
    redacted.chars().take(MAX_CHARS).collect()
}

fn write_plain_text_pdf(path: &Path, markdown: &str, max_pages: Option<usize>) -> Result<()> {
    const PAGE_WIDTH_MM: f32 = 210.0;
    const PAGE_HEIGHT_MM: f32 = 297.0;
    const MARGIN_X_MM: f32 = 12.0;
    const TOP_Y_MM: f32 = 285.0;
    const BOTTOM_MARGIN_MM: f32 = 12.0;
    const FONT_SIZE_PT: f32 = 10.0;
    const LINE_HEIGHT_MM: f32 = 4.5;
    const MAX_CHARS_PER_LINE: usize = 100;

    let lines = wrap_lines(markdown, MAX_CHARS_PER_LINE);
    let lines_per_page = (((TOP_Y_MM - BOTTOM_MARGIN_MM) / LINE_HEIGHT_MM).floor() as usize).max(1);
    let max_lines = max_pages.map(|pages| pages.saturating_mul(lines_per_page));

    let rendered_lines = if let Some(limit) = max_lines {
        lines.into_iter().take(limit).collect::<Vec<_>>()
    } else {
        lines
    };

    let page_count = rendered_lines.len().div_ceil(lines_per_page).max(1);
    let (doc, first_page, first_layer) = PdfDocument::new(
        "Audit Agent Report",
        Mm(PAGE_WIDTH_MM),
        Mm(PAGE_HEIGHT_MM),
        "Layer 1",
    );
    let font = doc.add_builtin_font(BuiltinFont::Helvetica)?;

    for page_idx in 0..page_count {
        let (page, layer) = if page_idx == 0 {
            (first_page, first_layer)
        } else {
            doc.add_page(
                Mm(PAGE_WIDTH_MM),
                Mm(PAGE_HEIGHT_MM),
                format!("Layer {}", page_idx + 1),
            )
        };
        let current_layer = doc.get_page(page).get_layer(layer);

        let start = page_idx * lines_per_page;
        let end = ((page_idx + 1) * lines_per_page).min(rendered_lines.len());
        for (line_idx, line) in rendered_lines[start..end].iter().enumerate() {
            let y = TOP_Y_MM - (line_idx as f32 * LINE_HEIGHT_MM);
            current_layer.use_text(line, FONT_SIZE_PT, Mm(MARGIN_X_MM), Mm(y), &font);
        }
    }

    let mut writer = BufWriter::new(File::create(path)?);
    doc.save(&mut writer)?;
    Ok(())
}

fn wrap_lines(text: &str, max_chars: usize) -> Vec<String> {
    let mut out = Vec::<String>::new();
    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            out.push(String::new());
            continue;
        }

        let mut current = String::new();
        for word in line.split_whitespace() {
            if current.is_empty() {
                current.push_str(word);
                continue;
            }
            if current.len() + 1 + word.len() > max_chars {
                out.push(current);
                current = word.to_string();
            } else {
                current.push(' ');
                current.push_str(word);
            }
        }
        if !current.is_empty() {
            out.push(current);
        }
    }

    if out.is_empty() {
        out.push(String::new());
    }
    out
}
