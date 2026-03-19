use std::hash::{Hash, Hasher};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use reqwest::blocking::Client;
use serde::Deserialize;

use crate::memory_block::config::ResolvedEmbeddingConfig;

pub trait EmbeddingProvider: Send + Sync {
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
    fn config(&self) -> &ResolvedEmbeddingConfig;
}

#[derive(Debug, Clone)]
pub struct OnnxEmbedder {
    config: ResolvedEmbeddingConfig,
}

impl OnnxEmbedder {
    pub fn from_env(config: ResolvedEmbeddingConfig) -> Result<Self> {
        if config.dimensions == 0 {
            bail!("embedding dimensions must be greater than zero");
        }
        Ok(Self { config })
    }
}

impl EmbeddingProvider for OnnxEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        if text.trim().is_empty() {
            bail!("embedding query text must not be empty");
        }
        Ok(hash_projection(text, self.config.dimensions as usize))
    }

    fn config(&self) -> &ResolvedEmbeddingConfig {
        &self.config
    }
}

#[derive(Debug, Clone)]
pub struct HttpEmbedder {
    config: ResolvedEmbeddingConfig,
    base_url: String,
    api_key: Option<String>,
    client: Client,
}

impl HttpEmbedder {
    pub fn new(
        config: ResolvedEmbeddingConfig,
        base_url: String,
        api_key: Option<String>,
    ) -> Result<Self> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build()
            .context("build HTTP client for embedding provider")?;
        Ok(Self {
            config,
            base_url,
            api_key,
            client,
        })
    }

    pub fn from_env(config: ResolvedEmbeddingConfig) -> Result<Self> {
        let provider = config.provider.trim().to_ascii_lowercase();
        let default_base_url = match provider.as_str() {
            "openai" => "https://api.openai.com",
            "ollama" => "http://localhost:11434",
            _ => "",
        };
        let base_url = std::env::var("KNOWLEDGE_EMBEDDING_BASE_URL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| default_base_url.to_string());
        if base_url.trim().is_empty() {
            bail!(
                "embedding provider `{}` requires KNOWLEDGE_EMBEDDING_BASE_URL",
                config.provider
            );
        }

        let api_key = std::env::var("KNOWLEDGE_EMBEDDING_API_KEY")
            .ok()
            .or_else(|| std::env::var("LLM_API_KEY").ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        Self::new(config, base_url, api_key)
    }
}

impl EmbeddingProvider for HttpEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        if text.trim().is_empty() {
            bail!("embedding query text must not be empty");
        }

        let url = format!("{}/v1/embeddings", self.base_url.trim_end_matches('/'));
        let payload = serde_json::json!({
            "model": self.config.model,
            "input": text,
            "encoding_format": "float",
        });

        let mut request = self.client.post(url).json(&payload);
        if let Some(api_key) = &self.api_key {
            request = request.bearer_auth(api_key);
        }

        let response = request
            .send()
            .context("send embedding request to provider")?;
        let status = response.status();
        let body = response
            .text()
            .context("read embedding provider response body")?;
        if !status.is_success() {
            return Err(anyhow!(
                "embedding request failed ({status}): {}",
                truncate_body(&body)
            ));
        }

        let parsed: OpenAiEmbeddingResponse =
            serde_json::from_str(&body).context("parse embedding provider JSON response")?;
        let vector = parsed
            .data
            .into_iter()
            .next()
            .map(|item| item.embedding)
            .context("embedding provider response missing data[0].embedding")?;
        if vector.len() != self.config.dimensions as usize {
            bail!(
                "embedding dimension mismatch: expected {}, got {}",
                self.config.dimensions,
                vector.len()
            );
        }
        Ok(vector)
    }

    fn config(&self) -> &ResolvedEmbeddingConfig {
        &self.config
    }
}

pub fn provider_from_resolved_config(
    config: ResolvedEmbeddingConfig,
) -> Result<Box<dyn EmbeddingProvider>> {
    let provider = config.provider.trim().to_ascii_lowercase();
    match provider.as_str() {
        "http" | "openai" | "ollama" => Ok(Box::new(HttpEmbedder::from_env(config)?)),
        "onnx" => Ok(Box::new(OnnxEmbedder::from_env(config)?)),
        _ => bail!("unsupported embedding provider `{}`", config.provider),
    }
}

pub fn resolved_config_and_provider_from_env()
-> Result<(ResolvedEmbeddingConfig, Box<dyn EmbeddingProvider>)> {
    let config = ResolvedEmbeddingConfig::from_env_or_default();
    let provider = provider_from_resolved_config(config.clone())?;
    Ok((config, provider))
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingResponse {
    data: Vec<OpenAiEmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingData {
    embedding: Vec<f32>,
}

fn truncate_body(body: &str) -> String {
    const LIMIT: usize = 240;
    let mut chars = body.chars();
    let truncated = chars.by_ref().take(LIMIT).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn hash_projection(text: &str, dimensions: usize) -> Vec<f32> {
    // Placeholder fallback used when an ONNX embedding runtime is not available.
    // This path is intentionally lightweight and deterministic for a single
    // runtime, but it is not a cross-language/cross-version stable vector space.
    // Production memory-block compatibility requires real ONNX embeddings.
    let mut vector = vec![0.0f32; dimensions];
    for token in text
        .split_whitespace()
        .map(|value| value.to_ascii_lowercase())
    {
        if token.is_empty() {
            continue;
        }

        // DefaultHasher output is not guaranteed stable across Rust versions.
        // Keep this only as a local fallback for tests/degraded runtime paths.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        token.hash(&mut hasher);
        let digest = hasher.finish();

        let index = (digest as usize) % dimensions;
        let sign = if (digest & 1) == 0 { 1.0f32 } else { -1.0f32 };
        let weight = 1.0f32 + (((digest >> 8) & 0xff) as f32 / 255.0f32);
        vector[index] += sign * weight;
    }
    vector
}
