use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::{Deserialize, Serialize};

const MAX_ENTRIES: usize = 100;

/// Severity distribution for one completed audit.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct FindingSeverityCounts {
    pub critical: u32,
    pub high: u32,
    pub medium: u32,
    pub low: u32,
    pub observation: u32,
}

/// Compressed summary of a completed audit suitable for cross-session recall.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditMemoryEntry {
    pub audit_id: String,
    pub timestamp: String,
    pub source_description: String,
    pub findings_by_severity: FindingSeverityCounts,
    pub engines_used: Vec<String>,
    pub key_findings: Vec<String>,
    pub tags: Vec<String>,
}

/// Persistent memory for cross-session learning.
#[derive(Debug, Default, Clone)]
pub struct LongTermMemory {
    entries: Vec<AuditMemoryEntry>,
    store_path: Option<PathBuf>,
}

impl LongTermMemory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_from_path(path: &Path) -> Result<Self> {
        let memory_path = path.join("long_term_memory.json");
        if memory_path.exists() {
            let content = std::fs::read_to_string(&memory_path)?;
            let entries: Vec<AuditMemoryEntry> = serde_json::from_str(&content)?;
            return Ok(Self {
                entries,
                store_path: Some(memory_path),
            });
        }

        Ok(Self {
            entries: Vec::new(),
            store_path: Some(memory_path),
        })
    }

    pub fn record_audit_outcome(&mut self, entry: AuditMemoryEntry) {
        self.entries
            .retain(|existing| existing.audit_id != entry.audit_id);
        self.entries.push(entry);

        if self.entries.len() > MAX_ENTRIES {
            let overflow = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(0..overflow);
        }
    }

    pub fn recall_similar(&self, context_tags: &[String], limit: usize) -> Vec<&AuditMemoryEntry> {
        if limit == 0 {
            return Vec::new();
        }

        let context = context_tags
            .iter()
            .map(|tag| tag.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();

        let mut scored = self
            .entries
            .iter()
            .map(|entry| {
                let score = entry
                    .tags
                    .iter()
                    .filter(|tag| context.contains(&tag.to_ascii_lowercase()))
                    .count();
                (score, entry)
            })
            .filter(|(score, _)| *score > 0)
            .collect::<Vec<_>>();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored
            .into_iter()
            .take(limit)
            .map(|(_, entry)| entry)
            .collect()
    }

    pub fn persist(&self) -> Result<()> {
        let Some(path) = self.store_path.as_ref() else {
            return Ok(());
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&self.entries)?;
        atomic_write(path, content.as_bytes())?;
        Ok(())
    }

    pub fn entries(&self) -> &[AuditMemoryEntry] {
        &self.entries
    }
}

fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let temp_path = make_temp_path(path);
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp_path)?;
    file.write_all(content)?;
    file.sync_all()?;
    drop(file);

    if let Err(err) = std::fs::rename(&temp_path, path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(err.into());
    }

    Ok(())
}

fn make_temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("long_term_memory.json");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let temp_name = format!("{file_name}.{nanos}.tmp");
    path.with_file_name(temp_name)
}
