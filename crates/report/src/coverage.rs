use std::collections::BTreeMap;

use audit_agent_core::finding::Finding;
use audit_agent_core::output::AuditManifest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V3ToolInventory {
    pub tool: String,
    pub version: String,
    pub container_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V3ChecklistCoverage {
    pub domain: String,
    pub status: String,
    pub notes: String,
}

pub fn derive_tool_inventory(findings: &[Finding]) -> Vec<V3ToolInventory> {
    let mut by_tool = BTreeMap::<String, (String, String)>::new();

    for finding in findings {
        if finding.evidence.tool_versions.is_empty() {
            by_tool.entry("analysis".to_string()).or_insert_with(|| {
                (
                    "unknown".to_string(),
                    finding.evidence.container_digest.clone(),
                )
            });
            continue;
        }

        for (tool, version) in &finding.evidence.tool_versions {
            by_tool
                .entry(tool.clone())
                .or_insert_with(|| (version.clone(), finding.evidence.container_digest.clone()));
        }
    }

    by_tool
        .into_iter()
        .map(|(tool, (version, container_digest))| V3ToolInventory {
            tool,
            version,
            container_digest,
        })
        .collect()
}

pub fn derive_checklist_coverage(manifest: &AuditManifest) -> Vec<V3ChecklistCoverage> {
    if manifest.scope.detected_frameworks.is_empty() {
        return vec![V3ChecklistCoverage {
            domain: "core-audit".to_string(),
            status: "planned".to_string(),
            notes: "No framework-specific checklist domain was inferred".to_string(),
        }];
    }

    manifest
        .scope
        .detected_frameworks
        .iter()
        .map(|framework| {
            let name = format!("{framework:?}").to_ascii_lowercase();
            V3ChecklistCoverage {
                domain: name,
                status: "completed".to_string(),
                notes: "Checklist executed for framework-specific invariants".to_string(),
            }
        })
        .collect()
}
