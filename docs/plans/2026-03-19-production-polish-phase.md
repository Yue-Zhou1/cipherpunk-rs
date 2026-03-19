# Production Polish Phase Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Turn the current v3 baseline into a production-oriented release by fixing the analysis substrate, deepening deterministic detection, adding stable finding provenance, making LLM context graph-aware, and upgrading the workstation from a graph display into a usable investigation surface.

**Architecture:** Execute this phase in four waves. Wave 1 fixes foundational substrate issues in `project-ir` and Rust semantic extraction. Wave 2 upgrades deterministic detection and provenance so findings, rules, and graph nodes share stable references. Wave 3 uses that provenance to build graph-budgeted LLM prompts and richer IR coverage for Circom. Wave 4 upgrades the Tauri workstation to support finding backtrace across review queue, graph lens, and source view. Keep research-heavy items explicitly deferred rather than half-implemented.

**Tech Stack:** Rust workspace crates, Cargo, tree-sitter, serde/serde_yaml, Tauri IPC, React + Vite + Vitest, Cytoscape.js, optional LLM providers through `services/llm`.

---

## Planning Conventions

- Recommended execution context: a dedicated git worktree.
- Preserve existing CLI and Tauri workflows while adding new behavior.
- Prefer additive schema changes with `#[serde(default)]` so existing sessions and JSON payloads remain readable.
- Do not introduce a cyclic dependency between `project-ir` and `engine-crypto`.
- Deterministic engines remain the source of truth for findings. LLM output stays advisory.
- Every task must land with tests and updated fixtures/docs where applicable.

## Release Gates

This phase is complete only when all of the following are true:

- `ProjectIrBuilder` uses real workspace metadata when possible instead of fabricating a one-member workspace.
- Rust semantic facts used by `project-ir` and crypto analysis are materially closer, with macro sites and cfg divergence surfaced in a shared, structured way.
- The rule engine executes `semantic_checks` and supports more than bare free-function name matching.
- Review items can carry stable `ir_node_ids` end-to-end through core/session, session-store, Tauri IPC, and frontend state.
- `ProjectIr` can return bounded graph neighborhoods and source-backed context snippets for prompting.
- Circom is indexed above file level in `project-ir` without coupling `project-ir` to the full solver implementation.
- The workstation can highlight graph nodes from a selected review item and keep graph/editor/review state synchronized.
- `cargo test`, `cargo test -p project-ir`, `cargo test -p engine-crypto`, `cargo test -p tauri-ui`, `cd ui && npm test`, and `cd ui && npm run build` are all green.

## Explicit Non-Goals For This Phase

- Full Rust compiler-grade macro expansion or HIR/MIR integration.
- Full Circom-to-Rust witness or variable traceability.
- Arbitrary right-click DAG mutation with free-form re-execution.
- Free-form LLM-generated YAML rules executed without deterministic validation.

Those items become follow-on work only after the substrate and provenance tasks in this plan are complete.

## Delivery Order

Implement tasks in order unless an earlier task explicitly says it can be parallelized.

1. Task 1 fixes workspace/source loading for `project-ir`.
2. Task 2 converges Rust semantic facts and usable macro/cfg surfacing.
3. Task 3 upgrades the deterministic rule engine.
4. Task 4 adds stable provenance to audit records and session persistence.
5. Task 5 expands `project-ir` neighborhood and Circom symbol coverage.
6. Task 6 builds graph-aware LLM context packing.
7. Task 7 upgrades the Tauri workstation to use that provenance interactively.

---

### Task 1: Replace Synthetic `ProjectIr` Workspace Loading

**Outcome:** `ProjectIrBuilder` uses real workspace/member/feature/dependency information for Cargo repositories and still supports non-Cargo directory inputs as a fallback path.

**Files:**
- Modify: `crates/data/project-ir/Cargo.toml`
- Modify: `crates/data/project-ir/src/lib.rs`
- Modify: `crates/services/intake/src/workspace.rs`
- Create: `crates/data/project-ir/tests/workspace_loader_tests.rs`
- Modify: `crates/apps/tauri-ui/tests/ui_ipc_tests.rs`

**Why this task exists:** `project-ir` currently fabricates a one-member `CargoWorkspace`, which drops true member boundaries, dependencies, and feature flags. That blocks realistic multi-crate provenance and makes later graph work less trustworthy.

**Implementation Steps:**

**Step 1: Write failing tests**

- Add a `project-ir` test that builds a small multi-member Cargo workspace and asserts:
  - both members are discovered
  - file nodes are created under the correct member roots
  - cfg feature nodes are preserved when present in member manifests
- Add a fallback test for a plain directory without a Cargo workspace manifest and assert `ProjectIrBuilder` still produces a usable graph instead of erroring.
- Add or extend a `tauri-ui` IPC test to confirm session-driven graph loading still works when the source is a Cargo workspace.

**Step 2: Introduce a shared loader**

- In `crates/data/project-ir/Cargo.toml`, add the missing dependency explicitly:

```toml
intake = { path = "../../services/intake" }
```

- Add a small loader helper inside `project-ir` that:
  - uses `intake::workspace::WorkspaceAnalyzer::analyze` when a root `Cargo.toml` is present
  - falls back to a synthetic single-member workspace only when the source is not a Cargo workspace
- Keep the fallback behavior explicit and documented inside the helper.

**Step 3: Switch `ProjectIrBuilder`**

- Replace the current `workspace_from_path` logic in `crates/data/project-ir/src/lib.rs`.
- Preserve current `with_value_previews` behavior and existing public API.
- Remove duplicated synthetic workspace code once the helper is in place.

**Step 4: Run targeted verification**

Run:

```bash
cargo test -p intake
cargo test -p project-ir
cargo test -p tauri-ui
```

Expected:

- New workspace loader tests pass.
- Existing graph IPC tests continue to pass.

**Step 5: Commit**

```bash
git add crates/data/project-ir/Cargo.toml crates/data/project-ir/src/lib.rs crates/services/intake/src/workspace.rs crates/data/project-ir/tests/workspace_loader_tests.rs crates/apps/tauri-ui/tests/ui_ipc_tests.rs
git commit -m "feat(project-ir): load real workspace metadata before building IR"
```

**Exit Criteria:**

- Cargo workspaces preserve actual members and feature flags in `ProjectIr`.
- Non-Cargo paths still build a graph.
- No UI IPC regressions.

---

### Task 2: Converge Rust Semantic Facts And Add Usable Macro Awareness

**Outcome:** `project-ir` and crypto analysis expose closer Rust semantic facts, with structured macro sites and cfg divergence surfaced in a way later tasks can consume.

**Files:**
- Modify: `crates/data/project-ir/src/semantic.rs`
- Modify: `crates/data/project-ir/src/rust.rs`
- Modify: `crates/engines/crypto/src/semantic/ra_client.rs`
- Modify: `crates/engines/crypto/tests/semantic_index_tests.rs`
- Modify: `crates/data/project-ir/tests/rust_graph_tests.rs`

**Why this task exists:** the codebase currently maintains two different Rust semantic paths. One is tree-sitter-based for `project-ir`; the other is a crypto-focused semantic index with macro/cfg/trait facts. Production polish needs those surfaces to stop drifting further apart.

**Implementation Steps:**

**Step 1: Write failing tests**

- Add or extend tests that assert the following facts are available in a structured form:
  - macro invocation sites
  - cfg feature divergence points
  - trait impl discovery used by Halo2 analysis
- Add a `project-ir` test that ensures macro sites are represented in the IR or attached metadata rather than discarded.

**Step 2: Define the shared fact surface**

- Choose one lightweight shared shape for Rust semantic facts, for example:
  - function symbols and calls
  - cfg feature markers
  - macro sites with file/span/name
  - optional trait impl references
- Keep the shape minimal and serializable. This phase does not need compiler-grade expansion data.

**Step 3: Upgrade the semantic collectors**

- Update `project-ir` semantic extraction to surface macro sites and cfg divergence in a structured way.
- Update the crypto semantic index so it can either emit or consume the same fact shape instead of inventing a parallel one-off representation.
- Keep macro handling as best-effort site awareness. Do not attempt full expansion.

**Step 4: Thread facts into `RustMapper`**

- Use the richer semantic facts inside `crates/data/project-ir/src/rust.rs`.
- Add IR nodes/edges or metadata sufficient for later provenance and prompting work.
- Preserve existing call/dataflow graph behavior unless tests need a deliberate adjustment.

**Step 5: Run targeted verification**

Run:

```bash
cargo test -p project-ir rust_graph_tests
cargo test -p engine-crypto semantic_index_tests
```

Then run:

```bash
cargo test -p project-ir
cargo test -p engine-crypto
```

**Step 6: Commit**

```bash
git add crates/data/project-ir/src/semantic.rs crates/data/project-ir/src/rust.rs crates/engines/crypto/src/semantic/ra_client.rs crates/engines/crypto/tests/semantic_index_tests.rs crates/data/project-ir/tests/rust_graph_tests.rs
git commit -m "feat(semantic): converge rust facts and add structured macro/cfg surfacing"
```

**Exit Criteria:**

- Macro sites are available structurally, not only as placeholder strings.
- `project-ir` and crypto analysis share materially closer semantic inputs.
- Existing Halo2 semantic tests stay green.

---

### Task 3: Upgrade The Deterministic Rule Engine

**Outcome:** the YAML engine executes `semantic_checks`, supports richer pattern kinds, and validates rule files up front.

**Files:**
- Modify: `crates/engines/crypto/src/rules/mod.rs`
- Modify: `crates/engines/crypto/tests/rule_evaluator_tests.rs`
- Modify: `crates/engines/crypto/tests/fixtures/rust-crypto/*`
- Modify: `rules/crypto-misuse/*.yaml`
- Modify: `README.md`

**Why this task exists:** this is the highest-ROI detection improvement. Right now the engine is mostly name-based matching with dormant `semantic_checks`.

**Implementation Steps:**

**Step 1: Write failing tests**

- Add tests that demonstrate:
  - `semantic_checks` are executed
  - `method_call` pattern kind works
  - at least one additional pattern kind works, for example `macro_call`, `attribute`, or `path_contains`
  - unknown pattern types or semantic check ids produce load-time errors rather than silent skips
- Update fixture expectations in the same change as the evaluator behavior change. The eight existing fixture files under `crates/engines/crypto/tests/fixtures/rust-crypto/` are part of the rule evaluator test contract, so any change in rule loading or matching behavior must update fixture files and `rule_evaluator_tests.rs` assertions in the same commit rather than as cleanup later.

**Step 2: Execute `semantic_checks`**

- Add a dispatcher in `rules/mod.rs` for a first small set of built-in semantic checks.
- Keep the initial check set narrow and deterministic. Good candidates are:
  - hardcoded key material in const/static
  - suspicious nonce initialization
  - hardcoded seed usage
- Return precise `CodeLocation` snippets for each triggered check.

**Step 3: Add richer pattern kinds**

- Extend `RulePattern` handling beyond `function_call`.
- Keep the first wave intentionally small:
  - `method_call`
  - `macro_call`
  - one path- or attribute-oriented matcher
- Do not attempt a full Semgrep/CodeQL DSL in this phase.

**Step 4: Validate rules at load time**

- Reject or clearly warn on unsupported pattern types and semantic checks.
- Keep rule loading deterministic and testable.
- Update the sample YAML rules to use the new capabilities where it adds real signal.

**Step 5: Update docs**

- Refresh the README or rule-pack docs so the supported rule schema is accurate.

**Step 6: Run targeted verification**

Run:

```bash
cargo test -p engine-crypto rule_evaluator_tests
cargo test -p engine-crypto
```

**Step 7: Commit**

```bash
git add crates/engines/crypto/src/rules/mod.rs crates/engines/crypto/tests/rule_evaluator_tests.rs crates/engines/crypto/tests/fixtures/rust-crypto rules/crypto-misuse README.md
git commit -m "feat(rules): execute semantic checks and add richer deterministic patterns"
```

**Exit Criteria:**

- Rule packs can express more than bare free-function name matches.
- Invalid rule files fail fast.
- `semantic_checks` are no longer dead configuration.

---

### Task 4: Add Stable Provenance To Audit Records And Session Persistence

**Outcome:** findings and candidate records can carry stable graph references from deterministic matches into session storage and Tauri IPC.

**Files:**
- Modify: `crates/engines/crypto/src/rules/mod.rs`
- Modify: `crates/core/src/session.rs`
- Modify: `crates/data/session-store/src/schema.rs`
- Modify: `crates/data/session-store/src/sqlite.rs`
- Modify: `crates/data/session-store/tests/session_store_tests.rs`
- Modify: `crates/apps/tauri-ui/src/ipc.rs`
- Modify: `crates/apps/tauri-ui/tests/ui_ipc_tests.rs`

**Why this task exists:** the current workstation seeds review items from heuristic hotspots. Interactive backtrace requires actual graph provenance to exist in the record model.

**Implementation Steps:**

**Step 1: Write failing tests**

- Add a rule-engine test that expects a `RuleMatch` to include `ir_node_ids` for resolvable symbol matches.
- Add a core/session serde round-trip test that includes the new provenance field on `AuditRecord`.
- Add a session-store round-trip test that persists and reloads provenance data.
- Add a `tauri-ui` IPC test asserting `load_review_queue` includes graph references when records have them.

**Step 2: Extend the data model**

- Add additive provenance fields with serde defaults:
  - `RuleMatch.ir_node_ids: Vec<String>`
  - `AuditRecord.ir_node_ids: Vec<String>`
- Be explicit about the backward-compatibility mechanism:
  - add `#[serde(default)]` to the new `ir_node_ids` fields
  - do not add a schema-versioning layer for this change
  - do not add an `ALTER TABLE` migration for existing `audit_records` rows, because `SessionStore` persists `AuditRecord` as `record_json` and older JSON blobs will deserialize into an empty vec
- If additional future-proofing is needed, add a second optional field for structured path refs, but keep it minimal in this phase.

**Step 3: Persist and expose provenance**

- Update session-store schema and read/write code.
- Update `ReviewQueueItemResponse` and any related frontend-facing payloads.
- Preserve backwards compatibility for existing rows and cached sessions via serde defaults, not SQL schema churn.

**Step 4: Replace heuristic record seeding where possible**

- Prefer generating review records from actual deterministic matches that have provenance.
- Keep the current hotspot-seeded record creation only as a fallback path when no true records exist yet.

**Step 5: Run targeted verification**

Run:

```bash
cargo test -p audit-agent-core
cargo test -p session-store
cargo test -p tauri-ui
```

**Step 6: Commit**

```bash
git add crates/engines/crypto/src/rules/mod.rs crates/core/src/session.rs crates/data/session-store/src/schema.rs crates/data/session-store/src/sqlite.rs crates/data/session-store/tests/session_store_tests.rs crates/apps/tauri-ui/src/ipc.rs crates/apps/tauri-ui/tests/ui_ipc_tests.rs
git commit -m "feat(provenance): persist graph references on audit records and review queue items"
```

**Exit Criteria:**

- Review items can carry `ir_node_ids` end-to-end.
- Existing serialized sessions still load.
- Heuristic hotspots are no longer the only basis for UI review records.

---

### Task 5: Expand `ProjectIr` Neighborhood APIs And Lift Circom Above File Level

**Outcome:** `ProjectIr` can answer bounded neighborhood queries, produce source-backed context snippets, and index Circom templates/signals rather than only files.

**Files:**
- Modify: `crates/data/project-ir/src/graph.rs`
- Modify: `crates/data/project-ir/src/lib.rs`
- Modify: `crates/data/project-ir/src/circom.rs`
- Create: `crates/data/project-ir/tests/circom_ir_tests.rs`
- Modify: `crates/data/project-ir/tests/rust_graph_tests.rs`

**Why this task exists:** graph-aware prompting and UI backtrace both need more than a flat file graph. Circom also needs to exist in the IR at a symbol level before later cross-language work becomes realistic.

**Implementation Steps:**

**Step 1: Write failing tests**

- Add tests for:
  - bounded neighborhood traversal
  - stable deduplicated node ordering or documented ordering semantics
  - context-snippet extraction capped by a supplied character budget
  - Circom template and signal indexing into the symbol graph

**Step 2: Add neighborhood APIs**

- Add `ProjectIr` helpers such as:
  - `ir_neighborhood`
  - `subgraph_for_nodes`
  - `context_snippets_for_nodes`
- Keep traversal deterministic and bounded.
- Avoid reading unbounded file content.

**Step 3: Lift Circom symbols**

- Extend `crates/data/project-ir/src/circom.rs` to emit lightweight symbol-level nodes for:
  - templates
  - signals
  - optionally components if it can be done without importing the full solver layer
- Keep full constraint solving in `engine-crypto`.
- Do not add a dependency from `project-ir` to `engine-crypto`.

**Step 4: Keep the graph model stable**

- If new node kinds or edge relations are introduced, document them in comments and tests.
- Preserve existing frontend expectations for file/feature/dataflow responses.

**Step 5: Run targeted verification**

Run:

```bash
cargo test -p project-ir rust_graph_tests
cargo test -p project-ir circom_ir_tests
cargo test -p project-ir
```

**Step 6: Commit**

```bash
git add crates/data/project-ir/src/graph.rs crates/data/project-ir/src/lib.rs crates/data/project-ir/src/circom.rs crates/data/project-ir/tests/circom_ir_tests.rs crates/data/project-ir/tests/rust_graph_tests.rs
git commit -m "feat(project-ir): add neighborhood APIs and symbol-level circom indexing"
```

**Exit Criteria:**

- `ProjectIr` can produce bounded neighborhoods and source snippets.
- Circom is represented above file level in the IR.
- No cyclic dependency is introduced.

---

### Task 6: Build Graph-Aware LLM Context Packing

**Outcome:** LLM prompting uses deterministic, graph-pruned context packs instead of raw strings or flat keyword bags whenever provenance is available.

**Files:**
- Modify: `crates/engines/crypto/src/kani/scaffolder.rs`
- Modify: `crates/services/llm/src/copilot.rs`
- Modify: `crates/services/llm/src/sanitize.rs`
- Modify: `crates/apps/tauri-ui/src/ipc.rs`
- Modify: `crates/services/llm/tests/copilot_tests.rs`
- Modify: `crates/engines/crypto/tests/kani_scaffolder_tests.rs`

**Why this task exists:** prompt quality is currently constrained by shallow string inputs and a hard character cap. This task converts the new graph/provenance work into immediate quality gains.

**Implementation Steps:**

**Step 1: Write failing tests**

- Add tests that assert:
  - graph-backed context is preferred over raw `source_context` when available
  - context is deterministically ordered and deduplicated
  - budget limits are honored without panics
  - fallback behavior works when no graph refs exist

**Step 2: Add a context packer**

- Introduce a small helper or struct to assemble prompt context from:
  - `ir_node_ids`
  - bounded graph neighborhoods
  - source snippets
  - hard char/token budget
- Keep the packer deterministic and testable.

**Step 3: Thread graph context into call sites**

- Extend `HarnessRequest` and relevant copilot entry points so they can accept graph-derived context.
- Prefer graph-backed context when available and fall back to raw strings when not.
- Keep `sanitize_prompt_input` as the final safety boundary, not the primary truncation strategy.

**Step 4: Improve toolbench context generation**

- Replace or augment the current bag-of-words context builder in `tauri-ui` IPC with graph-derived terms when a file or symbol selection is available.
- Keep graceful fallback to the current token-based behavior.

**Step 5: Run targeted verification**

Run:

```bash
cargo test -p engine-crypto kani_scaffolder_tests
cargo test -p llm copilot_tests
cargo test -p tauri-ui
```

Then run:

```bash
cargo test -p engine-crypto
cargo test -p llm
```

**Step 6: Commit**

```bash
git add crates/engines/crypto/src/kani/scaffolder.rs crates/services/llm/src/copilot.rs crates/services/llm/src/sanitize.rs crates/apps/tauri-ui/src/ipc.rs crates/services/llm/tests/copilot_tests.rs crates/engines/crypto/tests/kani_scaffolder_tests.rs
git commit -m "feat(llm): build deterministic graph-aware context packs for prompts"
```

**Exit Criteria:**

- Prompt context uses graph provenance when available.
- Prompt assembly remains deterministic and bounded.
- Existing no-LLM or template fallback flows continue to work.

---

### Task 7: Upgrade The Tauri Workstation To Support Finding Backtrace

**Outcome:** selecting a review item highlights the relevant graph nodes, updates workstation state, and navigates the analyst toward the associated source context.

**Files:**
- Modify: `crates/apps/tauri-ui/src/ipc.rs`
- Modify: `ui/src/ipc/commands.ts`
- Modify: `ui/src/features/workstation/GraphLens.tsx`
- Modify: `ui/src/features/workstation/ReviewQueue.tsx`
- Modify: `ui/src/features/workstation/WorkstationShell.tsx`
- Modify: `ui/src/features/workstation/CodeEditorPane.tsx`
- Modify: `ui/src/features/workstation/GraphLens.test.tsx`
- Modify: `ui/src/features/workstation/ReviewQueue.test.tsx`
- Modify: `ui/src/features/workstation/WorkstationShell.test.tsx`

**Why this task exists:** the current DAG is mostly a display surface. The workstation becomes materially more useful once provenance can drive synchronized graph/editor/review interactions.

**Implementation Steps:**

**Step 1: Write failing frontend tests**

- Add tests covering:
  - review item selection updates the current graph selection state
  - `GraphLens` accepts selected node IDs and applies highlight styling
  - workstation state survives selection changes without breaking existing loading/error flows

**Step 2: Extend frontend IPC types**

- Add `irNodeIds` and any additional provenance fields to the TypeScript command layer.
- Keep field names consistent with Tauri camelCase serialization.

**Step 3: Add workstation selection state**

- Store selected record id and selected graph node ids in `WorkstationShell` or the existing session state hook.
- Thread that state into `ReviewQueue`, `GraphLens`, and `CodeEditorPane`.

**Step 4: Add graph highlighting and navigation**

- Update `GraphLens` to:
  - highlight selected nodes
  - optionally fit/center the view on selection
  - preserve current redaction and lens-switch behavior
- Expose symbol/framework lenses if backend coverage is ready from earlier tasks.

**Step 5: Connect source navigation**

- When a selected record has a clear file location, update the active editor selection or file pane to match.
- Keep this additive. Do not break normal file browsing.

**Step 6: Run frontend verification**

Run:

```bash
cd ui && npm test
cd ui && npm run build
```

Then run:

```bash
cargo test -p tauri-ui
```

**Step 7: Commit**

```bash
git add crates/apps/tauri-ui/src/ipc.rs ui/src/ipc/commands.ts ui/src/features/workstation/GraphLens.tsx ui/src/features/workstation/ReviewQueue.tsx ui/src/features/workstation/WorkstationShell.tsx ui/src/features/workstation/CodeEditorPane.tsx ui/src/features/workstation/GraphLens.test.tsx ui/src/features/workstation/ReviewQueue.test.tsx ui/src/features/workstation/WorkstationShell.test.tsx
git commit -m "feat(workstation): add finding backtrace across review queue, graph lens, and editor"
```

**Exit Criteria:**

- Selecting a review item highlights the relevant graph nodes.
- The workstation can steer the analyst toward the matching file/snippet.
- Frontend tests and production build stay green.

---

## Deferred Follow-On Track

These items are intentionally outside this phase and should be captured as separate design or implementation plans after the above tasks land:

- Full Rust macro expansion suitable for constraint extraction in macro-heavy Halo2 code.
- Full Circom-to-Rust cross-language traceability at witness/signal/variable level.
- Dynamic rule generation from whitepapers or protocol docs, beyond a constrained invariant template system.
- Bounded re-run or "what-if" tooling for existing evidence artifacts.
- Arbitrary graph-node mutation and live backend re-analysis.

If the team wants to pursue the last two items, first write a separate design doc that narrows them to bounded, deterministic "rerun with controlled perturbation" behavior instead of open-ended sandbox mutation.

---

## Final Verification

After all seven tasks are complete, run the full matrix:

```bash
cargo test -p audit-agent-core
cargo test -p intake
cargo test -p project-ir
cargo test -p engine-crypto
cargo test -p session-store
cargo test -p llm
cargo test -p tauri-ui
cargo test
cd ui && npm test
cd ui && npm run build
```

All commands must pass before the phase is considered complete.

---

## Dependency Graph

```text
Task 1 (workspace loading)
  -> prerequisite for Task 5

Task 2 (semantic facts)
  -> prerequisite for Task 3
  -> recommended before Task 5 to avoid IR model drift, but not a hard blocker

Task 3 (rule engine)
  -> prerequisite for Task 4

Task 4 (provenance)
  -> prerequisite for Task 6
  -> prerequisite for Task 7

Task 5 (IR neighborhood + Circom symbols)
  -> prerequisite for Task 6
  -> strengthens Task 7

Task 6 (graph-aware LLM context)
  -> can begin only after Tasks 4 and 5

Task 7 (workstation backtrace)
  -> can begin only after Task 4
  -> benefits from Task 5 being complete
```

## Recommended Merge Strategy

- Land Task 1 alone.
- Land Task 2 before opening a Task 3 branch.
- Task 5 can begin after Task 1, but it should not be merged before Task 2 if both are modifying shared `project-ir` model assumptions.
- Land Task 4 before starting meaningful UI work.
- Land Task 5 before Task 6.
- Keep Task 7 as the final user-facing capstone for the phase.
