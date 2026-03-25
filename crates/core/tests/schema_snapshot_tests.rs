use std::fs;
use std::path::PathBuf;

use audit_agent_core::schema::{audit_yaml_json_schema_value, finding_json_schema_value};

fn repo_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("crates/")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

#[test]
fn finding_schema_matches_committed_file() {
    let path = repo_root().join("docs/schemas/finding-schema.json");
    let expected = fs::read_to_string(path).expect("read finding schema");
    let generated = serde_json::to_string_pretty(&finding_json_schema_value())
        .expect("serialize finding schema");
    assert_eq!(generated, expected);
}

#[test]
fn audit_yaml_schema_matches_committed_file() {
    let path = repo_root().join("docs/schemas/audit-yaml-schema.json");
    let expected = fs::read_to_string(path).expect("read audit schema");
    let generated = serde_json::to_string_pretty(&audit_yaml_json_schema_value())
        .expect("serialize audit schema");
    assert_eq!(generated, expected);
}
