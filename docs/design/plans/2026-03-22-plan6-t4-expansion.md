# Plan 6 — T4: Expansion

> **For Agent:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add cross-audit learning through formalized memory tiers, and add bounded web-research capabilities for fetching security advisories, CVE data, and protocol specifications during audits.

**Architecture:** Item A formalizes the existing `KnowledgeBase` + `MemoryBlock` into explicit working memory (session-scoped) and long-term memory (persistent, cross-session). Item B creates a new `research` service crate with whitelisted data sources and integrates it into the tool action system.

**Tech Stack:** Rust workspace crates, serde, reqwest, tokio, SQLite session store.

**Depends on:** Plan 1A (engine outcomes — stored in memory), Plan 1B (provenance — context for memory), Plan 4A (observability — research events recorded), Plan 4B (audit plan — research informs planning).

---

## Planning Conventions

- Recommended execution context: a dedicated git worktree.
- Memory is informational context, not authoritative. It never overrides deterministic engine results.
- Web research is bounded by an allowlist. No open-ended browsing.
- Research findings are informational — they do not generate `Finding` records directly.
- Rate limits and caching prevent abuse and redundant network calls.
- Deterministic engines and ProjectIR remain the source of truth for audit analysis; AI layers may assist planning, explanation, and recovery, but do not redefine deterministic results.
- Every task must land with tests.

## Release Gates

This plan is complete only when all of the following are true:

- `WorkingMemory` maintains session-scoped context and produces bounded summaries.
- `LongTermMemory` persists compressed audit outcomes and retrieves relevant past results.
- Working memory context is available only to AI-assist roles such as planning, advising, reporting, and tool recommendation.
- Deterministic engines remain context-independent and reproducible from source + config alone.
- `ResearchService` can query RustSec, NVD/CVE, and GitHub Advisory sources.
- URL allowlist enforces that only whitelisted domains are fetched.
- Research results are integrated into the toolbench context.
- Rate limits (max 10 API calls per session) and caching (24h TTL) are enforced.
- `cargo test -p knowledge`, `cargo test -p research` pass.

## Explicit Non-Goals

- Vector embedding storage (the `memory_block` feature flag exists but full vector search is deferred).
- Persistent knowledge graph (Graphiti/Neo4j — deferred per prioritization agreement).
- Autonomous research — the system only fetches what is explicitly requested or triggered by dependency analysis.
- Research-driven finding generation — research informs, never creates findings.

---

## Item A: Memory Tiers and Session Summarization

### Context

`KnowledgeBase` at `crates/services/knowledge/src/lib.rs` has an `AdjudicatedCase` store (true/false positive tracking), `ToolSequence` and `ReproPattern` stores, and an optional `MemoryBlock` (behind feature flag) for semantic search. But there is no clear separation between current-audit context (working memory) and cross-audit learning (long-term memory). Large audits can also generate so much context that LLM prompts overflow. `WorkingMemory` must remain an AI-assist input only; it is not an input to deterministic engine detection or analysis logic.

### Tasks

#### Task 1: Create WorkingMemory module

**File:** `crates/services/knowledge/src/working_memory.rs` (new)

```rust
use std::collections::HashMap;
use audit_agent_core::finding::{Finding, Severity};

/// Session-scoped memory that tracks the current audit's evolving state.
/// Designed to be created per-session and discarded when the session ends.
pub struct WorkingMemory {
    findings: Vec<FindingSummary>,
    engine_outcomes: Vec<EngineSummary>,
    tool_results: Vec<ToolResultSummary>,
    adviser_notes: Vec<String>,
}

#[derive(Debug, Clone)]
struct FindingSummary {
    id: String,
    title: String,
    severity: Severity,
    category: String,
    file_path: Option<String>,
}

#[derive(Debug, Clone)]
struct EngineSummary {
    engine: String,
    status: String,
    findings_count: usize,
}

#[derive(Debug, Clone)]
struct ToolResultSummary {
    tool: String,
    target: String,
    status: String,
}

const MAX_SUMMARY_CHARS: usize = 2_000;

impl WorkingMemory {
    pub fn new() -> Self {
        Self {
            findings: Vec::new(),
            engine_outcomes: Vec::new(),
            tool_results: Vec::new(),
            adviser_notes: Vec::new(),
        }
    }

    /// Record a finding from an engine.
    pub fn record_finding(&mut self, finding: &Finding) {
        self.findings.push(FindingSummary {
            id: finding.id.to_string(),
            title: finding.title.clone(),
            severity: finding.severity.clone(),
            category: format!("{:?}", finding.category),
            file_path: finding.affected_components.first()
                .map(|loc| loc.file.to_string_lossy().to_string()),
        });
    }

    /// Record an engine outcome.
    pub fn record_engine_outcome(&mut self, engine: &str, status: &str, findings_count: usize) {
        self.engine_outcomes.push(EngineSummary {
            engine: engine.to_string(),
            status: status.to_string(),
            findings_count,
        });
    }

    /// Record a tool action result.
    pub fn record_tool_result(&mut self, tool: &str, target: &str, status: &str) {
        self.tool_results.push(ToolResultSummary {
            tool: tool.to_string(),
            target: target.to_string(),
            status: status.to_string(),
        });
    }

    /// Record an adviser note.
    pub fn record_adviser_note(&mut self, note: &str) {
        self.adviser_notes.push(note.to_string());
    }

    /// Generate a bounded summary of the current audit state.
    /// Suitable for inclusion in LLM prompts without exceeding context limits.
    pub fn summarize(&self) -> String {
        let mut parts = Vec::new();

        // Severity counts
        let mut counts: HashMap<String, usize> = HashMap::new();
        for f in &self.findings {
            *counts.entry(format!("{:?}", f.severity)).or_default() += 1;
        }
        if !counts.is_empty() {
            let count_str = counts.iter()
                .map(|(k, v)| format!("{k}: {v}"))
                .collect::<Vec<_>>()
                .join(", ");
            parts.push(format!("Findings: {count_str}"));
        }

        // Top findings by severity
        let mut sorted = self.findings.clone();
        sorted.sort_by_key(|f| match f.severity {
            Severity::Critical => 0,
            Severity::High => 1,
            Severity::Medium => 2,
            Severity::Low => 3,
            Severity::Observation => 4,
        });
        let top: Vec<String> = sorted.iter().take(5)
            .map(|f| format!("- [{:?}] {}: {}", f.severity, f.id, f.title))
            .collect();
        if !top.is_empty() {
            parts.push(format!("Top findings:\n{}", top.join("\n")));
        }

        // Engine outcomes
        let engines: Vec<String> = self.engine_outcomes.iter()
            .map(|e| format!("- {}: {} ({} findings)", e.engine, e.status, e.findings_count))
            .collect();
        if !engines.is_empty() {
            parts.push(format!("Engines:\n{}", engines.join("\n")));
        }

        // Adviser notes
        if !self.adviser_notes.is_empty() {
            let notes = self.adviser_notes.iter().take(3)
                .map(|n| format!("- {n}"))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("Adviser notes:\n{notes}"));
        }

        let summary = parts.join("\n\n");

        // Truncate to budget
        if summary.len() > MAX_SUMMARY_CHARS {
            format!("{}...[truncated]", &summary[..MAX_SUMMARY_CHARS])
        } else {
            summary
        }
    }

    /// Generate context for AI-assist roles such as adviser, planning, or reporting.
    pub fn context_for_role(&self, role_name: &str) -> String {
        match role_name {
            "adviser" => {
                let relevant: Vec<String> = self.findings.iter()
                    .filter(|f| matches!(f.category.as_str(), "CryptoMisuse" | "Replay" | "Race"))
                    .take(3)
                    .map(|f| format!("- [{:?}] {}", f.severity, f.title))
                    .collect();
                if relevant.is_empty() {
                    "No additional working-memory context available.".to_string()
                } else {
                    format!("Recent cross-cutting findings:\n{}", relevant.join("\n"))
                }
            }
            "reporting" | "planning" => self.summarize(),
            _ => "No additional working-memory context available.".to_string(),
        }
    }
}
```

**Tests:**
- `summarize()` with 100 findings → output < 2000 chars.
- `context_for_role("adviser")` with crypto findings → includes relevant findings.
- `context_for_role("unknown")` → returns "No additional working-memory context" message.

---

#### Task 2: Create LongTermMemory module

**File:** `crates/services/knowledge/src/long_term.rs` (new)

```rust
use std::path::Path;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::store::KnowledgeStore;

/// Compressed summary of a completed audit, suitable for cross-session recall.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditMemoryEntry {
    pub audit_id: String,
    pub timestamp: String,
    pub source_description: String,
    pub findings_by_severity: FindingSeverityCounts,
    pub engines_used: Vec<String>,
    pub key_findings: Vec<String>,  // Top 5 finding titles
    pub tags: Vec<String>,          // Frameworks, categories detected
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FindingSeverityCounts {
    pub critical: u32,
    pub high: u32,
    pub medium: u32,
    pub low: u32,
    pub observation: u32,
}

/// Persistent memory that stores compressed audit outcomes for cross-session learning.
pub struct LongTermMemory {
    entries: Vec<AuditMemoryEntry>,
    store_path: Option<std::path::PathBuf>,
}

impl LongTermMemory {
    pub fn new() -> Self {
        Self { entries: Vec::new(), store_path: None }
    }

    pub fn load_from_path(path: &Path) -> Result<Self> {
        let memory_path = path.join("long_term_memory.json");
        if memory_path.exists() {
            let content = std::fs::read_to_string(&memory_path)?;
            let entries: Vec<AuditMemoryEntry> = serde_json::from_str(&content)?;
            Ok(Self {
                entries,
                store_path: Some(memory_path),
            })
        } else {
            Ok(Self {
                entries: Vec::new(),
                store_path: Some(memory_path),
            })
        }
    }

    /// Record a completed audit's outcome for future recall.
    pub fn record_audit_outcome(&mut self, entry: AuditMemoryEntry) {
        // Deduplicate by audit_id
        self.entries.retain(|e| e.audit_id != entry.audit_id);
        self.entries.push(entry);

        // Cap at 100 entries, dropping oldest
        if self.entries.len() > 100 {
            self.entries.drain(0..(self.entries.len() - 100));
        }
    }

    /// Recall audits relevant to the given context tags.
    pub fn recall_similar(&self, context_tags: &[String], limit: usize) -> Vec<&AuditMemoryEntry> {
        let context_set: std::collections::BTreeSet<String> = context_tags.iter()
            .map(|t| t.to_lowercase())
            .collect();

        let mut scored: Vec<(usize, &AuditMemoryEntry)> = self.entries.iter()
            .map(|entry| {
                let overlap = entry.tags.iter()
                    .filter(|t| context_set.contains(&t.to_lowercase()))
                    .count();
                (overlap, entry)
            })
            .filter(|(score, _)| *score > 0)
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().take(limit).map(|(_, e)| e).collect()
    }

    /// Persist to disk.
    pub fn persist(&self) -> Result<()> {
        if let Some(path) = &self.store_path {
            let content = serde_json::to_string_pretty(&self.entries)?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, content)?;
        }
        Ok(())
    }

    pub fn entries(&self) -> &[AuditMemoryEntry] {
        &self.entries
    }
}
```

**Tests:**
- Record + recall roundtrip: record audit with tags ["halo2", "crypto"], recall with ["halo2"] → found.
- Recall with no matching tags → empty.
- Capacity: record 110 entries → oldest 10 dropped.
- Persist + reload from path → entries preserved.

---

#### Task 3: Integrate WorkingMemory into orchestrator

**File:** `crates/apps/orchestrator/src/lib.rs`

In `execute_dag()`, maintain a `WorkingMemory` instance:

```rust
let mut working_memory = WorkingMemory::new();

for engine in &self.engines {
    // ... existing logic ...
    match engine.analyze(&ctx).await {
        Ok(engine_findings) => {
            for finding in &engine_findings {
                working_memory.record_finding(finding);
            }
            working_memory.record_engine_outcome(
                &engine_name, "completed", engine_findings.len()
            );
            findings.extend(engine_findings);
        }
        Err(err) => {
            working_memory.record_engine_outcome(&engine_name, "failed", 0);
            // ... existing error handling ...
        }
    }
}
```

Do **not** add working-memory summaries to `AuditContext` for deterministic engines. Instead, expose it only to AI-assist services that already depend on LLM context, for example:
- adviser prompts
- report-generation helpers
- planning/tool-recommendation helpers

If a helper is needed, add a narrow accessor such as:

```rust
pub fn llm_assist_context(&self, role_name: &str) -> Option<String> {
    Some(self.working_memory.context_for_role(role_name))
}
```

The orchestrator owns recording and summarization. Deterministic engines still run from source + config alone.

---

#### Task 4: Integrate LongTermMemory into orchestrator

**File:** `crates/apps/orchestrator/src/lib.rs`

After `produce_outputs()`, build and persist a long-term memory entry:

```rust
pub async fn run(&self, config: &AuditConfig) -> Result<AuditOutputs> {
    let dag = self.build_dag(config);
    let (findings, outcomes) = self.execute_dag(&dag, config).await?;
    let outputs = self.produce_outputs(&findings, &outcomes, config).await?;

    // Record to long-term memory
    if let Some(long_term) = &self.long_term_memory {
        let entry = AuditMemoryEntry {
            audit_id: config.audit_id.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            source_description: format!("{:?}", config.source.origin),
            findings_by_severity: FindingSeverityCounts {
                critical: outputs.manifest.finding_counts.critical,
                high: outputs.manifest.finding_counts.high,
                medium: outputs.manifest.finding_counts.medium,
                low: outputs.manifest.finding_counts.low,
                observation: outputs.manifest.finding_counts.observation,
            },
            engines_used: outcomes.iter().map(|o| o.engine.clone()).collect(),
            key_findings: findings.iter().take(5).map(|f| f.title.clone()).collect(),
            tags: config.scope.detected_frameworks.iter()
                .map(|f| format!("{:?}", f).to_lowercase())
                .collect(),
        };
        long_term.lock().await.record_audit_outcome(entry);
        let _ = long_term.lock().await.persist();
    }

    Ok(outputs)
}
```

Add to `AuditOrchestrator`:

```rust
pub struct AuditOrchestrator {
    // ... existing fields ...
    pub long_term_memory: Option<Arc<tokio::sync::Mutex<LongTermMemory>>>,
}

impl AuditOrchestrator {
    pub fn with_long_term_memory(mut self, memory: Arc<tokio::sync::Mutex<LongTermMemory>>) -> Self {
        self.long_term_memory = Some(memory);
        self
    }
}
```

---

#### Task 5: Export and wire up knowledge modules

**File:** `crates/services/knowledge/src/lib.rs`

Add:
```rust
pub mod working_memory;
pub mod long_term;

pub use working_memory::WorkingMemory;
pub use long_term::{LongTermMemory, AuditMemoryEntry};
```

---

## Item B: Controlled Web-Research Assistance

### Context

Auditors need external context — security advisories for dependencies, CVE data, protocol specifications — but the system operates purely on local code analysis. AGI integrates multiple search engines. This repo needs a much more constrained version: structured queries against known, trusted security data sources with strict allowlisting.

### Tasks

#### Task 6: Create the research crate

**File:** `crates/services/research/Cargo.toml`

```toml
[package]
name = "research"
version = "0.1.0"
edition = "2024"

[dependencies]
anyhow = "1"
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["time"] }
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
```

**File:** `Cargo.toml` (workspace)

Add `"crates/services/research"` to workspace members.

---

#### Task 7: Define research types

**File:** `crates/services/research/src/lib.rs`

```rust
pub mod allowlist;
pub mod cache;
pub mod sources;

use serde::{Deserialize, Serialize};

/// A research query — bounded to known, structured query types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResearchQuery {
    /// Search RustSec advisory database for a crate.
    RustSecAdvisory { crate_name: String },
    /// Search NVD for CVEs affecting a crate/package.
    CveSearch { crate_name: String, version: Option<String> },
    /// Search GitHub Advisory Database.
    GithubAdvisory { crate_name: String },
    /// Fetch a spec document from an allowlisted URL.
    SpecFetch { url: String },
}

/// A single finding from a research source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchFinding {
    pub source: String,
    pub id: String,
    pub title: String,
    pub description: String,
    pub severity: Option<String>,
    pub affected_versions: Option<String>,
    pub url: String,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

/// Result of a research query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchResult {
    pub query: String,
    pub findings: Vec<ResearchFinding>,
    pub source_url: String,
    pub cached: bool,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}
```

---

#### Task 8: Implement URL allowlist

**File:** `crates/services/research/src/allowlist.rs`

```rust
/// Allowlisted URL prefixes for SpecFetch queries.
const ALLOWED_PREFIXES: &[&str] = &[
    "https://rustsec.org/",
    "https://crates.io/api/",
    "https://docs.rs/",
    "https://eips.ethereum.org/",
    "https://raw.githubusercontent.com/RustSec/advisory-db/",
    "https://github.com/advisories/",
    "https://nvd.nist.gov/vuln/detail/",
    "https://services.nvd.nist.gov/rest/json/",
];

pub fn is_allowed_url(url: &str) -> bool {
    ALLOWED_PREFIXES.iter().any(|prefix| url.starts_with(prefix))
}

pub fn validate_url(url: &str) -> anyhow::Result<()> {
    if !is_allowed_url(url) {
        anyhow::bail!(
            "URL '{}' is not on the research allowlist. \
             Allowed prefixes: {:?}",
            url,
            ALLOWED_PREFIXES
        );
    }
    Ok(())
}
```

**Tests:** Allowed URLs pass. Random URLs fail. Tricky URLs (e.g., `https://rustsec.org.evil.com/`) fail because they don't match the prefix exactly.

---

#### Task 9: Implement response cache

**File:** `crates/services/research/src/cache.rs`

```rust
use std::collections::HashMap;
use std::time::{Duration, Instant};
use crate::ResearchResult;

const DEFAULT_TTL: Duration = Duration::from_secs(24 * 60 * 60); // 24 hours

pub struct ResearchCache {
    entries: HashMap<String, CacheEntry>,
    ttl: Duration,
}

struct CacheEntry {
    result: ResearchResult,
    inserted_at: Instant,
}

impl ResearchCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            ttl: DEFAULT_TTL,
        }
    }

    pub fn get(&self, key: &str) -> Option<&ResearchResult> {
        self.entries.get(key).and_then(|entry| {
            if entry.inserted_at.elapsed() < self.ttl {
                Some(&entry.result)
            } else {
                None
            }
        })
    }

    pub fn insert(&mut self, key: String, result: ResearchResult) {
        self.entries.insert(key, CacheEntry {
            result,
            inserted_at: Instant::now(),
        });
    }

    pub fn prune_expired(&mut self) {
        self.entries.retain(|_, entry| entry.inserted_at.elapsed() < self.ttl);
    }
}
```

---

#### Task 10: Implement RustSec source

**File:** `crates/services/research/src/sources/rustsec.rs`

```rust
use anyhow::Result;
use reqwest::Client;
use crate::{ResearchFinding, ResearchResult};

const RUSTSEC_API_BASE: &str = "https://crates.io/api/v1/crates";

pub struct RustSecSource {
    client: Client,
}

impl RustSecSource {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .user_agent("cipherpunk-audit-agent/0.1")
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        Ok(Self { client })
    }

    /// Check crates.io for known advisories affecting a crate.
    /// Uses the crates.io API which includes vulnerability information.
    pub async fn query(&self, crate_name: &str) -> Result<ResearchResult> {
        let url = format!("{RUSTSEC_API_BASE}/{crate_name}");
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

        // Parse advisories from the response if present
        let findings = parse_crate_advisories(&body, crate_name);

        Ok(ResearchResult {
            query: format!("RustSec advisory for '{crate_name}'"),
            findings,
            source_url: url,
            cached: false,
            fetched_at: chrono::Utc::now(),
        })
    }
}

fn parse_crate_advisories(body: &serde_json::Value, crate_name: &str) -> Vec<ResearchFinding> {
    // Parse the crates.io response for advisory/vulnerability information.
    // The exact structure depends on the crates.io API version.
    // Extract: advisory ID, title, description, affected versions, severity.
    let mut findings = Vec::new();

    if let Some(vulnerabilities) = body.pointer("/vulnerabilities") {
        if let Some(arr) = vulnerabilities.as_array() {
            for vuln in arr {
                findings.push(ResearchFinding {
                    source: "RustSec".to_string(),
                    id: vuln.get("id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                    title: vuln.get("advisory").and_then(|a| a.get("title")).and_then(|v| v.as_str())
                        .unwrap_or("Unknown advisory").to_string(),
                    description: vuln.get("advisory").and_then(|a| a.get("description")).and_then(|v| v.as_str())
                        .unwrap_or("").to_string(),
                    severity: vuln.get("advisory").and_then(|a| a.get("cvss")).and_then(|v| v.as_str())
                        .map(String::from),
                    affected_versions: vuln.get("versions").and_then(|v| v.get("patched")).and_then(|v| v.as_str())
                        .map(|s| format!("patched: {s}")),
                    url: format!("https://rustsec.org/advisories/{}.html",
                        vuln.get("id").and_then(|v| v.as_str()).unwrap_or("unknown")),
                    fetched_at: chrono::Utc::now(),
                });
            }
        }
    }

    findings
}
```

---

#### Task 11: Implement GitHub Advisory source

**File:** `crates/services/research/src/sources/github.rs`

```rust
use anyhow::Result;
use reqwest::Client;
use crate::{ResearchFinding, ResearchResult};

const GITHUB_ADVISORY_API: &str = "https://api.github.com/advisories";

pub struct GithubAdvisorySource {
    client: Client,
    token: Option<String>,  // Optional GitHub token for higher rate limits
}

impl GithubAdvisorySource {
    pub fn new() -> Result<Self> {
        let token = std::env::var("GITHUB_TOKEN").ok();
        let client = Client::builder()
            .user_agent("cipherpunk-audit-agent/0.1")
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        Ok(Self { client, token })
    }

    pub async fn query(&self, crate_name: &str) -> Result<ResearchResult> {
        let url = format!("{GITHUB_ADVISORY_API}?ecosystem=cargo&affects={crate_name}");

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
        let findings = body.iter().filter_map(|advisory| {
            Some(ResearchFinding {
                source: "GitHub Advisory".to_string(),
                id: advisory.get("ghsa_id")?.as_str()?.to_string(),
                title: advisory.get("summary")?.as_str()?.to_string(),
                description: advisory.get("description").and_then(|v| v.as_str())
                    .unwrap_or("").to_string(),
                severity: advisory.get("severity").and_then(|v| v.as_str()).map(String::from),
                affected_versions: None,
                url: advisory.get("html_url").and_then(|v| v.as_str())
                    .unwrap_or("").to_string(),
                fetched_at: chrono::Utc::now(),
            })
        }).collect();

        Ok(ResearchResult {
            query: format!("GitHub Advisory for '{crate_name}'"),
            findings,
            source_url: url,
            cached: false,
            fetched_at: chrono::Utc::now(),
        })
    }
}
```

---

#### Task 12: Build ResearchService with rate limiting

**File:** `crates/services/research/src/service.rs`

```rust
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use anyhow::Result;
use crate::allowlist;
use crate::cache::ResearchCache;
use crate::sources::{rustsec::RustSecSource, github::GithubAdvisorySource};
use crate::{ResearchQuery, ResearchResult};

const MAX_API_CALLS_PER_SESSION: usize = 10;
const REQUEST_TIMEOUT_SECS: u64 = 10;

pub struct ResearchService {
    rustsec: RustSecSource,
    github: GithubAdvisorySource,
    cache: Mutex<ResearchCache>,
    api_calls: AtomicUsize,
}

impl ResearchService {
    pub fn new() -> Result<Self> {
        Ok(Self {
            rustsec: RustSecSource::new()?,
            github: GithubAdvisorySource::new()?,
            cache: Mutex::new(ResearchCache::new()),
            api_calls: AtomicUsize::new(0),
        })
    }

    pub async fn query(&self, query: &ResearchQuery) -> Result<ResearchResult> {
        let cache_key = format!("{:?}", query);

        // Check cache first
        {
            let cache = self.cache.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
            if let Some(cached) = cache.get(&cache_key) {
                let mut result = cached.clone();
                result.cached = true;
                return Ok(result);
            }
        }

        // Check rate limit
        let calls = self.api_calls.fetch_add(1, Ordering::Relaxed);
        if calls >= MAX_API_CALLS_PER_SESSION {
            anyhow::bail!(
                "Research rate limit exceeded: {} of {} API calls used",
                calls, MAX_API_CALLS_PER_SESSION
            );
        }

        // Execute query
        let result = match query {
            ResearchQuery::RustSecAdvisory { crate_name } => {
                self.rustsec.query(crate_name).await?
            }
            ResearchQuery::GithubAdvisory { crate_name } => {
                self.github.query(crate_name).await?
            }
            ResearchQuery::CveSearch { crate_name, version: _ } => {
                // Use GitHub Advisory as proxy for CVE search
                self.github.query(crate_name).await?
            }
            ResearchQuery::SpecFetch { url } => {
                allowlist::validate_url(url)?;
                self.fetch_spec(url).await?
            }
        };

        // Cache result
        {
            let mut cache = self.cache.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
            cache.insert(cache_key, result.clone());
        }

        Ok(result)
    }

    async fn fetch_spec(&self, url: &str) -> Result<ResearchResult> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()?;

        let response = client.get(url).send().await?;
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
                description: if body.len() > 5000 {
                    format!("{}...[truncated]", &body[..5000])
                } else {
                    body
                },
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

    /// Reset rate limit counter (call at session start).
    pub fn reset_rate_limit(&self) {
        self.api_calls.store(0, Ordering::Relaxed);
    }
}
```

---

#### Task 13: Integrate research into toolbench context

**File:** `crates/services/session-manager/src/state.rs`

In `load_toolbench_context()`, after computing tool recommendations, check workspace dependencies for known advisories:

```rust
// Check top-level dependencies for advisories
let mut advisories = Vec::new();
if let Some(research) = &self.research_service {
    let workspace = self.load_workspace(session_id)?;
    for member in &workspace.members {
        for dep in &member.dependencies {
            match research.query(&ResearchQuery::RustSecAdvisory {
                crate_name: dep.name.clone(),
            }).await {
                Ok(result) if !result.findings.is_empty() => {
                    advisories.extend(result.findings);
                }
                _ => {}
            }
        }
    }
}
```

Add `advisories` field to `LoadToolbenchContextResponse`:

```rust
pub struct LoadToolbenchContextResponse {
    // ... existing fields ...
    #[serde(default)]
    pub advisories: Vec<ResearchAdvisoryView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchAdvisoryView {
    pub source: String,
    pub id: String,
    pub title: String,
    pub severity: Option<String>,
    pub url: String,
}
```

Note: This automatically respects the rate limit. If too many dependencies are checked, later queries will be served from cache or the rate limit will kick in.

---

#### Task 14: Add research to ToolFamily

**File:** `crates/core/src/tooling.rs`

Add:
```rust
pub enum ToolFamily {
    // ... existing ...
    Research,
}
```

**File:** `crates/apps/orchestrator/src/tool_actions.rs`

Handle `ToolFamily::Research` in `plan_tool_action()`:

```rust
ToolFamily::Research => {
    // Research doesn't use sandbox — delegate to ResearchService directly
    // Return results as ToolActionResult with artifact_refs pointing to cached data
}
```

---

#### Task 15: Tests

1. **Allowlist:** Verify allowed URLs pass, blocked URLs fail. Edge cases: similar-looking domains, paths with `..`.

2. **Cache:** Insert result, retrieve → cached=true. Wait past TTL (use short TTL for test) → cache miss.

3. **Rate limit:** Make 10 queries → succeed. 11th query → rate limit error.

4. **RustSec source (mock):** Mock HTTP response with advisory data. Verify parsing produces correct `ResearchFinding` fields.

5. **GitHub Advisory source (mock):** Mock HTTP response. Verify parsing.

6. **Integration:** Create `ResearchService`, query a well-known crate (e.g., `openssl` which has known advisories). Verify non-empty findings (this test can be `#[ignore]` for CI since it requires network).

---

## Dependency Map

```
Task 1  (WorkingMemory)         ← no deps
Task 2  (LongTermMemory)        ← no deps
Task 3  (orchestrator working)  ← Task 1, Plan 1A (execute_dag)
Task 4  (orchestrator long-term)← Task 2
Task 5  (exports)               ← Task 1, Task 2

Task 6  (research crate)        ← no deps
Task 7  (research types)        ← Task 6
Task 8  (allowlist)             ← Task 6
Task 9  (cache)                 ← Task 6
Task 10 (RustSec source)        ← Task 7
Task 11 (GitHub source)         ← Task 7
Task 12 (ResearchService)       ← Tasks 8, 9, 10, 11
Task 13 (toolbench integration) ← Task 12
Task 14 (ToolFamily)            ← Task 12
Task 15 (tests)                 ← Tasks 1-14
```

Items A and B are independent and can be developed in parallel.
