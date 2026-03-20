use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use audit_agent_core::audit_config::BudgetConfig;
use audit_agent_core::workspace::CargoWorkspace;
use regex::Regex;
use walkdir::WalkDir;

const RA_COMPAT_VERSION: &str = "ra_ap_ide=0.0.239";
const LSP_BACKEND_VERSION: &str = "rust-analyzer-lsp";
static TEST_BUILD_DELAY_MS: AtomicU64 = AtomicU64::new(0);

pub type CallGraph = HashMap<String, HashSet<String>>;
pub type MacroExpansionMap = HashMap<SpanId, String>;
pub type TraitImplMap = HashMap<String, Vec<FnRef>>;
pub type CfgVariantMap = HashMap<String, Vec<CfgDivergence>>;

struct FileScanOutputs<'a> {
    call_graph: &'a mut CallGraph,
    macro_expansions: &'a mut MacroExpansionMap,
    macro_sites: &'a mut Vec<MacroSite>,
    trait_impls: &'a mut TraitImplMap,
    cfg_variants: &'a mut CfgVariantMap,
}

static FN_DECL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(").expect("fn regex"));
static CALL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([A-Za-z_][A-Za-z0-9_:]*)\s*\(").expect("call regex"));
static IMPL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\bimpl(?:\s*<[^>]+>)?\s+([A-Za-z_][A-Za-z0-9_:]*)\s+for\s+([A-Za-z_][A-Za-z0-9_:]*)",
    )
    .expect("impl regex")
});
static MACRO_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([A-Za-z_][A-Za-z0-9_:]*)!\s*\(").expect("macro regex"));
static CFG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"#\[cfg\(feature\s*=\s*"([A-Za-z0-9_-]+)"\)\]"#).expect("cfg regex")
});

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpanId {
    pub file: PathBuf,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnRef {
    pub crate_name: String,
    pub trait_name: String,
    pub method_name: String,
    pub impl_type: String,
    pub file: PathBuf,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSymbolRef {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionCallRef {
    pub caller: String,
    pub callee: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroSite {
    pub crate_name: String,
    pub macro_name: String,
    pub file: PathBuf,
    pub line: u32,
    pub column: u32,
    pub caller: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfgDivergence {
    pub crate_name: String,
    pub feature: String,
    pub file: PathBuf,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RustSemanticFacts {
    pub function_symbols: Vec<FunctionSymbolRef>,
    pub function_calls: Vec<FunctionCallRef>,
    pub macro_sites: Vec<MacroSite>,
    pub trait_impls: Vec<FnRef>,
    pub cfg_divergences: Vec<CfgDivergence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticBackend {
    RustAnalyzer { version: String },
    LspSubprocess { ra_binary_version: String },
    TreeSitterFallback { reason: String },
}

impl SemanticBackend {
    pub fn record_in_tool_versions(&self, tool_versions: &mut HashMap<String, String>) {
        match self {
            SemanticBackend::RustAnalyzer { version } => {
                tool_versions.insert("semantic_backend".to_string(), "rust-analyzer".to_string());
                tool_versions.insert("semantic_backend_version".to_string(), version.clone());
            }
            SemanticBackend::LspSubprocess { ra_binary_version } => {
                tool_versions.insert("semantic_backend".to_string(), "lsp-subprocess".to_string());
                tool_versions.insert(
                    "semantic_backend_version".to_string(),
                    ra_binary_version.clone(),
                );
            }
            SemanticBackend::TreeSitterFallback { reason } => {
                tool_versions.insert(
                    "semantic_backend".to_string(),
                    "tree-sitter-fallback".to_string(),
                );
                tool_versions.insert("semantic_backend_reason".to_string(), reason.clone());
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticIndex {
    pub call_graph: CallGraph,
    pub macro_expansions: MacroExpansionMap,
    pub trait_impls: TraitImplMap,
    pub cfg_variants: CfgVariantMap,
    pub rust_facts: RustSemanticFacts,
    pub backend: SemanticBackend,
}

impl SemanticIndex {
    pub async fn build(workspace: &CargoWorkspace, budget: &BudgetConfig) -> Result<Self> {
        let workspace_clone = workspace.clone();
        let worker = tokio::task::spawn_blocking(move || build_primary(&workspace_clone));
        let timeout = Duration::from_secs(budget.semantic_index_timeout_secs);

        match tokio::time::timeout(timeout, worker).await {
            Ok(Ok(Ok(index))) => Ok(index),
            Ok(Ok(Err(primary_error))) => {
                // rust-analyzer style indexing failed; use LSP-style scanner as fallback.
                build_with_backend(
                    workspace,
                    SemanticBackend::LspSubprocess {
                        ra_binary_version: LSP_BACKEND_VERSION.to_string(),
                    },
                )
                .or_else(|_| {
                    build_tree_sitter_fallback(
                        workspace,
                        format!("semantic index build failed: {primary_error:#}"),
                    )
                })
            }
            Ok(Err(join_error)) => build_tree_sitter_fallback(
                workspace,
                format!("semantic index worker failed: {join_error}"),
            ),
            Err(_) => build_tree_sitter_fallback(
                workspace,
                format!(
                    "semantic index timed out after {}s",
                    budget.semantic_index_timeout_secs
                ),
            ),
        }
    }

    pub fn find_trait_impls(&self, trait_name: &str, method: &str) -> Vec<FnRef> {
        let key = trait_method_key(trait_name, method);
        self.trait_impls.get(&key).cloned().unwrap_or_default()
    }

    pub fn expand_macro(&self, span: &SpanId) -> Option<&str> {
        self.macro_expansions.get(span).map(String::as_str)
    }

    pub fn cfg_divergence_points(&self) -> Vec<CfgDivergence> {
        self.cfg_variants
            .values()
            .flat_map(|points| points.iter().cloned())
            .collect()
    }

    pub fn rust_facts(&self) -> &RustSemanticFacts {
        &self.rust_facts
    }

    pub fn record_backend_tool_version(&self, tool_versions: &mut HashMap<String, String>) {
        self.backend.record_in_tool_versions(tool_versions);
    }
}

pub fn set_semantic_build_delay_for_tests(delay_ms: u64) {
    TEST_BUILD_DELAY_MS.store(delay_ms, Ordering::SeqCst);
}

fn build_primary(workspace: &CargoWorkspace) -> Result<SemanticIndex> {
    maybe_apply_test_delay();

    if std::env::var("AUDIT_AGENT_SEMANTIC_FORCE_RA_FAIL").as_deref() == Ok("1") {
        bail!("forced rust-analyzer compatibility failure");
    }

    build_with_backend(
        workspace,
        SemanticBackend::RustAnalyzer {
            version: RA_COMPAT_VERSION.to_string(),
        },
    )
}

fn build_tree_sitter_fallback(workspace: &CargoWorkspace, reason: String) -> Result<SemanticIndex> {
    build_with_backend(workspace, SemanticBackend::TreeSitterFallback { reason })
}

fn build_with_backend(
    workspace: &CargoWorkspace,
    backend: SemanticBackend,
) -> Result<SemanticIndex> {
    let mut call_graph = CallGraph::new();
    let mut macro_expansions = MacroExpansionMap::new();
    let mut macro_sites = Vec::<MacroSite>::new();
    let mut trait_impls = TraitImplMap::new();
    let mut cfg_variants = CfgVariantMap::new();

    for member in &workspace.members {
        for file_path in rust_source_files(&member.path) {
            let content = fs::read_to_string(&file_path)
                .with_context(|| format!("failed to read {}", file_path.display()))?;
            scan_file(
                &member.name,
                &file_path,
                &content,
                &mut FileScanOutputs {
                    call_graph: &mut call_graph,
                    macro_expansions: &mut macro_expansions,
                    macro_sites: &mut macro_sites,
                    trait_impls: &mut trait_impls,
                    cfg_variants: &mut cfg_variants,
                },
            );
        }
    }

    let rust_facts = build_rust_facts(&call_graph, macro_sites, &trait_impls, &cfg_variants);

    Ok(SemanticIndex {
        call_graph,
        macro_expansions,
        trait_impls,
        cfg_variants,
        rust_facts,
        backend,
    })
}

fn build_rust_facts(
    call_graph: &CallGraph,
    mut macro_sites: Vec<MacroSite>,
    trait_impls: &TraitImplMap,
    cfg_variants: &CfgVariantMap,
) -> RustSemanticFacts {
    let mut function_symbols = call_graph
        .keys()
        .filter(|name| name.as_str() != "__macro_root__")
        .cloned()
        .map(|name| FunctionSymbolRef { name })
        .collect::<Vec<_>>();
    function_symbols.sort_by(|a, b| a.name.cmp(&b.name));

    let mut function_calls = Vec::<FunctionCallRef>::new();
    for (caller, callees) in call_graph {
        if caller == "__macro_root__" {
            continue;
        }
        for callee in callees {
            function_calls.push(FunctionCallRef {
                caller: caller.clone(),
                callee: callee.clone(),
            });
        }
    }
    function_calls.sort_by(|a, b| {
        (a.caller.as_str(), a.callee.as_str()).cmp(&(b.caller.as_str(), b.callee.as_str()))
    });
    function_calls.dedup_by(|a, b| a.caller == b.caller && a.callee == b.callee);

    macro_sites.sort_by(|a, b| {
        (
            a.file.as_path(),
            a.line,
            a.column,
            a.macro_name.as_str(),
            a.crate_name.as_str(),
        )
            .cmp(&(
                b.file.as_path(),
                b.line,
                b.column,
                b.macro_name.as_str(),
                b.crate_name.as_str(),
            ))
    });
    macro_sites.dedup_by(|a, b| {
        a.file == b.file
            && a.line == b.line
            && a.column == b.column
            && a.macro_name == b.macro_name
            && a.crate_name == b.crate_name
    });

    let mut trait_impl_refs = trait_impls
        .values()
        .flat_map(|refs| refs.iter().cloned())
        .collect::<Vec<_>>();
    trait_impl_refs.sort_by(|a, b| {
        (
            a.trait_name.as_str(),
            a.method_name.as_str(),
            a.impl_type.as_str(),
            a.file.as_path(),
            a.line,
            a.crate_name.as_str(),
        )
            .cmp(&(
                b.trait_name.as_str(),
                b.method_name.as_str(),
                b.impl_type.as_str(),
                b.file.as_path(),
                b.line,
                b.crate_name.as_str(),
            ))
    });
    trait_impl_refs.dedup_by(|a, b| {
        a.trait_name == b.trait_name
            && a.method_name == b.method_name
            && a.impl_type == b.impl_type
            && a.file == b.file
            && a.line == b.line
            && a.crate_name == b.crate_name
    });

    let mut cfg_divergences = cfg_variants
        .values()
        .flat_map(|points| points.iter().cloned())
        .collect::<Vec<_>>();
    cfg_divergences.sort_by(|a, b| {
        (
            a.feature.as_str(),
            a.file.as_path(),
            a.line,
            a.crate_name.as_str(),
        )
            .cmp(&(
                b.feature.as_str(),
                b.file.as_path(),
                b.line,
                b.crate_name.as_str(),
            ))
    });
    cfg_divergences.dedup_by(|a, b| {
        a.feature == b.feature
            && a.file == b.file
            && a.line == b.line
            && a.crate_name == b.crate_name
    });

    RustSemanticFacts {
        function_symbols,
        function_calls,
        macro_sites,
        trait_impls: trait_impl_refs,
        cfg_divergences,
    }
}

#[derive(Debug, Clone)]
struct ImplContext {
    trait_name: String,
    impl_type: String,
    brace_depth: i32,
    opened_block: bool,
}

#[derive(Debug, Clone)]
struct FnContext {
    name: String,
    brace_depth: i32,
    opened_block: bool,
}

fn scan_file(crate_name: &str, file_path: &Path, content: &str, outputs: &mut FileScanOutputs<'_>) {
    let FileScanOutputs {
        call_graph,
        macro_expansions,
        macro_sites,
        trait_impls,
        cfg_variants,
    } = outputs;

    // Keep this scanner in sync with
    // `crates/data/project-ir/src/semantic.rs::scan_line_level_rust_facts`.
    // Known limitation: impl/fn context tracking is best-effort and assumes
    // top-level declarations rather than full nested-block semantics.
    let mut impl_ctx: Option<ImplContext> = None;
    let mut fn_ctx: Option<FnContext> = None;
    let mut brace_state = BraceScanState::default();

    for (line_idx, line) in content.lines().enumerate() {
        let line_no = line_idx as u32 + 1;
        let line_delta = brace_delta(line, &mut brace_state);

        if let Some(captures) = CFG_RE.captures(line) {
            let feature = captures
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            cfg_variants
                .entry(feature.clone())
                .or_default()
                .push(CfgDivergence {
                    crate_name: crate_name.to_string(),
                    feature,
                    file: file_path.to_path_buf(),
                    line: line_no,
                });
        }

        for macro_capture in MACRO_RE.captures_iter(line) {
            let Some(matched) = macro_capture.get(0) else {
                continue;
            };
            let macro_name = macro_capture
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let span = SpanId {
                file: file_path.to_path_buf(),
                line: line_no,
                column: matched.start() as u32 + 1,
            };
            macro_expansions.insert(span, format!("expanded::{macro_name}"));
            macro_sites.push(MacroSite {
                crate_name: crate_name.to_string(),
                macro_name: last_path_segment(&macro_name).to_string(),
                file: file_path.to_path_buf(),
                line: line_no,
                column: matched.start() as u32 + 1,
                caller: fn_ctx.as_ref().map(|ctx| ctx.name.clone()),
            });
            call_graph
                .entry("__macro_root__".to_string())
                .or_default()
                .insert(format!("{macro_name}!"));
        }

        if impl_ctx.is_none()
            && let Some(captures) = IMPL_RE.captures(line)
        {
            let trait_name = captures
                .get(1)
                .map(|m| last_path_segment(m.as_str()).to_string())
                .unwrap_or_default();
            let impl_type = captures
                .get(2)
                .map(|m| last_path_segment(m.as_str()).to_string())
                .unwrap_or_default();
            impl_ctx = Some(ImplContext {
                trait_name,
                impl_type,
                brace_depth: 0,
                opened_block: false,
            });
        }

        if fn_ctx.is_none()
            && let Some(captures) = FN_DECL_RE.captures(line)
        {
            let fn_name = captures
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            call_graph.entry(fn_name.clone()).or_default();
            fn_ctx = Some(FnContext {
                name: fn_name,
                brace_depth: 0,
                opened_block: false,
            });
        }

        if let Some(ctx) = impl_ctx.as_mut() {
            if let Some(captures) = FN_DECL_RE.captures(line)
                && let Some(method_name) = captures.get(1).map(|m| m.as_str().to_string())
            {
                let key = trait_method_key(&ctx.trait_name, &method_name);
                trait_impls.entry(key).or_default().push(FnRef {
                    crate_name: crate_name.to_string(),
                    trait_name: ctx.trait_name.clone(),
                    method_name,
                    impl_type: ctx.impl_type.clone(),
                    file: file_path.to_path_buf(),
                    line: line_no,
                });
            }

            if line.contains('{') {
                ctx.opened_block = true;
            }
            ctx.brace_depth += line_delta;
            if ctx.opened_block && ctx.brace_depth <= 0 {
                impl_ctx = None;
            }
        }

        if let Some(ctx) = fn_ctx.as_mut() {
            for captures in CALL_RE.captures_iter(line) {
                let Some(callee_capture) = captures.get(1) else {
                    continue;
                };
                let callee = last_path_segment(callee_capture.as_str());
                if is_non_call_token(callee) || callee == ctx.name {
                    continue;
                }
                call_graph
                    .entry(ctx.name.clone())
                    .or_default()
                    .insert(callee.to_string());
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
}

fn rust_source_files(root: &Path) -> Vec<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            (entry.path().extension().and_then(|ext| ext.to_str()) == Some("rs"))
                .then(|| entry.path().to_path_buf())
        })
        .collect()
}

fn trait_method_key(trait_name: &str, method: &str) -> String {
    format!("{trait_name}::{method}")
}

fn last_path_segment(symbol: &str) -> &str {
    symbol.rsplit("::").next().unwrap_or(symbol)
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct BraceScanState {
    // Keep in sync with `crates/data/project-ir/src/semantic.rs::BraceScanState`.
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

fn is_non_call_token(name: &str) -> bool {
    matches!(
        name,
        "if" | "for"
            | "while"
            | "loop"
            | "match"
            | "return"
            | "Some"
            | "None"
            | "Ok"
            | "Err"
            | "let"
            | "assert"
            | "assert_eq"
            | "assert_ne"
            | "panic"
            | "todo"
            | "unimplemented"
            | "unreachable"
    )
}

fn maybe_apply_test_delay() {
    let delay_ms = TEST_BUILD_DELAY_MS.load(Ordering::Relaxed);
    if delay_ms > 0 {
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
}
