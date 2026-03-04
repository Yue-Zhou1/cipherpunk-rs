use schemars::{JsonSchema, schema_for};
use serde::Serialize;
use serde_json::Value;

use crate::audit_yaml::AuditYaml;
use crate::finding::Finding;

pub fn finding_json_schema_value() -> Value {
    schema_as_value::<Finding>()
}

pub fn audit_yaml_json_schema_value() -> Value {
    schema_as_value::<AuditYaml>()
}

fn schema_as_value<T: JsonSchema + Serialize>() -> Value {
    serde_json::to_value(schema_for!(T)).expect("json schema should serialize")
}
