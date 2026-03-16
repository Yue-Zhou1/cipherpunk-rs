use audit_agent_core::finding::Finding;

pub fn to_findings_json(findings: &[Finding]) -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(findings)?)
}
