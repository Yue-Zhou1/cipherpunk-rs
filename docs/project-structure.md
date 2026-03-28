# Project Structure

> 24-crate Rust workspace for security auditing across crypto/ZK, distributed consensus, and formal verification domains.

## High-Level Overview

```
cipherpunk-rs/
├── crates/                  24 Rust crates (core, engines, data, services, apps)
├── ui/                      React + Tauri v2 frontend
├── tools/                   Companion tools (pdf_foundry, etc.)
├── data/                    Rules, knowledge bases, LLM baselines
│   ├── rules/               YAML rule packs (crypto-misuse, economic)
│   ├── knowledge/           Domain checklists + tool playbooks
│   └── baselines/           LLM evaluation baselines
├── deploy/                  Docker configuration
├── docs/                    Design docs, JSON schemas
├── tests/                   Regression tests
├── scripts/                 Utility scripts
├── Cargo.toml               Workspace root
└── Cargo.lock               Dependency lock
```

## Crate Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              cipherpunk-rs                                  │
│                     Modular Rust Security Audit Platform                    │
│                        24 crates · Rust 2024 Edition                        │
└──────────────────────────────────┬──────────────────────────────────────────┘
                                   │
         ┌─────────────────────────┼─────────────────────────┐
         ▼                         ▼                         ▼
┌─────────────────┐    ┌────────────────────┐    ┌────────────────────┐
│   crates/       │    │     ui/            │    │    tools/          │
│  (Rust workspace│    │  React + Tauri v2  │    │  pdf_foundry etc.  │
│   24 crates)    │    │  frontend          │    │  companion tools   │
└────────┬────────┘    └────────────────────┘    └────────────────────┘
         │
         ├──────────────┬──────────────┬──────────────┬──────────────┐
         ▼              ▼              ▼              ▼              ▼
┌──────────────┐ ┌────────────┐ ┌────────────┐ ┌────────────┐ ┌────────────┐
│    CORE      │ │  ENGINES   │ │    DATA    │ │  SERVICES  │ │    APPS    │
│  (1 crate)   │ │ (3 crates) │ │ (4 crates) │ │ (8 crates) │ │ (4 crates) │
└──────────────┘ └────────────┘ └────────────┘ └────────────┘ └────────────┘
```

## Core

```
┌──────────────────────────────────────┐
│          audit-agent-core            │
│                                      │
│  Shared types, traits, config        │
│                                      │
│  crates/core/src/                    │
│  ├── lib.rs          Public API      │
│  ├── engine.rs       Engine traits   │
│  ├── audit_config.rs Config structs  │
│  ├── audit_yaml.rs   YAML parsing    │
│  ├── finding.rs      Finding types   │
│  ├── output.rs       Output manifest │
│  ├── schema.rs       Schema defs     │
│  ├── session.rs      Session types   │
│  ├── tooling.rs      Tool traits     │
│  ├── workspace.rs    Workspace types │
│  └── llm.rs          LLM trait       │
│                                      │
│  Exports: NoopEvidenceWriter,        │
│           NoopSandboxRunner,         │
│           LlmProvider                │
└──────────────────────────────────────┘
        ▲
        │  (all other crates depend on core)
```

## Engines

```
┌───────────────────────┐  ┌───────────────────────┐  ┌───────────────────────┐
│    engine-crypto      │  │  engine-distributed   │  │     engine-lean       │
│                       │  │                       │  │                       │
│ Crypto & ZK analysis  │  │ Consensus & economic  │  │ Lean formal verif.    │
│                       │  │ attack analysis       │  │ with AXLE integration │
│ Modules:              │  │                       │  │                       │
│ • intake_bridge       │  │ Modules:              │  │ Modules:              │
│ • kani (scaffolding)  │  │ • chaos (scripts)     │  │ • client              │
│ • rules (matching)    │  │ • economic (attacks)  │  │ • scaffold            │
│ • semantic (analysis) │  │ • feasibility         │  │ • tool_actions        │
│ • supply_chain        │  │   (MadSim)            │  │ • types               │
│ • tool_actions        │  │ • invariants          │  │                       │
│ • zk (Circom, Halo2)  │  │ • trace               │  │ Deps:                 │
│                       │  │ • verification        │  │  core, llm, reqwest   │
│ Deps:                 │  │ • tool_actions        │  │                       │
│  core, evidence,      │  │                       │  └───────────────────────┘
│  intake, llm,         │  │ Deps:                 │
│  sandbox, tree-sitter │  │  core, engine-crypto, │
│                       │  │  intake, llm          │
└───────────┬───────────┘  └───────────┬───────────┘
            │                          │
            └────────────┬─────────────┘
                         ▼
               (feed into orchestrator)
```

## Data Crates

```
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐  ┌─────────────────┐
│    evidence      │  │    findings      │  │   project-ir     │  │  session-store  │
│                  │  │                  │  │                  │  │                 │
│ Evidence store   │  │ Finding structs  │  │ Project IR graph │  │ SQLite-backed   │
│ + ZIP packing    │  │ + export formats │  │ (multi-language) │  │ session persist │
│                  │  │                  │  │                  │  │                 │
│ • manifest gen   │  │ • json_export    │  │ Language mappers │  │ Deps:           │
│ • reproducible   │  │ • pipeline       │  │ • Rust           │  │  core, rusqlite │
│   packaging      │  │ • sarif          │  │ • Circom         │  │  uuid, chrono   │
│                  │  │                  │  │ • Cairo          │  │                 │
│ Deps:            │  │ Deps:            │  │                  │  └─────────────────┘
│  core, tokio,    │  │  core, serde     │  │ Graph lenses:    │
│  zip             │  │                  │  │ • file / symbol  │
└──────────────────┘  └──────────────────┘  │ • feature / flow │
                                            │                  │
                                            │ Deps:            │
                                            │  core, intake,   │
                                            │  tree-sitter     │
                                            └──────────────────┘
```

## Services

```
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│     intake      │  │   knowledge     │  │      llm        │  │    sandbox      │
│                 │  │                 │  │                 │  │                 │
│ Source resolve  │  │ Domain checks,  │  │ LLM provider    │  │ Docker-based    │
│ git/local/zip   │  │ playbooks,      │  │ abstraction     │  │ execution       │
│ config parse    │  │ adjudicated     │  │                 │  │                 │
│ framework       │  │ cases           │  │ Backends:       │  │ Tools:          │
│ detection       │  │                 │  │ • OpenAI        │  │ • Kani / Z3     │
│                 │  │ • loader        │  │ • Anthropic     │  │ • Miri / Fuzz   │
│ • config        │  │ • long_term     │  │ • Ollama        │  │ • MadSim        │
│ • detection     │  │ • memory_block  │  │                 │  │ • Chaos         │
│ • diff          │  │ • working_mem   │  │ • adviser       │  │                 │
│ • source        │  │                 │  │ • contracts     │  │ • redaction     │
│ • workspace     │  │ Deps:           │  │ • copilot       │  │ • remote        │
│                 │  │  core, serde,   │  │ • enforcement   │  │                 │
│ Deps:           │  │  yaml           │  │ • sanitize      │  │ Deps:           │
│  core, git2,    │  │                 │  │ • semantic_mem  │  │  bollard, tokio │
│  sled,          │  │                 │  │                 │  │  futures        │
│  pdf-extract    │  │                 │  │ Deps:           │  │                 │
└─────────────────┘  └─────────────────┘  │  core, reqwest, │  └─────────────────┘
                                          │  sandbox        │
                                          └─────────────────┘

┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│    llm-eval     │  │     report      │  │    research     │  │session-manager  │
│                 │  │                 │  │                 │  │                 │
│ LLM evaluation  │  │ Report gen      │  │ External API    │  │ Session state   │
│ framework       │  │ (MD + PDF)      │  │ integration     │  │ orchestration   │
│                 │  │                 │  │                 │  │                 │
│ Deps:           │  │ Deps:           │  │ Deps:           │  │ Deps:           │
│  llm, serde,    │  │  core, findings │  │  reqwest,       │  │  core, engines, │
│  chrono         │  │  llm, printpdf, │  │  chrono         │  │  intake,        │
│                 │  │  sandbox        │  │                 │  │  knowledge, llm │
│                 │  │                 │  │                 │  │  project-ir,    │
│                 │  │                 │  │                 │  │  research,      │
│                 │  │                 │  │                 │  │  session-store, │
│                 │  │                 │  │                 │  │  orchestrator   │
└─────────────────┘  └─────────────────┘  └─────────────────┘  └─────────────────┘
```

## Apps (Entry Points)

```
┌──────────────────────┐  ┌──────────────────────┐
│   audit-agent-cli    │  │     orchestrator     │
│   (binary)           │  │                      │
│                      │  │  DAG execution       │
│  Commands:           │  │  engine              │
│  • analyze           │  │                      │
│  • diff              │  │  • events            │
│                      │  │  • jobs (DAG)        │
│  Deps: all major     │  │  • runtime           │
│  crates              │  │  • tool_actions      │
│                      │  │  • deduplication     │
└──────────┬───────────┘  └──────────┬───────────┘
           │                         │
           └────────────┬────────────┘
                        ▼
              ┌──────────────────┐
              │  User-facing     │
              │  interfaces      │
              └────────┬─────────┘
                       │
           ┌───────────┴───────────┐
           ▼                       ▼
┌──────────────────────┐  ┌──────────────────────┐
│      tauri-ui        │  │   audit-agent-web    │
│                      │  │                      │
│  Desktop app         │  │  Web server (Axum)   │
│  IPC bridge          │  │  API + static        │
│  Tauri <-> Rust      │  │  frontend serving    │
│                      │  │                      │
│  Deps: session-mgr,  │  │  Deps: session-mgr,  │
│   orchestrator,      │  │   tower-http         │
│   project-ir,        │  │                      │
│   knowledge          │  │                      │
└──────────────────────┘  └──────────────────────┘
```

## Dependency Flow

```
                    ┌───────────┐
                    │   core    │  <-- Foundation: types, traits, config
                    └─────┬─────┘
                          │
            ┌─────────────┼─────────────┐
            ▼             ▼             ▼
      ┌──────────┐  ┌──────────┐  ┌──────────┐
      │   DATA   │  │ SERVICES │  │ ENGINES  │
      │ evidence │  │ intake   │  │ crypto   │
      │ findings │  │ llm      │──│ distrib. │
      │ proj-ir  │  │ sandbox  │  │ lean     │
      │ session  │  │ knowledge│  └────┬─────┘
      └────┬─────┘  │ report   │       │
           │        │ research │       │
           │        └────┬─────┘       │
           │             │             │
           └──────┬──────┴─────────────┘
                  ▼
          ┌───────────────┐
          │ orchestrator  │  <-- DAG scheduler, dedup, tool dispatch
          └───────┬───────┘
                  │
          ┌───────┴───────┐
          ▼               ▼
   ┌────────────┐  ┌────────────┐
   │    CLI     │  │  tauri-ui  │  <-- User-facing apps
   │  web-srvr  │  └────────────┘
   └────────────┘
```

## Data-Driven Resources

```
data/
├── rules/                         YAML rule packs
│   ├── crypto-misuse/             Cryptographic misuse patterns
│   └── economic/                  Economic attack patterns
├── knowledge/                     Domain knowledge
│   ├── domains/                   Checklists
│   │   ├── crypto.yaml
│   │   ├── zk.yaml
│   │   ├── p2p-consensus.yaml
│   │   └── economic.yaml
│   └── playbooks/                 Tool playbooks
│       ├── rust-crypto.yaml
│       ├── lean-formal.yaml
│       ├── circom-zk.yaml
│       ├── cairo-starknet.yaml
│       └── distributed-consensus.yaml
└── baselines/                     LLM evaluation baselines

tests/regression/                  MadSim scenario fixtures
deploy/                            Docker configuration
scripts/                           Utility scripts
```
