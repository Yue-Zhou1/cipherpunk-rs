use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use audit_agent_core::audit_config::{AuditConfig, BuildVariant, CandidateConstraint, EntryPoint};
use audit_agent_core::workspace::CargoWorkspace;
use evidence::{EnvironmentManifest as EvidenceEnvironmentManifest, EvidenceManifest};
use intake::detection::{DetectedEntryPoint, EntryPointKind, FrameworkDetector};
use intake::workspace::WorkspaceAnalyzer;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub struct CryptoIntakeBridge;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CryptoEngineContext {
    pub workspace: CargoWorkspace,
    pub build_matrix: Vec<BuildVariant>,
    pub entry_points: Vec<DetectedEntryPoint>,
    pub spec_constraints: Vec<CandidateConstraint>,
    pub environment_manifest: EnvironmentManifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentManifest {
    pub rust_toolchain: String,
    pub cargo_lock_hash: String,
    pub workspace_root: PathBuf,
    pub audit_id: String,
    pub content_hash: String,
}

impl CryptoIntakeBridge {
    pub fn build_context(config: &AuditConfig) -> Result<CryptoEngineContext> {
        let workspace = WorkspaceAnalyzer::analyze(&config.source.local_path)?;
        let detection = FrameworkDetector::detect(&workspace);
        let entry_points = merge_entry_points(
            &workspace,
            detection.entry_points,
            &config.optional_inputs.known_entry_points,
        );
        let spec_constraints = config
            .optional_inputs
            .spec_document
            .as_ref()
            .map(|doc| doc.extracted_constraints.clone())
            .unwrap_or_default();

        let environment_manifest = build_environment_manifest(config, &workspace)?;

        Ok(CryptoEngineContext {
            workspace,
            build_matrix: config.scope.build_matrix.clone(),
            entry_points,
            spec_constraints,
            environment_manifest,
        })
    }
}

impl CryptoEngineContext {
    pub fn attach_environment_manifest(&self, manifest: &mut EvidenceManifest) {
        manifest.environment_manifest = Some(EvidenceEnvironmentManifest {
            rust_toolchain: self.environment_manifest.rust_toolchain.clone(),
            cargo_lock_hash: self.environment_manifest.cargo_lock_hash.clone(),
            workspace_root: self.environment_manifest.workspace_root.clone(),
            audit_id: self.environment_manifest.audit_id.clone(),
            content_hash: self.environment_manifest.content_hash.clone(),
        });
    }
}

fn merge_entry_points(
    workspace: &CargoWorkspace,
    detected: Vec<DetectedEntryPoint>,
    optional: &[EntryPoint],
) -> Vec<DetectedEntryPoint> {
    let mut merged = Vec::new();
    let mut seen = HashSet::<(String, String)>::new();

    for entry in detected {
        let key = (entry.crate_name.clone(), entry.function.clone());
        if seen.insert(key) {
            merged.push(entry);
        }
    }

    for entry in optional {
        let key = (entry.crate_name.clone(), entry.function.clone());
        if !seen.insert(key) {
            continue;
        }

        merged.push(DetectedEntryPoint {
            function: entry.function.clone(),
            crate_name: entry.crate_name.clone(),
            file: workspace.root.join("optional-input/entries.yaml"),
            line: 0,
            kind: infer_entry_point_kind(&entry.function),
        });
    }

    merged
}

fn infer_entry_point_kind(function: &str) -> EntryPointKind {
    let lower = function.to_ascii_lowercase();
    if lower.contains("verify") {
        EntryPointKind::Verifier
    } else if lower.contains("prove") || lower.contains("prover") {
        EntryPointKind::Prover
    } else if lower.contains("ingest") {
        EntryPointKind::Ingest
    } else {
        EntryPointKind::Unknown
    }
}

fn build_environment_manifest(config: &AuditConfig, workspace: &CargoWorkspace) -> Result<EnvironmentManifest> {
    let cargo_lock_hash = hash_cargo_lock(&workspace.root)?;
    let rust_toolchain = detect_rust_toolchain();

    Ok(EnvironmentManifest {
        rust_toolchain,
        cargo_lock_hash,
        workspace_root: workspace.root.clone(),
        audit_id: config.audit_id.clone(),
        content_hash: config.source.content_hash.clone(),
    })
}

fn hash_cargo_lock(workspace_root: &Path) -> Result<String> {
    let lock_path = workspace_root.join("Cargo.lock");
    let bytes = std::fs::read(&lock_path)
        .with_context(|| format!("failed to read {}", lock_path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hex::encode(hasher.finalize()))
}

fn detect_rust_toolchain() -> String {
    Command::new("rustc")
        .arg("-V")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "rustc unknown".to_string())
}
