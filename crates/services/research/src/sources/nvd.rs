use anyhow::Result;
use reqwest::Client;

use crate::{ResearchFinding, ResearchResult};

const DEFAULT_NVD_API_BASE: &str = "https://services.nvd.nist.gov/rest/json/cves/2.0";

pub struct NvdSource {
    client: Client,
    api_key: Option<String>,
    base_url: String,
}

impl NvdSource {
    pub fn new() -> Result<Self> {
        Self::with_base_url(DEFAULT_NVD_API_BASE.to_string())
    }

    pub fn with_base_url(base_url: String) -> Result<Self> {
        let api_key = std::env::var("NVD_API_KEY").ok();
        let client = Client::builder()
            .user_agent("cipherpunk-audit-agent/0.1")
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        Ok(Self {
            client,
            api_key,
            base_url,
        })
    }

    pub async fn query(&self, crate_name: &str, version: Option<&str>) -> Result<ResearchResult> {
        let mut keyword = crate_name.to_string();
        if let Some(version) = version {
            keyword.push(' ');
            keyword.push_str(version);
        }
        let encoded_keyword = urlencoding::encode(&keyword);
        let url = format!(
            "{}?keywordSearch={encoded_keyword}",
            self.base_url.trim_end_matches('/')
        );

        let mut request = self.client.get(&url);
        if let Some(api_key) = &self.api_key {
            request = request.header("apiKey", api_key);
        }
        let response = request.send().await?;
        if !response.status().is_success() {
            return Ok(ResearchResult {
                query: format!("CVE search for '{crate_name}'"),
                findings: vec![],
                source_url: url,
                cached: false,
                fetched_at: chrono::Utc::now(),
            });
        }

        let body: serde_json::Value = response.json().await?;
        let findings = parse_cve_findings(&body);

        Ok(ResearchResult {
            query: format!("CVE search for '{crate_name}'"),
            findings,
            source_url: url,
            cached: false,
            fetched_at: chrono::Utc::now(),
        })
    }
}

fn parse_cve_findings(body: &serde_json::Value) -> Vec<ResearchFinding> {
    let Some(vulnerabilities) = body.pointer("/vulnerabilities").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    vulnerabilities
        .iter()
        .filter_map(|entry| {
            let cve = entry.get("cve")?;
            let id = cve.get("id")?.as_str()?.to_string();
            let description = cve
                .get("descriptions")
                .and_then(|descriptions| descriptions.as_array())
                .and_then(|descriptions| {
                    descriptions.iter().find_map(|value| {
                        if value.get("lang").and_then(|lang| lang.as_str()) == Some("en") {
                            return value.get("value").and_then(|text| text.as_str());
                        }
                        None
                    })
                })
                .unwrap_or("")
                .to_string();
            let severity = cve
                .pointer("/metrics/cvssMetricV31/0/cvssData/baseScore")
                .or_else(|| cve.pointer("/metrics/cvssMetricV30/0/cvssData/baseScore"))
                .or_else(|| cve.pointer("/metrics/cvssMetricV2/0/cvssData/baseScore"))
                .map(|score| score.to_string().replace('"', ""));
            let advisory_url = cve
                .get("references")
                .and_then(|references| references.as_array())
                .and_then(|references| {
                    references
                        .iter()
                        .find_map(|reference| reference.get("url").and_then(|url| url.as_str()))
                })
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("https://nvd.nist.gov/vuln/detail/{id}"));

            Some(ResearchFinding {
                source: "NVD".to_string(),
                id: id.clone(),
                title: id,
                description,
                severity,
                affected_versions: None,
                url: advisory_url,
                fetched_at: chrono::Utc::now(),
            })
        })
        .collect()
}
