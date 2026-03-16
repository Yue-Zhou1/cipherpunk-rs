use std::collections::HashMap;
use std::fs;
use std::path::Path;

use audit_agent_core::audit_config::{
    AuditConfig, BudgetConfig, BuildVariant, CandidateConstraint, Confidence, EngineConfig,
    EntryPoint, ExtractionMethod, LlmConfig, OptionalInputs, ParsedSpecDocument, ResolvedScope,
    ResolvedSource, SourceOrigin, SpecSection, StructuredConstraint,
};
use audit_agent_core::finding::Framework;
use engine_crypto::intake_bridge::CryptoIntakeBridge;
use evidence::{EvidenceFile, EvidenceManifest, EvidencePack, EvidenceStore};
use tempfile::tempdir;

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directories");
    }
    fs::write(path, content).expect("write file");
}

fn sample_config(root: &Path) -> AuditConfig {
    let workspace_manifest = r#"
[workspace]
members = ["app"]
"#;
    let crate_manifest = r#"
[package]
name = "app"
version = "0.1.0"
edition = "2024"
"#;
    let crate_lib = r#"
sp1_zkvm::entrypoint!(main);
pub fn verify_proof() {}
"#;

    write_file(&root.join("Cargo.toml"), workspace_manifest);
    write_file(&root.join("Cargo.lock"), "# synthetic lock\n");
    write_file(&root.join("app/Cargo.toml"), crate_manifest);
    write_file(&root.join("app/src/lib.rs"), crate_lib);

    AuditConfig {
        audit_id: "audit-20260304-a1b2c3d4".to_string(),
        source: ResolvedSource {
            local_path: root.to_path_buf(),
            origin: SourceOrigin::Local {
                original_path: root.to_path_buf(),
            },
            commit_hash: "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string(),
            content_hash: "sha256:content-hash".to_string(),
        },
        scope: ResolvedScope {
            target_crates: vec!["app".to_string()],
            excluded_crates: vec![],
            build_matrix: vec![
                BuildVariant {
                    features: vec!["default".to_string()],
                    target_triple: "x86_64-unknown-linux-gnu".to_string(),
                    label: "default".to_string(),
                },
                BuildVariant {
                    features: vec!["asm".to_string()],
                    target_triple: "x86_64-unknown-linux-gnu".to_string(),
                    label: "asm".to_string(),
                },
            ],
            detected_frameworks: vec![Framework::SP1],
        },
        engines: EngineConfig {
            crypto_zk: true,
            distributed: false,
        },
        budget: BudgetConfig {
            kani_timeout_secs: 300,
            z3_timeout_secs: 600,
            fuzz_duration_secs: 3600,
            madsim_ticks: 100_000,
            max_llm_retries: 3,
            semantic_index_timeout_secs: 120,
        },
        optional_inputs: OptionalInputs {
            spec_document: Some(ParsedSpecDocument {
                source_path: root.join("spec.md"),
                extracted_constraints: vec![CandidateConstraint {
                    structured: StructuredConstraint::Uniqueness {
                        field: "nonce".to_string(),
                        scope: "session".to_string(),
                    },
                    source_text: "nonce must be unique per session".to_string(),
                    source_section: "4.2".to_string(),
                    confidence: Confidence::High,
                    extraction_method: ExtractionMethod::PatternMatch,
                }],
                sections: vec![SpecSection {
                    title: "4.2".to_string(),
                    content: "nonce must be unique".to_string(),
                }],
                raw_text: "nonce must be unique".to_string(),
            }),
            previous_audit: None,
            custom_invariants: vec![],
            known_entry_points: vec![EntryPoint {
                crate_name: "app".to_string(),
                function: "app::verify_proof".to_string(),
            }],
        },
        llm: LlmConfig {
            api_key_present: false,
            provider: None,
            no_llm_prose: true,
        },
        output_dir: root.join("audit-output"),
    }
}

#[test]
fn builds_crypto_engine_context_from_audit_config_without_user_input() {
    let dir = tempdir().expect("tempdir");
    let config = sample_config(dir.path());

    let ctx = CryptoIntakeBridge::build_context(&config).expect("build context");

    assert_eq!(ctx.workspace.root, dir.path());
    assert_eq!(ctx.workspace.members.len(), 1);
    assert_eq!(ctx.build_matrix, config.scope.build_matrix);
    assert_eq!(ctx.spec_constraints.len(), 1);
    assert_eq!(ctx.environment_manifest.audit_id, config.audit_id);
    assert_eq!(
        ctx.environment_manifest.content_hash,
        config.source.content_hash
    );
    assert_eq!(ctx.environment_manifest.workspace_root, dir.path());
    assert_eq!(ctx.environment_manifest.cargo_lock_hash.len(), 64);
    assert!(
        ctx.entry_points
            .iter()
            .any(|ep| ep.function == "app::verify_proof")
    );
}

#[tokio::test]
async fn writes_environment_manifest_into_engine_evidence_pack_manifest_json() {
    let dir = tempdir().expect("tempdir");
    let config = sample_config(dir.path());
    let ctx = CryptoIntakeBridge::build_context(&config).expect("build context");

    let mut manifest = EvidenceManifest {
        finding_id: "F-CRYPTO-0001".to_string(),
        title: "Synthetic finding".to_string(),
        agent_version: "0.1.0".to_string(),
        source_commit: config.source.commit_hash.clone(),
        source_content_hash: Some(config.source.content_hash.clone()),
        tool: "rule-engine".to_string(),
        tool_version: "0.1.0".to_string(),
        container_image: "busybox".to_string(),
        container_digest: "sha256:deadbeef".to_string(),
        reproduction_command: "sh -lc 'echo reproduced'".to_string(),
        expected_output_description: "reproduced".to_string(),
        files: vec![],
        environment_manifest: None,
    };
    ctx.attach_environment_manifest(&mut manifest);

    let pack = EvidencePack {
        manifest,
        files: vec![EvidenceFile::text(
            "harness/src/lib.rs",
            "pub fn check() {}\n",
        )],
    };

    let store = EvidenceStore::new(dir.path().join("evidence"));
    store
        .save_pack(
            &audit_agent_core::finding::FindingId::new("F-CRYPTO-0001"),
            &pack,
        )
        .await
        .expect("save evidence");

    let saved = store
        .load_manifest(&audit_agent_core::finding::FindingId::new("F-CRYPTO-0001"))
        .await
        .expect("load manifest");

    let env = saved
        .environment_manifest
        .expect("environment manifest should be written");
    assert_eq!(env.audit_id, config.audit_id);
    assert_eq!(env.content_hash, config.source.content_hash);
    assert_eq!(env.workspace_root, config.source.local_path);
}

#[test]
fn merges_detected_and_optional_entry_points_without_duplicates() {
    let dir = tempdir().expect("tempdir");
    let mut config = sample_config(dir.path());
    config.optional_inputs.known_entry_points.push(EntryPoint {
        crate_name: "app".to_string(),
        function: "sp1_zkvm::entrypoint!".to_string(),
    });

    let ctx = CryptoIntakeBridge::build_context(&config).expect("build context");
    let mut counts = HashMap::<String, usize>::new();
    for entry in &ctx.entry_points {
        *counts.entry(entry.function.clone()).or_default() += 1;
    }
    assert_eq!(counts.get("sp1_zkvm::entrypoint!"), Some(&1));
}
