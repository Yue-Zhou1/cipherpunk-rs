#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSIONS_FILE="${ROOT_DIR}/containers/versions.toml"

mapfile -t VERSION_VALUES < <(
  awk -F'=' '
    /^\s*\[/ { section=$0; next }
    section ~ /\[tools\]/ || section ~ /\[images\]/ {
      if ($2 != "") {
        val=$2
        gsub(/^[ \t"]+|[ \t"]+$/, "", val)
        if (val != "") print val
      }
    }
  ' "${VERSIONS_FILE}"
)

for version in "${VERSION_VALUES[@]}"; do
  if grep -R --line-number --fixed-strings "${version}" "${ROOT_DIR}/containers" \
    | grep -v "versions.toml" \
    | grep -v "check_version_dedup.sh" \
    >/dev/null; then
    echo "Found duplicated version string outside versions.toml: ${version}" >&2
    exit 1
  fi
done

echo "Version strings are centralized in containers/versions.toml"
