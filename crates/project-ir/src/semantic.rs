//! Semantic extraction for the first usable IR pipeline.
//!
//! Uses tree-sitter for Rust function/call extraction so call graphs are built
//! from parsed syntax rather than free-text matches.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tree_sitter::{Node, Parser};
use walkdir::WalkDir;

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

    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_rust::language()).is_err() {
        return SemanticFile {
            path,
            functions,
            function_calls,
            cfg_features,
        };
    }

    let Some(tree) = parser.parse(content, None) else {
        return SemanticFile {
            path,
            functions,
            function_calls,
            cfg_features,
        };
    };

    walk_node(
        tree.root_node(),
        content.as_bytes(),
        &mut functions,
        &mut function_calls,
        &mut cfg_features,
    );

    SemanticFile {
        path,
        functions,
        function_calls,
        cfg_features,
    }
}

fn walk_node(
    node: Node,
    source: &[u8],
    functions: &mut Vec<FunctionSymbol>,
    function_calls: &mut Vec<FunctionCallSite>,
    cfg_features: &mut Vec<String>,
) {
    if node.kind() == "attribute_item"
        && let Some(feature) = parse_cfg_feature(node, source)
        && !cfg_features.iter().any(|existing| existing == &feature)
    {
        cfg_features.push(feature);
    }

    if node.kind() == "function_item"
        && let Some(name_node) = node.child_by_field_name("name")
        && let Some(name) = node_text(name_node, source)
    {
        functions.push(FunctionSymbol {
            name: name.to_string(),
            line: node.start_position().row as u32 + 1,
        });
        collect_calls_in_function(node, source, name, function_calls);
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_node(child, source, functions, function_calls, cfg_features);
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
}
