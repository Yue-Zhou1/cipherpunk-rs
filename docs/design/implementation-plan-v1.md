# Blockchain Security Audit Agent — Implementation Plan

> **Based on:** System Design v2.0  
> **Target readers:** Engineers starting implementation  
> **Total estimated duration:** 17–21 weeks across 5 phases  
> **Stack:** Rust (orchestrator, engines, CLI), TypeScript/React (Tauri UI), Python (tooling glue scripts)

---

## How to Use This Document

Each phase maps to a design section. Each task has:
- A clear **scope** (what to build)
- **File/module layout** (where it lives)
- **Key interfaces** (types and traits to define first)
- **Acceptance criteria** (how you know it's done)
- **Blocking dependencies** (what must exist before starting)

Work within a phase can be parallelized across engineers. Do **not** start Phase N+1 until all blocking items in Phase N are checked off.

---

## Repository Structure (Target)

```
audit-agent/
├── Cargo.toml                        # workspace root
├── crates/
│   ├── core/                         # shared types, traits, evidence schema
│   ├── orchestrator/                 # DAG engine, task scheduler, cache
│   ├── sandbox/                      # Docker executor abstraction
│   ├── evidence/                     # Evidence Store, pack builder, manifest
│   ├── findings/                     # Findings DB, dedup, SARIF export
│   ├── llm/                          # LLM provider adapters + Evidence Gate
│   ├── engine-crypto/                # Crypto & ZK audit engine
│   ├── engine-distributed/           # Distributed consensus audit engine
│   └── report/                       # Three-layer report generator
├── ui/                               # Tauri + React frontend
│   ├── src-tauri/                    # Tauri backend (IPC bridge)
│   └── src/                          # React components
├── containers/
│   ├── kani/                         # Dockerfile for Kani + rustc
│   ├── z3/                           # Dockerfile for Z3
│   ├── miri/                         # Dockerfile for Miri + Sanitizers
│   ├── madsim/                       # Dockerfile for MadSim harness runner
│   └── fuzz/                         # Dockerfile for cargo-fuzz
├── rules/
│   ├── crypto-misuse/                # YAML rule definitions
│   └── distributed/                  # Distributed invariant definitions
├── scripts/                          # Python tooling glue (cargo-audit correlation, etc.)
├── tests/
│   ├── fixtures/                     # Test ZK crates, P2P projects
│   └── integration/                  # End-to-end audit pipeline tests
└── docs/
    ├── design-v2.md                  # System design reference
    └── adr/                          # Architecture Decision Records
```

---

## Phase 0 — Foundation (Week 1–2)

**Goal:** Establish shared types, traits, container infrastructure, and CI. Everything else builds on this. No business logic here.

### Task 0.1 — Core Types & Evidence Schema

**Crate:** `crates/core`

Define all shared data types that every other crate will import. Getting these right early prevents painful refactors later.

```rust
// crates/core/src/finding.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub id: FindingId,               // "F-ZK-0042" format
    pub title: String,
    pub severity: Severity,
    pub category: FindingCategory,
    pub framework: Framework,
    pub affected_components: Vec<CodeLocation>,
    pub prerequisites: String,
    pub exploit_path: String,
    pub impact: String,
    pub evidence: Evidence,
    pub evidence_gate_level: u8,     // 0-3
    pub llm_generated: bool,
    pub recommendation: String,
    pub regression_test: Option<String>,
    pub status: FindingStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Observation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Framework {
    Halo2,
    Circom,
    SP1,
    RISC0,
    MadSim,
    Loom,
    Static,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FindingCategory {
    UnderConstrained,
    SpecMismatch,
    CryptoMisuse,
    Replay,
    DoS,
    Race,
    Incentive,
    UnsafeUB,
    SideChannel,
    SupplyChain,
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
}
```

```rust
// crates/core/src/audit_config.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    pub repo_path: PathBuf,
    pub commit_hash: String,
    pub feature_sets: Vec<Vec<String>>,    // build matrix
    pub target_triples: Vec<String>,
    pub scope: AuditScope,
    pub budget: BudgetConfig,
    pub llm: LlmConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub kani_timeout_secs: u64,            // default: 300
    pub z3_timeout_secs: u64,             // default: 600
    pub fuzz_duration_secs: u64,          // default: 3600
    pub madsim_ticks: u64,                // default: 100_000
    pub max_retries: u8,                  // default: 3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditScope {
    Full,
    CryptoOnly,
    DistributedOnly,
    Diff { base_commit: String },
}
```

**Key trait to define:**

```rust
// crates/core/src/engine.rs
#[async_trait]
pub trait AuditEngine: Send + Sync {
    fn name(&self) -> &str;
    async fn analyze(&self, ctx: &AuditContext) -> Result<Vec<Finding>>;
    async fn supports(&self, ctx: &AuditContext) -> bool;
}
```

**Acceptance criteria:**
- [ ] All types compile with `serde` round-trip tests passing
- [ ] `Finding` JSON schema exported and committed to `docs/finding-schema.json`
- [ ] `AuditConfig` can be loaded from YAML file

---

### Task 0.2 — Container Infrastructure

**Directory:** `containers/`

Build versioned Docker images for every heavy tool. These are the most critical reproducibility artifacts.

```dockerfile
# containers/kani/Dockerfile
FROM rust:1.82.0-slim-bookworm

# Pin exact versions — DO NOT use "latest"
RUN cargo install kani-verifier --version 0.57.0 --locked
RUN rustup toolchain install nightly-2024-11-01

# Metadata for Evidence Pack
LABEL tool.kani.version="0.57.0"
LABEL tool.rustc.version="nightly-2024-11-01"
LABEL schema.version="1"

WORKDIR /workspace
```

```dockerfile
# containers/z3/Dockerfile  
FROM ubuntu:24.04
RUN apt-get install -y z3=4.13.0
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

**Version registry** — single source of truth:

```toml
# containers/versions.toml  — committed and never auto-updated
[tools]
kani = "0.57.0"
kani_rustc_toolchain = "nightly-2024-11-01"
z3 = "4.13.0"
madsim = "0.2.30"
miri_rustc_toolchain = "nightly-2024-11-01"
cargo_fuzz = "0.12.0"
cargo_audit = "0.21.0"
```

**Acceptance criteria:**
- [ ] All 5 images build successfully in CI
- [ ] Each image has a `--version` or equivalent smoke test command
- [ ] Image digests are printed and can be captured programmatically
- [ ] `containers/versions.toml` is the single source for all version references

---

### Task 0.3 — Sandbox Executor

**Crate:** `crates/sandbox`

Wrap Docker Engine API so every other crate can run a tool without knowing container details.

```rust
// crates/sandbox/src/lib.rs
pub struct SandboxExecutor {
    docker: Docker,               // bollard crate
    image_registry: ImageRegistry,
}

pub struct ExecutionRequest {
    pub image: ToolImage,         // Kani | Z3 | Miri | MadSim | Fuzz
    pub command: Vec<String>,
    pub mounts: Vec<Mount>,       // (host_path, container_path, readonly)
    pub env: HashMap<String, String>,
    pub budget: ResourceBudget,
    pub network: NetworkPolicy,   // Disabled | Allowlist(Vec<String>)
}

pub struct ExecutionResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub artifacts: Vec<ArtifactPath>,  // files written to /out/
    pub container_digest: String,
    pub duration_ms: u64,
    pub resource_usage: ResourceUsage,
}

pub struct ResourceBudget {
    pub cpu_quota: f64,           // cores, e.g. 2.0
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub timeout_secs: u64,
}

impl SandboxExecutor {
    pub async fn run(&self, req: ExecutionRequest) -> Result<ExecutionResult>;
    pub async fn pull_if_needed(&self, image: &ToolImage) -> Result<String>; // returns digest
}
```

**Acceptance criteria:**
- [ ] Can execute `echo hello` in a container and capture output
- [ ] Timeout enforcement works (process killed after deadline)
- [ ] Memory limit is enforced (OOM returns a typed error, not a panic)
- [ ] Container digest is captured and matches `docker inspect`
- [ ] Rootless mode works on Linux (test in CI with rootless Docker)

---

### Task 0.4 — Evidence Store

**Crate:** `crates/evidence`

```rust
pub struct EvidenceStore {
    base_dir: PathBuf,   // ~/.audit-agent/evidence/ by default
}

impl EvidenceStore {
    // Writes all files for a finding into base_dir/{finding_id}/
    pub async fn save_pack(&self, finding_id: &FindingId, pack: &EvidencePack) -> Result<()>;
    
    // Builds reproduce.sh from stored evidence
    pub async fn generate_reproduce_script(&self, finding_id: &FindingId) -> Result<String>;
    
    // Zip up the full evidence_pack/{finding_id}/ directory
    pub async fn export_zip(&self, finding_id: &FindingId, dest: &Path) -> Result<()>;
    
    pub async fn load_manifest(&self, finding_id: &FindingId) -> Result<EvidenceManifest>;
}

// Serialized to evidence_pack/{id}/manifest.json
#[derive(Serialize, Deserialize)]
pub struct EvidenceManifest {
    pub finding_id: FindingId,
    pub commit_hash: String,
    pub feature_set: Vec<String>,
    pub container_digest: String,
    pub tool_versions: HashMap<String, String>,
    pub command: String,
    pub seed: Option<String>,
    pub created_at: DateTime<Utc>,
    pub files: Vec<String>,       // relative paths of all evidence files
}
```

**Acceptance criteria:**
- [ ] Save and load round-trip for all evidence file types
- [ ] `reproduce.sh` is generated and actually runnable in a fresh container
- [ ] ZIP export passes integrity check (file count matches manifest)

---

### Task 0.5 — CI Pipeline

**File:** `.github/workflows/ci.yml`

```yaml
jobs:
  test:
    - cargo test --workspace
  container-build:
    - docker build containers/kani/
    - docker build containers/z3/
    - docker build containers/miri/
    - docker build containers/madsim/
    - docker build containers/fuzz/
  schema-check:
    - validate docs/finding-schema.json against crates/core types
```

**Acceptance criteria:**
- [ ] All jobs green on `main`
- [ ] Container builds are cached between runs (layer cache)
- [ ] Schema validation fails if `Finding` type changes without updating the schema doc

---

## Phase 1 — Crypto API Misuse Detection (Week 3–5)

**Goal:** Ship the fastest differentiator — a rule-based scanner for cryptographic API misuse in Rust. No LLM, no formal methods. Pure static analysis with tree-sitter + semantic rules.

### Task 1.1 — Repo Intake & Build Matrix

**Crate:** `crates/engine-crypto` → `src/intake.rs`

```rust
pub struct RepoIntake {
    pub workspace: CargoWorkspace,
    pub dependency_graph: DependencyGraph,
    pub build_matrix: Vec<BuildVariant>,
    pub environment_manifest: EnvironmentManifest,
}

pub struct BuildVariant {
    pub features: Vec<String>,
    pub target_triple: String,
    pub toolchain: String,
}

pub struct EnvironmentManifest {
    pub rust_toolchain: String,      // from rust-toolchain.toml
    pub cargo_lock_hash: String,     // sha256 of Cargo.lock
    pub workspace_root: PathBuf,
}

impl RepoIntake {
    // Parses workspace, detects all crates, generates build matrix
    pub async fn from_path(path: &Path, config: &AuditConfig) -> Result<Self>;
    
    // Returns the subset of variants that affect crypto code paths
    pub fn crypto_relevant_variants(&self) -> Vec<&BuildVariant>;
}
```

**Acceptance criteria:**
- [ ] Correctly parses a Cargo workspace with 10+ crates
- [ ] Detects `#[cfg(feature = "...")]` flags that change crypto implementations
- [ ] `EnvironmentManifest` is written to evidence output

---

### Task 1.2 — Crypto Misuse Rule Engine

**Crate:** `crates/engine-crypto` → `src/rules/`

This is the highest-ROI module. Define rules in YAML, evaluate against tree-sitter AST + semantic index.

**Rule schema:**

```yaml
# rules/crypto-misuse/nonce-reuse.yaml
id: CRYPTO-001
title: "Potential nonce reuse in encryption context"
severity: High
category: CryptoMisuse
description: |
  Detected a nonce/IV derived from a constant or counter without
  domain-binding. Nonce reuse in AEAD schemes breaks confidentiality.
detection:
  patterns:
    - type: function_call
      name_matches: ["encrypt", "seal", "aead_encrypt"]
      argument_pattern:
        position: 1   # nonce argument
        matches_any:
          - type: literal        # constant nonce
          - type: counter_expr   # simple counter without domain tag
  semantic_checks:
    - nonce_is_not_bound_to_session_id
    - nonce_not_hashed_with_domain_separator
references:
  - "https://eprint.iacr.org/2016/475"
remediation: |
  Derive nonces as HKDF(session_key, domain_separator, counter).
  Never use a raw counter or constant as a nonce.
```

```yaml
# rules/crypto-misuse/domain-separation.yaml
id: CRYPTO-002
title: "Missing domain separation in hash/transcript"
severity: High
category: CryptoMisuse
detection:
  patterns:
    - type: method_call
      receiver_type_matches: ["Transcript", "Hasher", "Sha256", "Blake2b"]
      method_matches: ["update", "absorb", "write"]
      context_check:
        no_preceding_domain_tag_within: 5_lines
```

```yaml
# rules/crypto-misuse/encoding-non-canonical.yaml
id: CRYPTO-003
title: "Field element deserialization without canonicality check"
severity: High
category: CryptoMisuse
detection:
  patterns:
    - type: function_call
      name_matches: ["from_bytes", "from_repr", "deserialize"]
      return_type_matches: ["FieldElement", "Fp", "Fr", "Scalar"]
      missing_check:
        - method: "is_some"
          or_pattern: "?"         # Option unwrap without check
```

```yaml
# rules/crypto-misuse/rng-misuse.yaml
id: CRYPTO-004
title: "Weak or deterministic RNG used in cryptographic context"
severity: Critical
category: CryptoMisuse
detection:
  patterns:
    - type: function_call
      name_matches: ["StdRng::seed_from_u64", "SmallRng::new", "thread_rng"]  
      context: crypto_critical_path   # in scope of prove/sign/keygen
```

**Rule evaluator:**

```rust
// crates/engine-crypto/src/rules/evaluator.rs
pub struct RuleEvaluator {
    rules: Vec<CryptoMisuseRule>,
    parser: tree_sitter::Parser,
}

impl RuleEvaluator {
    pub async fn evaluate(&self, file: &SourceFile) -> Vec<RuleMatch>;
}

pub struct RuleMatch {
    pub rule_id: String,
    pub location: CodeLocation,
    pub matched_snippet: String,
    pub confidence: Confidence,   // High | Medium | Low
}
```

**Minimum rule set for Phase 1:**

| ID | Rule | Severity |
|----|------|----------|
| CRYPTO-001 | Nonce reuse / missing domain binding | High |
| CRYPTO-002 | Missing domain separator in transcript/hash | High |
| CRYPTO-003 | Field element deserialization without canonicality check | High |
| CRYPTO-004 | Weak RNG in crypto-critical path | Critical |
| CRYPTO-005 | Missing point validation (small-subgroup check) | High |
| CRYPTO-006 | Unchecked `unwrap()` on crypto result type | Medium |
| CRYPTO-007 | Hardcoded cryptographic constant (key/seed) | Critical |
| CRYPTO-008 | `unsafe` block in signature verification path | Medium |

**Acceptance criteria:**
- [ ] All 8 rules implemented and tested against synthetic fixtures
- [ ] Rules load from YAML without recompiling (hot-reload in dev mode)
- [ ] Each match produces a `CodeLocation` with exact file + line range
- [ ] False positive rate < 20% on `halo2/src` and `arkworks/src` (manual spot-check)

---

### Task 1.3 — cargo-audit Call-Path Correlation (Closes G10)

**Crate:** `crates/engine-crypto` → `src/supply_chain.rs`

```rust
pub struct SupplyChainAnalyzer {
    call_graph: CallGraph,    // from rust-analyzer
}

pub struct CveCallPathResult {
    pub cve_id: String,
    pub affected_fn: String,
    pub crate_name: String,
    pub reachable_from_crypto_path: bool,
    pub call_chain: Vec<String>,   // fn names from crypto entry → CVE fn
    pub upgraded_severity: Severity,
}

impl SupplyChainAnalyzer {
    // Run cargo-audit, then for each CVE, check if affected fn is
    // reachable from a crypto-critical call path
    pub async fn analyze(&self, workspace: &CargoWorkspace) -> Result<Vec<CveCallPathResult>>;
}
```

**Severity escalation logic:**

```
cargo-audit reports CVE in crate X
  → check if any fn in crate X is in crypto call graph
    → not reachable from verify/prove/sign/keygen  →  keep original severity (usually Low)  
    → reachable from verify/prove/sign/keygen      →  escalate to High
    → reachable AND fn is in direct hot path        →  escalate to Critical
```

**Acceptance criteria:**
- [ ] Correctly identifies a known CVE in `curve25519-dalek` as reachable from a signing function
- [ ] Correctly downgrades a CVE in a dev-dependency to Low
- [ ] Output includes the full call chain for auditor review

---

### Task 1.4 — Evidence Pack Schema v1 + Export

**Crate:** `crates/evidence`

Implement the file layout defined in the design doc. This is used by all subsequent phases.

```
evidence_pack/{finding_id}/
├── manifest.json
├── harness/src/lib.rs        (Phase 2+)
├── harness/Cargo.toml        (Phase 2+)
├── smt2/query.smt2           (Phase 2+)
├── smt2/output.txt           (Phase 2+)
├── traces/trace.json         (Phase 4+)
├── traces/seed.txt           (Phase 4+)
├── traces/replay.sh          (Phase 4+)
├── corpus/                   (Phase 2+)
└── reproduce.sh
```

**Phase 1 `reproduce.sh` template (rule-based findings):**

```bash
#!/usr/bin/env bash
# Auto-generated by audit-agent v{version}
# Finding: {finding_id} — {title}
# Commit:  {commit_hash}
# To reproduce: bash reproduce.sh

set -euo pipefail

REPO_PATH="${1:-/path/to/audited/repo}"
cd "$REPO_PATH"
git checkout {commit_hash}

docker run --rm \
  --volume "$(pwd):/workspace:ro" \
  --env AUDIT_RULE={rule_id} \
  {container_image}@{container_digest} \
  audit-agent-scanner --rule {rule_id} --file {relative_file_path}

# Expected output: match at line {line_number}
```

**Acceptance criteria:**
- [ ] `manifest.json` validates against `docs/finding-schema.json`
- [ ] `reproduce.sh` runs in a clean Docker environment and reproduces the finding
- [ ] ZIP export works, file count matches manifest

---

### Phase 1 Integration Test

Run the full Phase 1 pipeline against a real target:

**Target:** [`halo2/src/`](https://github.com/privacy-scaling-explorations/halo2) or [`ark-crypto-primitives`](https://github.com/arkworks-rs/crypto-primitives)

```bash
audit-agent analyze \
  --repo https://github.com/privacy-scaling-explorations/halo2 \
  --commit a1b2c3d \
  --scope crypto-only \
  --phase 1
```

**Expected output:**
- At minimum 2 rule matches from the 8 rules
- Each match has a valid `CodeLocation`
- `evidence_pack/` directory created with `manifest.json` and `reproduce.sh`
- Markdown report generated in `output/report.md`

---

## Phase 2 — Core ZK Verification: Circom Path (Week 6–9)

**Goal:** Detect under-constrained circuits in Circom. This is the first formal-methods-backed finding in the system.

### Task 2.1 — Circom Signal Graph Builder

**Crate:** `crates/engine-crypto` → `src/zk/circom/signal_graph.rs`

Parse `.circom` files and build the signal dependency graph for Z3 analysis.

```rust
pub struct CircomSignalGraph {
    pub signals: Vec<Signal>,
    pub constraints: Vec<Constraint>,
    pub templates: HashMap<String, Template>,
    pub component_tree: ComponentTree,
}

pub struct Signal {
    pub name: String,
    pub kind: SignalKind,    // Input | Output | Intermediate
    pub template: String,
    pub constrained_by: Vec<ConstraintId>,
}

pub enum Constraint {
    // A * B === C  (R1CS form)
    R1CS { a: LinearCombination, b: LinearCombination, c: LinearCombination },
    // === direct equality
    Equality { lhs: LinearCombination, rhs: LinearCombination },
}

pub struct UnconstrainedSignal {
    pub signal: Signal,
    pub reason: String,     // "output signal has no constraint referencing it"
    pub risk_level: RiskLevel,
}

impl CircomSignalGraph {
    pub fn from_file(path: &Path) -> Result<Self>;
    
    // Quick check: any output/intermediate signal with no constraint? 
    pub fn find_trivially_unconstrained(&self) -> Vec<UnconstrainedSignal>;
    
    // Export constraint system to SMT-LIB2 format for Z3
    pub fn to_smt2(&self, target_signal: &str) -> String;
}
```

**Acceptance criteria:**
- [ ] Correctly parses circomlib `Num2Bits` template
- [ ] `find_trivially_unconstrained` catches a manually introduced unconstrained output signal
- [ ] SMT2 export is parseable by Z3 without errors

---

### Task 2.2 — Z3 Under-Constrained Checker

**Crate:** `crates/engine-crypto` → `src/zk/circom/z3_checker.rs`

Run Z3 in the sandbox container to find counterexamples.

```rust
pub struct Z3UnderConstrainedChecker {
    sandbox: Arc<SandboxExecutor>,
}

pub struct Z3CheckRequest {
    pub smt2_content: String,
    pub target_signal: String,
    pub field_prime: BigUint,   // BN254 or BLS12-381 prime
    pub budget: BudgetConfig,
}

pub enum Z3CheckResult {
    // Found two distinct witnesses satisfying all constraints
    UnderConstrained {
        witness_a: HashMap<String, BigUint>,
        witness_b: HashMap<String, BigUint>,
        counterexample_smt2: String,
    },
    // Proved uniqueness within bounds
    Constrained,
    // Z3 hit timeout or gave up
    Unknown { reason: String },
}

impl Z3UnderConstrainedChecker {
    pub async fn check(&self, req: Z3CheckRequest) -> Result<Z3CheckResult>;
}
```

**SMT2 query template (under-constrained check):**

```smt2
; Two witness sets W1 and W2 that both satisfy all constraints
; but have different output values → circuit is under-constrained
(declare-const w1_{signal} Int)
(declare-const w2_{signal} Int)
; ... field arithmetic constraints for both witness sets ...
(assert (not (= w1_output w2_output)))
(assert constraints_w1)
(assert constraints_w2)
(check-sat)
(get-model)
```

**Degradation when Z3 times out:**

```rust
// If Z3 returns Unknown, fall back to random witness search
pub async fn random_witness_search(
    &self,
    graph: &CircomSignalGraph,
    iterations: u64,   // default: 100_000
) -> Option<CounterexamplePair>;
```

**Acceptance criteria:**
- [ ] Detects the known under-constrained `LessThan` gadget in circomlib (documented in Veridise's QED2 paper)
- [ ] Z3 timeout correctly falls back to random search
- [ ] On `Constrained` result, produces SMT2 proof file for Evidence Pack
- [ ] Container digest captured in every result

---

### Task 2.3 — Kani Harness Synthesizer (Closes G9)

**Crate:** `crates/engine-crypto` → `src/kani/synthesizer.rs`

Generate Kani proof harnesses for boundary conditions. This is the first LLM integration point.

```rust
pub struct KaniSynthesizer {
    llm: Arc<dyn LlmProvider>,
    sandbox: Arc<SandboxExecutor>,
    evidence_gate: Arc<EvidenceGate>,
}

pub struct HarnessRequest {
    pub target_fn: FunctionSignature,
    pub focus: HarnessFocus,    // BoundaryConditions | Overflow | Panic | FieldOps
    pub source_context: String, // the function source + surrounding context
    pub max_bound: u64,         // Kani unwinding bound
}

pub struct HarnessResult {
    pub harness_code: String,
    pub cargo_toml: String,
    pub gate_level_reached: u8,    // 0-3
    pub kani_output: Option<KaniOutput>,
    pub shrink_attempts: u8,
}

impl KaniSynthesizer {
    pub async fn synthesize(&self, req: HarnessRequest) -> Result<HarnessResult>;
    
    // Called when state space explodes: reduce symbolic input domain
    async fn shrink_and_retry(&self, req: &HarnessRequest, error: &KaniError) -> Result<HarnessResult>;
}
```

**LLM prompt template for harness generation:**

```
You are a Rust formal verification expert using Kani model checker.
Generate a #[kani::proof] harness for the following function.

FUNCTION:
{source_code}

FOCUS: {focus}  
BOUND: {max_bound} (Kani unwinding limit)
FIELD_PRIME: {field_prime}  (if applicable)

Requirements:
- Use kani::any::<T>() for symbolic inputs
- Add kani::assume() for valid input preconditions  
- Add kani::assert!() for the safety property being checked
- Keep the harness under 50 lines
- The harness MUST compile with `cargo build --features kani`

Output ONLY the Rust code, no explanation.
```

**Evidence Gate integration (Closes G9):**

```rust
// crates/llm/src/evidence_gate.rs
pub struct EvidenceGate {
    sandbox: Arc<SandboxExecutor>,
}

impl EvidenceGate {
    pub async fn validate_harness(&self, harness: &HarnessCode) -> GateResult {
        // Level 0: syntax check via rustfmt --check
        let l0 = self.syntax_check(harness).await?;
        if l0.failed() { return GateResult::failed(0, l0.error); }

        // Level 1: compile check
        let l1 = self.compile_check(harness).await?;
        if l1.failed() { return GateResult::failed(1, l1.error); }

        // Level 2: execute kani
        let l2 = self.execute_kani(harness).await?;
        if l2.failed() { return GateResult::failed(2, l2.error); }

        // Level 3: reproduce with fixed seed
        let l3 = self.reproduce_check(harness, &l2.counterexample).await?;
        GateResult::passed(3, l3.counterexample)
    }

    // Auto-fix loop: call LLM with error message, try again
    pub async fn fix_and_retry(
        &self,
        harness: &HarnessCode,
        error: &str,
        llm: &dyn LlmProvider,
        max_retries: u8,
    ) -> GateResult;
}
```

**Acceptance criteria:**
- [ ] Generates compilable harness for a simple field arithmetic function without LLM help (template-based fallback)
- [ ] LLM-generated harness passes Evidence Gate Level 1 (compile) at least 70% of the time
- [ ] Auto-fix loop recovers from compile error in at least 50% of cases
- [ ] `llm_generated: true` is set on all LLM-generated harnesses
- [ ] Kani counterexample for `unchecked_add` on a field element is captured

---

### Task 2.4 — LLM Provider Adapters

**Crate:** `crates/llm`

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, prompt: &str, opts: &CompletionOpts) -> Result<String>;
    fn name(&self) -> &str;
}

pub struct OpenAiProvider { api_key: String, model: String }
pub struct AnthropicProvider { api_key: String, model: String }
pub struct OllamaProvider { base_url: String, model: String }

pub struct CompletionOpts {
    pub max_tokens: u32,
    pub temperature: f32,    // 0.0 for harness gen, 0.3 for reports
    pub system_prompt: Option<String>,
}
```

**Acceptance criteria:**
- [ ] All three providers implement `LlmProvider` trait
- [ ] Provider is selectable via `AuditConfig.llm.provider`
- [ ] Prompt/response logged to debug log (not to Evidence Pack — privacy)

---

### Phase 2 Integration Test

**Target:** circomlib `circuits/comparators.circom`

```bash
audit-agent analyze \
  --repo https://github.com/iden3/circomlib \
  --commit abc123 \
  --scope crypto-only \
  --framework circom
```

**Expected:**
- At least one finding with `category: UnderConstrained` and `evidence_gate_level: 3`
- Z3 counterexample saved to `evidence_pack/F-ZK-0001/smt2/`
- `reproduce.sh` runs Z3 and gets `sat` in < 60 seconds

---

## Phase 3 — Halo2 + Constraint Dependency Graph (Week 10–13)

**Goal:** Extend ZK analysis to Halo2 circuits with cross-file constraint dependency tracking — the primary technical differentiator.

### Task 3.1 — rust-analyzer Integration

**Crate:** `crates/engine-crypto` → `src/semantic/ra_client.rs`

Use `rust-analyzer` as a library (via `ra_ap_*` crates) to build the semantic index.

```rust
pub struct SemanticIndex {
    pub call_graph: CallGraph,
    pub type_map: TypeMap,
    pub macro_expansions: MacroExpansionMap,
    pub cfg_variants: CfgVariantMap,
}

pub struct CallGraph {
    // fn fully-qualified-name → Vec<fn fully-qualified-name>  
    pub edges: HashMap<FqName, Vec<FqName>>,
}

pub struct MacroExpansionMap {
    // macro call site → expanded source (post-expansion)
    pub expansions: HashMap<SpanId, String>,
}

impl SemanticIndex {
    pub async fn build(workspace: &CargoWorkspace) -> Result<Self>;
    
    // Find all implementations of a trait method (e.g., Chip::configure)
    pub fn find_trait_impls(&self, trait_name: &str, method: &str) -> Vec<FnRef>;
    
    // Get the macro-expanded source for a span
    pub fn expand_macro(&self, span: &SpanId) -> Option<&str>;
    
    // Get all cfg variants that change which code is compiled
    pub fn cfg_divergence_points(&self) -> Vec<CfgDivergence>;
}
```

**Note on `ra_ap_*` crates:** rust-analyzer exposes its internal crates with `ra_ap_` prefix. Use `ra_ap_ide`, `ra_ap_hir`, `ra_ap_syntax`. These are unstable but functional. Pin the exact version.

**Acceptance criteria:**
- [ ] Can resolve `Chip::configure` to its concrete implementation across crate boundaries
- [ ] Macro-expanded call graph differs from tree-sitter call graph for at least one proc macro in halo2
- [ ] `cfg(feature="asm")` divergence points are identified in a crate that has them

---

### Task 3.2 — Constraint Dependency Graph (Closes G2)

**Crate:** `crates/engine-crypto` → `src/zk/halo2/cdg.rs`

```rust
pub struct ConstraintDependencyGraph {
    pub chips: Vec<ChipNode>,
    pub edges: Vec<CdgEdge>,
    pub risk_annotations: Vec<RiskAnnotation>,
}

pub struct ChipNode {
    pub name: String,
    pub crate_name: String,
    pub file: PathBuf,
    pub configure_span: Span,
    pub synthesize_span: Span,
    pub constraints: ChipConstraints,
}

pub struct ChipConstraints {
    pub selectors: Vec<SelectorDef>,
    pub fixed_columns: Vec<ColumnDef>,
    pub advice_columns: Vec<ColumnDef>,
    pub lookup_tables: Vec<LookupDef>,
    pub permutation_args: Vec<PermutationArg>,
    pub custom_gates: Vec<GateExpr>,
}

pub struct CdgEdge {
    pub from_chip: ChipName,
    pub to_chip: ChipName,
    pub kind: EdgeKind,   // LookupInput | ColumnRef | PermutationGroup
    pub from_column: ColumnName,
    pub to_column: ColumnName,
}

#[derive(Debug)]
pub enum RiskAnnotation {
    // Output column exists but nothing constraints its range
    IsolatedNode { chip: ChipName, column: ColumnName },
    // Output range exceeds downstream lookup table domain  
    RangeGap { from_chip: ChipName, to_chip: ChipName, gap: RangeGap },
    // Two chips activate mutually exclusive gates at same row
    SelectorConflict { chip_a: ChipName, chip_b: ChipName, row_condition: String },
}

impl ConstraintDependencyGraph {
    pub fn build(semantic_index: &SemanticIndex) -> Result<Self>;
    pub fn high_risk_nodes(&self) -> Vec<&ChipNode>;
    pub fn to_dot(&self) -> String;   // for UI visualization
}
```

**Acceptance criteria:**
- [ ] Correctly identifies all chips in `halo2-gadgets` crate
- [ ] Builds edges between `RangeCheckChip` and consumers in the same circuit
- [ ] `IsolatedNode` risk annotation fires for a manually introduced unconstrained column
- [ ] DOT graph renders correctly in Graphviz (visual sanity check)

---

### Task 3.3 — Halo2 Local SMT Checker

**Crate:** `crates/engine-crypto` → `src/zk/halo2/smt_checker.rs`

For CDG high-risk nodes, extract the gate polynomial and check with Z3.

```rust
pub struct Halo2SmtChecker {
    sandbox: Arc<SandboxExecutor>,
}

// Extracts gate polynomial from ChipNode and builds SMT2 query
pub fn gate_to_smt2(gate: &GateExpr, field_prime: &BigUint) -> String {
    // Convert gate polynomial p(x) to:
    // (assert (exists ((x Int)) (and (= (p x) 0) (not (valid_witness x)))))
    // Check if gate has a satisfying assignment that shouldn't be valid
    todo!()
}
```

**Degradation when SMT is too slow:**

```rust
// Random counterexample search when Z3 returns Unknown
pub async fn random_gate_search(
    gate: &GateExpr,
    field_prime: &BigUint,
    iterations: u64,
) -> Option<Counterexample>;
```

**Acceptance criteria:**
- [ ] Finds under-constrained gate in a synthetic Halo2 chip with known vulnerability
- [ ] Falls back to random search within the configured timeout
- [ ] Finding is emitted with `framework: Halo2` and correct CDG node reference

---

### Task 3.4 — SP1/RISC0 Differential Tester

**Crate:** `crates/engine-crypto` → `src/zk/zkvm/diff_tester.rs`

```rust
pub struct ZkvmDiffTester {
    sandbox: Arc<SandboxExecutor>,
}

pub struct DiffTestRequest {
    pub guest_path: PathBuf,
    pub input_vectors: Vec<TestInput>,   // generated or provided
    pub public_input_schema: Schema,
}

pub enum DiffTestResult {
    Consistent,    // native == zkvm for all inputs
    Divergent {
        input: TestInput,
        native_output: Vec<u8>,
        zkvm_output: Vec<u8>,
    },
    AbiViolation {
        // guest read/wrote outside declared public input range
        description: String,
    },
}

impl ZkvmDiffTester {
    pub async fn run(&self, req: DiffTestRequest) -> Result<DiffTestResult>;
    
    // Check that image hash is bound to the correct program
    pub async fn verify_image_hash_binding(&self, guest_path: &Path) -> Result<bool>;
}
```

**Acceptance criteria:**
- [ ] Detects intentional divergence between native and SP1 guest execution
- [ ] `verify_image_hash_binding` fails when the image hash in the verifier doesn't match the guest binary

---

### Phase 3 Integration Test

**Target:** A real Halo2 project (e.g., `halo2-ecc` from Privacy Scaling Explorations)

**Expected:**
- CDG built with > 10 chips and > 5 edges
- At least one risk annotation emitted
- High-risk nodes sent to Z3, result captured (Constrained or UnderConstrained)
- CDG DOT file renders in UI (visual check)

---

## Phase 4 — Distributed Consensus Engine (Week 12–15)

> **Note:** Phase 3 and Phase 4 can be worked in parallel by separate engineers after Task 0.3 (Sandbox Executor) is complete.

### Task 4.1 — MadSim Feasibility Assessor (Closes G4)

**Crate:** `crates/engine-distributed` → `src/feasibility.rs`

```rust
pub struct MadSimFeasibilityAssessor;

#[derive(Debug)]
pub enum BridgeLevel {
    // Full MadSim harness auto-generated
    LevelA,
    // Adapter trait needed; scaffold generated, human confirmation required
    LevelB { adapter_points: Vec<AdapterPoint> },
    // Too coupled; use black-box simulation
    LevelC { reason: String },
}

pub struct AdapterPoint {
    pub file: PathBuf,
    pub line: u32,
    pub description: String,   // "Replace TcpStream with trait object"
    pub effort: Effort,        // Low | Medium | High
}

impl MadSimFeasibilityAssessor {
    pub fn assess(workspace: &CargoWorkspace, semantic_index: &SemanticIndex) -> BridgeLevel;
}
```

**Assessment heuristics:**

```
→ LevelA if:
  - tokio is a conditional dependency (feature = "tokio")
  - OR network layer uses a trait abstraction (trait NetworkTransport / trait Transport)
  - AND no raw std::net::TcpStream in hot paths

→ LevelB if:
  - tokio is a hard dependency
  - BUT network calls are clustered in <5 files
  - Estimated adapter points < 20

→ LevelC if:
  - tokio deeply coupled throughout (>50% of crates import tokio)
  - OR uses tokio internals directly (tokio::runtime::Builder)
  - OR has FFI calls in network paths
```

**Acceptance criteria:**
- [ ] `libp2p` → LevelB (network trait exists but needs adapter)
- [ ] A simple echo server → LevelA
- [ ] A project with tokio `Runtime::new()` scattered everywhere → LevelC

---

### Task 4.2 — MadSim Harness Builder (Level A)

**Crate:** `crates/engine-distributed` → `src/harness/builder.rs`

```rust
pub struct HarnessBuilder {
    llm: Arc<dyn LlmProvider>,
    evidence_gate: Arc<EvidenceGate>,
}

pub struct MadSimHarness {
    pub project_dir: PathBuf,   // runnable Cargo project
    pub entry_point: String,    // fn name of the simulation entry
    pub node_count: usize,
    pub topology: NetworkTopology,
}

impl HarnessBuilder {
    // For LevelA: auto-generate
    pub async fn generate_level_a(
        &self,
        workspace: &CargoWorkspace,
        config: &DistributedAuditConfig,
    ) -> Result<MadSimHarness>;
    
    // For LevelB: generate scaffold + adapter hints
    pub async fn generate_level_b_scaffold(
        &self,
        workspace: &CargoWorkspace,
        adapter_points: &[AdapterPoint],
    ) -> Result<AdapterScaffold>;
}
```

**LevelA harness template (LLM-assisted):**

```rust
// Template provided to LLM with workspace context
#[madsim::test]
async fn audit_harness_{name}() {
    let handle = madsim::runtime::Handle::current();
    
    // Spawn N nodes
    for i in 0..{node_count} {
        handle.create_node()
            .name(format!("node-{}", i))
            .ip(format!("10.0.0.{}", i + 1).parse().unwrap())
            .build()
            .spawn(async move {
                // TODO: LLM fills in the node startup logic
                {node_startup_code}
            });
    }
    
    madsim::time::sleep(Duration::from_secs({simulation_duration})).await;
    
    // Invariant checks
    {invariant_assertions}
}
```

**Acceptance criteria:**
- [ ] Level A harness compiles against a simple Rust P2P gossip protocol
- [ ] Harness runs to completion (smoke test) with 3 nodes, no chaos
- [ ] Level B scaffold lists adapter points with file/line locations

---

### Task 4.3 — Chaos Script Engine

**Crate:** `crates/engine-distributed` → `src/chaos/`

```rust
// Define chaos scenarios as composable DSL
#[derive(Serialize, Deserialize, Clone)]
pub struct ChaosScript {
    pub name: String,
    pub description: String,
    pub steps: Vec<ChaosStep>,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum ChaosStep {
    // Network chaos
    Partition { nodes: Vec<NodeId>, duration_ticks: u64 },
    Delay { nodes: Vec<NodeId>, delay_ms: u64, jitter_ms: u64 },
    Drop { nodes: Vec<NodeId>, drop_rate: f64 },
    Duplicate { nodes: Vec<NodeId>, dup_rate: f64 },
    Eclipse { target: NodeId, duration_ticks: u64 },
    
    // Node byzantine behavior  
    DoubleVote { node: NodeId, at_height: u64 },
    SelectiveForward { node: NodeId, drop_from: Vec<NodeId> },
    ForgeVrfOutput { node: NodeId },
    RefuseSync { node: NodeId, for_heights: RangeInclusive<u64> },
    
    // L2-specific
    SequencerDropTx { sequencer: NodeId, tx_pattern: TxPattern },
    ProposerReplayBatch { proposer: NodeId, batch_index: u64 },
    ProverSubmitWrongStateRoot { prover: NodeId, at_height: u64 },
    
    // Timing
    Wait { ticks: u64 },
    CheckInvariant { invariant: InvariantId },
}
```

**Pre-built scenario templates (ship these in Phase 4):**

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
- [ ] Partition scenario runs and triggers safety invariant violation on a buggy consensus (test with intentionally broken protocol)
- [ ] Scenarios serializable to JSON → included in Evidence Pack
- [ ] Same JSON + same seed → identical trace output

---

### Task 4.4 — Global Invariant Monitor

**Crate:** `crates/engine-distributed` → `src/invariants/`

```rust
#[async_trait]
pub trait Invariant: Send + Sync {
    fn id(&self) -> InvariantId;
    fn name(&self) -> &str;
    // Called after each scenario step; returns violation if found
    async fn check(&self, state: &SimulationState) -> Option<InvariantViolation>;
}

pub struct SafetyInvariant;   // No two nodes commit different values at same height
pub struct LivenessInvariant; // Progress within N ticks
pub struct IdempotencyInvariant; // Same message_id not processed twice
pub struct EscapeHatchInvariant; // Forced withdrawal reachable within N ticks
pub struct ProverConsistencyInvariant; // Multi-prover agreement on state root
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
    pub event_trace: Vec<SimEvent>,   // full event log up to violation
    pub node_states: HashMap<NodeId, NodeStateSnapshot>,
}
```

**Acceptance criteria:**
- [ ] `SafetyInvariant` fires when two nodes commit different blocks at same height (synthetic test)
- [ ] `EscapeHatchInvariant` fires when forced withdrawal is blocked for > N ticks
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
    // Serialize to evidence_pack/{id}/traces/trace.json
    pub fn to_json(&self) -> String;
    
    // Generate replay.sh: madsim run with fixed seed
    pub fn to_replay_script(&self, harness_path: &Path, container_image: &str) -> String;
    
    // Shrink: find minimal sub-trace that still triggers the violation
    pub fn shrink(&self, violation_tick: u64) -> TraceCapture;
}
```

**Acceptance criteria:**
- [ ] Two runs with same seed produce byte-identical `trace.json`
- [ ] `shrink` reduces a 10,000-event trace to < 50 events while preserving violation
- [ ] `replay.sh` re-triggers violation in a fresh container

---

### Phase 4 Integration Test

**Target:** A simple BFT consensus implementation (e.g., a minimal HotStuff or Tendermint implementation in Rust)

**Expected:**
- MadSim feasibility: LevelA
- Partition scenario detects liveness violation (node can't progress while isolated)
- Byzantine double-vote scenario triggers safety check
- Full trace captured with seed; `reproduce.sh` re-triggers in < 2 minutes

---

## Phase 5 — L2 Specifics, Reports & Productization (Week 14–17)

### Task 5.1 — Multi-Prover Trust Boundary Module (Closes G7)

**Crate:** `crates/engine-distributed` → `src/multiprover/`

```rust
pub struct MultiProverAuditor {
    sandbox: Arc<SandboxExecutor>,
}

pub struct MultiProverConfig {
    pub provers: Vec<ProverConfig>,       // ZK, TEE, Optimistic
    pub aggregator_contract: Option<PathBuf>,
    pub arbitration_logic: Option<PathBuf>,
}

pub struct ProverConfig {
    pub kind: ProverKind,      // ZK | TEE | Optimistic
    pub binary_path: PathBuf,
    pub key_material_paths: Vec<PathBuf>,
}

pub struct MultiProverAuditResult {
    pub key_isolation_findings: Vec<Finding>,
    pub consistency_findings: Vec<Finding>,
    pub arbitration_findings: Vec<Finding>,
    pub tee_attestation_findings: Vec<Finding>,
    pub liveness_findings: Vec<Finding>,
    pub incentive_observations: Vec<Finding>,
}

impl MultiProverAuditor {
    // Check 1: No key material shared across provers
    pub async fn check_key_isolation(&self, config: &MultiProverConfig) -> Vec<Finding>;
    
    // Check 2: Both provers agree on same state transitions
    pub async fn check_soundness_consistency(
        &self,
        config: &MultiProverConfig,
        test_inputs: &[StateTransition],
    ) -> Vec<Finding>;
    
    // Check 3: TEE attestation chain has no TOCTOU
    pub async fn check_tee_attestation(&self, config: &MultiProverConfig) -> Vec<Finding>;
    
    // Check 4: Arbitration logic correctness
    pub async fn check_arbitration_logic(&self, config: &MultiProverConfig) -> Vec<Finding>;
}
```

**Acceptance criteria:**
- [ ] Key isolation check correctly flags a scenario where two provers share an environment variable for key material
- [ ] Consistency check detects divergence between ZK and native execution on a boundary input
- [ ] All findings include `framework: Static` and `evidence_gate_level` appropriately set

---

### Task 5.2 — Economic Attack Module (Closes G5)

**Crate:** `crates/engine-distributed` → `src/economic/`

This module produces `Observation`-level findings only. No code proofs.

```rust
pub struct EconomicAttackAnalyzer {
    llm: Arc<dyn LlmProvider>,
}

pub struct EconomicRiskMatrix {
    pub risks: Vec<EconomicRisk>,
}

pub struct EconomicRisk {
    pub attack_vector: String,
    pub economic_gain: GainEstimate,   // Low | Medium | High | Critical
    pub execution_difficulty: Difficulty,
    pub affected_party: String,
    pub spec_references: Vec<SpecRef>, // EIP / whitepaper section / contract ABI
    pub finding: Finding,              // always Observation severity
}

pub struct SpecRef {
    pub kind: SpecKind,    // EIP | Whitepaper | ContractABI
    pub reference: String, // "EIP-4844 §3.2" or "docs/whitepaper.pdf p.12"
}
```

**Attack vector templates to analyze:**

```rust
const SEQUENCER_ATTACK_VECTORS: &[&str] = &[
    "Transaction ordering manipulation for MEV extraction",
    "Targeted address censorship (indefinite delay)",
    "Batch delay attack: withhold submission to extend arbitrage window",
    "Selective inclusion: only include own transactions during congestion",
];

const PROVER_ATTACK_VECTORS: &[&str] = &[
    "Invalid state root submission to trigger challenge game",
    "Coordinated prover downtime to block finalization",
    "Bond griefing: force challenger to waste bond on valid state root",
];

const SYBIL_ATTACK_VECTORS: &[&str] = &[
    "Multi-address proof submission reward extraction",
    "Fake node inflation for P2P reputation gaming",
    "Validator rotation manipulation via stake splitting",
];
```

**Acceptance criteria:**
- [ ] All findings have `severity: Observation` and `llm_generated: true`
- [ ] All findings have at least one `SpecRef` bound to a real document
- [ ] Risk matrix exported as structured JSON for report inclusion

---

### Task 5.3 — Three-Layer Report Generator (Closes G11)

**Crate:** `crates/report`

```rust
pub struct ReportGenerator {
    findings: Vec<Finding>,
    config: AuditConfig,
    evidence_store: Arc<EvidenceStore>,
}

impl ReportGenerator {
    // Layer 1: 1-2 page executive summary
    pub async fn executive_summary(&self) -> ExecutiveReport;
    
    // Layer 2: full technical report  
    pub async fn technical_report(&self) -> TechnicalReport;
    
    // Layer 3: evidence appendix (index + ZIP)
    pub async fn evidence_appendix(&self) -> EvidenceAppendix;
    
    // Export all three to output directory
    pub async fn export_all(&self, output_dir: &Path) -> Result<ExportManifest>;
}

pub struct ExecutiveReport {
    pub risk_score: u8,           // 0-100
    pub finding_summary: FindingSummary,  // count by severity
    pub top_findings: Vec<FindingSummary>, // top 5 with 2-line descriptions
    pub overall_recommendation: String,
    pub markdown: String,
}

pub struct TechnicalReport {
    pub findings: Vec<FindingReport>,  // full detail per finding
    pub methodology: String,
    pub tool_versions: HashMap<String, String>,
    pub markdown: String,
}
```

**Risk score algorithm:**

```
score = 100
- Critical finding:   -25 per finding (min 0)
- High finding:       -15 per finding
- Medium finding:     -5 per finding
- Low finding:        -2 per finding
- Observation:        -0 (informational only)
Capped at 0. Score of 70+ = acceptable. 50-70 = needs remediation. <50 = do not deploy.
```

**Acceptance criteria:**
- [ ] Executive summary fits in 2 pages as PDF
- [ ] Technical report renders all findings with code snippets properly escaped
- [ ] Evidence appendix ZIP contains all files referenced in manifests
- [ ] Risk score matches manual calculation for a test finding set

---

### Task 5.4 — Diff-Mode Incremental Pipeline (Closes G12)

**Crate:** `crates/orchestrator` → `src/diff_mode.rs`

```rust
pub struct DiffModeAnalyzer {
    cache: Arc<AnalysisCache>,
}

pub struct DiffAnalysis {
    pub base_commit: String,
    pub head_commit: String,
    pub affected_crates: Vec<String>,
    pub affected_modules: Vec<ModulePath>,
    pub full_rerun_required: bool,   // true if Cargo.toml or features changed
    pub rerun_tasks: Vec<TaskId>,
}

impl DiffModeAnalyzer {
    pub fn compute_diff(&self, base: &str, head: &str, workspace: &CargoWorkspace) -> DiffAnalysis;
    
    // Returns cached findings for unchanged modules
    pub fn load_cached_findings(&self, unchanged: &[ModulePath]) -> Vec<Finding>;
}

pub struct AnalysisCache {
    // Key: (commit_hash, feature_set_hash, tool_version_hash, module_path)
    store: sled::Db,
}
```

**Acceptance criteria:**
- [ ] Diff between two commits with only one changed file results in rerunning only that file's analysis
- [ ] Modifying `Cargo.toml` forces full rerun
- [ ] Cache hit rate > 80% for a PR that changes 2 files in a 50-file workspace
- [ ] Cached findings include original tool version — invalidated if tool version changes

---

### Task 5.5 — Tauri UI Core Views

**Directory:** `ui/`

Ship the minimum viable UI for Phase 5. Polish is deferred.

```
ui/src/
├── components/
│   ├── DagView.tsx          # DAG execution graph with node states
│   ├── FindingList.tsx      # Finding table with severity badges + LLM tag
│   ├── EvidencePanel.tsx    # Evidence pack viewer + export button
│   ├── TraceViewer.tsx      # MadSim event timeline
│   └── ReportExport.tsx     # Three-layer report export controls
├── pages/
│   ├── Audit.tsx            # Main audit configuration + launch
│   └── Results.tsx          # Post-audit findings + evidence
└── ipc/
    └── commands.ts          # Tauri IPC command bindings
```

**IPC commands (Tauri backend → frontend):**

```typescript
// ipc/commands.ts
export const startAudit = (config: AuditConfig): Promise<AuditId> =>
  invoke('start_audit', { config });

export const getFindings = (auditId: string): Promise<Finding[]> =>
  invoke('get_findings', { auditId });

export const getDagState = (auditId: string): Promise<DagState> =>
  invoke('get_dag_state', { auditId });

export const exportEvidencePack = (findingId: string, destPath: string): Promise<void> =>
  invoke('export_evidence_pack', { findingId, destPath });

export const exportReport = (auditId: string, layer: ReportLayer, destPath: string): Promise<void> =>
  invoke('export_report', { auditId, layer, destPath });
```

**Acceptance criteria:**
- [ ] DAG view shows real-time node state updates during an audit run
- [ ] Finding list correctly shows `LLM Generated` badge in orange
- [ ] Clicking a finding opens the evidence panel with all files listed
- [ ] "Export Evidence Pack" button creates a ZIP at the chosen path
- [ ] Report export produces a readable Markdown file for all three layers

---

## Cross-Phase: DAG Orchestrator

**Crate:** `crates/orchestrator`  
**Build throughout all phases — add nodes as engines become available**

```rust
pub struct AuditOrchestrator {
    engines: Vec<Box<dyn AuditEngine>>,
    sandbox: Arc<SandboxExecutor>,
    evidence_store: Arc<EvidenceStore>,
    findings_db: Arc<FindingsDb>,
    cache: Arc<AnalysisCache>,
}

pub struct AuditDag {
    nodes: HashMap<TaskId, DagNode>,
    edges: Vec<(TaskId, TaskId)>,   // dependency edges
}

pub struct DagNode {
    pub task_id: TaskId,
    pub engine: EngineRef,
    pub state: NodeState,
    pub budget: BudgetConfig,
    pub degradation: DegradationStrategy,
}

pub enum NodeState {
    Pending,
    Running { started_at: Instant },
    Success { duration_ms: u64 },
    Failed { error: String },
    Degraded { reason: String, fallback_used: String },
    Cached { from_commit: String },
}

pub enum DegradationStrategy {
    // Z3 timeout → random search
    FallbackTo(EngineRef),
    // Kani explosion → shrink domain
    ShrinkAndRetry { max_attempts: u8 },
    // Complete failure → emit Observation
    EmitObservation,
    // Skip and continue
    Skip,
}

impl AuditOrchestrator {
    pub async fn run(&self, config: &AuditConfig) -> Result<AuditReport>;
    
    // Build DAG from config (phase-aware: only include available engines)
    fn build_dag(&self, config: &AuditConfig) -> AuditDag;
    
    // Execute DAG with budget enforcement and degradation
    async fn execute_dag(&self, dag: &AuditDag) -> Vec<Finding>;
}
```

**Acceptance criteria:**
- [ ] DAG runs phases in dependency order
- [ ] Node timeout is enforced; timed-out node triggers degradation strategy
- [ ] Cached nodes report `NodeState::Cached` and return stored findings
- [ ] UI receives real-time state updates via Tauri events

---

## Testing Strategy

### Unit Tests
Every crate must have unit tests with > 80% coverage for business logic:
- Rule evaluator: one test per rule, including both match and non-match cases
- Signal graph parser: one test per Circom construct
- Evidence Gate: test each level failing independently

### Integration Test Fixtures

Maintain a set of intentionally vulnerable fixtures in `tests/fixtures/`:

```
tests/fixtures/
├── circom/
│   ├── underconstrained_lessthan.circom    # known CVE from QED2 paper
│   └── missing_range_check.circom
├── halo2/
│   ├── isolated_chip/                       # chip with unconstrained column
│   └── selector_conflict/
├── rust-crypto/
│   ├── nonce_reuse/                         # CRYPTO-001 trigger
│   ├── missing_domain_sep/                  # CRYPTO-002 trigger
│   └── weak_rng/                            # CRYPTO-004 trigger
└── distributed/
    ├── unsafe_bft/                           # Byzantine fault not handled
    └── unbounded_queue/                      # DoS via queue growth
```

### End-to-End Tests

```bash
# tests/integration/e2e_test.sh
# Phase 1: Crypto rules on real crate
audit-agent analyze --repo ./tests/fixtures/rust-crypto/nonce_reuse --expect-finding CRYPTO-001

# Phase 2: Circom under-constrained
audit-agent analyze --repo ./tests/fixtures/circom/underconstrained_lessthan --expect-finding F-ZK-*

# Phase 4: Distributed safety violation
audit-agent analyze --repo ./tests/fixtures/distributed/unsafe_bft \
  --scenario scenarios/partition-then-rejoin.yaml \
  --expect-finding F-DIST-*
```

---

## Definition of Done (Per Phase)

| Phase | Done When |
|-------|-----------|
| **Phase 0** | All containers build in CI; core types compile with serde round-trip; sandbox executor passes timeout + memory tests |
| **Phase 1** | 8 crypto rules fire correctly on fixtures; cargo-audit correlation works; Evidence Pack v1 reproducible |
| **Phase 2** | Circom under-constrained detection finds known vuln in circomlib; Kani harness passes Evidence Gate; LLM provider swappable |
| **Phase 3** | CDG built for halo2-gadgets; at least one risk annotation emitted; Halo2 SMT checker finds synthetic vuln |
| **Phase 4** | MadSim feasibility assessor classifies 3 test projects correctly; partition scenario finds liveness violation; trace replay works |
| **Phase 5** | Multi-prover consistency check works; three-layer report exports; Diff-Mode achieves >80% cache hit on 2-file PR; Tauri UI shows DAG + findings |

---

## Appendix — Key Dependencies

```toml
# Core Rust dependencies (add to workspace Cargo.toml)
[workspace.dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }
# Container management
bollard = "0.17"
# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
# CLI
clap = { version = "4", features = ["derive"] }
# Error handling
anyhow = "1"
thiserror = "1"
# Async trait
async-trait = "0.1"
# Tree-sitter
tree-sitter = "0.22"
tree-sitter-rust = "0.21"
# Big integers (field arithmetic)
num-bigint = "0.4"
# Date/time
chrono = { version = "0.4", features = ["serde"] }
# Embedded DB for cache
sled = "0.34"
# ZIP
zip = "2"
# Logging
tracing = "0.1"
tracing-subscriber = "0.3"

# rust-analyzer (pin exact commit for stability)
ra_ap_ide = "=0.0.239"
ra_ap_hir = "=0.0.239"
ra_ap_syntax = "=0.0.239"

# Tauri
tauri = { version = "2", features = ["shell-open"] }
```

```toml
# Pinned tool versions (containers/versions.toml — canonical reference)
[tools]
kani = "0.57.0"
kani_rustc = "nightly-2024-11-01"
z3 = "4.13.0"
madsim = "0.2.30"
miri_rustc = "nightly-2024-11-01"
cargo_fuzz = "0.12.0"
cargo_audit = "0.21.0"
circom = "2.1.9"
```

---

*Implementation Plan v1.0 | Based on System Design v2.0 | Total: ~17–21 weeks*
