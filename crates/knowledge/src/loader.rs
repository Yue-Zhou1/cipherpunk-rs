use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::models::{DomainChecklist, ToolPlaybook};

pub fn load_playbooks(dir: &Path) -> Result<Vec<ToolPlaybook>> {
    load_yaml_dir(dir, "playbook")
}

pub fn load_domains(dir: &Path) -> Result<Vec<DomainChecklist>> {
    load_yaml_dir(dir, "domain checklist")
}

fn load_yaml_dir<T>(dir: &Path, label: &str) -> Result<Vec<T>>
where
    T: serde::de::DeserializeOwned,
{
    let mut files = fs::read_dir(dir)
        .with_context(|| format!("read {} directory {}", label, dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("collect {label} directory entries"))?;
    files.sort_by_key(|entry| entry.path());

    let mut parsed = Vec::<T>::new();
    for entry in files {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("yaml") {
            continue;
        }

        let content =
            fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let item = serde_yaml::from_str::<T>(&content)
            .with_context(|| format!("parse {} yaml {}", label, path.display()))?;
        parsed.push(item);
    }

    Ok(parsed)
}
