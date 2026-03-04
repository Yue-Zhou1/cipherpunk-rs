use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use audit_agent_core::audit_config::{
    CandidateConstraint, Confidence, CustomAssertionTarget, CustomInvariant, EntryPoint,
    ExtractionMethod, ParsedPreviousAudit, ParsedSpecDocument, PriorFinding, PriorFindingStatus,
    SpecSection, StructuredConstraint,
};
use audit_agent_core::finding::Severity;
use num_bigint::BigUint;
use regex::Regex;
use serde::Deserialize;

pub struct OptionalInputParser;

impl OptionalInputParser {
    pub async fn parse_spec(path: &Path) -> Result<ParsedSpecDocument> {
        let raw_text = if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
        {
            pdf_extract::extract_text(path)
                .with_context(|| format!("failed to parse pdf {}", path.display()))?
        } else {
            fs::read_to_string(path)
                .with_context(|| format!("failed to read {}", path.display()))?
        };

        let extracted_constraints = extract_constraints(&raw_text)?;
        let sections = extract_sections(&raw_text);

        Ok(ParsedSpecDocument {
            source_path: path.to_path_buf(),
            extracted_constraints,
            sections,
            raw_text,
        })
    }

    pub async fn parse_previous_audit(path: &Path) -> Result<ParsedPreviousAudit> {
        let text = if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
        {
            pdf_extract::extract_text(path)
                .with_context(|| format!("failed to parse pdf {}", path.display()))?
        } else {
            fs::read_to_string(path)
                .with_context(|| format!("failed to read {}", path.display()))?
        };

        let mut prior_findings = vec![];
        let title_re = Regex::new(r"(?m)^##\s+(?P<id>[A-Za-z0-9\-]+)\s*[-:]\s*(?P<title>.+)$")?;
        for captures in title_re.captures_iter(&text) {
            prior_findings.push(PriorFinding {
                id: captures["id"].to_string(),
                title: captures["title"].to_string(),
                severity: Severity::Observation,
                description: "Imported from prior audit document".to_string(),
                status: PriorFindingStatus::Reported,
                location_hint: None,
            });
        }

        if prior_findings.is_empty() && !text.trim().is_empty() {
            prior_findings.push(PriorFinding {
                id: "PRIOR-1".to_string(),
                title: "Imported finding".to_string(),
                severity: Severity::Observation,
                description: text.lines().next().unwrap_or_default().to_string(),
                status: PriorFindingStatus::Reported,
                location_hint: None,
            });
        }

        Ok(ParsedPreviousAudit {
            source_path: path.to_path_buf(),
            prior_findings,
        })
    }

    pub fn parse_invariants(path: &Path) -> Result<Vec<CustomInvariant>> {
        #[derive(Deserialize)]
        struct InvariantsDoc {
            invariants: Vec<InvariantRow>,
        }

        #[derive(Deserialize)]
        struct InvariantRow {
            id: String,
            name: String,
            description: String,
            check_expr: String,
            violation_severity: Severity,
            spec_ref: Option<String>,
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let parsed: InvariantsDoc =
            serde_yaml::from_str(&content).context("invalid invariants yaml")?;

        Ok(parsed
            .invariants
            .into_iter()
            .map(|row| CustomInvariant {
                id: row.id,
                name: row.name,
                description: row.description,
                check_expr: row.check_expr,
                violation_severity: row.violation_severity,
                spec_ref: row.spec_ref,
            })
            .collect())
    }

    pub fn parse_entry_points(path: &Path) -> Result<Vec<EntryPoint>> {
        #[derive(Deserialize)]
        struct EntryDoc {
            entry_points: Vec<EntryRow>,
        }

        #[derive(Deserialize)]
        struct EntryRow {
            crate_name: Option<String>,
            crate_field_alias: Option<String>,
            function: String,
            #[serde(rename = "crate")]
            crate_legacy: Option<String>,
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;

        let parsed: EntryDoc =
            serde_yaml::from_str(&content).context("invalid entry-points yaml")?;
        Ok(parsed
            .entry_points
            .into_iter()
            .map(|row| EntryPoint {
                crate_name: row
                    .crate_name
                    .or(row.crate_field_alias)
                    .or(row.crate_legacy)
                    .unwrap_or_default(),
                function: row.function,
            })
            .collect())
    }
}

fn extract_constraints(text: &str) -> Result<Vec<CandidateConstraint>> {
    let range = Regex::new(r"(?i)\b([a-zA-Z_][a-zA-Z0-9_]*)\s+in\s*\[(\d+)\s*,\s*(\d+)\)")?;
    let unique = Regex::new(
        r"(?i)\b([a-zA-Z_][a-zA-Z0-9_]*)\s+must\s+be\s+unique(?:\s+per\s+([a-zA-Z0-9_]+))?",
    )?;
    let binding = Regex::new(
        r"(?i)\b([a-zA-Z_][a-zA-Z0-9_]*)\s+must\s+(?:equal|be\s+equal\s+to)\s+([a-zA-Z_][a-zA-Z0-9_]*)",
    )?;
    let rust_assert = Regex::new(r"assert\(([^)]+)\)")?;

    let mut constraints = vec![];

    for cap in range.captures_iter(text) {
        let signal = cap[1].to_string();
        let lower =
            BigUint::parse_bytes(cap[2].as_bytes(), 10).unwrap_or_else(|| BigUint::from(0u8));
        let upper =
            BigUint::parse_bytes(cap[3].as_bytes(), 10).unwrap_or_else(|| BigUint::from(0u8));
        constraints.push(CandidateConstraint {
            structured: StructuredConstraint::Range {
                signal,
                lower,
                upper,
            },
            source_text: cap
                .get(0)
                .map(|m| m.as_str())
                .unwrap_or_default()
                .to_string(),
            source_section: "General".to_string(),
            confidence: Confidence::Medium,
            extraction_method: ExtractionMethod::PatternMatch,
        });
    }

    for cap in unique.captures_iter(text) {
        constraints.push(CandidateConstraint {
            structured: StructuredConstraint::Uniqueness {
                field: cap[1].to_string(),
                scope: cap
                    .get(2)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_else(|| "global".to_string()),
            },
            source_text: cap
                .get(0)
                .map(|m| m.as_str())
                .unwrap_or_default()
                .to_string(),
            source_section: "General".to_string(),
            confidence: Confidence::Medium,
            extraction_method: ExtractionMethod::PatternMatch,
        });
    }

    for cap in binding.captures_iter(text) {
        constraints.push(CandidateConstraint {
            structured: StructuredConstraint::Binding {
                field_a: cap[1].to_string(),
                field_b: cap[2].to_string(),
            },
            source_text: cap
                .get(0)
                .map(|m| m.as_str())
                .unwrap_or_default()
                .to_string(),
            source_section: "General".to_string(),
            confidence: Confidence::Medium,
            extraction_method: ExtractionMethod::PatternMatch,
        });
    }

    for cap in rust_assert.captures_iter(text) {
        constraints.push(CandidateConstraint {
            structured: StructuredConstraint::Custom {
                assertion_code: cap[1].trim().to_string(),
                target: CustomAssertionTarget::Rust,
            },
            source_text: cap
                .get(0)
                .map(|m| m.as_str())
                .unwrap_or_default()
                .to_string(),
            source_section: "General".to_string(),
            confidence: Confidence::Low,
            extraction_method: ExtractionMethod::PatternMatch,
        });
    }

    Ok(constraints)
}

fn extract_sections(text: &str) -> Vec<SpecSection> {
    let mut sections = vec![];
    let mut current_title = "Document".to_string();
    let mut current_content = String::new();

    for line in text.lines() {
        if let Some(stripped) = line.strip_prefix('#') {
            if !current_content.trim().is_empty() {
                sections.push(SpecSection {
                    title: current_title.clone(),
                    content: current_content.trim().to_string(),
                });
                current_content.clear();
            }
            current_title = stripped.trim().to_string();
        } else {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    if !current_content.trim().is_empty() {
        sections.push(SpecSection {
            title: current_title,
            content: current_content.trim().to_string(),
        });
    }

    sections
}
