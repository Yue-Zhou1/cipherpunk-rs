use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
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
    pub ir_node_ids: Vec<String>,
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

static NONCE_LITERAL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bnonce\b\s*=\s*(?:0x[0-9a-fA-F_]+|\d[\d_]*(?:[uUiIfF]\d+)?)")
        .expect("nonce literal regex")
});
static HARDCODED_SECRET_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)\b(?:const|static|let)\b[^\n=]*\b(?:key|secret|seed)\b[^\n=]*=\s*(?:\[[^\]]+\]|b?"[^"]*"|0x[0-9a-fA-F_]+|\d[\d_]*)"#,
    )
    .expect("hardcoded secret regex")
});
static HARDCODED_SEED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\bseed\b[^\n=]*=\s*(?:0x[0-9a-fA-F_]+|\d[\d_]*)"#)
        .expect("hardcoded seed regex")
});
static DETERMINISTIC_RNG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:deterministic_rng_new|seed_from_u64)\b").expect("deterministic rng regex")
});
static MISSING_DOMAIN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:transcript_hash_no_domain|no_domain)\b").expect("domain regex")
});
static MISSING_CANONICALITY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:deserialize_field_unchecked|from_bytes_unchecked|unchecked)\b")
        .expect("canonicality regex")
});
static MISSING_SUBGROUP_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:verify_point_no_subgroup_check|no_subgroup_check)\b").expect("subgroup regex")
});
static UNCHECKED_UNWRAP_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\.(?:unwrap|expect)\s*\(|\bunwrap_or_default\s*\(").expect("unwrap regex")
});
static UNSAFE_VERIFY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bunsafe_signature_verify\b|unsafe.*verify|verify.*unsafe").expect("unsafe regex")
});
static MACRO_SUFFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"!$").expect("macro suffix regex"));

impl RuleEvaluator {
    pub fn load_from_dir(rules_dir: &Path) -> Result<Self> {
        let mut rules = Vec::new();
        for entry in WalkDir::new(rules_dir).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default();
            if ext != "yml" && ext != "yaml" {
                continue;
            }

            let content = fs::read_to_string(path)
                .with_context(|| format!("failed to read rule file {}", path.display()))?;
            let rule: CryptoMisuseRule = serde_yaml::from_str(&content)
                .with_context(|| format!("invalid rule yaml {}", path.display()))?;
            validate_rule_schema(&rule, path)?;
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
        let tree = {
            let Ok(mut parser) = self.parser.lock() else {
                return vec![];
            };
            parser.parse(&file.content, None)
        };

        let mut matches = Vec::new();
        for rule in &self.rules {
            let pattern_matches = if rule.detection.patterns.is_empty() {
                vec![]
            } else if let Some(parsed) = &tree {
                evaluate_patterns_for_rule(
                    &rule.id,
                    file,
                    parsed.root_node(),
                    &rule.detection.patterns,
                )
            } else {
                vec![]
            };

            if !rule.detection.patterns.is_empty() && pattern_matches.is_empty() {
                continue;
            }

            let mut semantic_matches =
                evaluate_semantic_checks_for_rule(&rule.id, file, &rule.detection.semantic_checks);
            if !rule.detection.semantic_checks.is_empty() && semantic_matches.is_empty() {
                continue;
            }

            if !semantic_matches.is_empty() {
                if let Some(pattern_match) = pattern_matches.first() {
                    for semantic_match in &mut semantic_matches {
                        merge_ir_node_ids(
                            &mut semantic_match.ir_node_ids,
                            &pattern_match.ir_node_ids,
                        );
                    }
                }
                matches.extend(semantic_matches);
            } else if !pattern_matches.is_empty() {
                matches.extend(pattern_matches);
            }
        }

        dedup_rule_matches(matches)
    }

    pub async fn evaluate_workspace(&self, ctx: &CryptoEngineContext) -> Vec<RuleMatch> {
        let mut all_matches = Vec::new();
        for member in &ctx.workspace.members {
            for entry in WalkDir::new(&member.path)
                .into_iter()
                .filter_map(|e| e.ok())
            {
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

        dedup_rule_matches(all_matches)
    }

    pub fn rules(&self) -> &[CryptoMisuseRule] {
        &self.rules
    }
}

fn validate_rule_schema(rule: &CryptoMisuseRule, path: &Path) -> Result<()> {
    if rule.detection.patterns.is_empty() && rule.detection.semantic_checks.is_empty() {
        bail!(
            "invalid rule {} in {}: detection must include at least one pattern or semantic_check",
            rule.id,
            path.display()
        );
    }

    for pattern in &rule.detection.patterns {
        if !is_supported_pattern_type(&pattern.pattern_type) {
            bail!(
                "invalid rule {} in {}: unsupported pattern type `{}`",
                rule.id,
                path.display(),
                pattern.pattern_type
            );
        }
        if pattern.name_matches.is_empty() {
            bail!(
                "invalid rule {} in {}: pattern `{}` requires non-empty name_matches",
                rule.id,
                path.display(),
                pattern.pattern_type
            );
        }
    }

    for check in &rule.detection.semantic_checks {
        if !is_supported_semantic_check(check) {
            bail!(
                "invalid rule {} in {}: unsupported semantic check `{}`",
                rule.id,
                path.display(),
                check
            );
        }
    }

    Ok(())
}

fn is_supported_pattern_type(pattern_type: &str) -> bool {
    matches!(
        pattern_type,
        "function_call" | "method_call" | "macro_call" | "path_contains" | "attribute"
    )
}

fn is_supported_semantic_check(check: &str) -> bool {
    matches!(
        check,
        "nonce_is_not_bound_to_session_id"
            | "missing_domain_separator"
            | "missing_canonicality_check"
            | "rng_is_predictable"
            | "missing_small_subgroup_check"
            | "unchecked_unwrap"
            | "hardcoded_secret_present"
            | "unsafe_in_verification_path"
            | "suspicious_nonce_initialization"
            | "hardcoded_seed_usage"
    )
}

fn evaluate_patterns_for_rule(
    rule_id: &str,
    file: &SourceFile,
    root: tree_sitter::Node,
    patterns: &[RulePattern],
) -> Vec<RuleMatch> {
    let mut out = Vec::new();
    for pattern in patterns {
        let mut matches_for_pattern = match pattern.pattern_type.as_str() {
            "function_call" => {
                find_function_call_matches(rule_id, file, &pattern.name_matches, root)
            }
            "method_call" => find_method_call_matches(rule_id, file, &pattern.name_matches, root),
            "macro_call" => find_macro_call_matches(rule_id, file, &pattern.name_matches, root),
            "path_contains" => find_path_contains_matches(rule_id, file, &pattern.name_matches),
            "attribute" => find_attribute_matches(rule_id, file, &pattern.name_matches, root),
            _ => vec![],
        };
        if let Some(first) = matches_for_pattern.drain(..).next() {
            out.push(first);
        }
    }
    dedup_rule_matches(out)
}

fn evaluate_semantic_checks_for_rule(
    rule_id: &str,
    file: &SourceFile,
    semantic_checks: &[String],
) -> Vec<RuleMatch> {
    // semantic_checks intentionally use AND semantics:
    // every check must produce at least one hit for this rule/file pair.
    if semantic_checks.is_empty() {
        return vec![];
    }

    let mut all = Vec::new();
    for check in semantic_checks {
        let hits = semantic_check_line_hits(file, check);
        if hits.is_empty() {
            return vec![];
        }
        for line in hits {
            all.push(rule_match_for_line(rule_id, file, line, None));
        }
    }

    dedup_rule_matches(all)
}

fn find_function_call_matches(
    rule_id: &str,
    file: &SourceFile,
    name_matches: &[String],
    root: tree_sitter::Node,
) -> Vec<RuleMatch> {
    let mut matches = Vec::new();
    visit_named_nodes(root, &mut |node| {
        if should_skip_node(&node, file) || node.kind() != "call_expression" {
            return;
        }
        let Some(function_node) = node
            .child_by_field_name("function")
            .or_else(|| node.child(0))
        else {
            return;
        };
        let fn_text = &file.content[function_node.start_byte()..function_node.end_byte()];
        let fn_name = canonical_callable_name(fn_text);
        if !matches_name(name_matches, fn_text) && !matches_name(name_matches, fn_name) {
            return;
        }
        let line = node.start_position().row as u32 + 1;
        matches.push(rule_match_for_line(rule_id, file, line, Some(fn_name)));
    });
    dedup_rule_matches(matches)
}

fn find_method_call_matches(
    rule_id: &str,
    file: &SourceFile,
    name_matches: &[String],
    root: tree_sitter::Node,
) -> Vec<RuleMatch> {
    let mut matches = Vec::new();
    visit_named_nodes(root, &mut |node| {
        if should_skip_node(&node, file) {
            return;
        }

        let method_name = if node.kind() == "method_call_expression" {
            node.child_by_field_name("method")
                .map(|method| file.content[method.start_byte()..method.end_byte()].to_string())
                .or_else(|| {
                    let node_text = &file.content[node.start_byte()..node.end_byte()];
                    node_text
                        .split('(')
                        .next()
                        .and_then(|before_paren| before_paren.rsplit('.').next())
                        .map(|value| value.trim().to_string())
                })
        } else if node.kind() == "call_expression" {
            let Some(function_node) = node
                .child_by_field_name("function")
                .or_else(|| node.child(0))
            else {
                return;
            };
            let fn_text = &file.content[function_node.start_byte()..function_node.end_byte()];
            fn_text.contains('.').then(|| {
                fn_text
                    .rsplit('.')
                    .next()
                    .unwrap_or(fn_text)
                    .trim()
                    .to_string()
            })
        } else {
            None
        };

        let Some(method_name) = method_name else {
            return;
        };
        if !matches_name(name_matches, &method_name) {
            return;
        }
        let line = node.start_position().row as u32 + 1;
        matches.push(rule_match_for_line(rule_id, file, line, Some(&method_name)));
    });
    dedup_rule_matches(matches)
}

fn find_macro_call_matches(
    rule_id: &str,
    file: &SourceFile,
    name_matches: &[String],
    root: tree_sitter::Node,
) -> Vec<RuleMatch> {
    let mut matches = Vec::new();
    visit_named_nodes(root, &mut |node| {
        if should_skip_node(&node, file) || node.kind() != "macro_invocation" {
            return;
        }
        // tree-sitter-rust models macro invocations with the macro path as child(0).
        let Some(macro_node) = node.child(0) else {
            return;
        };
        let macro_text = &file.content[macro_node.start_byte()..macro_node.end_byte()];
        let macro_name = canonical_macro_name(macro_text);
        if !matches_name(name_matches, macro_text)
            && !matches_name(name_matches, &macro_name)
            && !matches_name(name_matches, &format!("{macro_name}!"))
        {
            return;
        }
        let line = node.start_position().row as u32 + 1;
        matches.push(rule_match_for_line(
            rule_id,
            file,
            line,
            Some(&format!("{macro_name}!")),
        ));
    });
    dedup_rule_matches(matches)
}

fn find_path_contains_matches(
    rule_id: &str,
    file: &SourceFile,
    name_matches: &[String],
) -> Vec<RuleMatch> {
    let path = file.path.to_string_lossy();
    if name_matches.iter().any(|needle| path.contains(needle)) {
        return vec![rule_match_for_line(rule_id, file, 1, None)];
    }
    vec![]
}

fn find_attribute_matches(
    rule_id: &str,
    file: &SourceFile,
    name_matches: &[String],
    root: tree_sitter::Node,
) -> Vec<RuleMatch> {
    let mut matches = Vec::new();
    visit_named_nodes(root, &mut |node| {
        if should_skip_node(&node, file) || node.kind() != "attribute_item" {
            return;
        }
        let text = &file.content[node.start_byte()..node.end_byte()];
        if !name_matches.iter().any(|needle| text.contains(needle)) {
            return;
        }
        let line = node.start_position().row as u32 + 1;
        matches.push(rule_match_for_line(rule_id, file, line, None));
    });
    dedup_rule_matches(matches)
}

fn semantic_check_line_hits(file: &SourceFile, check_id: &str) -> Vec<u32> {
    match check_id {
        "nonce_is_not_bound_to_session_id" | "suspicious_nonce_initialization" => {
            lines_matching(&file.content, &NONCE_LITERAL_RE)
        }
        "hardcoded_secret_present" => lines_matching(&file.content, &HARDCODED_SECRET_RE),
        "hardcoded_seed_usage" => {
            let mut lines = lines_matching(&file.content, &HARDCODED_SEED_RE);
            lines.extend(lines_matching(&file.content, &DETERMINISTIC_RNG_RE));
            dedup_lines(lines)
        }
        "rng_is_predictable" => {
            let mut lines = lines_matching(&file.content, &DETERMINISTIC_RNG_RE);
            lines.extend(lines_matching(&file.content, &HARDCODED_SEED_RE));
            dedup_lines(lines)
        }
        "missing_domain_separator" => lines_matching(&file.content, &MISSING_DOMAIN_RE),
        "missing_canonicality_check" => lines_matching(&file.content, &MISSING_CANONICALITY_RE),
        "missing_small_subgroup_check" => lines_matching(&file.content, &MISSING_SUBGROUP_RE),
        "unchecked_unwrap" => lines_matching(&file.content, &UNCHECKED_UNWRAP_RE),
        "unsafe_in_verification_path" => lines_matching(&file.content, &UNSAFE_VERIFY_RE),
        _ => vec![],
    }
}

fn lines_matching(content: &str, regex: &Regex) -> Vec<u32> {
    content
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| regex.is_match(line).then_some(idx as u32 + 1))
        .collect()
}

fn dedup_lines(lines: Vec<u32>) -> Vec<u32> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for line in lines {
        if seen.insert(line) {
            deduped.push(line);
        }
    }
    deduped
}

fn dedup_rule_matches(matches: Vec<RuleMatch>) -> Vec<RuleMatch> {
    let mut seen = HashSet::<(String, PathBuf, u32, u32)>::new();
    let mut deduped = Vec::new();
    for matched in matches {
        let key = (
            matched.rule_id.clone(),
            matched.location.file.clone(),
            matched.location.line_range.0,
            matched.location.line_range.1,
        );
        if seen.insert(key) {
            deduped.push(matched);
        }
    }
    deduped
}

fn merge_ir_node_ids(target: &mut Vec<String>, incoming: &[String]) {
    let mut seen = target.iter().cloned().collect::<HashSet<_>>();
    for id in incoming {
        if seen.insert(id.clone()) {
            target.push(id.clone());
        }
    }
}

fn visit_named_nodes(root: tree_sitter::Node, visitor: &mut dyn FnMut(tree_sitter::Node)) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        visitor(node);
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
}

fn matches_name(candidates: &[String], actual: &str) -> bool {
    let normalized_actual = normalize_name(actual);
    candidates
        .iter()
        .map(|candidate| normalize_name(candidate))
        .any(|candidate| candidate == normalized_actual)
}

fn normalize_name(name: &str) -> String {
    canonical_macro_name(name)
        .rsplit("::")
        .next()
        .unwrap_or(name)
        .rsplit('.')
        .next()
        .unwrap_or(name)
        .trim()
        .to_string()
}

fn canonical_callable_name(callable: &str) -> &str {
    callable
        .rsplit("::")
        .next()
        .unwrap_or(callable)
        .rsplit('.')
        .next()
        .unwrap_or(callable)
        .trim()
}

fn canonical_macro_name(macro_name: &str) -> String {
    MACRO_SUFFIX_RE.replace(macro_name.trim(), "").to_string()
}

fn rule_match_for_line(
    rule_id: &str,
    file: &SourceFile,
    line: u32,
    symbol_name: Option<&str>,
) -> RuleMatch {
    let (start, end, snippet) = snippet_for_line(&file.content, line.max(1), 10);
    let mut ir_node_ids = vec![format!("file:{}", file.path.display())];
    if let Some(symbol) = symbol_name
        .map(normalized_provenance_symbol)
        .filter(|value| !value.is_empty())
    {
        ir_node_ids.push(format!("symbol:{}::{symbol}", file.path.display()));
        if symbol_name.is_some_and(|raw| raw.trim().ends_with('!')) {
            ir_node_ids.push(format!("symbol:{}::macro:{symbol}", file.path.display()));
        }
    }

    RuleMatch {
        rule_id: rule_id.to_string(),
        location: CodeLocation {
            crate_name: file.crate_name.clone(),
            module: file.module.clone(),
            file: file.path.clone(),
            line_range: (start, end),
            snippet: Some(snippet.clone()),
        },
        matched_snippet: snippet,
        confidence: Confidence::High,
        ir_node_ids,
    }
}

fn normalized_provenance_symbol(symbol: &str) -> String {
    symbol
        .trim()
        .trim_end_matches('!')
        .rsplit("::")
        .next()
        .unwrap_or(symbol)
        .rsplit('.')
        .next()
        .unwrap_or(symbol)
        .trim()
        .to_string()
}

/// Returns true if the node should be skipped for rule matching.
/// Skips: comments, string literals, #[cfg(test)] attributed blocks.
fn should_skip_node(node: &tree_sitter::Node, file: &SourceFile) -> bool {
    match node.kind() {
        "line_comment" | "block_comment" | "string_literal" | "raw_string_literal" => true,
        "attribute_item" => {
            let text = &file.content[node.start_byte()..node.end_byte()];
            text.contains("cfg(test)")
        }
        // Skip the entire mod block decorated with #[cfg(test)]
        "mod_item" => {
            let mut child_cursor = node.walk();
            if child_cursor.goto_first_child() {
                loop {
                    let child = child_cursor.node();
                    if child.kind() == "attribute_item" {
                        let text = &file.content[child.start_byte()..child.end_byte()];
                        if text.contains("cfg(test)") {
                            return true;
                        }
                    }
                    if !child_cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
            false
        }
        _ => false,
    }
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
