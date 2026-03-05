use audit_agent_core::finding::{Finding, Severity};
use audit_agent_core::output::{AuditManifest, FindingCounts};

pub fn render_executive_report(findings: &[Finding], manifest: &AuditManifest) -> String {
    let counts = FindingCounts::from(findings);
    let score = counts.risk_score();
    let band = risk_band(score);
    let recommendation = overall_recommendation(score);

    let mut out = String::new();
    out.push_str("# Executive Summary\n\n");
    out.push_str(&format!("- **Audit ID:** `{}`\n", manifest.audit_id));
    out.push_str(&format!(
        "- **Agent Version:** `{}`\n",
        manifest.agent_version
    ));
    out.push_str(&format!(
        "- **Source Commit:** `{}`\n",
        manifest.source.commit_hash
    ));
    out.push_str(&format!("- **Risk Score:** {score} ({band})\n"));
    out.push_str(&format!("- **Recommendation:** {recommendation}\n\n"));

    out.push_str("## Finding Summary\n\n");
    out.push_str(&format!(
        "| Severity | Count |\n|----------|-------|\n| Critical | {} |\n| High | {} |\n| Medium | {} |\n| Low | {} |\n| Observation | {} |\n\n",
        counts.critical, counts.high, counts.medium, counts.low, counts.observation
    ));

    // Top 5 findings by severity (exclude Observation)
    let mut top: Vec<&Finding> = findings
        .iter()
        .filter(|f| !matches!(f.severity, Severity::Observation))
        .collect();
    top.sort_by_key(|f| match f.severity {
        Severity::Critical => 0,
        Severity::High => 1,
        Severity::Medium => 2,
        Severity::Low => 3,
        Severity::Observation => 4,
    });
    top.truncate(5);

    if !top.is_empty() {
        out.push_str("## Top Findings\n\n");
        for finding in top {
            out.push_str(&format!(
                "**[{}] {}** ({:?}): {}\n\n",
                finding.id, finding.title, finding.severity, finding.impact
            ));
        }
    }

    out.push_str(
        "> **Note:** The risk score is an orientation signal. Consult the finding count table \
         and technical report for full severity breakdown. Do not use the score alone for \
         automated go/no-go decisions beyond the three-band gate.\n",
    );

    out
}

fn risk_band(score: u8) -> &'static str {
    if score >= 70 {
        "Deploy"
    } else if score >= 50 {
        "Fix before deploy"
    } else {
        "Do not deploy"
    }
}

fn overall_recommendation(score: u8) -> &'static str {
    risk_band(score)
}
