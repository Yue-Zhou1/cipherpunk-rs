# Crates Restructuring Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Reorganize the flat 18-crate `crates/` directory into 6 domain-centric nested groups.

**Architecture:** Move crate directories into group subdirectories (engines, data, services, workers, apps), update all `path =` references in Cargo.toml files, update hardcoded paths in tests and UI fallback data.

**Tech Stack:** Rust workspace (Cargo), Git, shell commands for directory moves

---

### Task 1: Create group directories and move crates

**Files:**
- Create directories: `crates/engines/`, `crates/data/`, `crates/services/`, `crates/workers/`, `crates/apps/`
- Move 17 crate directories (core stays in place)

**Step 1: Create group directories**

```bash
mkdir -p crates/engines crates/data crates/services crates/workers crates/apps
```

**Step 2: Move engine crates**

```bash
git mv crates/engine-crypto crates/engines/crypto
git mv crates/engine-distributed crates/engines/distributed
git mv crates/engine-lean crates/engines/lean
```

**Step 3: Move data crates**

```bash
git mv crates/evidence crates/data/evidence
git mv crates/findings crates/data/findings
git mv crates/project-ir crates/data/project-ir
git mv crates/session-store crates/data/session-store
```

**Step 4: Move service crates**

```bash
git mv crates/intake crates/services/intake
git mv crates/knowledge crates/services/knowledge
git mv crates/llm crates/services/llm
git mv crates/report crates/services/report
git mv crates/sandbox crates/services/sandbox
```

**Step 5: Move worker crates**

```bash
git mv crates/worker-protocol crates/workers/protocol
git mv crates/worker-runner crates/workers/runner
```

**Step 6: Move app crates**

```bash
git mv crates/cli crates/apps/cli
git mv crates/orchestrator crates/apps/orchestrator
git mv crates/tauri-ui crates/apps/tauri-ui
```

---

### Task 2: Update workspace Cargo.toml

**Files:**
- Modify: `Cargo.toml` (root)

**Step 1: Replace members list**

Change the `members` array to:

```toml
members = [
    "crates/core",
    "crates/engines/crypto",
    "crates/engines/distributed",
    "crates/engines/lean",
    "crates/data/evidence",
    "crates/data/findings",
    "crates/data/project-ir",
    "crates/data/session-store",
    "crates/services/intake",
    "crates/services/knowledge",
    "crates/services/llm",
    "crates/services/report",
    "crates/services/sandbox",
    "crates/workers/protocol",
    "crates/workers/runner",
    "crates/apps/cli",
    "crates/apps/orchestrator",
    "crates/apps/tauri-ui",
]
```

---

### Task 3: Update engine crate Cargo.toml dependency paths

**Files:**
- Modify: `crates/engines/crypto/Cargo.toml`
- Modify: `crates/engines/distributed/Cargo.toml`
- Modify: `crates/engines/lean/Cargo.toml`

**Step 1: Update engines/crypto/Cargo.toml**

| Old | New |
|-----|-----|
| `path = "../core"` | `path = "../../core"` |
| `path = "../evidence"` | `path = "../../data/evidence"` |
| `path = "../intake"` | `path = "../../services/intake"` |
| `path = "../llm"` | `path = "../../services/llm"` |
| `path = "../sandbox"` | `path = "../../services/sandbox"` |

**Step 2: Update engines/distributed/Cargo.toml**

| Old | New |
|-----|-----|
| `path = "../core"` | `path = "../../core"` |
| `path = "../engine-crypto"` | `path = "../crypto"` |
| `path = "../intake"` | `path = "../../services/intake"` |
| `path = "../llm"` | `path = "../../services/llm"` |

Note: `intake` appears in both `[dependencies]` and `[dev-dependencies]`.

**Step 3: Update engines/lean/Cargo.toml**

| Old | New |
|-----|-----|
| `path = "../core"` | `path = "../../core"` |
| `path = "../llm"` | `path = "../../services/llm"` |

---

### Task 4: Update data crate Cargo.toml dependency paths

**Files:**
- Modify: `crates/data/evidence/Cargo.toml`
- Modify: `crates/data/findings/Cargo.toml`
- Modify: `crates/data/project-ir/Cargo.toml`
- Modify: `crates/data/session-store/Cargo.toml`

All four have a single workspace dependency:

| Old | New |
|-----|-----|
| `path = "../core"` | `path = "../../core"` |

---

### Task 5: Update service crate Cargo.toml dependency paths

**Files:**
- Modify: `crates/services/intake/Cargo.toml`
- Modify: `crates/services/llm/Cargo.toml`
- Modify: `crates/services/report/Cargo.toml`
- Modify: `crates/services/sandbox/Cargo.toml`

**Step 1: Update services/intake/Cargo.toml**

| Old | New |
|-----|-----|
| `path = "../core"` | `path = "../../core"` |

**Step 2: Update services/llm/Cargo.toml**

| Old | New |
|-----|-----|
| `path = "../core"` | `path = "../../core"` |
| `path = "../sandbox"` | `path = "../sandbox"` |

Note: `sandbox` is a sibling within `services/`, so path stays `../sandbox`.

**Step 3: Update services/report/Cargo.toml**

| Old | New |
|-----|-----|
| `path = "../core"` | `path = "../../core"` |
| `path = "../findings"` | `path = "../../data/findings"` |
| `path = "../llm"` | `path = "../llm"` |
| `path = "../sandbox"` | `path = "../sandbox"` |

Note: `llm` and `sandbox` are siblings within `services/`.

**Step 4: Update services/sandbox/Cargo.toml**

| Old | New |
|-----|-----|
| `path = "../core"` | `path = "../../core"` |
| `path = "../worker-protocol"` | `path = "../../workers/protocol"` |

**Step 5: knowledge/Cargo.toml — no workspace deps, skip**

---

### Task 6: Update worker crate Cargo.toml dependency paths

**Files:**
- Modify: `crates/workers/runner/Cargo.toml`

**Step 1: Update workers/runner/Cargo.toml**

| Old | New |
|-----|-----|
| `path = "../worker-protocol"` | `path = "../protocol"` |

Note: `protocol` is a sibling within `workers/`.

---

### Task 7: Update app crate Cargo.toml dependency paths

**Files:**
- Modify: `crates/apps/cli/Cargo.toml`
- Modify: `crates/apps/orchestrator/Cargo.toml`
- Modify: `crates/apps/tauri-ui/Cargo.toml`

**Step 1: Update apps/cli/Cargo.toml**

| Old | New |
|-----|-----|
| `path = "../core"` | `path = "../../core"` |
| `path = "../engine-crypto"` | `path = "../../engines/crypto"` |
| `path = "../engine-distributed"` | `path = "../../engines/distributed"` |
| `path = "../intake"` | `path = "../../services/intake"` |
| `path = "../llm"` | `path = "../../services/llm"` |
| `path = "../orchestrator"` | `path = "../orchestrator"` |

Note: `orchestrator` is a sibling within `apps/`.

**Step 2: Update apps/orchestrator/Cargo.toml**

`[dependencies]`:

| Old | New |
|-----|-----|
| `path = "../core"` | `path = "../../core"` |
| `path = "../engine-crypto"` | `path = "../../engines/crypto"` |
| `path = "../engine-distributed"` | `path = "../../engines/distributed"` |
| `path = "../engine-lean"` | `path = "../../engines/lean"` |
| `path = "../findings"` | `path = "../../data/findings"` |
| `path = "../intake"` | `path = "../../services/intake"` |
| `path = "../llm"` | `path = "../../services/llm"` |
| `path = "../report"` | `path = "../../services/report"` |
| `path = "../session-store"` | `path = "../../data/session-store"` |

`[dev-dependencies]`:

| Old | New |
|-----|-----|
| `path = "../sandbox"` | `path = "../../services/sandbox"` |

**Step 3: Update apps/tauri-ui/Cargo.toml**

| Old | New |
|-----|-----|
| `path = "../core"` | `path = "../../core"` |
| `path = "../intake"` | `path = "../../services/intake"` |
| `path = "../knowledge"` | `path = "../../services/knowledge"` |
| `path = "../orchestrator"` | `path = "../orchestrator"` |
| `path = "../project-ir"` | `path = "../../data/project-ir"` |
| `path = "../session-store"` | `path = "../../data/session-store"` |

Note: `orchestrator` is a sibling within `apps/`.

---

### Task 8: Update ui/src-tauri/Cargo.toml

**Files:**
- Modify: `ui/src-tauri/Cargo.toml`

**Step 1: Update paths**

| Old | New |
|-----|-----|
| `path = "../../crates/tauri-ui"` | `path = "../../crates/apps/tauri-ui"` |
| `path = "../../crates/core"` | `path = "../../crates/core"` (unchanged) |

---

### Task 9: Update test files with hardcoded paths

**Files:**
- Modify: `crates/data/project-ir/tests/rust_graph_tests.rs`
- Modify: `crates/services/llm/tests/direct_complete_lint_tests.rs`

**Step 1: Update project-ir test fixture paths**

Replace (2 occurrences):
```
"crates/engine-crypto/tests/fixtures/rust-crypto"
```
With:
```
"crates/engines/crypto/tests/fixtures/rust-crypto"
```

**Step 2: Update llm lint test allowed paths**

Replace:
```rust
let is_allowed = normalized.ends_with("/crates/llm/src/provider.rs")
    || normalized.ends_with("/crates/llm/tests/provider_tests.rs")
    || normalized.ends_with("/crates/llm/tests/direct_complete_lint_tests.rs");
```
With:
```rust
let is_allowed = normalized.ends_with("/crates/services/llm/src/provider.rs")
    || normalized.ends_with("/crates/services/llm/tests/provider_tests.rs")
    || normalized.ends_with("/crates/services/llm/tests/direct_complete_lint_tests.rs");
```

---

### Task 10: Update UI fallback data

**Files:**
- Modify: `ui/src/ipc/commands.ts`

**Step 1: Update fallback paths**

| Old | New |
|-----|-----|
| `"crates/tauri-ui/src/ipc.rs"` | `"crates/apps/tauri-ui/src/ipc.rs"` |

Note: `crates/core/src/session.rs` stays unchanged since core didn't move.

---

### Task 11: Update README.md workspace structure

**Files:**
- Modify: `README.md`

**Step 1: Replace the workspace structure section**

Replace the `crates/` tree in the "Workspace Structure" section with:
```
crates/
  core/              shared types: AuditConfig, Finding, AuditManifest, engine traits
  engines/
    crypto/          crypto-misuse rules, Circom signal graph, Z3 checker, Kani scaffolder, Halo2 CDG
    distributed/     MadSim feasibility, chaos scripts, invariant monitor, economic attack checker
    lean/            Lean formal verification engine (AXLE integration)
  data/
    evidence/        evidence store (zip pack)
    findings/        SARIF/JSON export, deduplication pipeline
    project-ir/      project intermediate representation (multi-framework graph)
    session-store/   SQLite-backed session persistence
  services/
    intake/          source resolution (git/local/archive), config parsing, framework detection
    knowledge/       domain checklists, tool playbooks, adjudicated cases
    llm/             LlmProvider trait, OpenAI/Anthropic/Ollama adapters, evidence gate
    report/          report generation (.md + .pdf), regression artifact layout
    sandbox/         container-based execution (Docker/Kani/Z3)
  workers/
    protocol/        remote worker protocol definitions
    runner/          remote worker execution runtime
  apps/
    cli/             audit-agent binary (clap)
    orchestrator/    DAG execution, finding deduplication, output production
    tauri-ui/        IPC session layer for the desktop app
```

Also update the Remote Worker Rollout section paths:
- `crates/worker-protocol` → `crates/workers/protocol`
- `crates/worker-runner` → `crates/workers/runner`

---

### Task 12: Verify with cargo check

**Step 1: Run cargo check**

```bash
cargo check --workspace
```

Expected: compiles with no errors. If errors occur, fix the path references.

---

### Task 13: Verify with cargo test

**Step 1: Run cargo test**

```bash
cargo test --workspace
```

Expected: all tests pass. Pay special attention to:
- `project-ir` tests (fixture path change)
- `llm` lint test (allowed path change)

---

### Task 14: Commit

**Step 1: Stage and commit**

```bash
git add -A
git commit -m "refactor: reorganize crates/ into domain-centric nested groups

Move 17 crates into 6 group directories (engines, data, services,
workers, apps) for better discoverability, dependency boundaries,
and team ownership. Core stays at crates/core.

Update all Cargo.toml dependency paths, workspace members,
test fixture paths, UI fallback data, and README."
```
