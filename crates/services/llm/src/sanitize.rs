use anyhow::{Context, Result, anyhow};
use serde::de::DeserializeOwned;

pub fn sanitize_prompt_input(text: &str) -> String {
    const MAX_CHARS: usize = 4_000;
    let mut cleaned = String::with_capacity(text.len().min(MAX_CHARS));
    let mut char_count = 0usize;
    for ch in text.chars() {
        if ch == '\n' || ch == '\t' || !ch.is_control() {
            cleaned.push(ch);
            char_count += 1;
        }
        if char_count >= MAX_CHARS {
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
