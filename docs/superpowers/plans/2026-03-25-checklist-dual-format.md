# Dual-Format Cryptographic Audit Checklist Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite the canonical cryptographic audit checklist and generate two synchronized outputs: a human-readable checklist and an LLM-optimized checklist.

**Architecture:** Keep `data/rules/checklist.md` as the single canonical source, structured as layered Markdown with normalized check blocks and stable IDs. Add a small deterministic renderer that parses the canonical format and writes `data/rules/checklist_human.md` and `data/rules/checklist_llm.md`. Expand the canonical content around scenario-first dimensional analysis, protocol families, Ethereum boundaries, and evidence-oriented audit flow.

**Tech Stack:** Markdown, Python 3 standard library, `unittest`, repository docs under `docs/superpowers/`

**Spec:** `docs/superpowers/specs/2026-03-25-checklist-dual-format-design.md`

---

## File Structure

**Create:**
- `scripts/render_checklists.py` — parse canonical checklist blocks, validate IDs/shape, render human and LLM outputs, support `--check`
- `scripts/tests/__init__.py` — unittest package marker
- `scripts/tests/test_render_checklists.py` — parser / renderer / drift-detection tests
- `scripts/tests/fixtures/checklist_fixture.md` — minimal canonical fixture for tests
- `scripts/tests/fixtures/expected_checklist_human.md` — expected human output for fixture
- `scripts/tests/fixtures/expected_checklist_llm.md` — expected LLM output for fixture
- `data/rules/checklist_human.md` — generated manual-review checklist
- `data/rules/checklist_llm.md` — generated LLM checklist

**Modify:**
- `data/rules/checklist.md` — rewrite into canonical layered format with stable check IDs and expanded dimensional-analysis section

**Delete / keep deleted:**
- `data/rules/crypto_rust_audit_checklist_en.html` — do not restore; leave deleted

---

### Task 1: Add renderer fixture and failing tests

**Files:**
- Create: `scripts/tests/__init__.py`
- Create: `scripts/tests/test_render_checklists.py`
- Create: `scripts/tests/fixtures/checklist_fixture.md`
- Create: `scripts/tests/fixtures/expected_checklist_human.md`
- Create: `scripts/tests/fixtures/expected_checklist_llm.md`

- [ ] **Step 1: Create the fixture canonical checklist**

Write `scripts/tests/fixtures/checklist_fixture.md` with one minimal section using the final canonical format:

```md
# Test Checklist

## 0. Dimensional Analysis

### Purpose

Short human context.

### Checks

- [C] DIM-001 No value crosses curve families without an explicit converter.
  Why: Cross-field confusion causes soundness failures.
  Applies: arkworks, halo2.
  Misalignment: wrong modulus reduction.

- [H] DIM-002 Transcript domain tags bind protocol and version.
  Why: Missing domain binding permits replay.
  Applies: merlin, halo2.
  Misalignment: protocol-v1/v2 collision.
```

- [ ] **Step 2: Create expected human output fixture**

Write `scripts/tests/fixtures/expected_checklist_human.md` to preserve:
- section headers
- purpose text
- full check text
- `Why`, `Applies`, `Misalignment`

Start it with:

```md
# Cryptographic Rust Audit Checklist (Human)

> Generated from `data/rules/checklist.md`. Do not edit directly.
```

- [ ] **Step 3: Create expected LLM output fixture**

Write `scripts/tests/fixtures/expected_checklist_llm.md` to keep only compact normalized content:

```md
# Cryptographic Rust Audit Checklist (LLM)

> Generated from `data/rules/checklist.md`. Do not edit directly.

## 0. Dimensional Analysis
- [C] DIM-001 No value crosses curve families without an explicit converter.
- [H] DIM-002 Transcript domain tags bind protocol and version.
```

- [ ] **Step 4: Write failing unit tests for parser and render modes**

In `scripts/tests/test_render_checklists.py`, add tests for:
- parsing a valid canonical fixture
- rejecting duplicate check IDs
- rendering human output equal to `expected_checklist_human.md`
- rendering LLM output equal to `expected_checklist_llm.md`
- `--check` failing when generated files differ from expected content

Use Python `unittest` and `tempfile`. Import the renderer module by path.

- [ ] **Step 5: Run tests to verify they fail**

Run:

```bash
python3 -m unittest discover -s scripts/tests -v
```

Expected:
- import failure or missing module failure for `render_checklists.py`
- test suite exits non-zero

- [ ] **Step 6: Commit**

```bash
git add scripts/tests
git commit -m "test: add failing tests for checklist renderer"
```

---

### Task 2: Implement deterministic checklist renderer

**Files:**
- Create: `scripts/render_checklists.py`
- Modify: `scripts/tests/test_render_checklists.py`

- [ ] **Step 1: Implement canonical parser**

In `scripts/render_checklists.py`, implement a parser that:
- reads `data/rules/checklist.md` or an input path
- recognizes:
  - `##` section headers
  - `### Purpose`
  - `### Dimensions / scenarios`
  - `### Checks`
  - `### Examples / notes`
- recognizes canonical check blocks of the form:

```md
- [C] DIM-001 Statement.
  Why: ...
  Applies: ...
  Misalignment: ...
```

Data model should include:
- title
- ordered sections
- per-section prose buckets
- ordered checks with `severity`, `id`, `statement`, optional `why`, `applies`, `misalignment`

- [ ] **Step 2: Implement validation rules**

Add validation for:
- duplicate check IDs
- malformed severity markers
- missing statement text
- malformed continuation lines
- check lines outside `### Checks`

Raise clear errors like:

```text
Duplicate check ID: DIM-001
Malformed check block in section "0. Dimensional Analysis"
```

- [ ] **Step 3: Implement human renderer**

Render `checklist_human.md` with:
- generated-file header
- section titles in canonical order
- purpose text
- dimensions/scenario prose and tables
- check statements plus `Why`, `Applies`, `Misalignment`
- examples/notes sections where present

- [ ] **Step 4: Implement LLM renderer**

Render `checklist_llm.md` with:
- generated-file header
- section titles
- one compact line per check:

```md
- [C] DIM-001 No value crosses curve families without an explicit converter.
```

Drop:
- `Why`
- `Applies`
- `Misalignment`
- examples
- repeated prose

Keep section ordering unchanged.

- [ ] **Step 5: Implement CLI modes**

Support:

```bash
python3 scripts/render_checklists.py
python3 scripts/render_checklists.py --input path/to/checklist.md --out-dir path/to/out
python3 scripts/render_checklists.py --check
```

`--check` should:
- regenerate in memory
- compare against `data/rules/checklist_human.md` and `data/rules/checklist_llm.md`
- exit non-zero with a clear message if outputs are stale

- [ ] **Step 6: Run tests to verify they pass**

Run:

```bash
python3 -m unittest discover -s scripts/tests -v
```

Expected:
- all tests PASS

- [ ] **Step 7: Commit**

```bash
git add scripts/render_checklists.py scripts/tests
git commit -m "feat: add deterministic checklist renderer"
```

---

### Task 3: Rewrite canonical `checklist.md`

**Files:**
- Modify: `data/rules/checklist.md`

- [ ] **Step 1: Rewrite the file header and authoring contract**

At the top of `data/rules/checklist.md`, include:
- canonical-source wording
- note that derived files are `checklist_human.md` and `checklist_llm.md`
- brief authoring guidance that checks must use normalized structured blocks

Use a compact header such as:

```md
# Cryptographic Rust Audit Checklist

> Canonical source for generated checklist variants.
> Outputs: `checklist_human.md`, `checklist_llm.md`.
> Edit this file only, then run `python3 scripts/render_checklists.py`.
```

- [ ] **Step 2: Rebuild section `0` as scenario-first dimensional analysis**

Rewrite section `0` to include:
- `### Purpose`
- `### Dimensions / scenarios`
- `### Checks`

Cover these dimensions explicitly:
- curve / field family
- group / pairing slot
- representation / encoding
- trust / validation state
- exposure / secrecy class
- protocol role / object kind
- transcript / domain separation
- sequence / phase
- session / participant context
- parameter / setup / circuit / program identity
- proof system / arithmetization family
- recursion / nesting level
- execution / backend / feature variant
- environment / chain binding
- guest / host / wrapper boundary
- randomness / challenge source

Within the scenario prose, reference representative ecosystem families, including:
- arkworks families
- pairing/BLS families
- RustCrypto ECC / trait families
- dalek / merlin / subtle / zeroize families
- halo2 / bellman / bellperson / librustzcash / jubjub
- threshold / MPC / DKG / FROST families
- STARK / Plonkish / folding / recursion families
- zkVM families
- Ethereum KZG / precompile / ABI boundary libraries

- [ ] **Step 3: Rewrite the rest of the checklist into the approved architecture**

Use this exact top-level order:

1. `0. Dimensional Analysis`
2. `1. Scope and Threat Model`
3. `2. Input Objects and Validation`
4. `3. Algebra and Group Semantics`
5. `4. Randomness, Nonces, and Challenges`
6. `5. Transcript, Domain Separation, and Binding`
7. `6. Serialization and Cross-Boundary Encoding`
8. `7. Side-Channel and Secret Exposure`
9. `8. Rust Safety, APIs, and State Management`
10. `9. Protocol Families`
11. `10. Ethereum and On-Chain Boundaries`
12. `11. Testing, Differential Validation, and Evidence`

Each section should use:
- `### Purpose`
- optional `### Dimensions / scenarios`
- `### Checks`
- optional `### Examples / notes`

- [ ] **Step 4: Add stable IDs to every check**

Use prefixes aligned to sections, for example:
- `DIM-###`
- `SCOPE-###`
- `INPUT-###`
- `ALG-###`
- `RNG-###`
- `TRN-###`
- `SER-###`
- `SIDE-###`
- `RUST-###`
- `PROTO-###`
- `ETH-###`
- `TEST-###`

Ensure every ID is unique across the file.

- [ ] **Step 5: Normalize all checks into structured blocks**

Convert existing bullet-only checks into canonical blocks with at least:
- statement
- `Why`
- `Applies`
- `Misalignment`

Keep each check focused on one enforceable audit question.

- [ ] **Step 6: Run renderer in dry validation mode to catch malformed blocks**

Run:

```bash
python3 scripts/render_checklists.py --check
```

Expected:
- fail because generated files do not yet exist or are stale
- no parser/validation errors from the canonical source format itself

- [ ] **Step 7: Commit**

```bash
git add data/rules/checklist.md
git commit -m "docs: rewrite canonical cryptographic audit checklist"
```

---

### Task 4: Generate the human and LLM checklist variants

**Files:**
- Create: `data/rules/checklist_human.md`
- Create: `data/rules/checklist_llm.md`

- [ ] **Step 1: Generate both outputs**

Run:

```bash
python3 scripts/render_checklists.py
```

Expected:
- `data/rules/checklist_human.md` created or updated
- `data/rules/checklist_llm.md` created or updated

- [ ] **Step 2: Inspect the human output**

Open and verify that:
- section purpose text is preserved
- dimensional-analysis tables and scenario coverage render cleanly
- `Why`, `Applies`, and `Misalignment` lines remain attached to the right checks
- manual-review readability is substantially better than the canonical source

- [ ] **Step 3: Inspect the LLM output**

Open and verify that:
- only compact section headers and normalized checks remain
- repeated prose and examples are removed
- section order matches canonical order exactly
- token-heavy lists are not repeated unnecessarily

- [ ] **Step 4: Run drift check**

Run:

```bash
python3 scripts/render_checklists.py --check
```

Expected:
- success exit code
- message indicating generated checklists are up to date

- [ ] **Step 5: Commit**

```bash
git add data/rules/checklist_human.md data/rules/checklist_llm.md
git commit -m "docs: generate human and llm audit checklist variants"
```

---

### Task 5: Final verification and cleanup

**Files:**
- Modify: `data/rules/checklist.md` if final wording cleanup is needed
- Modify: generated checklist files if regeneration is needed

- [ ] **Step 1: Run the full renderer test suite**

Run:

```bash
python3 -m unittest discover -s scripts/tests -v
python3 scripts/render_checklists.py --check
```

Expected:
- all tests PASS
- drift check PASS

- [ ] **Step 2: Confirm deleted HTML file stays deleted**

Run:

```bash
git status --short data/rules
```

Expected:
- `data/rules/crypto_rust_audit_checklist_en.html` remains deleted
- canonical and generated checklist files appear as intended

- [ ] **Step 3: Review diffs for the three checklist artifacts**

Run:

```bash
git diff -- data/rules/checklist.md data/rules/checklist_human.md data/rules/checklist_llm.md
```

Expected:
- canonical file contains layered authoring content
- human file contains expanded manual-review rendering
- LLM file contains compact normalized rendering

- [ ] **Step 4: Commit**

```bash
git add data/rules/checklist.md data/rules/checklist_human.md data/rules/checklist_llm.md scripts/render_checklists.py scripts/tests
git commit -m "feat: add canonical and generated cryptographic audit checklists"
```

---

## Notes for Execution

- Do not update the auto-audit workflow in this implementation pass.
- Do not restore `data/rules/crypto_rust_audit_checklist_en.html`.
- If the canonical format needs one extra metadata field during implementation, add it only if it clearly improves deterministic rendering for both outputs.
- Prefer small, focused commits that preserve a readable history: tests first, renderer second, canonical rewrite third, generated outputs last.
