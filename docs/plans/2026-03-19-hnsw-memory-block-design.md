# LLM-Driven HNSW-Ready Memory-Block Architecture

**Date:** 2026-03-19
**Status:** Proposed
**Scope:** Offline Python foundry plus optional Rust runtime retrieval

## Motivation

Cipherpunk-rs currently relies on hand-authored YAML rule packs, curated
knowledge files, and deterministic semantic checks. That is the correct source
of truth for findings, but it leaves a gap: the project cannot currently mine
published audit reports and academic papers into a reusable, structured memory
layer.

This design introduces an offline-to-runtime pipeline that:

1. extracts structured `VulnerabilitySignature` records from PDFs
2. embeds them into a portable vector corpus
3. loads that corpus at runtime for low-latency semantic retrieval
4. feeds retrieved signatures into LLM prompt assembly for harness generation
5. hands generated harnesses to Kani/Z3 for deterministic verification

The first implementation is intentionally not "full HNSW everywhere." It is
HNSW-ready. Runtime search starts with brute-force cosine similarity over
normalized vectors because the corpus is expected to be small and the current
repo does not yet have a stable cross-language ANN format.

## Architecture Overview

```text
┌──────────────────────────────────────────────────────────────┐
│  Phase 1: Python Foundry (offline, incremental)             │
│                                                              │
│  PDF -> pymupdf4llm -> Markdown -> LLM Extract ->           │
│         VulnerabilitySignature JSON                          │
│                    │                                         │
│                    v                                         │
│         Embed (ONNX default / API optional)                  │
│                    │                                         │
│                    v                                         │
│         Bundle vectors + metadata -> knowledge.bin           │
└─────────────────────────────┬────────────────────────────────┘
                              │ file on disk
                              v
┌──────────────────────────────────────────────────────────────┐
│  Phase 2: Rust Runtime (knowledge + llm integration)        │
│                                                              │
│  Startup: load knowledge.bin if present                      │
│                    │                                         │
│  IR slice / toolbench context -> embed query ->             │
│  cosine search (v1) -> top-K signatures                      │
│                    │                                         │
│                    v                                         │
│  assemble RAG prompt -> LLM propose harness ->               │
│  Kani / Z3 verify                                            │
└──────────────────────────────────────────────────────────────┘
```

Python owns offline parsing, extraction, embedding, and artifact generation.
Rust owns runtime loading, querying, prompt construction, and deterministic
handoff. The contract is a versioned `knowledge.bin` file plus a Rust-owned
schema for the metadata block.

## Design Decisions

### D1: Standalone Python tool, not a runtime dependency

The foundry lives under `tools/pdf_foundry/` as an independent Python package.
It is not PyO3, not maturin, and not part of the runtime execution path.

Reasons:

- PDF parsing and batch extraction are offline tasks, not latency-sensitive
- Python has better PDF tooling for markdown reconstruction
- keeping Python out of the CLI/Tauri runtime avoids packaging complexity
- the existing Rust PDF path in `crates/services/intake` remains the runtime
  parser for user-supplied optional inputs and is not replaced

### D2: Schema-first contract owned by Rust

Before building the foundry, define a Rust-owned `VulnerabilitySignature` and
artifact metadata schema, then generate a JSON Schema snapshot that the Python
tool validates against.

Reasons:

- the repo already treats schema stability seriously
- Python extraction quality is only useful if the runtime can trust the shape
- the metadata block must remain readable across versions without guesswork

### D3: ONNX embeddings by default, API embeddings optional

- **Default:** local ONNX model (`all-MiniLM-L6-v2`, 384 dimensions)
- **Optional:** provider-backed embeddings if explicitly configured
- **Shared config:** both Python and Rust resolve the same embedding settings:
  provider, model, dimensions, distance metric, and normalization policy

The default path should work fully offline. API embeddings are an opt-in path,
not the baseline.

### D4: Exact vector-space match is mandatory

If an artifact was built with embedding configuration `X`, runtime retrieval may
only query it with the same effective embedding configuration `X`.

That means:

- no "warn and fall back to ONNX" behavior for an API-built artifact
- no mixing completion-model config with embedding-model config
- no retrieval against mismatched dimensions or normalization rules

On mismatch, runtime semantic retrieval is disabled for that artifact and the
application continues without the memory block.

### D5: Runtime file loading, not compile-time embedding

The memory block is loaded from disk at runtime, not embedded via
`include_bytes!`.

Reasons:

- the corpus should evolve independently of Rust recompilation
- users may want multiple corpora or private local corpora
- this preserves current `cargo build` behavior for CLI and Tauri

### D6: Brute-force cosine search first, ANN later

The first runtime implementation uses brute-force cosine similarity over
L2-normalized vectors. For hundreds to low thousands of signatures, this is
simple, correct, and fast enough.

HNSW is deferred until the corpus size justifies it. The file format is
versioned so a future ANN block can be added without invalidating the first
artifact format.

### D7: Embedding stays local to `knowledge::memory_block` in v1

The embedding abstraction for the memory block lives inside
`knowledge::memory_block::embedder`, not in `crates/services/llm`.

Reasons:

- the default path is ONNX and does not depend on completion providers
- `knowledge` should not gain a dependency on `llm` just to import a trait
- embeddings are a much narrower capability surface than completion
- the first real consumer is the memory block itself

The v1 structure is:

- `EmbeddingProvider` trait local to `knowledge::memory_block`
- `OnnxEmbedder` as the default implementation
- `HttpEmbedder` as an optional thin OpenAI-compatible `/v1/embeddings` client
- `ResolvedEmbeddingConfig` stored alongside the trait so vector-space identity
  is explicit and comparable at load time

Promotion to a shared crate or shared service layer happens only if a second
independent consumer appears outside the knowledge crate or if embedding
configuration/retry/auth logic becomes meaningfully shared.

### D8: Invariant extraction is natural-language-first

LLMs are unreliable at recovering formal mathematics from PDFs with rendered
LaTeX or diagrams. The foundry extracts:

- natural-language invariant description
- the vulnerable code pattern or boundary it applies to
- optional Kani/Z3 hint text

It does not claim formal proof extraction.

---

## Phase 1: Python Foundry

### Location and Structure

```text
tools/
└── pdf_foundry/
    ├── pyproject.toml
    ├── README.md
    ├── pdf_foundry/
    │   ├── __init__.py
    │   ├── __main__.py           # CLI entry point
    │   ├── config.py             # settings, env vars, paths
    │   ├── parsing/
    │   │   ├── __init__.py
    │   │   └── pdf_parser.py     # pymupdf4llm wrapper
    │   ├── extraction/
    │   │   ├── __init__.py
    │   │   ├── extractor.py      # LLM-driven structured extraction
    │   │   └── prompts.py        # extraction prompts
    │   ├── embedding/
    │   │   ├── __init__.py
    │   │   ├── onnx_embedder.py
    │   │   └── api_embedder.py
    │   ├── bundling/
    │   │   ├── __init__.py
    │   │   └── bundle_writer.py  # knowledge.bin serialization
    │   └── schema/
    │       ├── __init__.py
    │       └── signature.py      # Pydantic models matching Rust schema
    ├── models/
    │   └── all-MiniLM-L6-v2.onnx
    ├── data/
    │   ├── markdown/             # gitignored
    │   ├── signatures.jsonl      # extracted records
    │   └── manifest.json         # incremental ingest tracking
    ├── output/
    │   └── knowledge.bin         # generated artifact
    └── tests/
        ├── test_parser.py
        ├── test_extractor.py
        ├── test_embedder.py
        └── test_bundle_writer.py
```

### 1.1 PDF Parsing

**Library:** `pymupdf4llm`

**Why this path fits the repo:**

- the current Rust parser in `intake` is intentionally lightweight and should
  stay that way for runtime optional inputs
- the foundry needs better reconstruction of headings, code blocks, and tables
- this is an offline dependency, so heavier parsing is acceptable

**Behavior:**

- input: directory of PDFs
- output: one markdown file per PDF under `data/markdown/`
- preserve: section headers, code blocks, tables, page references where possible
- strip: repeated headers/footers, watermarks, decorative noise
- keep raw PDFs and markdown local and gitignored unless explicitly approved for
  redistribution

### 1.2 LLM-Driven Extraction

**Input:** markdown from step 1.1

**Process:**

1. split markdown by likely finding boundaries
2. extract strict JSON records from each chunk
3. validate against the shared schema
4. deduplicate by content hash
5. append accepted signatures to `signatures.jsonl`

**Required schema properties:**

- stable `id`
- source provenance
- vulnerability description and root cause
- remediation summary
- natural-language invariant summary
- tags/categories
- evidence excerpt or finding span
- extraction confidence and review status
- embedding text generated from a versioned template

**Optional fields in the Rust-owned schema contract:**

- `invariants.kani_hint` may be omitted or `null` when no concrete Kani hint is available
- `evidence.section_title` may be omitted or `null` when the source chunk has no stable section label

Python foundry validation must treat these fields as optional rather than required.

**Example shape:**

```json
{
  "id": "TOB-2023-001-finding-3",
  "source": {
    "report": "Trail of Bits - ProjectX Audit 2023",
    "pdf_path": "reports/tob-projectx-2023.pdf",
    "page_range": [12, 14],
    "kind": "audit_report"
  },
  "vulnerability": {
    "title": "Nonce reuse in ChaCha20-Poly1305 AEAD",
    "severity": "critical",
    "category": "nonce-reuse",
    "description": "The encrypt() path derives nonces from a counter that resets on restart.",
    "vulnerable_pattern": "let nonce = counter.fetch_add(1, Ordering::Relaxed);",
    "root_cause": "Non-persistent state allows nonce reuse across restarts"
  },
  "remediation": {
    "description": "Bind nonce derivation to persistent session state or use an extended-nonce scheme.",
    "code_pattern": "let nonce = derive_nonce(session_id, msg_counter);"
  },
  "invariants": {
    "natural_language": "Two distinct encrypt() calls must not reuse the same nonce.",
    "kani_hint": "kani::assume(nonce_a != nonce_b);"
  },
  "evidence": {
    "excerpt": "The nonce counter is reset to zero on process startup.",
    "section_title": "Finding 3"
  },
  "extraction": {
    "confidence": "medium",
    "review_status": "unreviewed",
    "embedding_text_version": "v1"
  },
  "tags": ["aead", "nonce", "chacha20", "state-persistence"],
  "embedding_text": "Nonce reuse in ChaCha20-Poly1305 AEAD. Non-persistent state allows nonce reuse across restarts. aead nonce chacha20 state-persistence"
}
```

The artifact should store short evidence excerpts and provenance, not the full
PDF body.

**Incremental behavior:**

- `manifest.json` tracks `{pdf_path: sha256_hash}`
- unchanged PDFs are skipped
- extraction results are append-only until explicit rebuild/compaction
- review tooling can later mark signatures as accepted/rejected without
  rebuilding the entire pipeline design

### 1.3 Embedding Generation

**Default: ONNX**

- model: `all-MiniLM-L6-v2`
- dimensions: `384`
- tokenizer: HuggingFace tokenizer matching the model
- vector policy: L2-normalize vectors before writing them to the artifact

**Optional: provider-backed embeddings**

- configured via dedicated embedding settings, not completion-only settings
- OpenAI-compatible and Ollama-compatible backends are realistic first targets
- if a configured backend does not offer embeddings, the foundry must fail fast
  or explicitly fall back before artifact creation, never silently mix modes

**Artifact metadata recorded:**

```json
{
  "provider": "onnx",
  "model": "all-MiniLM-L6-v2",
  "dimensions": 384,
  "distance_metric": "cosine",
  "l2_normalized": true,
  "embedding_text_version": "v1",
  "generated_at": "2026-03-19T14:30:00Z"
}
```

### 1.4 Binary Bundle Format

The first artifact format is a vector bundle, not a cross-language HNSW dump.

**`knowledge.bin` layout:**

```text
Offset  Content
------  --------------------------------------------------------
0x00    Magic: b"CPKN" (4 bytes)
0x04    Format version: u32 LE
0x08    Embedding provider: utf8, null-padded (32 bytes)
0x28    Embedding model: utf8, null-padded (64 bytes)
0x68    Embedding dimensions: u32 LE
0x6C    Number of signatures: u32 LE
0x70    Vector blob offset: u64 LE
0x78    Vector blob size: u64 LE
0x80    Metadata blob offset: u64 LE
0x88    Metadata blob size: u64 LE
0x90    Flags / reserved (padding to 256 bytes)
0x100   [Vector blob: contiguous f32 array, count * dims]
        [Metadata blob: MessagePack-encoded artifact metadata + signatures]
```

**Notes:**

- vectors are stored in row-major order
- vectors are already L2-normalized, so cosine search is a dot product
- metadata carries schema version, extraction metadata, and embedding metadata
- a future artifact version may add an ANN/HNSW section, but v1 does not depend
  on it

### 1.5 CLI Interface

```bash
# Parse PDFs, extract signatures, embed them, and write knowledge.bin
python -m pdf_foundry ingest --pdf-dir ./reports/

# Rebuild the artifact from existing signatures without reparsing PDFs
python -m pdf_foundry rebuild-bundle

# Export signatures as readable JSON for review
python -m pdf_foundry export --format json --output signatures.json

# Show corpus stats and embedding metadata
python -m pdf_foundry info
```

### 1.6 Dependencies

```toml
# pyproject.toml [project.dependencies]
pymupdf4llm = ">=0.0.10"
onnxruntime = ">=1.17.0"
tokenizers = ">=0.15.0"
msgpack = ">=1.0.0"
pydantic = ">=2.0.0"
httpx = ">=0.27.0"
click = ">=8.0.0"
```

`hnswlib` is intentionally not required for the first implementation.

---

## Phase 2: Rust Runtime Integration

### 2.1 Extend `crates/services/knowledge`

Do not invent a repo layout that does not exist. Extend the current knowledge
crate in-place.

**Current shape:**

```text
crates/services/knowledge/src/
├── lib.rs
├── loader.rs
├── models.rs
└── store.rs
```

**Proposed shape:**

```text
crates/services/knowledge/src/
├── lib.rs
├── loader.rs
├── models.rs
├── store.rs
└── memory_block/
    ├── mod.rs
    ├── format.rs
    ├── search.rs
    ├── embedder.rs
    ├── config.rs
    └── types.rs
```

This keeps lightweight YAML knowledge and vector memory together, but the heavy
dependencies must be feature-gated.

### 2.2 Loading and Validation

```rust
pub struct MemoryBlock {
    header: BlockHeader,
    vectors: memmap2::Mmap,
    signatures: Vec<VulnerabilitySignature>,
    metadata: ArtifactMetadata,
    embedder: Box<dyn EmbeddingProvider>,
}

impl MemoryBlock {
    pub fn load(path: &Path, config: &EmbeddingConfig) -> Result<Self, MemoryBlockError>;
    pub fn search(&self, query_text: &str, k: usize) -> Result<Vec<SearchResult>, MemoryBlockError>;
}
```

**Startup behavior:**

1. look for `knowledge.bin` at configured path
2. if absent, continue without semantic memory
3. validate header magic and version
4. validate embedding metadata against the resolved runtime embedding config
5. if metadata does not match exactly, disable the memory block for this run
6. load vectors and metadata; query-time embedding is only allowed after the
   above validation succeeds

### 2.3 Query Embedding

The query embedding surface is owned locally by `knowledge::memory_block`.
It should not be attached to the existing completion-oriented `llm` crate in
the first implementation.

**Local design:**

- `EmbeddingProvider` trait lives in `memory_block::embedder`
- `ResolvedEmbeddingConfig` lives in `memory_block::config`
- `OnnxEmbedder` is the default implementation
- `HttpEmbedder` is an optional `reqwest`-based client for
  OpenAI-compatible `/v1/embeddings` endpoints
- embedding config is resolved independently of completion config

Suggested surface:

```rust
pub trait EmbeddingProvider {
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
    fn config(&self) -> &ResolvedEmbeddingConfig;
}
```

This keeps the default ONNX path self-contained and avoids introducing a
`knowledge -> llm` dependency for a capability that the `llm` crate does not
currently model.

**Promotion trigger:**

Move this abstraction into a shared location only if:

1. a second crate needs embeddings for a separate purpose
2. HTTP auth/retry/telemetry logic is duplicated
3. the project needs one shared embedding config layer across multiple systems

### 2.4 Search Strategy

**First implementation:** brute-force cosine similarity over normalized vectors.

Reasons:

- simplest cross-language contract
- no ANN format mismatch problem
- fast enough for the expected corpus size
- easier to test and reason about

If the corpus later exceeds the point where scan cost matters, add an ANN phase:

1. keep the vector bundle as the canonical artifact
2. derive a Rust-native ANN index from the raw vectors
3. optionally persist that ANN structure as a secondary cache/artifact

Do not make the first runtime depend on Python `hnswlib` serialization.

### 2.5 Integration Points

The first integration point should not be "everything in orchestrator at once."
The repo already has better seams.

**Stage A: Tauri toolbench**

- extend similar-case retrieval in `crates/apps/tauri-ui/src/ipc.rs`
- semantic memory augments the existing tag-based `similar_cases` flow
- this gives immediate analyst-facing value and is easy to inspect

**Stage B: LLM context assembly**

- enrich `CopilotService` prompts with retrieved historical signatures
- enrich `KaniHarnessScaffolder` query context for assumption/harness generation
- keep all retrieved context advisory

**Stage C: broader orchestrator integration**

- once the memory block is stable, use it in orchestrated prompt assembly for
  candidate generation or harness synthesis

This order matches the existing codebase better than jumping straight to a new
orchestrator-wide dependency.

### 2.6 New Rust Dependencies

```toml
# In crates/services/knowledge/Cargo.toml
rmp-serde = "1"
memmap2 = "0.9"

# Optional, behind a `memory-block` feature
ort = "2"
tokenizers = "0.15"
```

If `ort` proves too heavy for default builds, keep the feature disabled by
default and let Tauri/CLI opt in explicitly.

---

## Integration Feasibility Assessment

### What aligns well with the existing codebase

1. **Optional-by-default behavior**

   The repo already treats LLM enhancement as optional. The memory block fits
   the same pattern: present means better context, absent means the system still
   works.

2. **Immediate integration seams already exist**

   `ProjectIr` context extraction, Tauri toolbench similar-case loading, and the
   Kani harness scaffolder all provide natural insertion points.

3. **Keeping Python offline preserves runtime simplicity**

   This avoids contaminating CLI/Tauri packaging while still allowing better PDF
   tooling than the lightweight runtime parser.

4. **The knowledge crate is a reasonable home if feature-gated**

   It already owns playbooks, domain checklists, and adjudicated cases. Vector
   memory belongs in the same conceptual layer, but only if heavyweight
   dependencies stay opt-in.

5. **Schema snapshots match the repo's style**

   The repo already values typed contracts and committed schema artifacts. The
   foundry should follow that pattern instead of inventing a Python-first schema.

### Risks and mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Embedding config mismatch between foundry and runtime | Invalid retrieval results | Treat mismatch as a hard disable for semantic retrieval; never query a bundle with a different vector space |
| `ort` increases artifact/binary size | Heavier builds and packaging | Feature-gate runtime embedding support |
| Extraction quality varies by report format | Low-signal signatures | Preserve evidence/provenance, record confidence, and require manual review/export before trusting the corpus |
| Source document licensing/provenance | Redistribution risk | Keep raw PDFs and markdown local/gitignored; distribute only structured signatures and short evidence excerpts unless licensing is clear |
| ANN added too early | Complexity without benefit | Start with brute-force search and add ANN only when corpus scale justifies it |

### What this does NOT change

- deterministic rule engine behavior
- the existing Rust optional-input PDF parser in `crates/services/intake`
- current `cargo build` behavior
- the ability to run CLI/Tauri without any memory artifact present

---

## Sequencing

### Step 0: Shared schema and artifact contract

- define Rust-owned `VulnerabilitySignature` and artifact metadata types
- generate JSON Schema snapshot for the Python foundry to validate against
- freeze the first `knowledge.bin` format

### Step 1: Python foundry core

- PDF parsing
- structured extraction
- ONNX embedding
- deliverable: real `signatures.jsonl` from a small sample corpus

### Step 2: Vector bundle writer and CLI

- write `knowledge.bin`
- add `ingest`, `rebuild-bundle`, `export`, and `info`
- deliverable: repeatable offline artifact generation

### Step 3: Rust loader and search

- add `memory_block` module in `crates/services/knowledge`
- validate metadata and query embedding compatibility
- implement brute-force cosine search
- deliverable: Rust can load the bundle and return top-K signatures for a query

### Step 4: First runtime integration

- augment Tauri toolbench similar-case retrieval
- add semantic context to `CopilotService` and/or `KaniHarnessScaffolder`
- deliverable: runtime prompts can use historical signatures without changing
  deterministic authority boundaries

### Step 5: Hardening

- feature flags
- better error messages
- fixture-based tests for parsing, bundling, loading, and search
- manual corpus review workflow

### Step 6: Optional ANN/HNSW acceleration

- only after corpus scale proves brute-force inadequate
- keep the vector bundle canonical
- add ANN as an optimization layer, not as the primary contract
