# AXLE Lean Engine Integration Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the stub `LeanExternal` adapter with a real two-stage workflow: LLM generates a Lean 4 theorem stub from a Rust target, then AXLE validates, decomposes, and searches for counterexamples.

**Architecture:** A new `engine-lean` crate owns an async `AxleClient` (direct `reqwest` HTTP calls to `https://axle.axiommath.ai/api/v1`) and an LLM scaffold generator. The orchestrator detects `ToolFamily::LeanExternal` before entering its sandbox path and dispatches to `engine_lean::execute_lean_action()` instead. The sandbox is never involved — AXLE is a remote API.

**Tech Stack:** Rust, `reqwest` (already used in `llm` crate), `serde_json`, `mockito` (test mocking, already a dev-dep in `llm`), AXLE HTTP API (optional `AXLE_API_KEY` env var), existing `LlmProvider` trait.

---

## Background: The Two-Stage Lean Workflow

Because AXLE only operates on existing Lean code, the full workflow is:

```
Stage 1 — LLM scaffold:
  Rust symbol name + code snippet
    → LLM prompt (LlmRole::LeanScaffold)
    → file.lean  (theorem stubs with `sorry`)

Stage 2 — AXLE pipeline (sequential):
  file.lean
    → AXLE /check        (compile check, fail fast)
    → AXLE /sorry2lemma  (decompose stubs into subgoals)
    → AXLE /disprove     (search for counterexamples)
    → LeanWorkflowOutput (serialized to result.json artifact)
```

The analyst reviews the LLM-generated stub before triggering Stage 2. Nothing is automatic end-to-end; the human-review gate is between the two stages.

## API Key Policy

AXLE supports unauthenticated requests with reduced concurrency, and authenticated requests (via `Authorization: Bearer <key>`) with higher concurrency. The `AXLE_API_KEY` env var is **optional**:

- If set: the `Authorization` header is sent on every request.
- If not set: no `Authorization` header is sent; AXLE applies anonymous rate limits.

No part of this integration fails or errors when the key is absent.

---

## Task 1: `engine-lean` crate skeleton + request/response types

**Files:**
- Create: `crates/engine-lean/Cargo.toml`
- Create: `crates/engine-lean/src/lib.rs`
- Create: `crates/engine-lean/src/types.rs`
- Modify: `Cargo.toml` (workspace members)

### Step 1: Create `crates/engine-lean/Cargo.toml`

```toml
[package]
name = "engine-lean"
edition.workspace = true
license.workspace = true
version.workspace = true

[dependencies]
anyhow = "1"
audit-agent-core = { path = "../core" }
chrono = "0.4"
llm = { path = "../llm" }
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tempfile = "3"
tokio = { version = "1", features = ["rt"] }

[dev-dependencies]
async-trait = "0.1"
mockito = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

### Step 2: Add to workspace

In `Cargo.toml`, add `"crates/engine-lean"` to the `members` array:

```toml
members = [
  "crates/cli", "crates/core", "crates/engine-crypto",
  "crates/engine-distributed", "crates/engine-lean",
  ...
]
```

### Step 3: Write the failing types test

Create `crates/engine-lean/src/types.rs`:

```rust
use serde::{Deserialize, Serialize};

pub const DEFAULT_LEAN_ENV: &str = "lean-4.28.0";
pub const AXLE_BASE_URL: &str = "https://axle.axiommath.ai/api/v1";

#[derive(Debug, Clone, Serialize)]
pub struct AxleCheckRequest {
    pub content: String,
    pub environment: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AxleLeanMessages {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub infos: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AxleCheckResponse {
    pub okay: bool,
    pub content: String,
    pub lean_messages: AxleLeanMessages,
    pub tool_messages: AxleLeanMessages,
    pub failed_declarations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AxleDisproveRequest {
    pub content: String,
    pub environment: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AxleDisproveResponse {
    pub content: String,
    pub lean_messages: AxleLeanMessages,
    pub tool_messages: AxleLeanMessages,
    pub disproved_theorems: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AxleSorry2LemmaRequest {
    pub content: String,
    pub environment: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extract_sorries: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extract_errors: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AxleSorry2LemmaResponse {
    pub content: String,
    pub lean_messages: AxleLeanMessages,
    pub lemma_names: Vec<String>,
}

/// Serialized to `result.json` artifact at the end of Stage 2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeanWorkflowOutput {
    pub check_okay: bool,
    pub check_errors: Vec<String>,
    pub extracted_lemmas: Vec<String>,
    pub disproved_theorems: Vec<String>,
    pub lean_environment: String,
    pub authenticated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_request_omits_none_fields() {
        let req = AxleCheckRequest {
            content: "theorem foo : 1 = 1 := rfl".to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            timeout_seconds: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("content"));
        assert!(!json.contains("timeout_seconds"));
    }

    #[test]
    fn lean_workflow_output_roundtrips() {
        let out = LeanWorkflowOutput {
            check_okay: true,
            check_errors: vec![],
            extracted_lemmas: vec!["lemma_0".to_string()],
            disproved_theorems: vec![],
            lean_environment: DEFAULT_LEAN_ENV.to_string(),
            authenticated: false,
        };
        let json = serde_json::to_string(&out).unwrap();
        let back: LeanWorkflowOutput = serde_json::from_str(&json).unwrap();
        assert!(back.check_okay);
        assert_eq!(back.extracted_lemmas, vec!["lemma_0"]);
        assert!(!back.authenticated);
    }
}
```

### Step 4: Create minimal `crates/engine-lean/src/lib.rs`

```rust
pub mod types;
```

### Step 5: Run tests

```bash
cargo test -p engine-lean
```

Expected: both unit tests pass.

### Step 6: Commit

```bash
git add crates/engine-lean/ Cargo.toml
git commit -m "feat(engine-lean): add crate scaffold and AXLE request/response types"
```

---

## Task 2: `AxleClient` HTTP implementation

`AxleClient` holds an `Option<String>` API key. When the key is `Some`, the `Authorization` header is sent. When `None`, the header is omitted and AXLE applies anonymous rate limits.

**Files:**
- Create: `crates/engine-lean/src/client.rs`
- Create: `crates/engine-lean/tests/client_tests.rs`
- Modify: `crates/engine-lean/src/lib.rs`

### Step 1: Write the failing client tests

Create `crates/engine-lean/tests/client_tests.rs`:

```rust
use engine_lean::client::AxleClient;
use engine_lean::types::{
    AxleCheckRequest, AxleDisproveRequest, AxleSorry2LemmaRequest, DEFAULT_LEAN_ENV,
};
use mockito::Server;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn check_response_ok() -> &'static str {
    r#"{"okay":true,"content":"theorem foo : 1 = 1 := rfl","lean_messages":{"errors":[],"warnings":[],"infos":[]},"tool_messages":{"errors":[],"warnings":[],"infos":[]},"failed_declarations":[]}"#
}

fn check_response_fail() -> &'static str {
    r#"{"okay":false,"content":"bad","lean_messages":{"errors":["unknown identifier 'x'"],"warnings":[],"infos":[]},"tool_messages":{"errors":[],"warnings":[],"infos":[]},"failed_declarations":["foo"]}"#
}

const DISPROVE_RESPONSE: &str = r#"{"content":"...","lean_messages":{"errors":[],"warnings":[],"infos":[]},"tool_messages":{"errors":[],"warnings":[],"infos":[]},"disproved_theorems":["foo"]}"#;
const SORRY2LEMMA_RESPONSE: &str = r#"{"content":"...","lean_messages":{"errors":[],"warnings":[],"infos":[]},"lemma_names":["subgoal_0","subgoal_1"]}"#;

// --- with API key ---

#[tokio::test]
async fn check_with_key_sends_authorization_header() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/check")
        .match_header("authorization", mockito::Matcher::Regex("Bearer test-key".into()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(check_response_ok())
        .create_async()
        .await;

    let client = AxleClient::new(server.url(), Some("test-key".to_string()));
    let result = client
        .check(&AxleCheckRequest {
            content: "theorem foo : 1 = 1 := rfl".to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            timeout_seconds: None,
        })
        .await
        .unwrap();

    assert!(result.okay);
    assert!(result.lean_messages.errors.is_empty());
    mock.assert_async().await;
}

// --- without API key (anonymous) ---

#[tokio::test]
async fn check_without_key_omits_authorization_header() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/check")
        // explicitly assert NO authorization header present
        .match_header("authorization", mockito::Matcher::Missing)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(check_response_ok())
        .create_async()
        .await;

    let client = AxleClient::new(server.url(), None);
    let result = client
        .check(&AxleCheckRequest {
            content: "theorem foo : 1 = 1 := rfl".to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            timeout_seconds: None,
        })
        .await
        .unwrap();

    assert!(result.okay);
    mock.assert_async().await;
}

#[tokio::test]
async fn check_returns_errors_when_lean_is_invalid() {
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/check")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(check_response_fail())
        .create_async()
        .await;

    let client = AxleClient::new(server.url(), None);
    let result = client
        .check(&AxleCheckRequest {
            content: "bad lean code".to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            timeout_seconds: None,
        })
        .await
        .unwrap();

    assert!(!result.okay);
    assert!(!result.lean_messages.errors.is_empty());
}

#[tokio::test]
async fn disprove_returns_disproved_theorems() {
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/disprove")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(DISPROVE_RESPONSE)
        .create_async()
        .await;

    let client = AxleClient::new(server.url(), None);
    let result = client
        .disprove(&AxleDisproveRequest {
            content: "theorem foo : False := sorry".to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            names: Some(vec!["foo".to_string()]),
            timeout_seconds: None,
        })
        .await
        .unwrap();

    assert_eq!(result.disproved_theorems, vec!["foo"]);
}

#[tokio::test]
async fn sorry2lemma_returns_extracted_lemma_names() {
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/sorry2lemma")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(SORRY2LEMMA_RESPONSE)
        .create_async()
        .await;

    let client = AxleClient::new(server.url(), None);
    let result = client
        .sorry2lemma(&AxleSorry2LemmaRequest {
            content: "theorem foo : 1 = 1 := sorry".to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            extract_sorries: Some(true),
            extract_errors: Some(true),
            timeout_seconds: None,
        })
        .await
        .unwrap();

    assert_eq!(result.lemma_names, vec!["subgoal_0", "subgoal_1"]);
}

// --- from_env ---

#[tokio::test]
async fn from_env_uses_key_when_set() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe { std::env::set_var("AXLE_API_KEY", "env-key") };
    let client = AxleClient::from_env("http://localhost".to_string());
    assert!(client.has_api_key());

    unsafe { std::env::remove_var("AXLE_API_KEY") };
}

#[tokio::test]
async fn from_env_proceeds_without_key() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe { std::env::remove_var("AXLE_API_KEY") };
    let client = AxleClient::from_env("http://localhost".to_string());
    assert!(!client.has_api_key()); // no key — anonymous mode
}
```

### Step 2: Run to confirm compile failure

```bash
cargo test -p engine-lean --test client_tests 2>&1 | head -10
```

Expected: compile error — `client` module doesn't exist yet.

### Step 3: Implement `crates/engine-lean/src/client.rs`

```rust
use anyhow::{Context, Result};
use reqwest::{Client, RequestBuilder};

use crate::types::{
    AxleCheckRequest, AxleCheckResponse, AxleDisproveRequest, AxleDisproveResponse,
    AxleSorry2LemmaRequest, AxleSorry2LemmaResponse, AXLE_BASE_URL,
};

pub struct AxleClient {
    api_key: Option<String>,
    base_url: String,
    client: Client,
}

impl AxleClient {
    /// Primary constructor. `api_key = None` → anonymous (reduced concurrency).
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        Self {
            api_key,
            base_url,
            client: Client::new(),
        }
    }

    /// Read base URL from `AXLE_API_URL` (defaults to production) and
    /// API key from `AXLE_API_KEY` (optional — absent means anonymous).
    pub fn from_env(base_url: String) -> Self {
        let resolved_url =
            std::env::var("AXLE_API_URL").unwrap_or(base_url);
        let api_key = std::env::var("AXLE_API_KEY")
            .ok()
            .filter(|k| !k.trim().is_empty());
        Self::new(resolved_url, api_key)
    }

    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    /// Attach the Authorization header only when a key is present.
    fn authenticate(&self, builder: RequestBuilder) -> RequestBuilder {
        match &self.api_key {
            Some(key) => builder.bearer_auth(key),
            None => builder,
        }
    }

    pub async fn check(&self, request: &AxleCheckRequest) -> Result<AxleCheckResponse> {
        let url = format!("{}/check", self.base_url);
        self.authenticate(self.client.post(&url))
            .json(request)
            .send()
            .await
            .context("AXLE /check request failed")?
            .error_for_status()
            .context("AXLE /check returned error status")?
            .json::<AxleCheckResponse>()
            .await
            .context("failed to parse AXLE /check response")
    }

    pub async fn disprove(
        &self,
        request: &AxleDisproveRequest,
    ) -> Result<AxleDisproveResponse> {
        let url = format!("{}/disprove", self.base_url);
        self.authenticate(self.client.post(&url))
            .json(request)
            .send()
            .await
            .context("AXLE /disprove request failed")?
            .error_for_status()
            .context("AXLE /disprove returned error status")?
            .json::<AxleDisproveResponse>()
            .await
            .context("failed to parse AXLE /disprove response")
    }

    pub async fn sorry2lemma(
        &self,
        request: &AxleSorry2LemmaRequest,
    ) -> Result<AxleSorry2LemmaResponse> {
        let url = format!("{}/sorry2lemma", self.base_url);
        self.authenticate(self.client.post(&url))
            .json(request)
            .send()
            .await
            .context("AXLE /sorry2lemma request failed")?
            .error_for_status()
            .context("AXLE /sorry2lemma returned error status")?
            .json::<AxleSorry2LemmaResponse>()
            .await
            .context("failed to parse AXLE /sorry2lemma response")
    }
}
```

### Step 4: Update `crates/engine-lean/src/lib.rs`

```rust
pub mod client;
pub mod types;
```

### Step 5: Run tests

```bash
cargo test -p engine-lean --test client_tests
```

Expected: all 7 tests pass. Particularly `check_without_key_omits_authorization_header` verifies anonymous mode works.

### Step 6: Commit

```bash
git add crates/engine-lean/src/client.rs crates/engine-lean/src/lib.rs \
        crates/engine-lean/tests/client_tests.rs
git commit -m "feat(engine-lean): implement AxleClient with optional auth and check/disprove/sorry2lemma"
```

---

## Task 3: LLM scaffold generator (Stage 1)

**Files:**
- Modify: `crates/llm/src/provider.rs` (add `LeanScaffold` to `LlmRole`)
- Create: `crates/engine-lean/src/scaffold.rs`
- Modify: `crates/engine-lean/src/lib.rs`

### Step 1: Write the failing scaffold tests

Create `crates/engine-lean/tests/scaffold_tests.rs`:

```rust
use anyhow::Result;
use async_trait::async_trait;
use engine_lean::scaffold::generate_lean_stub;
use llm::{CompletionOpts, LlmProvider};

struct FixedProvider(String);

#[async_trait]
impl LlmProvider for FixedProvider {
    async fn complete(&self, _prompt: &str, _opts: &CompletionOpts) -> Result<String> {
        Ok(self.0.clone())
    }
    fn name(&self) -> &str { "fixed" }
    fn is_available(&self) -> bool { true }
}

#[tokio::test]
async fn generate_stub_returns_llm_output() {
    let provider = FixedProvider(
        "import Mathlib\ntheorem foo_invariant : True := sorry".to_string(),
    );
    let stub = generate_lean_stub("foo", "fn foo(x: u64) -> u64 { x }", &provider)
        .await
        .unwrap();
    assert!(stub.contains("import Mathlib"));
    assert!(stub.contains("sorry"));
}

#[tokio::test]
async fn generate_stub_truncates_oversized_snippet() {
    let provider = FixedProvider("import Mathlib\ntheorem bar : True := sorry".to_string());
    // 10_000 char snippet — must not panic or error
    let big = "x".repeat(10_000);
    let result = generate_lean_stub("bar", &big, &provider).await;
    assert!(result.is_ok());
}
```

### Step 2: Run to confirm compile failure

```bash
cargo test -p engine-lean --test scaffold_tests 2>&1 | head -10
```

Expected: compile error — `scaffold` module doesn't exist.

### Step 3: Add `LeanScaffold` to `LlmRole` in `crates/llm/src/provider.rs`

```rust
pub enum LlmRole {
    Scaffolding,
    SearchHints,
    ProseRendering,
    LeanScaffold,   // ← add this line
}
```

### Step 4: Implement `crates/engine-lean/src/scaffold.rs`

```rust
use anyhow::Result;
use llm::{CompletionOpts, LlmProvider, LlmRole, llm_call};
use llm::sanitize::sanitize_prompt_input;

const MAX_SNIPPET_CHARS: usize = 3_000;

const LEAN_STUB_PROMPT: &str =
    "You are a Lean 4 formalization assistant. \
     Given a Rust function name and implementation, produce a Lean 4 theorem file \
     that formalizes the key invariants of that function. \
     Rules: start with `import Mathlib`, use `sorry` for all proof bodies, \
     output ONLY valid Lean 4 source — no prose, no markdown fences.";

/// Stage 1: generate a Lean 4 theorem stub for `target_name` from the Rust source.
/// The analyst must review the output before passing it to Stage 2 (AXLE).
pub async fn generate_lean_stub(
    target_name: &str,
    rust_snippet: &str,
    llm: &dyn LlmProvider,
) -> Result<String> {
    let safe_name = sanitize_prompt_input(target_name);
    let safe_snippet = sanitize_prompt_input(
        &rust_snippet.chars().take(MAX_SNIPPET_CHARS).collect::<String>(),
    );
    let prompt = format!(
        "{LEAN_STUB_PROMPT}\n\nFunction name: {safe_name}\nRust source:\n{safe_snippet}\n\nLean 4 formalization:"
    );
    llm_call(
        llm,
        LlmRole::LeanScaffold,
        &prompt,
        &CompletionOpts {
            temperature_millis: 200,
            max_tokens: 1024,
        },
    )
    .await
}
```

### Step 5: Update `crates/engine-lean/src/lib.rs`

```rust
pub mod client;
pub mod scaffold;
pub mod types;
```

### Step 6: Run tests

```bash
cargo test -p engine-lean --test scaffold_tests
cargo test -p llm
```

Expected: both pass. `LlmRole` is only used for tracing in `llm_call`, so adding a variant never breaks downstream.

### Step 7: Commit

```bash
git add crates/llm/src/provider.rs crates/engine-lean/src/scaffold.rs \
        crates/engine-lean/src/lib.rs crates/engine-lean/tests/scaffold_tests.rs
git commit -m "feat(engine-lean): add LeanScaffold LlmRole and LLM stub generator"
```

---

## Task 4: AXLE tool action executor

Reads a `.lean` file and runs `check → sorry2lemma → disprove`.

**Files:**
- Create: `crates/engine-lean/src/tool_actions/mod.rs`
- Create: `crates/engine-lean/src/tool_actions/axle.rs`
- Create: `crates/engine-lean/tests/axle_action_tests.rs`
- Modify: `crates/engine-lean/src/lib.rs`

### Step 1: Write the failing executor tests

Create `crates/engine-lean/tests/axle_action_tests.rs`:

```rust
use audit_agent_core::tooling::{
    ToolActionRequest, ToolActionStatus, ToolBudget, ToolFamily, ToolTarget,
};
use engine_lean::tool_actions::axle::execute_lean_action;
use mockito::Server;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use tempfile::NamedTempFile;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn lean_request(path: &str) -> ToolActionRequest {
    ToolActionRequest {
        session_id: "sess-test".to_string(),
        workspace_root: None,
        tool_family: ToolFamily::LeanExternal,
        target: ToolTarget::File { path: path.to_string() },
        budget: ToolBudget::default(),
    }
}

fn mock_check_ok(server: &mut mockito::ServerGuard) {
    server
        .mock("POST", "/check")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"okay":true,"content":"","lean_messages":{"errors":[],"warnings":[],"infos":[]},"tool_messages":{"errors":[],"warnings":[],"infos":[]},"failed_declarations":[]}"#)
        .create();
}

fn mock_sorry2lemma_with_lemmas(server: &mut mockito::ServerGuard) {
    server
        .mock("POST", "/sorry2lemma")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"content":"extracted content","lean_messages":{"errors":[],"warnings":[],"infos":[]},"lemma_names":["subgoal_0"]}"#)
        .create();
}

fn mock_disprove_no_counterexample(server: &mut mockito::ServerGuard) {
    server
        .mock("POST", "/disprove")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"content":"","lean_messages":{"errors":[],"warnings":[],"infos":[]},"tool_messages":{"errors":[],"warnings":[],"infos":[]},"disproved_theorems":[]}"#)
        .create();
}

#[tokio::test]
async fn full_pipeline_returns_completed_with_summary() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe { std::env::remove_var("AXLE_API_KEY") }; // anonymous mode
    let mut server = Server::new();
    mock_check_ok(&mut server);
    mock_sorry2lemma_with_lemmas(&mut server);
    mock_disprove_no_counterexample(&mut server);

    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "import Mathlib\ntheorem foo : True := sorry").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    let result = execute_lean_action(&lean_request(&path), &server.url())
        .await
        .unwrap();

    assert_eq!(result.status, ToolActionStatus::Completed);
    assert_eq!(result.tool_family, ToolFamily::LeanExternal);
    assert!(!result.artifact_refs.is_empty());
    let preview = result.stdout_preview.unwrap();
    assert!(preview.contains("check: ok"));
    assert!(preview.contains("lemmas extracted: 1"));
    assert!(preview.contains("disproved: none"));
}

#[tokio::test]
async fn full_pipeline_with_api_key_authenticated_field_is_true() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe { std::env::set_var("AXLE_API_KEY", "test-key") };
    let mut server = Server::new();
    mock_check_ok(&mut server);
    mock_sorry2lemma_with_lemmas(&mut server);
    mock_disprove_no_counterexample(&mut server);

    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "import Mathlib\ntheorem foo : True := sorry").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    let result = execute_lean_action(&lean_request(&path), &server.url())
        .await
        .unwrap();

    assert_eq!(result.status, ToolActionStatus::Completed);
    // stdout_preview encodes whether auth was used
    let preview = result.stdout_preview.unwrap();
    assert!(preview.contains("authenticated: true"));

    unsafe { std::env::remove_var("AXLE_API_KEY") };
}

#[tokio::test]
async fn check_failure_returns_failed_and_skips_remaining_steps() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe { std::env::remove_var("AXLE_API_KEY") };
    let mut server = Server::new();
    server
        .mock("POST", "/check")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"okay":false,"content":"","lean_messages":{"errors":["unknown id 'x'"],"warnings":[],"infos":[]},"tool_messages":{"errors":[],"warnings":[],"infos":[]},"failed_declarations":["foo"]}"#)
        .create();
    // sorry2lemma and disprove must NOT be called — no mock registered for them

    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "bad lean code here").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    let result = execute_lean_action(&lean_request(&path), &server.url())
        .await
        .unwrap();

    assert_eq!(result.status, ToolActionStatus::Failed);
    let preview = result.stdout_preview.unwrap();
    assert!(preview.contains("check: FAILED"));
    assert!(preview.contains("unknown id 'x'"));
}

#[tokio::test]
async fn missing_lean_file_returns_error() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe { std::env::remove_var("AXLE_API_KEY") };

    let result = execute_lean_action(
        &lean_request("/nonexistent/path.lean"),
        "http://localhost:9999",
    )
    .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("failed to read"));
}
```

### Step 2: Run to confirm compile failure

```bash
cargo test -p engine-lean --test axle_action_tests 2>&1 | head -10
```

### Step 3: Create `crates/engine-lean/src/tool_actions/mod.rs`

```rust
pub mod axle;
```

### Step 4: Implement `crates/engine-lean/src/tool_actions/axle.rs`

```rust
use anyhow::{Context, Result};
use audit_agent_core::engine::SandboxImage;
use audit_agent_core::tooling::{
    ToolActionRequest, ToolActionResult, ToolActionStatus, ToolExecutionPlan, ToolFamily,
    ToolTarget,
};
use chrono::Utc;

use crate::client::AxleClient;
use crate::types::{
    AxleCheckRequest, AxleDisproveRequest, AxleSorry2LemmaRequest, LeanWorkflowOutput,
    AXLE_BASE_URL, DEFAULT_LEAN_ENV,
};

/// Called by the orchestrator for ToolFamily::LeanExternal.
/// `axle_base_url` is injected so tests can point at a mockito server.
/// Production callers pass `engine_lean::types::AXLE_BASE_URL`.
pub async fn execute_lean_action(
    request: &ToolActionRequest,
    axle_base_url: &str,
) -> Result<ToolActionResult> {
    let lean_path = request.target.display_value();
    let target_slug = request.target.slug();
    let lean_content = std::fs::read_to_string(lean_path)
        .with_context(|| format!("failed to read Lean file: {lean_path}"))?;

    // API key is optional — absent means anonymous (lower concurrency on AXLE's side).
    let api_key = std::env::var("AXLE_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty());
    let authenticated = api_key.is_some();
    let client = AxleClient::new(axle_base_url.to_string(), api_key);

    let timeout_per_step = (request.budget.timeout_secs as f64) / 3.0;

    // Step 1: syntax + semantics validation
    let check = client
        .check(&AxleCheckRequest {
            content: lean_content.clone(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            timeout_seconds: Some(timeout_per_step),
        })
        .await?;

    if !check.okay {
        let output = LeanWorkflowOutput {
            check_okay: false,
            check_errors: check.lean_messages.errors.clone(),
            extracted_lemmas: vec![],
            disproved_theorems: vec![],
            lean_environment: DEFAULT_LEAN_ENV.to_string(),
            authenticated,
        };
        let summary = format!(
            "check: FAILED\nauthenticated: {authenticated}\nerrors: {}",
            check.lean_messages.errors.join("; ")
        );
        return Ok(build_result(request, ToolActionStatus::Failed, &target_slug, &output, summary));
    }

    // Step 2: decompose sorry/error stubs into standalone lemmas
    let sorry = client
        .sorry2lemma(&AxleSorry2LemmaRequest {
            content: lean_content.clone(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            extract_sorries: Some(true),
            extract_errors: Some(true),
            timeout_seconds: Some(timeout_per_step),
        })
        .await?;

    // Step 3: search for counterexamples (Plausible property-based testing)
    let (disprove_content, disprove_names) = if sorry.lemma_names.is_empty() {
        (lean_content, None)
    } else {
        (sorry.content.clone(), Some(sorry.lemma_names.clone()))
    };

    let disprove = client
        .disprove(&AxleDisproveRequest {
            content: disprove_content,
            environment: DEFAULT_LEAN_ENV.to_string(),
            names: disprove_names,
            timeout_seconds: Some(timeout_per_step),
        })
        .await?;

    let output = LeanWorkflowOutput {
        check_okay: true,
        check_errors: vec![],
        extracted_lemmas: sorry.lemma_names,
        disproved_theorems: disprove.disproved_theorems.clone(),
        lean_environment: DEFAULT_LEAN_ENV.to_string(),
        authenticated,
    };

    let summary = format!(
        "check: ok\nauthenticated: {authenticated}\nlemmas extracted: {}\ndisproved: {}",
        output.extracted_lemmas.len(),
        if output.disproved_theorems.is_empty() {
            "none".to_string()
        } else {
            output.disproved_theorems.join(", ")
        }
    );
    Ok(build_result(request, ToolActionStatus::Completed, &target_slug, &output, summary))
}

fn build_result(
    request: &ToolActionRequest,
    status: ToolActionStatus,
    target_slug: &str,
    output: &LeanWorkflowOutput,
    summary: String,
) -> ToolActionResult {
    let artifact_ref = format!(
        "{}/tool-runs/axle/{target_slug}/result.json",
        request.session_id
    );
    let preview = summary[..summary.len().min(1024)].to_string();
    ToolActionResult {
        action_id: format!("axle-{}", Utc::now().timestamp_micros()),
        session_id: request.session_id.clone(),
        tool_family: ToolFamily::LeanExternal,
        target: request.target.clone(),
        command: vec![
            "axle".to_string(),
            "check+sorry2lemma+disprove".to_string(),
            request.target.display_value().to_string(),
        ],
        artifact_refs: vec![artifact_ref],
        rationale: "AXLE: validate Lean file, decompose stubs, search for counterexamples"
            .to_string(),
        status,
        stdout_preview: Some(preview),
        stderr_preview: None,
    }
}

/// Sentinel plan stored in plan_tool_action for LeanExternal.
/// Never executed via sandbox — the orchestrator dispatches to execute_lean_action() first.
pub fn sentinel_plan(session_id: &str, target: &ToolTarget) -> ToolExecutionPlan {
    ToolExecutionPlan {
        tool_family: ToolFamily::LeanExternal,
        image: SandboxImage::Custom("axle-remote".to_string()),
        command: vec!["axle".to_string(), target.display_value().to_string()],
        artifact_refs: vec![format!(
            "{session_id}/tool-runs/axle/{}/result.json",
            target.slug()
        )],
        rationale: "AXLE remote API — dispatched directly, not via sandbox".to_string(),
    }
}
```

### Step 5: Update `crates/engine-lean/src/lib.rs`

```rust
pub mod client;
pub mod scaffold;
pub mod tool_actions;
pub mod types;

pub use tool_actions::axle::execute_lean_action;
```

### Step 6: Run tests

```bash
cargo test -p engine-lean --test axle_action_tests
```

Expected: all 4 tests pass.

### Step 7: Commit

```bash
git add crates/engine-lean/src/tool_actions/ crates/engine-lean/src/lib.rs \
        crates/engine-lean/tests/axle_action_tests.rs
git commit -m "feat(engine-lean): implement AXLE pipeline executor (check+sorry2lemma+disprove)"
```

---

## Task 5: Live integration tests against the real AXLE server

These tests hit `https://axle.axiommath.ai/api/v1` directly to verify the full contract
against the real service. They are `#[ignore]` and only run when explicitly invoked.

**Files:**
- Create: `crates/engine-lean/tests/live_axle_tests.rs`

### Step 1: Create `crates/engine-lean/tests/live_axle_tests.rs`

The test file uses four Lean snippets that exercise every branch of the pipeline:

```rust
//! Live integration tests against the real AXLE server.
//!
//! These tests require network access and are excluded from normal CI runs.
//! Run them with:
//!
//!   # Anonymous (no key, reduced concurrency):
//!   cargo test -p engine-lean -- --ignored live_
//!
//!   # Authenticated (higher concurrency):
//!   AXLE_API_KEY=<your-key> cargo test -p engine-lean -- --ignored live_

use engine_lean::client::AxleClient;
use engine_lean::tool_actions::axle::execute_lean_action;
use engine_lean::types::{
    AxleCheckRequest, AxleDisproveRequest, AxleSorry2LemmaRequest, DEFAULT_LEAN_ENV,
    AXLE_BASE_URL,
};
use audit_agent_core::tooling::{ToolActionRequest, ToolActionStatus, ToolBudget, ToolFamily, ToolTarget};
use std::io::Write;
use tempfile::NamedTempFile;

/// A theorem that compiles cleanly with no sorry — check should return okay=true.
const VALID_LEAN: &str = r#"
import Mathlib

theorem addition_comm (n m : Nat) : n + m = m + n := Nat.add_comm n m
"#;

/// A theorem with a sorry — check returns okay=true (sorry is accepted),
/// sorry2lemma should extract at least one lemma.
const LEAN_WITH_SORRY: &str = r#"
import Mathlib

theorem mul_comm_sorry (n m : Nat) : n * m = m * n := by sorry
"#;

/// A false claim — disprove should find a counterexample.
const FALSE_CLAIM: &str = r#"
import Mathlib

theorem false_addition : 1 + 1 = 3 := by sorry
"#;

/// Lean with a syntax error — check must return okay=false with error messages.
const INVALID_LEAN: &str = r#"
import Mathlib

theorem broken : @@@INVALID@@@ := by decide
"#;

fn make_client() -> AxleClient {
    AxleClient::from_env(AXLE_BASE_URL.to_string())
}

// ── /check ────────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires live network access to axle.axiommath.ai"]
async fn live_check_valid_lean_returns_okay_true() {
    let client = make_client();
    let result = client
        .check(&AxleCheckRequest {
            content: VALID_LEAN.to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            timeout_seconds: Some(120.0),
        })
        .await
        .expect("AXLE /check request must succeed");

    assert!(
        result.okay,
        "valid Lean should compile cleanly; errors: {:?}",
        result.lean_messages.errors
    );
    assert!(result.lean_messages.errors.is_empty());
    assert!(result.failed_declarations.is_empty());
}

#[tokio::test]
#[ignore = "requires live network access to axle.axiommath.ai"]
async fn live_check_invalid_lean_returns_okay_false_with_errors() {
    let client = make_client();
    let result = client
        .check(&AxleCheckRequest {
            content: INVALID_LEAN.to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            timeout_seconds: Some(120.0),
        })
        .await
        .expect("AXLE /check request must succeed (HTTP 200 even for invalid Lean)");

    assert!(
        !result.okay,
        "invalid Lean must not compile; got okay=true unexpectedly"
    );
    assert!(
        !result.lean_messages.errors.is_empty(),
        "invalid Lean must produce at least one error message"
    );
}

// ── /sorry2lemma ──────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires live network access to axle.axiommath.ai"]
async fn live_sorry2lemma_extracts_at_least_one_lemma() {
    let client = make_client();
    let result = client
        .sorry2lemma(&AxleSorry2LemmaRequest {
            content: LEAN_WITH_SORRY.to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            extract_sorries: Some(true),
            extract_errors: Some(false),
            timeout_seconds: Some(120.0),
        })
        .await
        .expect("AXLE /sorry2lemma request must succeed");

    assert!(
        !result.lemma_names.is_empty(),
        "sorry2lemma must extract at least one lemma from the sorry stub; got none.\n\
         lean_messages: {:?}",
        result.lean_messages.errors
    );
    // The extracted content should contain the original sorry theorem name.
    assert!(
        result.content.contains("mul_comm_sorry"),
        "extracted content must reference the original theorem name"
    );
}

// ── /disprove ─────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires live network access to axle.axiommath.ai"]
async fn live_disprove_finds_counterexample_for_false_claim() {
    let client = make_client();
    let result = client
        .disprove(&AxleDisproveRequest {
            content: FALSE_CLAIM.to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            names: Some(vec!["false_addition".to_string()]),
            timeout_seconds: Some(120.0),
        })
        .await
        .expect("AXLE /disprove request must succeed");

    assert!(
        result.disproved_theorems.contains(&"false_addition".to_string()),
        "AXLE must disprove the false 1+1=3 claim; disproved={:?}, errors={:?}",
        result.disproved_theorems,
        result.lean_messages.errors
    );
}

#[tokio::test]
#[ignore = "requires live network access to axle.axiommath.ai"]
async fn live_disprove_does_not_disprove_true_theorem() {
    let client = make_client();
    // Nat.add_comm is a true theorem; disprove should NOT find a counterexample.
    let result = client
        .disprove(&AxleDisproveRequest {
            content: VALID_LEAN.to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            names: Some(vec!["addition_comm".to_string()]),
            timeout_seconds: Some(120.0),
        })
        .await
        .expect("AXLE /disprove request must succeed");

    assert!(
        result.disproved_theorems.is_empty(),
        "AXLE must not disprove a true theorem; unexpectedly disproved={:?}",
        result.disproved_theorems
    );
}

// ── full pipeline via execute_lean_action ─────────────────────────────────────

#[tokio::test]
#[ignore = "requires live network access to axle.axiommath.ai"]
async fn live_full_pipeline_on_sorry_theorem_completes() {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(LEAN_WITH_SORRY.as_bytes()).unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    let request = ToolActionRequest {
        session_id: "live-test-session".to_string(),
        workspace_root: None,
        tool_family: ToolFamily::LeanExternal,
        target: ToolTarget::File { path },
        budget: ToolBudget {
            timeout_secs: 360,
            ..ToolBudget::default()
        },
    };

    let result = execute_lean_action(&request, AXLE_BASE_URL)
        .await
        .expect("full pipeline must complete without error");

    assert_eq!(
        result.status,
        ToolActionStatus::Completed,
        "pipeline must complete; preview={:?}",
        result.stdout_preview
    );
    assert!(!result.artifact_refs.is_empty());

    let preview = result.stdout_preview.unwrap();
    assert!(preview.contains("check: ok"), "preview: {preview}");
    assert!(preview.contains("lemmas extracted:"), "preview: {preview}");

    eprintln!("Live pipeline result:\n{preview}");
}

#[tokio::test]
#[ignore = "requires live network access to axle.axiommath.ai"]
async fn live_full_pipeline_on_false_claim_completes_and_reports_disproved() {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(FALSE_CLAIM.as_bytes()).unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    let request = ToolActionRequest {
        session_id: "live-test-session-disprove".to_string(),
        workspace_root: None,
        tool_family: ToolFamily::LeanExternal,
        target: ToolTarget::File { path },
        budget: ToolBudget {
            timeout_secs: 360,
            ..ToolBudget::default()
        },
    };

    let result = execute_lean_action(&request, AXLE_BASE_URL)
        .await
        .expect("full pipeline must complete without error");

    assert_eq!(result.status, ToolActionStatus::Completed);
    let preview = result.stdout_preview.unwrap();
    // The false_addition theorem should show up as disproved.
    assert!(
        !preview.contains("disproved: none"),
        "false_addition must be disproved; preview: {preview}"
    );
    eprintln!("Live disprove pipeline result:\n{preview}");
}
```

### Step 2: Run the live tests (requires network)

```bash
# Anonymous — works without any key:
cargo test -p engine-lean -- --ignored live_

# Authenticated — higher concurrency:
AXLE_API_KEY=<your-key> cargo test -p engine-lean -- --ignored live_
```

Expected output for each test:
- `live_check_valid_lean_returns_okay_true` → passes, `okay: true`
- `live_check_invalid_lean_returns_okay_false_with_errors` → passes, errors list non-empty
- `live_sorry2lemma_extracts_at_least_one_lemma` → passes, `lemma_names` non-empty
- `live_disprove_finds_counterexample_for_false_claim` → passes, `disproved_theorems = ["false_addition"]`
- `live_disprove_does_not_disprove_true_theorem` → passes, `disproved_theorems = []`
- `live_full_pipeline_on_sorry_theorem_completes` → `check: ok`, `lemmas extracted: 1+`, eprintln shows full summary
- `live_full_pipeline_on_false_claim_completes_and_reports_disproved` → `disproved:` contains a theorem name

### Step 3: Commit

```bash
git add crates/engine-lean/tests/live_axle_tests.rs
git commit -m "test(engine-lean): add live integration tests against real AXLE server"
```

---

## Task 6: Wire `LeanExternal` into the orchestrator

**Files:**
- Modify: `crates/orchestrator/Cargo.toml`
- Modify: `crates/orchestrator/src/tool_actions.rs`
- Modify: `crates/orchestrator/src/lib.rs`
- Create: `crates/orchestrator/tests/orchestrator_lean_tests.rs`

### Step 1: Write the failing orchestrator test

Create `crates/orchestrator/tests/orchestrator_lean_tests.rs`:

```rust
use audit_agent_core::tooling::{ToolActionRequest, ToolActionStatus, ToolBudget, ToolFamily, ToolTarget};
use orchestrator::AuditOrchestrator;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use tempfile::NamedTempFile;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Without AXLE_API_KEY the action must still attempt the request (anonymous mode),
/// and since there is no real AXLE server available in tests, it errors at the HTTP
/// level — NOT by returning a noop sandbox success. This proves LeanExternal no
/// longer goes through the sandbox.
#[tokio::test]
async fn lean_external_bypasses_sandbox_and_attempts_axle_call() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe { std::env::remove_var("AXLE_API_KEY") };

    // Point at a port guaranteed not to be listening so the HTTP call fails
    // immediately with a connection error rather than timing out.
    unsafe { std::env::set_var("AXLE_API_URL", "http://127.0.0.1:1") };

    let orchestrator = AuditOrchestrator::for_tests();

    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "import Mathlib\ntheorem foo : True := sorry").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    let request = ToolActionRequest {
        session_id: "sess-orchestrator-lean-test".to_string(),
        workspace_root: None,
        tool_family: ToolFamily::LeanExternal,
        target: ToolTarget::File { path },
        budget: ToolBudget::default(),
    };

    let result = orchestrator.run_tool_action(request).await;

    // The noop sandbox runner would have returned Ok(exit_code=0).
    // An HTTP connection error means we correctly went through the AXLE path.
    assert!(
        result.is_err(),
        "expected HTTP connection error from AXLE path, got Ok — sandbox may have intercepted"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("AXLE") || msg.contains("connection") || msg.contains("refused"),
        "unexpected error: {msg}"
    );

    unsafe { std::env::remove_var("AXLE_API_URL") };
}
```

### Step 2: Run to confirm current behavior (expected failure)

```bash
cargo test -p orchestrator --test orchestrator_lean_tests 2>&1 | head -20
```

Expected: test fails because `LeanExternal` currently routes to the noop sandbox and returns `Ok(...)`.

### Step 3: Add `engine-lean` to `crates/orchestrator/Cargo.toml`

```toml
[dependencies]
...
engine-lean = { path = "../engine-lean" }
```

### Step 4: Replace `LeanExternal` in `crates/orchestrator/src/tool_actions.rs`

Change:
```rust
ToolFamily::LeanExternal => external_plan(
    "lean-external-adapter",
    ToolFamily::LeanExternal,
    &request.session_id,
    &request.target,
    "External Lean adapter slot (experimental and opt-in)",
),
```
To:
```rust
ToolFamily::LeanExternal => engine_lean::tool_actions::axle::sentinel_plan(
    &request.session_id,
    &request.target,
),
```

### Step 5: Add early dispatch in `crates/orchestrator/src/lib.rs`

In `run_tool_action`, add the branch **before** the `plan_tool_action` call. The `AXLE_BASE_URL` is read from the `AXLE_API_URL` env var if set (for test injection), falling back to the production constant.

```rust
pub async fn run_tool_action(&self, request: ToolActionRequest) -> Result<ToolActionResult> {
    // LeanExternal bypasses the sandbox — it talks to the AXLE remote API directly.
    if request.tool_family == ToolFamily::LeanExternal {
        let base_url = std::env::var("AXLE_API_URL")
            .unwrap_or_else(|_| engine_lean::types::AXLE_BASE_URL.to_string());
        return engine_lean::execute_lean_action(&request, &base_url).await;
    }

    let plan = tool_actions::plan_tool_action(&request);
    // ... rest of the function unchanged ...
```

### Step 6: Run all orchestrator tests

```bash
cargo test -p orchestrator --test orchestrator_lean_tests
cargo test -p orchestrator
```

Expected: lean test passes; all existing orchestrator tests still pass.

### Step 7: Commit

```bash
git add crates/orchestrator/Cargo.toml crates/orchestrator/src/tool_actions.rs \
        crates/orchestrator/src/lib.rs crates/orchestrator/tests/orchestrator_lean_tests.rs
git commit -m "feat(orchestrator): dispatch LeanExternal to AXLE engine, bypass sandbox"
```

---

## Task 7: Lean formal playbook

**Important:** `ToolPlaybook` in `crates/knowledge/src/models.rs` deserializes exactly these fields:
`id`, `applies_to`, `domains`, `preferred_tools`, `initial_queries`. Any extra field causes a parse error.
Do not add `activation_conditions`, `notes`, or any other key.

**Files:**
- Create: `knowledge/playbooks/lean-formal.yaml`

### Step 1: Create `knowledge/playbooks/lean-formal.yaml`

```yaml
id: lean-formal
applies_to:
  - rust
  - circom
domains:
  - crypto
  - zk
preferred_tools:
  - lean-external
initial_queries:
  - key invariants expressible as Lean theorems
  - protocol safety properties amenable to counterexample search
  - overflow or underflow conditions in arithmetic-heavy functions
```

### Step 2: Verify playbook loads without error

```bash
cargo test -p knowledge
```

Expected: all existing tests pass and the new file doesn't cause a parse error.

### Step 3: Commit

```bash
git add knowledge/playbooks/lean-formal.yaml
git commit -m "feat(knowledge): add lean-formal playbook for AXLE-backed formal verification"
```

---

## Task 8: Full workspace build verification

### Step 1: Build the full workspace

```bash
cargo build --workspace 2>&1 | tail -20
```

Expected: no errors, `engine-lean` appears in the build.

### Step 2: Run all unit + mockito tests (no network needed)

```bash
cargo test --workspace 2>&1 | tail -30
```

Expected: all pass. Live tests are `#[ignore]` and will not run.

### Step 3: Verify the stub adapter is gone

```bash
grep -r "lean-external-adapter" crates/orchestrator/src/
```

Expected: no matches.

### Step 4: Commit

```bash
git commit --allow-empty -m "chore: verify full workspace build after AXLE integration"
```

---

## Environment Variables Reference

| Variable | Required | Default | Purpose |
|---|---|---|---|
| `AXLE_API_KEY` | No | absent = anonymous | Bearer token; if set, sent on every request for higher concurrency |
| `AXLE_API_URL` | No | `https://axle.axiommath.ai/api/v1` | Override base URL (used in tests to inject a local mockito or a staging server) |

---

## Running the Live Tests

```bash
# Anonymous mode (no key — works, limited concurrency):
cargo test -p engine-lean -- --ignored live_

# Authenticated mode:
AXLE_API_KEY=<your-key> cargo test -p engine-lean -- --ignored live_

# Single test:
AXLE_API_KEY=<your-key> cargo test -p engine-lean -- --ignored live_full_pipeline_on_false_claim_completes_and_reports_disproved -- --nocapture
```

The `--nocapture` flag prints the `eprintln!` summary so you can read the actual AXLE response.

---

## What Is NOT In Scope

- Automatic end-to-end execution without analyst review between Stage 1 and Stage 2
- Writing Lean 4 proofs to completion (`repair_proofs` is available but deferred)
- A dedicated Lean container image (AXLE is a remote API — no container needed)
