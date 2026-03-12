//! Semantic extraction for the first usable IR pipeline.
//!
//! Current implementation uses lightweight regex scanning to keep the pipeline
//! functional. Before workstation GA, migrate to tree-sitter-backed extraction
//! for higher-fidelity symbol and call tracking.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result};
use regex::Regex;
use walkdir::WalkDir;

static FN_DECL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(").expect("fn regex"));
static CALL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([A-Za-z_][A-Za-z0-9_:]*)\s*\(").expect("call regex"));
static CFG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"#\[cfg\(feature\s*=\s*"([A-Za-z0-9_-]+)"\)\]"#).expect("cfg regex")
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSymbol {
    pub name: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticFile {
    pub path: PathBuf,
    pub functions: Vec<FunctionSymbol>,
    pub function_calls: Vec<FunctionCallSite>,
    pub cfg_features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SemanticIndex {
    pub files: Vec<SemanticFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionCallSite {
    pub caller: String,
    pub callee: String,
}

#[derive(Debug, Clone)]
struct FnContext {
    name: String,
    brace_depth: i32,
    opened_block: bool,
}

pub fn build_rust_semantic_index(root: &Path) -> Result<SemanticIndex> {
    let mut files = Vec::<SemanticFile>::new();
    for entry in WalkDir::new(root).follow_links(true) {
        let entry = entry.with_context(|| format!("walk {}", root.display()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|v| v.to_str()) != Some("rs") {
            continue;
        }

        let path = entry.path().to_path_buf();
        let content =
            fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        files.push(scan_rust_file(path, &content));
    }
    Ok(SemanticIndex { files })
}

fn scan_rust_file(path: PathBuf, content: &str) -> SemanticFile {
    let mut functions = Vec::<FunctionSymbol>::new();
    let mut function_calls = Vec::<FunctionCallSite>::new();
    let mut cfg_features = Vec::<String>::new();
    let mut fn_ctx: Option<FnContext> = None;
    let mut brace_state = BraceScanState::default();

    for (line_idx, line) in content.lines().enumerate() {
        let line_no = line_idx as u32 + 1;
        let line_delta = brace_delta(line, &mut brace_state);

        if let Some(captures) = FN_DECL_RE.captures(line)
            && let Some(name) = captures.get(1)
        {
            let fn_name = name.as_str().to_string();
            functions.push(FunctionSymbol {
                name: fn_name.clone(),
                line: line_no,
            });
            if fn_ctx.is_none() {
                fn_ctx = Some(FnContext {
                    name: fn_name,
                    brace_depth: 0,
                    opened_block: false,
                });
            }
        }

        if let Some(captures) = CFG_RE.captures(line)
            && let Some(feature) = captures.get(1)
        {
            cfg_features.push(feature.as_str().to_string());
        }

        if let Some(ctx) = fn_ctx.as_mut() {
            for capture in CALL_RE.captures_iter(line) {
                let Some(name) = capture.get(1) else {
                    continue;
                };
                let callee = last_path_segment(name.as_str());
                if should_ignore_call_symbol(callee) || callee == ctx.name {
                    continue;
                }
                function_calls.push(FunctionCallSite {
                    caller: ctx.name.clone(),
                    callee: callee.to_string(),
                });
            }

            if line.contains('{') {
                ctx.opened_block = true;
            }
            ctx.brace_depth += line_delta;
            if ctx.opened_block && ctx.brace_depth <= 0 {
                fn_ctx = None;
            }
        }
    }

    SemanticFile {
        path,
        functions,
        function_calls,
        cfg_features,
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct BraceScanState {
    block_comment_depth: usize,
    in_string: bool,
    string_escaped: bool,
    raw_string_hashes: Option<usize>,
}

fn brace_delta(line: &str, state: &mut BraceScanState) -> i32 {
    let bytes = line.as_bytes();
    let mut delta = 0i32;
    let mut idx = 0usize;

    while idx < bytes.len() {
        if let Some(hashes) = state.raw_string_hashes {
            if raw_string_closes_at(bytes, idx, hashes) {
                state.raw_string_hashes = None;
                idx += 1 + hashes;
                continue;
            }
            idx += 1;
            continue;
        }

        if state.in_string {
            if state.string_escaped {
                state.string_escaped = false;
            } else {
                match bytes[idx] {
                    b'\\' => state.string_escaped = true,
                    b'"' => state.in_string = false,
                    _ => {}
                }
            }
            idx += 1;
            continue;
        }

        if state.block_comment_depth > 0 {
            if starts_with(bytes, idx, b"/*") {
                state.block_comment_depth += 1;
                idx += 2;
                continue;
            }
            if starts_with(bytes, idx, b"*/") {
                state.block_comment_depth -= 1;
                idx += 2;
                continue;
            }
            idx += 1;
            continue;
        }

        if starts_with(bytes, idx, b"//") {
            break;
        }
        if starts_with(bytes, idx, b"/*") {
            state.block_comment_depth += 1;
            idx += 2;
            continue;
        }
        if let Some(raw_hashes) = raw_string_opens_at(bytes, idx) {
            state.raw_string_hashes = Some(raw_hashes);
            idx += 2 + raw_hashes;
            continue;
        }

        match bytes[idx] {
            b'"' => {
                state.in_string = true;
                state.string_escaped = false;
            }
            b'{' => delta += 1,
            b'}' => delta -= 1,
            _ => {}
        }

        idx += 1;
    }

    delta
}

fn starts_with(bytes: &[u8], idx: usize, pattern: &[u8]) -> bool {
    idx + pattern.len() <= bytes.len() && &bytes[idx..idx + pattern.len()] == pattern
}

fn raw_string_opens_at(bytes: &[u8], idx: usize) -> Option<usize> {
    if bytes.get(idx) != Some(&b'r') {
        return None;
    }

    let mut cursor = idx + 1;
    while cursor < bytes.len() && bytes[cursor] == b'#' {
        cursor += 1;
    }

    (cursor < bytes.len() && bytes[cursor] == b'"').then_some(cursor - idx - 1)
}

fn raw_string_closes_at(bytes: &[u8], idx: usize, hashes: usize) -> bool {
    if bytes.get(idx) != Some(&b'"') || idx + 1 + hashes > bytes.len() {
        return false;
    }

    bytes[idx + 1..idx + 1 + hashes]
        .iter()
        .all(|byte| *byte == b'#')
}

fn last_path_segment(symbol: &str) -> &str {
    symbol.rsplit("::").next().unwrap_or(symbol)
}

fn should_ignore_call_symbol(symbol: &str) -> bool {
    matches!(
        symbol,
        "if" | "for"
            | "while"
            | "loop"
            | "match"
            | "return"
            | "let"
            | "Some"
            | "None"
            | "Ok"
            | "Err"
            | "assert"
            | "assert_eq"
            | "assert_ne"
            | "panic"
            | "todo"
            | "unimplemented"
            | "unreachable"
            | "Self"
            | "self"
    )
}
