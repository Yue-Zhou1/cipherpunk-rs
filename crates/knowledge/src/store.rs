use std::fs;
use std::io::Write;
use std::path::Path;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::models::{AdjudicatedCase, ReproPattern, ToolSequence};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KnowledgeStore {
    true_positives: Vec<AdjudicatedCase>,
    false_positives: Vec<AdjudicatedCase>,
    tool_sequences: Vec<ToolSequence>,
    repro_patterns: Vec<ReproPattern>,
}

impl KnowledgeStore {
    pub fn load_from_path(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path)
            .with_context(|| format!("read knowledge feedback store {}", path.display()))?;
        serde_yaml::from_str(&raw)
            .with_context(|| format!("parse knowledge feedback store {}", path.display()))
    }

    pub fn persist_to_path(&self, path: &Path) -> Result<()> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .with_context(|| format!("create knowledge store dir {}", parent.display()))?;

        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("knowledge-feedback.yaml");
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let tmp_path = parent.join(format!(".{file_name}.tmp-{}-{nanos}", process::id()));

        let serialized =
            serde_yaml::to_string(self).context("serialize knowledge feedback store")?;
        let mut tmp_file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp_path)
            .with_context(|| {
                format!(
                    "create temporary knowledge feedback store {}",
                    tmp_path.display()
                )
            })?;
        tmp_file
            .write_all(serialized.as_bytes())
            .with_context(|| format!("write temporary knowledge store {}", tmp_path.display()))?;
        tmp_file
            .sync_all()
            .with_context(|| format!("sync temporary knowledge store {}", tmp_path.display()))?;

        fs::rename(&tmp_path, path)
            .with_context(|| format!("replace knowledge feedback store {}", path.display()))
            .inspect_err(|_| {
                let _ = fs::remove_file(&tmp_path);
            })
    }

    pub fn ingest_true_positive(&mut self, case: AdjudicatedCase) {
        self.true_positives.push(case);
    }

    pub fn ingest_false_positive(&mut self, case: AdjudicatedCase) {
        self.false_positives.push(case);
    }

    pub fn ingest_tool_sequence(&mut self, sequence: ToolSequence) {
        self.tool_sequences.push(sequence);
    }

    pub fn ingest_repro_pattern(&mut self, pattern: ReproPattern) {
        self.repro_patterns.push(pattern);
    }

    pub fn true_positives(&self) -> &[AdjudicatedCase] {
        &self.true_positives
    }

    pub fn false_positives(&self) -> &[AdjudicatedCase] {
        &self.false_positives
    }

    pub fn tool_sequences(&self) -> &[ToolSequence] {
        &self.tool_sequences
    }

    pub fn repro_patterns(&self) -> &[ReproPattern] {
        &self.repro_patterns
    }
}
