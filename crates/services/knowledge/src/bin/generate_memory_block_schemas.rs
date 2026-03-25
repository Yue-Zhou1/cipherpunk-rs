#[cfg(feature = "memory-block")]
use std::fs;
#[cfg(feature = "memory-block")]
use std::path::PathBuf;

#[cfg(feature = "memory-block")]
use knowledge::memory_block::schema::{
    artifact_metadata_json_schema_value, vulnerability_signature_json_schema_value,
};

#[cfg(feature = "memory-block")]
fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|value| value.parent())
        .and_then(|value| value.parent())
        .expect("resolve repository root")
        .to_path_buf();
    let docs_dir = repo_root.join("docs/schemas");
    fs::create_dir_all(&docs_dir).expect("create docs dir");

    let vulnerability_path = docs_dir.join("memory-block-vulnerability-signature-schema.json");
    let artifact_path = docs_dir.join("memory-block-artifact-metadata-schema.json");

    fs::write(
        vulnerability_path,
        serde_json::to_string_pretty(&vulnerability_signature_json_schema_value())
            .expect("serialize vulnerability signature schema"),
    )
    .expect("write vulnerability signature schema");
    fs::write(
        artifact_path,
        serde_json::to_string_pretty(&artifact_metadata_json_schema_value())
            .expect("serialize artifact metadata schema"),
    )
    .expect("write artifact metadata schema");
}

#[cfg(not(feature = "memory-block"))]
fn main() {
    panic!("generate_memory_block_schemas requires the `memory-block` feature");
}
