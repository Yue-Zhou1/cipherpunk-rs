ARG RUST_BASE_IMAGE=rust
FROM ${RUST_BASE_IMAGE}

ARG MIRI_RUSTC_TOOLCHAIN

RUN rustup toolchain install ${MIRI_RUSTC_TOOLCHAIN} --component miri

LABEL tool.miri.rustc.version="${MIRI_RUSTC_TOOLCHAIN}"
ENV PATH="/usr/local/cargo/bin:${PATH}"
WORKDIR /workspace
