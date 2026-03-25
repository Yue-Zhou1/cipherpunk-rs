ARG RUST_BASE_IMAGE=rust
FROM ${RUST_BASE_IMAGE}

ARG CARGO_FUZZ_VERSION
ARG CARGO_AUDIT_VERSION

RUN cargo install cargo-fuzz --version ${CARGO_FUZZ_VERSION} --locked
RUN cargo install cargo-audit --version ${CARGO_AUDIT_VERSION} --locked

LABEL tool.cargo-fuzz.version="${CARGO_FUZZ_VERSION}"
LABEL tool.cargo-audit.version="${CARGO_AUDIT_VERSION}"
ENV PATH="/usr/local/cargo/bin:${PATH}"
WORKDIR /workspace
