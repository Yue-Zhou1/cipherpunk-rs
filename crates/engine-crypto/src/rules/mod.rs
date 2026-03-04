use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use audit_agent_core::audit_config::Confidence;
use audit_agent_core::finding::{CodeLocation, FindingCategory, Severity};
use regex::Regex;
use serde::Deserialize;
use tree_sitter::Parser;
use walkdir::WalkDir;

use crate::intake_bridge::CryptoEngineContext;

pub struct RuleEvaluator {
    rules: Vec<CryptoMisuseRule>,
    parser: std::sync::Mutex<Parser>,
}

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub crate_name: String,
    pub path: PathBuf,
    pub module: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleMatch {
    pub rule_id: String,
    pub location: CodeLocation,
    pub matched_snippet: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CryptoMisuseRule {
    pub id: String,
    pub title: String,
    pub severity: Severity,
    pub category: FindingCategory,
    pub description: String,
    pub detection: RuleDetection,
    #[serde(default)]
    pub references: Vec<String>,
    pub remediation: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleDetection {
    pub patterns: Vec<RulePattern>,
    #[serde(default)]
    pub semantic_checks: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RulePattern {
    #[serde(rename = "type")]
    pub pattern_type: String,
    #[serde(default)]
    pub name_matches: Vec<String>,
}

impl RuleEvaluator {
    pub fn load_from_dir(rules_dir: &Path) -> Result<Self> {
        let mut rules = Vec::new();
        for entry in WalkDir::new(rules_dir).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
            if ext != "yml" && ext != "yaml" {
                continue;
            }

            let content = fs::read_to_string(path)
                .with_context(|| format!("failed to read rule file {}", path.display()))?;
            let rule: CryptoMisuseRule = serde_yaml::from_str(&content)
                .with_context(|| format!("invalid rule yaml {}", path.display()))?;
            rules.push(rule);
        }

        rules.sort_by(|a, b| a.id.cmp(&b.id));

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::language())
            .context("failed to initialize rust parser")?;

        Ok(Self {
            rules,
            parser: std::sync::Mutex::new(parser),
        })
    }

    pub async fn evaluate_file(&self, file: &SourceFile) -> Vec<RuleMatch> {
        if let Ok(mut parser) = self.parser.lock() {
            let _ = parser.parse(&file.content, None);
        }

        let mut matches = Vec::new();
        for rule in &self.rules {
            let mut emitted_for_rule = false;
            for pattern in &rule.detection.patterns {
                if pattern.pattern_type != "function_call" {
                    continue;
                }
                for name in &pattern.name_matches {
                    if let Some(rule_match) = find_function_call_match(&rule.id, file, name) {
                        matches.push(rule_match);
                        emitted_for_rule = true;
                        break;
                    }
                }
                if emitted_for_rule {
                    break;
                }
            }
        }

        matches
    }

    pub async fn evaluate_workspace(&self, ctx: &CryptoEngineContext) -> Vec<RuleMatch> {
        let mut all_matches = Vec::new();
        for member in &ctx.workspace.members {
            for entry in WalkDir::new(&member.path).into_iter().filter_map(|e| e.ok()) {
                if !entry.file_type().is_file() {
                    continue;
                }
                if entry.path().extension().and_then(|v| v.to_str()) != Some("rs") {
                    continue;
                }
                let Ok(content) = fs::read_to_string(entry.path()) else {
                    continue;
                };
                let module = entry
                    .path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                let source_file = SourceFile {
                    crate_name: member.name.clone(),
                    path: entry.path().to_path_buf(),
                    module,
                    content,
                };
                all_matches.extend(self.evaluate_file(&source_file).await);
            }
        }
        all_matches
    }

    pub fn rules(&self) -> &[CryptoMisuseRule] {
        &self.rules
    }
}

fn find_function_call_match(rule_id: &str, file: &SourceFile, name: &str) -> Option<RuleMatch> {
    let escaped = regex::escape(name);
    let pattern = format!(r"\b{escaped}\s*\(");
    let re = Regex::new(&pattern).ok()?;
    let mat = re.find(&file.content)?;

    let line = line_number_for_offset(&file.content, mat.start())?;
    let (line_start, line_end, snippet) = snippet_for_line(&file.content, line, 10);

    Some(RuleMatch {
        rule_id: rule_id.to_string(),
        location: CodeLocation {
            crate_name: file.crate_name.clone(),
            module: file.module.clone(),
            file: file.path.clone(),
            line_range: (line_start, line_end),
            snippet: Some(snippet.clone()),
        },
        matched_snippet: snippet,
        confidence: Confidence::High,
    })
}

fn line_number_for_offset(content: &str, offset: usize) -> Option<u32> {
    if offset > content.len() {
        return None;
    }
    let prefix = &content[..offset];
    Some(prefix.bytes().filter(|b| *b == b'\n').count() as u32 + 1)
}

fn snippet_for_line(content: &str, line: u32, max_lines: u32) -> (u32, u32, String) {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return (line, line, String::new());
    }

    let line_index = line.saturating_sub(1) as usize;
    let half_window = max_lines / 2;
    let mut start = line.saturating_sub(half_window);
    if start == 0 {
        start = 1;
    }
    let mut end = start + max_lines - 1;
    let total_lines = lines.len() as u32;
    if end > total_lines {
        end = total_lines;
        start = end.saturating_sub(max_lines.saturating_sub(1));
        if start == 0 {
            start = 1;
        }
    }

    if line_index + 1 < start as usize || line_index + 1 > end as usize {
        start = line;
        end = (line + max_lines - 1).min(total_lines);
    }

    let snippet = (start..=end)
        .filter_map(|num| lines.get((num - 1) as usize).map(|l| (*l).to_string()))
        .collect::<Vec<_>>()
        .join("\n");

    (start, end, snippet)
}
