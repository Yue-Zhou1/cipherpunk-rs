# Cryptographic Rust Audit Checklist (Human)

> Generated from `data/rules/checklist.md`. Do not edit directly.

## 0. Dimensional Analysis

### Purpose

Use dimensional analysis as the top-level lens for every audit run. The goal is not to assume any library is mathematically authoritative; the goal is to ensure the checklist covers the real boundary classes that appear in Rust cryptography code used in blockchain and Ethereum systems.

### Dimensions / scenarios

Treat library names as coverage anchors, not as ground truth. The checklist should stay scenario-complete even as ecosystems change.

| Dimension | What it tracks | Representative scenarios |
|-----------|----------------|--------------------------|
| Curve / field family | Exact scalar, base, or extension field | BN254 vs BLS12-381 vs Pasta vs secp256k1 vs Ristretto vs Decaf377 vs Goldilocks |
| Group / pairing slot | Which subgroup or pairing side a value inhabits | G1 vs G2 vs GT, affine vs projective, prepared VK vs raw point |
| Representation / encoding | Bytes, wire format, and canonicality | SEC1, compressed points, ark-serialize, serde, ABI, RLP, SSZ, SCALE, Circom public inputs |
| Trust / validation state | What checks have actually happened | raw bytes, parsed, on-curve, subgroup-checked, verified proof, verified receipt |
| Protocol role / object kind | Semantic meaning of the object | secret key, public key, signature, witness, public input, VK, PK, receipt |
| Transcript / hash instantiation | Which transcript namespace and primitive family a value belongs to | Merlin labels, Poseidon parameter sets, Keccak tags, SHA-2/3, BLAKE2/3, EIP-712 |
| Trait / interface contract | Which algebraic or signing traits an object is assumed to satisfy | `ff::Field`, `group::Group`, `signature::Signer`, `digest::Digest`, non-native gadget traits |
| Session / phase | Ordering and participant context | pre-challenge vs post-challenge, signer set, round ID, batch ID, epoch / slot |
| Setup / program identity | Which proving setup or executable object a value belongs to | SRS, Powers of Tau, circuit ID, VK digest, image ID, accumulator context |
| Backend / execution variant | Which implementation path is active | asm vs portable, CPU vs GPU, `std` vs `no_std`, `wasm`, feature flags |
| Environment / boundary | Which chain or wrapper boundary the value crosses | EVM precompiles, calldata, verifier wrappers, bridge domains, Circom/ark bridges, zkVM guest-host |

Coverage anchors that should stay in mind while auditing include:

- `arkworks-rs` families such as `algebra`, `crypto-primitives`, `groth16`, `marlin`, `poly-commit`, `r1cs-std`, `snark`, curve crates like `ark-bn254`, `ark-bls12-381`, `ark-bw6-761`, and bridge layers such as `ark-circom` / `circom-compat`
- pairing and BLS ecosystems such as `blst`, `blstrs`, and `zkcrypto/bls12_381`
- RustCrypto ECC, trait, and bigint ecosystems such as `elliptic-curves`, `ff`, `group`, `signature`, `digest`, `zeroize`, `crypto-bigint`, and `ruint`
- `curve25519-dalek`, `ed25519-dalek`, `x25519-dalek`, `bulletproofs`, `merlin`, `subtle`, and `schnorrkel`
- `halo2`, `bellman`, `bellperson`, `librustzcash`, `jubjub`, `pasta_curves`, and adjacent Pasta/Jubjub systems
- curve and commitment ecosystems such as `decaf377` and `banderwagon` when reviewing privacy or Verkle-style constructions
- threshold and MPC families such as `frost`, `threshold_crypto`, threshold BLS, threshold ECDSA, `multi-party-ecdsa`, and `synedrion`
- hash and sponge families such as `sha2`, `sha3`, `blake2`, `blake3`, `ark-sponge`, `neptune`, and Poseidon variants embedded in proof systems
- STARK, Plonkish, folding, and recursion systems such as `Plonky3`, `plonky2`, `starky`, `winterfell`, `stwo`, `boojum`, `Nova`, `Sonobe`, `kimchi`, and `jellyfish`
- zkVM systems such as `risc0`, `sp1`, `Jolt`, and `nexus-zkvm`
- Ethereum boundary libraries such as `c-kzg-4844`, `rust-kzg`, `revm`, `alloy`, and `ethers-rs`

### Checks

- [C] DIM-001 No value crosses curve or field families without an explicit, spec-allowed converter.
  Why: Cross-field confusion causes soundness failures, invalid arithmetic, and proof mismatch.
  Applies: arkworks curve crates, halo2 and `pasta_curves`, bellperson, lambdaworks, `decaf377`, Plonky3/Goldilocks systems, Nova-family systems.
  Misalignment: scalar/base-field swap; wrong modulus reduction; non-native field gadget mismatch; curve-parameter crate mismatch.

- [C] DIM-002 Unverified objects never enter code paths that require on-curve, subgroup-checked, prepared, or verified values.
  Why: Trust-state confusion turns parseable attacker input into cryptographic authority.
  Applies: blst, blstrs, arkworks deserializers, halo2 verifier wrappers, zkVM receipt APIs.
  Misalignment: raw bytes treated as trusted; `_unchecked` constructors on attacker input; verified and unverified wrappers conflated.

- [C] DIM-003 Protocol-role values are never accepted interchangeably across APIs.
  Why: Role confusion lets the wrong object satisfy the right type shape.
  Applies: proving and verification keys, commitments and openings, proofs and public inputs, signatures and nonce commitments.
  Misalignment: VK treated as PK; witness/public-input swap; receipt treated as journal payload.

- [H] DIM-004 Transcript, session, and environment dimensions are bound before challenges or signatures are derived.
  Why: Context drift creates replay and transcript-collision bugs without changing local types.
  Applies: merlin transcripts, Poseidon and `ark-sponge` / `neptune` variants, FROST sessions, `schnorrkel`, EIP-712 signing, bridge verifiers.
  Misalignment: missing chain ID; missing protocol version; round or participant set omitted from transcript; Poseidon parameter set mismatch.

- [H] DIM-005 Setup, circuit, program, and recursion identities remain attached to every downstream artifact.
  Why: Objects from different proving or execution contexts often serialize similarly but are not substitutable.
  Applies: SRS and CRS consumers, Groth16 and Plonkish VKs, recursive accumulators, zkVM image IDs.
  Misalignment: wrong VK digest accepted; wrong SRS reused; inner statement not bound to outer proof.

- [H] DIM-006 Backend and boundary variants preserve identical security semantics.
  Why: Variant mismatches often appear only under asm, GPU, `wasm`, or precompile wrapper paths.
  Applies: blst and blstrs backends, bellperson GPU paths, RustCrypto backend variants, `c-kzg-4844` FFI bindings, `revm` integrations, EVM precompile wrappers.
  Misalignment: feature-flag divergence; portable/asm mismatch; FFI or calldata path skipping validation; foreign return codes treated as sufficient proof of validity.

## 1. Scope and Threat Model

### Purpose

Define the claimed security goal and the attacker’s control surface before auditing details. The checklist is only meaningful if the review distinguishes what the library enforces from what it pushes onto callers.

### Checks

- [H] SCOPE-001 Classify the target as a primitive, protocol, wrapper, or mixed crate before reviewing internals.
  Why: Audit priority changes sharply depending on whether the code implements math, a protocol, or a glue boundary.
  Applies: pure field crates, proof-system crates, Ethereum verifier wrappers, FFI bridges.
  Misalignment: primitive checklist applied to wrapper code only; protocol assumptions reviewed as if they were math invariants.

- [C] SCOPE-002 Enumerate attacker control over malformed input, transcript context, concurrency, and feature flags.
  Why: Missing attacker capabilities hide the exact boundary where misuse becomes exploitation.
  Applies: parsing APIs, verifier entrypoints, batch verification, runtime-configurable backends.
  Misalignment: assumes trusted input; ignores replay or reentrancy; ignores feature-flag influence on security behavior.

- [C] SCOPE-003 Record which guarantees the library enforces and which guarantees the caller must remember manually.
  Why: The highest-impact bugs often live at the boundary between documentation and actual enforcement.
  Applies: unchecked constructors, finalize patterns, transcript setup APIs, prepared verification contexts.
  Misalignment: caller required to remember subgroup checks; finalize step optional; docs imply stronger guarantees than code.

- [H] SCOPE-004 Bind parameter, setup, and trusted-material provenance into the threat model up front.
  Why: Many protocol failures are substitution or downgrade bugs rather than local arithmetic mistakes.
  Applies: SRS files, Powers of Tau output, proving and verification keys, program images, parameter registries.
  Misalignment: arbitrary parameters accepted; setup version ignored; trapdoor or provenance assumptions undocumented.

## 2. Input Objects and Validation

### Purpose

Treat parsing and validation as a security boundary. If objects are attacker-supplied, validation must happen before any meaningful operation, not after a convenience conversion.

### Checks

- [C] INPUT-001 Every externally supplied point or proof object is validated before use.
  Why: Parseable is not equivalent to safe or well-formed for cryptographic objects.
  Applies: curve points, signatures, proofs, receipts, commitments, verifier keys loaded from disk or wire.
  Misalignment: deserialize-then-use; lazy validation only on some code paths; wrapper bypassing underlying checks.

- [C] INPUT-002 On-curve and subgroup checks are treated as distinct requirements when the curve requires them.
  Why: A point may lie on the curve while still living outside the intended subgroup.
  Applies: BLS12-381, pairing-based systems, external G1 and G2 inputs, prepared-key loading.
  Misalignment: only on-curve checked; cofactor assumption undocumented; subgroup verified only in one entrypoint.

- [H] INPUT-003 Identity, infinity, zero, and sentinel values are handled explicitly rather than by accident.
  Why: Boundary values often trigger exceptional formulas or vacuous verification equations.
  Applies: point decoding, scalar multiplication, batch inversion, public input normalization, receipt decoding.
  Misalignment: `(0,0)` accepted as non-identity; identity treated as ordinary key; zero challenge or scalar admitted unexpectedly.

- [H] INPUT-004 Length, structure, and nesting bounds are enforced before expensive work begins.
  Why: Robust cryptography code still fails operationally if malformed inputs can trigger panics or resource spikes.
  Applies: proof blobs, Merkle paths, nested transcript states, recursive proof payloads, calldata decoders.
  Misalignment: panic on short input; deeply nested object accepted; length checks only after allocation.

- [M] INPUT-005 Validation failures are explicit errors and never silently coerce to default values.
  Why: Silent normalization creates exploit paths that look like successful parsing.
  Applies: scalar parsing, point decompression, optional challenge fields, verifier wrapper adapters.
  Misalignment: parse failure maps to zero or identity; default struct returned; malformed field treated as absent.

## 3. Algebra and Group Semantics

### Purpose

Review the actual mathematical domain the code inhabits, including scalar/base-field separation, group formulas, and pairing semantics. Rust memory safety does not protect against algebraic misuse.

### Checks

- [C] ALG-001 Scalar, base-field, and extension-field operations stay in their intended domains throughout the code path.
  Why: A value can have the right bit width while still being mathematically invalid for the intended operation.
  Applies: `ff` and `group` trait implementations, `crypto-bigint`, `ruint`, non-native gadgets, hash-to-field, challenge derivation, embedded-curve systems.
  Misalignment: scalar treated as base field; extension element reduced incorrectly; challenge field differs across prover and verifier; limb truncation changes field semantics.

- [C] ALG-002 Group-slot semantics stay exact for pairing and multi-pairing code.
  Why: Swapping G1, G2, GT, or prepared representations can preserve type shape while breaking protocol meaning.
  Applies: BLS verification, KZG commitments, pairing product checks, prepared verifying keys.
  Misalignment: pairing arguments reversed; Miller-loop output compared before final exponentiation; prepared and raw points mixed.

- [H] ALG-003 Coordinate-system and exceptional-case handling are reviewed for incomplete formulas and edge values.
  Why: Mixed affine/projective code often assumes preconditions that adversarial inputs violate.
  Applies: mixed-add formulas, batch normalization, Jacobian conversions, MSM helper code.
  Misalignment: incomplete formulas reachable; batch inversion with zero inputs; affine conversion assumes non-zero denominator.

- [H] ALG-004 Boundary values and reduction paths are exercised for zero, one, modulus minus one, modulus, and modulus plus one.
  Why: Many arithmetic bugs only appear at the boundaries where reduction or inversion rules change.
  Applies: field element parsing, challenge reduction, scalar serialization, limb decomposition.
  Misalignment: inversion of zero accepted; overflow hidden by debug-only behavior; reduction incomplete after accumulation.

- [M] ALG-005 Integer casts and native arithmetic are never relied on as a substitute for field semantics.
  Why: Native integer overflow or truncation silently changes protocol meaning.
  Applies: limb math, parser helpers, optimized arithmetic backends, test helpers promoted into production.
  Misalignment: `as` truncation; wrapping assumed intentional; host integer math used in place of field operations.

## 4. Randomness, Nonces, and Challenges

### Purpose

Audit randomness lineage explicitly: where entropy comes from, how nonces are derived, and whether challenges are bound to the full intended context.

### Checks

- [C] RNG-001 Production randomness comes from a cryptographically secure source on every supported platform.
  Why: A strong protocol becomes brittle if fallback or platform-specific entropy is weak.
  Applies: `CryptoRng` APIs, `OsRng`, `wasm` and `no_std` targets, hardware-backed RNG shims.
  Misalignment: test RNG reachable in production; `thread_rng()` used alone; platform fallback undocumented.

- [C] RNG-002 Deterministic nonces bind secret key, message, and protocol context exactly as required by the scheme.
  Why: Nonce reuse or partial binding can recover long-term secrets.
  Applies: ECDSA, Schnorr, BIP340, RFC6979 flows, `schnorrkel`, deterministic proof blinding.
  Misalignment: message omitted; context omitted; retry path reuses nonce state.

- [H] RNG-003 Session-oriented protocols guarantee fresh nonces and per-session state separation.
  Why: Threshold and MPC failures frequently come from state reuse across sessions rather than from local math errors.
  Applies: FROST, DKG, threshold ECDSA, coordinator-driven signing, batching protocols.
  Misalignment: concurrent sessions share nonce pool; participant set omitted; abort/retry reuses ephemeral values.

- [H] RNG-004 Challenge derivation is deterministic, unbiased, and aligned across prover and verifier.
  Why: Challenge mismatch can be a soundness bug even when both sides appear locally deterministic.
  Applies: Fiat-Shamir transforms, field reduction of hashes, batch challenges, recursive proving.
  Misalignment: truncation bias; prover and verifier use different field; challenge derived before all data absorbed.

## 5. Transcript, Domain Separation, and Binding

### Purpose

Transcript completeness and context binding are high-risk areas. A missing field, missing label, or wrong absorb order can invalidate the security proof of an otherwise correct construction.

### Checks

- [C] TRN-001 Every logically distinct context has explicit domain separation tags.
  Why: Reusing the same hash or transcript state across contexts enables replay and collision-style protocol confusion.
  Applies: Merlin, Poseidon, `ark-sponge`, `neptune`, Keccak, SHA-2/3, BLAKE2/3, EIP-712, recursive proof domains.
  Misalignment: same tag reused across protocol versions; role tag missing; chain or contract context omitted; hash family or round-constant set drifts across components.

- [C] TRN-002 All prior prover messages are absorbed before any challenge or random oracle query is derived.
  Why: An incomplete transcript creates classic “challenge too early” vulnerabilities.
  Applies: Sigma protocols, SNARK provers, FROST commitments, batch verification transcripts.
  Misalignment: commitment omitted; branch-specific absorb order; challenge sampled before finalize step.

- [H] TRN-003 Prover and verifier absorb data in the same order and representation.
  Why: Matching values with mismatched encoding or ordering is still a protocol break.
  Applies: proof systems, aggregate signatures, batch verification, zkVM receipt validation.
  Misalignment: prover uses field elements while verifier uses bytes; input order differs; length-prefix rules diverge.

- [H] TRN-004 Binding includes protocol version, role, participant context, and environment when those dimensions matter.
  Why: The strongest local transcript can still be replayable if its external context is not bound.
  Applies: threshold sessions, chain-bound signatures, cross-contract verifiers, bridge attestations.
  Misalignment: version omitted; participant ordering omitted; contract or fork context omitted.

- [M] TRN-005 Transcript APIs make unsafe states difficult to reach.
  Why: APIs that require callers to remember many mandatory absorbs invite misuse even when the implementation is sound.
  Applies: builder/finalize flows, transcript cloning, reusable transcript objects, wrapper abstractions.
  Misalignment: early squeeze allowed; clone-reuse permitted; resume-from-state bypasses required binds.

## 6. Serialization and Cross-Boundary Encoding

### Purpose

Serialization bugs are often the easiest way for mathematically correct code to become exploitable in real systems. Audit wire formats, canonicality, and boundary wrappers explicitly.

### Checks

- [C] SER-001 Exactly one accepted encoding exists for each valid object, and non-canonical encodings are rejected.
  Why: Multiple encodings for the same object create malleability and cross-implementation divergence.
  Applies: points, scalars, proofs, signatures, verifier keys, receipts, commitment openings.
  Misalignment: leading zeros accepted; redundant forms accepted; decode-then-encode silently normalizes attacker input.

- [H] SER-002 Serialization formats match the real wire protocol rather than internal memory layout or incidental `serde` defaults.
  Why: Internal representation compatibility is not the same as interoperability.
  Applies: SEC1, ark-serialize, ABI encoding, serde wrappers, RLP, SSZ, SCALE, binary proof formats, Circom public inputs.
  Misalignment: `serde` output used as wire format; endianness differs across layers; internal layout leaked as protocol bytes; field-element packing differs across ecosystems.

- [H] SER-003 Boundary wrappers preserve canonicality, lengths, and semantic role across FFI, calldata, and verifier boundaries.
  Why: Attackers often target the wrapper rather than the core cryptography implementation.
  Applies: EVM verifier contracts, Rust FFI, precompile adapters, `ark-circom` / `circom-compat` bridges, zkVM host wrappers, SDK serializers.
  Misalignment: calldata parser omits length checks; FFI wrapper trusts foreign return; proof bytes wrapped without validation; Circom and Rust disagree on public-input ordering.

- [M] SER-004 Cross-version and cross-implementation compatibility is intentionally checked rather than assumed.
  Why: A system can be locally correct and still fail operationally when another implementation interprets bytes differently.
  Applies: mixed-language stacks, legacy SDKs, bridge verifiers, client/server cryptography boundaries.
  Misalignment: version byte ignored; legacy format accepted without context; proof format drifts across releases.

## 7. Side-Channel and Secret Exposure

### Purpose

If secrets are involved, audit not only algorithmic soundness but also how information leaks through branches, memory access, formatting, logs, and utility traits.

### Checks

- [C] SIDE-001 Secret-dependent branches, indexing, and memory access patterns are absent from security-critical paths.
  Why: Timing and cache side channels frequently recover secrets without violating any functional tests.
  Applies: scalar multiplication, nonce derivation, hash-to-curve, witness handling, key comparison paths.
  Misalignment: branch on secret bit; table lookup by secret index; early exit on secret-related condition.

- [H] SIDE-002 Constant-time helper crates are used correctly rather than partially or cosmetically.
  Why: Converting a constant-time abstraction back into ordinary control flow defeats its purpose.
  Applies: `subtle::Choice`, constant-time equality helpers, branchless selection wrappers.
  Misalignment: `Choice` converted to `bool`; constant-time compare result used in early return; helper only wraps half the branch.

- [H] SIDE-003 Secret-bearing values are not exposed through formatting, logs, panic text, or overly informative error paths.
  Why: Operational telemetry is often a faster exfiltration path than cryptanalysis.
  Applies: `Debug`, `Display`, structured errors, panic messages, telemetry wrappers, CLI tooling.
  Misalignment: key material printed; error reveals exact secret-dependent check failure; witness values land in logs.

- [M] SIDE-004 Zeroization and secret-lifetime controls cover owned values, clones, and temporary buffers.
  Why: Secrets survive in memory if only the happy-path owner is cleaned up.
  Applies: `zeroize`, `ZeroizeOnDrop`, `Vec` reallocation, derived clones, temporary parsing buffers.
  Misalignment: cloned secret never cleared; heap buffer reused; derived wrapper lacks zeroization behavior.

## 8. Rust Safety, APIs, and State Management

### Purpose

Memory safety and API design still matter in cryptography reviews, especially around `unsafe`, unchecked constructors, builder flows, and shared mutable state.

### Checks

- [H] RUST-001 Every `unsafe` block has a documented invariant that is re-verified at each call site.
  Why: `unsafe` is often used in performance-critical code where silent invariant drift is easy.
  Applies: transmute paths, raw-pointer conversion, SIMD backends, FFI shims, manual serialization.
  Misalignment: invariant undocumented; caller assumptions changed; unchecked length or alignment trusted.

- [H] RUST-002 Unchecked constructors and assumption-carrying APIs are clearly isolated from ordinary safe flows.
  Why: Once unchecked entrypoints are easy to call, trust-state separation collapses.
  Applies: `_unchecked` constructors, `assume_valid`, prepared verifier builders, raw transcript restore APIs.
  Misalignment: unchecked API re-exported broadly; caller can skip required validation; safe and unsafe wrappers look identical.

- [H] RUST-003 Builder, finalize, and state-machine APIs enforce required sequencing.
  Why: Multi-step protocol APIs that rely on comments rather than type states tend to be misused.
  Applies: transcript setup, proof finalization, nonce commitment flows, verification builders.
  Misalignment: finalize optional; state reused after finalize; missing mandatory bind step.

- [H] RUST-004 Concurrency and shared state cannot duplicate, race, or leak cryptographic state.
  Why: Many nonce, cache, and transcript bugs emerge only under concurrent execution.
  Applies: global caches, `OnceCell`, shared RNG state, batch verification workers, parallel provers.
  Misalignment: shared nonce pool; unsound `Send/Sync`; race in precomputed table initialization.

- [M] RUST-005 Error handling remains robust under attacker-controlled inputs and variant backends.
  Why: Panic-or-default behavior creates exploit surfaces even when the math layer is correct.
  Applies: parser wrappers, FFI return handling, backend fallbacks, CLI helpers that become library code.
  Misalignment: `unwrap` on attacker input; panic in verifier path; fallback to default object on backend failure.

## 9. Protocol Families

### Purpose

Review the protocol family actually implemented instead of forcing all systems into one generic checklist. Different proof families, aggregation schemes, and recursion systems fail in different ways.

### Dimensions / scenarios

This section should be used as a routing layer for protocol-specific concerns:

- pairing and BLS systems
- Groth16, Marlin, and R1CS systems
- Plonkish systems and polynomial commitments
- STARK and FRI systems
- folding, accumulation, and recursion systems
- threshold, MPC, DKG, and FROST systems
- zkVM guest-host proving systems

### Checks

- [C] PROTO-001 Pairing and BLS systems preserve correct subgroup validation, rogue-key resistance, and exact verification equations.
  Why: Small subgroup and rogue-key mistakes often produce practical forgery vulnerabilities.
  Applies: BLS signatures, KZG commitments, pairing-product verifiers, aggregate signatures.
  Misalignment: PoP omitted; subgroup checks missing; pairing equation or sign convention differs from spec.

- [C] PROTO-002 Groth16, Marlin, and R1CS-based systems bind the exact circuit, public input encoding, and proving/verification setup.
  Why: These schemes depend heavily on setup identity and public input ordering.
  Applies: arkworks Groth16 and Marlin, bellman, bellperson, R1CS gadget ecosystems, `ark-circom` / `circom-compat`.
  Misalignment: wrong circuit ID accepted; public input order mismatch; setup file substitution; Circom witness/public-input conventions drift from Rust verifier assumptions.

- [H] PROTO-003 Plonkish systems validate the polynomial commitment scheme, lookup semantics, and transcript wiring end to end.
  Why: Plonkish systems are especially sensitive to PCS choice, transcript completeness, and custom-gadget wiring.
  Applies: halo2, `jellyfish`, `kimchi`, KZG- and IPA-based systems, custom lookup and gate frameworks.
  Misalignment: PCS mismatch; lookup multiplicity assumptions drift; transcript omits custom gate context.

- [H] PROTO-004 STARK and FRI systems preserve arithmetization, commitment, and verifier-parameter consistency.
  Why: STARK-family bugs often come from mismatched trace semantics or verifier parameter drift rather than from curve issues.
  Applies: Plonky3, plonky2 and `starky`, winterfell, stwo, boojum, FRI-based wrappers.
  Misalignment: trace semantics mismatch; FRI parameters drift; verifier uses different blowup or domain assumptions.

- [H] PROTO-005 Folding, accumulation, and recursion systems bind inner statements, accumulators, and verification context exactly.
  Why: Recursive proofs can look locally valid while silently forgetting what they are supposed to attest to.
  Applies: Nova, Sonobe, recursive Plonkish systems, recursive zkVM receipts.
  Misalignment: accumulator reused across statements; inner VK digest omitted; recursion depth or context not bound.

- [C] PROTO-006 Threshold, MPC, and DKG systems bind participant identity, session state, and share verification equations.
  Why: Session and participant confusion is a primary failure mode in distributed cryptography.
  Applies: FROST, `threshold_crypto`, threshold BLS, threshold ECDSA, `multi-party-ecdsa`, `synedrion`, DKG, MPC coordination layers.
  Misalignment: share indices collide; session separation missing; coordinator can bias or replay state.

- [H] PROTO-007 zkVM systems bind host inputs, guest program identity, public outputs, and verifier-wrapper assumptions.
  Why: zkVM boundaries are easy to misuse because the proof object hides a complex host/guest protocol.
  Applies: risc0, sp1, Jolt, nexus-zkvm, receipt and journal wrappers.
  Misalignment: host input omitted; image ID unchecked; journal interpreted without full verifier binding.

## 10. Ethereum and On-Chain Boundaries

### Purpose

Ethereum integrations add their own security boundary: precompiles, calldata, ABI encoding, chain/fork context, and KZG-specific setup flows. These concerns deserve a dedicated section rather than being scattered across generic serialization or transcript checks.

### Dimensions / scenarios

Focus especially on:

- precompile wrappers and gas-sensitive validation shortcuts
- ABI, calldata, and contract-side decoding of cryptographic objects
- EIP-712 signing domains and contract-bound signatures
- EIP-4844 blob commitments, KZG setup material, and verifier wrappers
- L1/L2 bridge, rollup, and fork-domain context binding

### Checks

- [C] ETH-001 On-chain and off-chain components agree on encoding, endian rules, and field interpretation exactly.
  Why: Boundary mismatches between Rust and EVM layers frequently become acceptance or replay bugs.
  Applies: ABI codecs, verifier wrappers, calldata decoders, bridge adapters, precompile wrappers.
  Misalignment: Rust uses compressed encoding while contract expects uncompressed; endian mismatch; field element padding differs.

- [H] ETH-002 Signatures and transcript-derived artifacts are bound to chain, fork, application, and contract context where required.
  Why: Ethereum systems often reuse cryptography across multiple contracts and networks.
  Applies: EIP-712, rollup proofs, bridge attestations, contract-scoped authorizations.
  Misalignment: chain ID omitted; contract address omitted; same signature valid on multiple forks.

- [H] ETH-003 KZG and blob-commitment flows validate setup provenance and verifier assumptions explicitly.
  Why: Trusted setup misuse or wrapper confusion can break security without touching local arithmetic code.
  Applies: EIP-4844 KZG libraries, setup loaders, proof verifiers, Rust bindings to C implementations.
  Misalignment: setup file accepted without version/provenance; binding wrapper trusts foreign return codes; proof and commitment domains mismatch.

- [H] ETH-004 Precompile and verifier wrappers preserve the same validation guarantees as the underlying cryptographic primitive.
  Why: Wrappers often optimize away checks that the math layer assumes already happened.
  Applies: BN254 precompiles, `revm` integrations, verifier adapters, SDK convenience layers, and Banderwagon or Verkle-adjacent wrappers where present.
  Misalignment: wrapper skips subgroup check; contract path validates less than native path; gas optimization changes semantics.

- [M] ETH-005 Bridge and L1/L2 systems bind message root, epoch, and environment context into the audited proof or signature path.
  Why: Cross-domain replay bugs often appear at message-boundary layers rather than inside the proof system itself.
  Applies: rollup bridges, light clients, message relays, receipt-verification wrappers.
  Misalignment: wrong root reused; epoch omitted; domain separator identical across L1 and L2.

## 11. Testing, Differential Validation, and Evidence

### Purpose

A checklist is only operationally useful if reviewers can reproduce findings, compare behavior across implementations, and prove generated outputs stay in sync with canonical sources.

### Checks

- [H] TEST-001 Negative tests exist for malformed points, malformed proofs, tampered signatures, and invalid transcript context.
  Why: Positive-path coverage alone cannot show that validation actually rejects dangerous input.
  Applies: parser tests, verifier tests, contract wrappers, bridge-boundary tests.
  Misalignment: only happy path tested; malformed input panics instead of returning error; invalid context accepted.

- [H] TEST-002 Boundary and differential tests compare behavior across representative implementations or variants where feasible.
  Why: Many dimensional mismatches only appear when two implementations or backends disagree.
  Applies: blst vs blstrs wrappers, portable vs asm backends, Rust vs contract verifiers, legacy vs current SDKs.
  Misalignment: one backend accepts non-canonical input; contract and Rust verifier disagree; setup version mismatch only visible cross-impl.

- [M] TEST-003 Fuzzing or property-style tests exist for high-risk parsers, hash-to-curve, and proof-verification boundaries.
  Why: Structured malformed input is hard to enumerate manually.
  Applies: deserializers, decompression paths, wrapper decoders, transcript parsers.
  Misalignment: rare malformed input bypasses validation; parser panics; degenerate field element path unexplored.

- [M] TEST-004 Each significant finding can be grounded in a reproducible artifact, test, or exact code path.
  Why: Audits degrade when findings remain unverifiable hypotheses.
  Applies: PoC tests, minimized fixtures, failing verifier cases, renderer drift checks for generated artifacts.
  Misalignment: bug reported without reproducer; expected failure mode unclear; no artifact ties the finding back to source.

- [M] TEST-005 Generated checklist artifacts remain synchronized with the canonical source through an explicit validation step.
  Why: A dual-format system is only trustworthy if drift is detectable immediately.
  Applies: `scripts/render_checklists.py --check`, generated markdown outputs, future CI drift checks.
  Misalignment: human and LLM variants diverge silently; generated files hand-edited; canonical source no longer matches outputs.
