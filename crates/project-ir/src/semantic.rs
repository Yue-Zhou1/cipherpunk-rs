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
    pub calls: Vec<String>,
    pub cfg_features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SemanticIndex {
    pub files: Vec<SemanticFile>,
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
    let mut calls = Vec::<String>::new();
    let mut cfg_features = Vec::<String>::new();

    for (line_idx, line) in content.lines().enumerate() {
        let line_no = line_idx as u32 + 1;
        if let Some(captures) = FN_DECL_RE.captures(line)
            && let Some(name) = captures.get(1)
        {
            functions.push(FunctionSymbol {
                name: name.as_str().to_string(),
                line: line_no,
            });
        }

        if let Some(captures) = CFG_RE.captures(line)
            && let Some(feature) = captures.get(1)
        {
            cfg_features.push(feature.as_str().to_string());
        }

        for capture in CALL_RE.captures_iter(line) {
            if let Some(name) = capture.get(1) {
                let symbol = name.as_str().split("::").last().unwrap_or_default();
                if should_ignore_call_symbol(symbol) {
                    continue;
                }
                calls.push(symbol.to_string());
            }
        }
    }

    SemanticFile {
        path,
        functions,
        calls,
        cfg_features,
    }
}

fn should_ignore_call_symbol(symbol: &str) -> bool {
    matches!(
        symbol,
        "if" | "for"
            | "while"
            | "loop"
            | "match"
            | "Some"
            | "None"
            | "Ok"
            | "Err"
            | "Self"
            | "self"
    )
}
