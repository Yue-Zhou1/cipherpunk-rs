use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::provider::{
    CompletionOpts, LlmCallOutput, LlmProvenance, LlmProvider, LlmRole, TemplateFallback,
    llm_call_traced,
};
use crate::provider::{anthropic_provider, ollama_provider, openai_provider};

/// Per-role LLM parameters. Fields are optional role-specific overrides.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub temperature_millis: Option<u16>,
    pub max_tokens: Option<u32>,
}

/// Complete role configuration map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

impl Default for LlmRoleConfigMap {
    fn default() -> Self {
        Self {
            scaffolding: RoleConfig {
                provider: None,
                model: None,
                temperature_millis: Some(100),
                max_tokens: Some(1024),
            },
            search_hints: RoleConfig {
                provider: None,
                model: None,
                temperature_millis: Some(100),
                max_tokens: Some(1024),
            },
            prose_rendering: RoleConfig {
                provider: None,
                model: None,
                temperature_millis: Some(200),
                max_tokens: Some(512),
            },
            lean_scaffold: RoleConfig {
                provider: None,
                model: None,
                temperature_millis: Some(200),
                max_tokens: Some(1024),
            },
        }
    }
}

impl LlmRoleConfigMap {
    /// Load role overrides from environment variables.
    /// Pattern: LLM_ROLE_{ROLE}_{PARAM}.
    pub fn from_env() -> Self {
        let mut config = Self::default();
        load_role_from_env("SCAFFOLDING", &mut config.scaffolding);
        load_role_from_env("SEARCH_HINTS", &mut config.search_hints);
        load_role_from_env("PROSE_RENDERING", &mut config.prose_rendering);
        load_role_from_env("LEAN_SCAFFOLD", &mut config.lean_scaffold);
        config
    }

    /// Merge role config from an external YAML role map.
    /// Overlay values override existing values.
    pub fn merge_yaml(&mut self, yaml_roles: &HashMap<String, RoleConfig>) {
        if let Some(rc) = yaml_roles.get("scaffolding") {
            merge_role(&mut self.scaffolding, rc);
        }
        if let Some(rc) = yaml_roles.get("search_hints") {
            merge_role(&mut self.search_hints, rc);
        }
        if let Some(rc) = yaml_roles.get("prose_rendering") {
            merge_role(&mut self.prose_rendering, rc);
        }
        if let Some(rc) = yaml_roles.get("lean_scaffold") {
            merge_role(&mut self.lean_scaffold, rc);
        }
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

fn merge_role(base: &mut RoleConfig, overlay: &RoleConfig) {
    if let Some(provider) = &overlay.provider {
        base.provider = Some(provider.clone());
    }
    if let Some(model) = &overlay.model {
        base.model = Some(model.clone());
    }
    if let Some(temperature_millis) = overlay.temperature_millis {
        base.temperature_millis = Some(temperature_millis);
    }
    if let Some(max_tokens) = overlay.max_tokens {
        base.max_tokens = Some(max_tokens);
    }
}

pub struct RoleAwareProvider {
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
        Self {
            providers,
            role_configs,
            default_provider,
        }
    }

    fn config_for_role(&self, role: &LlmRole) -> &RoleConfig {
        match role {
            LlmRole::Scaffolding => &self.role_configs.scaffolding,
            LlmRole::SearchHints => &self.role_configs.search_hints,
            LlmRole::ProseRendering => &self.role_configs.prose_rendering,
            LlmRole::LeanScaffold => &self.role_configs.lean_scaffold,
        }
    }

    fn resolve_for_role(
        &self,
        role: &LlmRole,
        fallback_opts: &CompletionOpts,
    ) -> (Arc<dyn LlmProvider>, CompletionOpts, Option<String>) {
        let rc = self.config_for_role(role);
        let defaults = Self::default_config_for_role(role);
        let caller_opts_are_default = fallback_opts == &CompletionOpts::default();

        let provider = rc
            .provider
            .as_ref()
            .and_then(|name| self.providers.get(name))
            .cloned()
            .unwrap_or_else(|| self.default_provider.clone());

        let opts = CompletionOpts {
            // Preserve legacy call-site behavior when role config remains at built-in defaults.
            temperature_millis: if caller_opts_are_default
                || rc.temperature_millis != defaults.temperature_millis
            {
                rc.temperature_millis
                    .unwrap_or(fallback_opts.temperature_millis)
            } else {
                fallback_opts.temperature_millis
            },
            // Preserve legacy call-site behavior when role config remains at built-in defaults.
            max_tokens: if caller_opts_are_default || rc.max_tokens != defaults.max_tokens {
                rc.max_tokens.unwrap_or(fallback_opts.max_tokens)
            } else {
                fallback_opts.max_tokens
            },
        };

        (provider, opts, rc.model.clone())
    }

    fn default_config_for_role(role: &LlmRole) -> RoleConfig {
        let defaults = LlmRoleConfigMap::default();
        match role {
            LlmRole::Scaffolding => defaults.scaffolding,
            LlmRole::SearchHints => defaults.search_hints,
            LlmRole::ProseRendering => defaults.prose_rendering,
            LlmRole::LeanScaffold => defaults.lean_scaffold,
        }
    }

    pub async fn complete_for_role(
        &self,
        role: LlmRole,
        prompt: &str,
    ) -> anyhow::Result<(String, LlmProvenance)> {
        llm_call_traced(self, role, prompt, &CompletionOpts::default()).await
    }

    pub fn role_configs(&self) -> &LlmRoleConfigMap {
        &self.role_configs
    }

    pub fn apply_yaml_overrides(&mut self, yaml_roles: &HashMap<String, RoleConfig>) {
        self.role_configs.merge_yaml(yaml_roles);
    }
}

#[async_trait]
impl LlmProvider for RoleAwareProvider {
    async fn complete(&self, prompt: &str, opts: &CompletionOpts) -> anyhow::Result<String> {
        LlmProvider::complete(self.default_provider.as_ref(), prompt, opts).await
    }

    async fn complete_with_role(
        &self,
        role: &LlmRole,
        prompt: &str,
        opts: &CompletionOpts,
    ) -> anyhow::Result<LlmCallOutput> {
        let (provider, resolved_opts, model_override) = self.resolve_for_role(role, opts);
        LlmProvider::complete_with_role_and_model(
            provider.as_ref(),
            role,
            prompt,
            &resolved_opts,
            model_override.as_deref(),
        )
        .await
    }

    fn name(&self) -> &str {
        "role-aware"
    }

    fn is_available(&self) -> bool {
        self.default_provider.is_available()
    }

    fn model(&self) -> Option<&str> {
        self.default_provider.model()
    }
}

pub async fn role_aware_llm_call(
    provider: &dyn LlmProvider,
    role: LlmRole,
    prompt: &str,
) -> anyhow::Result<(String, LlmProvenance)> {
    llm_call_traced(provider, role, prompt, &CompletionOpts::default()).await
}

pub fn role_aware_provider_from_env() -> RoleAwareProvider {
    let mut providers: HashMap<String, Arc<dyn LlmProvider>> = HashMap::new();
    let mut default_provider: Arc<dyn LlmProvider> = Arc::new(TemplateFallback);

    if let Some(provider) = openai_provider() {
        let provider = Arc::new(provider) as Arc<dyn LlmProvider>;
        default_provider = provider.clone();
        providers.insert("openai".to_string(), provider);
    }

    if let Some(provider) = anthropic_provider() {
        let provider = Arc::new(provider) as Arc<dyn LlmProvider>;
        if providers.is_empty() {
            default_provider = provider.clone();
        }
        providers.insert("anthropic".to_string(), provider);
    }

    if let Some(provider) = ollama_provider() {
        let provider = Arc::new(provider) as Arc<dyn LlmProvider>;
        if providers.is_empty() {
            default_provider = provider.clone();
        }
        providers.insert("ollama".to_string(), provider);
    }

    providers.insert(
        "template-fallback".to_string(),
        Arc::new(TemplateFallback) as Arc<dyn LlmProvider>,
    );

    RoleAwareProvider::new(providers, LlmRoleConfigMap::from_env(), default_provider)
}
