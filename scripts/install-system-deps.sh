#!/usr/bin/env bash
# Installs the system packages CI jobs need.
#
# A stalled mirror once spent an entire 30-minute job budget inside
# `apt-get install` before the job was cancelled, so every call here carries
# an acquire timeout and the whole install is retried a bounded number of
# times. `--no-install-recommends` also drops packages CI never uses, such as
# the 27 MB PocketSphinx model that libpocketsphinx3 recommends; the
# libpocketsphinx3 library itself is a hard dependency of libavfilter9 and
# stays either way.
#
# Usage: bash scripts/install-system-deps.sh <package> [package...]
set -euo pipefail

readonly MAX_ATTEMPTS=3
# Overridable so the stub-driven test does not have to wait out real delays.
readonly RETRY_DELAY_SECS="${INSTALL_DEPS_RETRY_DELAY_SECS:-10}"
readonly APT_OPTS=(
  -o Acquire::http::Timeout=15
  -o Acquire::https::Timeout=15
  -o Acquire::Retries=2
  -o Acquire::ForceIPv4=true
)

if [[ $# -eq 0 ]]; then
  echo "usage: $0 <package> [package...]" >&2
  exit 2
fi

apt_run() {
  sudo env DEBIAN_FRONTEND=noninteractive apt-get "$@" "${APT_OPTS[@]}"
}

for ((attempt = 1; attempt <= MAX_ATTEMPTS; attempt++)); do
  if apt_run update && apt_run install -y --no-install-recommends "$@"; then
    exit 0
  fi
  if ((attempt == MAX_ATTEMPTS)); then
    break
  fi
  delay=$((attempt * RETRY_DELAY_SECS))
  echo "apt attempt ${attempt}/${MAX_ATTEMPTS} failed; retrying in ${delay}s" >&2
  sleep "$delay"
done

echo "system dependency install failed after ${MAX_ATTEMPTS} attempts: $*" >&2
exit 1
