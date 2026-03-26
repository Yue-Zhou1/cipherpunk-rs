# Dual-Format Cryptographic Audit Checklist

**Date:** 2026-03-25
**Scope:** `data/rules/checklist.md`, generated `data/rules/checklist_human.md`, generated `data/rules/checklist_llm.md`, and the renderer used to produce them.
**Goal:** Establish one canonical cryptographic audit checklist source that can generate two optimized views: one for experienced human auditors and one for LLM-driven audit runs.

---

## Problem

The current checklist work is moving in two directions at once:

- it needs broader scenario coverage, especially around dimensional analysis for blockchain and Ethereum cryptography
- it also needs two different consumption modes:
  - a richer manual-review format for experienced auditors
  - a compact, low-token format for LLM-driven audit flows

Maintaining separate checklists by hand would create drift. The project therefore needs one canonical source with deterministic rendering into both outputs.

---

## Decision Summary

Use a layered Markdown source-of-truth:

- `data/rules/checklist.md` is the canonical source
- `data/rules/checklist_human.md` is generated for manual review
- `data/rules/checklist_llm.md` is generated for LLM ingestion

The canonical file is structured Markdown, not YAML/JSON. This keeps it editable by auditors while still being regular enough for deterministic rendering.

The deleted `data/rules/crypto_rust_audit_checklist_en.html` is out of scope and will not be recreated in this pass.

The auto-audit workflow is also out of scope for this pass. The only requirement carried forward is that `checklist_llm.md` is shaped so the workflow can later be pointed at it directly.

---

## Canonical Source Shape

`data/rules/checklist.md` should be rewritten as a layered, structured checklist with stable sections and normalized check blocks.

### Section layout

Each top-level section uses:

- `## <n>. <Section Name>`
- `### Purpose`
- `### Dimensions / scenarios` where needed
- `### Checks`
- `### Examples / notes` only where the examples add audit value

### Canonical check format

Each canonical check should be represented as a small structured block:

```md
- [C] DIM-001 No value crosses curve/field families without an explicit, spec-allowed converter.
  Why: Cross-field confusion causes soundness failures, invalid arithmetic, and proof mismatch.
  Applies: arkworks, halo2, bellperson, lambdaworks, Plonky3, Nova-family systems.
  Misalignment: scalar/base-field swap; non-native field gadget mismatch; wrong modulus reduction.
```

Rules:

- one concept per check
- stable check IDs
- stable section order
- no implicit sorting in generation
- free-form prose only in `Purpose`, `Dimensions / scenarios`, and `Examples / notes`

This keeps the file readable for humans and compressible for machine output.

---

## Content Architecture

The canonical checklist should be organized as a layered audit flow:

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

This structure keeps the dimensional-analysis lens first, then walks the reviewer through concrete audit domains.

---

## Dimensional Analysis Expansion

Section `0` should be rewritten from a short table into a scenario-first foundation for the whole checklist.

It should cover at least these dimensions:

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

It should also explicitly cover representative ecosystem families as coverage anchors rather than as sources of truth. These include:

- `arkworks-rs` families such as `algebra`, `crypto-primitives`, `groth16`, `marlin`, `poly-commit`, `r1cs-std`, and `snark`
- pairing / BLS implementations such as `blst`, `blstrs`, and `zkcrypto/bls12_381`
- RustCrypto elliptic-curve and trait ecosystems
- `curve25519-dalek`, `ed25519-dalek`, `merlin`, `subtle`, `zeroize`
- `halo2`, `bellman`, `bellperson`, `librustzcash`, `jubjub`
- threshold and MPC systems such as `frost`, threshold BLS, threshold ECDSA families
- STARK / Plonkish / folding / recursion systems such as `Plonky3`, `plonky2`, `winterfell`, `stwo`, `boojum`, `Nova`, `Sonobe`
- zkVM families such as `risc0`, `sp1`, `Jolt`, and `nexus-zkvm`
- Ethereum boundary libraries such as `c-kzg-4844`, `rust-kzg`, `revm`, `alloy`, and `ethers-rs`

The point of naming these libraries is not to treat them as mathematically authoritative. The point is to ensure the checklist covers the concrete usage scenarios and boundary classes that appear in real Rust blockchain cryptography systems.

---

## Protocol Coverage Model

The checklist should be comprehensive across scenario classes, not just library names.

The protocol-family section should include subsections or grouped checks for:

- pairing and BLS systems
- Groth16 / Marlin / R1CS systems
- Plonkish systems and polynomial commitments
- STARK / FRI systems
- folding / accumulation / recursion systems
- threshold / MPC / DKG / FROST systems
- zkVM guest-host proving systems

The Ethereum section should include:

- EVM precompile boundaries
- ABI / calldata / verifier-wrapper boundaries
- chain / fork / domain binding
- KZG setup and EIP-4844 blob commitment boundaries
- L1/L2 and bridge-domain context binding

Trait and utility layers should be addressed where they matter operationally:

- field / group traits
- signature / digest traits
- constant-time selection utilities
- zeroization and secret-exposure utilities

These should not appear as a flat inventory. They should appear as concrete dimensions and failure modes.

---

## Rendered Outputs

### Human output

`data/rules/checklist_human.md` should preserve:

- section purpose text
- dimensional-analysis tables
- grouped scenario coverage
- `Why`, `Applies`, and `Misalignment` lines
- short examples or notes where they materially help a human reviewer

This output should optimize for manual audit flow and expert scanning.

### LLM output

`data/rules/checklist_llm.md` should keep only:

- section titles
- stable check IDs
- severity
- terse normalized check statements
- optional short tags if needed for routing

It should drop:

- repeated rationale
- long examples
- narrative prose that does not change audit behavior
- token-heavy library lists repeated across multiple sections

This output should optimize for low-token ingestion and fast lookup by agents.

---

## Renderer Requirements

A small deterministic renderer should generate both output files from the canonical source.

Requirements:

- fail on duplicate check IDs
- fail on malformed check blocks
- preserve canonical section order exactly
- generate stable output across runs
- support a validation mode so future CI can detect drift

A simple script such as `scripts/render_checklists.py` is sufficient.

---

## Non-Goals

This design does not include:

- modifying the auto-audit workflow in this pass
- recreating the deleted HTML checklist
- converting the source-of-truth into YAML/JSON/TOML
- building an interactive UI for checklist editing

---

## Implementation Outline

The implementation should proceed in this order:

1. Rewrite `data/rules/checklist.md` into the canonical layered format.
2. Expand the dimensional-analysis section based on the agreed scenario model.
3. Add stable check IDs and normalized check blocks across the file.
4. Implement a renderer that reads the canonical file and writes `checklist_human.md` and `checklist_llm.md`.
5. Generate both output files and inspect them for fidelity and readability.
6. Add lightweight validation so malformed canonical data or duplicate IDs fail fast.

---

## Open Follow-Up

After this pass is complete, a later change can update the auto-audit workflow to read `data/rules/checklist_llm.md` explicitly.
