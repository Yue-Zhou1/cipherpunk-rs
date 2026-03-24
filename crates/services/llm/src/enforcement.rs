use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde::de::DeserializeOwned;

use crate::provider::{
    CompletionOpts, LlmProvenance, LlmProvider, LlmRole, json_only_prompt, llm_call_traced,
};
use crate::sanitize::parse_json_contract;

pub type LlmInteractionHook = Arc<dyn Fn(&LlmProvenance, bool) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u8,
    pub backoff_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnforcedResponse<T> {
    pub value: T,
    pub provenance: LlmProvenance,
}

#[derive(Clone)]
pub struct ContractEnforcer<T: DeserializeOwned + Clone> {
    role: LlmRole,
    contract_name: String,
    retry_policy: RetryPolicy,
    fallback: Option<T>,
    _phantom: PhantomData<T>,
}

impl<T: DeserializeOwned + Clone> ContractEnforcer<T> {
    pub fn new(role: LlmRole, contract_name: &str) -> Self {
        Self {
            role,
            contract_name: contract_name.to_string(),
            retry_policy: RetryPolicy {
                max_attempts: 1,
                backoff_ms: 0,
            },
            fallback: None,
            _phantom: PhantomData,
        }
    }

    pub fn with_retry(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    pub fn with_fallback(mut self, fallback: T) -> Self {
        self.fallback = Some(fallback);
        self
    }

    pub async fn execute(
        &self,
        provider: &dyn LlmProvider,
        task_description: &str,
        opts: &CompletionOpts,
        interaction_hook: Option<&LlmInteractionHook>,
    ) -> Result<EnforcedResponse<T>> {
        let prompt = json_only_prompt(&self.contract_name, task_description);
        let attempts = self.retry_policy.max_attempts.max(1);

        for attempt in 1..=attempts {
            match llm_call_traced(provider, self.role.clone(), &prompt, opts).await {
                Ok((response, mut provenance)) => {
                    provenance.attempt = attempt;
                    match parse_json_contract::<T>(&response) {
                        Ok(value) => {
                            if let Some(hook) = interaction_hook {
                                hook(&provenance, true);
                            }
                            return Ok(EnforcedResponse { value, provenance });
                        }
                        Err(parse_error) => {
                            if let Some(hook) = interaction_hook {
                                hook(&provenance, false);
                            }
                            tracing::warn!(
                                attempt,
                                contract = %self.contract_name,
                                error = %parse_error,
                                "contract parse failed — retrying"
                            );
                            if attempt < attempts && self.retry_policy.backoff_ms > 0 {
                                tokio::time::sleep(Duration::from_millis(
                                    self.retry_policy.backoff_ms,
                                ))
                                .await;
                            }
                        }
                    }
                }
                Err(call_error) => {
                    let failure = LlmProvenance {
                        provider: provider.name().to_string(),
                        model: provider.model().map(|value| value.to_string()),
                        role: format!("{:?}", self.role),
                        duration_ms: 0,
                        prompt_chars: prompt.len(),
                        response_chars: 0,
                        attempt,
                    };
                    if let Some(hook) = interaction_hook {
                        hook(&failure, false);
                    }
                    tracing::warn!(
                        attempt,
                        contract = %self.contract_name,
                        error = %call_error,
                        "llm call failed — retrying"
                    );
                    if attempt < attempts && self.retry_policy.backoff_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(self.retry_policy.backoff_ms))
                            .await;
                    }
                }
            }
        }

        if let Some(fallback) = &self.fallback {
            tracing::warn!(
                contract = %self.contract_name,
                "retries exhausted — using fallback"
            );
            let provenance = LlmProvenance {
                provider: "fallback".to_string(),
                model: None,
                role: format!("{:?}", self.role),
                duration_ms: 0,
                prompt_chars: 0,
                response_chars: 0,
                attempt: attempts,
            };
            if let Some(hook) = interaction_hook {
                hook(&provenance, false);
            }
            return Ok(EnforcedResponse {
                value: fallback.clone(),
                provenance,
            });
        }

        anyhow::bail!(
            "contract enforcement failed for '{}' after {} attempts",
            self.contract_name,
            attempts
        )
    }
}

pub fn retry_policy_for_role(role: &LlmRole) -> RetryPolicy {
    match role {
        LlmRole::Scaffolding => RetryPolicy {
            max_attempts: 3,
            backoff_ms: 1_000,
        },
        LlmRole::SearchHints => RetryPolicy {
            max_attempts: 2,
            backoff_ms: 500,
        },
        LlmRole::ProseRendering => RetryPolicy {
            max_attempts: 1,
            backoff_ms: 0,
        },
        LlmRole::LeanScaffold => RetryPolicy {
            max_attempts: 2,
            backoff_ms: 1_000,
        },
        LlmRole::Advisory => RetryPolicy {
            max_attempts: 1,
            backoff_ms: 0,
        },
    }
}
