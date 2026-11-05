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
# Per-request acquire timeouts only catch stalled connections, not a mirror
# that keeps trickling bytes: a 63 MB download once took 29 minutes that way
# and still exited 0. The network phase (index refresh and package download)
# therefore also gets a wall-clock budget, so a slow mirror costs one retry
# instead of the whole job. The dpkg install that follows runs unbounded
# because it is local and interrupting it can leave dpkg half-configured.
#
# Usage: bash scripts/install-system-deps.sh <package> [package...]
set -euo pipefail

readonly MAX_ATTEMPTS=3
# Overridable so the stub-driven test does not have to wait out real delays.
readonly RETRY_DELAY_SECS="${INSTALL_DEPS_RETRY_DELAY_SECS:-10}"
# Wall-clock budget for each bounded network call. An untouched run finishes
# in seconds; this bounds a mirror that dribbles bytes.
readonly NETWORK_TIMEOUT_SECS="${INSTALL_DEPS_NETWORK_TIMEOUT_SECS:-180}"
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

# Runs a network-only apt call under a wall-clock budget. `--kill-after`
# stops a wedged child; a timeout exits 124, which the caller counts as a
# failed attempt.
apt_run_bounded() {
  timeout --kill-after=15 "${NETWORK_TIMEOUT_SECS}" \
    sudo env DEBIAN_FRONTEND=noninteractive apt-get "$@" "${APT_OPTS[@]}"
}

for ((attempt = 1; attempt <= MAX_ATTEMPTS; attempt++)); do
  if apt_run_bounded update \
    && apt_run_bounded install -y --no-install-recommends --download-only "$@" \
    && apt_run install -y --no-install-recommends "$@"; then
    exit 0
  fi
  if ((attempt == MAX_ATTEMPTS)); then
    break
  fi
  delay=$((attempt * RETRY_DELAY_SECS))
  echo "apt attempt ${attempt}/${MAX_ATTEMPTS} failed (network budget ${NETWORK_TIMEOUT_SECS}s); retrying in ${delay}s" >&2
  sleep "$delay"
done

echo "system dependency install failed after ${MAX_ATTEMPTS} attempts: $*" >&2
exit 1
