use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuditYaml {
    pub source: AuditYamlSource,
    pub scope: Option<AuditYamlScope>,
    pub engines: Option<AuditYamlEngines>,
    pub budget: Option<AuditYamlBudget>,
    pub llm: Option<AuditYamlLlm>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuditYamlSource {
    pub url: Option<String>,
    pub local_path: Option<String>,
    pub commit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuditYamlScope {
    pub target_crates: Option<Vec<String>>,
    pub exclude_crates: Option<Vec<String>>,
    pub features: Option<Vec<Vec<String>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuditYamlEngines {
    pub crypto_zk: Option<bool>,
    pub distributed: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuditYamlBudget {
    pub kani_timeout_secs: Option<u64>,
    pub z3_timeout_secs: Option<u64>,
    pub fuzz_duration_secs: Option<u64>,
    pub madsim_ticks: Option<u64>,
    pub max_llm_retries: Option<u8>,
    pub semantic_index_timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuditYamlLlm {
    pub no_llm_prose: Option<bool>,
    pub roles: Option<HashMap<String, AuditYamlRoleConfig>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuditYamlRoleConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
    #[serde(rename = "temperature", alias = "temperature_millis")]
    pub temperature: Option<u16>,
    pub max_tokens: Option<u32>,
}
