use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use async_trait::async_trait;
use audit_agent_core::audit_config::BudgetConfig;
use num_bigint::BigUint;
use rand::{Rng, SeedableRng};
use regex::Regex;
use sandbox::{ExecutionRequest, Mount, NetworkPolicy, ResourceBudget, SandboxExecutor, ToolImage};

use crate::zk::circom::signal_graph::{
    CircomSignalGraph, Constraint, LinearCombination, SignalKind,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterexamplePair {
    pub witness_a: HashMap<String, BigUint>,
    pub witness_b: HashMap<String, BigUint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Z3CheckResult {
    UnderConstrained {
        witness_a: HashMap<String, BigUint>,
        witness_b: HashMap<String, BigUint>,
        smt2_file: PathBuf,
        container_digest: String,
    },
    Constrained {
        proof_file: PathBuf,
        container_digest: String,
    },
    Unknown {
        reason: String,
        fallback_result: Option<CounterexamplePair>,
        seed: Option<u64>,
        container_digest: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Z3ExecutionOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub container_digest: String,
}

#[async_trait]
pub trait Z3ExecutionRunner: Send + Sync {
    async fn execute(&self, smt2_file: &Path, timeout_secs: u64) -> Result<Z3ExecutionOutput>;
}

pub struct SandboxZ3Runner {
    sandbox: Arc<SandboxExecutor>,
}

impl SandboxZ3Runner {
    pub fn new(sandbox: Arc<SandboxExecutor>) -> Self {
        Self { sandbox }
    }
}

#[async_trait]
impl Z3ExecutionRunner for SandboxZ3Runner {
    async fn execute(&self, smt2_file: &Path, timeout_secs: u64) -> Result<Z3ExecutionOutput> {
        let parent = smt2_file
            .parent()
            .with_context(|| format!("SMT2 file {} has no parent", smt2_file.display()))?;
        let file_name = smt2_file
            .file_name()
            .and_then(|f| f.to_str())
            .with_context(|| format!("SMT2 file {} has invalid filename", smt2_file.display()))?;

        let request = ExecutionRequest {
            image: ToolImage::Z3,
            command: vec!["z3".to_string(), format!("/work/{file_name}")],
            mounts: vec![Mount {
                host_path: parent.to_path_buf(),
                container_path: PathBuf::from("/work"),
                read_only: false,
            }],
            env: HashMap::new(),
            budget: ResourceBudget {
                cpu_cores: 1.0,
                memory_mb: 1024,
                disk_gb: 2,
                timeout_secs,
            },
            network: NetworkPolicy::Disabled,
        };

        let output = self
            .sandbox
            .execute(request)
            .await
            .context("failed to execute Z3 in sandbox")?;
        Ok(Z3ExecutionOutput {
            stdout: output.stdout,
            stderr: output.stderr,
            exit_code: output.exit_code,
            container_digest: output.container_digest,
        })
    }
}

pub struct Z3UnderConstrainedChecker {
    runner: Arc<dyn Z3ExecutionRunner>,
}

impl Z3UnderConstrainedChecker {
    pub fn new(runner: Arc<dyn Z3ExecutionRunner>) -> Self {
        Self { runner }
    }

    pub fn with_sandbox(sandbox: Arc<SandboxExecutor>) -> Self {
        Self::new(Arc::new(SandboxZ3Runner::new(sandbox)))
    }

    pub async fn check(&self, smt2: &str, budget: &BudgetConfig) -> Result<Z3CheckResult> {
        self.check_internal(smt2, budget, None, 0).await
    }

    pub async fn check_with_graph(
        &self,
        smt2: &str,
        budget: &BudgetConfig,
        graph: &CircomSignalGraph,
        seed: u64,
    ) -> Result<Z3CheckResult> {
        self.check_internal(smt2, budget, Some(graph), seed).await
    }

    async fn check_internal(
        &self,
        smt2: &str,
        budget: &BudgetConfig,
        graph: Option<&CircomSignalGraph>,
        seed: u64,
    ) -> Result<Z3CheckResult> {
        let artifact_path = persist_artifact_file("query.smt2", smt2)?;
        let output = self
            .runner
            .execute(&artifact_path, budget.z3_timeout_secs)
            .await?;
        let symbol_map = symbol_map_from_smt2(smt2);

        if output.exit_code != 0 {
            let fallback_result = if let Some(graph) = graph {
                self.random_witness_search(graph, 1024, seed).await
            } else {
                None
            };
            return Ok(Z3CheckResult::Unknown {
                reason: format!(
                    "z3 exited with code {}: {}",
                    output.exit_code,
                    output.stderr.trim()
                ),
                fallback_result,
                seed: graph.map(|_| seed),
                container_digest: output.container_digest,
            });
        }

        let status = output
            .stdout
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("unknown");

        match status {
            "sat" => {
                let (witness_a, witness_b) = extract_witnesses(&output.stdout, &symbol_map);
                Ok(Z3CheckResult::UnderConstrained {
                    witness_a,
                    witness_b,
                    smt2_file: artifact_path,
                    container_digest: output.container_digest,
                })
            }
            "unsat" => Ok(Z3CheckResult::Constrained {
                proof_file: artifact_path,
                container_digest: output.container_digest,
            }),
            _ => {
                let fallback_result = if let Some(graph) = graph {
                    self.random_witness_search(graph, 1024, seed).await
                } else {
                    None
                };
                Ok(Z3CheckResult::Unknown {
                    reason: format!(
                        "z3 returned status '{status}'{}",
                        if output.stderr.trim().is_empty() {
                            String::new()
                        } else {
                            format!(" ({})", output.stderr.trim())
                        }
                    ),
                    fallback_result,
                    seed: graph.map(|_| seed),
                    container_digest: output.container_digest,
                })
            }
        }
    }

    async fn random_witness_search(
        &self,
        graph: &CircomSignalGraph,
        iterations: u64,
        seed: u64,
    ) -> Option<CounterexamplePair> {
        let unconstrained = graph.find_trivially_unconstrained();
        let target = unconstrained.first()?;
        let target_id = graph.signals.iter().position(|signal| {
            signal.template == target.template
                && signal.name == target.name
                && signal.kind == target.kind
        })?;

        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let input_ids = graph
            .signals
            .iter()
            .enumerate()
            .filter_map(|(idx, signal)| (signal.kind == SignalKind::Input).then_some(idx))
            .collect::<Vec<_>>();

        for _ in 0..iterations {
            let mut witness_a_values = vec![0i128; graph.signals.len()];
            let mut witness_b_values = vec![0i128; graph.signals.len()];

            for idx in 0..graph.signals.len() {
                let value = rng.gen_range(0i128..100_000i128);
                witness_a_values[idx] = value;
                witness_b_values[idx] = value;
            }

            for input_id in &input_ids {
                let value = rng.gen_range(0i128..100_000i128);
                witness_a_values[*input_id] = value;
                witness_b_values[*input_id] = value;
            }

            propagate_simple_equalities(graph, &mut witness_a_values);
            propagate_simple_equalities(graph, &mut witness_b_values);

            witness_b_values[target_id] = witness_a_values[target_id].saturating_add(1);
            if witness_a_values[target_id] == witness_b_values[target_id] {
                continue;
            }
            if !constraints_hold(graph, &witness_a_values)
                || !constraints_hold(graph, &witness_b_values)
            {
                continue;
            }

            let witness_a = materialize_witness(graph, &witness_a_values);
            let witness_b = materialize_witness(graph, &witness_b_values);
            if witness_a.get(&format!("{}::{}", target.template, target.name))
                == witness_b.get(&format!("{}::{}", target.template, target.name))
            {
                continue;
            }

            if !witness_a.is_empty() && !witness_b.is_empty() {
                return Some(CounterexamplePair {
                    witness_a,
                    witness_b,
                });
            }
        }

        None
    }
}

fn materialize_witness(graph: &CircomSignalGraph, values: &[i128]) -> HashMap<String, BigUint> {
    let mut witness = HashMap::<String, BigUint>::new();
    for (idx, signal) in graph.signals.iter().enumerate() {
        let Some(value) = values.get(idx).copied() else {
            continue;
        };
        if value < 0 {
            continue;
        }
        let key = format!("{}::{}", signal.template, signal.name);
        witness.insert(key, BigUint::from(value as u128));
    }
    witness
}

fn propagate_simple_equalities(graph: &CircomSignalGraph, values: &mut [i128]) {
    for _ in 0..graph.constraints.len().max(1) {
        let mut changed = false;
        for constraint in &graph.constraints {
            let Constraint::Equality { lhs, rhs } = constraint else {
                continue;
            };
            if let Some(lhs_id) = single_signal_target(lhs) {
                if let Some(rhs_value) = evaluate_linear_combination(rhs, values) {
                    if values.get(lhs_id).copied() != Some(rhs_value) {
                        values[lhs_id] = rhs_value;
                        changed = true;
                    }
                }
            } else if let Some(rhs_id) = single_signal_target(rhs) {
                if let Some(lhs_value) = evaluate_linear_combination(lhs, values) {
                    if values.get(rhs_id).copied() != Some(lhs_value) {
                        values[rhs_id] = lhs_value;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
}

fn single_signal_target(combination: &LinearCombination) -> Option<usize> {
    if combination.signal_refs.len() != 1 {
        return None;
    }
    let reference = combination.signal_refs.first()?;
    let signal_id = reference.signal_id?;
    let trimmed = combination.expr.trim();
    if trimmed == reference.raw || trimmed == reference.signal_name {
        return Some(signal_id);
    }
    None
}

fn constraints_hold(graph: &CircomSignalGraph, values: &[i128]) -> bool {
    graph
        .constraints
        .iter()
        .all(|constraint| constraint_holds(constraint, values))
}

fn constraint_holds(constraint: &Constraint, values: &[i128]) -> bool {
    match constraint {
        Constraint::Equality { lhs, rhs } => {
            let Some(lhs_value) = evaluate_linear_combination(lhs, values) else {
                return false;
            };
            let Some(rhs_value) = evaluate_linear_combination(rhs, values) else {
                return false;
            };
            lhs_value == rhs_value
        }
        Constraint::R1CS { a, b, c } => {
            let Some(a_value) = evaluate_linear_combination(a, values) else {
                return false;
            };
            let Some(b_value) = evaluate_linear_combination(b, values) else {
                return false;
            };
            let Some(c_value) = evaluate_linear_combination(c, values) else {
                return false;
            };
            a_value
                .checked_mul(b_value)
                .map(|value| value == c_value)
                .unwrap_or(false)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EvalToken {
    Plus,
    Minus,
    Star,
    LeftParen,
    RightParen,
    Number(i128),
    Identifier(String),
}

fn tokenize_expression(expr: &str) -> Option<Vec<EvalToken>> {
    let mut tokens = Vec::new();
    let bytes = expr.as_bytes();
    let mut idx = 0usize;

    while idx < bytes.len() {
        match bytes[idx] as char {
            ch if ch.is_ascii_whitespace() => idx += 1,
            '+' => {
                tokens.push(EvalToken::Plus);
                idx += 1;
            }
            '-' => {
                tokens.push(EvalToken::Minus);
                idx += 1;
            }
            '*' => {
                tokens.push(EvalToken::Star);
                idx += 1;
            }
            '(' => {
                tokens.push(EvalToken::LeftParen);
                idx += 1;
            }
            ')' => {
                tokens.push(EvalToken::RightParen);
                idx += 1;
            }
            ch if ch.is_ascii_digit() => {
                let start = idx;
                idx += 1;
                while idx < bytes.len() && (bytes[idx] as char).is_ascii_digit() {
                    idx += 1;
                }
                let value = expr[start..idx].parse::<i128>().ok()?;
                tokens.push(EvalToken::Number(value));
            }
            ch if ch.is_ascii_alphabetic() || ch == '_' => {
                let start = idx;
                idx += 1;
                while idx < bytes.len() {
                    let ch = bytes[idx] as char;
                    if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '[' | ']') {
                        idx += 1;
                    } else {
                        break;
                    }
                }
                tokens.push(EvalToken::Identifier(expr[start..idx].to_string()));
            }
            _ => return None,
        }
    }

    Some(tokens)
}

fn evaluate_linear_combination(combination: &LinearCombination, values: &[i128]) -> Option<i128> {
    let tokens = tokenize_expression(&combination.expr)?;
    let mut parser = EvalParser::new(tokens, combination, values);
    let value = parser.parse_expression()?;
    if parser.is_at_end() {
        Some(value)
    } else {
        None
    }
}

struct EvalParser<'a> {
    tokens: Vec<EvalToken>,
    cursor: usize,
    combination: &'a LinearCombination,
    values: &'a [i128],
}

impl<'a> EvalParser<'a> {
    fn new(tokens: Vec<EvalToken>, combination: &'a LinearCombination, values: &'a [i128]) -> Self {
        Self {
            tokens,
            cursor: 0,
            combination,
            values,
        }
    }

    fn is_at_end(&self) -> bool {
        self.cursor >= self.tokens.len()
    }

    fn parse_expression(&mut self) -> Option<i128> {
        let mut value = self.parse_term()?;
        loop {
            if self.match_token(EvalToken::Plus) {
                let rhs = self.parse_term()?;
                value = value.checked_add(rhs)?;
            } else if self.match_token(EvalToken::Minus) {
                let rhs = self.parse_term()?;
                value = value.checked_sub(rhs)?;
            } else {
                break;
            }
        }
        Some(value)
    }

    fn parse_term(&mut self) -> Option<i128> {
        let mut value = self.parse_factor()?;
        loop {
            if self.match_token(EvalToken::Star) {
                let rhs = self.parse_factor()?;
                value = value.checked_mul(rhs)?;
            } else {
                break;
            }
        }
        Some(value)
    }

    fn parse_factor(&mut self) -> Option<i128> {
        if self.match_token(EvalToken::Minus) {
            return self.parse_factor()?.checked_neg();
        }

        match self.peek()?.clone() {
            EvalToken::Number(value) => {
                self.cursor += 1;
                Some(value)
            }
            EvalToken::Identifier(ident) => {
                self.cursor += 1;
                self.resolve_identifier(&ident)
            }
            EvalToken::LeftParen => {
                self.cursor += 1;
                let value = self.parse_expression()?;
                if !self.match_token(EvalToken::RightParen) {
                    return None;
                }
                Some(value)
            }
            _ => None,
        }
    }

    fn resolve_identifier(&self, ident: &str) -> Option<i128> {
        if let Some(signal_id) = self
            .combination
            .signal_refs
            .iter()
            .find_map(|reference| (reference.raw == ident).then_some(reference.signal_id))
            .flatten()
        {
            return self.values.get(signal_id).copied();
        }
        let base = ident
            .split('.')
            .next()
            .unwrap_or(ident)
            .split('[')
            .next()
            .unwrap_or(ident);
        let signal_id = self.combination.signal_refs.iter().find_map(|reference| {
            (reference.signal_name == base)
                .then_some(reference.signal_id)
                .flatten()
        })?;
        self.values.get(signal_id).copied()
    }

    fn match_token(&mut self, token: EvalToken) -> bool {
        if matches!(self.peek(), Some(next) if *next == token) {
            self.cursor += 1;
            return true;
        }
        false
    }

    fn peek(&self) -> Option<&EvalToken> {
        self.tokens.get(self.cursor)
    }
}

pub fn extract_witnesses(
    model: &str,
    symbol_map: &HashMap<String, String>,
) -> (HashMap<String, BigUint>, HashMap<String, BigUint>) {
    let pattern =
        Regex::new(r"\(define-fun\s+([^\s\)]+)\s*\(\)\s+Int\s+(-?\d+)\s*\)").expect("regex");
    let mut witness_a = HashMap::<String, BigUint>::new();
    let mut witness_b = HashMap::<String, BigUint>::new();

    for capture in pattern.captures_iter(model) {
        let symbol = capture
            .get(1)
            .map(|v| v.as_str().trim_matches('|'))
            .unwrap_or_default();
        let value = capture
            .get(2)
            .and_then(|v| v.as_str().parse::<i128>().ok())
            .unwrap_or(0);
        if value < 0 {
            continue;
        }
        let value = value as u128;

        if let Some(base) = symbol.strip_suffix("__a") {
            let logical = symbol_map
                .get(base)
                .cloned()
                .unwrap_or_else(|| infer_logical_name(base));
            witness_a.insert(logical, BigUint::from(value));
        } else if let Some(base) = symbol.strip_suffix("__b") {
            let logical = symbol_map
                .get(base)
                .cloned()
                .unwrap_or_else(|| infer_logical_name(base));
            witness_b.insert(logical, BigUint::from(value));
        }
    }

    (witness_a, witness_b)
}

fn symbol_map_from_smt2(smt2: &str) -> HashMap<String, String> {
    let mut map = HashMap::<String, String>::new();
    for line in smt2.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("; witness-symbol ") {
            let mut parts = rest.splitn(2, " -> ");
            if let (Some(symbol), Some(logical)) = (parts.next(), parts.next()) {
                map.insert(symbol.trim().to_string(), logical.trim().to_string());
            }
        }
    }
    map
}

fn infer_logical_name(base: &str) -> String {
    if let Some(raw) = base.strip_prefix("t_") {
        let parts = raw.split('_').collect::<Vec<_>>();
        if parts.len() >= 3 {
            let template = parts[0];
            let signal = parts[1];
            return format!("{template}::{signal}");
        }
    }
    base.to_string()
}

fn persist_artifact_file(name: &str, content: &str) -> Result<PathBuf> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time before UNIX_EPOCH")?;
    let dir = std::env::temp_dir()
        .join("audit-agent-z3")
        .join(now.as_nanos().to_string());
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create artifact dir {}", dir.display()))?;
    let path = dir.join(name);
    std::fs::write(&path, content)
        .with_context(|| format!("failed to write artifact file {}", path.display()))?;
    Ok(path)
}
