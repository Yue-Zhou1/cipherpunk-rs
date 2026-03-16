use audit_agent_core::finding::Finding;
use audit_agent_core::output::AuditManifest;

pub fn render_technical_report(findings: &[Finding], manifest: &AuditManifest) -> String {
    let mut out = String::new();
    out.push_str("# Technical Audit Report\n\n");
    out.push_str(&format!("- Audit ID: `{}`\n", manifest.audit_id));
    out.push_str(&format!("- Agent Version: `{}`\n", manifest.agent_version));
    out.push_str(&format!(
        "- Source Commit: `{}`\n\n",
        manifest.source.commit_hash
    ));

    if findings.is_empty() {
        out.push_str("No findings.\n");
        return out;
    }

    for finding in findings {
        out.push_str(&format!("## [{}] {}\n\n", finding.id, finding.title));
        out.push_str(&format!("- Severity: `{:?}`\n", finding.severity));
        out.push_str(&format!("- Category: `{:?}`\n", finding.category));
        out.push_str(&format!("- Framework: `{:?}`\n", finding.framework));

        // Verification status badge
        match &finding.verification_status {
            audit_agent_core::finding::VerificationStatus::Verified => {
                out.push_str("- **Status:** \u{2705} Verified\n");
                out.push_str("  > Backed by tool output. Reproducible.\n");
            }
            audit_agent_core::finding::VerificationStatus::Unverified { reason } => {
                out.push_str("- **Status:** \u{26a0} Unverified\n");
                out.push_str(&format!("  > {reason}\n"));
                if matches!(
                    finding.category,
                    audit_agent_core::finding::FindingCategory::Incentive
                ) && matches!(
                    finding.severity,
                    audit_agent_core::finding::Severity::Observation
                ) {
                    out.push_str("  > Unverified — requires manual protocol review.\n");
                }
            }
        }

        let analysis_source = match finding
            .evidence
            .tool_versions
            .get("analysis_origin")
            .map(String::as_str)
        {
            Some("cache") => "Cached",
            _ => "New analysis",
        };
        out.push_str(&format!("- Analysis Source: {analysis_source}\n"));

        if let Some(primary) = finding.affected_components.first() {
            out.push_str(&format!(
                "- Location: `{}`:{}-{}\n",
                primary.file.display(),
                primary.line_range.0,
                primary.line_range.1
            ));
            if let Some(snippet) = &primary.snippet {
                out.push_str("\n```rust\n");
                out.push_str(snippet);
                if !snippet.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("```\n\n");
            }
        }

        // Attack scenario
        if !finding.prerequisites.is_empty() || !finding.exploit_path.is_empty() {
            out.push_str("### Attack Scenario\n");
            if !finding.prerequisites.is_empty() {
                out.push_str(&format!("- **Prerequisites:** {}\n", finding.prerequisites));
            }
            if !finding.exploit_path.is_empty() {
                out.push_str(&format!("- **Exploit Path:** {}\n", finding.exploit_path));
            }
            out.push('\n');
        }

        let reproduce = finding
            .evidence
            .command
            .clone()
            .unwrap_or_else(|| format!("bash evidence-pack/{}/reproduce.sh", finding.id));
        out.push_str(&format!("- Reproduce: `{reproduce}`\n\n"));
        out.push_str(&format!("### Impact\n{}\n\n", finding.impact));
        out.push_str(&format!(
            "### Recommendation\n{}\n\n",
            finding.recommendation
        ));
    }

    out
}
