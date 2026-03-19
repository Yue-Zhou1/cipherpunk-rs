# Cipherpunk Audit Agent

Automated security audit pipeline for Rust and ZK-focused codebases. Detects
crypto misuse, ZK constraint bugs (Circom/Halo2), distributed consensus flaws,
and economic attack surfaces — backed by formal verification evidence (Z3, Kani).

- **CLI** (`audit-agent analyze`, `audit-agent diff`) — headless pipeline.
- **Desktop app** (React + Tauri v2) — guided 6-step wizard with live DAG view.
- **Rule-driven engines** — deterministic detection from curated YAML rule packs; LLM assists search focus and prose, never decides what to report.
- **Multi-format output** — Markdown + PDF reports, JSON + SARIF findings, evidence pack with `reproduce.sh`, regression test harnesses.
- **Remote worker protocol** — optional remote execution backend for expensive sandbox jobs with signed artifact manifests.

## Workspace Structure

```
crates/
  core/                shared types: AuditConfig, Finding, AuditManifest, engine traits
  engines/
    crypto/            crypto-misuse rules, Circom signal graph, Z3 checker, Kani scaffolder, Halo2 CDG
    distributed/       MadSim feasibility, chaos scripts, invariant monitor, economic attack checker
    lean/              Lean formal verification engine (AXLE integration)
  data/
    evidence/          evidence store (zip pack)
    findings/          SARIF/JSON export, deduplication pipeline
    project-ir/        project intermediate representation (multi-framework graph)
    session-store/     SQLite-backed session persistence
  services/
    intake/            source resolution (git/local/archive), config parsing, framework detection
    knowledge/         domain checklists, tool playbooks, adjudicated cases
    llm/               LlmProvider trait, OpenAI/Anthropic/Ollama adapters, evidence gate
    report/            report generation (.md + .pdf), regression artifact layout
    sandbox/           container-based execution (Docker/Kani/Z3)
  workers/
    protocol/          remote worker protocol definitions
    runner/            remote worker execution runtime
  apps/
    cli/               audit-agent binary (clap)
    orchestrator/      DAG execution, finding deduplication, output production
    tauri-ui/          IPC session layer for the desktop app
ui/                    React + Vite frontend, Tauri v2 app shell
rules/                 YAML rule packs (crypto-misuse/, economic/)
docs/                  design docs, schemas, UI layout spec
```

## Prerequisites

- **Rust** stable toolchain (1.88+)
- **git**
- For the desktop app: **Node.js** (≥18) + **npm**, and **tauri-cli**

```bash
# install Tauri CLI (desktop app only)
cargo install tauri-cli --version '^2.0'
```

On Linux, additional system packages are required — see
[ui/src-tauri/LINUX_BOOTSTRAP.md](ui/src-tauri/LINUX_BOOTSTRAP.md).

## Quick Start (CLI)

### 1. Create a minimal `audit.yaml`

```yaml
source:
  local_path: /absolute/path/to/target-repo
engines:
  crypto_zk: true
  distributed: false
```

### 2. Run an analysis

```bash
cargo run -p audit-agent-cli -- analyze \
  --audit-yaml audit.yaml \
  --local-path /absolute/path/to/target-repo
```

Outputs land in `audit-output/` by default (override with `--output-dir`).

Other source modes:

```bash
# from a git URL (--commit is required)
cargo run -p audit-agent-cli -- analyze \
  --audit-yaml audit.yaml \
  --git-url https://github.com/org/repo \
  --commit a1b2c3d4

# from a .tar.gz / .zip archive
cargo run -p audit-agent-cli -- analyze \
  --audit-yaml audit.yaml \
  --archive /path/to/source.tar.gz
```

### 3. Run diff mode

Diff mode re-analyzes only the crates affected by a commit range and
caches results from unchanged modules:

```bash
cargo run -p audit-agent-cli -- diff \
  --repo-root /absolute/path/to/target-repo \
  --base <base_sha> \
  --head <head_sha>
```

### 4. CLI help

```bash
cargo run -p audit-agent-cli -- --help
cargo run -p audit-agent-cli -- analyze --help
cargo run -p audit-agent-cli -- diff --help
```

## Desktop App (Tauri)

```bash
cd ui
npm install
cargo tauri dev
```

This starts both the Vite dev server and the native Tauri window. The app
provides a 6-step wizard: source selection, configuration, optional inputs,
workspace confirmation, live execution, and results with export.

For frontend-only development (no Tauri IPC, uses mock data):

```bash
cd ui
npm run dev        # opens at http://localhost:5173
```

**Platform support:** Tauri is cross-platform by design. Linux system
dependencies are documented in
[ui/src-tauri/LINUX_BOOTSTRAP.md](ui/src-tauri/LINUX_BOOTSTRAP.md). macOS and
Windows should work with standard Tauri prerequisites but are not yet
explicitly documented.

## Output Artifacts

A completed analysis produces the following under the output directory
(`audit-output/` by default):

| File | Description |
|------|-------------|
| `report-executive.md` / `.pdf` | Executive summary with risk score and top findings |
| `report-technical.md` / `.pdf` | Full technical report with code snippets and reproduce commands |
| `findings.json` | Machine-readable finding list |
| `findings.sarif` | SARIF 2.1.0 for GitHub Code Scanning integration |
| `audit-manifest.json` | Audit metadata: tool versions, container digests, scope |
| `evidence-pack.zip` | Per-finding evidence (SMT2 queries, harnesses, traces, `reproduce.sh`) |
| `regression-tests/` | Generated Kani harnesses, proptest suites, MadSim scenarios |

## LLM Configuration

LLM integration is **optional**. Without credentials the system falls back to
deterministic template responses — all audit engines still run, findings are
still produced. LLM enhances three specific roles:

1. **Role 1 (Scaffolding):** syntax/type fixes in generated harnesses
2. **Role 2 (Search hints):** `kani::assume()` hints to focus model checking
3. **Role 3 (Prose):** readability polish on report recommendations

| Variable | Purpose |
|----------|---------|
| `LLM_PROVIDER` | `openai`, `anthropic`, `ollama`, or `template` (default) |
| `LLM_API_KEY` | API key for OpenAI (also used as default if provider unset) |
| `OPENAI_MODEL` | Model name (default: `gpt-4o-mini`) |
| `OPENAI_BASE_URL` | Custom endpoint (default: `https://api.openai.com`) |
| `ANTHROPIC_API_KEY` | API key for Anthropic |
| `ANTHROPIC_MODEL` | Model name (default: `claude-3-5-sonnet`) |
| `ANTHROPIC_BASE_URL` | Custom endpoint |
| `OLLAMA_BASE_URL` | Ollama server URL (e.g. `http://localhost:11434`) |
| `OLLAMA_MODEL` | Model name (default: `llama3`) |

To disable LLM prose polish in reports while keeping other LLM roles active,
pass `--no-llm-prose` to the CLI.

## Running Tests

```bash
# all Rust crates
cargo test

# CLI integration tests only
cargo test -p audit-agent-cli

# Tauri IPC layer
cargo test -p tauri-ui

# frontend (requires npm install first)
cd ui && npm test

# frontend production build check
cd ui && npm run build
```

## Crypto Rule Schema

Crypto misuse rules live under `rules/crypto-misuse/*.yaml`. The deterministic
engine validates rule files at load time and rejects unsupported fields.

Supported `detection.patterns[*].type` values:

- `function_call`
- `method_call`
- `macro_call`
- `path_contains`
- `attribute`

Supported `detection.semantic_checks[*]` values:

- `nonce_is_not_bound_to_session_id`
- `missing_domain_separator`
- `missing_canonicality_check`
- `rng_is_predictable`
- `missing_small_subgroup_check`
- `unchecked_unwrap`
- `hardcoded_secret_present`
- `unsafe_in_verification_path`
- `suspicious_nonce_initialization`
- `hardcoded_seed_usage`

Rules can combine pattern and semantic checks. Pattern matches define candidate
sites, and semantic checks are evaluated deterministically before findings are emitted.
When multiple `semantic_checks` are listed, they use AND semantics: all listed
checks must match for the rule to emit.

## Remote Worker Rollout

`v3` includes a remote execution protocol (`crates/workers/protocol`) and a basic
worker runner binary (`crates/workers/runner`). Local Docker remains the default
backend; remote execution can be enabled per sandbox runtime as rollout hardening
advances.

Manual rollout checks are documented in
[docs/plans/2026-03-12-v3-rollout-checklist.md](docs/plans/2026-03-12-v3-rollout-checklist.md).

## Troubleshooting

| Problem | Fix |
|---------|-----|
| `cargo tauri` not found | `cargo install tauri-cli --version '^2.0'` |
| Linux Tauri build fails (WebKitGTK/GTK errors) | Install packages from [LINUX_BOOTSTRAP.md](ui/src-tauri/LINUX_BOOTSTRAP.md), then `pkg-config --modversion glib-2.0 gio-2.0 gobject-2.0 gdk-3.0 cairo` |
| Frontend deps missing | `cd ui && npm install` |
| Tauri dev starts but no window | Confirm Vite is reachable at `http://localhost:5173`, then re-run `cargo tauri dev` from `ui/` |
| `--git-url` fails with commit error | `--commit` (full SHA) is required with `--git-url`. For branch names, add `--allow-branch-resolution` |
| LLM features degraded | Set `LLM_PROVIDER` and the corresponding API key env var. Without credentials, template fallback is used automatically |

## License

MIT (see [LICENSE](LICENSE)).
