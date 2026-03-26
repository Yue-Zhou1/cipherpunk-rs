# Cryptographic Rust Audit Checklist (LLM)

> Generated from `data/rules/checklist.md`. Do not edit directly.

## 0. Dimensional Analysis
- [C] DIM-001 No value crosses curve or field families without an explicit, spec-allowed converter.
- [C] DIM-002 Unverified objects never enter code paths that require on-curve, subgroup-checked, prepared, or verified values.
- [C] DIM-003 Protocol-role values are never accepted interchangeably across APIs.
- [H] DIM-004 Transcript, session, and environment dimensions are bound before challenges or signatures are derived.
- [H] DIM-005 Setup, circuit, program, and recursion identities remain attached to every downstream artifact.
- [H] DIM-006 Backend and boundary variants preserve identical security semantics.

## 1. Scope and Threat Model
- [H] SCOPE-001 Classify the target as a primitive, protocol, wrapper, or mixed crate before reviewing internals.
- [C] SCOPE-002 Enumerate attacker control over malformed input, transcript context, concurrency, and feature flags.
- [C] SCOPE-003 Record which guarantees the library enforces and which guarantees the caller must remember manually.
- [H] SCOPE-004 Bind parameter, setup, and trusted-material provenance into the threat model up front.

## 2. Input Objects and Validation
- [C] INPUT-001 Every externally supplied point or proof object is validated before use.
- [C] INPUT-002 On-curve and subgroup checks are treated as distinct requirements when the curve requires them.
- [H] INPUT-003 Identity, infinity, zero, and sentinel values are handled explicitly rather than by accident.
- [H] INPUT-004 Length, structure, and nesting bounds are enforced before expensive work begins.
- [M] INPUT-005 Validation failures are explicit errors and never silently coerce to default values.

## 3. Algebra and Group Semantics
- [C] ALG-001 Scalar, base-field, and extension-field operations stay in their intended domains throughout the code path.
- [C] ALG-002 Group-slot semantics stay exact for pairing and multi-pairing code.
- [H] ALG-003 Coordinate-system and exceptional-case handling are reviewed for incomplete formulas and edge values.
- [H] ALG-004 Boundary values and reduction paths are exercised for zero, one, modulus minus one, modulus, and modulus plus one.
- [M] ALG-005 Integer casts and native arithmetic are never relied on as a substitute for field semantics.

## 4. Randomness, Nonces, and Challenges
- [C] RNG-001 Production randomness comes from a cryptographically secure source on every supported platform.
- [C] RNG-002 Deterministic nonces bind secret key, message, and protocol context exactly as required by the scheme.
- [H] RNG-003 Session-oriented protocols guarantee fresh nonces and per-session state separation.
- [H] RNG-004 Challenge derivation is deterministic, unbiased, and aligned across prover and verifier.

## 5. Transcript, Domain Separation, and Binding
- [C] TRN-001 Every logically distinct context has explicit domain separation tags.
- [C] TRN-002 All prior prover messages are absorbed before any challenge or random oracle query is derived.
- [H] TRN-003 Prover and verifier absorb data in the same order and representation.
- [H] TRN-004 Binding includes protocol version, role, participant context, and environment when those dimensions matter.
- [M] TRN-005 Transcript APIs make unsafe states difficult to reach.

## 6. Serialization and Cross-Boundary Encoding
- [C] SER-001 Exactly one accepted encoding exists for each valid object, and non-canonical encodings are rejected.
- [H] SER-002 Serialization formats match the real wire protocol rather than internal memory layout or incidental `serde` defaults.
- [H] SER-003 Boundary wrappers preserve canonicality, lengths, and semantic role across FFI, calldata, and verifier boundaries.
- [M] SER-004 Cross-version and cross-implementation compatibility is intentionally checked rather than assumed.

## 7. Side-Channel and Secret Exposure
- [C] SIDE-001 Secret-dependent branches, indexing, and memory access patterns are absent from security-critical paths.
- [H] SIDE-002 Constant-time helper crates are used correctly rather than partially or cosmetically.
- [H] SIDE-003 Secret-bearing values are not exposed through formatting, logs, panic text, or overly informative error paths.
- [M] SIDE-004 Zeroization and secret-lifetime controls cover owned values, clones, and temporary buffers.

## 8. Rust Safety, APIs, and State Management
- [H] RUST-001 Every `unsafe` block has a documented invariant that is re-verified at each call site.
- [H] RUST-002 Unchecked constructors and assumption-carrying APIs are clearly isolated from ordinary safe flows.
- [H] RUST-003 Builder, finalize, and state-machine APIs enforce required sequencing.
- [H] RUST-004 Concurrency and shared state cannot duplicate, race, or leak cryptographic state.
- [M] RUST-005 Error handling remains robust under attacker-controlled inputs and variant backends.

## 9. Protocol Families
- [C] PROTO-001 Pairing and BLS systems preserve correct subgroup validation, rogue-key resistance, and exact verification equations.
- [C] PROTO-002 Groth16, Marlin, and R1CS-based systems bind the exact circuit, public input encoding, and proving/verification setup.
- [H] PROTO-003 Plonkish systems validate the polynomial commitment scheme, lookup semantics, and transcript wiring end to end.
- [H] PROTO-004 STARK and FRI systems preserve arithmetization, commitment, and verifier-parameter consistency.
- [H] PROTO-005 Folding, accumulation, and recursion systems bind inner statements, accumulators, and verification context exactly.
- [C] PROTO-006 Threshold, MPC, and DKG systems bind participant identity, session state, and share verification equations.
- [H] PROTO-007 zkVM systems bind host inputs, guest program identity, public outputs, and verifier-wrapper assumptions.

## 10. Ethereum and On-Chain Boundaries
- [C] ETH-001 On-chain and off-chain components agree on encoding, endian rules, and field interpretation exactly.
- [H] ETH-002 Signatures and transcript-derived artifacts are bound to chain, fork, application, and contract context where required.
- [H] ETH-003 KZG and blob-commitment flows validate setup provenance and verifier assumptions explicitly.
- [H] ETH-004 Precompile and verifier wrappers preserve the same validation guarantees as the underlying cryptographic primitive.
- [M] ETH-005 Bridge and L1/L2 systems bind message root, epoch, and environment context into the audited proof or signature path.

## 11. Testing, Differential Validation, and Evidence
- [H] TEST-001 Negative tests exist for malformed points, malformed proofs, tampered signatures, and invalid transcript context.
- [H] TEST-002 Boundary and differential tests compare behavior across representative implementations or variants where feasible.
- [M] TEST-003 Fuzzing or property-style tests exist for high-risk parsers, hash-to-curve, and proof-verification boundaries.
- [M] TEST-004 Each significant finding can be grounded in a reproducible artifact, test, or exact code path.
- [M] TEST-005 Generated checklist artifacts remain synchronized with the canonical source through an explicit validation step.
