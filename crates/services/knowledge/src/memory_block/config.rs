use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DistanceMetric {
    Cosine,
}

impl Default for DistanceMetric {
    fn default() -> Self {
        Self::Cosine
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResolvedEmbeddingConfig {
    pub provider: String,
    pub model: String,
    pub dimensions: u32,
    pub distance_metric: DistanceMetric,
    pub l2_normalized: bool,
    pub embedding_text_version: String,
}

impl ResolvedEmbeddingConfig {
    pub fn from_env_or_default() -> Self {
        Self {
            provider: env_or("KNOWLEDGE_EMBEDDING_PROVIDER", "onnx"),
            model: env_or("KNOWLEDGE_EMBEDDING_MODEL", "all-MiniLM-L6-v2"),
            dimensions: std::env::var("KNOWLEDGE_EMBEDDING_DIMENSIONS")
                .ok()
                .and_then(|raw| raw.parse::<u32>().ok())
                .unwrap_or(384),
            distance_metric: std::env::var("KNOWLEDGE_EMBEDDING_DISTANCE")
                .ok()
                .map(|raw| raw.trim().to_ascii_lowercase())
                .as_deref()
                .map(|raw| match raw {
                    "cosine" => DistanceMetric::Cosine,
                    _ => DistanceMetric::Cosine,
                })
                .unwrap_or(DistanceMetric::Cosine),
            l2_normalized: std::env::var("KNOWLEDGE_EMBEDDING_L2_NORMALIZED")
                .ok()
                .map(|raw| matches!(raw.trim(), "1" | "true" | "TRUE" | "True"))
                .unwrap_or(true),
            embedding_text_version: env_or("KNOWLEDGE_EMBEDDING_TEXT_VERSION", "v1"),
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}
