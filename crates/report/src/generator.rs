use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use audit_agent_core::audit_config::ParsedPreviousAudit;
use audit_agent_core::finding::Finding;
use audit_agent_core::output::AuditManifest;
use findings::json_export::to_findings_json;
use findings::pipeline::{deduplicate_findings, mark_regression_checks};
use findings::sarif::to_sarif;
use llm::{CompletionOpts, LlmProvider, LlmRole, llm_call};
use printpdf::{BuiltinFont, Mm, PdfDocument};

use crate::executive::render_executive_report;
use crate::regression::{generate_regression_tests, write_phase1_output_layout};
use crate::technical::render_technical_report;

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

        let report_executive_markdown = render_executive_report(&render_findings, &manifest);
        let report_technical_markdown = render_technical_report(&render_findings, &manifest);
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
    match llm_call(
        llm,
        LlmRole::ProseRendering,
        &prompt,
        &CompletionOpts {
            temperature_millis: 200,
            max_tokens: 512,
        },
    )
    .await
    {
        Ok(response) => (response, true),
        Err(_) => (text.to_string(), false),
    }
}

fn sanitize_prompt_input(text: &str) -> String {
    const MAX_CHARS: usize = 4_000;
    let mut cleaned = String::with_capacity(text.len().min(MAX_CHARS));
    for ch in text.chars() {
        if ch == '\n' || ch == '\t' || !ch.is_control() {
            cleaned.push(ch);
        }
        if cleaned.len() >= MAX_CHARS {
            break;
        }
    }

    let mut out = String::new();
    for line in cleaned.lines() {
        let trimmed = line.trim_start();
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("system:")
            || lower.starts_with("assistant:")
            || lower.starts_with("user:")
        {
            out.push_str("[role-label-redacted]\n");
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if out.ends_with('\n') {
        out.pop();
    }

    let mut sanitized = out
        .replace("```", "'''")
        .replace("<|", "< ")
        .replace("|>", " >")
        .replace("<<", "< ")
        .replace(">>", " >");

    for marker in [
        "SYSTEM:",
        "System:",
        "system:",
        "ASSISTANT:",
        "Assistant:",
        "assistant:",
        "USER:",
        "User:",
        "user:",
    ] {
        sanitized = sanitized.replace(marker, "[role-label-redacted]:");
    }
    sanitized
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
