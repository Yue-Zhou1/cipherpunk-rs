# Plan 1 — T0: Global Reliability

> **For Agent:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the audit pipeline resilient to partial failure and make every LLM interaction validated, retryable, and traceable. These two items are the foundation that every subsequent tier depends on.

**Architecture:** Two parallel workstreams that converge on the session event model. Item A changes how the orchestrator handles engine errors and how the manifest reports coverage. Item B introduces a shared enforcement layer for all LLM calls. Both emit structured events that feed into the observability layer (Plan 4).

**Tech Stack:** Rust workspace crates, serde, tokio, tracing, SQLite session store.

---

## Planning Conventions

- Recommended execution context: a dedicated git worktree.
- Preserve existing CLI, Tauri, and HTTP workflows while adding new behavior.
- Prefer additive schema changes with `#[serde(default)]` so existing sessions and JSON payloads remain readable.
- Every task must land with tests and updated fixtures/docs where applicable.
- Deterministic engines and ProjectIR remain the source of truth for audit analysis; AI layers may assist planning, explanation, and recovery, but do not redefine deterministic results.

## Release Gates

This plan is complete only when all of the following are true:

- A single engine failure does not abort the entire audit.
- `AuditManifest` contains per-engine outcome data (completed/failed/skipped with reason and duration).
- `AuditManifest` exposes a separate coverage/completeness indicator and warning set when engine coverage is incomplete.
- Risk score remains findings-derived; incomplete coverage reduces confidence, not the raw severity-derived score.
- Every `llm_call` site uses the unified enforcement layer with provenance capture.
- LLM contract failures retry according to per-role policy before falling back or erroring.
- LLM provenance (provider, model, role, duration, attempt) is recorded in session events.
- `cargo test` and `cargo test -p orchestrator` and `cargo test -p llm` are all green.

## Explicit Non-Goals

- Adviser/reflector logic (Plan 5B) — this plan only handles mechanical retry and graceful failure.
- Runtime provider failover (Plan 5A) — this plan uses the single configured provider; failover comes later.
- UI rendering of coverage warnings beyond passing them through existing `review_notes` — deeper UI work is Plan 2.

---

## Item A: Graceful Engine Degradation with Coverage Reporting

### Context

`execute_dag()` in `crates/apps/orchestrator/src/lib.rs:260-277` iterates engines sequentially and propagates errors via `?`, aborting the entire audit on the first failure. `AuditManifest.engines_run` is a flat `Vec<String>` with no per-engine status. Today the manifest cannot distinguish "no findings with full coverage" from "no findings because an engine crashed," so completeness needs to be surfaced separately from finding severity.

### Tasks

#### Task 1: Add engine outcome and coverage types to core

**File:** `crates/core/src/output.rs`

Add the following types alongside the existing `FindingCounts`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EngineOutcome {
    pub engine: String,
    pub status: EngineStatus,
    pub findings_count: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum EngineStatus {
    Completed,
    Failed { reason: String },
    Skipped { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CoverageReport {
    pub engines_requested: usize,
    pub engines_completed: usize,
    pub engines_failed: usize,
    pub engines_skipped: usize,
    pub coverage_complete: bool,
    pub warnings: Vec<String>,
}

impl CoverageReport {
    pub fn from_outcomes(outcomes: &[EngineOutcome]) -> Self {
        let completed = outcomes.iter().filter(|o| matches!(o.status, EngineStatus::Completed)).count();
        let failed = outcomes.iter().filter(|o| matches!(o.status, EngineStatus::Failed { .. })).count();
        let skipped = outcomes.iter().filter(|o| matches!(o.status, EngineStatus::Skipped { .. })).count();
        let mut warnings = Vec::new();
        for outcome in outcomes {
            if let EngineStatus::Failed { reason } = &outcome.status {
                warnings.push(format!("Engine '{}' failed: {}", outcome.engine, reason));
            }
            if let EngineStatus::Skipped { reason } = &outcome.status {
                warnings.push(format!("Engine '{}' skipped: {}", outcome.engine, reason));
            }
        }
        Self {
            engines_requested: outcomes.len(),
            engines_completed: completed,
            engines_failed: failed,
            engines_skipped: skipped,
            coverage_complete: failed == 0 && skipped == 0,
            warnings,
        }
    }
}
```

Add new fields to `AuditManifest` (with `#[serde(default)]` for backward compat):

```rust
pub struct AuditManifest {
    // ... existing fields ...
    #[serde(default)]
    pub engine_outcomes: Vec<EngineOutcome>,
    #[serde(default)]
    pub coverage: Option<CoverageReport>,
}
```

Keep `engines_run` for backward compatibility; derive it from `engine_outcomes` in `produce_outputs()`.

**Tests:** Unit test `CoverageReport::from_outcomes` with mixed statuses. Verify `coverage_complete` is false when any engine failed.

---

#### Task 2: Add separate coverage confidence/completeness reporting

**File:** `crates/core/src/output.rs`

Do **not** add a coverage-aware `risk_score_with_coverage()` helper. Keep `FindingCounts::risk_score()` findings-derived and introduce a separate confidence/completeness signal on the coverage side instead:

```rust
impl CoverageReport {
    /// Percentage of requested engines that completed successfully.
    pub fn confidence_percent(&self) -> u8 {
        if self.engines_requested == 0 {
            return 0;
        }
        ((self.engines_completed * 100) / self.engines_requested) as u8
    }
}
```

Use this in reports/UI as a separate signal:
- `Risk Score` remains `finding_counts.risk_score()`.
- `Coverage Status` / `Confidence` comes from `CoverageReport`.

**Tests:** `confidence_percent()` returns `100` for full coverage, drops when engines fail/skip, and does not change `FindingCounts::risk_score()`.

---

#### Task 3: Rewrite execute_dag to catch per-engine errors

**File:** `crates/apps/orchestrator/src/lib.rs`

Change the signature:

```rust
pub async fn execute_dag(
    &self,
    _dag: &AuditDag,
    config: &AuditConfig,
) -> Result<(Vec<Finding>, Vec<EngineOutcome>)>
```

Rewrite the engine loop:

```rust
let mut findings = Vec::<Finding>::new();
let mut outcomes = Vec::<EngineOutcome>::new();

for engine in &self.engines {
    let engine_name = engine.name().to_string();
    let start = std::time::Instant::now();

    if !engine.supports(&ctx).await {
        outcomes.push(EngineOutcome {
            engine: engine_name,
            status: EngineStatus::Skipped {
                reason: "engine reported unsupported for this context".to_string(),
            },
            findings_count: 0,
            duration_ms: start.elapsed().as_millis() as u64,
        });
        continue;
    }

    match engine.analyze(&ctx).await {
        Ok(engine_findings) => {
            let count = engine_findings.len();
            findings.extend(engine_findings);
            if let Some(sink) = &self.event_sink {
                sink.emit(AuditEvent::EngineCompleted {
                    engine: engine_name.clone(),
                    findings_count: count,
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
            outcomes.push(EngineOutcome {
                engine: engine_name,
                status: EngineStatus::Completed,
                findings_count: count,
                duration_ms: start.elapsed().as_millis() as u64,
            });
        }
        Err(err) => {
            tracing::error!(engine = %engine_name, error = %err, "engine failed — continuing with remaining engines");
            if let Some(sink) = &self.event_sink {
                sink.emit(AuditEvent::EngineFailed {
                    engine: engine_name.clone(),
                    reason: err.to_string(),
                });
            }
            outcomes.push(EngineOutcome {
                engine: engine_name,
                status: EngineStatus::Failed { reason: err.to_string() },
                findings_count: 0,
                duration_ms: start.elapsed().as_millis() as u64,
            });
        }
    }
}

Ok((findings, outcomes))
```

Update `run()` to pass outcomes through to `produce_outputs()`.

**Tests:** Orchestrator integration test with a `PanicEngine` (always returns `Err`) alongside a `StaticEngine`. Verify: both engines attempted, findings from StaticEngine preserved, outcomes contain one Failed + one Completed.

---

#### Task 4: Add engine lifecycle events

**File:** `crates/apps/orchestrator/src/events.rs`

Extend `AuditEvent`:

```rust
pub enum AuditEvent {
    EngineCompleted {
        engine: String,
        findings_count: usize,
        duration_ms: u64,
    },
    EngineFailed {
        engine: String,
        reason: String,
    },
    AuditCompleted {
        audit_id: String,
        output_dir: PathBuf,
        finding_count: usize,
    },
}
```

These events are emitted from `execute_dag()` (Task 3) and will be consumed by the session event store and WebSocket stream.

---

#### Task 5: Propagate outcomes through produce_outputs and manifest

**File:** `crates/apps/orchestrator/src/lib.rs`

Update `produce_outputs` signature:

```rust
pub async fn produce_outputs(
    &self,
    findings: &[Finding],
    outcomes: &[EngineOutcome],
    config: &AuditConfig,
) -> Result<AuditOutputs>
```

Inside `produce_outputs`:
1. Build `CoverageReport::from_outcomes(outcomes)`.
2. Keep `finding_counts.risk_score()` unchanged and surface coverage separately via `manifest.coverage`.
3. Populate `manifest.engine_outcomes = outcomes.to_vec()`.
4. Populate `manifest.coverage = Some(coverage)`.
5. Derive `manifest.engines_run` from `outcomes.iter().map(|o| o.engine.clone()).collect()` for backward compat.

---

#### Task 6: Add per-engine jobs to bootstrap_jobs

**File:** `crates/apps/orchestrator/src/jobs.rs`

Add `AuditJobKind::RunEngine { engine_name: String }` variant.

**File:** `crates/apps/orchestrator/src/lib.rs`

In `bootstrap_jobs()`, add one `RunEngine` job per engine that the session's config enables. This gives the UI per-engine status tracking in the activity console before domain checklist jobs.

---

#### Task 7: Surface coverage warnings in security overview and reports

**File:** `crates/services/session-manager/src/state.rs`

In `load_security_overview()`, when the manifest has a non-empty `coverage.warnings`, prepend them to `review_notes` with a `[COVERAGE]` prefix so the UI can distinguish them.

**File:** `crates/services/report/src/generator.rs`

In executive report generation, when `coverage.coverage_complete == false`:
- Add a "Coverage" section before findings.
- List each failed/skipped engine with reason.
- Bold warning: "**This report reflects partial analysis. The following engines did not complete successfully.**"

**Tests:** Generate report with incomplete coverage. Verify coverage section present in markdown output.

---

## Item B: Unified LLM Contract Enforcement, Retry/Repair, and Provenance

### Context

There are 7 `llm_call` sites across the codebase. `parse_json_contract()` in `crates/services/llm/src/sanitize.rs:87-106` does basic JSON parsing with code-fence stripping but no retry. `fix_syntax_and_retry()` in `crates/services/llm/src/evidence_gate.rs:149-221` has retry logic but only for harness compilation. There are two separate `LlmRole` enums — one in core (`MechanicalScaffolding`, `SearchSpaceGuidance`, `ProseRendering`) and one in the llm crate (`Scaffolding`, `SearchHints`, `ProseRendering`, `LeanScaffold`). No provenance is tracked. All session-bound LLM executions must carry `session_id` into the enforcement layer so provenance is not dropped for orchestrated calls.

### Tasks

#### Task 8: Add model() to LlmProvider trait

**File:** `crates/services/llm/src/provider.rs`

Add a default method to the trait:

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, prompt: &str, opts: &CompletionOpts) -> Result<String>;
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
    fn model(&self) -> Option<&str> { None }
}
```

Implement on each provider:
- `OpenAiProvider::model()` → `Some(&self.model)`
- `AnthropicProvider::model()` → `Some(&self.model)`
- `OllamaProvider::model()` → `Some(&self.model)`
- `TemplateFallback::model()` → `None`

---

#### Task 9: Define LlmProvenance and update llm_call

**File:** `crates/services/llm/src/provider.rs`

Add provenance type:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProvenance {
    pub provider: String,
    pub model: Option<String>,
    pub role: String,
    pub duration_ms: u64,
    pub prompt_chars: usize,
    pub response_chars: usize,
    pub attempt: u8,
}
```

**Migration strategy** — avoid a big-bang signature change:

1. Add `llm_call_traced()` as a new function that returns `Result<(String, LlmProvenance)>`:

```rust
pub async fn llm_call_traced(
    provider: &dyn LlmProvider,
    role: LlmRole,
    prompt: &str,
    opts: &CompletionOpts,
) -> Result<(String, LlmProvenance)> {
    let start = std::time::Instant::now();
    tracing::debug!(role = ?role, provider = provider.name(), "LLM call");
    let response = provider.complete(prompt, opts).await?;
    let duration = start.elapsed();
    tracing::info!(
        role = ?role,
        provider = provider.name(),
        model = provider.model().unwrap_or("unknown"),
        duration_ms = duration.as_millis() as u64,
        response_chars = response.len(),
        "LLM call completed"
    );
    let provenance = LlmProvenance {
        provider: provider.name().to_string(),
        model: provider.model().map(String::from),
        role: format!("{:?}", role),
        duration_ms: duration.as_millis() as u64,
        prompt_chars: prompt.len(),
        response_chars: response.len(),
        attempt: 1,
    };
    Ok((response, provenance))
}
```

2. Rewrite existing `llm_call()` to delegate to `llm_call_traced()` and discard provenance, so existing call sites compile unchanged:

```rust
pub async fn llm_call(
    provider: &dyn LlmProvider,
    role: LlmRole,
    prompt: &str,
    opts: &CompletionOpts,
) -> Result<String> {
    let (response, _provenance) = llm_call_traced(provider, role, prompt, opts).await?;
    Ok(response)
}
```

3. Migrate call sites to `llm_call_traced()` one at a time in subsequent tasks.

---

#### Task 10: Create the enforcement module

**File:** `crates/services/llm/src/enforcement.rs` (new)

```rust
use std::marker::PhantomData;
use anyhow::Result;
use serde::de::DeserializeOwned;
use crate::provider::{CompletionOpts, LlmProvenance, LlmProvider, LlmRole, llm_call_traced};
use crate::sanitize::parse_json_contract;

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u8,
    pub backoff_ms: u64,
}

#[derive(Debug, Clone)]
pub struct EnforcedResponse<T> {
    pub value: T,
    pub provenance: LlmProvenance,
}

pub struct ContractEnforcer<T: DeserializeOwned> {
    role: LlmRole,
    contract_name: String,
    retry_policy: RetryPolicy,
    fallback: Option<T>,
    _phantom: PhantomData<T>,
}
```

Implement the core enforcement loop:

```rust
impl<T: DeserializeOwned> ContractEnforcer<T> {
    pub fn new(role: LlmRole, contract_name: &str) -> Self { /* ... */ }
    pub fn with_retry(mut self, policy: RetryPolicy) -> Self { /* ... */ }
    pub fn with_fallback(mut self, fallback: T) -> Self { /* ... */ }

    pub async fn execute(
        &self,
        provider: &dyn LlmProvider,
        task_description: &str,
        opts: &CompletionOpts,
    ) -> Result<EnforcedResponse<T>> {
        let prompt = crate::provider::json_only_prompt(&self.contract_name, task_description);

        for attempt in 1..=self.retry_policy.max_attempts {
            match llm_call_traced(provider, self.role.clone(), &prompt, opts).await {
                Ok((response, mut provenance)) => {
                    provenance.attempt = attempt;
                    match parse_json_contract::<T>(&response) {
                        Ok(value) => return Ok(EnforcedResponse { value, provenance }),
                        Err(parse_err) => {
                            tracing::warn!(
                                attempt,
                                contract = %self.contract_name,
                                error = %parse_err,
                                "contract parse failed — retrying"
                            );
                            if attempt < self.retry_policy.max_attempts {
                                tokio::time::sleep(std::time::Duration::from_millis(
                                    self.retry_policy.backoff_ms,
                                )).await;
                            }
                        }
                    }
                }
                Err(call_err) => {
                    tracing::warn!(
                        attempt,
                        contract = %self.contract_name,
                        error = %call_err,
                        "llm call failed — retrying"
                    );
                    if attempt < self.retry_policy.max_attempts {
                        tokio::time::sleep(std::time::Duration::from_millis(
                            self.retry_policy.backoff_ms,
                        )).await;
                    }
                }
            }
        }

        // Retries exhausted — use fallback or error
        if let Some(ref fallback) = self.fallback {
            tracing::warn!(contract = %self.contract_name, "retries exhausted — using fallback");
            Ok(EnforcedResponse {
                value: fallback.clone(), // requires T: Clone for fallback path
                provenance: LlmProvenance {
                    provider: "fallback".to_string(),
                    model: None,
                    role: format!("{:?}", self.role),
                    duration_ms: 0,
                    prompt_chars: 0,
                    response_chars: 0,
                    attempt: self.retry_policy.max_attempts,
                },
            })
        } else {
            anyhow::bail!(
                "contract enforcement failed for '{}' after {} attempts",
                self.contract_name,
                self.retry_policy.max_attempts
            )
        }
    }
}
```

Add `retry_policy_for_role()`:

```rust
pub fn retry_policy_for_role(role: &LlmRole) -> RetryPolicy {
    match role {
        LlmRole::Scaffolding => RetryPolicy { max_attempts: 3, backoff_ms: 1000 },
        LlmRole::SearchHints => RetryPolicy { max_attempts: 2, backoff_ms: 500 },
        LlmRole::ProseRendering => RetryPolicy { max_attempts: 1, backoff_ms: 0 },
        LlmRole::LeanScaffold => RetryPolicy { max_attempts: 2, backoff_ms: 1000 },
    }
}
```

**Tests:** Unit tests with a mock provider:
- Valid JSON on first try → passes, attempt=1.
- Invalid JSON then valid JSON → passes, attempt=2.
- All attempts fail, fallback provided → returns fallback.
- All attempts fail, no fallback → returns error.
- Assertion count invariant preserved (for harness-like contracts).

---

#### Task 11: Migrate CopilotService to use ContractEnforcer

**File:** `crates/services/llm/src/copilot.rs`

Replace the private `complete_json()` method:

```rust
// Before:
async fn complete_json<T>(&self, role: LlmRole, prompt: &str) -> Result<T>
where T: serde::de::DeserializeOwned {
    let response = llm_call(&*self.provider, role, prompt, &CompletionOpts::default()).await?;
    parse_json_contract(&response)
}

// After:
async fn enforce_contract<T>(
    &self,
    role: LlmRole,
    contract_name: &str,
    task_description: &str,
) -> Result<EnforcedResponse<T>>
where T: DeserializeOwned + Clone + Default {
    let policy = enforcement::retry_policy_for_role(&role);
    let enforcer = ContractEnforcer::<T>::new(role, contract_name)
        .with_retry(policy)
        .with_fallback(T::default());
    enforcer.execute(&*self.provider, task_description, &CompletionOpts::default()).await
}
```

Update each public method:
- `plan_checklists()` → use `enforce_contract::<ChecklistPlan>(SearchHints, "ChecklistPlan", ...)`
- `generate_overview_note()` → use `enforce_contract::<ArchitectureOverview>(SearchHints, "ArchitectureOverview", ...)`
- `generate_candidate_*()` → use `ContractEnforcer::<CandidateDraft>` **without fallback** (candidates require real LLM output)

Keep post-parse validation (empty id/rationale checks) after enforcement.

---

#### Task 12: Add provenance to GateResult

**File:** `crates/services/llm/src/evidence_gate.rs`

Add field to `GateResult`:

```rust
pub struct GateResult {
    // ... existing fields ...
    #[serde(default)]
    pub provenance: Option<LlmProvenance>,
}
```

In `fix_syntax_and_retry()`, switch the inner `llm_call` to `llm_call_traced()`. Capture provenance from the successful (or last failed) attempt and store in the result.

---

#### Task 13: Migrate remaining call sites to llm_call_traced

Migrate one at a time. For each, replace `llm_call(provider, role, prompt, opts)` with `llm_call_traced(...)` and handle the `(String, LlmProvenance)` return:

| Call site | File | Action |
|-----------|------|--------|
| Kani scaffolder | `crates/engines/crypto/src/kani/scaffolder.rs:159` | Capture provenance, log it, discard (no session store access in engine) |
| Distributed harness builder | `crates/engines/distributed/src/harness/builder.rs:142` | Same pattern |
| Economic description | `crates/engines/distributed/src/economic/mod.rs:193` | Same pattern |
| Lean scaffold | `crates/engines/lean/src/scaffold.rs:29` | Same pattern |
| Report generator | `crates/services/report/src/generator.rs:271` | Same pattern |

After all sites migrated, deprecate the old `llm_call()` with `#[deprecated]` attribute pointing to `llm_call_traced()`.

---

#### Task 14: Consolidate the two LlmRole enums

**File:** `crates/core/src/llm.rs` — the core definition has `MechanicalScaffolding`, `SearchSpaceGuidance`, `ProseRendering`.

**File:** `crates/services/llm/src/provider.rs` — the llm-crate definition has `Scaffolding`, `SearchHints`, `ProseRendering`, `LeanScaffold`.

Strategy: keep the llm-crate version as the canonical one (it's more complete). In `crates/core/src/llm.rs`, re-export from the llm crate or deprecate in favor of the llm-crate enum. Since core should not depend on the llm service crate, add a `core::LlmRole` mapping function in the orchestrator that converts between them, or unify names by renaming core's variants to match.

Preferred approach: rename core's enum to match the llm crate's names, since the llm crate's names are used at all 7 call sites:

```rust
// crates/core/src/llm.rs
pub enum LlmRole {
    Scaffolding,       // was MechanicalScaffolding
    SearchHints,       // was SearchSpaceGuidance
    ProseRendering,    // unchanged
    LeanScaffold,      // new — was only in llm crate
}
```

Then remove the duplicate definition from the llm crate and import from core.

---

#### Task 15: Record LLM interactions in authoritative session events

**Files:** `crates/services/llm/src/enforcement.rs`, `crates/services/session-manager/src/state.rs`, `crates/data/session-store/src/sqlite.rs`

Do **not** add a new authoritative `llm_interactions` table in T0. `session_events` is the primary observability store for this plan; a materialized table can be added later only if query volume requires it.

Add a helper path that turns `LlmProvenance` into an `AuditEvent::LlmInteraction` / `SessionEvent` when a session context exists:

```rust
pub fn append_llm_interaction_event(
    &self,
    session_id: &str,
    provenance: &LlmProvenance,
    succeeded: bool,
) -> Result<()>
```

Requirements:
1. Session-bound execution paths must thread `session_id` into the enforcement layer or its callback wrapper.
2. The enforcement layer emits or returns the data needed to append a typed `llm.interaction` event.
3. Engine-level calls that truly have no session context continue to log via tracing only.
4. If a later plan needs a `llm_interactions` table for performance, it must be explicitly documented as a derived/materialized projection of `session_events`, not a second source of truth.

---

#### Task 16: Lint test — no raw parse_json_contract outside enforcement

**File:** `crates/services/llm/tests/enforcement_lint_tests.rs` (new)

Pattern: scan all `.rs` files under `crates/` for `parse_json_contract(` calls. Allow only:
- `crates/services/llm/src/sanitize.rs` (definition)
- `crates/services/llm/src/enforcement.rs` (the enforcement layer)
- `crates/services/llm/tests/` (test files)

Fail if any other file calls `parse_json_contract` directly. This ensures all LLM JSON parsing goes through the enforcement layer.

---

## Dependency Map

```
Task 1  (types)          ← no deps
Task 2  (coverage confidence) ← Task 1
Task 3  (execute_dag)    ← Task 1, Task 4
Task 4  (events)         ← no deps
Task 5  (produce_outputs)← Task 1, Task 2, Task 3
Task 6  (bootstrap jobs) ← Task 4
Task 7  (UI/reports)     ← Task 5

Task 8  (model trait)    ← no deps
Task 9  (provenance)     ← Task 8
Task 10 (enforcement)    ← Task 9
Task 11 (copilot)        ← Task 10
Task 12 (evidence gate)  ← Task 9
Task 13 (call sites)     ← Task 9
Task 14 (enum consolidation) ← no deps (can be done early)
Task 15 (session events) ← Task 9
Task 16 (lint test)      ← Task 10, Task 11
```

Items A and B are independent and can be developed in parallel branches, merging into a shared integration branch for final testing.
