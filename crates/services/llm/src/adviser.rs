use std::sync::Arc;

use anyhow::Result;
use audit_agent_core::audit_config::BudgetConfig;
use serde::{Deserialize, Serialize};

use crate::enforcement::{ContractEnforcer, RetryPolicy};
use crate::provider::{CompletionOpts, LlmProvider, LlmRole};

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

impl AdviserBudgetSnapshot {
    pub fn from_engine(engine_name: &str, budget: &BudgetConfig) -> Self {
        let engine = engine_name.to_ascii_lowercase();
        if engine.contains("kani") {
            return Self {
                timeout_secs: budget.kani_timeout_secs,
                memory_mb: 4096,
                cpu_cores: 2.0,
            };
        }
        if engine.contains("z3") || engine.contains("smt") {
            return Self {
                timeout_secs: budget.z3_timeout_secs,
                memory_mb: 2048,
                cpu_cores: 2.0,
            };
        }
        if engine.contains("fuzz") {
            return Self {
                timeout_secs: budget.fuzz_duration_secs,
                memory_mb: 8192,
                cpu_cores: 2.0,
            };
        }
        if engine.contains("madsim") {
            return Self {
                timeout_secs: budget.madsim_ticks,
                memory_mb: 4096,
                cpu_cores: 2.0,
            };
        }

        Self {
            timeout_secs: budget.semantic_index_timeout_secs,
            memory_mb: 2048,
            cpu_cores: 2.0,
        }
    }
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
    RetryWithRelaxedBudget { timeout_secs: u64, memory_mb: u64 },
    /// Skip this engine — it's unlikely to produce results here.
    SkipEngine { reason: String },
    /// Suggest an alternative tool family for the user to try manually.
    TryAlternativeTool {
        tool_family: String,
        suggestion: String,
    },
    /// Reduce the input scope (fewer files, smaller crate set).
    ReduceInputScope { suggestion: String },
    /// No useful suggestion — proceed with default behavior.
    NoSuggestion,
}

pub struct AdviserService {
    provider: Arc<dyn LlmProvider>,
}

impl AdviserService {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self { provider }
    }

    /// Ask the adviser for a suggestion when an engine fails.
    pub async fn suggest_on_failure(&self, context: &AdviserContext) -> Result<AdviserSuggestion> {
        let task = format!(
            "An audit engine failed. Suggest ONE recovery action.\n\n\\
             Engine: {engine}\n\\
             Error: {error}\n\\
             Attempt: {attempt}\n\\
             Elapsed: {elapsed}ms\n\\
             Findings so far: {findings}\n\\
             Current budget: timeout={timeout}s, memory={memory}MB\n\n\\
             Available actions (return ONE as JSON):\n\\
             - RetryWithRelaxedBudget: increase timeout_secs and/or memory_mb\n\\
             - SkipEngine: skip this engine with a reason\n\\
             - TryAlternativeTool: suggest a different tool family for the user\n\\
             - ReduceInputScope: suggest narrowing the analysis scope\n\\
             - NoSuggestion: no useful recovery possible\n\n\\
             Consider whether this is a resource failure, transient infra issue, \\
             or a fundamental incompatibility.\n\\
             Be specific about parameter values for RetryWithRelaxedBudget.",
            engine = context.engine_name,
            error = truncate(&context.error_message, 500),
            attempt = context.attempt_number,
            elapsed = context.elapsed_ms,
            findings = context.findings_so_far,
            timeout = context.budget.timeout_secs,
            memory = context.budget.memory_mb,
        );

        let enforcer =
            ContractEnforcer::<AdviserSuggestion>::new(LlmRole::Advisory, "AdviserSuggestion")
                .with_retry(RetryPolicy {
                    max_attempts: 1,
                    backoff_ms: 0,
                })
                .with_fallback(AdviserSuggestion {
                    action: AdviserAction::NoSuggestion,
                    rationale: "Adviser could not produce a suggestion".to_string(),
                });

        let result = enforcer
            .execute(
                self.provider.as_ref(),
                &task,
                &CompletionOpts {
                    temperature_millis: 100,
                    max_tokens: 512,
                },
                None,
            )
            .await?;

        Ok(result.value)
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}
