use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub mod loader;
#[cfg(feature = "memory-block")]
pub mod memory_block;
pub mod models;
pub mod store;

use loader::{load_domains, load_playbooks};
#[cfg(feature = "memory-block")]
use memory_block::MemoryBlock;
use models::{AdjudicatedCase, DomainChecklist, ReproPattern, ToolPlaybook, ToolSequence};
use store::KnowledgeStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRecommendation {
    pub tool: String,
    pub rationale: String,
}

#[derive(Debug)]
pub struct KnowledgeBase {
    playbooks: Vec<ToolPlaybook>,
    domains: BTreeMap<String, DomainChecklist>,
    store: KnowledgeStore,
    #[cfg(feature = "memory-block")]
    memory_block: Option<MemoryBlock>,
}

impl KnowledgeBase {
    pub fn load_from_repo_root() -> Result<Self> {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|value| value.parent())
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
            #[cfg(feature = "memory-block")]
            memory_block: None,
        })
    }

    pub fn load_from_repo_root_with_store(store_path: impl AsRef<Path>) -> Result<Self> {
        let mut kb = Self::load_from_repo_root()?;
        kb.store = KnowledgeStore::load_from_path(store_path.as_ref())?;
        Ok(kb)
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

    pub fn recommend_tools(
        &self,
        context: &[String],
        overview_notes: &[String],
    ) -> Vec<ToolRecommendation> {
        let normalized_context = context
            .iter()
            .map(|value| normalize(value))
            .collect::<BTreeSet<_>>();
        let normalized_overview = overview_notes
            .iter()
            .map(|value| normalize(value))
            .collect::<Vec<_>>();

        self.route_tools(context)
            .into_iter()
            .map(|tool| {
                let mut rationale =
                    self.playbooks
                        .iter()
                        .filter(|playbook| {
                            playbook.preferred_tools.iter().any(|entry| entry == &tool)
                                && (playbook
                                    .applies_to
                                    .iter()
                                    .any(|entry| normalized_context.contains(&normalize(entry)))
                                    || playbook.domains.iter().any(|entry| {
                                        normalized_context.contains(&normalize(entry))
                                    }))
                        })
                        .map(|playbook| format!("Matched playbook {}", playbook.id))
                        .collect::<Vec<_>>();

                if rationale.is_empty() {
                    rationale.push("Matched fallback tool routing".to_string());
                }

                if normalized_overview
                    .iter()
                    .any(|note| note.contains("redacted"))
                {
                    rationale.push("Overview notes indicate redaction-sensitive flows".to_string());
                }

                ToolRecommendation {
                    tool,
                    rationale: rationale.join("; "),
                }
            })
            .collect()
    }

    pub fn domain(&self, domain_id: &str) -> Option<&DomainChecklist> {
        self.domains.get(domain_id)
    }

    #[cfg(feature = "memory-block")]
    pub fn attach_memory_block(&mut self, memory_block: MemoryBlock) {
        self.memory_block = Some(memory_block);
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

    pub fn similar_cases(&self, context: &[String], limit: usize) -> Vec<AdjudicatedCase> {
        let normalized_context = context
            .iter()
            .map(|value| normalize(value))
            .collect::<BTreeSet<_>>();

        let mut matched = self
            .store
            .true_positives()
            .iter()
            .filter(|case| {
                case.tags
                    .iter()
                    .map(|tag| normalize(tag))
                    .any(|tag| normalized_context.contains(&tag))
            })
            .cloned()
            .collect::<Vec<_>>();

        if matched.is_empty() {
            matched.extend(self.store.false_positives().iter().cloned());
        }

        #[cfg(feature = "memory-block")]
        if let Some(memory_block) = &self.memory_block {
            let query_text = context.join(" ");
            if !query_text.trim().is_empty() {
                if let Ok(semantic) = memory_block.search(&query_text, limit.max(1)) {
                    let mut seen_ids = matched
                        .iter()
                        .map(|case| case.id.clone())
                        .collect::<BTreeSet<_>>();
                    for result in semantic {
                        let semantic_case = result.signature.to_adjudicated_case();
                        if seen_ids.insert(semantic_case.id.clone()) {
                            matched.push(semantic_case);
                        }
                    }
                }
            }
        }

        matched.into_iter().take(limit.max(1)).collect()
    }

    pub fn persist_feedback_store(&self, store_path: impl AsRef<Path>) -> Result<()> {
        self.store.persist_to_path(store_path.as_ref())
    }
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
