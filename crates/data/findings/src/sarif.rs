use std::path::PathBuf;

use audit_agent_core::finding::{Finding, Severity};
use audit_agent_core::output::AuditManifest;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SarifReport {
    pub version: String,
    #[serde(rename = "$schema")]
    pub schema: String,
    pub runs: Vec<SarifRun>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SarifRun {
    pub tool: SarifTool,
    pub artifacts: Vec<SarifArtifact>,
    pub results: Vec<SarifResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SarifTool {
    pub driver: SarifDriver,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SarifDriver {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SarifArtifact {
    pub location: SarifArtifactLocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SarifArtifactLocation {
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SarifResult {
    #[serde(rename = "ruleId")]
    pub rule_id: String,
    pub level: String,
    pub message: SarifMessage,
    pub locations: Vec<SarifLocation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SarifMessage {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SarifLocation {
    #[serde(rename = "physicalLocation")]
    pub physical_location: SarifPhysicalLocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SarifPhysicalLocation {
    #[serde(rename = "artifactLocation")]
    pub artifact_location: SarifArtifactLocation,
    pub region: SarifRegion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SarifRegion {
    #[serde(rename = "startLine")]
    pub start_line: u32,
    #[serde(rename = "endLine")]
    pub end_line: u32,
}

pub fn to_sarif(findings: &[Finding], manifest: &AuditManifest) -> SarifReport {
    let artifacts = unique_artifacts(findings);
    let results = findings
        .iter()
        .map(|finding| {
            let primary = finding.affected_components.first();
            let (uri, start_line, end_line) = if let Some(location) = primary {
                (
                    location.file.to_string_lossy().to_string(),
                    location.line_range.0,
                    location.line_range.1,
                )
            } else {
                ("unknown".to_string(), 1, 1)
            };

            SarifResult {
                rule_id: finding.id.to_string(),
                level: severity_to_level(&finding.severity).to_string(),
                message: SarifMessage {
                    text: finding.title.clone(),
                },
                locations: vec![SarifLocation {
                    physical_location: SarifPhysicalLocation {
                        artifact_location: SarifArtifactLocation { uri },
                        region: SarifRegion {
                            start_line,
                            end_line,
                        },
                    },
                }],
            }
        })
        .collect();

    SarifReport {
        version: "2.1.0".to_string(),
        schema: "https://json.schemastore.org/sarif-2.1.0.json".to_string(),
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifDriver {
                    name: "audit-agent".to_string(),
                    version: manifest.agent_version.clone(),
                },
            },
            artifacts,
            results,
        }],
    }
}

fn unique_artifacts(findings: &[Finding]) -> Vec<SarifArtifact> {
    let mut seen = std::collections::BTreeSet::<PathBuf>::new();
    for finding in findings {
        for location in &finding.affected_components {
            seen.insert(location.file.clone());
        }
    }

    seen.into_iter()
        .map(|path| SarifArtifact {
            location: SarifArtifactLocation {
                uri: path.to_string_lossy().to_string(),
            },
        })
        .collect()
}

fn severity_to_level(severity: &Severity) -> &'static str {
    match severity {
        Severity::Critical | Severity::High => "error",
        Severity::Medium | Severity::Low => "warning",
        Severity::Observation => "note",
    }
}
