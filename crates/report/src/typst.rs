use crate::generator::V3ReportBundle;

const EXECUTIVE_TEMPLATE: &str = include_str!("../templates/executive.typ");
const TECHNICAL_TEMPLATE: &str = include_str!("../templates/technical.typ");
const CANDIDATES_TEMPLATE: &str = include_str!("../templates/candidates.typ");

pub fn render_technical_typst(bundle: &V3ReportBundle) -> String {
    let technical = TECHNICAL_TEMPLATE
        .replace("<audit_id>", &bundle.manifest.audit_id)
        .replace("<tool_inventory>", &render_tool_inventory(bundle))
        .replace("<checklist_coverage>", &render_checklist_coverage(bundle))
        .replace("<verified_findings>", &render_verified_findings(bundle));
    let candidates = CANDIDATES_TEMPLATE.replace("<candidates>", &render_candidates(bundle));
    format!("{technical}\n\n{candidates}")
}

pub fn render_executive_typst(bundle: &V3ReportBundle) -> String {
    EXECUTIVE_TEMPLATE
        .replace("<audit_id>", &bundle.manifest.audit_id)
        .replace("<risk_score>", &bundle.manifest.risk_score.to_string())
        .replace("<source_commit>", &bundle.manifest.source.commit_hash)
        .replace("<target_crates>", &target_crates(bundle))
        .replace("<top_findings>", &render_top_findings(bundle))
}

fn render_top_findings(bundle: &V3ReportBundle) -> String {
    if bundle.findings.is_empty() {
        return "- No verified findings.".to_string();
    }

    bundle
        .findings
        .iter()
        .take(5)
        .map(|finding| {
            format!(
                "- [{}] {} ({:?})",
                finding.id, finding.title, finding.severity
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_tool_inventory(bundle: &V3ReportBundle) -> String {
    if bundle.tool_inventory.is_empty() {
        return "- No tool inventory captured.".to_string();
    }

    bundle
        .tool_inventory
        .iter()
        .map(|item| {
            format!(
                "- {} `{}` (container `{}`)",
                item.tool, item.version, item.container_digest
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_checklist_coverage(bundle: &V3ReportBundle) -> String {
    if bundle.checklist_coverage.is_empty() {
        return "- No checklist coverage metadata available.".to_string();
    }

    bundle
        .checklist_coverage
        .iter()
        .map(|coverage| {
            format!(
                "- {}: {} ({})",
                coverage.domain, coverage.status, coverage.notes
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_verified_findings(bundle: &V3ReportBundle) -> String {
    if bundle.findings.is_empty() {
        return "No verified findings.".to_string();
    }

    bundle
        .findings
        .iter()
        .map(|finding| {
            let reproduce = finding.evidence.command.as_deref().map_or_else(
                || format!("bash evidence-pack/{}/reproduce.sh", finding.id),
                str::to_string,
            );
            format!(
                "- [{}] {} ({:?})\n  - Reproduce: `{}`\n  - Evidence Digest: `{}`",
                finding.id,
                finding.title,
                finding.severity,
                reproduce,
                finding.evidence.container_digest
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_candidates(bundle: &V3ReportBundle) -> String {
    if bundle.candidates.is_empty() {
        return "No unverified candidates.".to_string();
    }

    bundle
        .candidates
        .iter()
        .map(|candidate| format!("- {}: {}", candidate.title, candidate.summary))
        .collect::<Vec<_>>()
        .join("\n")
}

fn target_crates(bundle: &V3ReportBundle) -> String {
    if bundle.manifest.scope.target_crates.is_empty() {
        "n/a".to_string()
    } else {
        bundle.manifest.scope.target_crates.join(", ")
    }
}
