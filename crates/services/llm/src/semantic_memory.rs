pub const MAX_SIGNATURES_IN_PROMPT: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticSignatureContext {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub invariant: String,
    pub remediation: String,
    pub tags: Vec<String>,
}

pub fn format_semantic_signatures(signatures: &[SemanticSignatureContext]) -> String {
    if signatures.is_empty() {
        return "none".to_string();
    }

    signatures
        .iter()
        .take(MAX_SIGNATURES_IN_PROMPT)
        .map(|signature| {
            format!(
                "- {id}: {title}\n  summary: {summary}\n  invariant: {invariant}\n  remediation: {remediation}\n  tags: {tags}",
                id = signature.id,
                title = signature.title,
                summary = signature.summary,
                invariant = signature.invariant,
                remediation = signature.remediation,
                tags = if signature.tags.is_empty() {
                    "none".to_string()
                } else {
                    signature.tags.join(", ")
                },
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
