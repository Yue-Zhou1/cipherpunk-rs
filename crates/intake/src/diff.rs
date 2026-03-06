use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::{Context, Result};
use audit_agent_core::finding::Finding;
use audit_agent_core::workspace::CargoWorkspace;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TaskId {
    AnalyzeCrate(String),
    AnalyzeFile(PathBuf),
}

#[derive(Debug, Clone)]
pub struct DiffAnalysis {
    pub base_commit: String,
    pub head_commit: String,
    pub affected_crates: Vec<String>,
    pub affected_files: Vec<PathBuf>,
    pub full_rerun_required: bool,
    pub rerun_tasks: Vec<TaskId>,
    pub cached_findings: Vec<Finding>,
    pub cache_hit_rate: f32,
}

pub struct AnalysisCache {
    inner: RwLock<HashMap<String, Vec<Finding>>>,
    persistent_db: Option<sled::Db>,
}

impl Default for AnalysisCache {
    fn default() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
            persistent_db: None,
        }
    }
}

impl AnalysisCache {
    pub fn open(path: &Path) -> Result<Self> {
        const LOCK_RETRY_ATTEMPTS: usize = 10;
        const LOCK_RETRY_DELAY: Duration = Duration::from_millis(25);

        let mut retries = 0;
        let db = loop {
            match sled::open(path) {
                Ok(db) => break db,
                Err(err) => {
                    let lock_contended = matches!(
                        &err,
                        sled::Error::Io(io_err)
                            if io_err.kind() == std::io::ErrorKind::WouldBlock
                                || io_err.to_string().contains("could not acquire lock")
                    );
                    if lock_contended && retries < LOCK_RETRY_ATTEMPTS {
                        retries += 1;
                        std::thread::sleep(LOCK_RETRY_DELAY);
                        continue;
                    }
                    return Err(err).with_context(|| format!("open sled db {}", path.display()));
                }
            }
        };
        Ok(Self {
            inner: RwLock::new(HashMap::new()),
            persistent_db: Some(db),
        })
    }

    pub fn insert(&self, commit: &str, findings: &[Finding]) {
        if let Ok(mut guard) = self.inner.write() {
            guard.insert(commit.to_string(), findings.to_vec());
        }
        if let Some(db) = &self.persistent_db
            && let Ok(encoded) = serde_json::to_vec(findings)
        {
            let _ = db.insert(commit.as_bytes(), encoded);
            let _ = db.flush();
        }
    }

    pub fn get(&self, commit: &str) -> Vec<Finding> {
        if let Some(cached) = self
            .inner
            .read()
            .ok()
            .and_then(|guard| guard.get(commit).cloned())
        {
            return cached;
        }

        let Some(db) = &self.persistent_db else {
            return vec![];
        };
        let Ok(Some(bytes)) = db.get(commit.as_bytes()) else {
            return vec![];
        };
        let Ok(findings) = serde_json::from_slice::<Vec<Finding>>(&bytes) else {
            return vec![];
        };
        if let Ok(mut guard) = self.inner.write() {
            guard.insert(commit.to_string(), findings.clone());
        }
        findings
    }
}

pub struct DiffModeAnalyzer {
    repo_root: PathBuf,
    workspace: CargoWorkspace,
    cache: Arc<AnalysisCache>,
}

impl DiffModeAnalyzer {
    pub fn new(repo_root: PathBuf, workspace: CargoWorkspace, cache: Arc<AnalysisCache>) -> Self {
        Self {
            repo_root,
            workspace,
            cache,
        }
    }

    pub fn compute_diff(&self, base: &str, head: &str) -> Result<DiffAnalysis> {
        let changed_files = changed_files(&self.repo_root, base, head)?;
        let full_rerun_required = changed_files
            .iter()
            .any(|path| path.file_name().and_then(|name| name.to_str()) == Some("Cargo.toml"));

        let affected_crates = if full_rerun_required {
            self.workspace
                .members
                .iter()
                .map(|member| member.name.clone())
                .collect::<Vec<_>>()
        } else {
            let mut affected = BTreeSet::new();
            for changed in &changed_files {
                for member in &self.workspace.members {
                    let Some(relative_member_path) = member.path.strip_prefix(&self.repo_root).ok()
                    else {
                        continue;
                    };
                    if changed.starts_with(relative_member_path) {
                        affected.insert(member.name.clone());
                    }
                }
            }
            affected.into_iter().collect::<Vec<_>>()
        };

        let rerun_tasks = if full_rerun_required {
            affected_crates
                .iter()
                .cloned()
                .map(TaskId::AnalyzeCrate)
                .collect::<Vec<_>>()
        } else {
            changed_files
                .iter()
                .cloned()
                .map(TaskId::AnalyzeFile)
                .collect::<Vec<_>>()
        };

        let changed_set = affected_crates.iter().cloned().collect::<HashSet<_>>();
        let previous = self.cache.get(base);
        let previous_total = previous.len();
        let mut cached_findings = Vec::<Finding>::new();
        for mut finding in previous {
            let finding_crate = finding
                .affected_components
                .first()
                .map(|component| component.crate_name.clone())
                .unwrap_or_default();
            if full_rerun_required || changed_set.contains(&finding_crate) {
                continue;
            }
            finding
                .evidence
                .tool_versions
                .insert("analysis_origin".to_string(), "cache".to_string());
            cached_findings.push(finding);
        }

        let cache_hit_rate = if previous_total == 0 {
            0.0
        } else {
            cached_findings.len() as f32 / previous_total as f32
        };

        Ok(DiffAnalysis {
            base_commit: base.to_string(),
            head_commit: head.to_string(),
            affected_crates,
            affected_files: changed_files,
            full_rerun_required,
            rerun_tasks,
            cached_findings,
            cache_hit_rate,
        })
    }
}

fn changed_files(repo_root: &Path, base: &str, head: &str) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("diff")
        .arg("--name-only")
        .arg(format!("{base}..{head}"))
        .output()
        .with_context(|| {
            format!(
                "failed to run git diff in {} for range {}..{}",
                repo_root.display(),
                base,
                head
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git diff failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut unique = BTreeSet::<PathBuf>::new();
    for path in stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(PathBuf::from)
    {
        unique.insert(path);
    }
    Ok(unique.into_iter().collect())
}
