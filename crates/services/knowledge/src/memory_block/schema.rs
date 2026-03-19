use schemars::{JsonSchema, schema_for};
use serde::Serialize;
use serde_json::Value;

use crate::memory_block::types::{ArtifactMetadata, VulnerabilitySignature};

pub fn vulnerability_signature_json_schema_value() -> Value {
    schema_as_value::<VulnerabilitySignature>()
}

pub fn artifact_metadata_json_schema_value() -> Value {
    schema_as_value::<ArtifactMetadata>()
}

fn schema_as_value<T: JsonSchema + Serialize>() -> Value {
    serde_json::to_value(schema_for!(T)).expect("json schema should serialize")
}
