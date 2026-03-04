# Security Audit Agent — Implementation Plan v2.1

> **Based on:** System Design v2.0  
> **Updated:** Full I/O contract integrated; `crates/intake` added; LLM role contract precisely defined  
> **Target readers:** Engineers starting implementation  
> **Total estimated duration:** 18–22 weeks across 5 phases (weeks 1–17 active implementation; weeks 18–22 integration, QA, and buffer)
> **Stack:** Rust (orchestrator, engines, CLI), TypeScript/React (Tauri UI)

---

## How to Use This Document

Each phase maps to a design section. Each task specifies:
- **Scope** — what to build
- **File/module layout** — where it lives in the repo
- **Key interfaces** — types and traits to define first
- **Acceptance criteria** — checkable conditions that confirm the task is done
- **Blocking dependencies** — what must exist before starting

Work within a phase can be parallelized across engineers.  
Do **not** start Phase N+1 until all `[ ]` acceptance criteria in Phase N are checked off.

---

## LLM Role Contract (Read Before Writing Any Code)

Every engineer must internalize this before touching any module. Violations here corrupt the entire audit pipeline.

### The Core Rule

**LLM influences what gets checked and how efficiently. LLM never determines what constitutes a finding.**

The boundary is exactly: **before a tool runs vs. after a tool runs.**

```
LLM influence ALLOWED (before tool runs):      LLM influence FORBIDDEN (after tool runs):
──────────────────────────────────────────     ──────────────────────────────────────────
Prioritize which functions to analyze          Decide if a counterexample is a real bug
Focus Kani search space via kani::assume()     Assign or adjust finding severity
Normalize spec prose → structured constraint   Interpret Z3 output semantics
Fill harness scaffolding (variable names,      Conclude a constraint is violated
  type coercions, use statements)              Score or rank economic risks
Generate MadSim entry point call              Determine if a trace is a safety violation
Improve prose readability of report text       Create a finding from LLM reasoning alone
```

The tool output is ground truth. LLM shapes the inputs to those tools — it never interprets their outputs.

### Three Permitted LLM Roles

**Role 1 — Mechanical Scaffolding**
Boilerplate generation, syntax fixing, type coercions, `use` statements. No domain judgment. Always allowed.

**Role 2 — Search Space Guidance** *(the valuable one, do not eliminate)*
LLM reads function context and rule trigger to suggest focused `kani::assume()` constraints, prioritize which CDG nodes to check first, or select the most relevant chaos scenario to run first. This shapes *efficiency*, not *correctness*. The Kani counterexample still has to be real. The MadSim invariant violation still has to happen. Nothing in Role 2 produces a finding by itself.

**Role 3 — Prose Rendering**
Given structured `Finding` fields, LLM improves readability of recommendation text and impact descriptions. LLM edits human-written content — it does not generate security conclusions.

### The Two-Tier Finding Label

Every finding carries a `verification_status` field that appears in all reports and the UI:

```
Verified     — finding backed by tool output: Kani counterexample, Z3 sat, MadSim trace,
               rule pattern match with code location. Deterministic. Reproducible.
               Auditor acts on these directly.

Unverified   — finding backed by LLM-assisted analysis (spec extraction, economic attack
               checklist description). No formal proof attached.
               Auditor must manually confirm before acting.
```

This distinction — not eliminating LLM from reasoning, but being transparent when it is involved — ensures the tool is trustworthy even when LLM judgment is used.

### What This Means Per Module (Quick Reference)

| Module | LLM role | Who decides the finding |
|--------|----------|------------------------|
| Crypto Rule Engine | none | tree-sitter + semantic pattern |
| CDG Builder | none | rust-analyzer graph analysis |
| Z3 Checker | none | Z3 SAT solver |
| Kani Harness Scaffolder | Role 2: `kani::assume()` hints; Role 1: scaffolding | Kani model checker |
| Spec Extractor | Role 1: normalize prose → structured JSON | Z3/Kani on the structured form |
| MadSim Harness Builder | Role 1: entry point call only | MadSim + invariant assertions |
| Economic Attack Checker | Role 3: description text only | Presence/absence of deterministic code pattern from YAML checklist |
| Report Renderer | Role 3: prose polish of recommendation field | Structured `Finding` fields |
| Evidence Gate (fix loop) | Role 1: syntax/type corrections only | Whether it compiles + Kani finds counterexample |

---

## I/O Contract (Read This First)

Before writing any code, every engineer must understand what the system accepts and produces. All module boundaries are designed around this contract.

### Inputs

#### Tier 1 — Required (audit cannot start without these)

**1. Source** — one of three forms:

```
Git URL    →  https://github.com/org/repo  +  commit SHA (mandatory, not a branch name)
Local path →  /absolute/path/to/repo       +  commit SHA (auto-read from git HEAD, user confirms)
Archive    →  .tar.gz or .zip upload        (no commit SHA; agent derives a content-hash instead)
```

**2. `audit.yaml`** — the single configuration file that drives the entire audit:

```yaml
# audit.yaml — complete example with all fields
source:
  url: https://github.com/org/zk-prover    # OR: local_path: /path/to/repo
  commit: a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4  # full 40-char SHA, required

scope:
  target_crates:           # explicit allowlist — omit to include all crates
    - zk-prover-core
    - zk-prover-verifier
  exclude_crates:          # always exclude test/bench/fuzz crates
    - zk-prover-bench
    - zk-prover-fuzz
  features:                # omit → agent auto-detects crypto-divergent variants
    - ["default"]
    - ["asm", "parallel"]

engines:
  crypto_zk: true          # Rust crypto + ZK circuit analysis
  distributed: false       # MadSim / consensus analysis (opt-in)

budget:
  kani_timeout_secs: 300   # per harness
  z3_timeout_secs: 600     # per sub-circuit
  fuzz_duration_secs: 3600
  madsim_ticks: 100000
  max_llm_retries: 3
```

#### Tier 2 — Optional (significantly improve output quality when provided)

| Input file | CLI flag | Effect |
|---|---|---|
| Protocol spec / whitepaper | `--spec spec.pdf` | Spec Generator extracts candidate constraints from document; improves constraint coverage vs. comment-guessing |
| Previous audit report | `--prev-audit report.pdf` | Agent skips known findings; retests previously reported issues; marks them `regression_check: true` |
| Custom invariants | `--invariants invariants.yaml` | Distributed engine adds domain-specific assertions beyond generic safety/liveness |
| Known entry points | `--entries entries.yaml` | Code Locator starts from explicit functions; useful when naming conventions are non-standard |
| LLM API key | `LLM_API_KEY` env var | If absent, all LLM features degrade to template-based fallbacks; audit still runs |

**Custom invariants format:**
```yaml
# invariants.yaml
invariants:
  - id: INV-001
    name: "Batch finalization deadline"
    description: "Sequencer must submit a batch within 12 hours of previous batch"
    check_expr: "ticks_since_last_batch <= 43200"
    violation_severity: High
    spec_ref: "docs/whitepaper.pdf §4.2"

  - id: INV-002
    name: "Escape hatch reachability"
    description: "After sequencer stops, forced withdrawal must be reachable"
    check_expr: "forced_withdrawal_reachable_within_ticks(720)"
    violation_severity: Critical
    spec_ref: "EIP-4844 §3.1"
```

**Known entry points format:**
```yaml
# entries.yaml
entry_points:
  - crate: zk-prover-core
    function: "prover::Prover::prove"
  - crate: zk-prover-verifier
    function: "verifier::verify_proof"
```

#### Tier 3 — UI-Guided (post-detection confirmation)

After intake completes, the agent displays a workspace summary and waits for user confirmation before running expensive analysis. The user can correct misdetections at this point:

```
Detected workspace: 12 crates
  ✓ zk-prover-core        [in scope — contains Chip::configure, Halo2 detected]
  ✓ zk-prover-verifier    [in scope — contains verify_proof entry point]
  ─ zk-prover-bench       [excluded — benchmark crate pattern detected]
  ─ zk-prover-fuzz        [excluded — fuzz harness pattern detected]
  ? zk-prover-utils       [ambiguous — include?]   ← user must choose

Detected frameworks:
  ✓ Halo2    (Chip::configure found in zk-prover-core)
  ✓ SP1      (sp1_zkvm::entrypoint! found in zk-prover-guest)
  ✗ Circom   (not detected)

Detected feature flags with crypto-path divergence:
  ✓ feature="asm"       → enters assembly path in field arithmetic
  ✓ feature="parallel"  → changes RNG initialization strategy

Build matrix: 4 variants
  [1] default
  [2] asm
  [3] parallel
  [4] asm + parallel

Estimated analysis time: ~2.5 hours
[Confirm and start] [Adjust scope] [Export audit.yaml]
```

---

### Outputs

Every output file lands in a user-specified `--output-dir` (default: `./audit-output/`).

#### Output 1 — Executive Summary
**Files:** `report-executive.md`, `report-executive.pdf`  
**Audience:** Project lead, client management, non-technical stakeholders  
**Contents:**
- Audit metadata: repo URL, commit hash, date, scope, tools used, agent version
- Risk score: 0–100 with color band (≥70 green / 50–69 yellow / <50 red)
- Finding count by severity: `Critical(N) High(N) Medium(N) Low(N) Obs(N)`
- Top 5 findings: one paragraph each, no code snippets
- Overall recommendation: one of `[Deploy / Fix before deploy / Do not deploy]`

#### Output 2 — Technical Audit Report
**Files:** `report-technical.md`, `report-technical.pdf`  
**Audience:** Developers, security engineers, internal audit review  
**Contents:** Full finding detail — per finding:
- ID, title, severity, category, framework, affected file:line
- Attack scenario: prerequisites → exploit path → impact
- Code snippet showing the vulnerable pattern
- Proof of concept: exact command to reproduce
- Recommendation with suggested code diff
- Regression test to add

#### Output 3 — Evidence Pack
**File:** `evidence-pack.zip`  
**Audience:** External reviewers, researchers, re-audit clients  
**Layout:**
```
evidence-pack.zip
└── {finding_id}/
    ├── manifest.json       ← all metadata + tool versions + container digest
    ├── reproduce.sh        ← one-command reproduction via Docker
    ├── harness/
    │   ├── src/lib.rs      ← Kani proof harness
    │   └── Cargo.toml
    ├── smt2/
    │   ├── query.smt2      ← Z3 input
    │   └── output.txt      ← solver output
    ├── traces/
    │   ├── trace.json      ← MadSim event log
    │   ├── seed.txt        ← deterministic seed
    │   └── replay.sh       ← MadSim replay script
    └── corpus/             ← fuzz corpus files
```
**Key property:** anyone with Docker can run `bash reproduce.sh` and get identical results 6+ months later.

#### Output 4 — SARIF File
**File:** `findings.sarif`  
**Audience:** CI/CD pipelines, GitHub Security tab, security dashboards  
**Use cases:**
- Upload to GitHub → appears in Security → Code scanning alerts
- Block PR merge if Critical findings present (GitHub Actions step)
- Feed into Defect Dojo, SonarQube, or similar

#### Output 5 — Regression Test Suite
**Directory:** `regression-tests/`  
**Audience:** Developers maintaining the codebase going forward  
```
regression-tests/
├── crypto_misuse_tests.rs    ← proptest/property tests for each crypto finding
├── kani_harnesses/           ← Kani proof harnesses to add to the target repo
└── madsim_scenarios/         ← chaos scenarios to add as permanent CI tests
```
**This is a first-class deliverable**, not an afterthought. Handing developers runnable tests prevents regression and justifies the tool's cost beyond the initial report.

#### Output 6 — Machine-Readable Findings
**File:** `findings.json`  
**Audience:** Programmatic consumers, custom integrations, future diff-mode baseline  
**Format:** JSON array of `Finding` objects per schema in `docs/finding-schema.json`

#### Output directory layout (complete)
```
audit-output/                       ← --output-dir
├── report-executive.md
├── report-executive.pdf
├── report-technical.md
├── report-technical.pdf
├── findings.json
├── findings.sarif
├── evidence-pack.zip
├── regression-tests/
│   ├── crypto_misuse_tests.rs
│   ├── kani_harnesses/
│   └── madsim_scenarios/
└── audit-manifest.json             ← machine-readable summary of this run
                                       (inputs used, versions, timing, finding counts)
```

**`audit-manifest.json` schema:**
```json
{
  "audit_id": "audit-20250115-a1b2c3d4",
  "agent_version": "0.3.0",
  "source": {
    "url": "https://github.com/org/repo",
    "commit": "a1b2c3d4...",
    "content_hash": null
  },
  "started_at": "2025-01-15T09:00:00Z",
  "completed_at": "2025-01-15T11:32:00Z",
  "scope": { "target_crates": [...], "features": [...] },
  "tool_versions": { "kani": "0.57.0", "z3": "4.13.0", ... },
  "container_digests": { "kani": "sha256:abc...", "z3": "sha256:def..." },
  "finding_counts": { "critical": 2, "high": 5, "medium": 8, "low": 12, "observation": 4 },
  "risk_score": 42,
  "engines_run": ["crypto_zk"],
  "optional_inputs": { "spec_provided": true, "prev_audit_provided": false }
}
```

---

## Repository Structure (Updated)

```
audit-agent/
├── Cargo.toml                         # workspace root
├── crates/
│   ├── core/                          # shared types, traits, I/O schema
│   ├── intake/                        # ★ NEW: all user input handling
│   │   ├── src/
│   │   │   ├── source.rs              # Git clone / local path / archive unpack
│   │   │   ├── config.rs              # audit.yaml parsing + validation
│   │   │   ├── workspace.rs           # Cargo workspace analysis
│   │   │   ├── detection.rs           # framework auto-detection
│   │   │   ├── confirmation.rs        # workspace summary + user confirmation
│   │   │   └── optional_inputs.rs     # spec PDF / prev report / invariants parsing
│   ├── orchestrator/                  # DAG engine, scheduler, cache, diff-mode
│   ├── sandbox/                       # Docker executor abstraction
│   ├── evidence/                      # Evidence Store, pack builder, manifest
│   ├── findings/                      # Findings DB, dedup, SARIF + JSON export
│   ├── llm/                           # LLM provider adapters + Evidence Gate
│   ├── engine-crypto/                 # Crypto & ZK audit engine
│   ├── engine-distributed/            # Distributed consensus audit engine
│   └── report/                        # Three-layer report + regression test generator
├── ui/                                # Tauri + React frontend
│   ├── src-tauri/                     # Tauri backend (IPC bridge)
│   └── src/                           # React components
├── containers/
│   ├── versions.toml                  # single source of truth for all tool versions
│   ├── kani/Dockerfile
│   ├── z3/Dockerfile
│   ├── miri/Dockerfile
│   ├── madsim/Dockerfile
│   └── fuzz/Dockerfile
├── rules/
│   ├── crypto-misuse/                 # YAML rule definitions (CRYPTO-001 through CRYPTO-008)
│   └── distributed/                   # built-in invariant definitions
├── tests/
│   ├── fixtures/                      # intentionally vulnerable test targets
│   └── integration/                   # end-to-end pipeline tests
└── docs/
    ├── design-v2.md
    ├── finding-schema.json            # canonical JSON schema for Finding type
    ├── audit-yaml-schema.json         # JSON schema for audit.yaml validation
    └── adr/                           # Architecture Decision Records
```

---

## Phase 0 — Foundation (Week 1–2)

**Goal:** Establish all shared types, the full I/O schema, container infrastructure, and CI. Zero business logic. Everything else builds on this phase — it is a strict blocker for all subsequent work.

---

### Task 0.1 — Core Types, I/O Schema & Finding Model

**Crate:** `crates/core`

Define every shared data type. Getting these right now prevents refactors later. The `Finding` type and `AuditConfig` type are the backbone of the entire system.

```rust
// crates/core/src/finding.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub id: FindingId,                    // "F-ZK-0042"
    pub title: String,
    pub severity: Severity,
    pub category: FindingCategory,
    pub framework: Framework,
    pub affected_components: Vec<CodeLocation>,
    pub prerequisites: String,
    pub exploit_path: String,
    pub impact: String,
    pub evidence: Evidence,
    pub evidence_gate_level: u8,          // 0–3; see Evidence Gate protocol
    pub llm_generated: bool,
    pub recommendation: String,
    pub regression_test: Option<String>,  // Rust code to add to repo
    pub status: FindingStatus,
    pub regression_check: bool,            // true if re-testing a prior audit finding
    pub verification_status: VerificationStatus,
}

/// Appears in all reports and UI. Set by the engine that produced the finding —
/// never by LLM. See LLM Role Contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationStatus {
    /// Backed by tool output: Kani counterexample, Z3 sat, MadSim trace,
    /// or rule pattern match with code location. Reproducible with reproduce.sh.
    Verified,
    /// Backed by LLM-assisted analysis (spec extraction, economic attack checklist).
    /// No formal proof. Auditor must manually confirm before acting.
    Unverified { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub command: Option<String>,
    pub seed: Option<String>,
    pub trace_file: Option<PathBuf>,
    pub counterexample: Option<String>,
    pub harness_path: Option<PathBuf>,
    pub smt2_file: Option<PathBuf>,
    pub container_digest: String,
    pub tool_versions: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeLocation {
    pub crate_name: String,
    pub module: String,
    pub file: PathBuf,
    pub line_range: (u32, u32),
    pub snippet: Option<String>,          // up to 10 lines of context
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Severity { Critical, High, Medium, Low, Observation }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FindingCategory {
    UnderConstrained, SpecMismatch, CryptoMisuse,
    Replay, DoS, Race, Incentive, UnsafeUB, SideChannel, SupplyChain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Framework { Halo2, Circom, SP1, RISC0, MadSim, Loom, Static }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FindingStatus { Open, Acknowledged, Fixed, Regressed, WontFix }
```

```rust
// crates/core/src/audit_config.rs
// This is the in-memory representation of audit.yaml after parsing + validation

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    pub audit_id: String,                  // generated: "audit-{date}-{commit_short}"
    pub source: ResolvedSource,
    pub scope: ResolvedScope,
    pub engines: EngineConfig,
    pub budget: BudgetConfig,
    pub optional_inputs: OptionalInputs,
    pub llm: LlmConfig,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedSource {
    pub local_path: PathBuf,               // always a local path after intake resolves it
    pub origin: SourceOrigin,              // Git(url) | Local | Archive
    pub commit_hash: String,               // 40-char SHA or content-hash for archives
    pub content_hash: String,              // sha256 of entire source tree (always set)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceOrigin {
    Git { url: String, original_ref: Option<String> },
    Local { original_path: PathBuf },
    Archive { original_filename: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedScope {
    pub target_crates: Vec<String>,        // confirmed by user in Tier-3 step
    pub excluded_crates: Vec<String>,
    pub build_matrix: Vec<BuildVariant>,   // feature combinations × target triples
    pub detected_frameworks: Vec<Framework>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildVariant {
    pub features: Vec<String>,
    pub target_triple: String,
    pub label: String,                     // human display: "asm + parallel"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub crypto_zk: bool,
    pub distributed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub kani_timeout_secs: u64,            // default: 300
    pub z3_timeout_secs: u64,             // default: 600
    pub fuzz_duration_secs: u64,          // default: 3600
    pub madsim_ticks: u64,                // default: 100_000
    pub max_llm_retries: u8,              // default: 3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionalInputs {
    pub spec_document: Option<ParsedSpecDocument>,
    pub previous_audit: Option<ParsedPreviousAudit>,
    pub custom_invariants: Vec<CustomInvariant>,
    pub known_entry_points: Vec<EntryPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomInvariant {
    pub id: String,
    pub name: String,
    pub description: String,
    pub check_expr: String,
    pub violation_severity: Severity,
    pub spec_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryPoint {
    pub crate_name: String,
    pub function: String,                  // fully qualified: "module::fn_name"
}
```

```rust
// crates/core/src/engine.rs
#[async_trait]
pub trait AuditEngine: Send + Sync {
    fn name(&self) -> &str;
    async fn analyze(&self, ctx: &AuditContext) -> Result<Vec<Finding>>;
    async fn supports(&self, ctx: &AuditContext) -> bool;
}

// Passed to every engine — contains everything resolved by intake
pub struct AuditContext {
    pub config: Arc<AuditConfig>,
    pub workspace: Arc<CargoWorkspace>,
    pub sandbox: Arc<SandboxExecutor>,
    pub evidence_store: Arc<EvidenceStore>,
    pub llm: Option<Arc<dyn LlmProvider>>,
}
```

```rust
// crates/core/src/output.rs
// Typed representation of the output directory

pub struct AuditOutputs {
    pub dir: PathBuf,
    pub manifest: AuditManifest,
    pub findings: Vec<Finding>,
}

pub struct AuditManifest {
    pub audit_id: String,
    pub agent_version: String,
    pub source: ResolvedSource,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub scope: ResolvedScope,
    pub tool_versions: HashMap<String, String>,
    pub container_digests: HashMap<String, String>,
    pub finding_counts: FindingCounts,
    pub risk_score: u8,
    pub engines_run: Vec<String>,
    pub optional_inputs_used: OptionalInputsSummary,
}

#[derive(Default, Serialize, Deserialize)]
pub struct FindingCounts {
    pub critical: u32,
    pub high: u32,
    pub medium: u32,
    pub low: u32,
    pub observation: u32,
}

impl FindingCounts {
    pub fn risk_score(&self) -> u8 {
        let raw = 100i32
            - (self.critical as i32 * 25)
            - (self.high as i32 * 15)
            - (self.medium as i32 * 5)
            - (self.low as i32 * 2);
        raw.clamp(0, 100) as u8
    }
}
```

**Acceptance criteria:**
- [ ] All types compile with `serde` round-trip tests (JSON serialize → deserialize → assert eq)
- [ ] `Finding` JSON schema generated and committed to `docs/finding-schema.json`
- [ ] `audit.yaml` JSON schema generated and committed to `docs/audit-yaml-schema.json`
- [ ] `FindingCounts::risk_score()` unit tested against a table of known inputs/outputs
- [ ] `AuditContext` can be constructed in a test with all fields populated

---

### Task 0.2 — `crates/intake` — All User Input Handling ★ New

**Crate:** `crates/intake`

This crate is the entry point for all user-provided data. It handles source acquisition, `audit.yaml` parsing, workspace detection, framework detection, optional input parsing, and the confirmation step. Nothing else in the system touches raw user input.

#### 0.2a — Source Resolver

```rust
// crates/intake/src/source.rs

pub struct SourceResolver;

impl SourceResolver {
    /// Resolve any source form into a local directory ready for analysis.
    /// This is always the first step — all later steps work on local paths.
    pub async fn resolve(input: &SourceInput, work_dir: &Path) -> Result<ResolvedSource>;
}

pub enum SourceInput {
    GitUrl {
        url: String,
        commit: String,          // required — CLI refuses to accept a branch name
        auth: Option<GitAuth>,   // None → unauthenticated (public repos only)
    },
    LocalPath {
        path: PathBuf,
        commit: Option<String>,  // if None: auto-read from `git rev-parse HEAD`
    },
    Archive {
        path: PathBuf,           // .tar.gz or .zip
    },
}

/// Authentication for private Git repositories.
/// Token is never written to Evidence Pack or audit-manifest.json.
pub enum GitAuth {
    /// GitHub/GitLab personal access token (HTTPS).
    /// Source: `GIT_TOKEN` env var or `--git-token` CLI flag.
    Token(String),
    /// SSH key file (for SSH URLs: git@github.com:org/repo).
    /// Source: `--ssh-key` CLI flag or auto-detected from `~/.ssh/`.
    SshKey { path: PathBuf, passphrase: Option<String> },
    /// Netrc file (reads credentials from `~/.netrc` automatically via git2).
    Netrc,
}

impl SourceResolver {
    async fn clone_git(url: &str, commit: &str, auth: Option<&GitAuth>, dest: &Path) -> Result<ResolvedSource> {
        // 1. Configure git2 callbacks with auth (token → header, SSH → ssh-agent/key, Netrc → default)
        // 2. git clone --no-checkout url dest
        // 3. git checkout commit
        // 4. verify HEAD matches commit exactly (defense against TOCTOU)
        // 5. compute content_hash = sha256(find . -type f | sort | xargs cat)
        todo!()
    }

    async fn resolve_local(path: &Path, commit: Option<&str>) -> Result<ResolvedSource> {
        // 1. verify path exists and is a Cargo workspace
        // 2. if commit is None: git rev-parse HEAD
        // 3. warn if working tree is dirty (uncommitted changes)
        // 4. compute content_hash
        todo!()
    }

    async fn unpack_archive(archive: &Path, dest: &Path) -> Result<ResolvedSource> {
        // 1. detect format (.tar.gz or .zip) by magic bytes, not extension
        // 2. unpack to dest
        // 3. find workspace root (Cargo.toml with [workspace])
        // 4. commit_hash = "archive:" + sha256(archive bytes)
        // 5. content_hash = sha256 of unpacked tree
        todo!()
    }
}
```

**Hard rule enforced here:** if user provides a branch name (e.g., `main`) instead of a commit SHA, the resolver auto-resolves it to the current SHA and returns a `Warning::BranchResolved { branch, resolved_sha }`. The UI/CLI displays this prominently and asks for confirmation. The audit is always pinned to a SHA.

#### 0.2b — Config Parser

```rust
// crates/intake/src/config.rs

pub struct ConfigParser;

impl ConfigParser {
    /// Parse and validate audit.yaml into a RawAuditConfig.
    /// Returns structured errors for every invalid field (not just the first).
    pub fn parse(path: &Path) -> Result<RawAuditConfig, Vec<ConfigError>>;

    /// Apply defaults and cross-field validation.
    pub fn validate(raw: RawAuditConfig) -> Result<ValidatedConfig, Vec<ConfigError>>;
}

// The YAML representation (before merging with auto-detected data)
#[derive(Deserialize)]
pub struct RawAuditConfig {
    pub source: RawSource,
    pub scope: Option<RawScope>,
    pub engines: Option<RawEngineConfig>,
    pub budget: Option<RawBudgetConfig>,
}

pub enum ConfigError {
    MissingField { field: String },
    InvalidCommitHash { value: String },
    BranchNameNotAllowed { branch: String, hint: String },
    UnknownCrate { crate_name: String, available: Vec<String> },
    InvalidBudgetValue { field: String, value: u64, reason: String },
    ConflictingOptions { field_a: String, field_b: String },
}
```

#### 0.2c — Workspace Analyzer

```rust
// crates/intake/src/workspace.rs

pub struct WorkspaceAnalyzer;

impl WorkspaceAnalyzer {
    pub fn analyze(root: &Path) -> Result<CargoWorkspace>;
}

pub struct CargoWorkspace {
    pub root: PathBuf,
    pub members: Vec<CrateMeta>,
    pub dependency_graph: DependencyGraph,
    pub feature_flags: FeatureFlagMap,    // crate → Vec<FeatureFlag>
}

pub struct CrateMeta {
    pub name: String,
    pub path: PathBuf,
    pub kind: CrateKind,    // Lib | Bin | Test | Bench | FuzzTarget
    pub dependencies: Vec<Dependency>,
}

pub enum CrateKind {
    Lib,
    Bin,
    Test,
    Bench,
    FuzzTarget,            // detected by: cargo-fuzz patterns or [[bin]] in [fuzz] workspace
    Example,
}

// Auto-detection of excluded crates — these patterns are almost never in scope
impl WorkspaceAnalyzer {
    pub fn suggest_exclusions(workspace: &CargoWorkspace) -> Vec<ExclusionSuggestion> {
        workspace.members.iter()
            .filter(|c| matches!(c.kind, CrateKind::Bench | CrateKind::FuzzTarget)
                || c.name.contains("-bench")
                || c.name.contains("-fuzz")
                || c.name.contains("-example"))
            .map(|c| ExclusionSuggestion {
                crate_name: c.name.clone(),
                reason: format!("{:?} crate — typically not in audit scope", c.kind),
            })
            .collect()
    }
}
```

#### 0.2d — Framework Detector

```rust
// crates/intake/src/detection.rs

pub struct FrameworkDetector;

impl FrameworkDetector {
    /// Scan workspace source files and detect which ZK/crypto frameworks are present.
    /// Uses tree-sitter for speed (this runs before the full semantic index).
    pub fn detect(workspace: &CargoWorkspace) -> DetectionResult;
}

pub struct DetectionResult {
    pub frameworks: Vec<DetectedFramework>,
    pub crypto_divergent_features: Vec<CryptoDivergentFeature>,
    pub entry_points: Vec<DetectedEntryPoint>,
}

pub struct DetectedFramework {
    pub framework: Framework,
    pub confidence: Confidence,     // High | Medium | Low
    pub evidence: Vec<String>,      // e.g., ["Chip::configure found in zk-core/src/chip.rs:42"]
}

pub struct CryptoDivergentFeature {
    pub feature_name: String,
    pub crate_name: String,
    pub description: String,        // "enters assembly path in field arithmetic"
    pub affected_files: Vec<PathBuf>,
}

pub struct DetectedEntryPoint {
    pub function: String,
    pub crate_name: String,
    pub file: PathBuf,
    pub line: u32,
    pub kind: EntryPointKind,       // Verifier | Prover | Ingest | GuestEntry
}

// Detection signatures (tree-sitter patterns)
const HALO2_SIGNATURES: &[&str] = &[
    "Chip::configure", "Chip::synthesize", "ConstraintSystem",
    "halo2_proofs", "halo2_gadgets",
];
const SP1_SIGNATURES: &[&str] = &["sp1_zkvm::entrypoint!", "sp1_zkvm::io::read"];
const RISC0_SIGNATURES: &[&str] = &["risc0_zkvm::guest::env", "risc0_zkvm::serde"];
const CIRCOM_EXTENSIONS: &[&str] = &[".circom"];
```

#### 0.2e — Optional Input Parser

```rust
// crates/intake/src/optional_inputs.rs

pub struct OptionalInputParser;

impl OptionalInputParser {
    /// Parse spec PDF/Markdown → extract candidate constraints as text
    pub async fn parse_spec(path: &Path) -> Result<ParsedSpecDocument>;

    /// Parse previous audit PDF/Markdown → extract prior findings for regression tracking  
    pub async fn parse_previous_audit(path: &Path) -> Result<ParsedPreviousAudit>;

    /// Parse custom invariants YAML
    pub fn parse_invariants(path: &Path) -> Result<Vec<CustomInvariant>>;

    /// Parse known entry points YAML
    pub fn parse_entry_points(path: &Path) -> Result<Vec<EntryPoint>>;
}

pub struct ParsedSpecDocument {
    pub source_path: PathBuf,
    pub extracted_constraints: Vec<CandidateConstraint>,
    pub sections: Vec<SpecSection>,
    pub raw_text: String,  // retained for LLM Role 2 search prioritization hints only
}

pub struct CandidateConstraint {
    pub structured: StructuredConstraint, // machine-readable; drives Z3/Kani directly
    pub source_text: String,              // original prose from spec, for auditor reference
    pub source_section: String,           // "§3.2 Range Proofs"
    pub confidence: Confidence,           // set by pattern match quality, not LLM certainty
    pub extraction_method: ExtractionMethod,
}

/// Z3/Kani consume this directly. LLM may normalize prose into this form,
/// but the struct is validated against a JSON schema before any tool uses it.
pub enum StructuredConstraint {
    Range      { signal: String, lower: BigUint, upper: BigUint },
    Uniqueness { field: String, scope: String },
    Binding    { field_a: String, field_b: String },
    Custom {
        assertion_code: String,
        /// Declares which tool will consume this assertion.
        /// Validation applied at extraction time:
        ///   Rust → `syn::parse_str::<Expr>` must succeed
        ///   Smt2 → must start with "(assert " and balance parentheses
        /// Constraints that fail validation are downgraded to `confidence: Low` and flagged.
        target: CustomAssertionTarget,
    },
}

pub enum CustomAssertionTarget {
    Rust,   // fed into Kani harness as `kani::assert!(...)`
    Smt2,   // fed into Z3 as an `(assert ...)` clause
}

pub enum ExtractionMethod {
    /// Regex/pattern matched directly — e.g. "∈ [0, 2^128)"
    PatternMatch,
    /// LLM normalized informal prose → StructuredConstraint, then schema-validated.
    /// Confidence capped at Medium regardless of LLM output.
    LlmNormalized,
}

pub struct ParsedPreviousAudit {
    pub source_path: PathBuf,
    pub prior_findings: Vec<PriorFinding>,
}

pub struct PriorFinding {
    pub id: String,                // from prior report, e.g. "AUDIT-2024-001"
    pub title: String,
    pub severity: Severity,
    pub description: String,
    pub status: PriorFindingStatus,  // Reported | Acknowledged | Fixed (if stated in doc)
    pub location_hint: Option<String>, // file/function mentioned in prior report
}
```

#### 0.2f — Workspace Confirmation

```rust
// crates/intake/src/confirmation.rs
// Renders the Tier-3 confirmation summary and collects user decisions

pub struct WorkspaceConfirmation;

pub struct ConfirmationSummary {
    pub crates: Vec<CrateDecision>,
    pub frameworks: Vec<DetectedFramework>,
    pub crypto_divergent_features: Vec<CryptoDivergentFeature>,
    pub build_matrix: Vec<BuildVariant>,
    /// Rough estimate shown to user before they confirm. Formula:
    ///   base = in_scope_crate_count × 8 mins          (rule scan)
    ///        + build_matrix.len() × 15 mins            (build per variant)
    ///        + kani_harness_estimate × kani_timeout_secs / 60
    ///        + z3_estimate × z3_timeout_secs / 60
    /// kani_harness_estimate = rule match count × 1.5 (P(escalation to harness))
    /// z3_estimate = circom_template_count (from DetectionResult)
    /// Display as a range ± 50% — not a guarantee.
    pub estimated_duration_mins: u64,
    pub warnings: Vec<IntakeWarning>,
}

pub enum CrateDecision {
    InScope    { meta: CrateMeta },
    Excluded   { meta: CrateMeta, reason: String },
    Ambiguous  { meta: CrateMeta, suggestion: String },  // user must choose
}

pub enum IntakeWarning {
    BranchResolved    { branch: String, resolved_sha: String },
    DirtyWorkingTree  { uncommitted_files: Vec<PathBuf> },
    LlmKeyMissing     { degraded_features: Vec<String> },
    LargeBuildMatrix  { variants: usize, estimated_hours: f32 },
    PreviousAuditParsed { prior_finding_count: usize },
}

impl WorkspaceConfirmation {
    // CLI: print summary, prompt for confirmation
    pub fn confirm_cli(summary: &ConfirmationSummary) -> Result<UserDecisions>;
    
    // UI: return summary as JSON for the Tauri frontend to render
    pub fn to_json(summary: &ConfirmationSummary) -> String;
}

pub struct UserDecisions {
    pub ambiguous_crates: HashMap<String, bool>,  // crate_name → include?
    pub override_features: Option<Vec<Vec<String>>>,
    pub confirmed: bool,
    pub export_audit_yaml: bool,  // if true, write resolved audit.yaml to disk
}
```

#### 0.2g — Intake Orchestrator (ties 0.2a–f together)

```rust
// crates/intake/src/lib.rs

pub struct IntakeOrchestrator;

pub struct IntakeResult {
    pub config: AuditConfig,          // fully resolved, ready for the DAG
    pub summary: ConfirmationSummary, // for display
    pub warnings: Vec<IntakeWarning>,
}

impl IntakeOrchestrator {
    pub async fn run(
        source: SourceInput,
        audit_yaml: &Path,
        optional: OptionalInputsRaw,
        work_dir: &Path,
    ) -> Result<IntakeResult>;
}

// The sequence:
// 1. SourceResolver::resolve(source) → ResolvedSource
// 2. ConfigParser::parse(audit_yaml) → ValidatedConfig
// 3. WorkspaceAnalyzer::analyze(resolved_source.local_path) → CargoWorkspace
// 4. FrameworkDetector::detect(workspace) → DetectionResult
// 5. OptionalInputParser::parse_*(optional inputs) → OptionalInputs
// 6. Merge everything → ConfirmationSummary
// 7. Wait for user confirmation (CLI prompt or IPC event)
// 8. Apply UserDecisions → AuditConfig (fully resolved)
```

**Acceptance criteria for Task 0.2:**
- [ ] `SourceResolver` clones a public GitHub repo, checks out a specific commit, and computes content hash
- [ ] `SourceResolver` rejects a branch name with a clear error message; auto-resolves and warns when forced
- [ ] `SourceResolver` unpacks a `.tar.gz` archive and finds the workspace root
- [ ] `ConfigParser` returns all validation errors at once (not just the first one)
- [ ] `WorkspaceAnalyzer` correctly identifies `CrateKind::FuzzTarget` and `Bench` for auto-exclusion
- [ ] `FrameworkDetector` detects Halo2 in `halo2-gadgets` and SP1 in a project using `sp1_zkvm::entrypoint!`
- [ ] `FrameworkDetector` detects `feature="asm"` as a crypto-divergent feature flag
- [ ] `OptionalInputParser` extracts at least 3 candidate constraints from a ZK whitepaper PDF or Markdown spec (Markdown accepted as primary; PDF via `pdf-extract` with known test fixture pre-validated)
- [ ] `ConfirmationSummary` serializes to JSON that the UI can render
- [ ] End-to-end: `IntakeOrchestrator::run()` on a real GitHub ZK repo produces a valid `AuditConfig`
- [ ] `GitAuth::Token` clones a private repo (integration test against a private test repo with `GIT_TOKEN` env var)
- [ ] `GitAuth` token value is absent from `audit-manifest.json` and all Evidence Pack files (grep check)

---

### Task 0.3 — Container Infrastructure

**Directory:** `containers/`

```dockerfile
# containers/kani/Dockerfile
FROM rust:1.82.0-slim-bookworm
RUN cargo install kani-verifier --version 0.57.0 --locked
RUN rustup toolchain install nightly-2024-11-01
LABEL tool.kani.version="0.57.0"
LABEL tool.rustc.version="nightly-2024-11-01"
WORKDIR /workspace
```

```dockerfile
# containers/z3/Dockerfile
FROM ubuntu:24.04
RUN apt-get update && apt-get install -y --no-install-recommends z3=4.13.0*
LABEL tool.z3.version="4.13.0"
WORKDIR /workspace
```

```dockerfile
# containers/madsim/Dockerfile
FROM rust:1.82.0-slim-bookworm
RUN cargo install madsim-util --version 0.2.30 --locked
LABEL tool.madsim.version="0.2.30"
WORKDIR /workspace
```

```toml
# containers/versions.toml — single source of truth, never auto-updated
[tools]
kani                  = "0.57.0"
kani_rustc_toolchain  = "nightly-2024-11-01"
z3                    = "4.13.0"
madsim                = "0.2.30"
miri_rustc_toolchain  = "nightly-2024-11-01"
cargo_fuzz            = "0.12.0"
cargo_audit           = "0.21.0"
circom                = "2.1.9"
```

**Acceptance criteria:**
- [ ] All 5 images build in CI without errors
- [ ] Each image's `--version` output matches the pinned version in `versions.toml`
- [ ] Image digests are captured programmatically and can be written to `AuditManifest`
- [ ] `versions.toml` is the only place version strings live (no duplication in Dockerfiles)

---

### Task 0.4 — Sandbox Executor

**Crate:** `crates/sandbox`

```rust
pub struct SandboxExecutor {
    docker: Docker,                    // bollard crate
    image_registry: ImageRegistry,
}

pub struct ExecutionRequest {
    pub image: ToolImage,              // Kani | Z3 | Miri | MadSim | Fuzz
    pub command: Vec<String>,
    pub mounts: Vec<Mount>,            // (host_path, container_path, read_only)
    pub env: HashMap<String, String>,
    pub budget: ResourceBudget,
    pub network: NetworkPolicy,        // Disabled | Allowlist(Vec<String>)
}

pub struct ExecutionResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub artifacts: Vec<PathBuf>,       // files written to container's /out/
    pub container_digest: String,      // sha256 of the image used
    pub duration_ms: u64,
    pub resource_usage: ResourceUsage,
}

pub struct ResourceBudget {
    pub cpu_cores: f64,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub timeout_secs: u64,
}
```

**Acceptance criteria:**
- [ ] Runs `echo hello` in a container and captures stdout
- [ ] Timeout kills the container and returns `Err(SandboxError::Timeout)`
- [ ] Memory limit enforced; OOM returns `Err(SandboxError::OomKilled)`
- [ ] Container digest captured and matches `docker inspect --format '{{.Id}}'`
- [ ] Rootless mode works in CI (test with rootless Docker socket)
- [ ] Network disabled by default; `curl example.com` returns connection error, not success

---

### Task 0.5 — Evidence Store

**Crate:** `crates/evidence`

```rust
pub struct EvidenceStore { base_dir: PathBuf }

impl EvidenceStore {
    pub async fn save_pack(&self, finding_id: &FindingId, pack: &EvidencePack) -> Result<()>;
    pub async fn generate_reproduce_script(&self, finding_id: &FindingId) -> Result<String>;
    pub async fn export_zip(&self, finding_ids: &[FindingId], dest: &Path) -> Result<()>;
    pub async fn load_manifest(&self, finding_id: &FindingId) -> Result<EvidenceManifest>;
}
```

**`reproduce.sh` template:**
```bash
#!/usr/bin/env bash
# Auto-generated by audit-agent v{version}
# Finding: {finding_id} — {title}
# Source commit: {commit_hash}   Content hash: {content_hash}
# Reproduced with: {tool} {tool_version}  Container: {container_digest}
set -euo pipefail
docker run --rm \
  --volume "{evidence_dir}:/evidence:ro" \
  --network none \
  {container_image}@{container_digest} \
  {reproduction_command}
# Expected: {expected_output_description}
```

**Acceptance criteria:**
- [ ] Save + load round-trip for all evidence file types
- [ ] `reproduce.sh` runs successfully in a clean Docker environment
- [ ] ZIP export file count matches `manifest.json` files list
- [ ] `export_zip` for multiple finding IDs creates a single ZIP with all findings

---

### Task 0.6 — CI Pipeline

**File:** `.github/workflows/ci.yml`

```yaml
jobs:
  lint:       cargo clippy --workspace -- -D warnings
  test:       cargo test --workspace
  containers: docker build for all 5 images + version smoke tests
  schema:     validate docs/finding-schema.json and docs/audit-yaml-schema.json
              against Rust types using cargo test --test schema_compat
```

**Acceptance criteria:**
- [ ] All jobs green on `main` and every PR
- [ ] Schema compatibility test fails if `Finding` fields change without updating `finding-schema.json`
- [ ] Container builds are layer-cached; rebuild takes < 30s when only code changes

---

## Phase 1 — Crypto API Misuse Detection (Week 3–5)

**Goal:** Ship the fastest differentiator — rule-based static analysis for cryptographic API misuse. No LLM, no formal methods. Pure tree-sitter + semantic rules with full I/O wired.

**Inputs consumed in this phase:**
- `ResolvedSource` (from intake)
- `ResolvedScope.target_crates` and `build_matrix`
- `OptionalInputs.known_entry_points` (guides rule scanning priority)

**Outputs produced in this phase:**
- `findings.json` (rule-match findings, `evidence_gate_level: 0`)
- `evidence-pack.zip` (manifest + reproduce.sh per finding)
- `report-technical.md` (first usable report)
- `findings.sarif` (CI integration ready)

---

### Task 1.1 — Repo Intake Integration & Build Matrix

**Crate:** `crates/engine-crypto` → `src/intake_bridge.rs`

Consume the `AuditConfig` from `crates/intake` and prepare the engine context.

```rust
pub struct CryptoEngineContext {
    pub workspace: CargoWorkspace,
    pub build_matrix: Vec<BuildVariant>,
    pub entry_points: Vec<DetectedEntryPoint>,  // from detection + optional entries.yaml
    pub spec_constraints: Vec<CandidateConstraint>, // from spec PDF if provided
    pub environment_manifest: EnvironmentManifest,
}

pub struct EnvironmentManifest {
    pub rust_toolchain: String,
    pub cargo_lock_hash: String,
    pub workspace_root: PathBuf,
    pub audit_id: String,
    pub content_hash: String,          // from ResolvedSource
}
```

**Acceptance criteria:**
- [ ] Builds engine context from a valid `AuditConfig` without additional user input
- [ ] `EnvironmentManifest` written to every `EvidencePack.manifest.json` for findings in this engine

---

### Task 1.2 — Crypto Misuse Rule Engine

**Crate:** `crates/engine-crypto` → `src/rules/`

```rust
pub struct RuleEvaluator {
    rules: Vec<CryptoMisuseRule>,
    parser: tree_sitter::Parser,
}

pub struct RuleMatch {
    pub rule_id: String,
    pub location: CodeLocation,         // includes snippet
    pub matched_snippet: String,
    pub confidence: Confidence,
}

impl RuleEvaluator {
    pub fn load_from_dir(rules_dir: &Path) -> Result<Self>;
    pub async fn evaluate_file(&self, file: &SourceFile) -> Vec<RuleMatch>;
    pub async fn evaluate_workspace(&self, ctx: &CryptoEngineContext) -> Vec<RuleMatch>;
}
```

**Rule YAML schema:**
```yaml
id: CRYPTO-001
title: "Potential nonce reuse in encryption context"
severity: High
category: CryptoMisuse
description: |
  Nonce derived from constant or counter without domain-binding.
  Nonce reuse in AEAD breaks confidentiality.
detection:
  patterns:
    - type: function_call
      name_matches: ["encrypt", "seal", "aead_encrypt"]
      argument_pattern:
        position: 1
        matches_any:
          - type: literal
          - type: counter_expr
  semantic_checks:
    - nonce_is_not_bound_to_session_id
references:
  - "https://eprint.iacr.org/2016/475"
remediation: |
  Derive nonces as HKDF(session_key, domain_separator, counter).
```

**Required rules for Phase 1 (minimum 8):**

| ID | Rule | Severity |
|----|------|----------|
| CRYPTO-001 | Nonce reuse / missing domain binding | High |
| CRYPTO-002 | Missing domain separator in transcript/hash | High |
| CRYPTO-003 | Field element deserialization without canonicality check | High |
| CRYPTO-004 | Weak or deterministic RNG in crypto-critical path | Critical |
| CRYPTO-005 | Missing point validation (small-subgroup check absent) | High |
| CRYPTO-006 | Unchecked `unwrap()` on crypto result type in hot path | Medium |
| CRYPTO-007 | Hardcoded cryptographic constant (key/seed in source) | Critical |
| CRYPTO-008 | `unsafe` block in signature verification critical path | Medium |

**Acceptance criteria:**
- [ ] All 8 rules fire on synthetic fixtures in `tests/fixtures/rust-crypto/`
- [ ] Each match produces `CodeLocation` with exact file + line range + 10-line snippet
- [ ] Rules load from YAML at startup (no recompile needed to add a rule)
- [ ] False positive rate < 20% on `halo2/src/` (manual spot-check against 20 files)

---

### Task 1.3 — cargo-audit Call-Path Correlation

**Crate:** `crates/engine-crypto` → `src/supply_chain.rs`

> **Implementation note:** Phase 1 uses a **tree-sitter call graph** (function call extraction from source text). The full rust-analyzer `SemanticIndex` — which resolves trait impls and cross-crate macro expansions — is not available until Phase 3 (Task 3.1). The tree-sitter graph is sufficient for the name-based matching used in Phase 1 escalation. In Phase 3, `SupplyChainAnalyzer` is upgraded to use `SemanticIndex` for higher-precision reachability.

```rust
pub struct SupplyChainAnalyzer {
    /// Phase 1: TreeSitterCallGraph (name-based, no cross-crate resolution)
    /// Phase 3+: SemanticCallGraph (rust-analyzer backed, full resolution)
    call_graph: CallGraphBackend,
}

pub enum CallGraphBackend {
    TreeSitter(TreeSitterCallGraph),   // available from Phase 1
    Semantic(SemanticCallGraph),       // available from Phase 3
}

pub struct CveCallPathResult {
    pub cve_id: String,
    pub crate_name: String,
    pub affected_fn: String,
    pub reachable_from_crypto_path: bool,
    pub call_chain: Vec<String>,
    pub original_severity: Severity,
    pub adjusted_severity: Severity,
    pub adjustment_reason: String,
    pub graph_backend: String,         // "tree-sitter" or "semantic" — recorded in finding evidence
}

impl SupplyChainAnalyzer {
    pub async fn analyze(&self, workspace: &CargoWorkspace) -> Result<Vec<CveCallPathResult>>;
}
```

**Escalation logic:**
```
CVE in crate X, fn F:
  F not reachable from {verify, prove, sign, keygen, ingest}  →  keep original severity
  F reachable from crypto path                                 →  escalate to High
  F in direct hot path (≤ 3 call frames from entry)           →  escalate to Critical
```

**Acceptance criteria:**
- [ ] Escalates a CVE in `curve25519-dalek` when reachable from a signing function (tree-sitter graph)
- [ ] Downgrades a CVE in a dev-dependency to Low
- [ ] Full call chain included in finding for auditor review
- [ ] `graph_backend` field recorded in every supply chain finding's evidence

---

### Task 1.4 — Output Writers: SARIF, JSON, Technical Report

**Crate:** `crates/findings` + `crates/report`

```rust
// crates/findings/src/sarif.rs
pub fn to_sarif(findings: &[Finding], manifest: &AuditManifest) -> SarifReport;

// crates/findings/src/json_export.rs
pub fn to_findings_json(findings: &[Finding]) -> String;

// crates/report/src/technical.rs
pub fn render_technical_report(findings: &[Finding], manifest: &AuditManifest) -> String; // Markdown
```

**SARIF output used for GitHub Actions:**
```yaml
# .github/workflows/audit.yml (example for clients)
- name: Upload SARIF
  uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: audit-output/findings.sarif
```

**Acceptance criteria:**
- [ ] SARIF output validates against SARIF 2.1.0 schema (use `sarifvalidator` tool in CI)
- [ ] `findings.json` validates against `docs/finding-schema.json`
- [ ] Technical report renders all findings with code snippets, no broken Markdown
- [ ] Each finding in the report has its `reproduce.sh` command inline

---

### Task 1.5 — Evidence Pack v1 + Regression Tests (Phase 1 variant)

**Crate:** `crates/evidence` + `crates/report`

Phase 1 findings are rule-match findings — no Kani/Z3 evidence yet. The pack contains:
- `manifest.json` with rule ID, matched snippet, tool version
- `reproduce.sh` that re-runs the scanner on the same file + commit

```rust
// crates/report/src/regression.rs
pub fn generate_regression_tests(findings: &[Finding]) -> RegressionTestSuite;

pub struct RegressionTestSuite {
    pub crypto_tests: Option<String>,     // Rust proptest code
    pub kani_harnesses: Vec<KaniHarness>, // populated in Phase 2+
    pub madsim_scenarios: Vec<String>,    // populated in Phase 4+
}
```

**Acceptance criteria:**
- [ ] Evidence pack ZIP is produced for at least one finding from Phase 1 fixtures
- [ ] `reproduce.sh` re-triggers the rule match deterministically
- [ ] Regression test file compiles as valid Rust (even if test logic is a placeholder)
- [ ] Full output directory structure matches the spec in the I/O contract section

---

### Phase 1 End-to-End Test

```bash
audit-agent analyze \
  --source https://github.com/privacy-scaling-explorations/halo2 \
  --commit a1b2c3d4 \
  --config audit.yaml \
  --output-dir ./audit-output

# Expected outputs (all must exist):
ls audit-output/
  report-executive.md
  report-technical.md
  findings.json
  findings.sarif
  evidence-pack.zip
  audit-manifest.json
  regression-tests/crypto_misuse_tests.rs
```

---

## Phase 2 — Core ZK Verification: Circom Path (Week 6–9)

**Inputs consumed in this phase (additions over Phase 1):**
- `.circom` source files discovered by `FrameworkDetector`
- `OptionalInputs.spec_constraints` (from Spec Extractor — pattern-matched + LLM-normalized to structured form)
- `LLM_API_KEY` env var (if present: LLM provides `kani::assume()` search hints + scaffolding; if absent: template fallback — audit still runs fully)

**Outputs added in this phase:**
- Findings with `framework: Circom`, `evidence_gate_level: 2–3`
- `evidence_pack/{id}/smt2/` (Z3 query + output)
- `evidence_pack/{id}/harness/` (Kani harness)
- `evidence_pack/{id}/corpus/` (fuzz corpus)
- `regression-tests/kani_harnesses/` populated

---

### Task 2.1 — Circom Signal Graph Builder

```rust
// crates/engine-crypto/src/zk/circom/signal_graph.rs

pub struct CircomSignalGraph {
    pub signals: Vec<Signal>,
    pub constraints: Vec<Constraint>,
    pub templates: HashMap<String, Template>,
}

pub struct Signal {
    pub name: String,
    pub kind: SignalKind,               // Input | Output | Intermediate
    pub template: String,
    pub constrained_by: Vec<ConstraintId>,
}

pub enum Constraint {
    R1CS { a: LinearCombination, b: LinearCombination, c: LinearCombination },
    Equality { lhs: LinearCombination, rhs: LinearCombination },
}

impl CircomSignalGraph {
    pub fn from_file(path: &Path) -> Result<Self>;
    pub fn find_trivially_unconstrained(&self) -> Vec<Signal>;
    pub fn to_smt2(&self, target_signal: &str, field_prime: &BigUint) -> String;
}
```

**Acceptance criteria:**
- [ ] Correctly parses `circomlib/circuits/comparators.circom`
- [ ] `find_trivially_unconstrained` catches a manually introduced unconstrained output
- [ ] SMT2 export parses without errors in Z3

---

### Task 2.2 — Z3 Under-Constrained Checker

```rust
// crates/engine-crypto/src/zk/circom/z3_checker.rs

pub struct Z3UnderConstrainedChecker { sandbox: Arc<SandboxExecutor> }

pub enum Z3CheckResult {
    UnderConstrained {
        witness_a: HashMap<String, BigUint>,
        witness_b: HashMap<String, BigUint>,
        smt2_file: PathBuf,      // saved for Evidence Pack
    },
    Constrained { proof_file: PathBuf },
    Unknown { reason: String, fallback_result: Option<RandomSearchResult> },
}

impl Z3UnderConstrainedChecker {
    pub async fn check(&self, smt2: &str, budget: &BudgetConfig) -> Result<Z3CheckResult>;
    
    // Fallback when Z3 returns Unknown
    async fn random_witness_search(
        &self,
        graph: &CircomSignalGraph,
        iterations: u64,
        seed: u64,
    ) -> Option<CounterexamplePair>;
}
```

**Acceptance criteria:**
- [ ] Detects the known under-constrained `LessThan` gadget in circomlib
- [ ] Timeout falls back to random search, with seed captured for Evidence Pack
- [ ] Container digest recorded in every `Z3CheckResult`

---

### Task 2.3 — Kani Harness Scaffolder + Evidence Gate (Closes G9)

The key architectural point: **the rule engine decides what to assert; LLM only helps focus the search and fill scaffolding.**

```rust
// crates/engine-crypto/src/kani/scaffolder.rs

pub struct KaniHarnessScaffolder {
    llm: Option<Arc<dyn LlmProvider>>,   // None → pure template mode
    sandbox: Arc<SandboxExecutor>,
}

/// Everything the scaffolder needs. Comes from the rule engine — NOT from LLM.
pub struct HarnessRequest {
    pub target_fn: FunctionSignature,
    pub source_context: String,          // function source + 20 lines of context
    pub rule_trigger: RuleTrigger,       // which rule fired and why
    pub required_assertion: AssertionSpec, // ← set by rule engine; LLM cannot change this
    pub max_bound: u64,
}

/// The assertion to check — generated deterministically from the rule, not the LLM.
pub enum AssertionSpec {
    NoOverflow  { operation: String },           // from CRYPTO-rule detecting unchecked arithmetic
    NoUnwrapPanic { call_site: CodeLocation },   // from CRYPTO-006
    FieldElementInRange { var: String, max: BigUint }, // from range constraint
    CustomAssertion { code: String },            // from spec extractor structured output
}

pub struct HarnessResult {
    pub harness_code: String,
    pub cargo_toml: String,
    pub gate_level_reached: u8,
    pub kani_output: Option<KaniOutput>,
    pub llm_assume_hints_used: bool,     // true if LLM contributed assume() constraints
    pub shrink_attempts: u8,
}

impl KaniHarnessScaffolder {
    pub async fn build(&self, req: &HarnessRequest) -> Result<HarnessResult> {
        // Step 1: Template engine generates harness skeleton with:
        //   - kani::any::<T>() for all inputs (deterministic)
        //   - kani::assert!() from req.required_assertion (deterministic, from rule)
        let skeleton = self.generate_skeleton(req);

        // Step 2 (optional, Role 2): if LLM available, ask for kani::assume() hints
        //   to focus the search space. These ONLY add preconditions — never change
        //   the assertion being checked.
        let assume_hints = if let Some(llm) = &self.llm {
            self.request_assume_hints(llm, req, &skeleton).await
                .unwrap_or_default()   // failure → proceed with no hints, not an error
        } else {
            vec![]
        };

        // Step 3: Insert assume hints into skeleton → final harness
        let harness = self.assemble(skeleton, assume_hints);

        // Step 4: Evidence Gate validates it (compile → execute → reproduce)
        self.evidence_gate.validate(&harness, req).await
    }

    /// Role 2 prompt — LLM may only suggest kani::assume() lines.
    /// Explicitly forbidden: changing assertions, adding new assert!(), claiming bugs.
    async fn request_assume_hints(
        &self,
        llm: &dyn LlmProvider,
        req: &HarnessRequest,
        skeleton: &str,
    ) -> Result<Vec<String>> {
        let prompt = format!(
            "You are helping focus a Kani model checker search.\n\
             The assertion being verified is fixed: {assertion}\n\
             Suggest kani::assume() preconditions that will help Kani find a \
             counterexample faster without over-constraining the input space.\n\
             Output ONLY valid Rust kani::assume!(...) lines. \
             Do NOT add new assert!() calls. Do NOT change existing assertions.\n\
             Function:\n{context}",
            assertion = req.required_assertion.to_string(),
            context = req.source_context,
        );
        let raw = llm.complete(&prompt, &CompletionOpts { temperature: 0.1, ..Default::default() }).await?;
        Ok(parse_assume_lines(&raw))
        // parse_assume_lines rules:
        //   1. Only lines matching `kani::assume!(...)` are kept.
        //   2. Lines containing trivially-false literals (`false`, `0 == 1`, `1 == 0`,
        //      `kani::assume!(false)`) are rejected — these over-constrain to vacuous truth.
        //   3. Maximum 8 assume lines accepted per harness to bound LLM influence.
    }
}
```

```rust
// crates/llm/src/evidence_gate.rs

pub struct EvidenceGate { sandbox: Arc<SandboxExecutor> }

impl EvidenceGate {
    pub async fn validate(&self, harness: &HarnessCode, req: &HarnessRequest) -> GateResult;

    /// LLM fix loop: Role 1 only — fix syntax/type errors, nothing else.
    /// Prompt explicitly forbids changing assertions.
    pub async fn fix_syntax_and_retry(
        &self,
        harness: &HarnessCode,
        compile_error: &str,
        llm: &dyn LlmProvider,
        max_retries: u8,
    ) -> GateResult;
}

pub struct GateResult {
    pub level_reached: u8,           // 0–3
    pub passed: bool,
    pub counterexample: Option<String>,
    pub failure_reason: Option<String>,
    pub attempts: u8,
    pub llm_fixed_syntax: bool,      // true if LLM was used to fix compile errors
}
```

**Evidence Gate levels (unchanged):**
```
Level 0 — Syntax:    rustfmt --check → fail → LLM fixes syntax/types (Role 1) → retry
Level 1 — Compile:   cargo build --features kani → fail → LLM fixes types (Role 1) → retry
Level 2 — Execute:   kani runs in sandbox → timeout → degrade to proptest
Level 3 — Reproduce: re-run with fixed seed → confirm same counterexample
```

**Acceptance criteria:**
- [ ] `AssertionSpec` is always set from the rule trigger — no code path allows LLM to set it
- [ ] `request_assume_hints` output is filtered: any line that is not `kani::assume!(...)` is dropped
- [ ] `parse_assume_lines` rejects `kani::assume!(false)` and other trivially-false literals (unit test with adversarial LLM mock returning `kani::assume!(false)`)
- [ ] `parse_assume_lines` caps output at 8 lines regardless of LLM verbosity
- [ ] Harness generated with no LLM available (`llm: None`) still compiles and runs
- [ ] `llm_assume_hints_used: true` is set on findings where LLM contributed assume hints
- [ ] Kani counterexample for `unchecked_add` captured in Evidence Pack with `verification_status: Verified`
- [ ] Evidence Gate fix loop prompt tested to confirm it cannot introduce new assertions (unit test with adversarial LLM mock)

---

### Task 2.4 — LLM Provider Adapters + Role Enforcement

```rust
// crates/llm/src/provider.rs

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, prompt: &str, opts: &CompletionOpts) -> Result<String>;
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
}

/// The three permitted LLM roles. Every call site must declare one.
#[derive(Debug, Clone, PartialEq)]
pub enum LlmRole {
    Scaffolding,     // Role 1: boilerplate, syntax fixes, type coercions
    SearchHints,     // Role 2: kani::assume() hints, CDG node prioritization
    ProseRendering,  // Role 3: recommendation/impact text polish
}

pub struct OpenAiProvider    { api_key: String, model: String }
pub struct AnthropicProvider { api_key: String, model: String }
pub struct OllamaProvider    { base_url: String, model: String }

/// Used automatically when LLM_API_KEY absent. Always available. Never fails.
pub struct TemplateFallback;

impl LlmProvider for TemplateFallback {
    async fn complete(&self, prompt: &str, _: &CompletionOpts) -> Result<String> {
        template_library::match_prompt(prompt)  // returns best-fit scaffold or empty string
    }
    fn is_available(&self) -> bool { true }
    fn name(&self) -> &str { "template-fallback" }
}

/// The ONLY way to invoke LLM anywhere in the codebase.
/// Direct calls to provider.complete() are forbidden — enforced by CI lint rule.
pub async fn llm_call(
    provider: &dyn LlmProvider,
    role: LlmRole,
    prompt: &str,
    opts: &CompletionOpts,
) -> Result<String> {
    tracing::debug!(role = ?role, provider = provider.name(), "LLM call");
    let response = provider.complete(prompt, opts).await?;
    tracing::trace!(role = ?role, chars = response.len(), "LLM response");
    // Prompt and response logged at TRACE only — never written to Evidence Pack
    Ok(response)
}
```

**Acceptance criteria:**
- [ ] All three providers + `TemplateFallback` implement `LlmProvider`
- [ ] `LLM_API_KEY` absent → `TemplateFallback` used silently; no stderr output
- [ ] CI lint rule (`forbid(direct_llm_complete)`) fails if any crate calls `provider.complete()` directly
- [ ] `llm_call()` logs role + provider at DEBUG; never writes to Evidence Pack (grep check in CI)
- [ ] `TemplateFallback` returns compilable harness skeleton for `field_mul`, `field_add`, `verify_proof`

---

### Phase 2 End-to-End Test

```bash
audit-agent analyze \
  --source https://github.com/iden3/circomlib \
  --commit abc123 \
  --config audit.yaml

# Must produce:
# findings.json: ≥1 finding with category=UnderConstrained, framework=Circom,
#                evidence_gate_level=3, verification_status=Verified
# evidence-pack.zip/{finding_id}/smt2/query.smt2  (non-empty)
# evidence-pack.zip/{finding_id}/reproduce.sh      (runs in < 60s, produces "sat")
# regression-tests/kani_harnesses/*.rs             (at least one file)
# report-technical.md: Verified badge on Z3 finding; no Unverified findings in Phase 2
```

---

## Phase 3 — Halo2 + Constraint Dependency Graph (Week 10–13)

**Inputs consumed (additions over Phase 2):**
- Halo2 source files detected by `FrameworkDetector`
- `OptionalInputs.spec_constraints` (higher value here than Circom — spec often defines chip behavior)

**Outputs added:**
- Findings with `framework: Halo2`
- CDG visualization data (for UI DAG view)
- Halo2-specific harnesses in `regression-tests/kani_harnesses/`

---

### Task 3.1 — rust-analyzer Integration

> **Implementation risk:** `ra_ap_ide`, `ra_ap_hir`, and `ra_ap_syntax` are internal rust-analyzer crates pinned at an exact version (`=0.0.239`). These APIs break across minor bumps, compile slowly (~3–5 min cold), and may conflict with the workspace's own `proc-macro-srv` version. This is the highest-risk dependency in the project.
>
> **Mitigation / fallback strategy:**
> 1. First, attempt the `ra_ap_*` library approach with the pinned version.
> 2. If integration proves unstable within Phase 3 week 1, switch to spawning `rust-analyzer` as an LSP subprocess and querying it over JSON-RPC (`textDocument/references`, `workspace/symbol`). This is slower but version-stable.
> 3. `SemanticIndex::build` must be wrapped in a timeout: if RA fails to index the workspace within `budget.semantic_index_timeout_secs` (default: 120), degrade to the tree-sitter graph and emit `Warning::SemanticIndexFailed`. Phase 3 analyses continue with reduced precision; findings note "semantic index unavailable — tree-sitter fallback used."
>
> Pin the RA version in a dedicated `ra-compat` workspace member that is compiled with a separate `rustup` toolchain if needed.

```rust
// crates/engine-crypto/src/semantic/ra_client.rs

pub struct SemanticIndex {
    pub call_graph: CallGraph,
    pub macro_expansions: MacroExpansionMap,
    pub trait_impls: TraitImplMap,
    pub cfg_variants: CfgVariantMap,
    pub backend: SemanticBackend,    // tracks which strategy succeeded
}

pub enum SemanticBackend {
    RustAnalyzer { version: String },
    LspSubprocess { ra_binary_version: String },
    TreeSitterFallback { reason: String },   // degraded; findings note reduced precision
}

impl SemanticIndex {
    pub async fn build(workspace: &CargoWorkspace, budget: &BudgetConfig) -> Result<Self>;
    pub fn find_trait_impls(&self, trait_name: &str, method: &str) -> Vec<FnRef>;
    pub fn expand_macro(&self, span: &SpanId) -> Option<&str>;
    pub fn cfg_divergence_points(&self) -> Vec<CfgDivergence>;
}
```

**Acceptance criteria:**
- [ ] Resolves `Chip::configure` to concrete implementations across crate boundaries
- [ ] Macro-expanded call graph differs from tree-sitter's for ≥1 Halo2 proc macro
- [ ] `cfg(feature="asm")` divergence detected in a crate that uses it
- [ ] `SemanticIndex::build` degrades to tree-sitter fallback on timeout; does not crash
- [ ] `SemanticBackend` variant recorded in `AuditManifest.tool_versions`

---

### Task 3.2 — Constraint Dependency Graph (Closes G2)

```rust
// crates/engine-crypto/src/zk/halo2/cdg.rs

pub struct ConstraintDependencyGraph {
    pub chips: Vec<ChipNode>,
    pub edges: Vec<CdgEdge>,
    pub risk_annotations: Vec<RiskAnnotation>,
}

pub enum RiskAnnotation {
    IsolatedNode    { chip: ChipName, column: ColumnName },
    RangeGap        { from_chip: ChipName, to_chip: ChipName, gap: String },
    SelectorConflict { chip_a: ChipName, chip_b: ChipName },
}

impl ConstraintDependencyGraph {
    pub fn build(semantic_index: &SemanticIndex) -> Result<Self>;
    pub fn high_risk_nodes(&self) -> Vec<&ChipNode>;
    pub fn to_dot(&self) -> String;       // Graphviz DOT for UI visualization
    pub fn to_json(&self) -> String;      // for UI CDG view
}
```

**Acceptance criteria:**
- [ ] Identifies all chips in `halo2-gadgets` with correct `configure`/`synthesize` spans
- [ ] Builds edges between `RangeCheckChip` and its consumers
- [ ] `IsolatedNode` fires for a manually introduced unconstrained column in a test fixture
- [ ] DOT output renders correctly in Graphviz (visual check)

---

### Task 3.3 — Halo2 Local SMT Checker + SP1/RISC0 Diff Tester

```rust
// crates/engine-crypto/src/zk/halo2/smt_checker.rs
pub struct Halo2SmtChecker { sandbox: Arc<SandboxExecutor> }

impl Halo2SmtChecker {
    pub async fn check_high_risk_nodes(
        &self,
        cdg: &ConstraintDependencyGraph,
        budget: &BudgetConfig,
    ) -> Vec<Finding>;
}

// crates/engine-crypto/src/zk/zkvm/diff_tester.rs
pub struct ZkvmDiffTester { sandbox: Arc<SandboxExecutor> }

impl ZkvmDiffTester {
    pub async fn run(&self, req: DiffTestRequest) -> Result<DiffTestResult>;
    pub async fn verify_image_hash_binding(&self, guest_path: &Path) -> Result<bool>;
}
```

**Acceptance criteria:**
- [ ] Finds under-constrained gate in synthetic Halo2 chip fixture
- [ ] SP1 divergence detected when native vs. zkVM outputs differ on boundary input
- [ ] `verify_image_hash_binding` fails for a mismatched image hash

---

## Phase 4 — Distributed Consensus Engine (Week 12–15)

> Phases 3 and 4 can run in parallel on separate engineer tracks. Only dependency is Task 0.4 (Sandbox Executor).

**Inputs consumed (first use of distributed inputs):**
- `AuditConfig.engines.distributed: true`
- `OptionalInputs.custom_invariants` (INV-001, INV-002, etc.)
- `OptionalInputs.known_entry_points` (network layer entry points)

**Outputs added:**
- Findings with `framework: MadSim` or `framework: Loom`
- `evidence_pack/{id}/traces/` (seed, trace.json, replay.sh)
- `regression-tests/madsim_scenarios/` populated

---

### Task 4.1 — MadSim Feasibility Assessor (Closes G4)

```rust
// crates/engine-distributed/src/feasibility.rs

pub enum BridgeLevel {
    LevelA,
    LevelB { adapter_points: Vec<AdapterPoint> },
    LevelC { reason: String },
}

impl MadSimFeasibilityAssessor {
    pub fn assess(workspace: &CargoWorkspace, semantic_index: &SemanticIndex) -> BridgeLevel;
}
```

**Acceptance criteria:**
- [ ] Simple echo server → LevelA
- [ ] `libp2p`-based project → LevelB with adapter points listed
- [ ] Project with scattered `tokio::Runtime::new()` → LevelC

---

### Task 4.2 — MadSim Harness Builder

**Crate:** `crates/engine-distributed` → `src/harness/builder.rs`

```rust
pub struct HarnessBuilder {
    llm: Option<Arc<dyn LlmProvider>>,   // None → pure template mode
    evidence_gate: Arc<EvidenceGate>,
}

pub struct MadSimHarness {
    pub project_dir: PathBuf,   // runnable Cargo project
    pub entry_point: String,    // fn name of the simulation entry
    pub node_count: usize,
    pub topology: NetworkTopology,
}

impl HarnessBuilder {
    // For LevelA: auto-generate (LLM Role 1 for entry call only)
    pub async fn generate_level_a(
        &self,
        workspace: &CargoWorkspace,
        entry_points: &[DetectedEntryPoint],
        config: &DistributedAuditConfig,
    ) -> Result<MadSimHarness> {
        // Step 1 (deterministic): template engine generates full harness skeleton
        //   - MadSim runtime setup
        //   - node spawn loop with NodeConfig from detected types
        //   - placeholder for entry point call: // ENTRY_POINT_CALL
        //   - invariant assertion calls at tick boundaries (from GlobalInvariantMonitor)
        let skeleton = self.generate_skeleton(entry_points, config);

        // Step 2 (Role 1, optional): LLM fills the entry point call only —
        //   e.g. translates detected fn start_node(cfg: NodeConfig)
        //   into:  node_handle.spawn(async { start_node(cfg.clone()).await });
        //   LLM output is filtered to a single spawn(async { ... }) line.
        //   If LLM unavailable or fails: insert TODO comment; harness still compiles.
        let entry_call = if let Some(llm) = &self.llm {
            llm_call(llm, LlmRole::Scaffolding,
                &self.entry_call_prompt(entry_points), &Default::default())
                .await
                .map(|raw| self.filter_entry_call(&raw))  // strips multi-line / non-spawn output
                .unwrap_or_else(|_| self.entry_call_todo_comment(entry_points))
        } else {
            self.entry_call_todo_comment(entry_points)
        };

        Ok(self.assemble(skeleton, entry_call))
    }

    // For LevelB: generate scaffold + adapter hints (no LLM)
    pub async fn generate_level_b_scaffold(
        &self,
        workspace: &CargoWorkspace,
        adapter_points: &[AdapterPoint],
    ) -> Result<AdapterScaffold>;

    /// Filters LLM output to exactly one `spawn(async { ... })` line.
    /// Rejects multi-line output; truncates to first matching line.
    fn filter_entry_call(&self, raw: &str) -> String {
        raw.lines()
            .find(|l| l.trim_start().starts_with("node_handle") || l.contains("spawn(async"))
            .unwrap_or("// TODO: fill entry point call")
            .to_string()
    }
}
```

**Key constraint:** LLM in the harness builder touches only the entry point call line. It does not write invariant assertions, does not configure the topology, and does not choose which chaos scenarios to run. Those are all deterministic.

**Harness template structure:**

```rust
// Template generated by the deterministic skeleton engine
#[madsim::test]
async fn audit_harness_{name}() {
    let handle = madsim::runtime::Handle::current();

    for i in 0..{node_count} {
        handle.create_node()
            .name(format!("node-{}", i))
            .ip(format!("10.0.0.{}", i + 1).parse().unwrap())
            .build()
            .spawn(async move {
                {entry_point_call}  // ← only this line comes from LLM (Role 1)
            });
    }

    madsim::time::sleep(Duration::from_secs({simulation_duration})).await;

    {invariant_assertions}  // ← generated deterministically from GlobalInvariantMonitor
}
```

**Acceptance criteria:**
- [ ] Level A harness compiles without LLM (entry point becomes TODO comment)
- [ ] Level B scaffold lists adapter points with file/line locations
- [ ] LLM entry call filtered: only a single `spawn(async { ... })` line accepted; multi-line output truncated to first matching line
- [ ] Harness runs to completion (smoke test) with 3 nodes, no chaos

---

### Task 4.3 — Chaos Script Engine

**Crate:** `crates/engine-distributed` → `src/chaos/`

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct ChaosScript {
    pub name: String,
    pub description: String,
    pub steps: Vec<ChaosStep>,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum ChaosStep {
    // Network chaos
    Partition       { nodes: Vec<NodeId>, duration_ticks: u64 },
    Delay           { nodes: Vec<NodeId>, delay_ms: u64, jitter_ms: u64 },
    Drop            { nodes: Vec<NodeId>, drop_rate: f64 },
    Duplicate       { nodes: Vec<NodeId>, dup_rate: f64 },
    Eclipse         { target: NodeId, duration_ticks: u64 },

    // Node byzantine behavior
    DoubleVote      { node: NodeId, at_height: u64 },
    SelectiveForward { node: NodeId, drop_from: Vec<NodeId> },
    ForgeVrfOutput  { node: NodeId },
    RefuseSync      { node: NodeId, for_heights: RangeInclusive<u64> },

    // L2-specific
    SequencerDropTx      { sequencer: NodeId, tx_pattern: TxPattern },
    ProposerReplayBatch  { proposer: NodeId, batch_index: u64 },
    ProverSubmitWrongStateRoot { prover: NodeId, at_height: u64 },

    // Timing / control
    Wait            { ticks: u64 },
    CheckInvariant  { invariant: InvariantId },
}
```

**Pre-built scenario templates (ship in Phase 4):**

```yaml
# scenarios/partition-then-rejoin.yaml
name: "Network Partition + Rejoin"
description: "Isolate a minority partition, verify no fork, then rejoin"
steps:
  - Partition: { nodes: [2, 3], duration_ticks: 1000 }
  - CheckInvariant: safety
  - Wait: { ticks: 500 }
  - CheckInvariant: liveness
  - CheckInvariant: safety

# scenarios/byzantine-double-vote.yaml
name: "Single Byzantine Double Vote"
description: "One validator sends conflicting votes"
steps:
  - Wait: { ticks: 100 }
  - DoubleVote: { node: 0, at_height: 50 }
  - CheckInvariant: safety

# scenarios/eclipse-attack.yaml
name: "Eclipse Attack on Validator"
description: "Surround target node, verify it cannot progress"
steps:
  - Eclipse: { target: 2, duration_ticks: 2000 }
  - CheckInvariant: { liveness_except_nodes: [2] }
```

**Acceptance criteria:**
- [ ] Partition scenario runs and triggers safety invariant violation on intentionally broken consensus fixture
- [ ] Scenarios serialize to JSON → included in Evidence Pack
- [ ] Same JSON + same seed → identical trace output

---

### Task 4.4 — Global Invariant Monitor

**Crate:** `crates/engine-distributed` → `src/invariants/`

```rust
#[async_trait]
pub trait Invariant: Send + Sync {
    fn id(&self) -> InvariantId;
    fn name(&self) -> &str;
    /// Called after each scenario step; returns violation if found.
    async fn check(&self, state: &SimulationState) -> Option<InvariantViolation>;
}

// Built-in invariants
pub struct SafetyInvariant;               // No two nodes commit different values at same height
pub struct LivenessInvariant;             // Progress within N ticks
pub struct IdempotencyInvariant;          // Same message_id not processed twice
pub struct EscapeHatchInvariant;          // Forced withdrawal reachable within N ticks
pub struct ProverConsistencyInvariant;    // Multi-prover agreement on state root
pub struct FinalizationMonotonicityInvariant; // Finalized roots never revoked

pub struct InvariantViolation {
    pub invariant_id: InvariantId,
    pub description: String,
    pub violated_at_tick: u64,
    pub involved_nodes: Vec<NodeId>,
    pub evidence: ViolationEvidence,
}

pub struct ViolationEvidence {
    pub seed: u64,
    pub event_trace: Vec<SimEvent>,                    // full event log up to violation
    pub node_states: HashMap<NodeId, NodeStateSnapshot>,
}

pub struct GlobalInvariantMonitor {
    pub invariants: Vec<Box<dyn Invariant>>,
}

impl GlobalInvariantMonitor {
    /// Wire in domain-specific invariants from the user's `invariants.yaml`.
    /// Called by the distributed engine after intake produces OptionalInputs.
    pub fn with_custom_invariants(mut self, invariants: &[CustomInvariant]) -> Self {
        for inv in invariants {
            self.invariants.push(Box::new(CustomInvariantChecker::new(inv.clone())));
        }
        self
    }
}
```

**Acceptance criteria:**
- [ ] `SafetyInvariant` fires when two nodes commit different blocks at same height (synthetic fixture)
- [ ] `EscapeHatchInvariant` fires when forced withdrawal is blocked for > N ticks
- [ ] Custom invariants from `invariants.yaml` fire correctly in simulation
- [ ] Violation evidence is sufficient to reproduce the issue with `reproduce.sh`

---

### Task 4.5 — Trace Capture & Seed Fixation

**Crate:** `crates/engine-distributed` → `src/trace.rs`

```rust
pub struct TraceCapture {
    pub seed: u64,
    pub events: Vec<SimEvent>,
    pub duration_ticks: u64,
}

pub struct SimEvent {
    pub tick: u64,
    pub kind: EventKind,    // MessageSent | MessageReceived | NodeCrash | InvariantCheck | ...
    pub node: NodeId,
    pub payload: serde_json::Value,
}

impl TraceCapture {
    /// Serialize to `evidence_pack/{id}/traces/trace.json`
    pub fn to_json(&self) -> String;

    /// Generate `replay.sh`: madsim run with fixed seed and pinned container digest
    pub fn to_replay_script(&self, harness_path: &Path, container_image: &str) -> String;

    /// Shrink: find minimal sub-trace that still triggers the violation.
    /// Pure template rendering — no LLM involved.
    pub fn shrink(&self, violation_tick: u64) -> TraceCapture;

    /// Generate a MadSim `#[test]` function from the captured seed + scenario script.
    /// Pure template rendering — no LLM involved. Used for regression-tests/madsim_scenarios/.
    pub fn to_regression_test(&self, test_name: &str) -> String;
}
```

**Acceptance criteria:**
- [ ] Two runs with same seed produce byte-identical `trace.json`
- [ ] `shrink` reduces a 10,000-event trace to < 50 events while preserving violation
- [ ] `replay.sh` re-triggers violation in a fresh container
- [ ] `to_regression_test` produces a compilable MadSim test function with no LLM
- [ ] All distributed findings have `verification_status: Verified` (MadSim trace) or `Unverified` (Level C black-box)
- [ ] `replay.sh` in Evidence Pack uses exact container digest from the run that found the bug

---

### Phase 4 Integration Test

**Target:** A minimal HotStuff or Tendermint implementation in Rust.

```
Expected:
- MadSim feasibility → LevelA
- Partition scenario → liveness violation (node can't progress while isolated)
- Byzantine double-vote scenario → safety invariant fires
- Full trace captured with seed; reproduce.sh re-triggers in < 2 minutes
- shrink() reduces trace to < 50 events
```

---

## Phase 5 — L2 Specifics, Reports & Productization (Week 14–17)

**Inputs consumed (final optional inputs wired):**
- `OptionalInputs.previous_audit` fully integrated: prior findings marked `regression_check: true`
- All optional inputs reflected in `audit-manifest.json`

**Outputs completed in this phase:**
- All 6 output types fully implemented and tested
- `audit-manifest.json` complete
- Tauri UI with full setup wizard

---

### Task 5.1 — Multi-Prover Trust Boundary Module + Economic Attack Checker

*(Multi-Prover interfaces unchanged from v1.)*

#### Economic Attack Checker — Corrected Architecture

The economic attack module is **not** LLM-driven reasoning. It is a curated YAML checklist of deterministic code patterns maintained by domain experts. LLM's only role (Role 3) is generating the human-readable description for each matched pattern — the *existence* of a finding comes from whether the code pattern is present or absent.

```rust
// crates/engine-distributed/src/economic/checker.rs

pub struct EconomicAttackChecker {
    checklist: Vec<EconomicAttackVector>,    // loaded from rules/economic/*.yaml
    llm: Option<Arc<dyn LlmProvider>>,       // Role 3 only: description prose
}

/// A single attack vector from the curated YAML library.
/// Written and maintained by domain experts, not generated by LLM.
pub struct EconomicAttackVector {
    pub id: String,                 // "ECON-001"
    pub name: String,
    pub category: EconCategory,     // Sequencer | Prover | Sybil | Bridge
    pub detection: EconDetection,   // the deterministic code check
    pub severity: Severity,         // always Observation for economic findings
    pub spec_refs: Vec<String>,     // EIP/whitepaper references
}

pub enum EconDetection {
    /// A named function/method call (or struct field access) is absent in all in-scope code.
    /// Checked via tree-sitter function call pattern — not free-text string match.
    /// False-positive guard: also checks that the crate is actually compiled (not dead code).
    CallSiteAbsent {
        /// Fully-qualified path segments to match (all must be absent for trigger).
        /// Examples: ["FairSequencer::sort_txs"], ["InclusionPolicy::check"]
        fn_patterns: Vec<String>,
        description: String,
    },
    /// A specific call site is present — confirms a known-bad pattern.
    CallSitePresent {
        fn_patterns: Vec<String>,
        description: String,
    },
    /// A struct field or config key is absent or has no value bound.
    /// Checked via tree-sitter struct literal / serde field scan.
    StructFieldAbsent { struct_name: String, field: String, description: String },
    /// Config constant or static is missing or below a threshold.
    ConfigBoundCheck { const_name: String, required_bound: ConfigBound, description: String },
}

impl EconomicAttackChecker {
    pub async fn analyze(
        &self,
        workspace: &CargoWorkspace,
        semantic_index: &SemanticIndex,
    ) -> Vec<Finding> {
        let mut findings = vec![];
        for vector in &self.checklist {
            // Step 1: run deterministic code check
            let check_result = self.run_detection(&vector.detection, workspace, semantic_index);

            if check_result.triggered() {
                // Step 2 (Role 3, optional): LLM writes human-readable description
                // The finding EXISTS because the check triggered, not because LLM said so
                let description = if let Some(llm) = &self.llm {
                    llm_call(llm, LlmRole::ProseRendering,
                        &self.description_prompt(vector, &check_result), &Default::default())
                        .await.unwrap_or_else(|_| vector.detection.default_description())
                } else {
                    vector.detection.default_description()
                };

                findings.push(Finding {
                    id: FindingId::new("ECON"),
                    severity: Severity::Observation,
                    verification_status: VerificationStatus::Unverified {
                        reason: "Economic attack analysis — no formal proof. \
                                 Requires manual protocol review.".into(),
                    },
                    llm_generated: false,        // finding exists from code check
                    // description came from LLM prose but finding is from code pattern
                    ..self.build_finding(vector, &check_result, description)
                });
            }
        }
        findings
    }
}
```

**Curated attack vector checklist (YAML, minimum for Phase 5):**

> **Detection note:** All `CallSiteAbsent` checks match function/method call patterns via tree-sitter, not free-text string search. This prevents false negatives from dead-code strings and false positives from comment matches.

```yaml
# rules/economic/sequencer.yaml
vectors:
  - id: ECON-001
    name: "No transaction ordering enforcement call"
    category: Sequencer
    detection:
      type: CallSiteAbsent
      # Matches: FairSequencer::sort_txs(), PriorityQueue::order_by_fee(), etc.
      fn_patterns:
        - "FairSequencer::sort"
        - "order_by_priority_fee"
        - "fair_sequencing::sort"
        - "CommitBoost::submit"
      description: "No call site found that enforces transaction ordering policy"
    severity: Observation
    spec_refs: ["EIP-1559 §2", "https://eips.ethereum.org/EIPS/eip-1559"]

  - id: ECON-002
    name: "No inclusion policy enforcement call"
    category: Sequencer
    detection:
      type: CallSiteAbsent
      fn_patterns:
        - "InclusionPolicy::check"
        - "CensorshipGuard::allow"
        - "TxFilter::is_allowed"
      description: "No call site found that enforces transaction inclusion policy"
    severity: Observation
    spec_refs: []

  - id: ECON-003
    name: "No maximum batch delay constant"
    category: Sequencer
    detection:
      type: ConfigBoundCheck
      const_name: "MAX_BATCH_DELAY"    # checks for const/static with this name pattern
      required_bound: { exists: true }
      description: "No MAX_BATCH_DELAY constant found; sequencer may delay batches indefinitely"
    severity: Observation
    spec_refs: []
```

**Key I/O addition:** `spec_refs` in findings are populated from `OptionalInputs.spec_constraints` when available; otherwise from the static YAML refs.

**Acceptance criteria:**
- [ ] All economic findings have `verification_status: Unverified` and `severity: Observation`
- [ ] Finding existence is determined by the code check result, not LLM output
- [ ] LLM absent → `default_description()` used; findings still produced
- [ ] Minimum 6 vectors in YAML checklist covering Sequencer, Prover, and Sybil categories
- [ ] Technical report clearly labels economic findings as "Unverified — requires manual protocol review"

---

### Task 5.2 — Three-Layer Report Generator + Regression Tests (Final)

The report generator is **template-driven**. All finding content comes from structured `Finding` fields. LLM's only involvement (Role 3, optional) is improving prose readability of the `recommendation` and `impact` fields — it never generates what to include.

```rust
// crates/report/src/lib.rs

pub struct ReportGenerator {
    findings: Vec<Finding>,
    config: Arc<AuditConfig>,
    evidence_store: Arc<EvidenceStore>,
    llm: Option<Arc<dyn LlmProvider>>,  // Role 3 only; None → raw field text used as-is
}

impl ReportGenerator {
    pub async fn generate_all(&self, output_dir: &Path) -> Result<()> {
        // All outputs generated from structured Finding fields — no LLM in the critical path
        self.write_executive_report(output_dir).await?;   // pure arithmetic + template
        self.write_technical_report(output_dir).await?;   // structured fields + optional prose polish
        self.write_findings_json(output_dir).await?;
        self.write_sarif(output_dir).await?;
        self.export_evidence_pack(output_dir).await?;
        self.write_regression_tests(output_dir).await?;
        self.write_audit_manifest(output_dir).await?;
        Ok(())
    }

    /// Executive summary: pure arithmetic + fixed template. Zero LLM.
    async fn write_executive_report(&self, dir: &Path) -> Result<()> {
        let counts = FindingCounts::from(&self.findings);
        let score = counts.risk_score();
        let band = RiskBand::from_score(score);
        // Top 5 findings selected by severity — deterministic sort, no LLM ranking
        let top5 = self.findings.iter()
            .filter(|f| !matches!(f.severity, Severity::Observation))
            .take(5)
            .collect::<Vec<_>>();
        // Template renders score, band, counts, top5 summaries from Finding.impact field
        render_executive_template(score, band, &counts, &top5, dir)
    }

    /// Technical report: structured fields. LLM Role 3 optionally polishes
    /// recommendation and impact text for readability — does not change content.
    async fn polish_prose_if_available(&self, text: &str) -> String {
        let Some(llm) = &self.llm else { return text.to_string(); };
        llm_call(llm, LlmRole::ProseRendering,
            &format!("Improve the readability of this security recommendation. \
                      Do NOT change the technical content, severity, or any code. \
                      Output only the improved text:\n\n{text}"),
            &CompletionOpts { temperature: 0.2, ..Default::default() })
            .await.unwrap_or_else(|_| text.to_string())
    }
}
```

**Verification status in reports:**

```markdown
<!-- Technical report rendering per finding -->
## F-ZK-0042 — Field element deserialization without canonicality check
**Severity:** High  |  **Framework:** Halo2  |  **Status:** ✅ Verified
> Backed by Kani counterexample. Reproducible: `bash evidence-pack/F-ZK-0042/reproduce.sh`

## ECON-001 — No transaction ordering constraint
**Severity:** Observation  |  **Framework:** Static  |  **Status:** ⚠ Unverified
> Pattern-based analysis. No formal proof. Requires manual protocol review.
```

**Risk score (unchanged):**
```
score = 100 − (Critical×25) − (High×15) − (Medium×5) − (Low×2)  [floor 0]
Observation findings do not affect the score.
≥70 → Deploy   50–69 → Fix before deploy   <50 → Do not deploy
```

> **Score limitation:** The additive formula loses discrimination at the low end — 4 Criticals and 1 Critical + 15 Highs both clamp to 0 ("Do not deploy"). The score is an **orientation signal**, not a precise risk rank. The executive summary must note this and direct auditors to the finding count table for full severity breakdown. Do not use the raw number for automated go/no-go decisions beyond the three-band gate.

**Acceptance criteria:**
- [ ] Executive summary generated with zero LLM calls when `llm: None`
- [ ] `Verified` / `Unverified` labels present on every finding in technical report
- [ ] LLM prose polish can be disabled via `--no-llm-prose` flag; report still complete
- [ ] Economic/Observation findings clearly labeled "Requires manual protocol review"
- [ ] Executive summary ≤ 2 pages as PDF
- [ ] SARIF validates against 2.1.0 schema; `findings.json` validates against schema
- [ ] Regression test file compiles with `cargo build` against a test workspace
- [ ] `audit-manifest.json` records whether LLM prose polish was used

---

### Task 5.3 — Diff-Mode Incremental Pipeline

**New input path for diff-mode:**
```bash
audit-agent diff \
  --base-commit a1b2c3d4 \
  --head-commit e5f6a7b8 \
  --config audit.yaml \
  --output-dir ./audit-output-diff
```

```rust
pub struct DiffModeAnalyzer { cache: Arc<AnalysisCache> }

impl DiffModeAnalyzer {
    pub fn compute_diff(&self, base: &str, head: &str) -> DiffAnalysis;
}

pub struct DiffAnalysis {
    pub base_commit: String,
    pub head_commit: String,
    pub affected_crates: Vec<String>,
    pub full_rerun_required: bool,    // if Cargo.toml or features changed
    pub rerun_tasks: Vec<TaskId>,
    pub cached_findings: Vec<Finding>, // from unchanged modules
}
```

**Acceptance criteria:**
- [ ] PR changing 2 files in a 50-file workspace → only those 2 files re-analyzed
- [ ] `Cargo.toml` change → full rerun triggered
- [ ] Cache hit rate > 80% on a 2-file change PR
- [ ] Output report clearly labels which findings are from cache vs. new analysis

---

### Task 5.4 — Tauri UI: Full Setup Wizard + Results View

**UI flow (maps exactly to the I/O contract Tier 1–3):**

```
Step 1: Source
  ├── Tab: Git URL  →  [URL input] + [Commit SHA input] (branch auto-resolves with warning)
  ├── Tab: Local Path  →  [path picker] + [auto-detected commit display]
  └── Tab: Upload Archive  →  [file drop zone] (.tar.gz / .zip)

Step 2: Configuration
  ├── Upload audit.yaml  OR  fill form (scope, engines, budget)
  └── [Download generated audit.yaml] button

Step 3: Optional Inputs
  ├── Spec document  →  [PDF/MD upload, optional]
  ├── Previous audit →  [PDF/MD upload, optional]
  ├── Custom invariants → [YAML upload, optional]
  └── LLM API key  →  [password input, optional — shows degraded features if absent]

Step 4: Workspace Confirmation  (Tier-3 — from ConfirmationSummary JSON)
  ├── Crate list with in-scope / excluded / ambiguous decisions
  ├── Detected frameworks
  ├── Build matrix with estimated time
  ├── Any IntakeWarning banners (branch resolved, dirty tree, etc.)
  ├── [Confirm and Start] button
  └── [Export audit.yaml] button  →  saves fully resolved config

Step 5: Live Execution  (during audit)
  ├── DAG view with real-time node states
  ├── Live finding count by severity (updates as findings arrive)
  └── Log stream per DAG node (click to expand)

Step 6: Results
  ├── Finding list (filterable by severity / framework / category)
  │   └── LLM Generated badge (orange) on applicable findings
  ├── Evidence panel (per finding: all files + reproduce.sh preview)
  ├── CDG visualization (Halo2 findings)
  ├── MadSim trace viewer (distributed findings)
  └── Export panel:
      ├── [Download Executive Report PDF]
      ├── [Download Technical Report PDF]
      ├── [Download Evidence Pack ZIP]
      ├── [Download findings.sarif]
      ├── [Download findings.json]
      └── [Download Regression Tests ZIP]
```

**New IPC commands (additions over v1):**
```typescript
// intake flow
export const resolveSource    = (input: SourceInput): Promise<ResolvedSource>     => invoke(...)
export const parseConfig      = (path: string): Promise<ValidatedConfig | ConfigErrors> => invoke(...)
export const detectWorkspace  = (source: ResolvedSource): Promise<ConfirmationSummary> => invoke(...)
export const confirmWorkspace = (decisions: UserDecisions): Promise<AuditConfig>  => invoke(...)
export const exportAuditYaml  = (config: AuditConfig, path: string): Promise<void> => invoke(...)

// results
export const getAuditManifest = (auditId: string): Promise<AuditManifest>         => invoke(...)
export const downloadOutput   = (auditId: string, type: OutputType, dest: string): Promise<void> => invoke(...)
```

**Acceptance criteria:**
- [ ] Branch name in Step 1 shows visible warning: "Resolved to SHA abc123 — audit is pinned to this commit"
- [ ] Workspace confirmation renders all `CrateDecision` types with correct styling
- [ ] `IntakeWarning.LlmKeyMissing` shows which features are degraded
- [ ] "Export audit.yaml" in Step 4 produces a valid YAML that can be fed back to the CLI
- [ ] All 6 output types downloadable from Step 6 results panel
- [ ] Evidence panel shows `reproduce.sh` contents inline with copy button

---

## Cross-Phase: DAG Orchestrator

**Crate:** `crates/orchestrator`

```rust
pub struct AuditOrchestrator {
    engines: Vec<Box<dyn AuditEngine>>,
    sandbox: Arc<SandboxExecutor>,
    evidence_store: Arc<EvidenceStore>,
    findings_db: Arc<FindingsDb>,
    cache: Arc<AnalysisCache>,
    output_dir: PathBuf,
}

impl AuditOrchestrator {
    // Entry point — called after intake produces AuditConfig
    pub async fn run(&self, config: &AuditConfig) -> Result<AuditOutputs>;
    
    fn build_dag(&self, config: &AuditConfig) -> AuditDag;
    async fn execute_dag(&self, dag: &AuditDag) -> Vec<Finding>;
    
    // After all findings collected: run all output writers
    async fn produce_outputs(&self, findings: &[Finding], config: &AuditConfig) -> Result<AuditOutputs>;
}
```

**`produce_outputs` sequence:**
```
1. findings_db.deduplicate(findings)
   Deduplication key: (rule_id, file, line_range_start)
     - Same rule firing on different call sites at different lines → kept as separate findings
     - Same rule + same file + same line_range_start → merged; evidence from highest-confidence run retained
     - Kani and rule-match findings for the same location → kept separately (different category/evidence)
2. findings_db.mark_regression_checks(findings, prev_audit)  ← uses OptionalInputs
3. report_generator.generate_all(output_dir)
   → report-executive.md + .pdf
   → report-technical.md + .pdf
   → findings.json
   → findings.sarif
   → evidence-pack.zip
   → regression-tests/
   → audit-manifest.json
4. emit AuditCompleted event to UI
```

---

## Testing Strategy

### Fixtures (required before any integration test)

```
tests/fixtures/
├── circom/
│   ├── underconstrained_lessthan.circom    # known under-constrained from QED2 paper
│   └── missing_range_check.circom
├── halo2/
│   ├── isolated_chip/                       # chip with unconstrained column
│   └── selector_conflict/
├── rust-crypto/
│   ├── nonce_reuse/                         # CRYPTO-001
│   ├── missing_domain_sep/                  # CRYPTO-002
│   ├── weak_rng/                            # CRYPTO-004
│   └── hardcoded_key/                       # CRYPTO-007
├── distributed/
│   ├── unsafe_bft/                          # safety violation on partition
│   └── unbounded_queue/                     # DoS via unbounded queue
└── audit-yamls/
    ├── valid-full.yaml                       # all fields
    ├── valid-minimal.yaml                    # required fields only
    ├── invalid-branch-not-sha.yaml           # should fail validation
    └── invalid-missing-commit.yaml           # should fail validation
```

### End-to-End Test Matrix

| Test | Input | Expected output |
|------|-------|-----------------|
| Phase 1 smoke | `halo2` repo + `audit-yamls/valid-minimal.yaml` | ≥1 CRYPTO-* finding; all 6 output files exist |
| Phase 1 SARIF | any Phase 1 run | `findings.sarif` validates against SARIF 2.1.0 schema |
| Phase 1 evidence | any Phase 1 finding | `reproduce.sh` re-triggers the match in a fresh container |
| Phase 2 Circom | `circomlib` repo | ≥1 UnderConstrained finding; Z3 counterexample in smt2/ |
| Phase 2 Kani | `rust-crypto/nonce_reuse` fixture | Kani counterexample captured; `evidence_gate_level: 3` |
| Phase 3 CDG | `halo2-gadgets` repo | CDG built; ≥1 risk annotation; DOT renders |
| Phase 4 MadSim | `distributed/unsafe_bft` fixture | Safety invariant violation; trace captures seed |
| Phase 4 replay | output of above | `replay.sh` re-triggers in < 2 min in fresh container |
| Phase 5 report | any completed audit | All 6 outputs; `audit-manifest.json` valid JSON |
| Phase 5 diff | 2-file PR on any repo | Cache hit rate > 80%; output marks cached vs. new |
| Intake: branch | Git URL + branch name | Warning emitted; SHA shown; user must confirm |
| Intake: archive | `.tar.gz` of a ZK repo | Valid `AuditConfig` produced; content hash set |
| Intake: prev audit | audit with `--prev-audit` | Prior findings marked `regression_check: true` |

---

## Definition of Done (Per Phase)

| Phase | Done When |
|-------|-----------|
| **Phase 0** | All containers build; core types serde round-trip; intake resolves all 3 source types; sandbox enforces timeout + memory; evidence pack saves + reproduces |
| **Phase 1** | 8 crypto rules fire on fixtures; SARIF + JSON valid; evidence pack reproducible; Technical report has inline reproduce commands |
| **Phase 2** | Circom under-constrained finds known vuln; Kani harness passes Evidence Gate Level 3; LLM absent → TemplateFallback used silently |
| **Phase 3** | CDG built for halo2-gadgets with risk annotations; Halo2 SMT finds synthetic vuln; SP1 diff tester detects divergence |
| **Phase 4** | MadSim feasibility classifies 3 projects; partition scenario triggers safety; trace replay works; custom invariants from YAML fire |
| **Phase 5** | All 6 outputs produced and validated; previous audit marks regression checks; diff-mode >80% cache hit; Tauri setup wizard completes end-to-end |

---

## Appendix — Key Dependencies

```toml
# Cargo.toml [workspace.dependencies]
tokio          = { version = "1",     features = ["full"] }
bollard        = "0.17"               # Docker Engine API
serde          = { version = "1",     features = ["derive"] }
serde_json     = "1"
serde_yaml     = "0.9"
clap           = { version = "4",     features = ["derive"] }
anyhow         = "1"
thiserror      = "1"
async-trait    = "0.1"
tree-sitter    = "0.22"
tree-sitter-rust = "0.21"
num-bigint     = "0.4"
chrono         = { version = "0.4",   features = ["serde"] }
sled           = "0.34"               # embedded cache DB
zip            = "2"
tracing        = "0.1"
tracing-subscriber = "0.3"
reqwest        = { version = "0.12",  features = ["json"] }  # LLM API calls
pdf-extract    = "0.7"               # spec PDF parsing
jsonschema     = "0.18"              # schema validation in tests
git2           = "0.19"              # git clone / rev-parse

# rust-analyzer (pin exact version — these are unstable)
ra_ap_ide      = "=0.0.239"
ra_ap_hir      = "=0.0.239"
ra_ap_syntax   = "=0.0.239"

# Tauri
tauri          = { version = "2",     features = ["shell-open"] }
```

```toml
# containers/versions.toml
[tools]
kani                 = "0.57.0"
kani_rustc_toolchain = "nightly-2024-11-01"
z3                   = "4.13.0"
madsim               = "0.2.30"
miri_rustc_toolchain = "nightly-2024-11-01"
cargo_fuzz           = "0.12.0"
cargo_audit          = "0.21.0"
circom               = "2.1.9"
```

---

*Implementation Plan v2.0 | Based on System Design v2.0 | I/O contract fully integrated | Total: ~18–22 weeks*
