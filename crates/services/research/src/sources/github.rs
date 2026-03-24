use anyhow::Result;
use reqwest::Client;

use crate::{ResearchFinding, ResearchResult};

const DEFAULT_GITHUB_ADVISORY_API: &str = "https://api.github.com/advisories";

pub struct GithubAdvisorySource {
    client: Client,
    token: Option<String>,
    base_url: String,
}

impl GithubAdvisorySource {
    pub fn new() -> Result<Self> {
        Self::with_base_url(DEFAULT_GITHUB_ADVISORY_API.to_string())
    }

    pub fn with_base_url(base_url: String) -> Result<Self> {
        let token = std::env::var("GITHUB_TOKEN").ok();
        let client = Client::builder()
            .user_agent("cipherpunk-audit-agent/0.1")
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        Ok(Self {
            client,
            token,
            base_url,
        })
    }

    pub async fn query(&self, crate_name: &str) -> Result<ResearchResult> {
        let encoded = urlencoding::encode(crate_name);
        let url = format!(
            "{}?ecosystem=cargo&affects={encoded}",
            self.base_url.trim_end_matches('/')
        );

        let mut request = self.client.get(&url);
        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        request = request.header("Accept", "application/vnd.github+json");

        let response = request.send().await?;
        if !response.status().is_success() {
            return Ok(ResearchResult {
                query: format!("GitHub Advisory for '{crate_name}'"),
                findings: vec![],
                source_url: url,
                cached: false,
                fetched_at: chrono::Utc::now(),
            });
        }

        let body: Vec<serde_json::Value> = response.json().await?;
        let findings = body
            .iter()
            .filter_map(|advisory| {
                Some(ResearchFinding {
                    source: "GitHub Advisory".to_string(),
                    id: advisory.get("ghsa_id")?.as_str()?.to_string(),
                    title: advisory.get("summary")?.as_str()?.to_string(),
                    description: advisory
                        .get("description")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string(),
                    severity: advisory
                        .get("severity")
                        .and_then(|value| value.as_str())
                        .map(ToString::to_string),
                    affected_versions: None,
                    url: advisory
                        .get("html_url")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string(),
                    fetched_at: chrono::Utc::now(),
                })
            })
            .collect();

        Ok(ResearchResult {
            query: format!("GitHub Advisory for '{crate_name}'"),
            findings,
            source_url: url,
            cached: false,
            fetched_at: chrono::Utc::now(),
        })
    }
}
