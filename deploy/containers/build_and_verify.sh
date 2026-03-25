#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSIONS_FILE="${ROOT_DIR}/deploy/containers/versions.toml"
DIGESTS_FILE="${ROOT_DIR}/deploy/containers/image-digests.json"

toml_get() {
  local section="$1"
  local key="$2"
  awk -F'=' -v section="$section" -v key="$key" '
    /^\s*\[/ {
      current=$0
      gsub(/^\s*\[/, "", current)
      gsub(/\]\s*$/, "", current)
      next
    }
    current == section {
      left=$1
      gsub(/^[ \t]+|[ \t]+$/, "", left)
      if (left == key) {
        val=$2
        gsub(/^[ \t"]+|[ \t"]+$/, "", val)
        print val
        exit
      }
    }
  ' "${VERSIONS_FILE}"
}

RUST_BASE="$(toml_get images rust_base)"
UBUNTU_BASE="$(toml_get images ubuntu_base)"
KANI_VERSION="$(toml_get tools kani)"
KANI_TOOLCHAIN="$(toml_get tools kani_rustc_toolchain)"
Z3_VERSION="$(toml_get tools z3)"
MADSIM_VERSION="$(toml_get tools madsim)"
MIRI_TOOLCHAIN="$(toml_get tools miri_rustc_toolchain)"
CARGO_FUZZ_VERSION="$(toml_get tools cargo_fuzz)"
CARGO_AUDIT_VERSION="$(toml_get tools cargo_audit)"

KANI_TAG="audit-agent/kani:${KANI_VERSION}"
Z3_TAG="audit-agent/z3:${Z3_VERSION}"
MADSIM_TAG="audit-agent/madsim:${MADSIM_VERSION}"
MIRI_TAG="audit-agent/miri:${MIRI_TOOLCHAIN}"
FUZZ_TAG="audit-agent/fuzz:${CARGO_FUZZ_VERSION}-${CARGO_AUDIT_VERSION}"

build_image() {
  local dockerfile="$1"
  local tag="$2"
  shift 2

  if [[ "${USE_BUILDX_CACHE:-0}" == "1" ]]; then
    local buildx_driver
    buildx_driver="$(docker buildx inspect 2>/dev/null | awk '/Driver:/ { print $2; exit }' || true)"
    if [[ "${buildx_driver}" == "docker-container" || "${buildx_driver}" == "kubernetes" ]]; then
      local cache_dir="${DOCKER_CACHE_DIR:-${ROOT_DIR}/.docker-buildx-cache}"
      mkdir -p "${cache_dir}"
      docker buildx build \
        --load \
        --cache-from "type=local,src=${cache_dir}" \
        --cache-to "type=local,dest=${cache_dir},mode=max" \
        -f "${dockerfile}" \
        -t "${tag}" \
        "$@" \
        "${ROOT_DIR}"
    else
      echo "buildx driver '${buildx_driver:-unknown}' does not support local cache export; building without explicit cache flags"
      docker buildx build \
        --load \
        -f "${dockerfile}" \
        -t "${tag}" \
        "$@" \
        "${ROOT_DIR}"
    fi
  else
    docker build \
      -f "${dockerfile}" \
      -t "${tag}" \
      "$@" \
      "${ROOT_DIR}"
  fi
}

build_image \
  "${ROOT_DIR}/deploy/containers/kani/Dockerfile" \
  "${KANI_TAG}" \
  --build-arg RUST_BASE_IMAGE="${RUST_BASE}" \
  --build-arg KANI_VERSION="${KANI_VERSION}" \
  --build-arg KANI_RUSTC_TOOLCHAIN="${KANI_TOOLCHAIN}"

build_image \
  "${ROOT_DIR}/deploy/containers/z3/Dockerfile" \
  "${Z3_TAG}" \
  --build-arg UBUNTU_BASE_IMAGE="${UBUNTU_BASE}" \
  --build-arg Z3_VERSION="${Z3_VERSION}"

build_image \
  "${ROOT_DIR}/deploy/containers/madsim/Dockerfile" \
  "${MADSIM_TAG}" \
  --build-arg RUST_BASE_IMAGE="${RUST_BASE}" \
  --build-arg MADSIM_VERSION="${MADSIM_VERSION}"

build_image \
  "${ROOT_DIR}/deploy/containers/miri/Dockerfile" \
  "${MIRI_TAG}" \
  --build-arg RUST_BASE_IMAGE="${RUST_BASE}" \
  --build-arg MIRI_RUSTC_TOOLCHAIN="${MIRI_TOOLCHAIN}"

build_image \
  "${ROOT_DIR}/deploy/containers/fuzz/Dockerfile" \
  "${FUZZ_TAG}" \
  --build-arg RUST_BASE_IMAGE="${RUST_BASE}" \
  --build-arg CARGO_FUZZ_VERSION="${CARGO_FUZZ_VERSION}" \
  --build-arg CARGO_AUDIT_VERSION="${CARGO_AUDIT_VERSION}"

# version checks
docker run --rm "${KANI_TAG}" /usr/local/cargo/bin/cargo install --list | grep -F "kani-verifier v${KANI_VERSION}" >/dev/null
docker run --rm "${Z3_TAG}" z3 -version | grep -F "${Z3_VERSION}" >/dev/null
docker run --rm "${MADSIM_TAG}" /usr/local/bin/madsim --version | grep -F "${MADSIM_VERSION}" >/dev/null
docker run --rm "${MIRI_TAG}" /usr/local/cargo/bin/rustup toolchain list | grep -F "${MIRI_TOOLCHAIN}" >/dev/null
docker run --rm "${FUZZ_TAG}" /usr/local/cargo/bin/cargo fuzz --version | grep -F "${CARGO_FUZZ_VERSION}" >/dev/null
docker run --rm "${FUZZ_TAG}" /usr/local/cargo/bin/cargo audit --version | grep -F "${CARGO_AUDIT_VERSION}" >/dev/null

KANI_DIGEST="$(docker inspect --format '{{.Id}}' "${KANI_TAG}")"
Z3_DIGEST="$(docker inspect --format '{{.Id}}' "${Z3_TAG}")"
MADSIM_DIGEST="$(docker inspect --format '{{.Id}}' "${MADSIM_TAG}")"
MIRI_DIGEST="$(docker inspect --format '{{.Id}}' "${MIRI_TAG}")"
FUZZ_DIGEST="$(docker inspect --format '{{.Id}}' "${FUZZ_TAG}")"

cat > "${DIGESTS_FILE}" <<EOF
{
  "kani": "${KANI_DIGEST}",
  "z3": "${Z3_DIGEST}",
  "madsim": "${MADSIM_DIGEST}",
  "miri": "${MIRI_DIGEST}",
  "fuzz": "${FUZZ_DIGEST}"
}
EOF

echo "Wrote digests to ${DIGESTS_FILE}"
