use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use audit_agent_core::audit_config::{
    AuditConfig, BuildVariant, EngineConfig, LlmConfig, OptionalInputs, ResolvedScope,
    ResolvedSource, SourceOrigin,
};
use audit_agent_core::output::AuditManifest;
use intake::config::{ConfigParser, ValidatedConfig};
use intake::confirmation::{ConfirmationSummary, CrateDecision, IntakeWarning, UserDecisions};
use intake::detection::FrameworkDetector;
use intake::source::{SourceInput, SourceResolver};
use intake::workspace::WorkspaceAnalyzer;
use serde::{Deserialize, Serialize};
use zip::CompressionMethod;
use zip::write::FileOptions;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedSourceView {
    pub source: ResolvedSource,
    pub warnings: Vec<IntakeWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfigParseResponse {
    Validated {
        target_crates: Option<Vec<String>>,
        exclude_crates: Option<Vec<String>>,
        output_dir: PathBuf,
    },
    ConfigErrors {
        errors: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrateDecisionStyle {
    InScope,
    Excluded,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputType {
    ExecutivePdf,
    TechnicalPdf,
    EvidencePackZip,
    FindingsSarif,
    FindingsJson,
    RegressionTestsZip,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidencePreview {
    pub script: String,
    pub copyable: bool,
}

pub async fn resolve_source(input: SourceInput, work_dir: &Path) -> Result<ResolvedSourceView> {
    let resolved = SourceResolver::resolve(&input, work_dir).await?;
    Ok(ResolvedSourceView {
        source: resolved.source,
        warnings: resolved
            .warnings
            .into_iter()
            .map(IntakeWarning::from)
            .collect(),
    })
}

pub fn parse_config(path: &Path) -> ConfigParseResponse {
    match ConfigParser::parse(path) {
        Ok(validated) => ConfigParseResponse::Validated {
            target_crates: validated.scope.target_crates,
            exclude_crates: validated.scope.exclude_crates,
            output_dir: validated.output_dir,
        },
        Err(errors) => ConfigParseResponse::ConfigErrors {
            errors: errors.into_iter().map(|error| format!("{error}")).collect(),
        },
    }
}

pub fn detect_workspace(source: &ResolvedSource) -> Result<ConfirmationSummary> {
    let workspace = WorkspaceAnalyzer::analyze(&source.local_path)?;
    let detection = FrameworkDetector::detect(&workspace);
    let suggestions = WorkspaceAnalyzer::suggest_exclusions(&workspace)
        .into_iter()
        .map(|item| (item.crate_name, item.reason))
        .collect::<HashMap<_, _>>();

    Ok(ConfirmationSummary {
        crates: workspace
            .members
            .into_iter()
            .map(|meta| {
                if let Some(reason) = suggestions.get(&meta.name) {
                    CrateDecision::Excluded {
                        meta,
                        reason: reason.clone(),
                    }
                } else {
                    CrateDecision::InScope { meta }
                }
            })
            .collect(),
        frameworks: detection.frameworks,
        crypto_divergent_features: detection.crypto_divergent_features,
        build_matrix: vec![BuildVariant {
            features: vec!["default".to_string()],
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            label: "default".to_string(),
        }],
        estimated_duration_mins: 30,
        warnings: vec![],
    })
}

pub fn confirm_workspace(
    decisions: UserDecisions,
    source: ResolvedSource,
    validated: ValidatedConfig,
    summary: ConfirmationSummary,
    no_llm_prose: bool,
) -> Result<AuditConfig> {
    if !decisions.confirmed {
        anyhow::bail!("workspace confirmation declined by user");
    }

    let mut target_crates = Vec::<String>::new();
    let mut excluded_crates = Vec::<String>::new();
    for decision in summary.crates {
        match decision {
            CrateDecision::InScope { meta } => target_crates.push(meta.name),
            CrateDecision::Excluded { meta, .. } => excluded_crates.push(meta.name),
            CrateDecision::Ambiguous { meta, .. } => {
                let include = decisions
                    .ambiguous_crates
                    .get(&meta.name)
                    .copied()
                    .unwrap_or(false);
                if include {
                    target_crates.push(meta.name);
                } else {
                    excluded_crates.push(meta.name);
                }
            }
        }
    }

    let llm_missing = std::env::var("LLM_API_KEY").is_err();

    Ok(AuditConfig {
        audit_id: format!(
            "audit-{}-{}",
            chrono::Utc::now().format("%Y%m%d"),
            source.commit_hash.chars().take(8).collect::<String>()
        ),
        source,
        scope: ResolvedScope {
            target_crates,
            excluded_crates,
            build_matrix: build_matrix(validated.scope.features),
            detected_frameworks: summary
                .frameworks
                .iter()
                .map(|framework| framework.framework.clone())
                .collect(),
        },
        engines: EngineConfig {
            crypto_zk: validated.engines.crypto_zk.unwrap_or(true),
            distributed: validated.engines.distributed.unwrap_or(false),
        },
        budget: validated.budget,
        optional_inputs: OptionalInputs {
            spec_document: None,
            previous_audit: None,
            custom_invariants: vec![],
            known_entry_points: vec![],
        },
        llm: LlmConfig {
            api_key_present: !llm_missing,
            provider: std::env::var("LLM_PROVIDER").ok(),
            no_llm_prose,
        },
        output_dir: validated.output_dir,
    })
}

pub fn export_audit_yaml(config: &AuditConfig, path: &Path) -> Result<()> {
    let source_map = match &config.source.origin {
        SourceOrigin::Git {
            url,
            original_ref: _,
        } => serde_json::json!({
            "url": url,
            "commit": config.source.commit_hash
        }),
        SourceOrigin::Local { original_path } => serde_json::json!({
            "local_path": original_path
        }),
        SourceOrigin::Archive { original_filename } => serde_json::json!({
            "archive": original_filename
        }),
    };

    let yaml = serde_yaml::to_string(&serde_json::json!({
        "source": source_map,
        "scope": {
            "target_crates": config.scope.target_crates,
            "exclude_crates": config.scope.excluded_crates,
            "features": config.scope.build_matrix.iter().map(|variant| variant.features.clone()).collect::<Vec<_>>(),
        },
        "engines": {
            "crypto_zk": config.engines.crypto_zk,
            "distributed": config.engines.distributed
        },
        "budget": {
            "kani_timeout_secs": config.budget.kani_timeout_secs,
            "z3_timeout_secs": config.budget.z3_timeout_secs,
            "fuzz_duration_secs": config.budget.fuzz_duration_secs,
            "madsim_ticks": config.budget.madsim_ticks,
            "max_llm_retries": config.budget.max_llm_retries,
            "semantic_index_timeout_secs": config.budget.semantic_index_timeout_secs
        }
    }))?;

    fs::write(path, yaml).with_context(|| format!("write audit yaml {}", path.display()))?;
    Ok(())
}

pub fn branch_resolution_banner(warnings: &[IntakeWarning]) -> Option<String> {
    warnings.iter().find_map(|warning| match warning {
        IntakeWarning::BranchResolved { resolved_sha, .. } => {
            let short = resolved_sha.chars().take(6).collect::<String>();
            Some(format!(
                "Resolved to SHA {short} — audit is pinned to this commit"
            ))
        }
        _ => None,
    })
}

pub fn warning_message(warning: &IntakeWarning) -> String {
    match warning {
        IntakeWarning::BranchResolved {
            branch,
            resolved_sha,
        } => {
            format!("Branch {branch} resolved to {resolved_sha}. Audit will be pinned to this SHA.")
        }
        IntakeWarning::DirtyWorkingTree { uncommitted_files } => format!(
            "Working tree is dirty with {} uncommitted files.",
            uncommitted_files.len()
        ),
        IntakeWarning::LlmKeyMissing { degraded_features } => format!(
            "LLM key missing. Degraded features: {}",
            degraded_features.join(", ")
        ),
        IntakeWarning::LargeBuildMatrix {
            variants,
            estimated_hours,
        } => {
            format!("Build matrix has {variants} variants (estimated {estimated_hours:.1} hours).")
        }
        IntakeWarning::PreviousAuditParsed {
            prior_finding_count,
        } => format!("Parsed previous audit with {prior_finding_count} findings."),
    }
}

pub fn llm_missing_details(warnings: &[IntakeWarning]) -> Option<Vec<String>> {
    warnings.iter().find_map(|warning| match warning {
        IntakeWarning::LlmKeyMissing { degraded_features } => Some(degraded_features.clone()),
        _ => None,
    })
}

pub fn crate_decision_style(decision: &CrateDecision) -> CrateDecisionStyle {
    match decision {
        CrateDecision::InScope { .. } => CrateDecisionStyle::InScope,
        CrateDecision::Excluded { .. } => CrateDecisionStyle::Excluded,
        CrateDecision::Ambiguous { .. } => CrateDecisionStyle::Ambiguous,
    }
}

pub fn get_audit_manifest(output_dir: &Path) -> Result<AuditManifest> {
    let path = output_dir.join("audit-manifest.json");
    let content = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    Ok(serde_json::from_str(&content)?)
}

pub fn download_output(output_dir: &Path, output_type: OutputType, dest: &Path) -> Result<()> {
    let src = match output_type {
        OutputType::ExecutivePdf => output_dir.join("report-executive.pdf"),
        OutputType::TechnicalPdf => output_dir.join("report-technical.pdf"),
        OutputType::EvidencePackZip => output_dir.join("evidence-pack.zip"),
        OutputType::FindingsSarif => output_dir.join("findings.sarif"),
        OutputType::FindingsJson => output_dir.join("findings.json"),
        OutputType::RegressionTestsZip => {
            let existing_zip = output_dir.join("regression-tests.zip");
            if existing_zip.exists() {
                existing_zip
            } else {
                create_regression_zip(output_dir, &existing_zip)?;
                existing_zip
            }
        }
    };

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create destination parent {}", parent.display()))?;
    }
    fs::copy(&src, dest)
        .with_context(|| format!("copy {} to {}", src.display(), dest.display()))?;
    Ok(())
}

pub fn get_reproduce_preview(evidence_root: &Path, finding_id: &str) -> Result<EvidencePreview> {
    let path = evidence_root.join(finding_id).join("reproduce.sh");
    let script = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    Ok(EvidencePreview {
        script,
        copyable: true,
    })
}

fn create_regression_zip(output_dir: &Path, zip_path: &Path) -> Result<()> {
    let root = output_dir.join("regression-tests");
    let file =
        fs::File::create(zip_path).with_context(|| format!("create {}", zip_path.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = FileOptions::<()>::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    add_directory_to_zip(&mut zip, &root, &root, options)?;
    zip.finish().context("finalize regression-tests zip")?;
    Ok(())
}

fn add_directory_to_zip(
    zip: &mut zip::ZipWriter<fs::File>,
    root: &Path,
    current: &Path,
    options: FileOptions<()>,
) -> Result<()> {
    for entry in fs::read_dir(current).with_context(|| format!("read {}", current.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            add_directory_to_zip(zip, root, &path, options)?;
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .with_context(|| format!("strip prefix {}", root.display()))?;
        let relative = relative.to_string_lossy().replace('\\', "/");
        zip.start_file(relative, options)?;
        let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        zip.write_all(&bytes)?;
    }
    Ok(())
}

fn build_matrix(features: Option<Vec<Vec<String>>>) -> Vec<BuildVariant> {
    features
        .unwrap_or_else(|| vec![vec!["default".to_string()]])
        .into_iter()
        .map(|features| BuildVariant {
            label: if features.is_empty() {
                "default".to_string()
            } else {
                features.join(" + ")
            },
            features,
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
        })
        .collect()
}
