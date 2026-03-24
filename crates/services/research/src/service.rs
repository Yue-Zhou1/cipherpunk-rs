use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::Result;

use crate::allowlist;
use crate::cache::ResearchCache;
use crate::sources::{github::GithubAdvisorySource, nvd::NvdSource, rustsec::RustSecSource};
use crate::{ResearchFinding, ResearchQuery, ResearchResult};

const DEFAULT_MAX_API_CALLS_PER_SESSION: usize = 10;
const REQUEST_TIMEOUT_SECS: u64 = 10;

pub struct ResearchService {
    rustsec: RustSecSource,
    github: GithubAdvisorySource,
    nvd: NvdSource,
    spec_client: reqwest::Client,
    cache: Mutex<ResearchCache>,
    api_calls: AtomicUsize,
    max_api_calls: usize,
}

impl ResearchService {
    pub fn new() -> Result<Self> {
        let spec_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()?;
        Ok(Self {
            rustsec: RustSecSource::new()?,
            github: GithubAdvisorySource::new()?,
            nvd: NvdSource::new()?,
            spec_client,
            cache: Mutex::new(ResearchCache::new()),
            api_calls: AtomicUsize::new(0),
            max_api_calls: DEFAULT_MAX_API_CALLS_PER_SESSION,
        })
    }

    pub fn with_base_urls_for_tests(
        rustsec_base_url: String,
        github_base_url: String,
        nvd_base_url: String,
        max_api_calls: usize,
        cache_ttl: Duration,
    ) -> Result<Self> {
        let spec_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()?;
        Ok(Self {
            rustsec: RustSecSource::with_base_url(rustsec_base_url)?,
            github: GithubAdvisorySource::with_base_url(github_base_url)?,
            nvd: NvdSource::with_base_url(nvd_base_url)?,
            spec_client,
            cache: Mutex::new(ResearchCache::with_ttl(cache_ttl)),
            api_calls: AtomicUsize::new(0),
            max_api_calls,
        })
    }

    pub async fn query(&self, query: &ResearchQuery) -> Result<ResearchResult> {
        let cache_key = serde_json::to_string(query)?;

        if let Ok(cache) = self.cache.lock() {
            if let Some(mut cached) = cache.get(&cache_key) {
                cached.cached = true;
                return Ok(cached);
            }
        }

        let calls_before = self.api_calls.fetch_add(1, Ordering::Relaxed);
        if calls_before >= self.max_api_calls {
            self.api_calls.fetch_sub(1, Ordering::Relaxed);
            anyhow::bail!(
                "Research rate limit exceeded: {} of {} API calls used",
                calls_before,
                self.max_api_calls
            );
        }

        let result = match query {
            ResearchQuery::RustSecAdvisory { crate_name } => self.rustsec.query(crate_name).await?,
            ResearchQuery::CveSearch {
                crate_name,
                version,
            } => self.nvd.query(crate_name, version.as_deref()).await?,
            ResearchQuery::GithubAdvisory { crate_name } => self.github.query(crate_name).await?,
            ResearchQuery::SpecFetch { url } => {
                allowlist::validate_url(url)?;
                self.fetch_spec(url).await?
            }
        };

        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(cache_key, result.clone());
            cache.prune_expired();
        }

        Ok(result)
    }

    pub fn reset_rate_limit(&self) {
        self.api_calls.store(0, Ordering::Relaxed);
    }

    async fn fetch_spec(&self, url: &str) -> Result<ResearchResult> {
        let response = self.spec_client.get(url).send().await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            anyhow::bail!("Spec fetch failed ({status}): {url}");
        }

        Ok(ResearchResult {
            query: format!("Spec fetch: {url}"),
            findings: vec![ResearchFinding {
                source: "spec-fetch".to_string(),
                id: url.to_string(),
                title: format!("Specification document from {url}"),
                description: truncate_spec_description(&body, 5_000),
                severity: None,
                affected_versions: None,
                url: url.to_string(),
                fetched_at: chrono::Utc::now(),
            }],
            source_url: url.to_string(),
            cached: false,
            fetched_at: chrono::Utc::now(),
        })
    }
}

fn truncate_spec_description(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }

    let suffix = "...[truncated]";
    let suffix_len = suffix.chars().count();
    if max_chars <= suffix_len {
        return suffix.chars().take(max_chars).collect();
    }

    let prefix_len = max_chars - suffix_len;
    let mut output = input.chars().take(prefix_len).collect::<String>();
    output.push_str(suffix);
    output
}

#[cfg(test)]
mod tests {
    use super::truncate_spec_description;

    #[test]
    fn truncate_spec_description_is_utf8_safe() {
        let input = "€".repeat(1_667);
        let truncated = truncate_spec_description(&input, 1_000);
        assert!(truncated.ends_with("...[truncated]"));
        assert_eq!(truncated.chars().count(), 1_000);
    }
}
