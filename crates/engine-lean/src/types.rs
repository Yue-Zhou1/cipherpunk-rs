use serde::{Deserialize, Serialize};

pub const DEFAULT_LEAN_ENV: &str = "lean-4.28.0";
pub const AXLE_BASE_URL: &str = "https://axle.axiommath.ai/api/v1";

#[derive(Debug, Clone, Serialize)]
pub struct AxleCheckRequest {
    pub content: String,
    pub environment: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AxleLeanMessages {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub infos: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AxleCheckResponse {
    pub okay: bool,
    pub content: String,
    pub lean_messages: AxleLeanMessages,
    pub tool_messages: AxleLeanMessages,
    pub failed_declarations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AxleDisproveRequest {
    pub content: String,
    pub environment: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AxleDisproveResponse {
    pub content: String,
    pub lean_messages: AxleLeanMessages,
    pub tool_messages: AxleLeanMessages,
    pub disproved_theorems: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AxleSorry2LemmaRequest {
    pub content: String,
    pub environment: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extract_sorries: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extract_errors: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AxleSorry2LemmaResponse {
    pub content: String,
    pub lean_messages: AxleLeanMessages,
    pub lemma_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeanWorkflowOutput {
    pub check_okay: bool,
    pub check_errors: Vec<String>,
    pub extracted_lemmas: Vec<String>,
    pub disproved_theorems: Vec<String>,
    pub lean_environment: String,
    pub authenticated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_request_omits_none_fields() {
        let req = AxleCheckRequest {
            content: "theorem foo : 1 = 1 := rfl".to_string(),
            environment: DEFAULT_LEAN_ENV.to_string(),
            timeout_seconds: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("content"));
        assert!(!json.contains("timeout_seconds"));
    }

    #[test]
    fn lean_workflow_output_roundtrips() {
        let out = LeanWorkflowOutput {
            check_okay: true,
            check_errors: vec![],
            extracted_lemmas: vec!["lemma_0".to_string()],
            disproved_theorems: vec![],
            lean_environment: DEFAULT_LEAN_ENV.to_string(),
            authenticated: false,
        };
        let json = serde_json::to_string(&out).unwrap();
        let back: LeanWorkflowOutput = serde_json::from_str(&json).unwrap();
        assert!(back.check_okay);
        assert_eq!(back.extracted_lemmas, vec!["lemma_0"]);
        assert!(!back.authenticated);
    }
}
