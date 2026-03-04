use std::path::PathBuf;

use num_bigint::BigUint;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::finding::{Framework, Severity};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AuditConfig {
    pub audit_id: String,
    pub source: ResolvedSource,
    pub scope: ResolvedScope,
    pub engines: EngineConfig,
    pub budget: BudgetConfig,
    pub optional_inputs: OptionalInputs,
    pub llm: LlmConfig,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResolvedSource {
    pub local_path: PathBuf,
    pub origin: SourceOrigin,
    pub commit_hash: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum SourceOrigin {
    Git {
        url: String,
        original_ref: Option<String>,
    },
    Local {
        original_path: PathBuf,
    },
    Archive {
        original_filename: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResolvedScope {
    pub target_crates: Vec<String>,
    pub excluded_crates: Vec<String>,
    pub build_matrix: Vec<BuildVariant>,
    pub detected_frameworks: Vec<Framework>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BuildVariant {
    pub features: Vec<String>,
    pub target_triple: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EngineConfig {
    pub crypto_zk: bool,
    pub distributed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BudgetConfig {
    pub kani_timeout_secs: u64,
    pub z3_timeout_secs: u64,
    pub fuzz_duration_secs: u64,
    pub madsim_ticks: u64,
    pub max_llm_retries: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OptionalInputs {
    pub spec_document: Option<ParsedSpecDocument>,
    pub previous_audit: Option<ParsedPreviousAudit>,
    pub custom_invariants: Vec<CustomInvariant>,
    pub known_entry_points: Vec<EntryPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CustomInvariant {
    pub id: String,
    pub name: String,
    pub description: String,
    pub check_expr: String,
    pub violation_severity: Severity,
    pub spec_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EntryPoint {
    pub crate_name: String,
    pub function: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LlmConfig {
    pub api_key_present: bool,
    pub provider: Option<String>,
    pub no_llm_prose: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OptionalInputsSummary {
    pub spec_provided: bool,
    pub prev_audit_provided: bool,
    pub invariants_count: usize,
    pub entry_points_count: usize,
    pub llm_prose_used: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ParsedSpecDocument {
    pub source_path: PathBuf,
    pub extracted_constraints: Vec<CandidateConstraint>,
    pub sections: Vec<SpecSection>,
    pub raw_text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CandidateConstraint {
    pub structured: StructuredConstraint,
    pub source_text: String,
    pub source_section: String,
    pub confidence: Confidence,
    pub extraction_method: ExtractionMethod,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum StructuredConstraint {
    Range {
        signal: String,
        #[schemars(with = "String")]
        lower: BigUint,
        #[schemars(with = "String")]
        upper: BigUint,
    },
    Uniqueness {
        field: String,
        scope: String,
    },
    Binding {
        field_a: String,
        field_b: String,
    },
    Custom {
        assertion_code: String,
        target: CustomAssertionTarget,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CustomAssertionTarget {
    Rust,
    Smt2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ExtractionMethod {
    PatternMatch,
    LlmNormalized,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SpecSection {
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ParsedPreviousAudit {
    pub source_path: PathBuf,
    pub prior_findings: Vec<PriorFinding>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PriorFinding {
    pub id: String,
    pub title: String,
    pub severity: Severity,
    pub description: String,
    pub status: PriorFindingStatus,
    pub location_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum PriorFindingStatus {
    Reported,
    Acknowledged,
    Fixed,
}
