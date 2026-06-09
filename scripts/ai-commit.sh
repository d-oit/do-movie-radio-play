#!/usr/bin/env bash
# Atomic commit wrapper.
#
# Runs the quality gate (fmt, clippy, test), stages all changes, and creates
# a single commit. Replaces the inline fallback referenced from AGENTS.md
# (`quality_gate.sh && git add -A && git commit`).
#
# Usage:
#   bash scripts/ai-commit.sh "commit message"
#   bash scripts/ai-commit.sh                  # opens editor for message
#   bash scripts/ai-commit.sh --amend          # amend previous commit
#   bash scripts/ai-commit.sh --no-verify "m"  # skip quality gate
#   bash scripts/ai-commit.sh --help

set -euo pipefail

run_gate=true
amend=false
message=""

print_help() {
  cat <<'EOF'
Usage: bash scripts/ai-commit.sh [OPTIONS] [COMMIT_MESSAGE]

Runs the quality gate, stages all changes, and creates a single commit.

Options:
  --amend       Amend the previous commit instead of creating a new one.
  --no-verify   Skip the quality gate (emergency use only).
  --help, -h    Show this help text.

If COMMIT_MESSAGE is omitted, the editor is opened for the message.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --amend)
      amend=true
      shift
      ;;
    --no-verify)
      run_gate=false
      shift
      ;;
    -h|--help)
      print_help
      exit 0
      ;;
    --)
      shift
      if [[ $# -gt 0 ]]; then
        message="$*"
      fi
      break
      ;;
    -*)
      echo "unknown flag: $1" >&2
      print_help >&2
      exit 1
      ;;
    *)
      message="$1"
      shift
      ;;
  esac
done

if ! command -v git >/dev/null 2>&1; then
  echo "git is required" >&2
  exit 1
fi

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "not inside a git work tree" >&2
  exit 1
fi

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if [[ "$amend" == true && -n "$message" ]]; then
  echo "--amend cannot be combined with an explicit message" >&2
  exit 1
fi

if [[ "$run_gate" == true ]]; then
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  gate="${script_dir}/quality_gate.sh"
  if [[ ! -x "$gate" ]]; then
    echo "quality gate not found or not executable: $gate" >&2
    exit 1
  fi
  echo "==> running quality gate"
  bash "$gate"
else
  echo "==> --no-verify: skipping quality gate" >&2
fi

git add -A

if [[ "$amend" == true ]]; then
  if [[ -n "$(git status --porcelain)" ]] || ! git diff --cached --quiet; then
    git commit --amend --no-edit
  else
    echo "no staged changes to amend" >&2
    exit 1
  fi
  exit 0
fi

if [[ -z "$message" ]]; then
  git commit
  exit 0
fi

if [[ "${#message}" -lt 3 ]]; then
  echo "commit message too short (min 3 chars)" >&2
  exit 1
fi

git commit -m "$message"
