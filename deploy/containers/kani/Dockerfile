ARG RUST_BASE_IMAGE=rust
FROM ${RUST_BASE_IMAGE}

ARG KANI_VERSION
ARG KANI_RUSTC_TOOLCHAIN

RUN cargo install kani-verifier --version ${KANI_VERSION} --locked
RUN rustup toolchain install ${KANI_RUSTC_TOOLCHAIN}

LABEL tool.kani.version="${KANI_VERSION}"
LABEL tool.rustc.version="${KANI_RUSTC_TOOLCHAIN}"
ENV PATH="/usr/local/cargo/bin:${PATH}"
WORKDIR /workspace
