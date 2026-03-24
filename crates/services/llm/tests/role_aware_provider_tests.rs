use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use llm::provider::LlmCallOutput;
use llm::{
    CompletionOpts, LlmProvider, LlmRole, LlmRoleConfigMap, ProviderFailoverRecord,
    RoleAwareProvider, RoleConfig, llm_call_traced, role_aware_llm_call,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct CallRecord {
    prompt: String,
    opts: CompletionOpts,
    model: String,
}

#[derive(Clone)]
struct RecordingProvider {
    name: &'static str,
    model: &'static str,
    calls: Arc<Mutex<Vec<CallRecord>>>,
}

impl RecordingProvider {
    fn new(name: &'static str, model: &'static str) -> Self {
        Self {
            name,
            model,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn calls(&self) -> Vec<CallRecord> {
        self.calls.lock().expect("calls lock").clone()
    }
}

#[async_trait]
impl LlmProvider for RecordingProvider {
    async fn complete(&self, prompt: &str, opts: &CompletionOpts) -> Result<String> {
        self.calls.lock().expect("calls lock").push(CallRecord {
            prompt: prompt.to_string(),
            opts: opts.clone(),
            model: self.model.to_string(),
        });
        Ok(format!("{}::{prompt}", self.name))
    }

    async fn complete_with_role_and_model(
        &self,
        _role: &LlmRole,
        prompt: &str,
        opts: &CompletionOpts,
        model_override: Option<&str>,
    ) -> Result<LlmCallOutput> {
        let model = model_override.unwrap_or(self.model).to_string();
        self.calls.lock().expect("calls lock").push(CallRecord {
            prompt: prompt.to_string(),
            opts: opts.clone(),
            model: model.clone(),
        });
        Ok(LlmCallOutput {
            response: format!("{}::{prompt}", self.name),
            provider: self.name.to_string(),
            model: Some(model),
        })
    }

    fn name(&self) -> &str {
        self.name
    }

    fn is_available(&self) -> bool {
        true
    }

    fn model(&self) -> Option<&str> {
        Some(self.model)
    }
}

#[derive(Clone)]
struct SequenceProvider {
    name: &'static str,
    model: &'static str,
    calls: Arc<Mutex<Vec<CallRecord>>>,
    responses: Arc<Mutex<VecDeque<Result<String>>>>,
}

impl SequenceProvider {
    fn new(name: &'static str, model: &'static str, responses: Vec<Result<String>>) -> Self {
        Self {
            name,
            model,
            calls: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(responses.into())),
        }
    }

    fn calls(&self) -> Vec<CallRecord> {
        self.calls.lock().expect("calls lock").clone()
    }
}

#[async_trait]
impl LlmProvider for SequenceProvider {
    async fn complete(&self, prompt: &str, opts: &CompletionOpts) -> Result<String> {
        self.calls.lock().expect("calls lock").push(CallRecord {
            prompt: prompt.to_string(),
            opts: opts.clone(),
            model: self.model.to_string(),
        });
        self.responses
            .lock()
            .expect("responses lock")
            .pop_front()
            .unwrap_or_else(|| Err(anyhow!("no configured response")))
    }

    async fn complete_with_role_and_model(
        &self,
        _role: &LlmRole,
        prompt: &str,
        opts: &CompletionOpts,
        model_override: Option<&str>,
    ) -> Result<LlmCallOutput> {
        let model = model_override.unwrap_or(self.model).to_string();
        self.calls.lock().expect("calls lock").push(CallRecord {
            prompt: prompt.to_string(),
            opts: opts.clone(),
            model: model.clone(),
        });
        let response = self
            .responses
            .lock()
            .expect("responses lock")
            .pop_front()
            .unwrap_or_else(|| Err(anyhow!("no configured response")))?;
        Ok(LlmCallOutput {
            response,
            provider: self.name.to_string(),
            model: Some(model),
        })
    }

    fn name(&self) -> &str {
        self.name
    }

    fn is_available(&self) -> bool {
        true
    }

    fn model(&self) -> Option<&str> {
        Some(self.model)
    }
}

#[tokio::test]
async fn role_aware_provider_dispatches_by_role_with_role_specific_opts() {
    let openai = RecordingProvider::new("openai", "gpt-4o-mini");
    let anthropic = RecordingProvider::new("anthropic", "claude-3-5-sonnet");

    let mut providers: HashMap<String, Arc<dyn LlmProvider>> = HashMap::new();
    providers.insert("openai".to_string(), Arc::new(openai.clone()));
    providers.insert("anthropic".to_string(), Arc::new(anthropic.clone()));

    let role_configs = LlmRoleConfigMap {
        scaffolding: RoleConfig {
            provider: Some("openai".to_string()),
            model: None,
            temperature_millis: Some(111),
            max_tokens: Some(1234),
            fallback_chain: vec![],
        },
        search_hints: RoleConfig {
            provider: Some("anthropic".to_string()),
            model: None,
            temperature_millis: Some(222),
            max_tokens: Some(4321),
            fallback_chain: vec![],
        },
        prose_rendering: RoleConfig::default(),
        lean_scaffold: RoleConfig::default(),
        advisory: RoleConfig::default(),
    };

    let provider = RoleAwareProvider::new(providers, role_configs, Arc::new(openai.clone()));

    let (scaffold_text, scaffold_provenance) = provider
        .complete_for_role(LlmRole::Scaffolding, "scaffold")
        .await
        .expect("scaffolding call");
    let (hints_text, hints_provenance) = provider
        .complete_for_role(LlmRole::SearchHints, "hints")
        .await
        .expect("search hints call");

    assert_eq!(scaffold_text, "openai::scaffold");
    assert_eq!(scaffold_provenance.provider, "openai");
    assert_eq!(hints_text, "anthropic::hints");
    assert_eq!(hints_provenance.provider, "anthropic");

    assert_eq!(openai.calls().len(), 1);
    assert_eq!(openai.calls()[0].opts.temperature_millis, 111);
    assert_eq!(openai.calls()[0].opts.max_tokens, 1234);

    assert_eq!(anthropic.calls().len(), 1);
    assert_eq!(anthropic.calls()[0].opts.temperature_millis, 222);
    assert_eq!(anthropic.calls()[0].opts.max_tokens, 4321);
}

#[tokio::test]
async fn role_aware_provider_falls_back_to_default_provider_for_unknown_provider_name() {
    let default_provider = RecordingProvider::new("default", "default-model");
    let secondary = RecordingProvider::new("secondary", "secondary-model");

    let mut providers: HashMap<String, Arc<dyn LlmProvider>> = HashMap::new();
    providers.insert("secondary".to_string(), Arc::new(secondary.clone()));

    let role_configs = LlmRoleConfigMap {
        scaffolding: RoleConfig {
            provider: Some("missing-provider".to_string()),
            model: None,
            temperature_millis: Some(150),
            max_tokens: Some(999),
            fallback_chain: vec![],
        },
        search_hints: RoleConfig::default(),
        prose_rendering: RoleConfig::default(),
        lean_scaffold: RoleConfig::default(),
        advisory: RoleConfig::default(),
    };

    let provider =
        RoleAwareProvider::new(providers, role_configs, Arc::new(default_provider.clone()));
    let (text, provenance) = provider
        .complete_for_role(LlmRole::Scaffolding, "fallback")
        .await
        .expect("fallback call");

    assert_eq!(text, "default::fallback");
    assert_eq!(provenance.provider, "default");
    assert_eq!(default_provider.calls().len(), 1);
    assert!(secondary.calls().is_empty());
}

#[tokio::test]
async fn llm_provider_trait_complete_preserves_default_provider_behavior() {
    let default_provider = RecordingProvider::new("default", "default-model");

    let provider = RoleAwareProvider::new(
        HashMap::new(),
        LlmRoleConfigMap::default(),
        Arc::new(default_provider.clone()),
    );

    let result = <RoleAwareProvider as LlmProvider>::complete(
        &provider,
        "plain-complete",
        &CompletionOpts {
            temperature_millis: 777,
            max_tokens: 888,
        },
    )
    .await
    .expect("plain complete");

    assert_eq!(result, "default::plain-complete");
    let calls = default_provider.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].opts.temperature_millis, 777);
    assert_eq!(calls[0].opts.max_tokens, 888);
}

#[tokio::test]
async fn role_aware_llm_call_delegates_to_role_aware_provider() {
    let default_provider = RecordingProvider::new("default", "default-model");

    let provider = RoleAwareProvider::new(
        HashMap::new(),
        LlmRoleConfigMap::default(),
        Arc::new(default_provider.clone()),
    );

    let (text, provenance) = role_aware_llm_call(&provider, LlmRole::Scaffolding, "hello")
        .await
        .expect("role aware helper call");

    assert_eq!(text, "default::hello");
    assert_eq!(provenance.provider, "default");
}

#[tokio::test]
async fn llm_call_traced_uses_role_dispatch_for_role_aware_trait_object() {
    let primary = RecordingProvider::new("primary", "primary-model");
    let secondary = RecordingProvider::new("secondary", "secondary-model");

    let mut providers: HashMap<String, Arc<dyn LlmProvider>> = HashMap::new();
    providers.insert("primary".to_string(), Arc::new(primary.clone()));
    providers.insert("secondary".to_string(), Arc::new(secondary.clone()));

    let role_configs = LlmRoleConfigMap {
        scaffolding: RoleConfig {
            provider: Some("primary".to_string()),
            model: None,
            temperature_millis: Some(111),
            max_tokens: Some(222),
            fallback_chain: vec![],
        },
        search_hints: RoleConfig {
            provider: Some("secondary".to_string()),
            model: None,
            temperature_millis: Some(333),
            max_tokens: Some(444),
            fallback_chain: vec![],
        },
        prose_rendering: RoleConfig::default(),
        lean_scaffold: RoleConfig::default(),
        advisory: RoleConfig::default(),
    };
    let role_aware = RoleAwareProvider::new(providers, role_configs, Arc::new(primary.clone()));
    let as_trait: Arc<dyn LlmProvider> = Arc::new(role_aware);

    let (_response, provenance) = llm_call_traced(
        as_trait.as_ref(),
        LlmRole::SearchHints,
        "needs secondary",
        &CompletionOpts::default(),
    )
    .await
    .expect("llm_call_traced");

    assert_eq!(provenance.provider, "secondary");
    assert_eq!(secondary.calls().len(), 1);
    assert_eq!(secondary.calls()[0].opts.temperature_millis, 333);
    assert_eq!(secondary.calls()[0].opts.max_tokens, 444);
    assert!(primary.calls().is_empty());
}

#[tokio::test]
async fn role_aware_provider_applies_model_override_from_role_config() {
    let openai = RecordingProvider::new("openai", "gpt-4o-mini");

    let mut providers: HashMap<String, Arc<dyn LlmProvider>> = HashMap::new();
    providers.insert("openai".to_string(), Arc::new(openai.clone()));

    let role_configs = LlmRoleConfigMap {
        scaffolding: RoleConfig {
            provider: Some("openai".to_string()),
            model: Some("gpt-4.1".to_string()),
            temperature_millis: Some(111),
            max_tokens: Some(222),
            fallback_chain: vec![],
        },
        search_hints: RoleConfig::default(),
        prose_rendering: RoleConfig::default(),
        lean_scaffold: RoleConfig::default(),
        advisory: RoleConfig::default(),
    };

    let provider = RoleAwareProvider::new(providers, role_configs, Arc::new(openai));
    let (_response, provenance) = provider
        .complete_for_role(LlmRole::Scaffolding, "model-override")
        .await
        .expect("role-aware call");

    assert_eq!(provenance.provider, "openai");
    assert_eq!(provenance.model.as_deref(), Some("gpt-4.1"));
    assert_eq!(
        provider.role_configs().scaffolding.model.as_deref(),
        Some("gpt-4.1")
    );
}

#[tokio::test]
async fn role_defaults_do_not_override_existing_callsite_opts() {
    let default_provider = RecordingProvider::new("default", "default-model");
    let role_aware = RoleAwareProvider::new(
        HashMap::new(),
        LlmRoleConfigMap::default(),
        Arc::new(default_provider.clone()),
    );
    let as_trait: Arc<dyn LlmProvider> = Arc::new(role_aware);

    let (_response, provenance) = llm_call_traced(
        as_trait.as_ref(),
        LlmRole::ProseRendering,
        "legacy-opts",
        &CompletionOpts {
            temperature_millis: 200,
            max_tokens: 256,
        },
    )
    .await
    .expect("llm_call_traced");

    assert_eq!(provenance.provider, "default");
    let calls = default_provider.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].opts.temperature_millis, 200);
    assert_eq!(calls[0].opts.max_tokens, 256);
}

#[tokio::test]
async fn role_defaults_apply_when_callsite_uses_completion_defaults() {
    let default_provider = RecordingProvider::new("default", "default-model");
    let role_aware = RoleAwareProvider::new(
        HashMap::new(),
        LlmRoleConfigMap::default(),
        Arc::new(default_provider.clone()),
    );
    let as_trait: Arc<dyn LlmProvider> = Arc::new(role_aware);

    let (_response, provenance) = llm_call_traced(
        as_trait.as_ref(),
        LlmRole::ProseRendering,
        "role-default-opts",
        &CompletionOpts::default(),
    )
    .await
    .expect("llm_call_traced");

    assert_eq!(provenance.provider, "default");
    let calls = default_provider.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].opts.temperature_millis, 200);
    assert_eq!(calls[0].opts.max_tokens, 512);
}

#[tokio::test]
async fn transient_primary_failure_uses_fallback_chain_provider() {
    let primary = SequenceProvider::new(
        "openai",
        "gpt-4o-mini",
        vec![Err(anyhow!(
            "OpenAI request failed (503): service unavailable"
        ))],
    );
    let fallback = SequenceProvider::new(
        "template-fallback",
        "template",
        vec![Ok("fallback-ok".to_string())],
    );
    let mut providers: HashMap<String, Arc<dyn LlmProvider>> = HashMap::new();
    providers.insert("openai".to_string(), Arc::new(primary.clone()));
    providers.insert("template-fallback".to_string(), Arc::new(fallback.clone()));
    let role_configs = LlmRoleConfigMap {
        scaffolding: RoleConfig {
            provider: Some("openai".to_string()),
            model: None,
            temperature_millis: Some(100),
            max_tokens: Some(256),
            fallback_chain: vec!["template-fallback".to_string()],
        },
        search_hints: RoleConfig::default(),
        prose_rendering: RoleConfig::default(),
        lean_scaffold: RoleConfig::default(),
        advisory: RoleConfig::default(),
    };
    let provider = RoleAwareProvider::new(providers, role_configs, Arc::new(primary.clone()));

    let (response, provenance) = provider
        .complete_for_role(LlmRole::Scaffolding, "recover-me")
        .await
        .expect("fallback should recover transient failure");

    assert_eq!(response, "fallback-ok");
    assert_eq!(provenance.provider, "template-fallback(failover)");
    assert_eq!(primary.calls().len(), 1);
    assert_eq!(fallback.calls().len(), 1);
}

#[tokio::test]
async fn permanent_primary_failure_does_not_try_fallback_chain() {
    let primary = SequenceProvider::new(
        "openai",
        "gpt-4o-mini",
        vec![Err(anyhow!("OpenAI request failed (401): unauthorized"))],
    );
    let fallback = SequenceProvider::new(
        "template-fallback",
        "template",
        vec![Ok("fallback-ok".to_string())],
    );
    let mut providers: HashMap<String, Arc<dyn LlmProvider>> = HashMap::new();
    providers.insert("openai".to_string(), Arc::new(primary.clone()));
    providers.insert("template-fallback".to_string(), Arc::new(fallback.clone()));
    let role_configs = LlmRoleConfigMap {
        scaffolding: RoleConfig {
            provider: Some("openai".to_string()),
            model: None,
            temperature_millis: Some(100),
            max_tokens: Some(256),
            fallback_chain: vec!["template-fallback".to_string()],
        },
        search_hints: RoleConfig::default(),
        prose_rendering: RoleConfig::default(),
        lean_scaffold: RoleConfig::default(),
        advisory: RoleConfig::default(),
    };
    let provider = RoleAwareProvider::new(providers, role_configs, Arc::new(primary.clone()));

    let err = provider
        .complete_for_role(LlmRole::Scaffolding, "no-recover")
        .await
        .expect_err("permanent failures should not use fallback");
    assert!(err.to_string().contains("401"));
    assert_eq!(primary.calls().len(), 1);
    assert_eq!(fallback.calls().len(), 0);
}

#[tokio::test]
async fn circuit_breaker_skips_fallback_after_threshold() {
    let primary = SequenceProvider::new(
        "openai",
        "gpt-4o-mini",
        vec![
            Err(anyhow!("OpenAI request failed (503): service unavailable")),
            Err(anyhow!("OpenAI request failed (503): service unavailable")),
            Err(anyhow!("OpenAI request failed (503): service unavailable")),
        ],
    );
    let fallback = SequenceProvider::new(
        "template-fallback",
        "template",
        vec![
            Err(anyhow!(
                "Template fallback transient failure (503): unavailable"
            )),
            Err(anyhow!(
                "Template fallback transient failure (503): unavailable"
            )),
            Err(anyhow!(
                "Template fallback transient failure (503): unavailable"
            )),
        ],
    );

    let mut providers: HashMap<String, Arc<dyn LlmProvider>> = HashMap::new();
    providers.insert("openai".to_string(), Arc::new(primary.clone()));
    providers.insert("template-fallback".to_string(), Arc::new(fallback.clone()));
    let role_configs = LlmRoleConfigMap {
        scaffolding: RoleConfig {
            provider: Some("openai".to_string()),
            model: None,
            temperature_millis: Some(100),
            max_tokens: Some(256),
            fallback_chain: vec!["template-fallback".to_string()],
        },
        search_hints: RoleConfig::default(),
        prose_rendering: RoleConfig::default(),
        lean_scaffold: RoleConfig::default(),
        advisory: RoleConfig::default(),
    };
    let provider = RoleAwareProvider::new(providers, role_configs, Arc::new(primary.clone()))
        .with_circuit_breaker_policy(2, Duration::from_secs(300));

    let _ = provider
        .complete_for_role(LlmRole::Scaffolding, "first")
        .await;
    let _ = provider
        .complete_for_role(LlmRole::Scaffolding, "second")
        .await;
    let _ = provider
        .complete_for_role(LlmRole::Scaffolding, "third")
        .await;

    assert_eq!(fallback.calls().len(), 2);
}

#[tokio::test]
async fn circuit_breaker_retries_after_reset_window() {
    let primary = SequenceProvider::new(
        "openai",
        "gpt-4o-mini",
        vec![
            Err(anyhow!("OpenAI request failed (503): service unavailable")),
            Err(anyhow!("OpenAI request failed (503): service unavailable")),
            Err(anyhow!("OpenAI request failed (503): service unavailable")),
        ],
    );
    let fallback = SequenceProvider::new(
        "template-fallback",
        "template",
        vec![
            Err(anyhow!(
                "Template fallback transient failure (503): unavailable"
            )),
            Err(anyhow!(
                "Template fallback transient failure (503): unavailable"
            )),
            Err(anyhow!(
                "Template fallback transient failure (503): unavailable"
            )),
        ],
    );

    let mut providers: HashMap<String, Arc<dyn LlmProvider>> = HashMap::new();
    providers.insert("openai".to_string(), Arc::new(primary.clone()));
    providers.insert("template-fallback".to_string(), Arc::new(fallback.clone()));
    let role_configs = LlmRoleConfigMap {
        scaffolding: RoleConfig {
            provider: Some("openai".to_string()),
            model: None,
            temperature_millis: Some(100),
            max_tokens: Some(256),
            fallback_chain: vec!["template-fallback".to_string()],
        },
        search_hints: RoleConfig::default(),
        prose_rendering: RoleConfig::default(),
        lean_scaffold: RoleConfig::default(),
        advisory: RoleConfig::default(),
    };
    let provider = RoleAwareProvider::new(providers, role_configs, Arc::new(primary.clone()))
        .with_circuit_breaker_policy(1, Duration::from_millis(20));

    let _ = provider
        .complete_for_role(LlmRole::Scaffolding, "first")
        .await;
    let _ = provider
        .complete_for_role(LlmRole::Scaffolding, "second")
        .await;
    assert_eq!(fallback.calls().len(), 1);
    tokio::time::sleep(Duration::from_millis(25)).await;
    let _ = provider
        .complete_for_role(LlmRole::Scaffolding, "third")
        .await;
    assert_eq!(fallback.calls().len(), 2);
}

#[tokio::test]
async fn failover_hook_is_emitted_when_fallback_provider_is_used() {
    let primary = SequenceProvider::new(
        "openai",
        "gpt-4o-mini",
        vec![Err(anyhow!(
            "OpenAI request failed (503): service unavailable"
        ))],
    );
    let fallback = SequenceProvider::new(
        "template-fallback",
        "template",
        vec![Ok("recovered".to_string())],
    );

    let mut providers: HashMap<String, Arc<dyn LlmProvider>> = HashMap::new();
    providers.insert("openai".to_string(), Arc::new(primary.clone()));
    providers.insert("template-fallback".to_string(), Arc::new(fallback.clone()));

    let role_configs = LlmRoleConfigMap {
        scaffolding: RoleConfig {
            provider: Some("openai".to_string()),
            model: None,
            temperature_millis: Some(100),
            max_tokens: Some(256),
            fallback_chain: vec!["template-fallback".to_string()],
        },
        search_hints: RoleConfig::default(),
        prose_rendering: RoleConfig::default(),
        lean_scaffold: RoleConfig::default(),
        advisory: RoleConfig::default(),
    };
    let captured = Arc::new(Mutex::new(Vec::<ProviderFailoverRecord>::new()));
    let captured_hook = Arc::clone(&captured);
    let provider = RoleAwareProvider::new(providers, role_configs, Arc::new(primary.clone()))
        .with_failover_hook(Arc::new(move |record: ProviderFailoverRecord| {
            captured_hook.lock().expect("hook lock").push(record);
        }));

    let (_response, provenance) = provider
        .complete_for_role(LlmRole::Scaffolding, "recover")
        .await
        .expect("fallback should recover");

    assert_eq!(provenance.provider, "template-fallback(failover)");
    let events = captured.lock().expect("hook lock");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].from, "openai");
    assert_eq!(events[0].to, "template-fallback");
    assert_eq!(events[0].role, "Scaffolding");
}
