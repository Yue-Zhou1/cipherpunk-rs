# Crates Directory Restructuring Design

**Date:** 2026-03-16
**Branch:** refactor-crates

## Problem

The `crates/` directory contains 18 crates at a flat level, making it hard to:
- Discover where functionality lives
- Understand dependency boundaries
- Assign team ownership
- Minimize unnecessary recompilation

## Approach: Domain-Centric Nested Grouping

Group crates into 6 nested directories by domain purpose.

### New Layout

```
crates/
  core/                          # audit-agent-core — foundational types & traits
  engines/
    crypto/                      # engine-crypto
    distributed/                 # engine-distributed
    lean/                        # engine-lean
  data/
    evidence/                    # evidence
    findings/                    # findings
    project-ir/                  # project-ir
    session-store/               # session-store
  services/
    intake/                      # intake
    knowledge/                   # knowledge
    llm/                         # llm
    report/                      # report
    sandbox/                     # sandbox
  workers/
    protocol/                    # worker-protocol
    runner/                      # worker-runner
  apps/
    cli/                         # audit-agent-cli
    orchestrator/                # orchestrator
    tauri-ui/                    # tauri-ui
```

### What Changes

1. **Filesystem paths** — crates move into nested group directories
2. **Workspace Cargo.toml** — `members` list updated to new paths
3. **Dependency paths** — all `path = "../<crate>"` references updated to reflect new nesting
4. **CI/CD, docs, configs** — any file referencing old crate paths

### What Stays the Same

- All crate names in `[package].name` (e.g., `engine-crypto` stays `engine-crypto`)
- All `use` / `extern crate` statements in Rust source code
- All module structures within each crate

### Path Mapping

| Old path | New path |
|---|---|
| `crates/core` | `crates/core` |
| `crates/engine-crypto` | `crates/engines/crypto` |
| `crates/engine-distributed` | `crates/engines/distributed` |
| `crates/engine-lean` | `crates/engines/lean` |
| `crates/evidence` | `crates/data/evidence` |
| `crates/findings` | `crates/data/findings` |
| `crates/project-ir` | `crates/data/project-ir` |
| `crates/session-store` | `crates/data/session-store` |
| `crates/intake` | `crates/services/intake` |
| `crates/knowledge` | `crates/services/knowledge` |
| `crates/llm` | `crates/services/llm` |
| `crates/report` | `crates/services/report` |
| `crates/sandbox` | `crates/services/sandbox` |
| `crates/worker-protocol` | `crates/workers/protocol` |
| `crates/worker-runner` | `crates/workers/runner` |
| `crates/cli` | `crates/apps/cli` |
| `crates/orchestrator` | `crates/apps/orchestrator` |
| `crates/tauri-ui` | `crates/apps/tauri-ui` |

### Risk Mitigation

- Work on dedicated `refactor-crates` branch
- Atomic commit: move directories + update all paths together
- Verify with `cargo check --workspace` and `cargo test --workspace`
- No crate renames means zero Rust source changes
