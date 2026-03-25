# Repository Top-Level Restructure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reorganize top-level directories into a clean monorepo layout (`data/`, `deploy/`, `scripts/`, `tests/`, `docs/` subdirs) while keeping `crates/` untouched.

**Architecture:** Pure file moves + path reference updates. No logic changes. Every task is a batch of related moves followed by updating all references to those paths (Rust source, tests, CI, shell scripts, README). Each task is independently committable and testable.

**Tech Stack:** git mv, Rust (CARGO_MANIFEST_DIR path resolution), GitHub Actions YAML, Bash, Docker

**Spec:** `docs/superpowers/specs/2026-03-25-repo-restructure-design.md`

---

## File Structure

No new Rust source files. All changes are moves and edits to existing files, plus 3 new meta-files.

**New files:**
- `.env.example` — LLM env var documentation
- `SECURITY.md` — vulnerability reporting guidance
- (directories created implicitly by `git mv`: `data/`, `deploy/`, `scripts/`, `tests/`, `docs/schemas/`, `docs/design/`)

**Moved files:** see Move Map in spec.

**Modified files (path references):**
- `crates/services/knowledge/src/lib.rs`
- `crates/services/session-manager/src/state.rs`
- `crates/engines/crypto/tests/rule_evaluator_tests.rs`
- `crates/engines/distributed/tests/economic_checker_tests.rs`
- `crates/core/src/bin/generate_schemas.rs`
- `crates/core/tests/schema_compat.rs`
- `crates/core/tests/schema_snapshot_tests.rs`
- `crates/data/findings/tests/output_tests.rs`
- `crates/services/knowledge/src/bin/generate_memory_block_schemas.rs`
- `crates/services/knowledge/tests/schema_snapshot_tests.rs`
- `.github/workflows/ci.yml`
- `.github/workflows/containers.yml`
- `deploy/containers/build_and_verify.sh` (after move)
- `deploy/containers/check_version_dedup.sh` (after move)
- `scripts/start-web-http.sh` (after move)
- `deploy/docker-compose.yml` (after move)
- `.gitignore`
- `README.md`

---

### Task 1: Move `data/` directories (rules, knowledge, baselines)

**Files:**
- Move: `rules/` → `data/rules/`
- Move: `knowledge/` → `data/knowledge/`
- Move: `baselines/` → `data/baselines/`
- Modify: `crates/services/knowledge/src/lib.rs:47-48`
- Modify: `crates/services/session-manager/src/state.rs:1672`
- Modify: `crates/engines/crypto/tests/rule_evaluator_tests.rs:21`
- Modify: `crates/engines/distributed/tests/economic_checker_tests.rs:34`

- [ ] **Step 1: Move the three directories**

```bash
mkdir -p data
git mv rules data/rules
git mv knowledge data/knowledge
git mv baselines data/baselines
```

- [ ] **Step 2: Update knowledge crate source paths**

In `crates/services/knowledge/src/lib.rs`, change:
- Line 47: `"knowledge/playbooks"` → `"data/knowledge/playbooks"`
- Line 48: `"knowledge/domains"` → `"data/knowledge/domains"`

- [ ] **Step 3: Update session-manager source path**

In `crates/services/session-manager/src/state.rs`, change:
- Line 1672: `"rules/crypto-misuse"` → `"data/rules/crypto-misuse"`

- [ ] **Step 4: Update crypto engine test path**

In `crates/engines/crypto/tests/rule_evaluator_tests.rs`, change:
- Line 21: `"rules/crypto-misuse"` → `"data/rules/crypto-misuse"`

- [ ] **Step 5: Update distributed engine test path**

In `crates/engines/distributed/tests/economic_checker_tests.rs`, change:
- Line 34: `"rules/economic"` → `"data/rules/economic"`

- [ ] **Step 6: Run tests to verify**

```bash
cargo test -p knowledge --lib
cargo test -p session-manager
cargo test -p engine-crypto --test rule_evaluator_tests
cargo test -p engine-distributed --test economic_checker_tests
```

Expected: all PASS.

- [ ] **Step 7: Commit**

```bash
git add -A data/ crates/services/knowledge/src/lib.rs crates/services/session-manager/src/state.rs crates/engines/crypto/tests/rule_evaluator_tests.rs crates/engines/distributed/tests/economic_checker_tests.rs
git commit -m "Move rules/, knowledge/, baselines/ under data/ and update path references"
```

---

### Task 2: Move `deploy/` files (Dockerfile, docker-compose, containers)

**Files:**
- Move: `Dockerfile` → `deploy/Dockerfile`
- Move: `docker-compose.yml` → `deploy/docker-compose.yml`
- Move: `containers/` → `deploy/containers/`
- Modify: `deploy/docker-compose.yml` (build context)
- Modify: `deploy/containers/build_and_verify.sh` (ROOT_DIR + all container paths)
- Modify: `deploy/containers/check_version_dedup.sh` (ROOT_DIR + paths)
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/containers.yml`

- [ ] **Step 1: Move files**

```bash
mkdir -p deploy
git mv Dockerfile deploy/Dockerfile
git mv docker-compose.yml deploy/docker-compose.yml
git mv containers deploy/containers
```

- [ ] **Step 2: Fix docker-compose.yml build context**

In `deploy/docker-compose.yml`, change the `build:` line from a simple string to an object so Docker finds the Dockerfile at the new location and uses repo root as context:

Old:
```yaml
services:
  audit-agent:
    build: .
```

New:
```yaml
services:
  audit-agent:
    build:
      context: ..
      dockerfile: deploy/Dockerfile
```

- [ ] **Step 3: Fix build_and_verify.sh ROOT_DIR and paths**

In `deploy/containers/build_and_verify.sh`:

Line 4 — ROOT_DIR goes up two levels now:
- Old: `ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"`
- New: `ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"`

Line 5:
- Old: `VERSIONS_FILE="${ROOT_DIR}/containers/versions.toml"`
- New: `VERSIONS_FILE="${ROOT_DIR}/deploy/containers/versions.toml"`

Line 6:
- Old: `DIGESTS_FILE="${ROOT_DIR}/containers/image-digests.json"`
- New: `DIGESTS_FILE="${ROOT_DIR}/deploy/containers/image-digests.json"`

Lines 85, 92, 98, 104, 110 — all Dockerfile references:
- Old: `"${ROOT_DIR}/containers/<tool>/Dockerfile"`
- New: `"${ROOT_DIR}/deploy/containers/<tool>/Dockerfile"`

The 5 tools are: `kani`, `z3`, `madsim`, `miri`, `fuzz`.

- [ ] **Step 4: Fix check_version_dedup.sh ROOT_DIR and paths**

In `deploy/containers/check_version_dedup.sh`:

Line 4:
- Old: `ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"`
- New: `ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"`

Line 5:
- Old: `VERSIONS_FILE="${ROOT_DIR}/containers/versions.toml"`
- New: `VERSIONS_FILE="${ROOT_DIR}/deploy/containers/versions.toml"`

Line 21:
- Old: `grep -R --line-number --fixed-strings "${version}" "${ROOT_DIR}/containers"`
- New: `grep -R --line-number --fixed-strings "${version}" "${ROOT_DIR}/deploy/containers"`

Line 30:
- Old: `echo "Version strings are centralized in containers/versions.toml"`
- New: `echo "Version strings are centralized in deploy/containers/versions.toml"`

- [ ] **Step 5: Update CI workflow — ci.yml**

In `.github/workflows/ci.yml`:

Containers job cache key:
- Old: `key: ${{ runner.os }}-buildx-${{ hashFiles('containers/**') }}`
- New: `key: ${{ runner.os }}-buildx-${{ hashFiles('deploy/containers/**') }}`

Containers job scripts (2 lines):
- Old: `run: bash containers/check_version_dedup.sh`
- New: `run: bash deploy/containers/check_version_dedup.sh`
- Old: `run: bash containers/build_and_verify.sh`
- New: `run: bash deploy/containers/build_and_verify.sh`

- [ ] **Step 6: Update CI workflow — containers.yml**

In `.github/workflows/containers.yml`:

Path triggers (both push and pull_request):
- Old: `- "containers/**"`
- New: `- "deploy/containers/**"`

Also add the workflow itself to triggers:
- Old: `- ".github/workflows/containers.yml"` (keep as-is)

Script paths (2 lines):
- Old: `run: bash containers/check_version_dedup.sh`
- New: `run: bash deploy/containers/check_version_dedup.sh`
- Old: `run: bash containers/build_and_verify.sh`
- New: `run: bash deploy/containers/build_and_verify.sh`

- [ ] **Step 7: Verify check_version_dedup.sh runs**

```bash
bash deploy/containers/check_version_dedup.sh
```

Expected: `Version strings are centralized in deploy/containers/versions.toml`

- [ ] **Step 8: Commit**

```bash
git add -A deploy/ .github/workflows/ci.yml .github/workflows/containers.yml
git commit -m "Move Dockerfile, docker-compose, containers/ under deploy/ and fix all paths"
```

---

### Task 3: Move scripts and regression tests

**Files:**
- Move: `start-web-http.sh` → `scripts/start-web-http.sh`
- Move: `regression-tests/` → `tests/regression/`
- Modify: `scripts/start-web-http.sh` (ROOT_DIR fix)

- [ ] **Step 1: Move files**

```bash
mkdir -p scripts tests
git mv start-web-http.sh scripts/start-web-http.sh
git mv regression-tests tests/regression
```

- [ ] **Step 2: Fix start-web-http.sh ROOT_DIR**

In `scripts/start-web-http.sh`, line 4:

- Old: `ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"`
- New: `ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"`

- [ ] **Step 3: Verify the script resolves correctly**

```bash
bash -x scripts/start-web-http.sh 2>&1 | head -5
```

Expected: ROOT_DIR should resolve to the repo root (not `scripts/`). The script will try to start servers — just confirm ROOT_DIR is correct then Ctrl+C.

- [ ] **Step 4: Commit**

```bash
git add -A scripts/ tests/
git commit -m "Move start-web-http.sh to scripts/ and regression-tests/ to tests/regression/"
```

---

### Task 4: Reorganize docs/ (schemas + design subdirs)

**Files:**
- Move: `docs/*.json` (4 files) → `docs/schemas/`
- Move: `docs/implementation-plan-v1.md` → `docs/design/`
- Move: `docs/implementation-plan-v2.md` → `docs/design/`
- Move: `docs/ui-design.md` → `docs/design/`
- Move: `docs/plans/` → `docs/design/plans/`
- Modify: `crates/core/src/bin/generate_schemas.rs:13`
- Modify: `crates/core/tests/schema_compat.rs:18,27`
- Modify: `crates/core/tests/schema_snapshot_tests.rs:18,27`
- Modify: `crates/data/findings/tests/output_tests.rs:166`
- Modify: `crates/services/knowledge/src/bin/generate_memory_block_schemas.rs:20`
- Modify: `crates/services/knowledge/tests/schema_snapshot_tests.rs:24,33`

- [ ] **Step 1: Move JSON schemas into docs/schemas/**

```bash
mkdir -p docs/schemas
git mv docs/finding-schema.json docs/schemas/
git mv docs/audit-yaml-schema.json docs/schemas/
git mv docs/memory-block-vulnerability-signature-schema.json docs/schemas/
git mv docs/memory-block-artifact-metadata-schema.json docs/schemas/
```

- [ ] **Step 2: Move design docs into docs/design/**

```bash
mkdir -p docs/design
git mv docs/implementation-plan-v1.md docs/design/
git mv docs/implementation-plan-v2.md docs/design/
git mv docs/ui-design.md docs/design/
git mv docs/plans docs/design/plans
```

- [ ] **Step 3: Update generate_schemas.rs**

In `crates/core/src/bin/generate_schemas.rs`, line 13:
- Old: `let docs_dir = repo_root.join("docs");`
- New: `let docs_dir = repo_root.join("docs/schemas");`

- [ ] **Step 4: Update core schema_compat.rs**

In `crates/core/tests/schema_compat.rs`:
- Line 18: `"docs/finding-schema.json"` → `"docs/schemas/finding-schema.json"`
- Line 27: `"docs/audit-yaml-schema.json"` → `"docs/schemas/audit-yaml-schema.json"`

- [ ] **Step 5: Update core schema_snapshot_tests.rs**

In `crates/core/tests/schema_snapshot_tests.rs`:
- Line 18: `"docs/finding-schema.json"` → `"docs/schemas/finding-schema.json"`
- Line 27: `"docs/audit-yaml-schema.json"` → `"docs/schemas/audit-yaml-schema.json"`

- [ ] **Step 6: Update findings output_tests.rs**

In `crates/data/findings/tests/output_tests.rs`, line 166:
- Old: `repo_root.join("docs/finding-schema.json")`
- New: `repo_root.join("docs/schemas/finding-schema.json")`

- [ ] **Step 7: Update knowledge generate_memory_block_schemas.rs**

In `crates/services/knowledge/src/bin/generate_memory_block_schemas.rs`, line 20:
- Old: `let docs_dir = repo_root.join("docs");`
- New: `let docs_dir = repo_root.join("docs/schemas");`

- [ ] **Step 8: Update knowledge schema_snapshot_tests.rs**

In `crates/services/knowledge/tests/schema_snapshot_tests.rs`:
- Line 24: `"docs/memory-block-vulnerability-signature-schema.json"` → `"docs/schemas/memory-block-vulnerability-signature-schema.json"`
- Line 33: `"docs/memory-block-artifact-metadata-schema.json"` → `"docs/schemas/memory-block-artifact-metadata-schema.json"`

- [ ] **Step 9: Run all affected tests**

```bash
cargo test -p audit-agent-core --test schema_compat
cargo test -p audit-agent-core --test schema_snapshot_tests
cargo test -p findings --test output_tests
cargo test -p knowledge --test schema_snapshot_tests
```

Expected: all PASS.

- [ ] **Step 10: Commit**

```bash
git add -A docs/ crates/core/src/bin/generate_schemas.rs crates/core/tests/schema_compat.rs crates/core/tests/schema_snapshot_tests.rs crates/data/findings/tests/output_tests.rs crates/services/knowledge/src/bin/generate_memory_block_schemas.rs crates/services/knowledge/tests/schema_snapshot_tests.rs
git commit -m "Reorganize docs/ into schemas/ and design/ subdirs, update all path references"
```

---

### Task 5: Add new meta-files (.env.example, SECURITY.md, .gitignore updates)

**Files:**
- Create: `.env.example`
- Create: `SECURITY.md`
- Modify: `.gitignore`

- [ ] **Step 1: Create .env.example**

```bash
cat > .env.example << 'ENVEOF'
# ── LLM Provider ─────────────────────────────────────────
# Options: openai | anthropic | ollama | template (default)
# When unset or "template", all engines still run — LLM only
# enhances scaffolding, search hints, and prose polish.
LLM_PROVIDER=template

# ── OpenAI ───────────────────────────────────────────────
# OPENAI_API_KEY=sk-...
# OPENAI_MODEL=gpt-4o-mini
# OPENAI_BASE_URL=https://api.openai.com

# ── Anthropic ────────────────────────────────────────────
# ANTHROPIC_API_KEY=sk-ant-...
# ANTHROPIC_MODEL=claude-3-5-sonnet
# ANTHROPIC_BASE_URL=

# ── Ollama (local) ───────────────────────────────────────
# OLLAMA_BASE_URL=http://localhost:11434
# OLLAMA_MODEL=llama3
ENVEOF
```

- [ ] **Step 2: Create SECURITY.md**

```markdown
# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | ✅        |

## Reporting a Vulnerability

If you discover a security vulnerability, please report it responsibly:

1. **Do not** open a public issue.
2. Use [GitHub Security Advisories](../../security/advisories/new) to report privately.
3. Alternatively, email **security@<your-domain>** with details.

## Response Timeline

- **Acknowledgement:** within 48 hours
- **Initial assessment:** within 1 week
- **Fix or mitigation:** best effort, typically within 30 days
```

Update the email placeholder to your actual contact.

- [ ] **Step 3: Update .gitignore**

Append to `.gitignore`:

```gitignore

# Environment
.env
.env.*
!.env.example

# Frontend build output
ui/dist/
```

- [ ] **Step 4: Remove ui/dist/ from git tracking (if tracked)**

```bash
git ls-files ui/dist/ | head -5
```

If files are listed:
```bash
git rm -r --cached ui/dist/
```

If no files listed, skip — already untracked.

- [ ] **Step 5: Commit**

```bash
git add .env.example SECURITY.md .gitignore
git commit -m "Add .env.example, SECURITY.md, and update .gitignore for env files and ui/dist"
```

---

### Task 6: Update README.md

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update Workspace Structure section**

Replace the tree diagram (lines ~15-43) with the new layout reflecting `data/`, `deploy/`, `scripts/`, `tests/`, `docs/schemas/`, `docs/design/`.

- [ ] **Step 2: Update CLI example paths**

- Line ~116: `--baseline baselines/template-fallback.json` → `--baseline data/baselines/template-fallback.json`
- Line ~121: `--compare baselines/template-fallback.json` → `--compare data/baselines/template-fallback.json`

- [ ] **Step 3: Update Crypto Rule Schema section**

- Line ~223: `rules/crypto-misuse/*.yaml` → `data/rules/crypto-misuse/*.yaml`

- [ ] **Step 4: Update Remote Worker Rollout link**

- Line ~260: `[docs/plans/2026-03-12-v3-rollout-checklist.md](docs/plans/2026-03-12-v3-rollout-checklist.md)` → `[docs/design/plans/2026-03-12-v3-rollout-checklist.md](docs/design/plans/2026-03-12-v3-rollout-checklist.md)`

- [ ] **Step 5: Review for any other stale path references**

Search README.md for any remaining references to old paths: `containers/`, `baselines/`, `rules/`, `knowledge/`, `regression-tests/`, `docs/plans/`, `start-web-http.sh`, `Dockerfile`, `docker-compose`. Update any found.

Note: line ~175 `regression-tests/` in the Output Artifacts table describes generated output artifacts (under `audit-output/`), NOT source paths — leave unchanged.

- [ ] **Step 6: Commit**

```bash
git add README.md
git commit -m "Update README.md paths to reflect new directory layout"
```

---

### Task 7: Full validation

- [ ] **Step 1: Run full workspace tests**

```bash
cargo test --workspace
```

Expected: all PASS. This validates every Rust path reference was updated correctly.

- [ ] **Step 2: Run clippy**

```bash
cargo clippy --workspace -- -D warnings
```

Expected: no errors.

- [ ] **Step 3: Verify frontend builds**

```bash
cd ui && npm run build
```

Expected: build succeeds (no paths in frontend reference moved files).

- [ ] **Step 4: Verify container version check script**

```bash
bash deploy/containers/check_version_dedup.sh
```

Expected: prints `Version strings are centralized in deploy/containers/versions.toml`.

- [ ] **Step 5: Spot-check top-level layout**

```bash
ls -1 --group-directories-first
```

Expected top-level directories: `.github`, `.githooks`, `crates`, `data`, `deploy`, `docs`, `scripts`, `tests`, `tools`, `ui` plus root files (`Cargo.toml`, `Cargo.lock`, `LICENSE`, `README.md`, `.env.example`, `SECURITY.md`, `.gitignore`).

No stale directories should remain: `baselines/`, `containers/`, `knowledge/`, `regression-tests/`, `rules/`. No loose `Dockerfile`, `docker-compose.yml`, `start-web-http.sh` at root.

- [ ] **Step 6: Commit validation pass (no changes expected)**

If any fixes were needed, commit them:
```bash
git add -A && git commit -m "Fix remaining path references found during validation"
```
