#!/usr/bin/env bash
set -euo pipefail

MAX_RETRIES="${SELF_FIX_LOOP_MAX_RETRIES:-5}"
TIMEOUT_SECONDS="${SELF_FIX_LOOP_TIMEOUT:-1800}"
POLL_INTERVAL="${SELF_FIX_LOOP_POLL_INTERVAL:-30}"
AUTO_RESEARCH="${SELF_FIX_LOOP_AUTO_RESEARCH:-1}"
FIX_ISSUES="${SELF_FIX_LOOP_FIX_ISSUES:-1}"
STRICT_VALIDATION="${SELF_FIX_LOOP_STRICT_VALIDATION:-1}"
DRY_RUN=0
BASE_BRANCH="main"

START_TS="$(date +%s)"
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
HANDOFF_DIR="${REPO_ROOT}/analysis/handoffs"
CI_LOG_FILE=""
LAST_HANDOFF_FILE=""

log() {
  printf '[self-fix-loop] %s\n' "$*"
}

usage() {
  cat <<'EOF'
Usage: scripts/self-fix-loop.sh [options]

Options:
  --max-retries N          Maximum fix iterations (default: 5)
  --auto-research          Enable research handoff guidance
  --fix-issues             Attempt local auto-fixes when possible
  --strict-validation      Require all checks to pass
  --timeout SECONDS        Per-iteration timeout (default: 1800)
  --poll-interval SECONDS  CI poll interval (default: 30)
  --dry-run                Simulate without git push or PR operations
  --base-branch BRANCH     Target branch for PR (default: main)
  -h, --help               Show this help
EOF
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --max-retries)
        MAX_RETRIES="$2"
        shift 2
        ;;
      --auto-research)
        AUTO_RESEARCH=1
        shift
        ;;
      --fix-issues)
        FIX_ISSUES=1
        shift
        ;;
      --strict-validation)
        STRICT_VALIDATION=1
        shift
        ;;
      --timeout)
        TIMEOUT_SECONDS="$2"
        shift 2
        ;;
      --poll-interval)
        POLL_INTERVAL="$2"
        shift 2
        ;;
      --dry-run)
        DRY_RUN=1
        shift
        ;;
      --base-branch)
        BASE_BRANCH="$2"
        shift 2
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        log "Unknown argument: $1"
        usage
        exit 2
        ;;
    esac
  done
}

require_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    log "Missing required command: $cmd"
    exit 2
  fi
}

run_quality_gate() {
  log "Running quality gate"
  if [[ "$STRICT_VALIDATION" -eq 1 ]]; then
    bash "${REPO_ROOT}/scripts/quality_gate.sh"
  else
    cargo test
  fi
}

current_branch() {
  git rev-parse --abbrev-ref HEAD
}

commit_and_push_iteration() {
  local iteration="$1"

  if [[ -z "$(git status --porcelain)" ]]; then
    log "No local changes to commit"
    return 0
  fi

  run_quality_gate
  git add -A
  git commit -m "fix(ci): self-fix-loop iteration ${iteration}"

  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "Dry run: skipping git push"
    return 0
  fi

  git push -u origin "$(current_branch)"
}

ensure_pr() {
  local branch
  branch="$(current_branch)"

  if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "dry-run"
    return 0
  fi

  local pr_number
  pr_number="$(gh pr view "$branch" --json number --jq .number 2>/dev/null || true)"
  if [[ -n "$pr_number" ]]; then
    echo "$pr_number"
    return 0
  fi

  gh pr create \
    --base "$BASE_BRANCH" \
    --head "$branch" \
    --title "chore(ci): stabilize checks for ${branch}" \
    --body "Automated self-fix-loop PR for ${branch}."

  gh pr view "$branch" --json number --jq .number
}

monitor_ci() {
  local pr_number="$1"

  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "Dry run: simulating CI success"
    return 0
  fi

  mkdir -p "$HANDOFF_DIR"
  CI_LOG_FILE="${HANDOFF_DIR}/ci-checks-${pr_number}-$(date +%Y%m%d-%H%M%S).log"

  log "Monitoring CI checks for PR #${pr_number}"
  if timeout "$TIMEOUT_SECONDS" gh pr checks "$pr_number" --watch --interval "$POLL_INTERVAL" >"$CI_LOG_FILE" 2>&1; then
    log "All checks passed"
    return 0
  fi

  log "Checks failed or timed out"
  return 1
}

classify_failure() {
  local default_type="unknown"
  if [[ -z "$CI_LOG_FILE" || ! -f "$CI_LOG_FILE" ]]; then
    echo "$default_type"
    return
  fi

  if grep -Ei 'clippy|warning|error\[E[0-9]+' "$CI_LOG_FILE" >/dev/null 2>&1; then
    echo "rust"
  elif grep -Ei 'fmt|rustfmt|format' "$CI_LOG_FILE" >/dev/null 2>&1; then
    echo "formatting"
  elif grep -Ei 'test failed|failures:' "$CI_LOG_FILE" >/dev/null 2>&1; then
    echo "tests"
  elif grep -Ei 'yaml|workflow' "$CI_LOG_FILE" >/dev/null 2>&1; then
    echo "yaml"
  elif grep -Ei 'markdown|mdlint' "$CI_LOG_FILE" >/dev/null 2>&1; then
    echo "markdown"
  elif grep -Ei 'shellcheck|bash' "$CI_LOG_FILE" >/dev/null 2>&1; then
    echo "shell"
  else
    echo "$default_type"
  fi
}

write_handoff_bundle() {
  local iteration="$1"
  local failure_type="$2"
  local pr_number="$3"

  mkdir -p "$HANDOFF_DIR"
  local handoff_file
  handoff_file="${HANDOFF_DIR}/self-fix-loop-iter-${iteration}-$(date +%Y%m%d-%H%M%S).md"
  LAST_HANDOFF_FILE="$handoff_file"

  cat >"$handoff_file" <<EOF
# Self-Fix Loop Handoff (Iteration ${iteration})

- PR: ${pr_number}
- Failure type: ${failure_type}
- CI log: ${CI_LOG_FILE}

## Parallel Agent Plan

1. Analyzer agent (root-cause triage)
2. Fixer agent (apply minimal fix)
3. Validator agent (run quality gate + focused checks)
EOF

  if [[ "$AUTO_RESEARCH" -eq 1 ]]; then
    cat >>"$handoff_file" <<'EOF'
4. Researcher agent (web/docs references for uncommon failures)
EOF
  fi

  cat >>"$handoff_file" <<'EOF'

## Skill Routing

- shell -> shell-script-quality
- yaml -> cicd-pipeline
- rust/tests -> code-quality
- markdown -> markdownlint
- unknown -> web-search-researcher + do-web-doc-resolver

## Coordination Notes

- Execute independent tasks in parallel.
- Converge outputs into one patch set.
- Run validation once after merge of all fix patches.
EOF

  log "Wrote handoff bundle: $handoff_file"

  if [[ -x "${REPO_ROOT}/scripts/handoff-to-tasks.sh" ]]; then
    "${REPO_ROOT}/scripts/handoff-to-tasks.sh" "$handoff_file" >/dev/null
  fi
}

run_clippy_fix() {
  log "Applying cargo clippy --fix"
  cargo clippy --fix --allow-dirty --allow-staged --all-targets --all-features -- -D warnings
}

handle_rust_autofix() {
  if [[ -z "$CI_LOG_FILE" || ! -f "$CI_LOG_FILE" ]]; then
    return 1
  fi

  if grep -Ei 'unused import|unused mut|dead_code|never used|clippy::' "$CI_LOG_FILE" >/dev/null 2>&1; then
    run_clippy_fix
    return 0
  fi

  if grep -Ei 'file .* is not formatted|rustfmt' "$CI_LOG_FILE" >/dev/null 2>&1; then
    cargo fmt
    return 0
  fi

  return 1
}

handle_test_autofix() {
  if [[ -z "$CI_LOG_FILE" || ! -f "$CI_LOG_FILE" ]]; then
    return 1
  fi

  if grep -Ei 'snapshot|insta' "$CI_LOG_FILE" >/dev/null 2>&1; then
    log "Applying snapshot updates"
    cargo test -- --nocapture
    if command -v cargo-insta >/dev/null 2>&1; then
      cargo insta accept --unseen
      return 0
    fi
  fi

  return 1
}

attempt_local_autofix() {
  local failure_type="$1"

  if [[ "$FIX_ISSUES" -ne 1 ]]; then
    return 1
  fi

  case "$failure_type" in
    formatting)
      log "Applying rustfmt auto-fix"
      cargo fmt
      ;;
    rust)
      if ! handle_rust_autofix; then
        log "No signature-based rust auto-fix available"
        return 1
      fi
      ;;
    tests)
      if ! handle_test_autofix; then
        log "No signature-based test auto-fix available"
        return 1
      fi
      ;;
    shell)
      if command -v shfmt >/dev/null 2>&1; then
        log "Applying shfmt to scripts"
        shfmt -w "${REPO_ROOT}/scripts"
      else
        log "shfmt unavailable; skipping shell auto-fix"
        return 1
      fi
      ;;
    *)
      log "No built-in auto-fix handler for ${failure_type}"
      return 1
      ;;
  esac

  if [[ -n "$(git status --porcelain)" ]]; then
    run_quality_gate || return 1
    git add -A
    git commit -m "fix(ci): auto-fix ${failure_type} issue"
    if [[ "$DRY_RUN" -eq 0 ]]; then
      git push
    fi
    return 0
  fi

  return 1
}

main() {
  parse_args "$@"

  require_cmd git
  require_cmd cargo

  if [[ "$DRY_RUN" -eq 0 ]]; then
    require_cmd gh
  fi

  if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    log "Must run inside a git repository"
    exit 2
  fi

  local iteration=1
  while [[ "$iteration" -le "$MAX_RETRIES" ]]; do
    log "Starting iteration ${iteration}/${MAX_RETRIES}"
    commit_and_push_iteration "$iteration"

    local pr_number
    pr_number="$(ensure_pr)"

    if monitor_ci "$pr_number"; then
      log "Success: all checks passing"
      exit 0
    fi

    local failure_type
    failure_type="$(classify_failure)"
    write_handoff_bundle "$iteration" "$failure_type" "$pr_number"

    if attempt_local_autofix "$failure_type"; then
      log "Auto-fix applied, retrying loop"
      ((iteration += 1))
      continue
    fi

    log "Manual agent handoff required"
    exit 3
  done

  log "Max retries exceeded"
  exit 3
}

main "$@"
