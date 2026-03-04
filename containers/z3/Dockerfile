ARG UBUNTU_BASE_IMAGE=ubuntu
FROM ${UBUNTU_BASE_IMAGE}

ARG Z3_VERSION

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    git \
    python3 \
    build-essential \
  && rm -rf /var/lib/apt/lists/*

RUN git clone --depth 1 --branch z3-${Z3_VERSION} https://github.com/Z3Prover/z3.git /tmp/z3 \
  && cd /tmp/z3 \
  && python3 scripts/mk_make.py --prefix=/usr/local \
  && cd build \
  && make -j"$(nproc)" \
  && make install \
  && rm -rf /tmp/z3

LABEL tool.z3.version="${Z3_VERSION}"
WORKDIR /workspace
