use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use audit_agent_core::audit_config::ParsedPreviousAudit;
use audit_agent_core::finding::{Finding, VerificationStatus};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FindingKey {
    rule_id: String,
    file: PathBuf,
    line_start: u32,
}

pub fn deduplicate_findings(findings: &[Finding]) -> Vec<Finding> {
    let mut out = Vec::<Finding>::new();
    let mut key_to_index = HashMap::<FindingKey, usize>::new();

    for finding in findings.iter().cloned() {
        let key = dedup_key_for_finding(&finding);
        if let Some(existing_idx) = key_to_index.get(&key).copied() {
            if prefer_over(&finding, &out[existing_idx]) {
                out[existing_idx] = finding;
            }
            continue;
        }

        let idx = out.len();
        out.push(finding);
        key_to_index.insert(key, idx);
    }

    out
}

pub fn mark_regression_checks(findings: &mut [Finding], previous_audit: &ParsedPreviousAudit) {
    let mut keyed = HashSet::<FindingKey>::new();
    let mut keyed_ids = HashSet::<String>::new();
    let mut id_only = HashSet::<String>::new();

    for prior in &previous_audit.prior_findings {
        if let Some((file, line_start)) = parse_location_hint(prior.location_hint.as_deref()) {
            let key = FindingKey {
                rule_id: prior.id.clone(),
                file,
                line_start,
            };
            keyed_ids.insert(prior.id.clone());
            keyed.insert(key);
        } else {
            id_only.insert(prior.id.clone());
        }
    }

    for finding in findings {
        let id = finding.id.to_string();
        let key = dedup_key_for_finding(finding);
        finding.regression_check =
            keyed.contains(&key) || (!keyed_ids.contains(&id) && id_only.contains(&id));
    }
}

fn dedup_key_for_finding(finding: &Finding) -> FindingKey {
    let (file, line_start) = finding
        .affected_components
        .first()
        .map(|location| (location.file.clone(), location.line_range.0))
        .unwrap_or_else(|| (PathBuf::from("unknown"), 0));

    FindingKey {
        rule_id: finding.id.to_string(),
        file,
        line_start,
    }
}

fn prefer_over(candidate: &Finding, existing: &Finding) -> bool {
    if verification_rank(candidate) != verification_rank(existing) {
        return verification_rank(candidate) > verification_rank(existing);
    }
    if analysis_rank(candidate) != analysis_rank(existing) {
        return analysis_rank(candidate) > analysis_rank(existing);
    }
    if candidate.evidence_gate_level != existing.evidence_gate_level {
        return candidate.evidence_gate_level > existing.evidence_gate_level;
    }

    false
}

fn verification_rank(finding: &Finding) -> u8 {
    match finding.verification_status {
        VerificationStatus::Verified => 2,
        VerificationStatus::Unverified { .. } => 1,
    }
}

fn analysis_rank(finding: &Finding) -> u8 {
    match finding
        .evidence
        .tool_versions
        .get("analysis_origin")
        .map(String::as_str)
    {
        Some("cache") => 0,
        _ => 1,
    }
}

fn parse_location_hint(hint: Option<&str>) -> Option<(PathBuf, u32)> {
    let hint = hint?.trim();
    let (file, remainder) = hint.rsplit_once(':')?;
    let line_token = remainder
        .split(['-', ' ', ')'])
        .next()
        .unwrap_or_default()
        .trim();
    let line_start = line_token.parse::<u32>().ok()?;
    Some((PathBuf::from(file), line_start))
}
