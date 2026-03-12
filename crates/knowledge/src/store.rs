use crate::models::{AdjudicatedCase, ReproPattern, ToolSequence};

// Local in-memory store for early v3 tasks. Session-backed persistence can be
// added later through session-store without changing the KnowledgeBase API.
#[derive(Debug, Default, Clone)]
pub struct KnowledgeStore {
    true_positives: Vec<AdjudicatedCase>,
    false_positives: Vec<AdjudicatedCase>,
    tool_sequences: Vec<ToolSequence>,
    repro_patterns: Vec<ReproPattern>,
}

impl KnowledgeStore {
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
