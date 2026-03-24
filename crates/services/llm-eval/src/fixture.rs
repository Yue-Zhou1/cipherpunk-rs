use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateFallbackSupport {
    Required,
    Supported,
    Skip,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalFixture {
    pub id: String,
    pub role: String,
    pub prompt: String,
    pub template_fallback: TemplateFallbackSupport,
    pub assertions: Vec<EvalAssertion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EvalAssertion {
    JsonValid,
    ContainsKeyword { value: String },
    NotContainsKeyword { value: String },
    ParsesAsContract { contract: String },
    FieldNotEmpty { path: String },
    MaxChars { value: usize },
    MinChars { value: usize },
}

pub fn load_fixtures_from_dir(dir: &Path) -> Result<Vec<EvalFixture>> {
    let mut fixture_paths = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("read fixture dir {}", dir.display()))? {
        let entry = entry.with_context(|| format!("read fixture entry in {}", dir.display()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        if ext == "yaml" || ext == "yml" {
            fixture_paths.push(path);
        }
    }

    fixture_paths.sort();

    let mut fixtures = Vec::new();
    for path in fixture_paths {
        let content = fs::read_to_string(&path)
            .with_context(|| format!("read fixture file {}", path.display()))?;
        let mut parsed: Vec<EvalFixture> = serde_yaml::from_str(&content)
            .with_context(|| format!("parse fixture yaml {}", path.display()))?;
        fixtures.append(&mut parsed);
    }

    Ok(fixtures)
}
