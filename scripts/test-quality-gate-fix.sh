#!/usr/bin/env bash
# scripts/test-quality-gate-fix.sh
# Hermetic regression test for quality_gate.sh exit code and output handling.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

MOCK_BIN="$TMP_DIR/bin"
mkdir -p "$MOCK_BIN"

# Create mock cargo executable
cat << 'EOF' > "$MOCK_BIN/cargo"
#!/usr/bin/env bash
set -euo pipefail

cmd="${1:-}"

if [[ "$cmd" == "fmt" ]]; then
  if [[ " $* " == *" -- --check "* ]]; then
    if [[ "${TEST_FMT_CHECK_EXIT:-0}" != "0" ]]; then
      echo "mock error: cargo fmt check failed" >&2
      exit "${TEST_FMT_CHECK_EXIT}"
    fi
    exit 0
  else
    if [[ "${TEST_FMT_FIX_EXIT:-0}" != "0" ]]; then
      echo "mock error: cargo fmt fix failed" >&2
      exit "${TEST_FMT_FIX_EXIT}"
    fi
    exit 0
  fi
elif [[ "$cmd" == "clippy" ]]; then
  if [[ " $* " == *" --fix "* ]]; then
    if [[ "${TEST_CLIPPY_FIX_EXIT:-0}" != "0" ]]; then
      echo "mock error: cargo clippy fix failed" >&2
      exit "${TEST_CLIPPY_FIX_EXIT}"
    fi
    exit 0
  else
    if [[ "${TEST_CLIPPY_CHECK_EXIT:-0}" != "0" ]]; then
      echo "mock error: cargo clippy check failed" >&2
      exit "${TEST_CLIPPY_CHECK_EXIT}"
    fi
    exit 0
  fi
elif [[ "$cmd" == "bench" ]]; then
  echo "Mixer_render_mix_reusing_buffers_3_tracks_5s"
  echo "time:   [16.317 ms 16.473 ms 17.097 ms]"
  echo "Mixer_render_complex_radio_play_scenario_5s"
  echo "time:   [16.317 ms 16.473 ms 17.097 ms]"
  exit 0
else
  exit 0
fi
EOF

chmod +x "$MOCK_BIN/cargo"

# Prepare PATH with MOCK_BIN prepended
export PATH="$MOCK_BIN:$PATH"

assert_failure() {
  local test_name="$1"
  local exit_code="$2"
  local output="$3"
  local expected_fail_pattern="$4"
  local forbidden_pass_pattern="$5"
  local expected_err_detail="$6"

  if [[ "$exit_code" -eq 0 ]]; then
    echo "❌ $test_name: expected non-zero exit code, got 0" >&2
    exit 1
  fi

  if ! grep -q "$expected_fail_pattern" <<< "$output"; then
    echo "❌ $test_name: expected output to contain '$expected_fail_pattern'" >&2
    echo "Actual output:" >&2
    echo "$output" >&2
    exit 1
  fi

  if [[ -n "$forbidden_pass_pattern" ]] && grep -q "$forbidden_pass_pattern" <<< "$output"; then
    echo "❌ $test_name: output incorrectly contained '$forbidden_pass_pattern'" >&2
    echo "Actual output:" >&2
    echo "$output" >&2
    exit 1
  fi

  if [[ -n "$expected_err_detail" ]] && ! grep -q "$expected_err_detail" <<< "$output"; then
    echo "❌ $test_name: expected stderr detail '$expected_err_detail' in output" >&2
    echo "Actual output:" >&2
    echo "$output" >&2
    exit 1
  fi

  echo "✅ $test_name: passed"
}

assert_success() {
  local test_name="$1"
  local exit_code="$2"
  local output="$3"
  local expected_pass_pattern="$4"

  if [[ "$exit_code" -ne 0 ]]; then
    echo "❌ $test_name: expected exit code 0, got $exit_code" >&2
    echo "Actual output:" >&2
    echo "$output" >&2
    exit 1
  fi

  if ! grep -q "$expected_pass_pattern" <<< "$output"; then
    echo "❌ $test_name: expected output to contain '$expected_pass_pattern'" >&2
    echo "Actual output:" >&2
    echo "$output" >&2
    exit 1
  fi

  echo "✅ $test_name: passed"
}

echo "Running quality_gate.sh regression tests..."

# 1. Test failed fmt autofix
set +e
OUT1=$(TEST_FMT_FIX_EXIT=1 TEST_CLIPPY_FIX_EXIT=0 ./scripts/quality_gate.sh --fix 2>&1)
EC1=$?
set -e
assert_failure "1. Failed fmt autofix" "$EC1" "$OUT1" "Format: auto-fix failed" "Format: auto-fixed" "mock error: cargo fmt fix failed"

# 2. Test failed clippy autofix
set +e
OUT2=$(TEST_FMT_FIX_EXIT=0 TEST_CLIPPY_FIX_EXIT=1 ./scripts/quality_gate.sh --fix 2>&1)
EC2=$?
set -e
assert_failure "2. Failed clippy autofix" "$EC2" "$OUT2" "Clippy: auto-fix failed" "Clippy: auto-fixed" "mock error: cargo clippy fix failed"

# 3. Test both failed autofix
set +e
OUT3=$(TEST_FMT_FIX_EXIT=1 TEST_CLIPPY_FIX_EXIT=1 ./scripts/quality_gate.sh --fix 2>&1)
EC3=$?
set -e
assert_failure "3. Failed both fmt & clippy autofix" "$EC3" "$OUT3" "Format: auto-fix failed" "auto-fixed" "mock error: cargo fmt fix failed"

# 4a. Test fmt fix succeeds but post-fix recheck fails
set +e
OUT4A=$(TEST_FMT_FIX_EXIT=0 TEST_FMT_CHECK_EXIT=1 ./scripts/quality_gate.sh --fix 2>&1)
EC4A=$?
set -e
assert_failure "4a. Successful fmt fix with failed post-fix recheck" "$EC4A" "$OUT4A" "Format: post-fix check failed" "Format: auto-fixed" "mock error: cargo fmt check failed"

# 4b. Test clippy fix succeeds but post-fix recheck fails
set +e
OUT4B=$(TEST_CLIPPY_FIX_EXIT=0 TEST_CLIPPY_CHECK_EXIT=1 ./scripts/quality_gate.sh --fix 2>&1)
EC4B=$?
set -e
assert_failure "4b. Successful clippy fix with failed post-fix recheck" "$EC4B" "$OUT4B" "Clippy: post-fix check failed" "Clippy: auto-fixed" "mock error: cargo clippy check failed"

# 5. Test successful fix + successful recheck
set +e
OUT5=$(TEST_FMT_FIX_EXIT=0 TEST_FMT_CHECK_EXIT=0 TEST_CLIPPY_FIX_EXIT=0 TEST_CLIPPY_CHECK_EXIT=0 ./scripts/quality_gate.sh --fix 2>&1)
EC5=$?
set -e
assert_success "5. Successful fix and recheck" "$EC5" "$OUT5" "All Quality Gates PASSED"

# 6. Test non-fix mode intact
set +e
OUT6A=$(TEST_FMT_CHECK_EXIT=1 ./scripts/quality_gate.sh 2>&1)
EC6A=$?
set -e
assert_failure "6a. Non-fix mode fmt check failure" "$EC6A" "$OUT6A" "Format: run 'cargo fmt --all' to fix" "" "mock error: cargo fmt check failed"

set +e
OUT6B=$(TEST_FMT_CHECK_EXIT=0 TEST_CLIPPY_CHECK_EXIT=0 ./scripts/quality_gate.sh 2>&1)
EC6B=$?
set -e
assert_success "6b. Non-fix mode success" "$EC6B" "$OUT6B" "All Quality Gates PASSED"

echo "All quality_gate.sh regression tests passed successfully!"
