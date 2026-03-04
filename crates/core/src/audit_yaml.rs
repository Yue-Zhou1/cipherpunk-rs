use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuditYaml {
    pub source: AuditYamlSource,
    pub scope: Option<AuditYamlScope>,
    pub engines: Option<AuditYamlEngines>,
    pub budget: Option<AuditYamlBudget>,
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
}
