#![cfg(feature = "memory-block")]

use std::fs;
use std::path::PathBuf;

use knowledge::memory_block::schema::{
    artifact_metadata_json_schema_value, vulnerability_signature_json_schema_value,
};

fn repo_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("services/")
        .parent()
        .expect("crates/")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

#[test]
fn vulnerability_signature_schema_matches_committed_snapshot() {
    let path = repo_root().join("docs/memory-block-vulnerability-signature-schema.json");
    let expected = fs::read_to_string(path).expect("read vulnerability signature schema snapshot");
    let generated = serde_json::to_string_pretty(&vulnerability_signature_json_schema_value())
        .expect("serialize vulnerability signature schema");
    assert_eq!(generated, expected);
}

#[test]
fn artifact_metadata_schema_matches_committed_snapshot() {
    let path = repo_root().join("docs/memory-block-artifact-metadata-schema.json");
    let expected = fs::read_to_string(path).expect("read artifact metadata schema snapshot");
    let generated = serde_json::to_string_pretty(&artifact_metadata_json_schema_value())
        .expect("serialize artifact metadata schema");
    assert_eq!(generated, expected);
}
