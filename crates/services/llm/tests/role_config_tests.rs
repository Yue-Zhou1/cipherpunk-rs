use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use llm::{LlmRoleConfigMap, RoleConfig};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn default_role_config_map_matches_expected_defaults() {
    let config = LlmRoleConfigMap::default();

    assert_eq!(config.scaffolding.temperature_millis, Some(100));
    assert_eq!(config.scaffolding.max_tokens, Some(1024));

    assert_eq!(config.search_hints.temperature_millis, Some(100));
    assert_eq!(config.search_hints.max_tokens, Some(1024));

    assert_eq!(config.prose_rendering.temperature_millis, Some(200));
    assert_eq!(config.prose_rendering.max_tokens, Some(512));

    assert_eq!(config.lean_scaffold.temperature_millis, Some(200));
    assert_eq!(config.lean_scaffold.max_tokens, Some(1024));
}

#[test]
fn from_env_applies_role_overrides() {
    let _guard = env_lock().lock().expect("env lock");
    let vars = [
        "LLM_ROLE_SCAFFOLDING_PROVIDER",
        "LLM_ROLE_SCAFFOLDING_MODEL",
        "LLM_ROLE_SCAFFOLDING_TEMPERATURE",
        "LLM_ROLE_SCAFFOLDING_MAX_TOKENS",
        "LLM_ROLE_PROSE_RENDERING_MODEL",
        "LLM_ROLE_PROSE_RENDERING_TEMPERATURE",
    ];
    let snapshot = snapshot_env(&vars);

    // SAFETY: test-local env mutation guarded by env lock.
    unsafe {
        std::env::set_var("LLM_ROLE_SCAFFOLDING_PROVIDER", "openai");
        std::env::set_var("LLM_ROLE_SCAFFOLDING_MODEL", "gpt-4o-mini");
        std::env::set_var("LLM_ROLE_SCAFFOLDING_TEMPERATURE", "333");
        std::env::set_var("LLM_ROLE_SCAFFOLDING_MAX_TOKENS", "4096");
        std::env::set_var("LLM_ROLE_PROSE_RENDERING_MODEL", "claude-3-5-sonnet");
        std::env::set_var("LLM_ROLE_PROSE_RENDERING_TEMPERATURE", "250");
    }

    let config = LlmRoleConfigMap::from_env();
    assert_eq!(config.scaffolding.provider.as_deref(), Some("openai"));
    assert_eq!(config.scaffolding.model.as_deref(), Some("gpt-4o-mini"));
    assert_eq!(config.scaffolding.temperature_millis, Some(333));
    assert_eq!(config.scaffolding.max_tokens, Some(4096));
    assert_eq!(
        config.prose_rendering.model.as_deref(),
        Some("claude-3-5-sonnet")
    );
    assert_eq!(config.prose_rendering.temperature_millis, Some(250));

    restore_env(snapshot);
}

#[test]
fn from_env_ignores_invalid_numeric_values() {
    let _guard = env_lock().lock().expect("env lock");
    let vars = [
        "LLM_ROLE_SEARCH_HINTS_TEMPERATURE",
        "LLM_ROLE_SEARCH_HINTS_MAX_TOKENS",
    ];
    let snapshot = snapshot_env(&vars);

    // SAFETY: test-local env mutation guarded by env lock.
    unsafe {
        std::env::set_var("LLM_ROLE_SEARCH_HINTS_TEMPERATURE", "NaN");
        std::env::set_var("LLM_ROLE_SEARCH_HINTS_MAX_TOKENS", "oops");
    }

    let config = LlmRoleConfigMap::from_env();
    assert_eq!(config.search_hints.temperature_millis, Some(100));
    assert_eq!(config.search_hints.max_tokens, Some(1024));

    restore_env(snapshot);
}

#[test]
fn merge_yaml_overrides_selected_values_only() {
    let mut config = LlmRoleConfigMap::default();

    let mut yaml_roles = HashMap::new();
    yaml_roles.insert(
        "prose_rendering".to_string(),
        RoleConfig {
            provider: Some("anthropic".to_string()),
            model: Some("claude-3-5-sonnet".to_string()),
            temperature_millis: Some(275),
            max_tokens: None,
        },
    );
    yaml_roles.insert(
        "search_hints".to_string(),
        RoleConfig {
            provider: None,
            model: None,
            temperature_millis: None,
            max_tokens: Some(2048),
        },
    );

    config.merge_yaml(&yaml_roles);

    assert_eq!(
        config.prose_rendering.provider.as_deref(),
        Some("anthropic")
    );
    assert_eq!(
        config.prose_rendering.model.as_deref(),
        Some("claude-3-5-sonnet")
    );
    assert_eq!(config.prose_rendering.temperature_millis, Some(275));
    assert_eq!(config.prose_rendering.max_tokens, Some(512));
    assert_eq!(config.search_hints.max_tokens, Some(2048));
}

#[test]
fn yaml_merge_overrides_env_values() {
    let _guard = env_lock().lock().expect("env lock");
    let vars = [
        "LLM_ROLE_SCAFFOLDING_MODEL",
        "LLM_ROLE_SCAFFOLDING_MAX_TOKENS",
    ];
    let snapshot = snapshot_env(&vars);

    // SAFETY: test-local env mutation guarded by env lock.
    unsafe {
        std::env::set_var("LLM_ROLE_SCAFFOLDING_MODEL", "gpt-4o-mini");
        std::env::set_var("LLM_ROLE_SCAFFOLDING_MAX_TOKENS", "1500");
    }

    let mut config = LlmRoleConfigMap::from_env();
    let mut yaml_roles = HashMap::new();
    yaml_roles.insert(
        "scaffolding".to_string(),
        RoleConfig {
            provider: None,
            model: Some("gpt-4.1".to_string()),
            temperature_millis: None,
            max_tokens: Some(2048),
        },
    );
    config.merge_yaml(&yaml_roles);

    assert_eq!(config.scaffolding.model.as_deref(), Some("gpt-4.1"));
    assert_eq!(config.scaffolding.max_tokens, Some(2048));

    restore_env(snapshot);
}

fn snapshot_env(keys: &[&str]) -> Vec<(String, Option<String>)> {
    keys.iter()
        .map(|k| ((*k).to_string(), std::env::var(k).ok()))
        .collect()
}

fn restore_env(state: Vec<(String, Option<String>)>) {
    for (key, value) in state {
        match value {
            Some(v) => {
                // SAFETY: restoring test-local env vars.
                unsafe { std::env::set_var(key, v) };
            }
            None => {
                // SAFETY: restoring test-local env vars.
                unsafe { std::env::remove_var(key) };
            }
        }
    }
}
