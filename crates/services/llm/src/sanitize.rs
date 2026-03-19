use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow};
use serde::de::DeserializeOwned;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphContextEntry {
    pub node_id: String,
    pub content: String,
}

pub fn sanitize_prompt_input(text: &str) -> String {
    sanitize_prompt_input_with_limit(text, 4_000)
}

pub fn sanitize_prompt_input_with_limit(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let mut cleaned = String::with_capacity(text.len().min(max_chars));
    let mut char_count = 0usize;
    for ch in text.chars() {
        if ch == '\n' || ch == '\t' || !ch.is_control() {
            cleaned.push(ch);
            char_count += 1;
        }
        if char_count >= max_chars {
            break;
        }
    }

    cleaned
        .replace("```", "'''")
        .replace("<|", "< ")
        .replace("|>", " >")
        .replace("<<", "< ")
        .replace(">>", " >")
}

pub fn pack_graph_aware_context(
    source_context: &str,
    graph_context: &[GraphContextEntry],
    max_chars: usize,
) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let mut ordered = BTreeMap::<String, String>::new();
    for entry in graph_context {
        let node_id = entry.node_id.trim();
        let content = entry.content.trim();
        if node_id.is_empty() || content.is_empty() {
            continue;
        }
        ordered
            .entry(node_id.to_string())
            .or_insert_with(|| content.to_string());
    }

    let mut packed = String::new();
    let mut remaining = max_chars;
    for (node_id, content) in ordered {
        if remaining == 0 {
            break;
        }
        let block = format!("node={node_id}\n{content}\n");
        let block_chars = block.chars().count();
        if block_chars <= remaining {
            packed.push_str(&block);
            remaining -= block_chars;
            continue;
        }

        packed.extend(block.chars().take(remaining));
        remaining = 0;
    }

    if packed.trim().is_empty() {
        return sanitize_prompt_input_with_limit(source_context, max_chars);
    }

    packed
}

pub fn parse_json_contract<T: DeserializeOwned>(response: &str) -> Result<T> {
    let trimmed = response.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("empty JSON response"));
    }

    let json_payload = strip_code_fence(trimmed);
    serde_json::from_str::<T>(json_payload).context("invalid structured JSON response")
}

fn strip_code_fence(value: &str) -> &str {
    if !value.starts_with("```") {
        return value;
    }

    let without_start = value
        .trim_start_matches("```json")
        .trim_start_matches("```");
    without_start.trim_end_matches("```").trim()
}
