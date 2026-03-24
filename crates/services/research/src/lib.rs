pub mod allowlist;
pub mod cache;
pub mod service;
pub mod sources;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A research query bounded to structured source types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResearchQuery {
    RustSecAdvisory {
        crate_name: String,
    },
    CveSearch {
        crate_name: String,
        version: Option<String>,
    },
    GithubAdvisory {
        crate_name: String,
    },
    SpecFetch {
        url: String,
    },
}

/// A single structured finding from a research source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchFinding {
    pub source: String,
    pub id: String,
    pub title: String,
    pub description: String,
    pub severity: Option<String>,
    pub affected_versions: Option<String>,
    pub url: String,
    pub fetched_at: DateTime<Utc>,
}

/// Result of a research query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchResult {
    pub query: String,
    pub findings: Vec<ResearchFinding>,
    pub source_url: String,
    pub cached: bool,
    pub fetched_at: DateTime<Utc>,
}

pub use service::ResearchService;
