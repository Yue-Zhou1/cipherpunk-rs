# V3 Hybrid Audit Workstation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build `v3` as a hybrid wizard + auditing workstation with persistent audit sessions, code-to-graph mapping, AI-assisted security workflow orchestration, deterministic helper-tool execution, reproducible evidence, and commercial-grade reporting.

**Architecture:** Keep the existing wizard as the intake flow, but refactor it to create persistent `AuditSession` objects backed by a local SQLite store and shared `Project IR`. Add a workstation shell that uses the same backend objects for code navigation, graph lenses, tool actions, notes/candidates/findings, and evidence review. Expand AI into a structured copilot that can create overviews, plan domain checklists, and draft unverified candidates, while deterministic tools and explicit human confirmation remain the only routes to `Verified` output.

**Tech Stack:** Rust workspace crates, Tauri v2 IPC, React + Vite frontend, SQLite + FTS5, tree-sitter, rust-analyzer-derived semantic index, Cytoscape.js, Monaco Editor, Docker sandboxing, optional remote worker execution, YAML playbooks/checklists, Typst-based report templates.

---

## Planning Conventions

- `Verified Finding` remains export-grade output.
- `Unverified Candidate` and `Review Note` are workstation-first records that may or may not become findings.
- Use `SQLite + FTS5` as the default local persistence layer.
- Treat `Lean` as an experimental external tool adapter. Do not block `v3` GA on first-class Lean proof authoring.
- Keep `wizard mode` usable throughout implementation. Do not regress `v2` CLI intake/export behavior while building the workstation.

## Delivery Order

Implement the tasks in order. Do not start the workstation UI before the `AuditSession` and `Project IR` models exist.

---

### Task 1: Establish The V3 Core Schema

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/core/src/finding.rs`
- Modify: `crates/core/src/output.rs`
- Create: `crates/core/src/session.rs`
- Test: `crates/core/tests/session_models_tests.rs`

**Step 1: Write the failing tests**

```rust
use audit_agent_core::session::{
    AuditRecord, AuditRecordKind, AuditSession, ProjectSnapshot, SessionUiState,
};
use audit_agent_core::finding::{Severity, VerificationStatus};

#[test]
fn audit_record_kind_round_trips_through_json() {
    let record = AuditRecord::candidate(
        "CAND-001",
        "Possible nonce reuse",
        VerificationStatus::Unverified {
            reason: "AI hotspot review".to_string(),
        },
    );
    let json = serde_json::to_string(&record).unwrap();
    let parsed: AuditRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.kind, AuditRecordKind::Candidate);
}

#[test]
fn audit_session_embeds_snapshot_domains_and_ui_state() {
    let session = AuditSession {
        session_id: "sess-1".to_string(),
        snapshot: ProjectSnapshot::minimal("snap-1"),
        selected_domains: vec!["crypto".to_string(), "consensus".to_string()],
        ui_state: SessionUiState::default(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    assert_eq!(session.selected_domains.len(), 2);
}
```

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p audit-agent-core session_models_tests -q`

Expected: FAIL with missing `session` module and unresolved `AuditRecord` symbols.

**Step 3: Add the new v3 core types**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum AuditRecordKind {
    ReviewNote,
    Candidate,
    Finding,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectSnapshot {
    pub snapshot_id: String,
    pub source: ResolvedSource,
    pub target_crates: Vec<String>,
    pub excluded_crates: Vec<String>,
    pub build_matrix: Vec<BuildVariant>,
    pub detected_frameworks: Vec<Framework>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AuditRecord {
    pub record_id: String,
    pub kind: AuditRecordKind,
    pub title: String,
    pub summary: String,
    pub severity: Option<Severity>,
    pub verification_status: VerificationStatus,
    pub locations: Vec<CodeLocation>,
    pub evidence_refs: Vec<String>,
    pub labels: Vec<String>,
}
```

**Step 4: Extend exported output models without breaking v2 report compatibility**

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AuditOutputs {
    pub dir: PathBuf,
    pub manifest: AuditManifest,
    pub findings: Vec<Finding>,
    pub candidates: Vec<AuditRecord>,
    pub review_notes: Vec<AuditRecord>,
}
```

Keep the existing `Finding` type for exported verified findings. Do not replace it with a loose union type.

**Step 5: Re-export the new module and regenerate schemas**

Run:

```bash
cargo test -p audit-agent-core session_models_tests -q
cargo run -p audit-agent-core --bin generate_schemas
```

Expected: PASS and updated schema generation without serde/schema regressions.

**Step 6: Commit**

```bash
git add Cargo.toml crates/core/src/lib.rs crates/core/src/finding.rs crates/core/src/output.rs crates/core/src/session.rs crates/core/tests/session_models_tests.rs docs/*.json
git commit -m "feat: add v3 session and audit record schema"
```

---

### Task 2: Add Persistent Session Storage With SQLite + FTS5

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/session-store/Cargo.toml`
- Create: `crates/session-store/src/lib.rs`
- Create: `crates/session-store/src/schema.rs`
- Create: `crates/session-store/src/sqlite.rs`
- Create: `crates/session-store/src/search.rs`
- Test: `crates/session-store/tests/session_store_tests.rs`

**Step 1: Write the failing store tests**

```rust
use session_store::SessionStore;

#[test]
fn create_and_reload_session_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::open(dir.path().join("sessions.sqlite")).unwrap();
    let session = audit_agent_core::session::AuditSession::sample("sess-1");
    store.create_session(&session).unwrap();
    let loaded = store.load_session("sess-1").unwrap().unwrap();
    assert_eq!(loaded.session_id, "sess-1");
}

#[test]
fn full_text_search_returns_matching_records() {
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::open(dir.path().join("sessions.sqlite")).unwrap();
    store.insert_searchable_record("sess-1", "nonce reuse in signer").unwrap();
    let hits = store.search_records("nonce").unwrap();
    assert_eq!(hits.len(), 1);
}
```

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p session-store -q`

Expected: FAIL because the crate does not exist yet.

**Step 3: Create the crate and schema**

Use `rusqlite` with FTS5-enabled SQLite. Implement tables for:

- `project_snapshots`
- `audit_sessions`
- `audit_records`
- `tool_runs`
- `evidence_artifacts`
- `checklist_runs`
- `session_events`
- `record_search` as an FTS5 virtual table

Use a managed root such as `.audit-sessions/<session-id>/artifacts/` for large evidence files.

**Step 4: Implement the minimal store API**

```rust
pub struct SessionStore { /* sqlite connection pool or single connection */ }

impl SessionStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self>;
    pub fn create_session(&self, session: &AuditSession) -> Result<()>;
    pub fn load_session(&self, session_id: &str) -> Result<Option<AuditSession>>;
    pub fn upsert_record(&self, session_id: &str, record: &AuditRecord) -> Result<()>;
    pub fn append_event(&self, session_id: &str, event: &SessionEvent) -> Result<()>;
    pub fn search_records(&self, query: &str) -> Result<Vec<RecordSearchHit>>;
}
```

**Step 5: Run the crate tests**

Run: `cargo test -p session-store -q`

Expected: PASS, including persistence across reopened connections.

**Step 6: Commit**

```bash
git add Cargo.toml crates/session-store
git commit -m "feat: add persistent session store"
```

---

### Task 3: Add The Knowledge And Playbook System

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/knowledge/Cargo.toml`
- Create: `crates/knowledge/src/lib.rs`
- Create: `crates/knowledge/src/models.rs`
- Create: `crates/knowledge/src/loader.rs`
- Create: `crates/knowledge/src/store.rs`
- Create: `knowledge/playbooks/rust-crypto.yaml`
- Create: `knowledge/playbooks/circom-zk.yaml`
- Create: `knowledge/playbooks/cairo-starknet.yaml`
- Create: `knowledge/playbooks/distributed-consensus.yaml`
- Create: `knowledge/domains/crypto.yaml`
- Create: `knowledge/domains/zk.yaml`
- Create: `knowledge/domains/p2p-consensus.yaml`
- Create: `knowledge/domains/economic.yaml`
- Test: `crates/knowledge/tests/playbook_tests.rs`

**Step 1: Write the failing tests**

```rust
use knowledge::KnowledgeBase;

#[test]
fn playbooks_load_and_route_tools_for_rust_crypto() {
    let kb = KnowledgeBase::load_from_repo_root().unwrap();
    let routing = kb.route_tools(&["rust".to_string(), "crypto".to_string()]);
    assert!(routing.iter().any(|tool| tool == "kani"));
    assert!(routing.iter().any(|tool| tool == "cargo-fuzz"));
}

#[test]
fn domains_include_required_checklist_items() {
    let kb = KnowledgeBase::load_from_repo_root().unwrap();
    let domain = kb.domain("zk").unwrap();
    assert!(domain.items.iter().any(|item| item.id == "witness-shape"));
}
```

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p knowledge -q`

Expected: FAIL because the crate and YAML bundles do not exist.

**Step 3: Create the playbook and domain models**

```rust
pub struct ToolPlaybook {
    pub id: String,
    pub applies_to: Vec<String>,
    pub domains: Vec<String>,
    pub preferred_tools: Vec<String>,
    pub initial_queries: Vec<String>,
}

pub struct DomainChecklist {
    pub id: String,
    pub name: String,
    pub items: Vec<ChecklistItem>,
}
```

Seed the initial YAML packs with best-practice guidance for:

- Rust cryptography
- Circom and general ZK circuits
- Cairo/Starknet
- P2P/consensus/distributed systems

**Step 4: Add store support for adjudicated cases**

Expose simple APIs to ingest and retrieve:

- true positives
- false positives
- helpful tool sequences
- reusable repro patterns

Do not make the knowledge base the final source of truth for verification.

**Step 5: Run the tests**

Run: `cargo test -p knowledge -q`

Expected: PASS with YAML loading and routing behavior covered.

**Step 6: Commit**

```bash
git add Cargo.toml crates/knowledge knowledge
git commit -m "feat: add v3 knowledge base and playbooks"
```

---

### Task 4: Build The Shared Project IR And Code-To-Graph Pipeline

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/project-ir/Cargo.toml`
- Create: `crates/project-ir/src/lib.rs`
- Create: `crates/project-ir/src/graph.rs`
- Create: `crates/project-ir/src/redaction.rs`
- Create: `crates/project-ir/src/rust.rs`
- Create: `crates/project-ir/src/circom.rs`
- Create: `crates/project-ir/src/cairo.rs`
- Create: `crates/project-ir/src/semantic.rs`
- Test: `crates/project-ir/tests/rust_graph_tests.rs`
- Test: `crates/project-ir/tests/dataflow_redaction_tests.rs`

**Step 1: Write the failing tests against existing fixtures**

```rust
use project_ir::{GraphLensKind, ProjectIrBuilder};

#[tokio::test]
async fn rust_fixture_builds_file_and_symbol_graphs() {
    let fixture = std::path::PathBuf::from("crates/engine-crypto/tests/fixtures/rust-crypto");
    let ir = ProjectIrBuilder::for_path(&fixture).build().await.unwrap();
    assert!(!ir.file_graph.nodes.is_empty());
    assert!(!ir.symbol_graph.nodes.is_empty());
}

#[tokio::test]
async fn dataflow_edges_are_redacted_by_default() {
    let fixture = std::path::PathBuf::from("crates/engine-crypto/tests/fixtures/rust-crypto");
    let ir = ProjectIrBuilder::for_path(&fixture).build().await.unwrap();
    assert!(ir.dataflow_graph.edges.iter().all(|edge| edge.value_preview.is_none()));
}
```

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p project-ir -q`

Expected: FAIL because the crate does not exist.

**Step 3: Define the IR types**

```rust
pub struct ProjectIr {
    pub file_graph: Graph<FileNode>,
    pub symbol_graph: Graph<SymbolNode>,
    pub feature_graph: Graph<FeatureNode>,
    pub dataflow_graph: Graph<DataflowNode>,
    pub framework_views: Vec<FrameworkView>,
}

pub trait LanguageMapper {
    fn can_handle(&self, workspace: &CargoWorkspace) -> bool;
    fn build(&self, workspace: &CargoWorkspace) -> anyhow::Result<ProjectIrFragment>;
}
```

**Step 4: Implement the first usable graph pipeline**

- reuse and generalize the existing Rust semantic index logic from `crates/engine-crypto/src/semantic/ra_client.rs`
- add Rust file/symbol relationships first
- add Circom and Cairo adapters behind the trait, with feature detection and graceful partial support
- treat concrete values as redacted unless a later policy layer allows previews

**Step 5: Run the new test suite**

Run: `cargo test -p project-ir -q`

Expected: PASS with at least one Rust graph fixture and default-redaction behavior covered.

**Step 6: Commit**

```bash
git add Cargo.toml crates/project-ir
git commit -m "feat: add shared project ir and graph extraction"
```

---

### Task 5: Refactor The Orchestrator Around Sessions, Jobs, And Real Runtime Traits

**Files:**
- Modify: `crates/core/src/engine.rs`
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/orchestrator/src/lib.rs`
- Create: `crates/orchestrator/src/jobs.rs`
- Create: `crates/orchestrator/src/events.rs`
- Create: `crates/orchestrator/src/runtime.rs`
- Modify: `crates/intake/src/lib.rs`
- Modify: `crates/evidence/src/lib.rs`
- Modify: `crates/sandbox/src/lib.rs`
- Test: `crates/orchestrator/tests/job_runtime_tests.rs`

**Step 1: Write failing orchestrator tests**

```rust
use orchestrator::{AuditJobKind, AuditOrchestrator};

#[tokio::test]
async fn project_ir_job_is_emitted_when_session_is_created() {
    let orchestrator = AuditOrchestrator::for_tests();
    let session = audit_agent_core::session::AuditSession::sample("sess-1");
    let jobs = orchestrator.bootstrap_jobs(&session).await.unwrap();
    assert!(jobs.iter().any(|job| matches!(job.kind, AuditJobKind::BuildProjectIr)));
}

#[tokio::test]
async fn llm_context_is_available_to_non_verifying_jobs() {
    let orchestrator = AuditOrchestrator::for_tests();
    let ctx = orchestrator.test_context();
    assert!(ctx.llm.is_some());
}
```

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p orchestrator job_runtime_tests -q`

Expected: FAIL because the current orchestrator has no job model and drops `llm` from `AuditContext`.

**Step 3: Replace placeholder runtime types with traits**

Add traits in `crates/core/src/engine.rs`:

```rust
#[async_trait]
pub trait SandboxRunner: Send + Sync {
    async fn execute(&self, request: SandboxRequest) -> anyhow::Result<SandboxResult>;
}

#[async_trait]
pub trait EvidenceWriter: Send + Sync {
    async fn save(&self, artifact: EvidenceArtifact) -> anyhow::Result<()>;
}
```

Have `crates/sandbox` and `crates/evidence` implement these traits. Stop relying on the empty placeholder structs in `crates/core/src/lib.rs`.

**Step 4: Introduce a real job model**

```rust
pub enum AuditJobKind {
    BuildProjectIr,
    GenerateAiOverview,
    PlanChecklists,
    RunDomainChecklist { domain_id: String },
    RunToolAction { action_id: String },
    ExportReports,
}
```

Store job lifecycle events in the `SessionStore`.

**Step 5: Run targeted tests**

Run:

```bash
cargo test -p orchestrator job_runtime_tests -q
cargo test -p evidence -q
cargo test -p sandbox -q
```

Expected: PASS with the orchestrator using real runtime traits and persisted job metadata.

**Step 6: Commit**

```bash
git add crates/core/src/engine.rs crates/core/src/lib.rs crates/orchestrator crates/intake/src/lib.rs crates/evidence/src/lib.rs crates/sandbox/src/lib.rs
git commit -m "refactor: add v3 session-aware orchestrator runtime"
```

---

### Task 6: Add Typed AI Copilot Contracts For Overviews, Checklists, And Candidates

**Files:**
- Modify: `crates/llm/src/lib.rs`
- Create: `crates/llm/src/copilot.rs`
- Create: `crates/llm/src/contracts.rs`
- Create: `crates/llm/src/sanitize.rs`
- Modify: `crates/llm/src/provider.rs`
- Test: `crates/llm/tests/copilot_tests.rs`

**Step 1: Write the failing tests**

```rust
use llm::copilot::{ChecklistPlan, CopilotService};

#[tokio::test]
async fn checklist_plan_parses_structured_json_only() {
    let service = CopilotService::with_mock_json(r#"{"domains":[{"id":"crypto","rationale":"key material present"}]}"#);
    let plan: ChecklistPlan = service.plan_checklists("rust crypto workspace").await.unwrap();
    assert_eq!(plan.domains[0].id, "crypto");
}

#[tokio::test]
async fn candidate_generation_never_returns_verified_status() {
    let service = CopilotService::with_mock_json(r#"{"title":"Possible bug","summary":"review me"}"#);
    let candidate = service.generate_candidate("hotspot").await.unwrap();
    assert!(matches!(candidate.verification_status, audit_agent_core::finding::VerificationStatus::Unverified { .. }));
}
```

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p llm copilot_tests -q`

Expected: FAIL because the typed copilot contract layer does not exist.

**Step 3: Create the typed contract layer**

Add structs such as:

```rust
pub struct ArchitectureOverview {
    pub assets: Vec<String>,
    pub trust_boundaries: Vec<String>,
    pub hotspots: Vec<String>,
    pub likely_domains: Vec<String>,
}

pub struct ChecklistPlan {
    pub domains: Vec<DomainPlan>,
}

pub struct CandidateDraft {
    pub title: String,
    pub summary: String,
    pub suggested_tools: Vec<String>,
    pub confidence: String,
}
```

Force JSON-only outputs and validate them before converting to workstation records.

**Step 4: Enforce the trust boundary**

- AI-generated overview material becomes `ReviewNote`
- AI-generated hypotheses become `Unverified Candidate`
- no AI path may create `Verified Finding`

**Step 5: Run the tests**

Run: `cargo test -p llm copilot_tests -q`

Expected: PASS with prompt sanitization, JSON parsing, and trust-boundary assertions covered.

**Step 6: Commit**

```bash
git add crates/llm/src/lib.rs crates/llm/src/copilot.rs crates/llm/src/contracts.rs crates/llm/src/sanitize.rs crates/llm/src/provider.rs crates/llm/tests/copilot_tests.rs
git commit -m "feat: add typed ai copilot contracts"
```

---

### Task 7: Refactor The Wizard Into Audit Session Creation

**Files:**
- Modify: `ui/src/App.tsx`
- Modify: `ui/src/types.ts`
- Modify: `ui/src/ipc/commands.ts`
- Create: `ui/src/features/wizard/WizardShell.tsx`
- Create: `ui/src/features/wizard/sessionFlow.ts`
- Modify: `crates/tauri-ui/src/ipc.rs`
- Modify: `ui/src-tauri/src/commands.rs`
- Modify: `ui/src-tauri/src/lib.rs`
- Test: `ui/src/App.test.tsx`
- Test: `crates/tauri-ui/tests/ui_ipc_tests.rs`

**Step 1: Write the failing UI and IPC tests**

```tsx
it("creates an audit session and enters workstation mode after confirmation", async () => {
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: /confirm and start audit/i }));
  expect(await screen.findByText(/workstation/i)).toBeInTheDocument();
});
```

```rust
#[tokio::test]
async fn confirm_workspace_creates_session_id_and_snapshot() {
    let mut session = tauri_ui::ipc::UiSessionState::new(std::path::PathBuf::from(".audit-work"));
    let response = session.create_audit_session_for_tests().await.unwrap();
    assert!(response.session_id.starts_with("sess-"));
}
```

**Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p tauri-ui ui_ipc_tests -q
cd ui && npm test -- --runInBand
```

Expected: FAIL because there is no `create_audit_session` flow and the app remains a stepper-only experience.

**Step 3: Add session-creation IPC**

Add commands for:

- `create_audit_session`
- `list_audit_sessions`
- `open_audit_session`

Return a stable `session_id`, `snapshot_id`, and initial job list.

**Step 4: Split app mode**

Introduce:

```ts
type AppMode =
  | { kind: "wizard" }
  | { kind: "workstation"; sessionId: string };
```

Keep the stepper flow inside `WizardShell`. After confirmation, switch to workstation mode instead of staying inside the wizard.

**Step 5: Run the tests**

Run:

```bash
cargo test -p tauri-ui ui_ipc_tests -q
cd ui && npm test -- --runInBand
```

Expected: PASS with session creation and mode switching covered.

**Step 6: Commit**

```bash
git add ui/src/App.tsx ui/src/types.ts ui/src/ipc/commands.ts ui/src/features/wizard crates/tauri-ui/src/ipc.rs ui/src-tauri/src/commands.rs ui/src-tauri/src/lib.rs ui/src/App.test.tsx crates/tauri-ui/tests/ui_ipc_tests.rs
git commit -m "feat: refactor wizard into session creation flow"
```

---

### Task 8: Build The Workstation Shell

**Files:**
- Modify: `ui/package.json`
- Create: `ui/src/features/workstation/WorkstationShell.tsx`
- Create: `ui/src/features/workstation/ProjectExplorer.tsx`
- Create: `ui/src/features/workstation/CodeEditorPane.tsx`
- Create: `ui/src/features/workstation/ToolbenchPanel.tsx`
- Create: `ui/src/features/workstation/ActivityConsole.tsx`
- Create: `ui/src/features/workstation/useSessionState.ts`
- Modify: `ui/src/styles.css`
- Modify: `ui/src/ipc/commands.ts`
- Modify: `ui/src-tauri/src/commands.rs`
- Test: `ui/src/features/workstation/WorkstationShell.test.tsx`

**Step 1: Write the failing component test**

```tsx
it("renders explorer, editor, toolbench, and console panels", () => {
  render(<WorkstationShell sessionId="sess-1" />);
  expect(screen.getByText(/project explorer/i)).toBeInTheDocument();
  expect(screen.getByText(/toolbench/i)).toBeInTheDocument();
  expect(screen.getByText(/activity console/i)).toBeInTheDocument();
});
```

**Step 2: Run the test to verify it fails**

Run: `cd ui && npm test -- --runInBand WorkstationShell`

Expected: FAIL because the workstation components do not exist.

**Step 3: Add the shell layout**

Use:

- Monaco Editor for the center code pane
- a dedicated explorer panel instead of reusing the wizard crate list
- a persistent bottom console for logs, traces, and evidence previews

**Step 4: Add file-tree and file-content IPC**

Add commands for:

- `get_project_tree`
- `read_source_file`
- `tail_session_console`

Do not use mock findings as the data source for the workstation.

**Step 5: Run the tests and build**

Run:

```bash
cd ui && npm test -- --runInBand WorkstationShell
cd ui && npm run build
```

Expected: PASS with a production build that renders the new shell.

**Step 6: Commit**

```bash
git add ui/package.json ui/src/features/workstation ui/src/styles.css ui/src/ipc/commands.ts ui/src-tauri/src/commands.rs
git commit -m "feat: add workstation shell"
```

---

### Task 9: Add Graph Lenses And The AI Security Overview Panel

**Files:**
- Modify: `ui/package.json`
- Create: `ui/src/features/workstation/GraphLens.tsx`
- Create: `ui/src/features/workstation/SecurityOverviewPanel.tsx`
- Create: `ui/src/features/workstation/ChecklistPanel.tsx`
- Modify: `ui/src/features/workstation/WorkstationShell.tsx`
- Modify: `ui/src/ipc/commands.ts`
- Modify: `ui/src-tauri/src/commands.rs`
- Modify: `crates/tauri-ui/src/ipc.rs`
- Modify: `crates/project-ir/src/lib.rs`
- Test: `ui/src/features/workstation/GraphLens.test.tsx`

**Step 1: Write the failing graph and overview tests**

```tsx
it("switches between file, feature, and dataflow graph lenses", async () => {
  render(<GraphLens sessionId="sess-1" />);
  fireEvent.click(screen.getByRole("button", { name: /feature graph/i }));
  expect(await screen.findByText(/feature graph/i)).toBeInTheDocument();
});
```

```tsx
it("shows ai-generated assets, trust boundaries, and hotspots as review notes", async () => {
  render(<SecurityOverviewPanel sessionId="sess-1" />);
  expect(await screen.findByText(/trust boundaries/i)).toBeInTheDocument();
});
```

**Step 2: Run the tests to verify they fail**

Run: `cd ui && npm test -- --runInBand GraphLens SecurityOverviewPanel`

Expected: FAIL because graph and overview panels do not exist.

**Step 3: Add backend commands for graph and overview loading**

Add:

- `load_file_graph`
- `load_feature_graph`
- `load_dataflow_graph`
- `load_security_overview`
- `load_checklist_plan`

All graph requests must be session-scoped and backed by the persisted `Project IR`.

**Step 4: Render graphs with redaction-safe defaults**

- file and feature lenses show topology first
- dataflow lens shows redacted values by default
- add an explicit affordance for value previews only after policy approval

**Step 5: Run the tests**

Run:

```bash
cd ui && npm test -- --runInBand GraphLens SecurityOverviewPanel ChecklistPanel
cd ui && npm run build
```

Expected: PASS with graph switching and overview rendering covered.

**Step 6: Commit**

```bash
git add ui/package.json ui/src/features/workstation/GraphLens.tsx ui/src/features/workstation/SecurityOverviewPanel.tsx ui/src/features/workstation/ChecklistPanel.tsx ui/src/features/workstation/WorkstationShell.tsx ui/src/ipc/commands.ts ui/src-tauri/src/commands.rs crates/tauri-ui/src/ipc.rs crates/project-ir/src/lib.rs
git commit -m "feat: add project graph lenses and security overview"
```

---

### Task 10: Add The Targeted Toolbench And Domain Checklist Execution

**Files:**
- Create: `crates/core/src/tooling.rs`
- Modify: `crates/orchestrator/src/lib.rs`
- Create: `crates/orchestrator/src/tool_actions.rs`
- Modify: `crates/sandbox/src/lib.rs`
- Create: `crates/engine-crypto/src/tool_actions/kani.rs`
- Create: `crates/engine-crypto/src/tool_actions/z3.rs`
- Create: `crates/engine-crypto/src/tool_actions/fuzz.rs`
- Create: `crates/engine-distributed/src/tool_actions/madsim.rs`
- Create: `crates/engine-distributed/src/tool_actions/chaos.rs`
- Modify: `crates/knowledge/src/lib.rs`
- Modify: `ui/src/features/workstation/ToolbenchPanel.tsx`
- Modify: `ui/src/ipc/commands.ts`
- Test: `crates/orchestrator/tests/tool_actions_tests.rs`
- Test: `ui/src/features/workstation/ToolbenchPanel.test.tsx`

**Step 1: Write the failing toolbench tests**

```rust
use orchestrator::{ToolActionRequest, ToolFamily};

#[tokio::test]
async fn kani_action_creates_job_and_artifact_refs() {
    let orchestrator = orchestrator::AuditOrchestrator::for_tests();
    let result = orchestrator
        .run_tool_action(ToolActionRequest::kani("sess-1", "crate::module::target_fn"))
        .await
        .unwrap();
    assert_eq!(result.tool_family, ToolFamily::Kani);
    assert!(!result.artifact_refs.is_empty());
}
```

```tsx
it("shows checklist and helper tools for the selected symbol", async () => {
  render(<ToolbenchPanel sessionId="sess-1" selection={{ kind: "symbol", id: "prove" }} />);
  expect(await screen.findByText(/kani/i)).toBeInTheDocument();
  expect(await screen.findByText(/domain checklists/i)).toBeInTheDocument();
});
```

**Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p orchestrator tool_actions_tests -q
cd ui && npm test -- --runInBand ToolbenchPanel
```

Expected: FAIL because there is no targeted tool-action registry.

**Step 3: Add the tool-action model**

```rust
pub enum ToolFamily {
    Kani,
    Z3,
    CargoFuzz,
    MadSim,
    Chaos,
    CircomZ3,
    CairoExternal,
    LeanExternal,
}

pub struct ToolActionRequest {
    pub session_id: String,
    pub tool_family: ToolFamily,
    pub target: ToolTarget,
    pub budget: ToolBudget,
}
```

Use first-class implementations for:

- `Kani`
- `Z3`
- `cargo-fuzz`
- `MadSim/chaos` automation

Provide `CairoExternal` and `LeanExternal` plugin slots behind explicit external-tool adapters.

**Step 4: Wire AI checklist planning into tool selection**

The toolbench should show:

- recommended tools
- domain checklist items
- rationale from the knowledge base and AI overview
- previous adjudicated similar cases, if any

**Step 5: Run tests**

Run:

```bash
cargo test -p orchestrator tool_actions_tests -q
cargo test -p engine-crypto -q
cargo test -p engine-distributed -q
cd ui && npm test -- --runInBand ToolbenchPanel
```

Expected: PASS with targeted tool actions producing jobs and artifact references.

**Step 6: Commit**

```bash
git add crates/core/src/tooling.rs crates/orchestrator crates/sandbox/src/lib.rs crates/engine-crypto/src/tool_actions crates/engine-distributed/src/tool_actions crates/knowledge/src/lib.rs ui/src/features/workstation/ToolbenchPanel.tsx ui/src/ipc/commands.ts crates/orchestrator/tests/tool_actions_tests.rs ui/src/features/workstation/ToolbenchPanel.test.tsx
git commit -m "feat: add targeted toolbench and checklist execution"
```

---

### Task 11: Add The Review Queue, Evidence Upgrade, And Commercial Report Pipeline

**Files:**
- Modify: `crates/findings/src/pipeline.rs`
- Modify: `crates/evidence/src/lib.rs`
- Modify: `crates/report/src/lib.rs`
- Create: `crates/report/src/coverage.rs`
- Create: `crates/report/src/typst.rs`
- Create: `crates/report/templates/executive.typ`
- Create: `crates/report/templates/technical.typ`
- Create: `crates/report/templates/candidates.typ`
- Modify: `crates/report/src/generator.rs`
- Modify: `ui/src/features/workstation/ActivityConsole.tsx`
- Create: `ui/src/features/workstation/ReviewQueue.tsx`
- Modify: `ui/src/ipc/commands.ts`
- Test: `crates/report/tests/v3_report_tests.rs`
- Test: `ui/src/features/workstation/ReviewQueue.test.tsx`

**Step 1: Write the failing tests**

```rust
#[test]
fn technical_report_includes_verified_findings_candidate_appendix_and_tool_inventory() {
    let report = report::generator::render_v3_report(sample_v3_bundle());
    assert!(report.contains("Verified Findings"));
    assert!(report.contains("Unverified Candidates"));
    assert!(report.contains("Tool Inventory"));
}
```

```tsx
it("allows the engineer to confirm or reject a candidate", async () => {
  render(<ReviewQueue sessionId="sess-1" />);
  expect(await screen.findByRole("button", { name: /confirm finding/i })).toBeInTheDocument();
  expect(await screen.findByRole("button", { name: /mark false positive/i })).toBeInTheDocument();
});
```

**Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p report v3_report_tests -q
cd ui && npm test -- --runInBand ReviewQueue
```

Expected: FAIL because the v3 review workflow and report format do not exist.

**Step 3: Upgrade evidence and review flow**

- add workstation review actions: `confirm`, `reject`, `suppress`, `annotate`
- persist reviewer decisions back into `SessionStore` and `KnowledgeBase`
- ensure verified findings have artifact provenance and replay paths

**Step 4: Replace plain-text PDF output with template-based report generation**

Use `Typst` templates compiled in a sandboxed report step. The technical report must include:

- project metadata and scope
- tool inventory
- checklist coverage summary
- verified findings with evidence and reproduction
- unverified candidate appendix
- recommended fixes and regression-test section

**Step 5: Run tests**

Run:

```bash
cargo test -p report v3_report_tests -q
cargo test -p findings -q
cargo test -p evidence -q
cd ui && npm test -- --runInBand ReviewQueue
```

Expected: PASS with report sections, evidence references, and candidate review actions covered.

**Step 6: Commit**

```bash
git add crates/findings/src/pipeline.rs crates/evidence/src/lib.rs crates/report ui/src/features/workstation/ActivityConsole.tsx ui/src/features/workstation/ReviewQueue.tsx ui/src/ipc/commands.ts crates/report/tests/v3_report_tests.rs ui/src/features/workstation/ReviewQueue.test.tsx
git commit -m "feat: add review queue and v3 report pipeline"
```

---

### Task 12: Add Remote Workers, Hardening, And Final Verification

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/worker-protocol/Cargo.toml`
- Create: `crates/worker-protocol/src/lib.rs`
- Create: `crates/worker-runner/Cargo.toml`
- Create: `crates/worker-runner/src/lib.rs`
- Create: `crates/worker-runner/src/main.rs`
- Modify: `crates/sandbox/src/lib.rs`
- Create: `crates/sandbox/src/remote.rs`
- Create: `crates/sandbox/src/redaction.rs`
- Modify: `README.md`
- Create: `docs/plans/2026-03-12-v3-rollout-checklist.md`
- Test: `crates/sandbox/tests/remote_worker_tests.rs`
- Test: `crates/orchestrator/tests/end_to_end_v3_tests.rs`

**Step 1: Write failing remote-worker tests**

```rust
#[tokio::test]
async fn remote_worker_executes_job_and_returns_signed_artifact_manifest() {
    let runner = sandbox::remote::RemoteExecutor::for_tests();
    let result = runner.execute(sample_remote_request()).await.unwrap();
    assert!(!result.container_digest.is_empty());
    assert!(!result.artifacts.is_empty());
}
```

**Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p sandbox remote_worker_tests -q
cargo test -p orchestrator end_to_end_v3_tests -q
```

Expected: FAIL because there is no worker protocol or remote executor.

**Step 3: Add remote execution support**

Introduce:

```rust
pub enum ExecutionBackend {
    LocalDocker,
    RemoteWorker,
}
```

Add:

- a serializable worker protocol
- a basic worker-runner binary for sandbox jobs
- backend selection logic with local fallback
- real `Allowlist` network handling in sandbox mode

**Step 4: Add redaction and observability**

- centralize AI prompt redaction rules
- emit structured session/job logs
- record retries, timeouts, and worker failures
- add a rollout checklist for manual verification against representative repos

**Step 5: Run the full verification suite**

Run:

```bash
cargo test
cd ui && npm test
cd ui && npm run build
```

Expected: PASS across Rust crates and frontend build/tests. Fix any failures before closing the task.

**Step 6: Commit**

```bash
git add Cargo.toml crates/worker-protocol crates/worker-runner crates/sandbox/src/lib.rs crates/sandbox/src/remote.rs crates/sandbox/src/redaction.rs README.md docs/plans/2026-03-12-v3-rollout-checklist.md crates/sandbox/tests/remote_worker_tests.rs crates/orchestrator/tests/end_to_end_v3_tests.rs
git commit -m "feat: add remote workers and v3 production hardening"
```

---

## Exit Criteria

`v3` is ready for internal production trials when all of the following are true:

- wizard mode creates persistent audit sessions instead of ending at a result screen
- workstation mode is the main interface after session creation
- project IR supports file, feature, and redacted dataflow graph lenses
- AI overview and checklist planning produce `ReviewNote` and `Unverified Candidate` records only
- targeted toolbench actions work for Kani, Z3, fuzzing, and distributed simulation
- verified findings are reproducible and backed by saved artifacts
- the review queue allows engineers to confirm/reject candidates
- the knowledge base stores playbooks, domains, and adjudicated feedback
- local persistence and search survive restarts
- remote execution is available for expensive jobs or explicitly deferred with documented rollout criteria
- reports include tool inventory, checklist coverage, verified findings, candidate appendix, repro steps, and recommended fixes

## Deferred Work After V3 GA

- richer Cairo/Starknet deterministic analysis beyond plugin-based adapters
- first-class Lean automation instead of external tool slot only
- multi-user collaboration and server-backed shared sessions
- large-scale case ingestion pipelines from external audit datasets

## Engineer Notes

- Keep each task small enough to review independently.
- Do not reintroduce browser-only mock data for core workstation behavior.
- Prefer adding typed domain objects over stuffing new fields into stringly typed maps.
- Do not relax the verification boundary to make AI output look more complete.
- When unsure, bias toward reproducibility and human review over convenience.
