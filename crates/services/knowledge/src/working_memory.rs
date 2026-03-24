use std::collections::HashMap;

use audit_agent_core::finding::{Finding, FindingCategory, Severity};

const MAX_SUMMARY_CHARS: usize = 2_000;
const NO_CONTEXT_MESSAGE: &str = "No additional working-memory context available.";

/// Session-scoped memory for a single audit run.
/// This is intended for AI-assist surfaces only.
#[derive(Debug, Default, Clone)]
pub struct WorkingMemory {
    findings: Vec<FindingSummary>,
    engine_outcomes: Vec<EngineSummary>,
    tool_results: Vec<ToolResultSummary>,
    adviser_notes: Vec<String>,
}

#[derive(Debug, Clone)]
struct FindingSummary {
    id: String,
    title: String,
    severity: Severity,
    category: FindingCategory,
    file_path: Option<String>,
}

#[derive(Debug, Clone)]
struct EngineSummary {
    engine: String,
    status: String,
    findings_count: usize,
}

#[derive(Debug, Clone)]
struct ToolResultSummary {
    tool: String,
    target: String,
    status: String,
}

impl WorkingMemory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_finding(&mut self, finding: &Finding) {
        self.findings.push(FindingSummary {
            id: finding.id.to_string(),
            title: finding.title.clone(),
            severity: finding.severity.clone(),
            category: finding.category.clone(),
            file_path: finding
                .affected_components
                .first()
                .map(|location| location.file.to_string_lossy().to_string()),
        });
    }

    pub fn record_engine_outcome(&mut self, engine: &str, status: &str, findings_count: usize) {
        self.engine_outcomes.push(EngineSummary {
            engine: engine.to_string(),
            status: status.to_string(),
            findings_count,
        });
    }

    pub fn record_tool_result(&mut self, tool: &str, target: &str, status: &str) {
        self.tool_results.push(ToolResultSummary {
            tool: tool.to_string(),
            target: target.to_string(),
            status: status.to_string(),
        });
    }

    pub fn record_adviser_note(&mut self, note: &str) {
        self.adviser_notes.push(note.to_string());
    }

    pub fn summarize(&self) -> String {
        let mut parts = Vec::<String>::new();

        let mut severity_counts = HashMap::<String, usize>::new();
        for finding in &self.findings {
            *severity_counts
                .entry(format!("{:?}", finding.severity))
                .or_default() += 1;
        }
        if !severity_counts.is_empty() {
            let counts = severity_counts
                .iter()
                .map(|(name, count)| format!("{name}: {count}"))
                .collect::<Vec<_>>()
                .join(", ");
            parts.push(format!("Findings: {counts}"));
        }

        let mut sorted = self.findings.clone();
        sorted.sort_by_key(|finding| severity_rank(&finding.severity));
        let top_findings = sorted
            .iter()
            .take(5)
            .map(|finding| match finding.file_path.as_ref() {
                Some(path) => format!(
                    "- [{:?}] {}: {} ({path})",
                    finding.severity, finding.id, finding.title
                ),
                None => format!(
                    "- [{:?}] {}: {}",
                    finding.severity, finding.id, finding.title
                ),
            })
            .collect::<Vec<_>>();
        if !top_findings.is_empty() {
            parts.push(format!("Top findings:\n{}", top_findings.join("\n")));
        }

        let engines = self
            .engine_outcomes
            .iter()
            .map(|outcome| {
                format!(
                    "- {}: {} ({} findings)",
                    outcome.engine, outcome.status, outcome.findings_count
                )
            })
            .collect::<Vec<_>>();
        if !engines.is_empty() {
            parts.push(format!("Engines:\n{}", engines.join("\n")));
        }

        let tools = self
            .tool_results
            .iter()
            .map(|result| format!("- {} {} ({})", result.tool, result.target, result.status))
            .collect::<Vec<_>>();
        if !tools.is_empty() {
            parts.push(format!("Tool results:\n{}", tools.join("\n")));
        }

        if !self.adviser_notes.is_empty() {
            let notes = self
                .adviser_notes
                .iter()
                .take(3)
                .map(|note| format!("- {note}"))
                .collect::<Vec<_>>();
            parts.push(format!("Adviser notes:\n{}", notes.join("\n")));
        }

        truncate_to_char_limit(parts.join("\n\n"), MAX_SUMMARY_CHARS)
    }

    pub fn context_for_role(&self, role_name: &str) -> String {
        match role_name {
            "adviser" => {
                let relevant = self
                    .findings
                    .iter()
                    .filter(|finding| {
                        matches!(
                            &finding.category,
                            FindingCategory::CryptoMisuse
                                | FindingCategory::Replay
                                | FindingCategory::Race
                        )
                    })
                    .take(3)
                    .map(|finding| format!("- [{:?}] {}", finding.severity, finding.title))
                    .collect::<Vec<_>>();
                if relevant.is_empty() {
                    NO_CONTEXT_MESSAGE.to_string()
                } else {
                    format!("Recent cross-cutting findings:\n{}", relevant.join("\n"))
                }
            }
            "planning" | "reporting" => self.summarize(),
            _ => NO_CONTEXT_MESSAGE.to_string(),
        }
    }
}

fn severity_rank(severity: &Severity) -> usize {
    match severity {
        Severity::Critical => 0,
        Severity::High => 1,
        Severity::Medium => 2,
        Severity::Low => 3,
        Severity::Observation => 4,
    }
}

fn truncate_to_char_limit(input: String, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input;
    }

    let suffix = "...[truncated]";
    let suffix_len = suffix.chars().count();
    if max_chars <= suffix_len {
        return suffix.chars().take(max_chars).collect();
    }

    let prefix_len = max_chars - suffix_len;
    let mut output = input.chars().take(prefix_len).collect::<String>();
    output.push_str(suffix);
    output
}
