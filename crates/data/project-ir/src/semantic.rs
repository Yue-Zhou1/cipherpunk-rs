//! Semantic extraction for the first usable IR pipeline.
//!
//! Uses tree-sitter for Rust function/call extraction so call graphs are built
//! from parsed syntax rather than free-text matches.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result};
use regex::Regex;
use tree_sitter::{Node, Parser};
use walkdir::WalkDir;

static FN_DECL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(").expect("fn regex"));
static IMPL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\bimpl(?:\s*<[^>]+>)?\s+([A-Za-z_][A-Za-z0-9_:]*)\s+for\s+([A-Za-z_][A-Za-z0-9_:]*)",
    )
    .expect("impl regex")
});
static MACRO_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([A-Za-z_][A-Za-z0-9_:]*)!\s*\(").expect("macro regex"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSymbol {
    pub name: String,
    pub qualified_name: Option<String>,
    pub line: u32,
    pub signature: Option<FunctionSignatureSymbol>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignatureSymbol {
    pub parameters: Vec<ParameterSymbol>,
    pub return_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterSymbol {
    pub name: String,
    pub type_annotation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableSymbol {
    pub name: String,
    pub line: u32,
    pub function: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroSite {
    pub macro_name: String,
    pub line: u32,
    pub column: u32,
    pub caller: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitImplRef {
    pub trait_name: String,
    pub method_name: String,
    pub impl_type: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfgDivergence {
    pub feature: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticFile {
    pub path: PathBuf,
    pub functions: Vec<FunctionSymbol>,
    pub variables: Vec<VariableSymbol>,
    pub function_calls: Vec<FunctionCallSite>,
    pub cfg_features: Vec<String>,
    pub macro_sites: Vec<MacroSite>,
    pub trait_impls: Vec<TraitImplRef>,
    pub cfg_divergences: Vec<CfgDivergence>,
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

#[derive(Debug, Default)]
struct SemanticScanState {
    functions: Vec<FunctionSymbol>,
    variables: Vec<VariableSymbol>,
    function_calls: Vec<FunctionCallSite>,
    cfg_features: Vec<String>,
    cfg_divergences: Vec<CfgDivergence>,
}

impl SemanticScanState {
    fn into_semantic_file(
        self,
        path: PathBuf,
        macro_sites: Vec<MacroSite>,
        trait_impls: Vec<TraitImplRef>,
    ) -> SemanticFile {
        SemanticFile {
            path,
            functions: self.functions,
            variables: self.variables,
            function_calls: self.function_calls,
            cfg_features: self.cfg_features,
            macro_sites,
            trait_impls,
            cfg_divergences: self.cfg_divergences,
        }
    }
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
    let mut scan_state = SemanticScanState::default();
    let mut macro_sites = Vec::<MacroSite>::new();
    let mut trait_impls = Vec::<TraitImplRef>::new();

    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_rust::language()).is_err() {
        return scan_state.into_semantic_file(path, macro_sites, trait_impls);
    }

    let Some(tree) = parser.parse(content, None) else {
        return scan_state.into_semantic_file(path, macro_sites, trait_impls);
    };

    walk_node(
        tree.root_node(),
        content.as_bytes(),
        None,
        &[],
        &mut scan_state,
    );
    scan_line_level_rust_facts(content, &mut macro_sites, &mut trait_impls);

    scan_state.into_semantic_file(path, macro_sites, trait_impls)
}

fn walk_node(
    node: Node,
    source: &[u8],
    current_function: Option<&str>,
    module_scope: &[String],
    scan_state: &mut SemanticScanState,
) {
    if node.kind() == "attribute_item"
        && let Some(feature) = parse_cfg_feature(node, source)
    {
        let line = node.start_position().row as u32 + 1;
        if !scan_state
            .cfg_features
            .iter()
            .any(|existing| existing == &feature)
        {
            scan_state.cfg_features.push(feature.clone());
        }
        if !scan_state
            .cfg_divergences
            .iter()
            .any(|existing| existing.feature == feature && existing.line == line)
        {
            scan_state
                .cfg_divergences
                .push(CfgDivergence { feature, line });
        }
    }

    if node.kind() == "function_item"
        && let Some(name_node) = node.child_by_field_name("name")
        && let Some(name) = node_text(name_node, source)
    {
        let function_name = name.to_string();
        let qualified_name = if module_scope.is_empty() {
            function_name.clone()
        } else {
            format!("{}::{function_name}", module_scope.join("::"))
        };
        scan_state.functions.push(FunctionSymbol {
            name: function_name.clone(),
            qualified_name: Some(qualified_name),
            line: node.start_position().row as u32 + 1,
            signature: extract_function_signature(node, source),
        });
        collect_calls_in_function(node, source, &function_name, &mut scan_state.function_calls);

        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            walk_node(
                child,
                source,
                Some(&function_name),
                module_scope,
                scan_state,
            );
        }
        return;
    }

    if node.kind() == "let_declaration"
        && let Some(pattern) = node.child_by_field_name("pattern")
        && let Some(name) = first_pattern_identifier(pattern, source)
    {
        scan_state.variables.push(VariableSymbol {
            name: name.to_string(),
            line: node.start_position().row as u32 + 1,
            function: current_function.map(str::to_string),
        });
    }

    if (node.kind() == "const_item" || node.kind() == "static_item")
        && let Some(name_node) = node.child_by_field_name("name")
        && let Some(name) = node_text(name_node, source)
    {
        scan_state.variables.push(VariableSymbol {
            name: name.to_string(),
            line: node.start_position().row as u32 + 1,
            function: None,
        });
    }

    if node.kind() == "mod_item"
        && let Some(name_node) = node.child_by_field_name("name")
        && let Some(name) = node_text(name_node, source)
    {
        let mut nested_scope = module_scope.to_vec();
        nested_scope.push(name.to_string());

        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            walk_node(child, source, current_function, &nested_scope, scan_state);
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_node(child, source, current_function, module_scope, scan_state);
    }
}

fn extract_function_signature(node: Node, source: &[u8]) -> Option<FunctionSignatureSymbol> {
    let params_node = node.child_by_field_name("parameters")?;
    let mut parameters = Vec::<ParameterSymbol>::new();

    let mut cursor = params_node.walk();
    for parameter_node in params_node.named_children(&mut cursor) {
        if parameter_node.kind() == "self_parameter" {
            parameters.push(ParameterSymbol {
                name: "self".to_string(),
                type_annotation: None,
            });
            continue;
        }

        if parameter_node.kind() != "parameter" {
            continue;
        }

        let name = parameter_node
            .child_by_field_name("pattern")
            .and_then(|pattern| node_text(pattern, source))
            .unwrap_or("_")
            .trim()
            .to_string();
        let type_annotation = parameter_node
            .child_by_field_name("type")
            .and_then(|ty| node_text(ty, source))
            .map(str::trim)
            .map(str::to_string)
            .filter(|value| !value.is_empty());
        parameters.push(ParameterSymbol {
            name,
            type_annotation,
        });
    }

    let return_type = node
        .child_by_field_name("return_type")
        .and_then(|ty| node_text(ty, source))
        .map(str::trim)
        .map(|value| value.trim_start_matches("->").trim().to_string())
        .filter(|value| !value.is_empty());

    Some(FunctionSignatureSymbol {
        parameters,
        return_type,
    })
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

fn scan_line_level_rust_facts(
    content: &str,
    macro_sites: &mut Vec<MacroSite>,
    trait_impls: &mut Vec<TraitImplRef>,
) {
    // Keep this lightweight scanner in sync with
    // `crates/engines/crypto/src/semantic/ra_client.rs::scan_file`.
    // Both intentionally duplicate brace/comment handling to avoid a crate cycle.
    // Known limitation: impl/fn context tracking is best-effort and assumes
    // top-level declarations rather than full nested-block semantics.
    let mut impl_ctx: Option<ImplContext> = None;
    let mut fn_ctx: Option<FnContext> = None;
    let mut brace_state = BraceScanState::default();

    for (line_idx, line) in content.lines().enumerate() {
        let line_no = line_idx as u32 + 1;
        let line_delta = brace_delta(line, &mut brace_state);

        for macro_capture in MACRO_RE.captures_iter(line) {
            let Some(matched) = macro_capture.get(0) else {
                continue;
            };
            let macro_name = macro_capture
                .get(1)
                .map(|value| last_path_segment(value.as_str()).to_string())
                .unwrap_or_default();
            macro_sites.push(MacroSite {
                macro_name,
                line: line_no,
                column: matched.start() as u32 + 1,
                caller: fn_ctx.as_ref().map(|ctx| ctx.name.clone()),
            });
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
                trait_impls.push(TraitImplRef {
                    trait_name: ctx.trait_name.clone(),
                    method_name,
                    impl_type: ctx.impl_type.clone(),
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

fn collect_calls_in_function(
    function_node: Node,
    source: &[u8],
    caller: &str,
    out: &mut Vec<FunctionCallSite>,
) {
    let start = function_node
        .child_by_field_name("body")
        .unwrap_or(function_node);
    let mut stack = vec![start];

    while let Some(node) = stack.pop() {
        if node.kind() == "function_item" && node.start_byte() != function_node.start_byte() {
            continue;
        }

        if node.kind() == "call_expression"
            && let Some(function_expr) = node.child_by_field_name("function")
            && let Some(callee) = extract_callee(function_expr, source)
            && !should_ignore_call_symbol(&callee)
            && callee != caller
        {
            out.push(FunctionCallSite {
                caller: caller.to_string(),
                callee,
            });
        }

        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
}

fn extract_callee(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "field_identifier" | "type_identifier" => {
            node_text(node, source).map(|value| value.to_string())
        }
        "field_expression" => node
            .child_by_field_name("field")
            .and_then(|field| extract_callee(field, source)),
        "generic_function" => node
            .child_by_field_name("function")
            .and_then(|inner| extract_callee(inner, source)),
        "scoped_identifier" | "scoped_type_identifier" => node
            .child_by_field_name("name")
            .and_then(|name| extract_callee(name, source))
            .or_else(|| last_named_child(node).and_then(|child| extract_callee(child, source))),
        _ => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if let Some(callee) = extract_callee(child, source) {
                    return Some(callee);
                }
            }
            None
        }
    }
}

fn parse_cfg_feature(node: Node, source: &[u8]) -> Option<String> {
    let mut tokens = Vec::<AttributeToken<'_>>::new();
    collect_attribute_tokens(node, source, &mut tokens);

    let mut saw_cfg = false;
    let mut await_feature_value = false;

    for token in tokens {
        match token.kind {
            AttributeTokenKind::Identifier => {
                if token.text == "cfg" {
                    saw_cfg = true;
                    await_feature_value = false;
                } else if saw_cfg && token.text == "feature" {
                    await_feature_value = true;
                }
            }
            AttributeTokenKind::StringLiteral if saw_cfg && await_feature_value => {
                if let Some(value) = parse_string_literal_value(token.text) {
                    return Some(value);
                }
            }
            AttributeTokenKind::StringLiteral => {}
        }
    }

    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttributeTokenKind {
    Identifier,
    StringLiteral,
}

#[derive(Debug, Clone, Copy)]
struct AttributeToken<'a> {
    kind: AttributeTokenKind,
    text: &'a str,
}

fn collect_attribute_tokens<'a>(node: Node, source: &'a [u8], out: &mut Vec<AttributeToken<'a>>) {
    match node.kind() {
        "identifier" | "field_identifier" => {
            if let Some(text) = node_text(node, source) {
                out.push(AttributeToken {
                    kind: AttributeTokenKind::Identifier,
                    text,
                });
            }
        }
        "string_literal" => {
            if let Some(text) = node_text(node, source) {
                out.push(AttributeToken {
                    kind: AttributeTokenKind::StringLiteral,
                    text,
                });
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_attribute_tokens(child, source, out);
    }
}

fn parse_string_literal_value(text: &str) -> Option<String> {
    if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        return Some(text[1..text.len() - 1].to_string());
    }

    let first_quote = text.find('"')?;
    let last_quote = text.rfind('"')?;
    (last_quote > first_quote).then(|| text[first_quote + 1..last_quote].to_string())
}

fn first_pattern_identifier<'a>(node: Node, source: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "identifier" | "field_identifier" | "shorthand_field_identifier" => {
            return node_text(node, source);
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(name) = first_pattern_identifier(child, source) {
            return Some(name);
        }
    }

    None
}

fn node_text<'a>(node: Node, source: &'a [u8]) -> Option<&'a str> {
    node.utf8_text(source).ok()
}

fn last_named_child(node: Node) -> Option<Node> {
    for idx in (0..node.named_child_count()).rev() {
        if let Some(child) = node.named_child(idx) {
            return Some(child);
        }
    }
    None
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

fn last_path_segment(symbol: &str) -> &str {
    symbol.rsplit("::").next().unwrap_or(symbol)
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct BraceScanState {
    // Keep in sync with `crates/engines/crypto/src/semantic/ra_client.rs::BraceScanState`.
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::scan_rust_file;

    #[test]
    fn scanner_ignores_calls_embedded_in_comments_and_strings() {
        let source = r#"
            fn alpha() {
                let _ = "bogus_call()";
                // ignored_call();
                helper();
            }

            fn helper() {}
        "#;

        let file = scan_rust_file(PathBuf::from("src/lib.rs"), source);
        assert_eq!(file.functions.len(), 2);
        assert_eq!(file.function_calls.len(), 1);
        assert_eq!(file.function_calls[0].caller, "alpha");
        assert_eq!(file.function_calls[0].callee, "helper");
    }

    #[test]
    fn scanner_extracts_cfg_feature_flags() {
        let source = r#"
            #[cfg(feature = "fast-path")]
            fn run() {}
        "#;

        let file = scan_rust_file(PathBuf::from("src/lib.rs"), source);
        assert_eq!(file.cfg_features, vec!["fast-path".to_string()]);
    }

    #[test]
    fn scanner_extracts_cfg_feature_from_nested_cfg_expression() {
        let source = r#"
            #[cfg(all(feature = "fast-path", target_os = "linux"))]
            fn run() {}
        "#;

        let file = scan_rust_file(PathBuf::from("src/lib.rs"), source);
        assert_eq!(file.cfg_features, vec!["fast-path".to_string()]);
    }

    #[test]
    fn scanner_ignores_non_cfg_attributes_when_collecting_features() {
        let source = r#"
            #[doc = "feature = \"not-a-cfg-feature\""]
            fn run() {}
        "#;

        let file = scan_rust_file(PathBuf::from("src/lib.rs"), source);
        assert!(file.cfg_features.is_empty());
    }

    #[test]
    fn scanner_extracts_variable_symbols_with_function_context() {
        let source = r#"
            const LIMIT: usize = 32;
            static ENABLED: bool = true;

            fn run(input: usize) -> usize {
                let local = input + 1;
                local
            }
        "#;

        let file = scan_rust_file(PathBuf::from("src/lib.rs"), source);
        assert!(
            file.variables.iter().any(|variable| {
                variable.name == "LIMIT" && variable.line == 2 && variable.function.is_none()
            }),
            "const declarations should be surfaced as top-level variable symbols"
        );
        assert!(
            file.variables.iter().any(|variable| {
                variable.name == "ENABLED" && variable.line == 3 && variable.function.is_none()
            }),
            "static declarations should be surfaced as top-level variable symbols"
        );
        assert!(
            file.variables.iter().any(|variable| {
                variable.name == "local"
                    && variable.line == 6
                    && variable.function.as_deref() == Some("run")
            }),
            "let declarations should include the enclosing function name"
        );
    }

    #[test]
    fn scanner_builds_qualified_names_with_module_scope() {
        let source = r#"
            mod outer {
                pub mod inner {
                    pub fn run() {}
                }
            }
        "#;

        let file = scan_rust_file(PathBuf::from("src/lib.rs"), source);
        let run = file
            .functions
            .iter()
            .find(|function| function.name == "run")
            .expect("run function");
        assert_eq!(run.qualified_name.as_deref(), Some("outer::inner::run"));
    }
}
