use std::fs;
use std::path::PathBuf;

use audit_agent_core::schema::{audit_yaml_json_schema_value, finding_json_schema_value};

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf();
    let docs_dir = repo_root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");

    let finding_path = docs_dir.join("finding-schema.json");
    let audit_path = docs_dir.join("audit-yaml-schema.json");

    fs::write(
        finding_path,
        serde_json::to_string_pretty(&finding_json_schema_value()).expect("serialize"),
    )
    .expect("write finding schema");
    fs::write(
        audit_path,
        serde_json::to_string_pretty(&audit_yaml_json_schema_value()).expect("serialize"),
    )
    .expect("write audit schema");
}
