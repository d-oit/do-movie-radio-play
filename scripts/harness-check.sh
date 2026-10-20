#!/usr/bin/env bash
# harness-check.sh — run harness sensors with agent-optimised error output and prerequisite checks
# Usage: ./scripts/harness-check.sh <sensor|signal_set>
# Signal sets: feedback | verification | release | benchmarks | audio_integration | all
# Individual sensors: fmt | check | clippy | test | deny | audit | msrv | skills | agents | workflows | shellcheck | secret_scan | do_harness
set -euo pipefail

SENSOR=${1:-"feedback"}

PASSED="\033[0;32m✅ HARNESS OK\033[0m"
FAILED="\033[0;31m❌ HARNESS VIOLATION\033[0m"
SKIPPED="\033[1;33m⚠️ HARNESS SKIPPED\033[0m"

run_with_prereq() {
    local name="$1" prereq="$2" cmd="$3" hint="$4"
    echo "▶ Running sensor: $name"
    if [[ -n "$prereq" ]]; then
        for p in $prereq; do
            if ! command -v "$p" &>/dev/null; then
                echo -e "$SKIPPED [$name]"
                echo "  PREREQUISITE MISSING: '$p' is not installed."
                echo "  NOTE: Harness skipped this check; full verification requires '$p'."
                echo "  FIX HINT: $hint"
                return 0
            fi
        done
    fi

    if eval "$cmd"; then
        echo -e "$PASSED [$name]"
        return 0
    else
        echo -e "$FAILED [$name]"
        echo ""
        echo "  AGENT FIX HINT: $hint"
        echo "  See HARNESS.md for the full sensor ↔ guide map."
        return 1
    fi
}

secret_scan_check() {
    local pattern="(api_key|token|secret|password|auth|key)[[:space:]]*[:=][[:space:]]*['\"][a-zA-Z0-9_\-]{16,}['\"]"
    local exclude_secret='example\.com|example\.org|test\.com|GITHUB_TOKEN|CARGO_REGISTRY_TOKEN|worktree'
    if grep -rE "$pattern" --exclude-dir=.git --exclude-dir=target --exclude-dir=.agents --exclude-dir=.opencode crates/ config/ 2>/dev/null | grep -vE "$exclude_secret"; then
        return 1
    fi
    return 0
}

export -f secret_scan_check 2>/dev/null || true

case "$SENSOR" in
  fmt)
    run_with_prereq "cargo fmt" "" \
        "cargo fmt --all -- --check" \
        "Run: cargo fmt --all"
    ;;
  check)
    run_with_prereq "cargo check" "" \
        "cargo check --workspace --all-targets" \
        "Fix compilation errors across workspace targets."
    ;;
  clippy)
    run_with_prereq "cargo clippy" "" \
        "cargo clippy --workspace --all-targets --all-features -- -D warnings" \
        "Fix all warnings. Check .clippy.toml for allowed exceptions."
    ;;
  test)
    if command -v cargo-nextest &>/dev/null; then
      run_with_prereq "cargo nextest" "cargo-nextest" \
          "cargo nextest run --workspace" \
          "Fix the failing test."
    else
      run_with_prereq "cargo test" "" \
          "cargo test --workspace" \
          "Fix the failing test."
    fi
    ;;
  deny)
    run_with_prereq "cargo deny" "cargo-deny" \
        "cargo deny check" \
        "Install cargo-deny or check deny.toml for violations."
    ;;
  audit)
    run_with_prereq "cargo audit" "cargo-audit" \
        "cargo audit" \
        "Install cargo-audit or resolve dependency advisories."
    ;;
  msrv)
    run_with_prereq "MSRV audit" "" \
        "bash scripts/audit-msrv.sh" \
        "Ensure rust-version compliance across all Cargo.toml manifests."
    ;;
  skills)
    run_with_prereq "validate-skills" "" \
        "bash scripts/validate-skills.sh" \
        "Fix skill frontmatter or symlinks via scripts/setup-skills.sh."
    ;;
  agents)
    run_with_prereq "validate-agent-entrypoints" "" \
        "bash scripts/validate-agent-entrypoints.sh" \
        "Fix entrypoints in .agents/ or script references."
    ;;
  workflows)
    run_with_prereq "validate-workflows" "" \
        "bash scripts/validate-workflows.sh" \
        "Fix workflow YAML formatting or mandatory keys in .github/workflows/."
    ;;
  shellcheck)
    run_with_prereq "shellcheck" "shellcheck" \
        "shellcheck scripts/*.sh" \
        "Fix shell script warnings or install shellcheck."
    ;;
  secret_scan)
    run_with_prereq "secret scan" "" \
        "bash -c '$(declare -f secret_scan_check); secret_scan_check'" \
        "Remove detected secrets/credentials from repository source files."
    ;;
  do_harness)
    run_with_prereq "do-harness selection test" "python3" \
        "python3 scripts/test-do-harness.py" \
        "Ensure do-harness.toml correctly maps changed paths to sensors."
    ;;
  feedback)
    $0 fmt
    $0 check
    ;;
  verification)
    $0 clippy
    $0 test
    $0 msrv
    $0 skills
    $0 agents
    $0 workflows
    $0 shellcheck
    $0 do_harness
    ;;
  release)
    $0 verification
    run_with_prereq "version check" "" "bash scripts/check_version.sh" "Fix version mismatch between VERSION and Cargo.toml."
    $0 deny
    $0 audit
    $0 secret_scan
    ;;
  benchmarks)
    run_with_prereq "render benchmarks" "" \
        "cargo bench -p benchmarks --bench render_bench -- --quick" \
        "Fix render benchmark regressions."
    run_with_prereq "pipeline benchmarks" "" \
        "cargo bench -p benchmarks --bench pipeline_bench -- --noplot" \
        "Fix pipeline benchmark regressions."
    ;;
  audio_integration)
    run_with_prereq "test local tts" "" \
        "cargo test -p movie-radio-voice --features local-tts" \
        "Fix local TTS feature failures."
    run_with_prereq "test sinc resampling" "" \
        "cargo test -p movie-radio-pipeline --features movie-radio-pipeline/high-quality-resample" \
        "Fix high-quality sinc resample failures."
    run_with_prereq "audio fixtures validation" "python3" \
        "python3 scripts/check_validation_coverage.py --tier A --strict-files" \
        "Ensure tier A audio validation fixtures exist."
    ;;
  all)
    $0 feedback
    $0 verification
    ;;
  *)
    echo "Usage: $0 <feedback|verification|release|benchmarks|audio_integration|all|sensor_name>"
    exit 1
    ;;
esac
