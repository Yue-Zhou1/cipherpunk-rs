#![cfg(feature = "memory-block")]

use knowledge::memory_block::schema::{
    artifact_metadata_json_schema_value, vulnerability_signature_json_schema_value,
};

#[test]
fn vulnerability_signature_schema_contains_required_contract_fields() {
    let schema = vulnerability_signature_json_schema_value();
    let required = schema
        .pointer("/required")
        .and_then(|value| value.as_array())
        .expect("required array");
    let required = required
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>();

    for field in [
        "id",
        "source",
        "vulnerability",
        "remediation",
        "invariants",
        "evidence",
        "extraction",
        "tags",
        "embedding_text",
    ] {
        assert!(
            required.contains(&field),
            "schema missing required field `{field}`"
        );
    }
}

#[test]
fn artifact_metadata_schema_exposes_embedding_and_signatures() {
    let schema = artifact_metadata_json_schema_value();
    let required = schema
        .pointer("/required")
        .and_then(|value| value.as_array())
        .expect("required array");
    let required = required
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>();

    for field in ["schema_version", "generated_at", "embedding", "signatures"] {
        assert!(
            required.contains(&field),
            "schema missing required field `{field}`"
        );
    }
}
