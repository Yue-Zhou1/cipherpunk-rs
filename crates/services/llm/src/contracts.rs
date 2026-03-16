use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ArchitectureOverview {
    #[serde(default)]
    pub assets: Vec<String>,
    #[serde(default)]
    pub trust_boundaries: Vec<String>,
    #[serde(default)]
    pub hotspots: Vec<String>,
    #[serde(default)]
    pub likely_domains: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainPlan {
    pub id: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChecklistPlan {
    #[serde(default)]
    pub domains: Vec<DomainPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CandidateDraft {
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub suggested_tools: Vec<String>,
    #[serde(default)]
    pub confidence: String,
}
