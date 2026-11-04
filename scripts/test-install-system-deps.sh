#!/usr/bin/env bash
# Exercises scripts/install-system-deps.sh against a stub apt-get so the apt
# flags, package passing and retry/abort behavior are covered without touching
# the machine's packages.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly installer="${script_dir}/install-system-deps.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

mkdir -p "${tmp}/bin"
cat >"${tmp}/bin/sudo" <<'STUB'
#!/usr/bin/env bash
env_args=()
while [[ $# -gt 0 && "$1" == *=* ]]; do
  env_args+=("$1")
  shift
done
exec env "${env_args[@]}" "$@"
STUB

cat >"${tmp}/bin/apt-get" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"${APT_LOG}"
if [[ -n "${APT_FAIL:-}" ]]; then
  exit 100
fi
exit 0
STUB
chmod +x "${tmp}/bin/sudo" "${tmp}/bin/apt-get"

export PATH="${tmp}/bin:${PATH}"
export INSTALL_DEPS_RETRY_DELAY_SECS=0

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# 1. Missing packages is a usage error, not a silent no-op.
set +e
bash "${installer}" >/dev/null 2>&1
status=$?
set -e
[[ ${status} -eq 2 ]] || fail "expected exit 2 without arguments, got ${status}"

# 2. A successful install passes both apt calls the timeout options and uses
#    --no-install-recommends, which is what keeps the PocketSphinx recommend
#    chain out of the download.
export APT_LOG="${tmp}/success.log"
: >"${APT_LOG}"
bash "${installer}" ffmpeg libasound2-dev

grep -q '^update ' "${APT_LOG}" || fail "apt-get update was not called"
grep -q '^install -y --no-install-recommends ' "${APT_LOG}" \
  || fail "install did not request --no-install-recommends"
grep -q 'Acquire::http::Timeout=15' "${APT_LOG}" || fail "missing http timeout"
grep -q 'ffmpeg libasound2-dev' "${APT_LOG}" || fail "packages were not forwarded"
[[ "$(grep -c '^install ' "${APT_LOG}")" -eq 1 ]] \
  || fail "successful install should not retry"

# 3. A failing apt is retried MAX_ATTEMPTS times and then aborts, so the job
#    fails fast with a clear message instead of burning its whole budget.
export APT_LOG="${tmp}/failure.log"
: >"${APT_LOG}"
export APT_FAIL=1
set +e
output="$(bash "${installer}" ffmpeg 2>&1)"
status=$?
set -e
unset APT_FAIL

[[ ${status} -eq 1 ]] || fail "expected exit 1 when apt keeps failing, got ${status}"
[[ "$(grep -c '^update ' "${APT_LOG}")" -eq 3 ]] \
  || fail "expected 3 update attempts, got $(grep -c '^update ' "${APT_LOG}")"
[[ "${output}" == *"failed after 3 attempts"* ]] \
  || fail "abort message missing from output"

echo "install-system-deps: all checks passed"
