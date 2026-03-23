# Plan 3 — T1/T2: LLM Intelligence

> **For Agent:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make LLM behavior configurable per role so that different audit tasks can use different models, temperatures, and token budgets. Then build an evaluation harness so that role config changes are measurable, not taste-driven.

**Architecture:** Item A introduces a `RoleAwareProvider` that wraps multiple underlying providers and dispatches per role. Item B creates a new `llm-eval` crate with fixture-based benchmarking. Both are backend-only; no UI changes.

**Tech Stack:** Rust workspace crates, serde/serde_yaml, reqwest, tokio, clap (CLI subcommand).

**Depends on:** Plan 1B (unified LLM contract enforcement and provenance). The `RoleAwareProvider` delegates to `llm_call_traced()` and the `ContractEnforcer`. The eval harness measures outputs from the enforcement layer.

---

## Planning Conventions

- Recommended execution context: a dedicated git worktree.
- Do not break existing `provider_from_env()` behavior — `RoleAwareProvider` is an opt-in upgrade.
- Keep environment variable naming consistent: `LLM_ROLE_{ROLE}_{PARAM}` pattern.
- Evaluation fixtures must declare whether `TemplateFallback` support is required, supported, or explicitly skipped for that role.
- Deterministic engines and ProjectIR remain the source of truth for audit analysis; AI layers may assist planning, explanation, and recovery, but do not redefine deterministic results.
- Every task must land with tests.

## Release Gates

This plan is complete only when all of the following are true:

- `RoleAwareProvider` can dispatch different roles to different providers/models/parameters.
- Role config can be loaded from environment variables or `audit.yaml`.
- Existing behavior is unchanged when no role-specific env vars are set.
- The eval CLI subcommand runs fixtures against any configured provider and reports pass/fail.
- At least 3 fixtures per supported role exist.
- `TemplateFallback` passes fixtures for explicitly supported roles; unsupported roles are marked skipped, not failed.
- Baseline save/compare workflow is documented and functional.
- `cargo test -p llm` and `cargo test -p llm-eval` pass.

## Explicit Non-Goals

- Runtime provider failover (Plan 5A) — this plan routes by role at startup, not by availability.
- Adviser/reflector (Plan 5B) — `LlmRole::Advisory` is not added yet.
- Prompt engineering — this plan provides the infrastructure to measure prompt changes, not the prompts themselves.

---

## Item A: Role-Specific LLM Configuration

### Context

All 7 `llm_call` sites currently use whatever single provider `provider_from_env()` returns. `CompletionOpts::default()` is `temperature=0.1, max_tokens=1024`. Some call sites override (economic: 200/256, report: 200/512, lean: 200/1024), but there is no centralized configuration. The repo's LLM philosophy is that different roles have different quality requirements — scaffolding tolerates lower quality, search hints need precision, prose can be creative.

### Tasks

#### Task 1: Define RoleConfig types

**File:** `crates/services/llm/src/role_config.rs` (new module)

```rust
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::provider::LlmRole;

/// Per-role LLM parameters. All fields are optional overrides;
/// `None` means "use the global default."
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub temperature_millis: Option<u16>,
    pub max_tokens: Option<u32>,
}

/// Complete role configuration map.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRoleConfigMap {
    #[serde(default)]
    pub scaffolding: RoleConfig,
    #[serde(default)]
    pub search_hints: RoleConfig,
    #[serde(default)]
    pub prose_rendering: RoleConfig,
    #[serde(default)]
    pub lean_scaffold: RoleConfig,
}
```

Provide a `Default` implementation that codifies what is currently scattered across call sites:

```rust
impl Default for LlmRoleConfigMap {
    fn default() -> Self {
        Self {
            scaffolding: RoleConfig {
                provider: None, model: None,
                temperature_millis: Some(100), max_tokens: Some(1024),
            },
            search_hints: RoleConfig {
                provider: None, model: None,
                temperature_millis: Some(100), max_tokens: Some(1024),
            },
            prose_rendering: RoleConfig {
                provider: None, model: None,
                temperature_millis: Some(200), max_tokens: Some(512),
            },
            lean_scaffold: RoleConfig {
                provider: None, model: None,
                temperature_millis: Some(200), max_tokens: Some(1024),
            },
        }
    }
}
```

**Tests:** `LlmRoleConfigMap::default()` has the expected values for each role.

---

#### Task 2: Load role config from environment variables

**File:** `crates/services/llm/src/role_config.rs`

```rust
impl LlmRoleConfigMap {
    /// Load role overrides from environment variables.
    /// Pattern: LLM_ROLE_{ROLE}_{PARAM}
    /// Example: LLM_ROLE_SCAFFOLDING_MODEL=gpt-4o-mini
    ///          LLM_ROLE_PROSE_RENDERING_TEMPERATURE=350
    ///          LLM_ROLE_SEARCH_HINTS_PROVIDER=anthropic
    pub fn from_env() -> Self {
        let mut config = Self::default();
        load_role_from_env("SCAFFOLDING", &mut config.scaffolding);
        load_role_from_env("SEARCH_HINTS", &mut config.search_hints);
        load_role_from_env("PROSE_RENDERING", &mut config.prose_rendering);
        load_role_from_env("LEAN_SCAFFOLD", &mut config.lean_scaffold);
        config
    }
}

fn load_role_from_env(role_name: &str, config: &mut RoleConfig) {
    if let Ok(v) = std::env::var(format!("LLM_ROLE_{role_name}_PROVIDER")) {
        config.provider = Some(v);
    }
    if let Ok(v) = std::env::var(format!("LLM_ROLE_{role_name}_MODEL")) {
        config.model = Some(v);
    }
    if let Ok(v) = std::env::var(format!("LLM_ROLE_{role_name}_TEMPERATURE")) {
        if let Ok(n) = v.parse::<u16>() {
            config.temperature_millis = Some(n);
        }
    }
    if let Ok(v) = std::env::var(format!("LLM_ROLE_{role_name}_MAX_TOKENS")) {
        if let Ok(n) = v.parse::<u32>() {
            config.max_tokens = Some(n);
        }
    }
}
```

**Tests:** Set env vars in test, call `from_env()`, verify overrides applied. Unset vars → defaults preserved.

---

#### Task 3: Load role config from audit.yaml

**File:** `crates/services/llm/src/role_config.rs`

```rust
impl LlmRoleConfigMap {
    /// Merge role config from audit.yaml's llm.roles section.
    /// YAML values override env values override defaults.
    pub fn merge_yaml(&mut self, yaml_roles: &HashMap<String, RoleConfig>) {
        if let Some(rc) = yaml_roles.get("scaffolding") { merge_role(&mut self.scaffolding, rc); }
        if let Some(rc) = yaml_roles.get("search_hints") { merge_role(&mut self.search_hints, rc); }
        if let Some(rc) = yaml_roles.get("prose_rendering") { merge_role(&mut self.prose_rendering, rc); }
        if let Some(rc) = yaml_roles.get("lean_scaffold") { merge_role(&mut self.lean_scaffold, rc); }
    }
}

fn merge_role(base: &mut RoleConfig, overlay: &RoleConfig) {
    if overlay.provider.is_some() { base.provider = overlay.provider.clone(); }
    if overlay.model.is_some() { base.model = overlay.model.clone(); }
    if overlay.temperature_millis.is_some() { base.temperature_millis = overlay.temperature_millis; }
    if overlay.max_tokens.is_some() { base.max_tokens = overlay.max_tokens; }
}
```

**File:** `crates/core/src/audit_config.rs`

Extend `LlmConfig`:

```rust
pub struct LlmConfig {
    pub no_llm_prose: bool,
    #[serde(default)]
    pub roles: HashMap<String, llm::RoleConfig>,  // Optional role overrides from audit.yaml
}
```

Note: `core` should not depend on the `llm` crate. Define a `RoleConfigOverride` struct in core with the same shape, and convert in the orchestrator. Or define the shared struct in core and import in llm.

**File:** `docs/audit-yaml-schema.json`

Add to the schema:

```json
"llm": {
  "type": "object",
  "properties": {
    "no_llm_prose": { "type": "boolean" },
    "roles": {
      "type": "object",
      "additionalProperties": {
        "type": "object",
        "properties": {
          "provider": { "type": "string" },
          "model": { "type": "string" },
          "temperature": { "type": "integer", "description": "Temperature in millis (100 = 0.1)" },
          "max_tokens": { "type": "integer" }
        }
      }
    }
  }
}
```

**Tests:** Parse audit.yaml with roles section. Verify merge produces correct values. Verify YAML overrides env overrides defaults.

---

#### Task 4: Build RoleAwareProvider

**File:** `crates/services/llm/src/role_config.rs`

```rust
use std::sync::Arc;
use crate::provider::{LlmProvider, CompletionOpts, LlmProvenance, LlmRole, llm_call_traced};

pub struct RoleAwareProvider {
    /// Named providers: "openai" → Arc<OpenAiProvider>, etc.
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    role_configs: LlmRoleConfigMap,
    default_provider: Arc<dyn LlmProvider>,
}

impl RoleAwareProvider {
    pub fn new(
        providers: HashMap<String, Arc<dyn LlmProvider>>,
        role_configs: LlmRoleConfigMap,
        default_provider: Arc<dyn LlmProvider>,
    ) -> Self {
        Self { providers, role_configs, default_provider }
    }

    /// Select the provider and opts for a given role.
    fn resolve_for_role(&self, role: &LlmRole) -> (Arc<dyn LlmProvider>, CompletionOpts) {
        let rc = match role {
            LlmRole::Scaffolding => &self.role_configs.scaffolding,
            LlmRole::SearchHints => &self.role_configs.search_hints,
            LlmRole::ProseRendering => &self.role_configs.prose_rendering,
            LlmRole::LeanScaffold => &self.role_configs.lean_scaffold,
        };

        let provider = rc.provider.as_ref()
            .and_then(|name| self.providers.get(name))
            .cloned()
            .unwrap_or_else(|| self.default_provider.clone());

        let opts = CompletionOpts {
            temperature_millis: rc.temperature_millis.unwrap_or(100),
            max_tokens: rc.max_tokens.unwrap_or(1024),
        };

        (provider, opts)
    }

    /// Complete with role-aware dispatch. Returns provenance for tracing.
    pub async fn complete_for_role(
        &self,
        role: LlmRole,
        prompt: &str,
    ) -> anyhow::Result<(String, LlmProvenance)> {
        let (provider, opts) = self.resolve_for_role(&role);
        llm_call_traced(&*provider, role, prompt, &opts).await
    }
}
```

Implement `LlmProvider` for `RoleAwareProvider` for compatibility, but do **not** treat that compatibility trait path as sufficient for role-aware routing:

```rust
#[async_trait]
impl LlmProvider for RoleAwareProvider {
    async fn complete(&self, prompt: &str, opts: &CompletionOpts) -> anyhow::Result<String> {
        // Generic trait calls have no role context, so they preserve existing
        // behavior by using the default provider only.
        self.default_provider.complete(prompt, opts).await
    }
    fn name(&self) -> &str { "role-aware" }
    fn is_available(&self) -> bool { self.default_provider.is_available() }
    fn model(&self) -> Option<&str> { self.default_provider.model() }
}
```

This keeps backward compatibility, but the plan is not complete until role-bearing call sites migrate to a role-aware execution helper.

**Tests:** Create `RoleAwareProvider` with two mock providers. Verify Scaffolding dispatches to provider A, SearchHints dispatches to provider B. Verify `CompletionOpts` match role config. Verify the generic trait path still uses the default provider when no role context exists.

---

#### Task 5: Factory function for RoleAwareProvider

**File:** `crates/services/llm/src/role_config.rs`

```rust
pub fn role_aware_provider_from_env() -> RoleAwareProvider {
    use crate::provider::{openai_provider, anthropic_provider, ollama_provider, TemplateFallback};

    let mut providers = HashMap::new();
    let mut default: Arc<dyn LlmProvider> = Arc::new(TemplateFallback);

    if let Some(p) = openai_provider() {
        default = Arc::new(p.clone());
        providers.insert("openai".to_string(), Arc::new(p) as Arc<dyn LlmProvider>);
    }
    if let Some(p) = anthropic_provider() {
        if !providers.contains_key("openai") { default = Arc::new(p.clone()); }
        providers.insert("anthropic".to_string(), Arc::new(p) as Arc<dyn LlmProvider>);
    }
    if let Some(p) = ollama_provider() {
        if providers.is_empty() { default = Arc::new(p.clone()); }
        providers.insert("ollama".to_string(), Arc::new(p) as Arc<dyn LlmProvider>);
    }
    providers.insert("template-fallback".to_string(), Arc::new(TemplateFallback) as Arc<dyn LlmProvider>);

    let role_configs = LlmRoleConfigMap::from_env();
    RoleAwareProvider::new(providers, role_configs, default)
}
```

Note: The existing `openai_provider()`, `anthropic_provider()`, `ollama_provider()` functions in `provider.rs` are currently private. Make them `pub(crate)` so `role_config.rs` can use them.

---

#### Task 5A: Introduce a role-aware execution helper and migrate all role-bearing call sites

**Files:** `crates/services/llm/src/lib.rs`, `crates/services/llm/src/provider.rs`, every role-bearing caller listed below

Add a single supported entry point for role-aware execution:

```rust
pub async fn role_aware_llm_call(
    provider: &RoleAwareProvider,
    role: LlmRole,
    prompt: &str,
) -> anyhow::Result<(String, LlmProvenance)> {
    provider.complete_for_role(role, prompt).await
}
```

Then migrate every role-bearing call site in this stage so role configuration is actually exercised. Compatibility through `impl LlmProvider for RoleAwareProvider` is preserved for generic callers only; it is not sufficient to satisfy Item A on its own.

Target migration list:
- `crates/services/llm/src/copilot.rs`
- `crates/services/llm/src/evidence_gate.rs`
- `crates/engines/crypto/src/kani/scaffolder.rs`
- `crates/engines/distributed/src/harness/builder.rs`
- `crates/engines/distributed/src/economic/mod.rs`
- `crates/engines/lean/src/scaffold.rs`
- `crates/services/report/src/generator.rs`

Release criterion for this task: role-bearing calls no longer bypass role dispatch.

---

#### Task 6: Integrate RoleAwareProvider in CLI

**File:** `crates/apps/cli/src/lib.rs`

Replace:
```rust
let llm = Arc::<dyn LlmProvider>::from(provider_from_env());
```

With:
```rust
let llm = Arc::new(role_aware_provider_from_env());
```

This switches construction to the new provider type, but the value of Item A is only delivered once Task 5A migrates role-bearing call sites to the role-aware helper. Generic `LlmProvider::complete()` remains compatibility-only and should be treated as fallback behavior, not the main routing path.

---

#### Task 7: Remove scattered CompletionOpts overrides

Once `RoleAwareProvider` is active and role configs have the correct defaults:

| File | Current override | Action |
|------|-----------------|--------|
| `crates/engines/distributed/src/economic/mod.rs:193` | `CompletionOpts { temperature_millis: 200, max_tokens: 256 }` | Remove; `prose_rendering` role config has `200/512`. Adjust `prose_rendering.max_tokens` default to `256` if economic needs it, or add a new role |
| `crates/services/report/src/generator.rs:271` | `CompletionOpts { temperature_millis: 200, max_tokens: 512 }` | Remove; matches `prose_rendering` default |
| `crates/engines/lean/src/scaffold.rs:29` | `CompletionOpts { temperature_millis: 200, max_tokens: 1024 }` | Remove; matches `lean_scaffold` default |

If the economic module needs different tokens than prose rendering, consider whether it should be a separate role or whether `max_tokens: 512` is acceptable for both. The conservative approach: keep the override for now and add `LlmRole::EconomicDescription` later when the eval harness (Item B) can measure the difference.

---

#### Task 8: Export and wire up

**File:** `crates/services/llm/src/lib.rs`

Add:
```rust
pub mod role_config;
pub use role_config::{LlmRoleConfigMap, RoleAwareProvider, RoleConfig, role_aware_provider_from_env};
```

---

## Item B: Provider Evaluation Fixtures and Regression Benchmarks

### Context

There is no way to measure whether LLM configuration changes improve or regress output quality. If you change the scaffolding model from `gpt-4o-mini` to `claude-3-5-sonnet`, you cannot tell whether the Kani harnesses are better or worse without manually inspecting them. The eval harness closes this gap.

### Tasks

#### Task 9: Create the llm-eval crate

**File:** `crates/services/llm-eval/Cargo.toml`

```toml
[package]
name = "llm-eval"
version = "0.1.0"
edition = "2024"

[dependencies]
llm = { path = "../llm" }
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
chrono = { version = "0.4", features = ["serde"] }
```

**File:** `Cargo.toml` (workspace)

Add `"crates/services/llm-eval"` to workspace members.

---

#### Task 10: Define fixture and result types

**File:** `crates/services/llm-eval/src/lib.rs`

```rust
pub mod fixture;
pub mod runner;
pub mod reporter;

pub use fixture::{EvalFixture, EvalAssertion};
pub use runner::{EvalRunner, EvalResult};
pub use reporter::MarkdownReporter;
```

**File:** `crates/services/llm-eval/src/fixture.rs`

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalFixture {
    pub id: String,
    pub role: String,  // "Scaffolding", "SearchHints", "ProseRendering", "LeanScaffold"
    pub prompt: String,
    pub assertions: Vec<EvalAssertion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EvalAssertion {
    /// Response is valid JSON.
    JsonValid,
    /// Response contains the keyword (case-insensitive).
    ContainsKeyword { value: String },
    /// Response does NOT contain the keyword (case-insensitive).
    NotContainsKeyword { value: String },
    /// Response parses as the named JSON contract.
    ParsesAsContract { contract: String },
    /// A JSON field at the given path is present and non-empty.
    FieldNotEmpty { path: String },
    /// Response is at most N characters.
    MaxChars { value: usize },
    /// Response is at least N characters.
    MinChars { value: usize },
}
```

---

#### Task 11: Implement the eval runner

**File:** `crates/services/llm-eval/src/runner.rs`

```rust
use std::sync::Arc;
use std::time::Instant;
use anyhow::Result;
use llm::provider::{CompletionOpts, LlmProvenance, LlmProvider, LlmRole, llm_call_traced};
use crate::fixture::{EvalFixture, EvalAssertion};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    pub fixture_id: String,
    pub passed: bool,
    pub assertions_passed: usize,
    pub assertions_total: usize,
    pub duration_ms: u64,
    pub provider: String,
    pub model: Option<String>,
    pub failure_reasons: Vec<String>,
}

pub struct EvalRunner {
    provider: Arc<dyn LlmProvider>,
}

impl EvalRunner {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self { provider }
    }

    pub async fn run_fixture(&self, fixture: &EvalFixture) -> EvalResult {
        let role = parse_role(&fixture.role);
        let start = Instant::now();

        let response = match llm_call_traced(
            &*self.provider,
            role,
            &fixture.prompt,
            &CompletionOpts::default(),
        ).await {
            Ok((text, _provenance)) => text,
            Err(err) => {
                return EvalResult {
                    fixture_id: fixture.id.clone(),
                    passed: false,
                    assertions_passed: 0,
                    assertions_total: fixture.assertions.len(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    provider: self.provider.name().to_string(),
                    model: self.provider.model().map(String::from),
                    failure_reasons: vec![format!("LLM call failed: {err}")],
                };
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;
        let mut passed_count = 0usize;
        let mut failures = Vec::new();

        for assertion in &fixture.assertions {
            match check_assertion(assertion, &response) {
                Ok(()) => passed_count += 1,
                Err(reason) => failures.push(reason),
            }
        }

        EvalResult {
            fixture_id: fixture.id.clone(),
            passed: failures.is_empty(),
            assertions_passed: passed_count,
            assertions_total: fixture.assertions.len(),
            duration_ms,
            provider: self.provider.name().to_string(),
            model: self.provider.model().map(String::from),
            failure_reasons: failures,
        }
    }

    pub async fn run_all(&self, fixtures: &[EvalFixture]) -> Vec<EvalResult> {
        let mut results = Vec::new();
        for fixture in fixtures {
            results.push(self.run_fixture(fixture).await);
        }
        results
    }
}

fn check_assertion(assertion: &EvalAssertion, response: &str) -> Result<(), String> {
    match assertion {
        EvalAssertion::JsonValid => {
            serde_json::from_str::<serde_json::Value>(response.trim())
                .map(|_| ())
                .map_err(|e| format!("JsonValid failed: {e}"))
        }
        EvalAssertion::ContainsKeyword { value } => {
            if response.to_lowercase().contains(&value.to_lowercase()) {
                Ok(())
            } else {
                Err(format!("ContainsKeyword '{value}' not found"))
            }
        }
        EvalAssertion::NotContainsKeyword { value } => {
            if response.to_lowercase().contains(&value.to_lowercase()) {
                Err(format!("NotContainsKeyword '{value}' was found"))
            } else {
                Ok(())
            }
        }
        EvalAssertion::ParsesAsContract { contract } => {
            // Use parse_json_contract with a generic Value check + contract name logging
            let trimmed = response.trim();
            serde_json::from_str::<serde_json::Value>(trimmed)
                .map(|_| ())
                .map_err(|e| format!("ParsesAsContract '{contract}' failed: {e}"))
        }
        EvalAssertion::FieldNotEmpty { path } => {
            let value: serde_json::Value = serde_json::from_str(response.trim())
                .map_err(|e| format!("FieldNotEmpty '{path}' — JSON parse failed: {e}"))?;
            let field = value.pointer(path)
                .ok_or_else(|| format!("FieldNotEmpty '{path}' — field not found"))?;
            match field {
                serde_json::Value::String(s) if s.trim().is_empty() => {
                    Err(format!("FieldNotEmpty '{path}' — field is empty string"))
                }
                serde_json::Value::Array(a) if a.is_empty() => {
                    Err(format!("FieldNotEmpty '{path}' — field is empty array"))
                }
                serde_json::Value::Null => {
                    Err(format!("FieldNotEmpty '{path}' — field is null"))
                }
                _ => Ok(()),
            }
        }
        EvalAssertion::MaxChars { value } => {
            if response.len() <= *value {
                Ok(())
            } else {
                Err(format!("MaxChars {value} exceeded: got {}", response.len()))
            }
        }
        EvalAssertion::MinChars { value } => {
            if response.len() >= *value {
                Ok(())
            } else {
                Err(format!("MinChars {value} not met: got {}", response.len()))
            }
        }
    }
}

fn parse_role(role: &str) -> LlmRole {
    match role.to_lowercase().as_str() {
        "scaffolding" => LlmRole::Scaffolding,
        "searchhints" | "search_hints" => LlmRole::SearchHints,
        "proserendering" | "prose_rendering" => LlmRole::ProseRendering,
        "leanscaffold" | "lean_scaffold" => LlmRole::LeanScaffold,
        _ => LlmRole::Scaffolding,
    }
}
```

**Tests:** Unit test with mock provider returning known strings. Verify each assertion type passes/fails correctly.

---

#### Task 12: Create initial fixture files

**File:** `crates/services/llm-eval/fixtures/scaffolding.yaml`

```yaml
- id: scaffolding-field-mul
  role: Scaffolding
  prompt: |
    Generate a Kani harness for fn field_mul(a: u64, b: u64) -> u64 that verifies
    the output equals a.wrapping_mul(b). Return compilable Rust source code only.
  assertions:
    - type: ContainsKeyword
      value: "fn harness"
    - type: ContainsKeyword
      value: "kani::assert"
    - type: ContainsKeyword
      value: "wrapping_mul"
    - type: MinChars
      value: 50

- id: scaffolding-field-add
  role: Scaffolding
  prompt: |
    Generate a Kani harness for fn field_add(a: u64, b: u64) -> u64 that verifies
    the output equals a.wrapping_add(b). Return compilable Rust source code only.
  assertions:
    - type: ContainsKeyword
      value: "fn harness"
    - type: ContainsKeyword
      value: "wrapping_add"
    - type: MinChars
      value: 50

- id: scaffolding-verify-proof
  role: Scaffolding
  prompt: |
    Generate a Kani harness for fn verify_proof(bytes: &[u8]) -> bool.
    Return compilable Rust source code only.
  assertions:
    - type: ContainsKeyword
      value: "fn harness"
    - type: MinChars
      value: 30
```

**File:** `crates/services/llm-eval/fixtures/search_hints.yaml`

```yaml
- id: search-hints-checklist-plan
  role: SearchHints
  prompt: |
    Return only valid JSON for contract `ChecklistPlan`.
    Do not include markdown, prose, or code fences.
    Task:
    Select applicable audit domains for this workspace:
    Crates: threshold-bls, pairing-crypto, bls-signatures
    Frameworks: halo2
    Dependencies: ark-bls12-381, ark-ec
  assertions:
    - type: JsonValid
    - type: FieldNotEmpty
      path: "/domains"

- id: search-hints-architecture-overview
  role: SearchHints
  prompt: |
    Return only valid JSON for contract `ArchitectureOverview`.
    Do not include markdown, prose, or code fences.
    Task:
    Generate architecture overview fields for:
    A BLS threshold signature library with pairing-based cryptography.
    Key operations: keygen, sign, aggregate, verify.
  assertions:
    - type: JsonValid
    - type: FieldNotEmpty
      path: "/assets"
    - type: FieldNotEmpty
      path: "/hotspots"

- id: search-hints-candidate-draft
  role: SearchHints
  prompt: |
    Return only valid JSON for contract `CandidateDraft`.
    Do not include markdown, prose, or code fences.
    Task:
    Generate a concise candidate for hotspot:
    fn aggregate_signatures(sigs: &[Signature]) -> Result<Signature>
    No length check on input array. Potential DoS with empty input.
  assertions:
    - type: JsonValid
    - type: FieldNotEmpty
      path: "/title"
    - type: FieldNotEmpty
      path: "/summary"
```

**File:** `crates/services/llm-eval/fixtures/prose_rendering.yaml`

```yaml
- id: prose-improve-finding
  role: ProseRendering
  prompt: |
    Improve the following finding description for clarity and technical precision.
    Keep it under 400 characters.
    Original: "The nonce is reused across multiple signatures which allows
    an attacker to recover the private key via linear algebra on the signature equations."
  assertions:
    - type: ContainsKeyword
      value: "nonce"
    - type: ContainsKeyword
      value: "private key"
    - type: MaxChars
      value: 600

- id: prose-executive-summary
  role: ProseRendering
  prompt: |
    Write a one-paragraph executive summary for an audit that found:
    2 Critical (nonce reuse, missing domain separation),
    1 High (predictable RNG seed),
    3 Medium findings.
    Target audience: non-technical stakeholders.
  assertions:
    - type: ContainsKeyword
      value: "critical"
    - type: MinChars
      value: 100
    - type: MaxChars
      value: 1500

- id: prose-recommendation
  role: ProseRendering
  prompt: |
    Write a remediation recommendation for: BLS signature aggregation
    does not validate that all signatures are on the same message.
    Keep it actionable and under 200 words.
  assertions:
    - type: ContainsKeyword
      value: "message"
    - type: MinChars
      value: 50
```

---

#### Task 13: Implement markdown reporter

**File:** `crates/services/llm-eval/src/reporter.rs`

```rust
use crate::runner::EvalResult;

pub struct MarkdownReporter;

impl MarkdownReporter {
    /// Generate a markdown report from eval results.
    pub fn generate(results: &[EvalResult], baseline: Option<&[EvalResult]>) -> String {
        let mut md = String::new();
        md.push_str("# LLM Evaluation Report\n\n");
        md.push_str(&format!("**Provider:** {}\n", results.first().map(|r| r.provider.as_str()).unwrap_or("unknown")));
        md.push_str(&format!("**Model:** {}\n\n", results.first().and_then(|r| r.model.as_deref()).unwrap_or("unknown")));

        // Summary
        let total = results.len();
        let passed = results.iter().filter(|r| r.passed).count();
        md.push_str(&format!("**Results:** {passed}/{total} fixtures passed\n\n"));

        // Detail table
        md.push_str("| Fixture | Status | Assertions | Duration | Regressions |\n");
        md.push_str("|---------|--------|------------|----------|-------------|\n");
        for result in results {
            let status = if result.passed { "PASS" } else { "FAIL" };
            let assertions = format!("{}/{}", result.assertions_passed, result.assertions_total);
            let duration = format!("{}ms", result.duration_ms);
            let regression = baseline.map(|b| {
                b.iter().find(|br| br.fixture_id == result.fixture_id).map(|br| {
                    if br.passed && !result.passed { "REGRESSED" }
                    else if !br.passed && result.passed { "FIXED" }
                    else { "-" }
                }).unwrap_or("NEW")
            }).unwrap_or("-");
            md.push_str(&format!("| {} | {} | {} | {} | {} |\n",
                result.fixture_id, status, assertions, duration, regression));
        }

        // Failure details
        let failures: Vec<_> = results.iter().filter(|r| !r.passed).collect();
        if !failures.is_empty() {
            md.push_str("\n## Failures\n\n");
            for result in failures {
                md.push_str(&format!("### {}\n\n", result.fixture_id));
                for reason in &result.failure_reasons {
                    md.push_str(&format!("- {reason}\n"));
                }
                md.push_str("\n");
            }
        }

        md
    }
}
```

---

#### Task 14: Add CLI subcommand

**File:** `crates/apps/cli/src/lib.rs`

Add a new subcommand to the existing clap `Cli` struct:

```rust
#[derive(clap::Subcommand)]
enum Commands {
    Analyze { /* existing */ },
    Diff { /* existing */ },
    Eval {
        /// Provider to evaluate (default: from env)
        #[arg(long)]
        provider: Option<String>,
        /// Path to save baseline results
        #[arg(long)]
        baseline: Option<PathBuf>,
        /// Path to baseline to compare against
        #[arg(long)]
        compare: Option<PathBuf>,
        /// Fixture directory (default: built-in)
        #[arg(long)]
        fixtures: Option<PathBuf>,
    },
}
```

Implementation:
1. Load fixtures from YAML files (built-in or custom path).
2. Instantiate provider (from `--provider` flag or `provider_from_env()`).
3. Run all fixtures via `EvalRunner`.
4. If `--baseline` specified, save results as JSON.
5. If `--compare` specified, load baseline and generate comparative report.
6. Print markdown report to stdout.
7. Exit with code 1 if any fixture failed or regressed.

---

#### Task 15: Template fallback baseline

Run the eval suite against `TemplateFallback` and save as the floor baseline:

```bash
LLM_PROVIDER=template cargo run -p audit-agent -- eval --baseline baselines/template-fallback.json
```

All real providers must score above this baseline. Supported `TemplateFallback` roles should pass their required fixtures. Unsupported roles should be marked skipped in the fixture metadata rather than counted as failures.

**Test:** Automated test that runs eval against `TemplateFallback`, verifies supported-role fixtures pass, and verifies unsupported roles are reported as skipped.

---

## Dependency Map

```
Task 1  (types)              ← no deps
Task 2  (env loading)        ← Task 1
Task 3  (yaml loading)       ← Task 1
Task 4  (RoleAwareProvider)  ← Task 1, Task 2
Task 5  (factory)            ← Task 4
Task 5A (helper migration)   ← Task 4, Task 5
Task 6  (CLI integration)    ← Task 5
Task 7  (remove overrides)   ← Task 5A, Task 6
Task 8  (exports)            ← Task 4

Task 9  (eval crate)         ← no deps
Task 10 (types)              ← Task 9
Task 11 (runner)             ← Task 10
Task 12 (fixtures)           ← Task 10
Task 13 (reporter)           ← Task 11
Task 14 (CLI subcommand)     ← Task 11, Task 12, Task 13
Task 15 (baseline)           ← Task 14
```

Items A and B are independent and can be developed in parallel.
