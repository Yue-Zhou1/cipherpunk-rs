use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use audit_agent_core::audit_config::ParsedPreviousAudit;
use audit_agent_core::finding::Finding;
use audit_agent_core::output::AuditManifest;
use findings::json_export::to_findings_json;
use findings::sarif::to_sarif;
use llm::{CompletionOpts, LlmProvider, LlmRole, llm_call};

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
        let mut findings = self.findings.clone();
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

        // PDF rendering is intentionally minimal here: write compact report text stubs with .pdf
        // extension so downstream integrations have stable output files for productization tests.
        std::fs::write(
            output_dir.join("report-executive.pdf"),
            clamp_lines(&report_executive_markdown, 120),
        )?;
        std::fs::write(
            output_dir.join("report-technical.pdf"),
            &report_technical_markdown,
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

fn mark_regression_checks(findings: &mut [Finding], previous_audit: &ParsedPreviousAudit) {
    let prior_ids = previous_audit
        .prior_findings
        .iter()
        .map(|finding| finding.id.clone())
        .collect::<std::collections::HashSet<_>>();
    let prior_titles = previous_audit
        .prior_findings
        .iter()
        .map(|finding| finding.title.to_ascii_lowercase())
        .collect::<std::collections::HashSet<_>>();

    for finding in findings {
        let id = finding.id.to_string();
        if prior_ids.contains(&id) || prior_titles.contains(&finding.title.to_ascii_lowercase()) {
            finding.regression_check = true;
        }
    }
}

async fn polish_text(llm: &dyn LlmProvider, text: &str) -> (String, bool) {
    let prompt = format!(
        "Improve readability of this security text without changing technical content. \
         Output only improved text:\n\n{text}"
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

fn clamp_lines(input: &str, max_lines: usize) -> String {
    input.lines().take(max_lines).collect::<Vec<_>>().join("\n")
}
