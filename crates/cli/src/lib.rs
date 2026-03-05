use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use audit_agent_core::engine::{AuditContext, AuditEngine};
use audit_agent_core::finding::{
    Evidence, Finding, FindingId, FindingStatus, Framework, VerificationStatus,
};
use audit_agent_core::output::AuditOutputs;
use clap::{ArgGroup, Args, Parser, Subcommand};
use engine_crypto::intake_bridge::CryptoIntakeBridge;
use engine_crypto::rules::{CryptoMisuseRule, RuleEvaluator};
use engine_crypto::semantic::ra_client::SemanticIndex;
use engine_distributed::economic::EconomicAttackChecker;
use intake::OptionalInputsRaw;
use intake::diff::{AnalysisCache, DiffAnalysis, DiffModeAnalyzer};
use intake::source::{GitAuth, SourceInput};
use intake::{IntakeOrchestrator, workspace::WorkspaceAnalyzer};
use llm::{LlmProvider, provider_from_env};
use orchestrator::AuditOrchestrator;

#[derive(Debug, Parser)]
#[command(name = "audit-agent")]
#[command(about = "Security audit agent CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Analyze(AnalyzeArgs),
    Diff(DiffArgs),
}

#[derive(Debug, Clone, Args)]
#[command(group(
    ArgGroup::new("source")
        .required(true)
        .args(["git_url", "local_path", "archive"])
))]
pub struct AnalyzeArgs {
    #[arg(long)]
    pub audit_yaml: PathBuf,

    #[arg(long)]
    pub git_url: Option<String>,

    #[arg(long)]
    pub local_path: Option<PathBuf>,

    #[arg(long)]
    pub archive: Option<PathBuf>,

    #[arg(long)]
    pub commit: Option<String>,

    #[arg(long)]
    pub allow_branch_resolution: bool,

    #[arg(long)]
    pub git_token: Option<String>,

    #[arg(long, default_value = ".audit-work")]
    pub work_dir: PathBuf,

    #[arg(long)]
    pub spec: Option<PathBuf>,

    #[arg(long = "prev-audit")]
    pub prev_audit: Option<PathBuf>,

    #[arg(long)]
    pub invariants: Option<PathBuf>,

    #[arg(long)]
    pub entries: Option<PathBuf>,

    #[arg(long)]
    pub output_dir: Option<PathBuf>,

    #[arg(long)]
    pub evidence_pack_zip: Option<PathBuf>,

    #[arg(long, default_value = "rules")]
    pub rules_dir: PathBuf,

    #[arg(long)]
    pub no_llm_prose: bool,
}

#[derive(Debug, Clone, Args)]
pub struct DiffArgs {
    #[arg(long)]
    pub repo_root: PathBuf,

    #[arg(long)]
    pub base: String,

    #[arg(long)]
    pub head: String,

    #[arg(long)]
    pub cache_dir: Option<PathBuf>,
}

pub async fn run_cli(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Analyze(args) => {
            let outputs = run_analyze(args).await?;
            println!(
                "Audit completed: id={}, findings={}, output_dir={}",
                outputs.manifest.audit_id,
                outputs.findings.len(),
                outputs.dir.display()
            );
        }
        Command::Diff(args) => {
            let diff = run_diff(args)?;
            println!(
                "Diff computed: full_rerun={}, affected_crates={}, affected_files={}, cache_hit_rate={:.2}",
                diff.full_rerun_required,
                diff.affected_crates.len(),
                diff.affected_files.len(),
                diff.cache_hit_rate
            );
        }
    }
    Ok(())
}

pub async fn run_analyze(args: AnalyzeArgs) -> Result<AuditOutputs> {
    let source = parse_source_input(&args)?;
    let optional = OptionalInputsRaw {
        spec_path: args.spec.clone(),
        previous_audit_path: args.prev_audit.clone(),
        invariants_path: args.invariants.clone(),
        entry_points_path: args.entries.clone(),
        no_llm_prose: args.no_llm_prose,
    };

    let intake =
        IntakeOrchestrator::run(source, &args.audit_yaml, optional, &args.work_dir).await?;
    let mut config = intake.config;
    if let Some(output_dir) = args.output_dir.clone() {
        config.output_dir = output_dir;
    }

    let output_dir = config.output_dir.clone();
    let evidence_pack_zip = args
        .evidence_pack_zip
        .clone()
        .unwrap_or_else(|| output_dir.join("evidence-pack.zip"));
    ensure_placeholder_evidence_zip(&evidence_pack_zip)?;

    let rules_dir = args.rules_dir.clone();
    let mut engines: Vec<Box<dyn AuditEngine>> = Vec::new();
    if config.engines.crypto_zk {
        engines.push(Box::new(CryptoRuleEngine {
            rules_dir: rules_dir.join("crypto-misuse"),
        }));
    }
    if config.engines.distributed {
        engines.push(Box::new(DistributedEconomicEngine {
            rules_dir: rules_dir.join("economic"),
        }));
    }

    let llm = Arc::<dyn LlmProvider>::from(provider_from_env());
    let orchestrator = AuditOrchestrator::new(output_dir, evidence_pack_zip)
        .with_engines(engines)
        .with_llm(llm);

    orchestrator.run(&config).await
}

pub fn run_diff(args: DiffArgs) -> Result<DiffAnalysis> {
    let workspace = WorkspaceAnalyzer::analyze(&args.repo_root)?;
    let cache = if let Some(path) = args.cache_dir {
        Arc::new(AnalysisCache::open(&path)?)
    } else {
        Arc::new(AnalysisCache::default())
    };
    let analyzer = DiffModeAnalyzer::new(args.repo_root, workspace, cache);
    analyzer.compute_diff(&args.base, &args.head)
}

fn parse_source_input(args: &AnalyzeArgs) -> Result<SourceInput> {
    if let Some(url) = args.git_url.clone() {
        let commit = args
            .commit
            .clone()
            .ok_or_else(|| anyhow!("`--commit` is required with `--git-url`"))?;
        let auth = args
            .git_token
            .clone()
            .or_else(|| std::env::var("GIT_TOKEN").ok())
            .map(GitAuth::Token);
        return Ok(SourceInput::GitUrl {
            url,
            commit,
            auth,
            allow_branch_resolution: args.allow_branch_resolution,
        });
    }

    if let Some(path) = args.local_path.clone() {
        return Ok(SourceInput::LocalPath {
            path,
            commit: args.commit.clone(),
        });
    }

    if let Some(path) = args.archive.clone() {
        return Ok(SourceInput::Archive { path });
    }

    Err(anyhow!("one source option is required"))
}

fn ensure_placeholder_evidence_zip(path: &PathBuf) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create evidence dir {}", parent.display()))?;
    }
    std::fs::write(path, b"placeholder evidence pack")
        .with_context(|| format!("write placeholder evidence zip {}", path.display()))
}

#[derive(Debug, Clone)]
struct CryptoRuleEngine {
    rules_dir: PathBuf,
}

#[async_trait]
impl AuditEngine for CryptoRuleEngine {
    fn name(&self) -> &str {
        "crypto-rule-engine"
    }

    async fn analyze(&self, ctx: &AuditContext) -> Result<Vec<Finding>> {
        let engine_ctx = CryptoIntakeBridge::build_context(ctx.config.as_ref())?;
        let evaluator = RuleEvaluator::load_from_dir(&self.rules_dir)?;
        let rules = evaluator
            .rules()
            .iter()
            .map(|rule| (rule.id.clone(), rule.clone()))
            .collect::<HashMap<_, _>>();

        let default_framework = ctx
            .config
            .scope
            .detected_frameworks
            .first()
            .cloned()
            .unwrap_or(Framework::Static);

        let matches = evaluator.evaluate_workspace(&engine_ctx).await;
        let findings = matches
            .into_iter()
            .enumerate()
            .filter_map(|(idx, matched)| {
                let rule = rules.get(&matched.rule_id)?;
                Some(build_crypto_finding(
                    idx + 1,
                    rule,
                    &matched,
                    default_framework.clone(),
                ))
            })
            .collect();
        Ok(findings)
    }

    async fn supports(&self, ctx: &AuditContext) -> bool {
        ctx.config.engines.crypto_zk
    }
}

fn build_crypto_finding(
    ordinal: usize,
    rule: &CryptoMisuseRule,
    matched: &engine_crypto::rules::RuleMatch,
    framework: Framework,
) -> Finding {
    Finding {
        id: FindingId::new(format!(
            "F-{}-{ordinal:04}",
            rule.id.replace(' ', "-").to_uppercase()
        )),
        title: rule.title.clone(),
        severity: rule.severity.clone(),
        category: rule.category.clone(),
        framework,
        affected_components: vec![matched.location.clone()],
        prerequisites: "Vulnerable pattern is reachable by application control flow.".to_string(),
        exploit_path: format!(
            "Rule {} matched snippet: {}",
            rule.id, matched.matched_snippet
        ),
        impact: rule.description.clone(),
        evidence: Evidence {
            command: Some(format!("rule-match {}", rule.id)),
            seed: None,
            trace_file: None,
            counterexample: None,
            harness_path: None,
            smt2_file: None,
            container_digest: "n/a".to_string(),
            tool_versions: HashMap::from([
                ("crypto_rule_engine".to_string(), "tree-sitter".to_string()),
                ("rule_id".to_string(), rule.id.clone()),
            ]),
        },
        evidence_gate_level: 0,
        llm_generated: false,
        recommendation: rule.remediation.clone(),
        regression_test: None,
        status: FindingStatus::Open,
        regression_check: false,
        verification_status: VerificationStatus::Verified,
    }
}

#[derive(Debug, Clone)]
struct DistributedEconomicEngine {
    rules_dir: PathBuf,
}

#[async_trait]
impl AuditEngine for DistributedEconomicEngine {
    fn name(&self) -> &str {
        "distributed-economic-engine"
    }

    async fn analyze(&self, ctx: &AuditContext) -> Result<Vec<Finding>> {
        let semantic = SemanticIndex::build(ctx.workspace.as_ref(), &ctx.config.budget).await?;
        let checker = EconomicAttackChecker::load_from_dir(&self.rules_dir, None)?;
        Ok(checker.analyze(ctx.workspace.as_ref(), &semantic).await)
    }

    async fn supports(&self, ctx: &AuditContext) -> bool {
        ctx.config.engines.distributed
    }
}
