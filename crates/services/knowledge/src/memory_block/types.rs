use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::memory_block::config::ResolvedEmbeddingConfig;
use crate::models::AdjudicatedCase;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SignatureSource {
    pub report: String,
    pub pdf_path: String,
    pub page_range: [u32; 2],
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VulnerabilityDetails {
    pub title: String,
    pub severity: String,
    pub category: String,
    pub description: String,
    pub vulnerable_pattern: String,
    pub root_cause: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Remediation {
    pub description: String,
    pub code_pattern: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Invariants {
    pub natural_language: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kani_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Evidence {
    pub excerpt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExtractionMetadata {
    pub confidence: String,
    pub review_status: String,
    pub embedding_text_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VulnerabilitySignature {
    pub id: String,
    pub source: SignatureSource,
    pub vulnerability: VulnerabilityDetails,
    pub remediation: Remediation,
    pub invariants: Invariants,
    pub evidence: Evidence,
    pub extraction: ExtractionMetadata,
    pub tags: Vec<String>,
    pub embedding_text: String,
}

impl VulnerabilitySignature {
    pub fn to_adjudicated_case(&self) -> AdjudicatedCase {
        AdjudicatedCase {
            id: self.id.clone(),
            title: self.vulnerability.title.clone(),
            summary: self.vulnerability.description.clone(),
            tags: self.tags.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactMetadata {
    pub schema_version: u32,
    pub generated_at: String,
    pub embedding: ResolvedEmbeddingConfig,
    pub signatures: Vec<VulnerabilitySignature>,
}
