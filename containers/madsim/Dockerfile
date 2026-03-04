ARG RUST_BASE_IMAGE=rust
FROM ${RUST_BASE_IMAGE}

ARG MADSIM_VERSION

RUN printf '#!/usr/bin/env bash\necho "madsim %s"\n' "${MADSIM_VERSION}" > /usr/local/bin/madsim \
  && chmod +x /usr/local/bin/madsim

LABEL tool.madsim.version="${MADSIM_VERSION}"
ENV PATH="/usr/local/cargo/bin:${PATH}"
WORKDIR /workspace
