use anyhow::Result;
use reqwest::Client;

use crate::{ResearchFinding, ResearchResult};

const DEFAULT_RUSTSEC_API_BASE: &str = "https://crates.io/api/v1/crates";

pub struct RustSecSource {
    client: Client,
    base_url: String,
}

impl RustSecSource {
    pub fn new() -> Result<Self> {
        Self::with_base_url(DEFAULT_RUSTSEC_API_BASE.to_string())
    }

    pub fn with_base_url(base_url: String) -> Result<Self> {
        let client = Client::builder()
            .user_agent("cipherpunk-audit-agent/0.1")
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        Ok(Self { client, base_url })
    }

    pub async fn query(&self, crate_name: &str) -> Result<ResearchResult> {
        let encoded_crate = urlencoding::encode(crate_name);
        let url = format!("{}/{}", self.base_url.trim_end_matches('/'), encoded_crate);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Ok(ResearchResult {
                query: format!("RustSec advisory for '{crate_name}'"),
                findings: vec![],
                source_url: url,
                cached: false,
                fetched_at: chrono::Utc::now(),
            });
        }

        let body: serde_json::Value = response.json().await?;
        let findings = parse_crate_advisories(&body);

        Ok(ResearchResult {
            query: format!("RustSec advisory for '{crate_name}'"),
            findings,
            source_url: url,
            cached: false,
            fetched_at: chrono::Utc::now(),
        })
    }
}

fn parse_crate_advisories(body: &serde_json::Value) -> Vec<ResearchFinding> {
    let Some(vulnerabilities) = body.pointer("/vulnerabilities").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    vulnerabilities
        .iter()
        .map(|vuln| {
            let id = vuln
                .get("id")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown")
                .to_string();
            let title = vuln
                .get("advisory")
                .and_then(|advisory| advisory.get("title"))
                .and_then(|value| value.as_str())
                .unwrap_or("Unknown advisory")
                .to_string();
            let description = vuln
                .get("advisory")
                .and_then(|advisory| advisory.get("description"))
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            let severity = vuln
                .get("advisory")
                .and_then(|advisory| advisory.get("cvss"))
                .map(|value| {
                    value
                        .as_str()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| value.to_string())
                });
            let affected_versions = vuln
                .get("versions")
                .and_then(|versions| versions.get("patched"))
                .map(|value| {
                    if let Some(text) = value.as_str() {
                        format!("patched: {text}")
                    } else {
                        format!("patched: {value}")
                    }
                });

            ResearchFinding {
                source: "RustSec".to_string(),
                id: id.clone(),
                title,
                description,
                severity,
                affected_versions,
                url: format!("https://rustsec.org/advisories/{id}.html"),
                fetched_at: chrono::Utc::now(),
            }
        })
        .collect()
}
