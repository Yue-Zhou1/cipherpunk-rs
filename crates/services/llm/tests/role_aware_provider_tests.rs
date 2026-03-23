use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use llm::provider::LlmCallOutput;
use llm::{
    CompletionOpts, LlmProvider, LlmRole, LlmRoleConfigMap, RoleAwareProvider, RoleConfig,
    llm_call_traced, role_aware_llm_call,
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
        },
        search_hints: RoleConfig {
            provider: Some("anthropic".to_string()),
            model: None,
            temperature_millis: Some(222),
            max_tokens: Some(4321),
        },
        prose_rendering: RoleConfig::default(),
        lean_scaffold: RoleConfig::default(),
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
        },
        search_hints: RoleConfig::default(),
        prose_rendering: RoleConfig::default(),
        lean_scaffold: RoleConfig::default(),
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
        },
        search_hints: RoleConfig {
            provider: Some("secondary".to_string()),
            model: None,
            temperature_millis: Some(333),
            max_tokens: Some(444),
        },
        prose_rendering: RoleConfig::default(),
        lean_scaffold: RoleConfig::default(),
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
        },
        search_hints: RoleConfig::default(),
        prose_rendering: RoleConfig::default(),
        lean_scaffold: RoleConfig::default(),
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
