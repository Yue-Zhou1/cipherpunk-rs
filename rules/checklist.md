# Comprehensive Audit Checklist for Cryptographic Rust Libraries

## 1. Scope and Threat Model

Before reading code, define the security boundary clearly.

### 1.1 What kind of library is this?

Determine whether it is primarily:

- a primitive implementation library
    - field arithmetic
    - curve operations
    - hash functions
    - pairings
    - FFT / MSM / polynomial commitments
- a protocol library
    - signatures
    - proof systems
    - threshold schemes
    - MPC
    - anonymous credentials
- a wrapper / glue layer
    - serialization
    - transcript wrappers
    - verifier API
    - FFI bridge
    - bindings to other languages

The audit priorities differ significantly by category.

### 1.2 What is the attacker allowed to do?

Clarify whether the attacker can:

- provide arbitrary malformed inputs
- control public inputs, witness values, messages, or transcript messages
- observe timing or memory behavior
- trigger parsing on untrusted data
- call APIs concurrently
- influence build flags, feature flags, or runtime environment
- replay old proofs, signatures, sessions, or nonces
- mix objects across protocol contexts

### 1.3 What security properties does the library claim?

Identify the exact properties claimed, for example:

- correctness
- soundness
- completeness
- binding / hiding / extractability
- EUF-CMA / IND-CPA / IND-CCA
- zero-knowledge / witness indistinguishability
- constant-time behavior
- misuse resistance
- deterministic encoding / canonical serialization

### 1.4 Which guarantees are enforced by the library, and which are pushed onto the caller?

This is a major source of bugs.

Check whether:

- the API enforces correct usage by construction
- callers must remember critical checks manually
- required preconditions are explicit in docs
- unsafe or unchecked modes are too easy to misuse

### 1.5 Where do parameters / setup material come from?

If the library uses CRS/SRS, proving keys, verification keys, public parameters, or setup output, check:

- source of parameters
- format and versioning
- whether the context is bound
- whether arbitrary parameters can be supplied
- whether parameters are checked before use

### 1.6 Are formats stable across versions and platforms?

Check whether:

- encodings are canonical
- versions are explicit
- format changes are detectable
- different backends produce the same bytes
- endianness is consistent
- round-trip behavior is stable

### 1.7 What platform assumptions exist?

Review assumptions around:

- `std` vs `no_std`
- `wasm`
- x86_64 vs ARM
- SIMD / assembly backends
- nightly features
- platform-specific entropy sources
- fallback vs optimized code paths

### 1.8 What are the security-critical boundaries?

Usually the highest-risk code is around:

- parsing / deserialization
- verification
- finalization
- challenge generation
- transcript absorb/squeeze
- key generation
- nonce generation
- parameter loading
- unsafe blocks
- FFI boundaries
- batch verification
- aggregation
- caches / precomputation

---

## 2. Cryptographic Correctness

This is often the most important layer. A Rust library can be memory-safe and still be cryptographically broken.

### 2.1 Field elements / scalars / big integers

Check:

- whether non-canonical encodings are accepted
- whether reduction is complete and correct
- whether conversion into field/scalar space is well defined
- whether overflow or truncation changes semantics
- whether zero, one, minus one, modulus minus one, and boundary values are handled safely
- whether inversion of zero is rejected or safely handled
- whether division can silently fail
- whether byte-to-field mappings are ambiguous or biased

### 2.2 Elliptic curve points

Check:

- on-curve validation
- subgroup membership checks
- cofactor clearing where required
- rejection of invalid points
- treatment of the identity / point at infinity
- compressed and uncompressed decoding paths
- whether decoding revalidates the point
- resistance to invalid-curve and small-subgroup attacks

### 2.3 Pairing and extension-field objects

Check:

- separation of G1, G2, scalar field, base field, and target group types
- whether all inputs to pairings are validated
- whether target group elements are validated after parsing
- whether comparison happens in the correct representation
- whether final exponentiation assumptions are respected

### 2.4 Group operations

Check:

- whether mixed-coordinate arithmetic is correct
- whether exceptional cases are handled properly
- whether incomplete formulas can be triggered unexpectedly
- whether scalar multiplication handles zero and edge cases
- whether batch normalization or inversion is safe in degenerate cases

---

## 3. Randomness, Nonces, and Sampling

### 3.1 RNG source

Check whether the implementation uses a cryptographically secure RNG and whether that remains true across all platforms.

Review:

- use of `CryptoRng` vs ordinary RNG traits
- accidental use of deterministic test RNGs in production
- entropy source quality in `wasm` or `no_std`
- fallback RNG behavior
- seeding behavior
- whether the caller can inject insecure randomness too easily

### 3.2 Nonce generation

For signatures, proofs, encryption, or threshold protocols, review:

- nonce reuse risk
- deterministic nonce construction
- whether nonce derivation is bound to message, secret key, domain, and context
- whether failures or retries can reuse state
- whether concurrency can duplicate or race nonce generation

### 3.3 Rejection sampling / Gaussian sampling / challenge sampling

Check:

- whether rejection conditions are correct
- whether output distribution is biased
- whether modulo bias exists
- whether loops can be used for denial-of-service
- whether timing varies in a secret-dependent way
- whether edge-case inputs can force excessive retries

## 4. Hashing, Transcript, and Fiat-Shamir

This is a high-risk area in signatures, proofs, commitments, and MPC.

### 4.1 Domain separation

Check whether all logically distinct contexts are separated, such as:

- key generation
- proving vs verifying
- signing vs verification
- commitment vs opening
- batch verification
- aggregation
- recursion levels
- protocol rounds
- session identifiers

Also check:

- type tags
- role tags
- protocol version tags
- chain or network identifiers
- length prefixes to avoid ambiguity

### 4.2 Absorb/squeeze order

Check:

- prover and verifier absorb in exactly the same order
- no branch-dependent transcript divergence
- no omitted public inputs or context fields
- no challenge sampled before all required data is absorbed
- recursive or aggregated proofs fully bind the inner statement

### 4.3 Challenge derivation

Check:

- hash-to-field correctness
- challenge space size
- truncation safety
- absence of modulo bias
- rejection of invalid or special challenge values if necessary
- deterministic behavior across platforms and backends

### 4.4 Transcript API design

Check whether the API makes misuse easy.

Red flags include:

- caller must remember to absorb critical fields manually
- transcript can be cloned and reused unsafely
- squeeze can be called too early
- state can be serialized and resumed in insecure ways
- the same transcript object can be reused across logically separate contexts

---

## 5. Serialization, Parsing, and Canonical Encoding

This is one of the most common sources of practical vulnerabilities.

### 5.1 Canonical encoding

Check:

- whether there is exactly one accepted encoding for a valid object
- whether non-canonical encodings are rejected
- whether decode-then-encode normalizes representation
- whether leading zeros, trailing data, or redundant forms are accepted
- whether compressed and uncompressed forms are consistently validated

### 5.2 Structural parsing

Check:

- length checks
- maximum length bounds
- nested structure limits
- denial-of-service via deeply nested or large objects
- parsing failures return errors rather than panics
- no silent fallback to zero, identity, or default values

### 5.3 Cross-language / cross-version compatibility

Check:

- compatibility with other implementations
- stability across crate versions
- version tags in serialized data
- endianness assumptions
- `serde` behavior and whether it matches the intended wire format
- whether internal memory layout is ever confused with serialized format

Focus especially on:

- proofs
- signatures
- public keys
- secret keys
- curve points
- scalars
- Merkle paths
- commitment openings
- verification keys / proving keys / setup files

---

## 6. Constant-Time and Side-Channel Resistance

Not every library must be constant-time, but if secrets are involved, side-channel review matters.

### 6.1 Secret-dependent control flow

Check for:

- secret-dependent branches
- secret-dependent array indexing
- secret-dependent memory access patterns
- early exits based on secret-related conditions
- variable-time equality checks on secrets
- variable-time inversion / reduction / multiplication where secrets are involved

### 6.2 Rust-specific constant-time pitfalls

Check:

- correct use of crates like `subtle`
- whether `Choice` values are converted to `bool` too early
- whether compiler optimizations could undermine intended constant-time behavior
- whether debug-only behavior differs materially
- whether panic paths or assertions expose secret-dependent behavior

### 6.3 Secret leakage via logs, errors, and debug output

Check:

- `Debug` / `Display` on secret-bearing types
- logs containing private key material, nonces, witness data, seeds, or intermediate values
- panic messages or backtraces exposing secrets
- metrics or traces leaking secret sizes, branches, or errors

### 6.4 Memory clearing

Check:

- use of zeroization for secret material
- whether all copies are cleared, not just one instance
- heap buffers, stack buffers, temporary vectors, and caches
- whether clones of secrets remain in memory
- whether secrets are moved into immutable strings or logs
- whether drop behavior is reliable

---

## 7. Protocol Binding and Context Integrity

This is one of the most common causes of severe cryptographic bugs.

Check whether proofs, signatures, commitments, ciphertexts, and protocol messages are fully bound to:

- message contents
- statement / instance
- public inputs
- domain / protocol identifier
- version
- chain ID / network ID / application ID
- verification key / proving key / SRS / CRS
- participant identity
- session ID / round ID
- role (prover, verifier, signer, coordinator, aggregator)
- recursion depth / inner statement
- zkVM program hash / image ID / guest binary hash
- state root / note root / Merkle root / epoch / block number where applicable

Typical failure modes:

- verifying “something” but not the intended context
- failing to bind parameter sets
- accepting reordered public inputs
- forgetting to bind the verification key or circuit ID
- cross-protocol or cross-session replay

---

## 8. Rust Implementation Safety

### 8.1 Unsafe blocks

Every `unsafe` block should be reviewed carefully.

Check:

- whether there is a documented safety invariant
- whether the invariant actually holds at every call site
- aliasing
- uninitialized memory
- out-of-bounds access
- alignment assumptions
- pointer validity
- `MaybeUninit` usage
- `transmute`
- raw pointer arithmetic
- `set_len`
- `from_raw_parts`
- zero-copy parsing assumptions

### 8.2 FFI boundaries

Check:

- length validation on inbound buffers
- null-pointer handling
- ownership conventions
- allocator compatibility
- validation of data returned from foreign code
- platform-dependent ABI assumptions
- whether parsed foreign values are trusted without validation

### 8.3 Assembly / SIMD / optimized backends

Check:

- equivalence of optimized and fallback paths
- constant-time consistency across implementations
- feature detection correctness
- platform guards
- whether unsupported CPUs can enter the wrong path
- whether optimization changes semantics at edge cases

### 8.4 Integer arithmetic

Check:

- intentional vs accidental wrapping arithmetic
- `as` casts that truncate or reinterpret values
- multiplication or indexing overflow
- debug vs release differences
- window size / limb count / degree calculations
- counter wraparound
- signed/unsigned conversions

### 8.5 Error handling

Check:

- malformed input never triggers a panic on attacker-controlled paths
- verification failure returns a clean error / false
- no silent fallback to default or zero values
- internal invariant violations are distinguished from user input failures
- `unwrap` / `expect` are not reachable from untrusted input paths
- batch failures do not hide partial invalidity

## 9. Types, Traits, and API Misuse Resistance

Cryptographic safety often depends on good type design.

### 9.1 Strong type separation

Check whether the type system separates:

- verified vs unverified objects
- G1 vs G2 vs scalar vs field elements
- public keys vs secret keys
- signatures vs commitments vs hashes
- proving keys vs verification keys
- one curve / parameter set from another
- one protocol context from another

### 9.2 Dangerous constructors

Review any API like:

- `unchecked`
- `from_raw`
- `from_bytes_unchecked`
- `assume_valid`
- `new_unchecked`

Check whether:

- they are clearly marked
- docs are explicit
- safe alternatives exist
- misuse is easy and likely

### 9.3 Builders and finalize patterns

Check whether the library forces correct sequencing or leaves critical steps optional.

Examples:

- validation required but not enforced
- finalize step easy to forget
- transcript setup incomplete unless caller remembers several calls
- multi-round protocols missing type-state protections

### 9.4 Secret-bearing traits

Check whether secret types implement traits that are risky:

- `Debug`
- `Display`
- `Clone`
- `Copy`
- `Serialize`
- `Deserialize`
- `Eq` / `Ord` in ways that encourage unsafe use

---

## 10. Concurrency, Shared State, and Determinism

Many cryptographic crates are later used in multithreaded systems.

Check:

- thread safety of global caches
- `lazy_static` / `OnceCell` / `OnceLock` initialization
- `unsafe impl Send/Sync`
- shared transcript state
- shared RNG or nonce state
- mutable precomputation tables
- race conditions in batch verification or proving
- determinism under parallel execution
- thread-local behavior affecting reproducibility

Especially important for:

- nonce generation
- proof generation
- batch verification
- caches of precomputed points or FFT domains

---

## 11. Secret Lifetime and Data Hygiene

Check:

- zeroization of secret keys, witness data, seeds, nonces, and ephemeral secrets
- copies made during serialization or formatting
- temporary buffers
- reallocation of vectors containing secrets
- swap, mmap, or persistence layers
- crash dumps
- memory snapshots
- test fixtures containing real secret material

---

## 12. Higher-Level Protocol Checks

If the library is not just a primitive crate, review system-level invariants too.

### 12.1 Key generation / setup / parameter loading

Check:

- integrity validation on loaded parameters
- hash checking or version checking of setup files
- handling of trusted setup output
- whether trapdoor material can accidentally persist
- session or domain binding in setup
- downgrade or substitution risk

### 12.2 Prover / verifier consistency

Check:

- exact agreement on transcript
- same public input ordering
- same challenge derivation
- same circuit ID / parameter set / version
- no feature-flag divergence
- reference and optimized backends remain equivalent

### 12.3 Batch verification and aggregation

This is a high-risk area.

Check:

- soundness of random linear combination
- independence of batching challenges
- whether one invalid item can be masked by others
- duplicate-input handling
- rogue-key resistance where applicable
- ability to recover which item failed
- correct binding of every instance in aggregate contexts

### 12.4 Threshold / MPC / DKG / FROST-style schemes

Check:

- participant indexing and ordering
- duplicate participant handling
- rogue-key resistance
- round separation
- session binding
- commitment consistency
- nonce commitments bound to message and signer
- abort/restart safety
- complaint and recovery paths
- verification of partial shares
- coordinator bias or misuse

### 12.5 zk proof system / zkDSL / circuit compiler

Check:

- completeness of constraints
- public input ordering and count
- witness generation consistent with constraints
- selectors / lookups / copy constraints / permutation arguments
- circuit / program ID binding
- verification key binding
- recursion outer-inner statement binding
- acceptance of proofs from the wrong circuit or wrong parameters
- misleading behavior of mock provers or test harnesses

### 12.6 zkVM / guest-host pipeline

Check:

- whether all host-provided inputs enter the proof
- whether public outputs / journal are fully bound
- whether program hash / image ID / version is verified
- trust placed on hints / oracles
- caching or preload behavior outside the proof boundary
- verifier wrapper checks

---

## 13. Dependencies and Supply Chain

### 13.1 Cargo dependencies

Check:

- outdated crates
- known vulnerabilities
- git dependencies pinned to immutable revisions
- hidden behavior changes through semver upgrades
- dangerous optional features
- default features changing security behavior

### 13.2 Third-party crypto crates

Check:

- maturity and review history
- known side-channel or encoding issues
- whether the current library uses them correctly
- whether assumptions in docs actually hold in the calling code

### 13.3 Build scripts / code generation / proc macros

Check:

- security-relevant code hidden behind codegen
- generated constants or tables and their provenance
- platform-specific behavior in `build.rs`
- macro-generated logic that bypasses review expectations

---

## 14. Testing and Verification Quality

### 14.1 Unit tests

Check whether tests cover:

- malformed inputs
- boundary values
- zero / identity / infinity
- non-canonical encodings
- incorrect proofs / signatures / commitments
- wrong keys / wrong parameters / wrong domains
- reordered or duplicated inputs
- stale roots / wrong epochs / wrong versions

### 14.2 Property-based tests / fuzzing

Look for:

- parser fuzzing
- serialization round-trip fuzzing
- malformed group element fuzzing
- transcript misuse fuzzing
- proof / verifier differential fuzzing
- edge-case arithmetic fuzzing

### 14.3 Differential testing

Check for comparisons between:

- optimized and reference implementations
- batch and single verification
- different backends
- Rust and external reference implementations
- platform-specific implementations

### 14.4 Negative tests

These are essential.

Ensure there are tests showing that:

- invalid inputs fail
- random bytes do not parse as valid objects
- wrong verification keys fail
- wrong statements fail
- wrong domains fail
- replayed sessions fail
- wrong ordering fails

### 14.5 Benchmarks do not leak into production semantics

Check whether benchmark-only fast paths or unchecked helpers are ever reachable in production builds.

---

## 15. Documentation and Secure Usability

Check whether:

- examples are safe by default
- caller obligations are documented clearly
- unsafe or unchecked functions are clearly labeled
- the docs explain required validation steps
- there are obvious misuse footguns
- the API encourages secure defaults rather than expert-only correct usage

A cryptographically correct library can still be dangerous if the API makes misuse easy.

## 16. Priority Review Hotspots

If time is limited, start with these modules or functions first:

### Highest priority

- `verify`, `batch_verify`, `final_verify`
- `deserialize`, `from_bytes`, `parse`, `read`
- transcript modules
- challenge generation
- `keygen`, `sign`, `prove`, `open`
- `unsafe` blocks
- FFI modules
- `unchecked` constructors
- parameter loading
- aggregation / batch verification code

### Second priority

- caches and precomputation
- zeroization logic
- type conversions
- builders / finalizers
- error conversion code
- optimized arithmetic backends

### Third priority

- docs and examples
- benches
- CLI wrappers
- integration helpers

---

## 17. Common Vulnerability Patterns to Actively Search For

- parsed objects used before validation
- missing subgroup checks
- missing on-curve checks
- accepting non-canonical encodings
- verifier missing one context-binding field
- transcript absorb order mismatch
- challenge derived too early
- replay across domains or sessions
- default or zero values used after parse failure
- unchecked constructors exposed too widely
- batch verification masking invalid items
- feature flags changing security-relevant behavior
- debug or log output leaking secrets
- nonce reuse due to shared state or retries
- unverified and verified objects sharing the same type
- wrong parameter sets accepted together
- optimized backend diverging from reference backend

---

## 18. Recommended Finding Categories for Audit Notes

When documenting issues, it is useful to classify them as:

- soundness / forgery risk
- privacy / secrecy leakage
- side-channel risk
- API misuse risk
- denial-of-service / robustness risk
- serialization / interoperability risk
- unsafe / memory safety risk
- configuration / feature-flag risk
- dependency / supply-chain risk

---

## 19. Practical Master Checklist

You can use this condensed checklist during an audit:

### Scope

- [ ]  Security goal identified
- [ ]  Threat model identified
- [ ]  Caller obligations identified

### Input validation

- [ ]  Length checks
- [ ]  Canonical parsing
- [ ]  On-curve / subgroup checks
- [ ]  No malformed-input panic

### Binding

- [ ]  Message / statement / public input binding
- [ ]  Domain / version / context binding
- [ ]  Parameter / key / circuit / program binding
- [ ]  Session / round / role binding where relevant

### Transcript

- [ ]  Domain separation complete
- [ ]  Absorb/squeeze order consistent
- [ ]  Challenge derivation sound
- [ ]  API misuse resistant

### Randomness

- [ ]  CSPRNG used
- [ ]  Nonce generation safe
- [ ]  Deterministic nonce derivation correct
- [ ]  No concurrency reuse risk

### Side-channel / secrecy

- [ ]  No secret-dependent branching where unsafe
- [ ]  Constant-time comparisons where needed
- [ ]  No secret logging / formatting
- [ ]  Zeroization complete

### Rust safety

- [ ]  Unsafe blocks justified
- [ ]  FFI boundaries validated
- [ ]  Integer casts / overflow reviewed
- [ ]  Error handling robust
- [ ]  Feature flags do not change security semantics unexpectedly

### Concurrency

- [ ]  Shared state reviewed
- [ ]  Send/Sync correctness reviewed
- [ ]  Cache races reviewed
- [ ]  Determinism under parallel execution understood

### Protocol-specific

- [ ]  Batch verification sound
- [ ]  Aggregation sound
- [ ]  Threshold / MPC session separation
- [ ]  zk proof / zkVM context binding complete

### Supply chain

- [ ]  Dependencies reviewed
- [ ]  Optional features reviewed
- [ ]  Codegen / build scripts reviewed

### Testing

- [ ]  Negative tests exist
- [ ]  Fuzzing exists
- [ ]  Differential tests exist
- [ ]  Cross-platform / cross-feature behavior tested

### Documentation

- [ ]  Safe examples
- [ ]  Clear caller obligations
- [ ]  Dangerous APIs labeled
- [ ]  Security invariants documented