# Plan 5 — T2/T3: Resilience

> **For Agent:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the LLM pipeline resilient to provider outages without silently corrupting reproducibility, and add a bounded supervision layer that helps the system recover from stuck or failed verification workflows.

**Architecture:** Item A adds failover logic to the `RoleAwareProvider` (Plan 3A) with provenance tracking (Plan 1B) so every model switch is recorded. Item B adds an `AdviserService` that observes engine failures, suggests recovery actions, and may trigger bounded mechanical retries — but never modifies findings or verification status.

**Tech Stack:** Rust workspace crates, tokio, reqwest, serde, tracing.

**Depends on:** Plan 1A (engine degradation — adviser responds to engine failures), Plan 1B (provenance — failover is recorded), Plan 3A (RoleAwareProvider — failover extends the role-aware dispatch), Plan 4A (observability — failover and adviser events are recorded).

---

## Planning Conventions

- Recommended execution context: a dedicated git worktree.
- Failover must be transparent — every model switch is recorded in provenance.
- The adviser never generates, modifies, or validates findings.
- The system may apply bounded mechanical recovery actions from adviser suggestions, with full provenance and hard limits.
- Deterministic engines and ProjectIR remain the source of truth for audit analysis; AI layers may assist planning, explanation, and recovery, but do not redefine deterministic results.
- Hard limits on adviser invocations prevent runaway LLM costs.
- Every task must land with tests.

## Release Gates

This plan is complete only when all of the following are true:

- `RoleAwareProvider` tries the next provider in the chain on transient errors.
- Failover events are recorded in provenance and session events.
- A circuit breaker prevents hammering a down provider.
- Failover is recorded in provenance and surfaced as a runtime warning/event; coverage remains reserved for engine execution completeness.
- The `AdviserService` can suggest recovery actions when an engine fails.
- The orchestrator can apply mechanical suggestions (retry with larger budget) with hard limits.
- Automatic retry is limited to explicitly classified retryable failures (timeout, OOM, transient sandbox/resource exhaustion), not semantic or unsupported-input failures.
- Adviser invocations are capped at 5 per audit.
- `cargo test -p llm` and `cargo test -p orchestrator` pass.

## Explicit Non-Goals

- Automatic model selection based on quality (that's the eval harness's job, Plan 3B).
- Adviser-initiated re-analysis or finding generation.
- Load balancing across providers.
- Cost-based routing (use the cheapest model that passes evals).

---

## Item A: Runtime Provider Failover with Provenance

### Context

`provider_from_env()` at `crates/services/llm/src/provider.rs:184` selects one provider at startup and never switches. `RoleAwareProvider` (Plan 3A) dispatches by role but still uses a single provider per role. If that provider has a transient outage mid-audit, all LLM-assisted operations fail. Silent model switching is a reproducibility risk — if the scaffolding model changes from `gpt-4o-mini` to `claude-3-5-sonnet` mid-audit, the harnesses may be structurally different.

### Tasks

#### Task 1: Define transient vs permanent error classification

**File:** `crates/services/llm/src/provider.rs`

Add error classification:

```rust
/// Classify an LLM call error as transient (retryable) or permanent.
pub fn is_transient_error(err: &anyhow::Error) -> bool {
    let msg = err.to_string().to_lowercase();

    // HTTP status codes
    if msg.contains("429") || msg.contains("rate limit") { return true; }
    if msg.contains("500") || msg.contains("internal server error") { return true; }
    if msg.contains("502") || msg.contains("bad gateway") { return true; }
    if msg.contains("503") || msg.contains("service unavailable") { return true; }
    if msg.contains("504") || msg.contains("gateway timeout") { return true; }

    // Network errors
    if msg.contains("timed out") || msg.contains("timeout") { return true; }
    if msg.contains("connection refused") { return true; }
    if msg.contains("connection reset") { return true; }
    if msg.contains("dns") { return true; }

    // Permanent errors — do NOT retry
    // 400 Bad Request, 401 Unauthorized, 403 Forbidden, 404 Not Found
    false
}
```

Enhance each provider's `complete()` to preserve HTTP status in error context:

```rust
// In parse_openai_response, parse_anthropic_response, parse_ollama_response:
// Already done — they include status code in the error message.
// Ensure the status code is present: "OpenAI request failed (503): ..."
```

**Tests:** Verify classification: 429 → transient, 503 → transient, timeout → transient, 401 → permanent, 400 → permanent.

---

#### Task 2: Add fallback chain to RoleConfig

**File:** `crates/services/llm/src/role_config.rs`

Extend `RoleConfig`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub temperature_millis: Option<u16>,
    pub max_tokens: Option<u32>,
    /// Fallback providers to try in order if the primary fails with a transient error.
    /// Provider names: "openai", "anthropic", "ollama", "template-fallback".
    #[serde(default)]
    pub fallback_chain: Vec<String>,
}
```

Update `Default` for `LlmRoleConfigMap` to include default fallback chains:

```rust
scaffolding: RoleConfig {
    // ...existing...
    fallback_chain: vec!["template-fallback".to_string()],
},
search_hints: RoleConfig {
    // ...existing...
    fallback_chain: vec![],  // No fallback — search hints need real LLM
},
prose_rendering: RoleConfig {
    // ...existing...
    fallback_chain: vec![],  // No fallback — accept raw text on failure
},
lean_scaffold: RoleConfig {
    // ...existing...
    fallback_chain: vec!["template-fallback".to_string()],
},
```

Load from env: `LLM_ROLE_SCAFFOLDING_FALLBACK_CHAIN=ollama,template-fallback` (comma-separated).

---

#### Task 3: Implement failover in RoleAwareProvider

**File:** `crates/services/llm/src/role_config.rs`

Rewrite `complete_for_role()` to include failover:

```rust
pub async fn complete_for_role(
    &self,
    role: LlmRole,
    prompt: &str,
) -> anyhow::Result<(String, LlmProvenance)> {
    let rc = self.config_for_role(&role);
    let (primary, opts) = self.resolve_provider_and_opts(&role, &rc);

    // Try primary provider
    match llm_call_traced(&*primary, role.clone(), prompt, &opts).await {
        Ok(result) => return Ok(result),
        Err(err) => {
            if !is_transient_error(&err) {
                return Err(err);  // Permanent error — don't try fallbacks
            }
            tracing::warn!(
                role = ?role,
                provider = primary.name(),
                error = %err,
                "primary provider failed with transient error — trying fallback chain"
            );
        }
    }

    // Try fallback chain
    for fallback_name in &rc.fallback_chain {
        let Some(fallback_provider) = self.providers.get(fallback_name) else {
            tracing::debug!(provider = %fallback_name, "fallback provider not available — skipping");
            continue;
        };

        if self.is_circuit_open(fallback_name) {
            tracing::debug!(provider = %fallback_name, "circuit breaker open — skipping");
            continue;
        }

        // Emit failover event
        if let Some(recorder) = &self.event_recorder {
            recorder(AuditEvent::ProviderFailover {
                from: primary.name().to_string(),
                to: fallback_name.clone(),
                role: format!("{:?}", role),
                reason: "transient error on primary".to_string(),
            });
        }

        match llm_call_traced(&**fallback_provider, role.clone(), prompt, &opts).await {
            Ok((response, mut provenance)) => {
                // Mark provenance as failover
                provenance.provider = format!("{}(failover)", provenance.provider);
                return Ok((response, provenance));
            }
            Err(fallback_err) => {
                tracing::warn!(
                    provider = %fallback_name,
                    error = %fallback_err,
                    "fallback provider also failed — trying next"
                );
                self.record_failure(fallback_name);
            }
        }
    }

    anyhow::bail!(
        "all providers failed for role {:?} — primary and {} fallback(s) exhausted",
        role,
        rc.fallback_chain.len()
    )
}
```

---

#### Task 4: Implement circuit breaker

**File:** `crates/services/llm/src/role_config.rs`

Add circuit breaker state to `RoleAwareProvider`:

```rust
use std::sync::Mutex;
use std::time::Instant;

struct CircuitBreakerState {
    consecutive_failures: u32,
    last_failure: Option<Instant>,
}

pub struct RoleAwareProvider {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    role_configs: LlmRoleConfigMap,
    default_provider: Arc<dyn LlmProvider>,
    event_recorder: Option<Arc<dyn Fn(AuditEvent) + Send + Sync>>,
    circuit_breakers: Mutex<HashMap<String, CircuitBreakerState>>,
}

const CIRCUIT_BREAKER_THRESHOLD: u32 = 3;
const CIRCUIT_BREAKER_RESET_SECS: u64 = 300;  // 5 minutes

impl RoleAwareProvider {
    fn is_circuit_open(&self, provider_name: &str) -> bool {
        let breakers = self.circuit_breakers.lock().unwrap();
        if let Some(state) = breakers.get(provider_name) {
            if state.consecutive_failures >= CIRCUIT_BREAKER_THRESHOLD {
                // Check if reset window has passed
                if let Some(last) = state.last_failure {
                    if last.elapsed().as_secs() < CIRCUIT_BREAKER_RESET_SECS {
                        return true;  // Still open
                    }
                }
            }
        }
        false
    }

    fn record_failure(&self, provider_name: &str) {
        let mut breakers = self.circuit_breakers.lock().unwrap();
        let state = breakers.entry(provider_name.to_string()).or_insert(CircuitBreakerState {
            consecutive_failures: 0,
            last_failure: None,
        });
        state.consecutive_failures += 1;
        state.last_failure = Some(Instant::now());
    }

    fn record_success(&self, provider_name: &str) {
        let mut breakers = self.circuit_breakers.lock().unwrap();
        if let Some(state) = breakers.get_mut(provider_name) {
            state.consecutive_failures = 0;
        }
    }
}
```

**Tests:**
- 3 failures → circuit opens → provider skipped.
- After 5 minutes → circuit resets → provider tried again.
- Success resets consecutive failure count.

---

#### Task 5: Add ProviderFailover event

**File:** `crates/apps/orchestrator/src/events.rs`

Add to `AuditEvent`:

```rust
ProviderFailover {
    from: String,
    to: String,
    role: String,
    reason: String,
},
```

---

#### Task 6: Add failover warning to CoverageReport

**File:** `crates/core/src/output.rs`

When building `CoverageReport`, check if any provenance records show `"(failover)"` in the provider field. If so, add a warning:

```
"LLM provider failover occurred: {role} switched from {from} to {to}. Findings produced during failover may differ from baseline."
```

This is generated at the orchestrator level where both the coverage report and LLM provenance are available. Add a new field to `CoverageReport`:

```rust
pub struct CoverageReport {
    // ... existing fields ...
    #[serde(default)]
    pub failover_warnings: Vec<String>,
}
```

Merge `failover_warnings` into the overall `warnings` when rendering the report.

---

#### Task 7: Event recording for failover

Ensure failover events are persisted:
1. The `event_recorder` callback on `RoleAwareProvider` emits `AuditEvent::ProviderFailover`.
2. The orchestrator or session manager converts this to a `SessionEvent` and appends to the session store.
3. The activity console (Plan 4A) shows failover events with an amber badge.

**Test:** Integration: configure primary with a mock that fails, fallback with a mock that succeeds. Verify failover event recorded. Verify provenance shows fallback provider.

---

## Item B: Adviser Layer with Bounded Automatic Recovery

### Context

When formal verification tools fail — Z3 times out, Kani hits resource limits, a sandbox OOMs — the system currently marks the finding as "Unverified" or the engine as "Failed" and moves on. AGI's adviser pattern suggests having a supervisory LLM that analyzes failures and recommends recovery actions. The critical constraint: the adviser never generates or modifies findings, and any automatic recovery must remain bounded, mechanical, and fully traceable.

### Tasks

#### Task 8: Define adviser types

**File:** `crates/services/llm/src/adviser.rs` (new module)

```rust
use serde::{Deserialize, Serialize};

/// Context passed to the adviser when an engine or tool fails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdviserContext {
    pub engine_name: String,
    pub error_message: String,
    pub attempt_number: u8,
    pub elapsed_ms: u64,
    pub findings_so_far: usize,
    pub budget: AdviserBudgetSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdviserBudgetSnapshot {
    pub timeout_secs: u64,
    pub memory_mb: u64,
    pub cpu_cores: f64,
}

/// The adviser's suggestion — a bounded set of mechanical actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdviserSuggestion {
    pub action: AdviserAction,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AdviserAction {
    /// Retry the engine with a larger resource budget.
    RetryWithRelaxedBudget {
        timeout_secs: u64,
        memory_mb: u64,
    },
    /// Skip this engine — it's unlikely to produce results here.
    SkipEngine {
        reason: String,
    },
    /// Suggest an alternative tool family for the user to try manually.
    TryAlternativeTool {
        tool_family: String,
        suggestion: String,
    },
    /// Reduce the input scope (fewer files, smaller crate set).
    ReduceInputScope {
        suggestion: String,
    },
    /// No useful suggestion — proceed with default behavior.
    NoSuggestion,
}
```

---

#### Task 9: Implement AdviserService

**File:** `crates/services/llm/src/adviser.rs`

```rust
use std::sync::Arc;
use anyhow::Result;
use crate::enforcement::{ContractEnforcer, RetryPolicy, EnforcedResponse};
use crate::provider::{LlmProvider, LlmRole, CompletionOpts};

pub struct AdviserService {
    provider: Arc<dyn LlmProvider>,
}

impl AdviserService {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self { provider }
    }

    /// Ask the adviser for a suggestion when an engine fails.
    /// Returns None if the adviser cannot produce a useful suggestion.
    pub async fn suggest_on_failure(
        &self,
        context: &AdviserContext,
    ) -> Result<AdviserSuggestion> {
        let task = format!(
            "An audit engine failed. Suggest ONE recovery action.\n\n\
             Engine: {engine}\n\
             Error: {error}\n\
             Attempt: {attempt}\n\
             Elapsed: {elapsed}ms\n\
             Findings so far: {findings}\n\
             Current budget: timeout={timeout}s, memory={memory}MB\n\n\
             Available actions (return ONE as JSON):\n\
             - RetryWithRelaxedBudget: increase timeout_secs and/or memory_mb\n\
             - SkipEngine: skip this engine with a reason\n\
             - TryAlternativeTool: suggest a different tool family for the user\n\
             - ReduceInputScope: suggest narrowing the analysis scope\n\
             - NoSuggestion: no useful recovery possible\n\n\
             Consider: Is the error a resource issue (OOM, timeout)? \
             A configuration issue? Or a fundamental incompatibility?\n\
             Be specific about parameter values for RetryWithRelaxedBudget.",
            engine = context.engine_name,
            error = truncate(&context.error_message, 500),
            attempt = context.attempt_number,
            elapsed = context.elapsed_ms,
            findings = context.findings_so_far,
            timeout = context.budget.timeout_secs,
            memory = context.budget.memory_mb,
        );

        let enforcer = ContractEnforcer::<AdviserSuggestion>::new(
            LlmRole::Advisory,
            "AdviserSuggestion",
        )
        .with_retry(RetryPolicy { max_attempts: 1, backoff_ms: 0 })
        .with_fallback(AdviserSuggestion {
            action: AdviserAction::NoSuggestion,
            rationale: "Adviser could not produce a suggestion".to_string(),
        });

        let result = enforcer.execute(
            &*self.provider,
            &task,
            &CompletionOpts { temperature_millis: 100, max_tokens: 512 },
        ).await?;

        Ok(result.value)
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}
```

---

#### Task 10: Add LlmRole::Advisory

**File:** `crates/services/llm/src/provider.rs`

Add variant:

```rust
pub enum LlmRole {
    Scaffolding,
    SearchHints,
    ProseRendering,
    LeanScaffold,
    Advisory,  // New
}
```

**File:** `crates/services/llm/src/role_config.rs`

Add advisory config to `LlmRoleConfigMap`:

```rust
pub struct LlmRoleConfigMap {
    // ... existing ...
    #[serde(default)]
    pub advisory: RoleConfig,
}

// Default:
advisory: RoleConfig {
    provider: None,
    model: None,
    temperature_millis: Some(100),
    max_tokens: Some(512),
    fallback_chain: vec![],  // Don't failover adviser — if it fails, just skip
},
```

Update `resolve_for_role` in `RoleAwareProvider` to handle `LlmRole::Advisory`.

Update `retry_policy_for_role` in enforcement:

```rust
LlmRole::Advisory => RetryPolicy { max_attempts: 1, backoff_ms: 0 },
```

---

#### Task 11: Integrate adviser into orchestrator execute_dag

**File:** `crates/apps/orchestrator/src/lib.rs`

In the rewritten `execute_dag()` (Plan 1A Task 3), after catching an engine error:

```rust
Err(err) => {
    tracing::error!(engine = %engine_name, error = %err, "engine failed");

    // Consult adviser if available and within limits
    let mut suggestion = None;
    if adviser_calls_remaining > 0 {
        if let Some(adviser) = &self.adviser {
            adviser_calls_remaining -= 1;
            let ctx = AdviserContext {
                engine_name: engine_name.clone(),
                error_message: err.to_string(),
                attempt_number: attempt,
                elapsed_ms: start.elapsed().as_millis() as u64,
                findings_so_far: findings.len(),
                budget: AdviserBudgetSnapshot::from_engine(engine.name(), config),
            };

            match adviser.suggest_on_failure(&ctx).await {
                Ok(s) => {
                    tracing::info!(
                        engine = %engine_name,
                        action = ?s.action,
                        rationale = %s.rationale,
                        "adviser suggestion"
                    );
                    if let Some(sink) = &self.event_sink {
                        sink.emit(AuditEvent::AdviserConsulted {
                            engine: engine_name.clone(),
                            suggestion: format!("{:?}", s.action),
                            applied: false,  // Updated below if applied
                        });
                    }
                    suggestion = Some(s);
                }
                Err(adviser_err) => {
                    tracing::warn!(error = %adviser_err, "adviser call failed — proceeding without suggestion");
                }
            }
        }
    }

    // Apply mechanical suggestions only for retryable failures and only when
    // the engine family exposes an allowlisted recovery policy.
    if attempt == 1 {
        if is_retryable_engine_failure(&err) {
            if let Some(AdviserSuggestion { action: AdviserAction::RetryWithRelaxedBudget { timeout_secs, memory_mb }, .. }) = &suggestion {
                if engine_supports_budget_adjustment(engine.name(), *timeout_secs, *memory_mb) {
                    tracing::info!(engine = %engine_name, timeout_secs, memory_mb, "applying adviser suggestion: retry with relaxed budget");
                    let mut relaxed_config = config.clone();
                    apply_retry_budget_adjustment(&mut relaxed_config, engine.name(), *timeout_secs, *memory_mb)?;
                    // ... rebuild context with relaxed budget and retry engine.analyze()
                    // If retry succeeds, record EngineCompleted with adviser annotation
                    // If retry fails, record EngineFailed
                    attempt += 1;
                    continue;  // retry loop
                }
            }
        }
    }

    // Record failure
    outcomes.push(EngineOutcome {
        engine: engine_name.clone(),
        status: EngineStatus::Failed { reason: err.to_string() },
        findings_count: 0,
        duration_ms: start.elapsed().as_millis() as u64,
        adviser_suggestion: suggestion.map(|s| format!("{:?}: {}", s.action, s.rationale)),
    });
}
```

Add adviser to `AuditOrchestrator`:

```rust
pub struct AuditOrchestrator {
    // ... existing fields ...
    pub adviser: Option<AdviserService>,
}

impl AuditOrchestrator {
    pub fn with_adviser(mut self, adviser: AdviserService) -> Self {
        self.adviser = Some(adviser);
        self
    }
}
```

Do not implement one global mutation path like `config.budget.kani_timeout_secs = ...` for all engines. Recovery must be engine-family specific and allowlisted.

---

#### Task 12: Hard limits on adviser invocations

**File:** `crates/apps/orchestrator/src/lib.rs`

In `execute_dag()`:

```rust
const MAX_ADVISER_CALLS_PER_AUDIT: u8 = 5;
const MAX_RETRIES_PER_ENGINE: u8 = 1;

let mut adviser_calls_remaining = MAX_ADVISER_CALLS_PER_AUDIT;
```

These constants are defined at the module level, not configurable. Rationale: adviser costs should be bounded absolutely, not per-user-configuration. If a user wants more retries, they should adjust engine budgets directly rather than relying on the adviser.

---

#### Task 13: Add adviser_suggestion to EngineOutcome

**File:** `crates/core/src/output.rs`

Extend `EngineOutcome`:

```rust
pub struct EngineOutcome {
    pub engine: String,
    pub status: EngineStatus,
    pub findings_count: usize,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adviser_suggestion: Option<String>,
}
```

This is a human-readable summary of what the adviser suggested, if anything. Goes into the manifest for post-audit review.

---

#### Task 14: Add AdviserConsulted event

**File:** `crates/apps/orchestrator/src/events.rs`

```rust
AdviserConsulted {
    engine: String,
    suggestion: String,
    applied: bool,
},
```

Record in session events for observability (Plan 4A).

---

#### Task 15: Export and wire up

**File:** `crates/services/llm/src/lib.rs`

Add:
```rust
pub mod adviser;
pub use adviser::{AdviserService, AdviserContext, AdviserSuggestion, AdviserAction};
```

**File:** `crates/apps/cli/src/lib.rs`

When constructing the orchestrator, if LLM is available:

```rust
let adviser = AdviserService::new(Arc::clone(&llm));
orchestrator = orchestrator.with_adviser(adviser);
```

---

#### Task 16: Tests

1. **Unit — AdviserService**: Mock provider returns valid `AdviserSuggestion` JSON. Verify parsing. Mock provider returns garbage → fallback `NoSuggestion` returned.

2. **Integration — retry with relaxed budget**: Configure orchestrator with a `BudgetSensitiveEngine` that fails with "timeout" when budget < 300s, succeeds when budget >= 300s. Adviser suggests `RetryWithRelaxedBudget { timeout_secs: 600 }`. Verify: engine fails first attempt, adviser consulted, engine retried with larger budget, engine succeeds on retry. Findings collected.

3. **Integration — adviser suggests skip**: Adviser returns `SkipEngine`. Verify engine outcome is `Failed` (not `Skipped` — skipping is only for `supports() == false`). The suggestion is logged but the orchestrator records it as a failed engine, not a clean skip.

4. **Guard — adviser call limit**: Configure orchestrator with 6 failing engines. Verify only 5 adviser calls made (constant `MAX_ADVISER_CALLS_PER_AUDIT`).

5. **Guard — retry limit**: Configure engine that always fails. Verify only 1 adviser-suggested retry per engine (constant `MAX_RETRIES_PER_ENGINE`).

---

## Dependency Map

```
Task 1  (error classification)  ← no deps
Task 2  (fallback chain)        ← Plan 3A Task 1 (RoleConfig)
Task 3  (failover logic)        ← Task 1, Task 2, Plan 1B (llm_call_traced)
Task 4  (circuit breaker)       ← Task 3
Task 5  (failover event)        ← Plan 1A (events.rs)
Task 6  (coverage warning)      ← Plan 1A (CoverageReport), Task 3
Task 7  (event recording)       ← Task 5, Plan 4A (session events)

Task 8  (adviser types)         ← no deps
Task 9  (adviser service)       ← Task 8, Plan 1B (ContractEnforcer)
Task 10 (advisory role)         ← Task 9, Plan 3A (RoleAwareProvider)
Task 11 (orchestrator)          ← Task 9, Plan 1A (execute_dag rewrite)
Task 12 (hard limits)           ← Task 11
Task 13 (outcome annotation)    ← Plan 1A (EngineOutcome)
Task 14 (adviser event)         ← Plan 1A (events.rs)
Task 15 (exports)               ← Task 9
Task 16 (tests)                 ← Tasks 9-15
```

Item A (failover) and Item B (adviser) are independent and can be developed in parallel, but both depend on Plans 1 and 3 being complete.
