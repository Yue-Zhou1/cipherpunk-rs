use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
use num_bigint::BigUint;
use regex::Regex;

static SIGNAL_TOKEN_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[A-Za-z_][A-Za-z0-9_]*(?:\[[^\]]+\])?(?:\.[A-Za-z_][A-Za-z0-9_]*(?:\[[^\]]+\])?)*")
        .expect("signal token regex compiles")
});

pub type ConstraintId = usize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CircomSignalGraph {
    pub signals: Vec<Signal>,
    pub constraints: Vec<Constraint>,
    pub templates: HashMap<String, Template>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub name: String,
    pub signal_ids: Vec<usize>,
    pub constraint_ids: Vec<ConstraintId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signal {
    pub name: String,
    pub kind: SignalKind,
    pub template: String,
    pub constrained_by: Vec<ConstraintId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalKind {
    Input,
    Output,
    Intermediate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Constraint {
    R1CS {
        a: LinearCombination,
        b: LinearCombination,
        c: LinearCombination,
    },
    Equality {
        lhs: LinearCombination,
        rhs: LinearCombination,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinearCombination {
    pub expr: String,
    pub signal_refs: Vec<SignalRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalRef {
    pub raw: String,
    pub signal_name: String,
    pub signal_id: Option<usize>,
}

impl CircomSignalGraph {
    pub fn from_file(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read Circom file {}", path.display()))?;
        let content = strip_block_comments(&raw);

        let mut graph = Self {
            signals: vec![],
            constraints: vec![],
            templates: HashMap::new(),
        };
        let mut signal_lookup = HashMap::<(String, String), Vec<usize>>::new();
        let mut current_template: Option<String> = None;
        let mut template_start_depth = 0i32;
        let mut brace_depth = 0i32;

        for raw_line in content.lines() {
            let line = strip_line_comment(raw_line).trim();
            if line.is_empty() {
                update_brace_depth(line, &mut brace_depth);
                if brace_depth < 0 {
                    bail!("unbalanced braces in Circom source");
                }
                if current_template.is_some() && brace_depth <= template_start_depth {
                    current_template = None;
                }
                continue;
            }

            if let Some(template_name) = parse_template_start(line) {
                current_template = Some(template_name.clone());
                template_start_depth = brace_depth;
                graph
                    .templates
                    .entry(template_name.clone())
                    .or_insert(Template {
                        name: template_name,
                        signal_ids: vec![],
                        constraint_ids: vec![],
                    });
            }

            if let Some(template_name) = current_template.as_ref() {
                if let Some((kind, name)) = parse_signal_decl(line) {
                    let signal_id = graph.signals.len();
                    graph.signals.push(Signal {
                        name: name.clone(),
                        kind,
                        template: template_name.clone(),
                        constrained_by: vec![],
                    });

                    signal_lookup
                        .entry((template_name.clone(), name))
                        .or_default()
                        .push(signal_id);
                    if let Some(template) = graph.templates.get_mut(template_name) {
                        template.signal_ids.push(signal_id);
                    }
                } else if let Some(constraint) =
                    parse_constraint(line, template_name, &signal_lookup)
                {
                    let constraint_id = graph.constraints.len();
                    let touched_signals = constraint_signal_ids(&constraint);
                    graph.constraints.push(constraint);
                    if let Some(template) = graph.templates.get_mut(template_name) {
                        template.constraint_ids.push(constraint_id);
                    }
                    for signal_id in touched_signals {
                        if let Some(signal) = graph.signals.get_mut(signal_id) {
                            signal.constrained_by.push(constraint_id);
                        }
                    }
                }
            }

            update_brace_depth(line, &mut brace_depth);
            if brace_depth < 0 {
                bail!("unbalanced braces in Circom source");
            }
            if current_template.is_some() && brace_depth <= template_start_depth {
                current_template = None;
            }
        }

        if brace_depth != 0 {
            bail!("unbalanced braces in Circom source");
        }

        Ok(graph)
    }

    pub fn find_trivially_unconstrained(&self) -> Vec<Signal> {
        self.signals
            .iter()
            .filter(|signal| signal.kind == SignalKind::Output && signal.constrained_by.is_empty())
            .cloned()
            .collect()
    }

    pub fn to_smt2(&self, target_signal: &str, field_prime: &BigUint) -> String {
        let prime = field_prime.to_string();
        let mut lines = vec![
            "(set-logic QF_NIA)".to_string(),
            "; auto-generated by CircomSignalGraph::to_smt2".to_string(),
            "; two witness model: __a and __b".to_string(),
        ];

        let mut signal_symbols = HashMap::<usize, String>::new();
        for (idx, signal) in self.signals.iter().enumerate() {
            let base = format!(
                "t_{}_{}_{}",
                sanitize_ident(&signal.template),
                sanitize_ident(&signal.name),
                idx
            );
            signal_symbols.insert(idx, base.clone());
            lines.push(format!(
                "; witness-symbol {base} -> {}::{}",
                signal.template, signal.name
            ));
            lines.push(format!("(declare-const {base}__a Int)"));
            lines.push(format!("(declare-const {base}__b Int)"));
            lines.push(format!(
                "(assert (and (>= {base}__a 0) (< {base}__a {prime})))"
            ));
            lines.push(format!(
                "(assert (and (>= {base}__b 0) (< {base}__b {prime})))"
            ));
        }

        for constraint in &self.constraints {
            match constraint {
                Constraint::Equality { lhs, rhs } => {
                    let lhs_a = linear_combination_to_smt(lhs, "a", &signal_symbols);
                    let rhs_a = linear_combination_to_smt(rhs, "a", &signal_symbols);
                    let lhs_b = linear_combination_to_smt(lhs, "b", &signal_symbols);
                    let rhs_b = linear_combination_to_smt(rhs, "b", &signal_symbols);
                    lines.push(format!(
                        "(assert (= (mod {lhs_a} {prime}) (mod {rhs_a} {prime})))"
                    ));
                    lines.push(format!(
                        "(assert (= (mod {lhs_b} {prime}) (mod {rhs_b} {prime})))"
                    ));
                }
                Constraint::R1CS { a, b, c } => {
                    let a_a = linear_combination_to_smt(a, "a", &signal_symbols);
                    let b_a = linear_combination_to_smt(b, "a", &signal_symbols);
                    let c_a = linear_combination_to_smt(c, "a", &signal_symbols);
                    let a_b = linear_combination_to_smt(a, "b", &signal_symbols);
                    let b_b = linear_combination_to_smt(b, "b", &signal_symbols);
                    let c_b = linear_combination_to_smt(c, "b", &signal_symbols);
                    lines.push(format!(
                        "(assert (= (mod (* {a_a} {b_a}) {prime}) (mod {c_a} {prime})))"
                    ));
                    lines.push(format!(
                        "(assert (= (mod (* {a_b} {b_b}) {prime}) (mod {c_b} {prime})))"
                    ));
                }
            }
        }

        for (idx, signal) in self.signals.iter().enumerate() {
            if signal.kind == SignalKind::Input {
                let base = signal_symbols
                    .get(&idx)
                    .cloned()
                    .unwrap_or_else(|| format!("sig_{idx}"));
                lines.push(format!(
                    "(assert (= {base}__a {base}__b)) ; same input across both witnesses"
                ));
            }
        }

        let targets: Vec<usize> = self
            .signals
            .iter()
            .enumerate()
            .filter(|(_, signal)| signal.kind == SignalKind::Output && signal.name == target_signal)
            .map(|(idx, _)| idx)
            .collect();

        if !targets.is_empty() {
            if targets.len() == 1 {
                let base = signal_symbols
                    .get(&targets[0])
                    .cloned()
                    .unwrap_or_else(|| "target_0".to_string());
                lines.push("; target outputs differ".to_string());
                lines.push(format!("(assert (not (= {base}__a {base}__b)))"));
            } else {
                let terms = targets
                    .iter()
                    .map(|idx| {
                        let base = signal_symbols
                            .get(idx)
                            .cloned()
                            .unwrap_or_else(|| format!("target_{idx}"));
                        format!("(not (= {base}__a {base}__b))")
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                lines.push("; target outputs differ".to_string());
                lines.push(format!("(assert (or {terms}))"));
            }
        } else {
            lines.push(format!(
                "; target signal '{target_signal}' was not found as an output signal"
            ));
        }

        lines.push("(check-sat)".to_string());
        lines.push("(get-model)".to_string());
        lines.join("\n")
    }
}

fn parse_template_start(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("template ") {
        return None;
    }
    let after = trimmed.trim_start_matches("template ").trim_start();
    let end = after.find('(')?;
    Some(after[..end].trim().to_string())
}

fn parse_signal_decl(line: &str) -> Option<(SignalKind, String)> {
    let trimmed = line.trim();
    if !trimmed.starts_with("signal ") {
        return None;
    }
    let normalized = trimmed.trim_end_matches(';');
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 2 || tokens[0] != "signal" {
        return None;
    }

    let (kind, token) = match tokens[1] {
        "input" => (SignalKind::Input, *tokens.get(2)?),
        "output" => (SignalKind::Output, *tokens.get(2)?),
        _ => (SignalKind::Intermediate, *tokens.get(1)?),
    };
    let name = base_signal_name(token);
    Some((kind, name))
}

fn parse_constraint(
    line: &str,
    template: &str,
    signal_lookup: &HashMap<(String, String), Vec<usize>>,
) -> Option<Constraint> {
    let normalized = line.trim().trim_end_matches(';').trim();

    if let Some((lhs, rhs)) = split_once_symbol(normalized, "<==") {
        return Some(Constraint::Equality {
            lhs: parse_linear_combination(lhs, template, signal_lookup),
            rhs: parse_linear_combination(rhs, template, signal_lookup),
        });
    }
    if let Some((lhs, rhs)) = split_once_symbol(normalized, "==>") {
        return Some(Constraint::Equality {
            lhs: parse_linear_combination(rhs, template, signal_lookup),
            rhs: parse_linear_combination(lhs, template, signal_lookup),
        });
    }
    if let Some((lhs, rhs)) = split_once_symbol(normalized, "===") {
        if let Some((a, b)) = split_top_level_multiply(lhs) {
            return Some(Constraint::R1CS {
                a: parse_linear_combination(&a, template, signal_lookup),
                b: parse_linear_combination(&b, template, signal_lookup),
                c: parse_linear_combination(rhs, template, signal_lookup),
            });
        }
        return Some(Constraint::Equality {
            lhs: parse_linear_combination(lhs, template, signal_lookup),
            rhs: parse_linear_combination(rhs, template, signal_lookup),
        });
    }

    None
}

fn parse_linear_combination(
    expr: &str,
    template: &str,
    signal_lookup: &HashMap<(String, String), Vec<usize>>,
) -> LinearCombination {
    let mut signal_refs = Vec::new();
    let mut seen = HashSet::<String>::new();

    for capture in SIGNAL_TOKEN_REGEX.find_iter(expr) {
        let raw = capture.as_str().to_string();
        if !seen.insert(raw.clone()) {
            continue;
        }
        let signal_name = base_signal_name(&raw);
        let signal_id = signal_lookup
            .get(&(template.to_string(), signal_name.clone()))
            .and_then(|ids| ids.first().copied());
        signal_refs.push(SignalRef {
            raw,
            signal_name,
            signal_id,
        });
    }

    LinearCombination {
        expr: expr.trim().to_string(),
        signal_refs,
    }
}

fn split_once_symbol<'a>(text: &'a str, symbol: &str) -> Option<(&'a str, &'a str)> {
    let idx = text.find(symbol)?;
    let left = text[..idx].trim();
    let right = text[idx + symbol.len()..].trim();
    Some((left, right))
}

fn split_top_level_multiply(expr: &str) -> Option<(String, String)> {
    let mut depth = 0i32;
    for (idx, ch) in expr.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            '*' if depth == 0 => {
                let left = expr[..idx].trim();
                let right = expr[idx + 1..].trim();
                if !left.is_empty() && !right.is_empty() {
                    return Some((left.to_string(), right.to_string()));
                }
            }
            _ => {}
        }
    }
    None
}

fn constraint_signal_ids(constraint: &Constraint) -> Vec<usize> {
    let mut ids = vec![];
    match constraint {
        Constraint::Equality { lhs, rhs } => {
            ids.extend(
                lhs.signal_refs
                    .iter()
                    .filter_map(|reference| reference.signal_id),
            );
            ids.extend(
                rhs.signal_refs
                    .iter()
                    .filter_map(|reference| reference.signal_id),
            );
        }
        Constraint::R1CS { a, b, c } => {
            ids.extend(
                a.signal_refs
                    .iter()
                    .filter_map(|reference| reference.signal_id),
            );
            ids.extend(
                b.signal_refs
                    .iter()
                    .filter_map(|reference| reference.signal_id),
            );
            ids.extend(
                c.signal_refs
                    .iter()
                    .filter_map(|reference| reference.signal_id),
            );
        }
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn linear_combination_to_smt(
    combination: &LinearCombination,
    witness_suffix: &str,
    signal_symbols: &HashMap<usize, String>,
) -> String {
    if let Some(ast) = parse_expression_ast(combination) {
        return expression_to_smt(&ast, witness_suffix, signal_symbols);
    }

    // Fallback keeps SMT emission resilient for Circom expressions this parser does not yet support.
    let mut terms = vec![];
    for reference in &combination.signal_refs {
        if let Some(signal_id) = reference.signal_id {
            let base = signal_symbols
                .get(&signal_id)
                .cloned()
                .unwrap_or_else(|| format!("sig_{signal_id}"));
            terms.push(format!("{base}__{witness_suffix}"));
        }
    }

    terms.sort();
    terms.dedup();

    if terms.is_empty() {
        return parse_int_literal_or_zero(&combination.expr);
    }
    if terms.len() == 1 {
        return terms[0].clone();
    }
    format!("(+ {})", terms.join(" "))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExpressionNode {
    Const(i128),
    Signal(usize),
    Neg(Box<ExpressionNode>),
    Add(Box<ExpressionNode>, Box<ExpressionNode>),
    Sub(Box<ExpressionNode>, Box<ExpressionNode>),
    Mul(Box<ExpressionNode>, Box<ExpressionNode>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExpressionToken {
    Plus,
    Minus,
    Star,
    LeftParen,
    RightParen,
    Number(i128),
    Identifier(String),
}

fn parse_expression_ast(combination: &LinearCombination) -> Option<ExpressionNode> {
    let tokens = tokenize_expression(&combination.expr)?;
    let mut parser = ExpressionParser::new(tokens, combination);
    let ast = parser.parse_expression()?;
    if parser.is_at_end() { Some(ast) } else { None }
}

fn expression_to_smt(
    node: &ExpressionNode,
    witness_suffix: &str,
    signal_symbols: &HashMap<usize, String>,
) -> String {
    match node {
        ExpressionNode::Const(value) => int_to_smt(*value),
        ExpressionNode::Signal(signal_id) => {
            let base = signal_symbols
                .get(signal_id)
                .cloned()
                .unwrap_or_else(|| format!("sig_{signal_id}"));
            format!("{base}__{witness_suffix}")
        }
        ExpressionNode::Neg(inner) => format!(
            "(- {})",
            expression_to_smt(inner, witness_suffix, signal_symbols)
        ),
        ExpressionNode::Add(lhs, rhs) => format!(
            "(+ {} {})",
            expression_to_smt(lhs, witness_suffix, signal_symbols),
            expression_to_smt(rhs, witness_suffix, signal_symbols)
        ),
        ExpressionNode::Sub(lhs, rhs) => format!(
            "(- {} {})",
            expression_to_smt(lhs, witness_suffix, signal_symbols),
            expression_to_smt(rhs, witness_suffix, signal_symbols)
        ),
        ExpressionNode::Mul(lhs, rhs) => format!(
            "(* {} {})",
            expression_to_smt(lhs, witness_suffix, signal_symbols),
            expression_to_smt(rhs, witness_suffix, signal_symbols)
        ),
    }
}

fn tokenize_expression(expr: &str) -> Option<Vec<ExpressionToken>> {
    let mut tokens = Vec::new();
    let chars = expr.as_bytes();
    let mut idx = 0usize;

    while idx < chars.len() {
        match chars[idx] as char {
            ch if ch.is_ascii_whitespace() => idx += 1,
            '+' => {
                tokens.push(ExpressionToken::Plus);
                idx += 1;
            }
            '-' => {
                tokens.push(ExpressionToken::Minus);
                idx += 1;
            }
            '*' => {
                tokens.push(ExpressionToken::Star);
                idx += 1;
            }
            '(' => {
                tokens.push(ExpressionToken::LeftParen);
                idx += 1;
            }
            ')' => {
                tokens.push(ExpressionToken::RightParen);
                idx += 1;
            }
            ch if ch.is_ascii_digit() => {
                let start = idx;
                idx += 1;
                while idx < chars.len() && (chars[idx] as char).is_ascii_digit() {
                    idx += 1;
                }
                let value = expr[start..idx].parse::<i128>().ok()?;
                tokens.push(ExpressionToken::Number(value));
            }
            ch if ch.is_ascii_alphabetic() || ch == '_' => {
                let start = idx;
                idx += 1;
                while idx < chars.len() {
                    let ch = chars[idx] as char;
                    if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '[' | ']') {
                        idx += 1;
                    } else {
                        break;
                    }
                }
                tokens.push(ExpressionToken::Identifier(expr[start..idx].to_string()));
            }
            _ => return None,
        }
    }

    Some(tokens)
}

struct ExpressionParser<'a> {
    tokens: Vec<ExpressionToken>,
    cursor: usize,
    combination: &'a LinearCombination,
}

impl<'a> ExpressionParser<'a> {
    fn new(tokens: Vec<ExpressionToken>, combination: &'a LinearCombination) -> Self {
        Self {
            tokens,
            cursor: 0,
            combination,
        }
    }

    fn is_at_end(&self) -> bool {
        self.cursor >= self.tokens.len()
    }

    fn parse_expression(&mut self) -> Option<ExpressionNode> {
        let mut node = self.parse_term()?;
        loop {
            if self.match_token(ExpressionToken::Plus) {
                let rhs = self.parse_term()?;
                node = ExpressionNode::Add(Box::new(node), Box::new(rhs));
            } else if self.match_token(ExpressionToken::Minus) {
                let rhs = self.parse_term()?;
                node = ExpressionNode::Sub(Box::new(node), Box::new(rhs));
            } else {
                break;
            }
        }
        Some(node)
    }

    fn parse_term(&mut self) -> Option<ExpressionNode> {
        let mut node = self.parse_factor()?;
        loop {
            if self.match_token(ExpressionToken::Star) {
                let rhs = self.parse_factor()?;
                node = ExpressionNode::Mul(Box::new(node), Box::new(rhs));
            } else {
                break;
            }
        }
        Some(node)
    }

    fn parse_factor(&mut self) -> Option<ExpressionNode> {
        if self.match_token(ExpressionToken::Minus) {
            let inner = self.parse_factor()?;
            return Some(ExpressionNode::Neg(Box::new(inner)));
        }

        match self.peek()?.clone() {
            ExpressionToken::Number(value) => {
                self.cursor += 1;
                Some(ExpressionNode::Const(value))
            }
            ExpressionToken::Identifier(ident) => {
                self.cursor += 1;
                Some(
                    resolve_signal_id(self.combination, &ident)
                        .map(ExpressionNode::Signal)
                        .unwrap_or(ExpressionNode::Const(0)),
                )
            }
            ExpressionToken::LeftParen => {
                self.cursor += 1;
                let inner = self.parse_expression()?;
                if !self.match_token(ExpressionToken::RightParen) {
                    return None;
                }
                Some(inner)
            }
            _ => None,
        }
    }

    fn match_token(&mut self, token: ExpressionToken) -> bool {
        if matches!(self.peek(), Some(next) if *next == token) {
            self.cursor += 1;
            return true;
        }
        false
    }

    fn peek(&self) -> Option<&ExpressionToken> {
        self.tokens.get(self.cursor)
    }
}

fn resolve_signal_id(combination: &LinearCombination, ident: &str) -> Option<usize> {
    if let Some(id) = combination
        .signal_refs
        .iter()
        .find_map(|reference| (reference.raw == ident).then_some(reference.signal_id))
        .flatten()
    {
        return Some(id);
    }
    let base = base_signal_name(ident);
    combination.signal_refs.iter().find_map(|reference| {
        (reference.signal_name == base)
            .then_some(reference.signal_id)
            .flatten()
    })
}

fn parse_int_literal_or_zero(expr: &str) -> String {
    expr.trim()
        .parse::<i128>()
        .map(int_to_smt)
        .unwrap_or_else(|_| "0".to_string())
}

fn int_to_smt(value: i128) -> String {
    if value < 0 {
        format!("(- {})", value.unsigned_abs())
    } else {
        value.to_string()
    }
}

fn base_signal_name(token: &str) -> String {
    token
        .split('.')
        .next()
        .unwrap_or(token)
        .split('[')
        .next()
        .unwrap_or(token)
        .to_string()
}

fn sanitize_ident(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out
}

fn strip_line_comment(line: &str) -> &str {
    line.split("//").next().unwrap_or(line)
}

fn strip_block_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_comment = false;

    while let Some(ch) = chars.next() {
        if in_comment {
            if ch == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_comment = false;
            }
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            in_comment = true;
            continue;
        }
        output.push(ch);
    }

    output
}

fn update_brace_depth(line: &str, brace_depth: &mut i32) {
    *brace_depth += line.matches('{').count() as i32;
    *brace_depth -= line.matches('}').count() as i32;
}
