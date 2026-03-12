use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use anyhow::{Context, Result};

pub mod loader;
pub mod models;
pub mod store;

use loader::{load_domains, load_playbooks};
use models::{AdjudicatedCase, DomainChecklist, ReproPattern, ToolPlaybook, ToolSequence};
use store::KnowledgeStore;

#[derive(Debug, Clone)]
pub struct KnowledgeBase {
    playbooks: Vec<ToolPlaybook>,
    domains: BTreeMap<String, DomainChecklist>,
    store: KnowledgeStore,
}

impl KnowledgeBase {
    pub fn load_from_repo_root() -> Result<Self> {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|value| value.parent())
            .context("resolve repository root")?
            .to_path_buf();

        let playbooks = load_playbooks(&repo_root.join("knowledge/playbooks"))?;
        let domains = load_domains(&repo_root.join("knowledge/domains"))?
            .into_iter()
            .map(|domain| (domain.id.clone(), domain))
            .collect::<BTreeMap<_, _>>();

        Ok(Self {
            playbooks,
            domains,
            store: KnowledgeStore::default(),
        })
    }

    pub fn route_tools(&self, context: &[String]) -> Vec<String> {
        let context = context
            .iter()
            .map(|value| normalize(value))
            .collect::<BTreeSet<_>>();

        let mut routed = BTreeSet::<String>::new();
        for playbook in &self.playbooks {
            let applies = playbook
                .applies_to
                .iter()
                .any(|value| context.contains(&normalize(value)));
            let domain_match = playbook
                .domains
                .iter()
                .any(|value| context.contains(&normalize(value)));
            if applies || domain_match {
                for tool in &playbook.preferred_tools {
                    routed.insert(tool.clone());
                }
            }
        }

        routed.into_iter().collect()
    }

    pub fn domain(&self, domain_id: &str) -> Option<&DomainChecklist> {
        self.domains.get(domain_id)
    }

    pub fn ingest_true_positive(&mut self, case: AdjudicatedCase) {
        self.store.ingest_true_positive(case);
    }

    pub fn ingest_false_positive(&mut self, case: AdjudicatedCase) {
        self.store.ingest_false_positive(case);
    }

    pub fn ingest_tool_sequence(&mut self, sequence: ToolSequence) {
        self.store.ingest_tool_sequence(sequence);
    }

    pub fn ingest_repro_pattern(&mut self, pattern: ReproPattern) {
        self.store.ingest_repro_pattern(pattern);
    }

    pub fn true_positives(&self) -> &[AdjudicatedCase] {
        self.store.true_positives()
    }

    pub fn false_positives(&self) -> &[AdjudicatedCase] {
        self.store.false_positives()
    }

    pub fn tool_sequences(&self) -> &[ToolSequence] {
        self.store.tool_sequences()
    }

    pub fn repro_patterns(&self) -> &[ReproPattern] {
        self.store.repro_patterns()
    }
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
