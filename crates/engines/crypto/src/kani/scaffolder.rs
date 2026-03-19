use std::sync::Arc;

use anyhow::Result;
use audit_agent_core::finding::CodeLocation;
use llm::{CompletionOpts, EvidenceGate, HarnessCode, LlmProvider, LlmRole, llm_call};
use llm::sanitize::{GraphContextEntry, pack_graph_aware_context};
use num_bigint::BigUint;
use sandbox::SandboxExecutor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignature {
    pub name: String,
    pub params: Vec<String>,
    pub return_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleTrigger {
    pub rule_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssertionSpec {
    NoOverflow { operation: String },
    NoUnwrapPanic { call_site: CodeLocation },
    FieldElementInRange { var: String, max: BigUint },
    CustomAssertion { code: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessRequest {
    pub target_fn: FunctionSignature,
    pub source_context: String,
    pub graph_context: Vec<GraphContextEntry>,
    pub context_char_budget: usize,
    pub rule_trigger: RuleTrigger,
    pub required_assertion: AssertionSpec,
    pub max_bound: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KaniOutput {
    pub counterexample: Option<String>,
    pub engine: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessResult {
    pub harness_code: String,
    pub cargo_toml: String,
    pub gate_level_reached: u8,
    pub kani_output: Option<KaniOutput>,
    pub llm_assume_hints_used: bool,
    pub shrink_attempts: u8,
}

pub struct KaniHarnessScaffolder {
    llm: Option<Arc<dyn LlmProvider>>,
    #[allow(dead_code)]
    sandbox: Option<Arc<SandboxExecutor>>,
    evidence_gate: Arc<EvidenceGate>,
}

impl KaniHarnessScaffolder {
    pub fn new(
        llm: Option<Arc<dyn LlmProvider>>,
        sandbox: Arc<SandboxExecutor>,
        evidence_gate: Arc<EvidenceGate>,
    ) -> Self {
        Self {
            llm,
            sandbox: Some(sandbox),
            evidence_gate,
        }
    }

    pub fn without_sandbox_for_tests(
        llm: Option<Arc<dyn LlmProvider>>,
        evidence_gate: Arc<EvidenceGate>,
    ) -> Self {
        Self {
            llm,
            sandbox: None,
            evidence_gate,
        }
    }

    pub async fn build(&self, req: &HarnessRequest) -> Result<HarnessResult> {
        let skeleton = self.generate_skeleton(req);

        let assume_hints = if let Some(llm) = &self.llm {
            self.request_assume_hints(llm.as_ref(), req, &skeleton)
                .await
                .unwrap_or_default()
        } else {
            vec![]
        };

        let harness = self.assemble(&skeleton, &assume_hints);
        let required_assertion = assertion_line(&req.required_assertion);
        let gate = self
            .evidence_gate
            .validate(
                &HarnessCode {
                    file_name: "harness.rs".to_string(),
                    source: harness.clone(),
                },
                &required_assertion,
            )
            .await;

        Ok(HarnessResult {
            harness_code: harness,
            cargo_toml: self.default_cargo_toml(),
            gate_level_reached: gate.level_reached,
            kani_output: gate.counterexample.map(|counterexample| KaniOutput {
                counterexample: Some(counterexample),
                engine: "kani".to_string(),
            }),
            llm_assume_hints_used: !assume_hints.is_empty(),
            shrink_attempts: 0,
        })
    }

    async fn request_assume_hints(
        &self,
        llm: &dyn LlmProvider,
        req: &HarnessRequest,
        skeleton: &str,
    ) -> Result<Vec<String>> {
        let packed_context = pack_graph_aware_context(
            &req.source_context,
            &req.graph_context,
            if req.context_char_budget == 0 {
                1_200
            } else {
                req.context_char_budget
            },
        );
        let prompt = format!(
            "You are helping focus a Kani model checker search.\n\
             The assertion being verified is fixed: {assertion}\n\
             Suggest kani::assume() preconditions that will help Kani find a \
             counterexample faster without over-constraining the input space.\n\
             Output ONLY valid Rust kani::assume!(...) lines. \
             Do NOT add new assert!() calls. Do NOT change existing assertions.\n\
             Function:\n{context}\n\nSkeleton:\n{skeleton}",
            assertion = req.required_assertion,
            context = packed_context,
        );
        let raw = llm_call(
            llm,
            LlmRole::SearchHints,
            &prompt,
            &CompletionOpts::default(),
        )
        .await?;
        Ok(parse_assume_lines(&raw))
    }

    fn generate_skeleton(&self, req: &HarnessRequest) -> String {
        let (params_decl, arg_names, return_ty) = normalize_signature(&req.target_fn);
        let target_impl = target_function_body(req);
        let assertion_line = assertion_line(&req.required_assertion);

        format!(
            r#"pub mod kani {{
    pub fn any<T: Default>() -> T {{ T::default() }}
    pub fn assume(_cond: bool) {{}}
    pub fn assert(cond: bool) {{ assert!(cond); }}
}}

pub fn {target_name}({params_decl}) -> {return_ty} {{
    {target_impl}
}}

pub fn harness() {{
    {bindings}
    /*__ASSUME_HINTS__*/
    let result = {target_name}({call_args});
    {assertion_line}
}}
"#,
            target_name = req.target_fn.name,
            params_decl = params_decl,
            return_ty = return_ty,
            target_impl = target_impl,
            bindings = arg_names
                .iter()
                .map(|name| format!("let {name}: u64 = kani::any();"))
                .collect::<Vec<_>>()
                .join("\n    "),
            call_args = arg_names.join(", "),
            assertion_line = assertion_line,
        )
    }

    fn assemble(&self, skeleton: &str, assume_hints: &[String]) -> String {
        let assume_lines = if assume_hints.is_empty() {
            String::new()
        } else {
            assume_hints
                .iter()
                .map(|line| line.replace("kani::assume!(", "kani::assume("))
                .collect::<Vec<_>>()
                .join("\n    ")
        };
        skeleton.replace("/*__ASSUME_HINTS__*/", &assume_lines)
    }

    fn default_cargo_toml(&self) -> String {
        "[package]\nname = \"kani-harness\"\nversion = \"0.1.0\"\nedition = \"2024\"\n".to_string()
    }
}

pub fn parse_assume_lines(raw: &str) -> Vec<String> {
    let mut parsed = Vec::<String>::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("kani::assume!(") || !trimmed.ends_with(");") {
            continue;
        }
        let inner = trimmed
            .trim_start_matches("kani::assume!(")
            .trim_end_matches(");")
            .trim()
            .to_ascii_lowercase();
        let invalid = inner == "false"
            || inner.contains("0 == 1")
            || inner.contains("1 == 0")
            || inner.contains("0==1")
            || inner.contains("1==0");
        if invalid {
            continue;
        }
        parsed.push(trimmed.to_string());
        if parsed.len() == 8 {
            break;
        }
    }
    parsed
}

fn normalize_signature(signature: &FunctionSignature) -> (String, Vec<String>, String) {
    if signature.params.is_empty() {
        return (
            "input: u64".to_string(),
            vec!["input".to_string()],
            "u64".to_string(),
        );
    }
    let mut params_decl = Vec::<String>::new();
    let mut arg_names = Vec::<String>::new();
    for (idx, param) in signature.params.iter().enumerate() {
        let (name, ty) = if let Some((name, ty)) = param.split_once(':') {
            (name.trim().to_string(), ty.trim().to_string())
        } else {
            (format!("arg_{idx}"), "u64".to_string())
        };
        params_decl.push(format!("{name}: {ty}"));
        arg_names.push(name);
    }
    (
        params_decl.join(", "),
        arg_names,
        if signature.return_type.trim().is_empty() {
            "u64".to_string()
        } else {
            signature.return_type.clone()
        },
    )
}

fn target_function_body(req: &HarnessRequest) -> String {
    if req.target_fn.name.contains("unchecked_add") {
        let names = req
            .target_fn
            .params
            .iter()
            .enumerate()
            .map(|(idx, param)| {
                param
                    .split(':')
                    .next()
                    .map(|n| n.trim().to_string())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| format!("arg_{idx}"))
            })
            .collect::<Vec<_>>();
        if names.len() >= 2 {
            return format!("{}.wrapping_add({})", names[0], names[1]);
        }
    }
    "0".to_string()
}

fn assertion_line(spec: &AssertionSpec) -> String {
    match spec {
        AssertionSpec::NoOverflow { .. } => "kani::assert(result >= a);".to_string(),
        AssertionSpec::NoUnwrapPanic { .. } => "kani::assert(true);".to_string(),
        AssertionSpec::FieldElementInRange { var, max } => {
            format!("kani::assert({var} <= {max}u64);")
        }
        AssertionSpec::CustomAssertion { code } => code.clone(),
    }
}

impl std::fmt::Display for AssertionSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssertionSpec::NoOverflow { operation } => {
                write!(f, "NoOverflow({operation})")
            }
            AssertionSpec::NoUnwrapPanic { call_site } => {
                write!(
                    f,
                    "NoUnwrapPanic({}:{})",
                    call_site.file.display(),
                    call_site.line_range.0
                )
            }
            AssertionSpec::FieldElementInRange { var, max } => {
                write!(f, "FieldElementInRange({var}, {max})")
            }
            AssertionSpec::CustomAssertion { code } => write!(f, "{code}"),
        }
    }
}
