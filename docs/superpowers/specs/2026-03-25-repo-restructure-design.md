# Repository Top-Level Restructure

**Date:** 2026-03-25
**Scope:** Top-level directory layout only. The `crates/` workspace is untouched.
**Goal:** Clean, portfolio-ready monorepo layout for a project containing Rust workspace, React/Tauri UI, Python tooling, and LLM integration.

---

## Current Top-Level Layout (problems)

```
.audit-work/           # empty, committed — noise
baselines/             # single file, orphaned at root
containers/            # Docker tool images, sibling to root Dockerfile
Dockerfile             # web-server build, loose at root
docker-compose.yml     # loose at root
knowledge/             # YAML domain/playbook data, loose at root
regression-tests/      # single subdir, loose at root
rules/                 # YAML rule packs, loose at root
start-web-http.sh      # lone script at root
tools/pdf_foundry/     # Python project, no context for newcomers
```

**Missing files:** `.env.example`, `SECURITY.md`.
**Build artifact committed:** `ui/dist/` should be gitignored.

---

## Target Top-Level Layout

```
cipherpunk-rs/
├── crates/                        # Rust workspace (untouched)
├── ui/                            # React + Vite + Tauri frontend
├── tools/
│   └── pdf_foundry/               # Python companion tool (unchanged internally)
├── data/
│   ├── rules/                     # crypto-misuse/, economic/ YAML rule packs
│   │   ├── checklist.md
│   │   ├── crypto_rust_audit_checklist_en.html
│   │   ├── crypto-misuse/
│   │   └── economic/
│   ├── knowledge/                 # domains/, playbooks/ YAML
│   │   ├── domains/
│   │   └── playbooks/
│   └── baselines/                 # LLM eval baselines
│       └── template-fallback.json
├── deploy/
│   ├── Dockerfile                 # web-server multi-stage build
│   ├── docker-compose.yml         # local dev compose
│   └── containers/                # tool images (fuzz/, kani/, miri/, z3/, madsim/)
│       ├── versions.toml
│       ├── image-digests.json
│       ├── build_and_verify.sh
│       └── check_version_dedup.sh
├── scripts/
│   └── start-web-http.sh
├── tests/
│   └── regression/
│       └── madsim_scenarios/
├── docs/
│   ├── schemas/                   # JSON schemas (audit-yaml, finding, memory-block-*)
│   ├── design/                    # implementation plans, ui-design
│   └── superpowers/specs/         # brainstorm design specs
├── .github/                       # CI workflows
├── .githooks/                     # pre-push hook
├── Cargo.toml
├── Cargo.lock
├── LICENSE
├── README.md
├── .gitignore                     # updated with ui/dist/, .env, .env.*
├── .env.example                   # NEW — LLM provider config template
└── SECURITY.md                    # NEW — vulnerability reporting guidance
```

---

## Move Map

| Current Path | New Path |
|---|---|
| `rules/` | `data/rules/` |
| `knowledge/` | `data/knowledge/` |
| `baselines/` | `data/baselines/` |
| `Dockerfile` | `deploy/Dockerfile` |
| `docker-compose.yml` | `deploy/docker-compose.yml` |
| `containers/` | `deploy/containers/` |
| `start-web-http.sh` | `scripts/start-web-http.sh` |
| `regression-tests/` | `tests/regression/` |
| `docs/*.json` | `docs/schemas/` |
| `docs/implementation-plan-*.md`, `docs/plans/`, `docs/ui-design.md` | `docs/design/` |

**Note:** `.audit-work/` is already gitignored and is not tracked by git. It only exists as a local untracked directory — no `git rm` needed. Can be deleted locally.

**Note:** `docs/superpowers/` stays in place under `docs/` (not moved into `docs/design/`). It contains brainstorming specs that are structurally separate from implementation plans.

---

## New Files

### `.env.example`

Documents all LLM-related environment variables. Secrets are commented out so copying the file produces a safe, working default (template fallback).

```bash
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
```

### `SECURITY.md`

Short file with:
- Supported versions
- How to report vulnerabilities (email or GitHub private advisory)
- Response timeline expectation

### `.gitignore` additions

```gitignore
# Environment
.env
.env.*
!.env.example

# Frontend build output
ui/dist/
```

---

## Path Updates Required

All paths use `CARGO_MANIFEST_DIR` and walk up to repo root via `.parent()` chains. The relative offsets from each crate to repo root are unchanged (crates stay in place), so only the final `.join(...)` segments need updating.

### Rust source changes

| File | Old path | New path |
|---|---|---|
| `crates/services/knowledge/src/lib.rs:47` | `"knowledge/playbooks"` | `"data/knowledge/playbooks"` |
| `crates/services/knowledge/src/lib.rs:48` | `"knowledge/domains"` | `"data/knowledge/domains"` |
| `crates/services/session-manager/src/state.rs:1672` | `"rules/crypto-misuse"` | `"data/rules/crypto-misuse"` |

### Rust test changes

| File | Old path | New path |
|---|---|---|
| `crates/engines/crypto/tests/rule_evaluator_tests.rs:21` | `"rules/crypto-misuse"` | `"data/rules/crypto-misuse"` |
| `crates/engines/distributed/tests/economic_checker_tests.rs:34` | `"rules/economic"` | `"data/rules/economic"` |

### Schema generator / snapshot tests

| File | Old path | New path |
|---|---|---|
| `crates/core/src/bin/generate_schemas.rs:13` | `repo_root.join("docs")` | `repo_root.join("docs/schemas")` |
| `crates/core/tests/schema_compat.rs:18` | `"docs/finding-schema.json"` | `"docs/schemas/finding-schema.json"` |
| `crates/core/tests/schema_compat.rs:27` | `"docs/audit-yaml-schema.json"` | `"docs/schemas/audit-yaml-schema.json"` |
| `crates/core/tests/schema_snapshot_tests.rs:18` | `"docs/finding-schema.json"` | `"docs/schemas/finding-schema.json"` |
| `crates/core/tests/schema_snapshot_tests.rs:27` | `"docs/audit-yaml-schema.json"` | `"docs/schemas/audit-yaml-schema.json"` |
| `crates/data/findings/tests/output_tests.rs:166` | `"docs/finding-schema.json"` | `"docs/schemas/finding-schema.json"` |
| `crates/services/knowledge/src/bin/generate_memory_block_schemas.rs:20` | `repo_root.join("docs")` | `repo_root.join("docs/schemas")` |
| `crates/services/knowledge/tests/schema_snapshot_tests.rs:24` | `"docs/memory-block-vulnerability-signature-schema.json"` | `"docs/schemas/memory-block-vulnerability-signature-schema.json"` |
| `crates/services/knowledge/tests/schema_snapshot_tests.rs:33` | `"docs/memory-block-artifact-metadata-schema.json"` | `"docs/schemas/memory-block-artifact-metadata-schema.json"` |

### CI workflow changes

**`.github/workflows/ci.yml`:**
- `hashFiles('containers/**')` → `hashFiles('deploy/containers/**')`
- `bash containers/check_version_dedup.sh` → `bash deploy/containers/check_version_dedup.sh`
- `bash containers/build_and_verify.sh` → `bash deploy/containers/build_and_verify.sh`

**`.github/workflows/containers.yml`:**
- Path triggers: `containers/**` → `deploy/containers/**`
- Script paths: same as above

### Shell script path fixes

**`deploy/containers/build_and_verify.sh`:**

The script currently resolves `ROOT_DIR` via `"$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"`. When the script lived at `containers/build_and_verify.sh`, `..` resolved to repo root. After moving to `deploy/containers/`, `..` resolves to `deploy/` — **wrong**.

Required changes:
- Line 4: `/.."` → `/../.."` (go up two levels to reach repo root)
- Line 5: `${ROOT_DIR}/containers/versions.toml` → `${ROOT_DIR}/deploy/containers/versions.toml`
- Line 6: `${ROOT_DIR}/containers/image-digests.json` → `${ROOT_DIR}/deploy/containers/image-digests.json`
- Lines 85, 92, 98, 104, 110: `${ROOT_DIR}/containers/<tool>/Dockerfile` → `${ROOT_DIR}/deploy/containers/<tool>/Dockerfile`

**`deploy/containers/check_version_dedup.sh`:**

Same `ROOT_DIR` fix:
- Line 4: `/.."` → `/../.."`
- Line 5: `${ROOT_DIR}/containers/versions.toml` → `${ROOT_DIR}/deploy/containers/versions.toml`
- Line 21: `"${ROOT_DIR}/containers"` → `"${ROOT_DIR}/deploy/containers"`
- Line 30: echo message `"containers/versions.toml"` → `"deploy/containers/versions.toml"`

**`scripts/start-web-http.sh`:**

Currently at repo root, `ROOT_DIR` is set to the script's directory (no `..`). After moving to `scripts/`, this becomes `<repo>/scripts/` — breaking all `$ROOT_DIR/ui` and `$ROOT_DIR/.audit-work` references.

Required change:
- Line 4: `"$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"` → `"$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"`

### Dockerfile

**`deploy/Dockerfile`:**
- Build context will be repo root (set via `docker-compose.yml` context or `-f` flag)
- `COPY` paths already reference `ui/` and `.` which remain correct when context = repo root

### docker-compose.yml

**`deploy/docker-compose.yml`:**
- Add `build.context: ..` and `build.dockerfile: deploy/Dockerfile` so the build context is repo root

---

## README.md Update

Update **all sections** that reference moved paths. The affected sections are:

- **Workspace Structure** — replace the tree diagram with the new layout
- **Quick Start / CLI examples** — `baselines/template-fallback.json` → `data/baselines/template-fallback.json` (lines ~116, ~121)
- **Crypto Rule Schema** — `rules/crypto-misuse/*.yaml` → `data/rules/crypto-misuse/*.yaml` (line ~223)
- **Docker/deploy references** — if any `Dockerfile` or `docker-compose` paths are mentioned, update to `deploy/`

---

## Validation

After all moves and path updates:

1. `cargo test --workspace` — all Rust tests pass (covers all schema, rules, knowledge path references)
2. `cargo clippy --workspace -- -D warnings` — no lint errors
3. `cd ui && npm run build` — frontend builds
4. `bash deploy/containers/build_and_verify.sh` — container builds pass (requires Docker; skip if unavailable)
5. `bash scripts/start-web-http.sh` — verify ROOT_DIR resolves correctly (smoke test: does it find `ui/` and `.audit-work`?)
6. CI workflow diff review — confirm all `containers/**` → `deploy/containers/**` path triggers and script references are updated

---

## Out of Scope

- Internal `crates/` structure (explicitly excluded)
- Renaming crate package names or Cargo.toml entries
- Any `regression-tests/` references inside `audit-output/` (those are generated output paths, not source paths)
- `tools/pdf_foundry/` internal structure
