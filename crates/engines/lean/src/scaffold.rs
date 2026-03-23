use anyhow::Result;
use llm::sanitize::sanitize_prompt_input;
use llm::{LlmProvider, LlmRole, role_aware_llm_call};

const MAX_SNIPPET_CHARS: usize = 3_000;

const LEAN_STUB_PROMPT: &str = "You are a Lean 4 formalization assistant. \
Given a Rust function name and implementation, produce a Lean 4 theorem file \
that formalizes the key invariants of that function. \
Rules: start with `import Mathlib`, use `sorry` for all proof bodies, \
output ONLY valid Lean 4 source - no prose, no markdown fences.";

pub async fn generate_lean_stub(
    target_name: &str,
    rust_snippet: &str,
    llm: &dyn LlmProvider,
) -> Result<String> {
    let safe_name = sanitize_prompt_input(target_name);
    let safe_snippet = sanitize_prompt_input(
        &rust_snippet
            .chars()
            .take(MAX_SNIPPET_CHARS)
            .collect::<String>(),
    );
    let prompt = format!(
        "{LEAN_STUB_PROMPT}\n\nFunction name: {safe_name}\nRust source:\n{safe_snippet}\n\nLean 4 formalization:"
    );

    let (response, provenance) = role_aware_llm_call(llm, LlmRole::LeanScaffold, &prompt).await?;
    tracing::debug!(
        provider = %provenance.provider,
        model = ?provenance.model,
        role = %provenance.role,
        duration_ms = provenance.duration_ms,
        attempt = provenance.attempt,
        "captured lean-scaffold LLM provenance"
    );
    Ok(response)
}
