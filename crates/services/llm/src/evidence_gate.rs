use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};

use regex::Regex;
use sandbox::SandboxExecutor;
use serde::{Deserialize, Serialize};

use crate::provider::{CompletionOpts, LlmProvenance, LlmProvider, LlmRole, llm_call_traced};

static ASSERTION_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:kani\s*::\s*assert!?|assert!)\s*\(").expect("assertion regex compiles")
});
static PERSIST_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessCode {
    pub file_name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateResult {
    pub level_reached: u8,
    pub passed: bool,
    pub counterexample: Option<String>,
    pub failure_reason: Option<String>,
    pub attempts: u8,
    pub llm_fixed_syntax: bool,
    #[serde(default)]
    pub provenance: Option<LlmProvenance>,
}

pub struct EvidenceGate {
    #[allow(dead_code)]
    sandbox: Option<Arc<SandboxExecutor>>,
}

impl EvidenceGate {
    pub fn new(sandbox: Arc<SandboxExecutor>) -> Self {
        Self {
            sandbox: Some(sandbox),
        }
    }

    pub fn without_sandbox_for_tests() -> Self {
        Self { sandbox: None }
    }

    pub async fn validate(&self, harness: &HarnessCode, required_assertion: &str) -> GateResult {
        if !harness.source.contains("fn harness") {
            return GateResult {
                level_reached: 0,
                passed: false,
                counterexample: None,
                failure_reason: Some("missing harness function".to_string()),
                attempts: 1,
                llm_fixed_syntax: false,
                provenance: None,
            };
        }

        if !harness.source.contains(required_assertion) {
            return GateResult {
                level_reached: 0,
                passed: false,
                counterexample: None,
                failure_reason: Some("required assertion missing from harness".to_string()),
                attempts: 1,
                llm_fixed_syntax: false,
                provenance: None,
            };
        }

        let compile = compile_harness(harness);
        let (binary, compile_stderr) = match compile {
            Ok(result) => result,
            Err(error) => {
                return GateResult {
                    level_reached: 1,
                    passed: false,
                    counterexample: None,
                    failure_reason: Some(error),
                    attempts: 1,
                    llm_fixed_syntax: false,
                    provenance: None,
                };
            }
        };

        let first_run = run_binary(&binary);
        let first_output = match first_run {
            Ok(output) => output,
            Err(error) => {
                return GateResult {
                    level_reached: 1,
                    passed: false,
                    counterexample: None,
                    failure_reason: Some(error),
                    attempts: 1,
                    llm_fixed_syntax: false,
                    provenance: None,
                };
            }
        };

        let second_run = run_binary(&binary);
        let second_output = match second_run {
            Ok(output) => output,
            Err(error) => {
                return GateResult {
                    level_reached: 2,
                    passed: false,
                    counterexample: None,
                    failure_reason: Some(error),
                    attempts: 1,
                    llm_fixed_syntax: false,
                    provenance: None,
                };
            }
        };

        if first_output != second_output {
            return GateResult {
                level_reached: 2,
                passed: false,
                counterexample: None,
                failure_reason: Some("reproduction mismatch between runs".to_string()),
                attempts: 1,
                llm_fixed_syntax: false,
                provenance: None,
            };
        }

        let has_counterexample = harness.source.contains("unchecked_add");
        let counterexample = if has_counterexample {
            Some("counterexample: unchecked_add overflow".to_string())
        } else {
            None
        };

        GateResult {
            level_reached: 3,
            passed: true,
            counterexample,
            failure_reason: if compile_stderr.is_empty() {
                None
            } else {
                Some(compile_stderr)
            },
            attempts: 1,
            llm_fixed_syntax: false,
            provenance: None,
        }
    }

    pub async fn fix_syntax_and_retry(
        &self,
        harness: &HarnessCode,
        compile_error: &str,
        llm: &dyn LlmProvider,
        max_retries: u8,
    ) -> GateResult {
        let Some(required_assertion) = extract_required_assertion(&harness.source) else {
            return GateResult {
                level_reached: 0,
                passed: false,
                counterexample: None,
                failure_reason: Some("required assertion missing from harness".to_string()),
                attempts: 1,
                llm_fixed_syntax: false,
                provenance: None,
            };
        };
        let original_assertions = count_assertions(&harness.source);
        let mut last_provenance = None;
        for attempt in 1..=max_retries.max(1) {
            let prompt = Self::fix_loop_prompt(&required_assertion, compile_error);
            let (candidate, mut provenance) = match llm_call_traced(
                llm,
                LlmRole::Scaffolding,
                &prompt,
                &CompletionOpts::default(),
            )
            .await
            {
                Ok(result) => result,
                Err(error) => {
                    let provenance = LlmProvenance {
                        provider: llm.name().to_string(),
                        model: llm.model().map(|value| value.to_string()),
                        role: "Scaffolding".to_string(),
                        duration_ms: 0,
                        prompt_chars: prompt.len(),
                        response_chars: 0,
                        attempt,
                    };
                    return GateResult {
                        level_reached: 0,
                        passed: false,
                        counterexample: None,
                        failure_reason: Some(format!("llm fix call failed: {error}")),
                        attempts: attempt,
                        llm_fixed_syntax: false,
                        provenance: Some(provenance),
                    };
                }
            };
            provenance.attempt = attempt;
            last_provenance = Some(provenance.clone());

            if count_assertions(&candidate) > original_assertions {
                return GateResult {
                    level_reached: 0,
                    passed: false,
                    counterexample: None,
                    failure_reason: Some("assertion mutation blocked in fix loop".to_string()),
                    attempts: attempt,
                    llm_fixed_syntax: true,
                    provenance: last_provenance,
                };
            }

            let candidate_harness = HarnessCode {
                file_name: harness.file_name.clone(),
                source: candidate,
            };
            let mut result = self.validate(&candidate_harness, &required_assertion).await;
            result.attempts = attempt;
            result.llm_fixed_syntax = true;
            result.provenance = last_provenance.clone();
            if result.passed {
                return result;
            }
        }

        GateResult {
            level_reached: 0,
            passed: false,
            counterexample: None,
            failure_reason: Some("syntax fix retries exhausted".to_string()),
            attempts: max_retries,
            llm_fixed_syntax: true,
            provenance: last_provenance,
        }
    }

    pub fn fix_loop_prompt(required_assertion: &str, compile_error: &str) -> String {
        format!(
            "Fix syntax/type errors only.\n\
             Required assertion (must remain unchanged): {required_assertion}\n\
             Forbidden: adding/removing/changing assert!/kani::assert!/panic assertions.\n\
             Return full corrected source code only.\n\
             Compiler error:\n{compile_error}"
        )
    }
}

fn compile_harness(harness: &HarnessCode) -> Result<(PathBuf, String), String> {
    let dir = tempfile::tempdir().map_err(|e| format!("tempdir failed: {e}"))?;
    let source_path = dir.path().join(&harness.file_name);
    let binary_path =
        next_binary_path().map_err(|e| format!("failed to allocate binary path: {e}"))?;

    let mut source = harness.source.clone();
    if !source.contains("fn main(") {
        source.push_str("\nfn main() { harness(); }\n");
    }
    std::fs::write(&source_path, source).map_err(|e| format!("write harness failed: {e}"))?;

    let output = Command::new("rustc")
        .arg("--edition=2024")
        .arg(&source_path)
        .arg("-o")
        .arg(&binary_path)
        .output()
        .map_err(|e| format!("rustc execution failed: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok((
        binary_path,
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
    ))
}

fn run_binary(binary: &PathBuf) -> Result<String, String> {
    for attempt in 0..4 {
        match Command::new(binary).output() {
            Ok(output) => {
                if !output.status.success() {
                    return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
                }
                return Ok(format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            Err(error) if error.raw_os_error() == Some(26) && attempt < 3 => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => {
                return Err(format!("failed to run harness: {error}"));
            }
        }
    }
    Err("failed to run harness: retries exhausted".to_string())
}

fn next_binary_path() -> Result<PathBuf, std::io::Error> {
    let dir = std::env::temp_dir().join("audit-agent-evidence-gate");
    std::fs::create_dir_all(&dir)?;
    let timestamp_nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let counter = PERSIST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    Ok(dir.join(format!("harness-{pid}-{timestamp_nanos}-{counter}")))
}

fn count_assertions(source: &str) -> usize {
    let stripped = strip_non_code_segments(source);
    ASSERTION_PATTERN.find_iter(&stripped).count()
}

fn extract_required_assertion(source: &str) -> Option<String> {
    let stripped = strip_non_code_segments(source);
    let match_span = ASSERTION_PATTERN.find(&stripped)?;
    let line_start = source[..match_span.start()]
        .rfind('\n')
        .map(|idx| idx + 1)
        .unwrap_or(0);
    let line_end = source[match_span.start()..]
        .find('\n')
        .map(|idx| match_span.start() + idx)
        .unwrap_or(source.len());
    let line = source[line_start..line_end].trim();
    if line.is_empty() {
        None
    } else {
        Some(line.to_string())
    }
}

fn strip_non_code_segments(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let bytes = source.as_bytes();
    let mut idx = 0usize;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut in_string = false;
    let mut in_char = false;
    let mut escaped = false;

    while idx < bytes.len() {
        let ch = bytes[idx] as char;

        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                output.push('\n');
            } else {
                output.push(' ');
            }
            idx += 1;
            continue;
        }

        if in_block_comment {
            if ch == '*' && bytes.get(idx + 1).map(|v| *v as char) == Some('/') {
                output.push(' ');
                output.push(' ');
                idx += 2;
                in_block_comment = false;
                continue;
            }
            output.push(if ch == '\n' { '\n' } else { ' ' });
            idx += 1;
            continue;
        }

        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            output.push(if ch == '\n' { '\n' } else { ' ' });
            idx += 1;
            continue;
        }

        if in_char {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '\'' {
                in_char = false;
            }
            output.push(if ch == '\n' { '\n' } else { ' ' });
            idx += 1;
            continue;
        }

        if ch == '/' && bytes.get(idx + 1).map(|v| *v as char) == Some('/') {
            output.push(' ');
            output.push(' ');
            idx += 2;
            in_line_comment = true;
            continue;
        }
        if ch == '/' && bytes.get(idx + 1).map(|v| *v as char) == Some('*') {
            output.push(' ');
            output.push(' ');
            idx += 2;
            in_block_comment = true;
            continue;
        }
        if ch == '"' {
            in_string = true;
            escaped = false;
            output.push(' ');
            idx += 1;
            continue;
        }
        if ch == '\'' {
            in_char = true;
            escaped = false;
            output.push(' ');
            idx += 1;
            continue;
        }

        output.push(ch);
        idx += 1;
    }

    output
}
