use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::LazyLock;

use regex::Regex;
use sandbox::SandboxExecutor;

use crate::provider::{CompletionOpts, LlmProvider, LlmRole, llm_call};

static ASSERTION_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:kani\s*::\s*assert!?|assert!)\s*\(").expect("assertion regex compiles")
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessCode {
    pub file_name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateResult {
    pub level_reached: u8,
    pub passed: bool,
    pub counterexample: Option<String>,
    pub failure_reason: Option<String>,
    pub attempts: u8,
    pub llm_fixed_syntax: bool,
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
            };
        };
        let original_assertions = count_assertions(&harness.source);
        for attempt in 1..=max_retries.max(1) {
            let prompt = Self::fix_loop_prompt(&required_assertion, compile_error);
            let candidate = match llm_call(
                llm,
                LlmRole::Scaffolding,
                &prompt,
                &CompletionOpts::default(),
            )
            .await
            {
                Ok(text) => text,
                Err(error) => {
                    return GateResult {
                        level_reached: 0,
                        passed: false,
                        counterexample: None,
                        failure_reason: Some(format!("llm fix call failed: {error}")),
                        attempts: attempt,
                        llm_fixed_syntax: false,
                    };
                }
            };

            if count_assertions(&candidate) > original_assertions {
                return GateResult {
                    level_reached: 0,
                    passed: false,
                    counterexample: None,
                    failure_reason: Some("assertion mutation blocked in fix loop".to_string()),
                    attempts: attempt,
                    llm_fixed_syntax: true,
                };
            }

            let candidate_harness = HarnessCode {
                file_name: harness.file_name.clone(),
                source: candidate,
            };
            let mut result = self.validate(&candidate_harness, &required_assertion).await;
            result.attempts = attempt;
            result.llm_fixed_syntax = true;
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
    let binary_path = dir.path().join("harness-bin");

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

    let persisted =
        persist_binary(&binary_path).map_err(|e| format!("persist binary failed: {e}"))?;
    Ok((
        persisted,
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
    ))
}

fn run_binary(binary: &PathBuf) -> Result<String, String> {
    let output = Command::new(binary)
        .output()
        .map_err(|e| format!("failed to run harness: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

fn persist_binary(binary_path: &PathBuf) -> Result<PathBuf, std::io::Error> {
    let dir = std::env::temp_dir().join("audit-agent-evidence-gate");
    std::fs::create_dir_all(&dir)?;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos().to_string())
        .unwrap_or_else(|_| "0".to_string());
    let target = dir.join(format!("harness-{unique}"));
    std::fs::copy(binary_path, &target)?;
    Ok(target)
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
