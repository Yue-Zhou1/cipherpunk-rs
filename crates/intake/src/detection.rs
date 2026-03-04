use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use audit_agent_core::audit_config::Confidence;
use audit_agent_core::finding::Framework;
use audit_agent_core::workspace::CargoWorkspace;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

pub struct FrameworkDetector;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DetectionResult {
    pub frameworks: Vec<DetectedFramework>,
    pub crypto_divergent_features: Vec<CryptoDivergentFeature>,
    pub entry_points: Vec<DetectedEntryPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedFramework {
    pub framework: Framework,
    pub confidence: Confidence,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CryptoDivergentFeature {
    pub feature_name: String,
    pub crate_name: String,
    pub description: String,
    pub affected_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedEntryPoint {
    pub function: String,
    pub crate_name: String,
    pub file: PathBuf,
    pub line: u32,
    pub kind: EntryPointKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryPointKind {
    Verifier,
    Prover,
    Ingest,
    GuestEntry,
    Unknown,
}

const HALO2_SIGNATURES: &[&str] = &[
    "Chip::configure",
    "Chip::synthesize",
    "ConstraintSystem",
    "halo2_proofs",
    "halo2_gadgets",
];
const SP1_SIGNATURES: &[&str] = &["sp1_zkvm::entrypoint!", "sp1_zkvm::io::read"];
const RISC0_SIGNATURES: &[&str] = &["risc0_zkvm::guest::env", "risc0_zkvm::serde"];

impl FrameworkDetector {
    pub fn detect(workspace: &CargoWorkspace) -> DetectionResult {
        let mut framework_hits: HashMap<Framework, Vec<String>> = HashMap::new();
        let mut entry_points = vec![];

        for member in &workspace.members {
            for entry in WalkDir::new(&member.path)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if !entry.file_type().is_file() {
                    continue;
                }

                if path.extension().and_then(|e| e.to_str()) == Some("circom") {
                    framework_hits
                        .entry(Framework::Circom)
                        .or_default()
                        .push(format!("{} (.circom)", path.display()));
                }

                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }

                let Ok(content) = fs::read_to_string(path) else {
                    continue;
                };
                for signature in HALO2_SIGNATURES {
                    if content.contains(signature) {
                        framework_hits
                            .entry(Framework::Halo2)
                            .or_default()
                            .push(format!("{} found in {}", signature, path.display()));
                    }
                }
                for signature in SP1_SIGNATURES {
                    if content.contains(signature) {
                        framework_hits
                            .entry(Framework::SP1)
                            .or_default()
                            .push(format!("{} found in {}", signature, path.display()));
                    }
                    if signature == &"sp1_zkvm::entrypoint!" && content.contains(signature) {
                        if let Some(line) = find_line(&content, signature) {
                            entry_points.push(DetectedEntryPoint {
                                function: "sp1_zkvm::entrypoint!".to_string(),
                                crate_name: member.name.clone(),
                                file: path.to_path_buf(),
                                line,
                                kind: EntryPointKind::GuestEntry,
                            });
                        }
                    }
                }
                for signature in RISC0_SIGNATURES {
                    if content.contains(signature) {
                        framework_hits
                            .entry(Framework::RISC0)
                            .or_default()
                            .push(format!("{} found in {}", signature, path.display()));
                    }
                }
            }
        }

        let mut frameworks = framework_hits
            .into_iter()
            .map(|(framework, evidence)| DetectedFramework {
                framework,
                confidence: Confidence::High,
                evidence,
            })
            .collect::<Vec<_>>();

        frameworks.sort_by_key(|f| format!("{:?}", f.framework));

        let crypto_divergent_features = detect_crypto_divergent_features(workspace);

        DetectionResult {
            frameworks,
            crypto_divergent_features,
            entry_points,
        }
    }
}

fn detect_crypto_divergent_features(workspace: &CargoWorkspace) -> Vec<CryptoDivergentFeature> {
    let mut results = vec![];

    for member in &workspace.members {
        let member_features = workspace
            .feature_flags
            .get(&member.name)
            .cloned()
            .unwrap_or_default();
        let feature_names: HashSet<String> = member_features.into_iter().map(|f| f.name).collect();

        if !feature_names.contains("asm") {
            continue;
        }

        let mut affected_files = vec![];
        for entry in WalkDir::new(&member.path)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !entry.file_type().is_file()
                || path.extension().and_then(|e| e.to_str()) != Some("rs")
            {
                continue;
            }
            if let Ok(content) = fs::read_to_string(path) {
                if content.contains("feature = \"asm\"") {
                    affected_files.push(path.to_path_buf());
                }
            }
        }

        results.push(CryptoDivergentFeature {
            feature_name: "asm".to_string(),
            crate_name: member.name.clone(),
            description: "enters assembly path in field arithmetic".to_string(),
            affected_files,
        });
    }

    results
}

fn find_line(content: &str, needle: &str) -> Option<u32> {
    content
        .lines()
        .position(|line| line.contains(needle))
        .map(|idx| idx as u32 + 1)
}
