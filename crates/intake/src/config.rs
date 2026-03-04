use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use audit_agent_core::audit_config::BudgetConfig;
use serde::Deserialize;

pub struct ConfigParser;

#[derive(Debug, Clone, Deserialize)]
pub struct RawAuditConfig {
    pub source: RawSource,
    pub scope: Option<RawScope>,
    pub engines: Option<RawEngineConfig>,
    pub budget: Option<RawBudgetConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawSource {
    pub url: Option<String>,
    pub local_path: Option<String>,
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawScope {
    pub target_crates: Option<Vec<String>>,
    pub exclude_crates: Option<Vec<String>>,
    pub features: Option<Vec<Vec<String>>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawEngineConfig {
    pub crypto_zk: Option<bool>,
    pub distributed: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawBudgetConfig {
    pub kani_timeout_secs: Option<u64>,
    pub z3_timeout_secs: Option<u64>,
    pub fuzz_duration_secs: Option<u64>,
    pub madsim_ticks: Option<u64>,
    pub max_llm_retries: Option<u8>,
}

#[derive(Debug, Clone)]
pub struct ValidatedConfig {
    pub source: RawSource,
    pub scope: RawScope,
    pub engines: RawEngineConfig,
    pub budget: BudgetConfig,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    MissingField {
        field: String,
    },
    InvalidCommitHash {
        value: String,
    },
    BranchNameNotAllowed {
        branch: String,
        hint: String,
    },
    UnknownCrate {
        crate_name: String,
        available: Vec<String>,
    },
    InvalidBudgetValue {
        field: String,
        value: u64,
        reason: String,
    },
    ConflictingOptions {
        field_a: String,
        field_b: String,
    },
    ParseError {
        reason: String,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ConfigError {}

impl ConfigParser {
    pub fn parse(path: &Path) -> Result<ValidatedConfig, Vec<ConfigError>> {
        let content = fs::read_to_string(path).map_err(|err| {
            vec![ConfigError::ParseError {
                reason: format!("failed to read file: {err}"),
            }]
        })?;

        let raw: RawAuditConfig = serde_yaml::from_str(&content).map_err(|err| {
            vec![ConfigError::ParseError {
                reason: format!("invalid yaml: {err}"),
            }]
        })?;

        Self::validate(raw)
    }

    pub fn validate(raw: RawAuditConfig) -> Result<ValidatedConfig, Vec<ConfigError>> {
        let mut errors = vec![];

        let has_url = raw.source.url.is_some();
        let has_local = raw.source.local_path.is_some();
        match (has_url, has_local) {
            (false, false) => errors.push(ConfigError::MissingField {
                field: "source.url or source.local_path".to_string(),
            }),
            (true, true) => errors.push(ConfigError::ConflictingOptions {
                field_a: "source.url".to_string(),
                field_b: "source.local_path".to_string(),
            }),
            _ => {}
        }

        if has_url {
            match raw.source.commit.as_deref() {
                None => errors.push(ConfigError::MissingField {
                    field: "source.commit".to_string(),
                }),
                Some(commit) => {
                    if is_branch_like(commit) {
                        errors.push(ConfigError::BranchNameNotAllowed {
                            branch: commit.to_string(),
                            hint: "Provide a full 40-character SHA".to_string(),
                        });
                    } else if !is_sha(commit) {
                        errors.push(ConfigError::InvalidCommitHash {
                            value: commit.to_string(),
                        });
                    }
                }
            }
        }

        if let Some(scope) = raw.scope.as_ref() {
            if let Some(target_crates) = scope.target_crates.as_ref() {
                if target_crates.is_empty() {
                    errors.push(ConfigError::MissingField {
                        field: "scope.target_crates (must be non-empty when provided)".to_string(),
                    });
                }
            }
        }

        let budget = raw.budget.clone().unwrap_or(RawBudgetConfig {
            kani_timeout_secs: Some(300),
            z3_timeout_secs: Some(600),
            fuzz_duration_secs: Some(3600),
            madsim_ticks: Some(100_000),
            max_llm_retries: Some(3),
        });

        validate_u64(
            "budget.kani_timeout_secs",
            budget.kani_timeout_secs,
            &mut errors,
        );
        validate_u64(
            "budget.z3_timeout_secs",
            budget.z3_timeout_secs,
            &mut errors,
        );
        validate_u64(
            "budget.fuzz_duration_secs",
            budget.fuzz_duration_secs,
            &mut errors,
        );
        validate_u64("budget.madsim_ticks", budget.madsim_ticks, &mut errors);
        validate_u8(
            "budget.max_llm_retries",
            budget.max_llm_retries,
            &mut errors,
        );

        if !errors.is_empty() {
            return Err(errors);
        }

        Ok(ValidatedConfig {
            source: raw.source,
            scope: raw.scope.unwrap_or(RawScope {
                target_crates: None,
                exclude_crates: None,
                features: None,
            }),
            engines: raw.engines.unwrap_or(RawEngineConfig {
                crypto_zk: Some(true),
                distributed: Some(false),
            }),
            budget: BudgetConfig {
                kani_timeout_secs: budget.kani_timeout_secs.unwrap_or(300),
                z3_timeout_secs: budget.z3_timeout_secs.unwrap_or(600),
                fuzz_duration_secs: budget.fuzz_duration_secs.unwrap_or(3600),
                madsim_ticks: budget.madsim_ticks.unwrap_or(100_000),
                max_llm_retries: budget.max_llm_retries.unwrap_or(3),
            },
            output_dir: PathBuf::from("audit-output"),
        })
    }
}

fn validate_u64(field: &str, value: Option<u64>, errors: &mut Vec<ConfigError>) {
    if let Some(value) = value {
        if value == 0 {
            errors.push(ConfigError::InvalidBudgetValue {
                field: field.to_string(),
                value,
                reason: "must be > 0".to_string(),
            });
        }
    }
}

fn validate_u8(field: &str, value: Option<u8>, errors: &mut Vec<ConfigError>) {
    if let Some(value) = value {
        if value == 0 {
            errors.push(ConfigError::InvalidBudgetValue {
                field: field.to_string(),
                value: value as u64,
                reason: "must be > 0".to_string(),
            });
        }
    }
}

fn is_sha(value: &str) -> bool {
    value.len() == 40 && value.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_branch_like(value: &str) -> bool {
    !is_sha(value)
}
