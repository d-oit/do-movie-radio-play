#!/usr/bin/env bash
# Synchronize cross-references and timestamps in repository documentation.
#
# Performs non-mutating checks first, then optionally refreshes a sync
# timestamp in tracked docs. Exits non-zero if any check fails.
#
# Usage:
#   bash scripts/update-all-docs.sh           # check + refresh
#   bash scripts/update-all-docs.sh --check   # check only (no writes)
#   bash scripts/update-all-docs.sh --refresh # write timestamps only

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mode="all"
for arg in "$@"; do
  case "$arg" in
    --check)   mode="check" ;;
    --refresh) mode="refresh" ;;
    -h|--help)
      cat <<'EOF'
Usage: bash scripts/update-all-docs.sh [--check|--refresh]

  --check    Run drift checks only; do not modify any files.
  --refresh  Refresh sync timestamps only; do not run drift checks.
  (default)  Run drift checks, then refresh sync timestamps.
EOF
      exit 0
      ;;
    *)
      echo "unknown argument: $arg" >&2
      exit 1
      ;;
  esac
done

agents_md="AGENTS.md"
failures=0

# 1) Verify all skill files referenced in AGENTS.md exist on disk.
check_skill_refs() {
  echo "==> checking skill references in $agents_md"
  if [[ ! -f "$agents_md" ]]; then
    echo "missing $agents_md" >&2
    return 1
  fi
  local missing=0
  # Extract markdown links of the form [.agents/skills/<name>/SKILL.md](...)
  while IFS= read -r ref; do
    [[ -z "$ref" ]] && continue
    if [[ ! -f "$ref" ]]; then
      echo "  - broken reference: $ref" >&2
      missing=$((missing + 1))
    fi
  done < <(grep -oE '\.agents/skills/[A-Za-z0-9_-]+/SKILL\.md' "$agents_md" | sort -u)
  if [[ "$missing" -gt 0 ]]; then
    echo "  $missing broken skill reference(s) in $agents_md" >&2
    return 1
  fi
  echo "  ok"
  return 0
}

# 2) Verify the Template Sync table has no rows still marked "Gap".
check_template_sync_table() {
  echo "==> checking Template Sync table for unresolved gaps"
  if ! grep -q '^## Template Sync' "$agents_md"; then
    echo "  - Template Sync section missing in $agents_md" >&2
    return 1
  fi
  # Look for pipe-delimited "Gap" cells within the table.
  local gap_count
  gap_count=$(awk '/^\|.*\|/{ if ($0 ~ /\| Gap \|/) c++ } END { print c+0 }' "$agents_md")
  if [[ "$gap_count" -gt 0 ]]; then
    echo "  - $gap_count unresolved 'Gap' row(s) remain in Template Sync table" >&2
    return 1
  fi
  echo "  ok"
  return 0
}

# 3) Verify the plans/ directory still has the most recent closeout plan.
check_recent_plan() {
  echo "==> checking plans/ for a recent closeout (within 90 days)"
  if [[ ! -d plans ]]; then
    echo "  - plans/ directory missing" >&2
    return 1
  fi
  local today_epoch
  today_epoch=$(date -u +%s)
  local cutoff_epoch=$((today_epoch - 90 * 24 * 60 * 60))
  local newest=0
  while IFS= read -r f; do
    [[ -z "$f" ]] && continue
    local mtime
    mtime=$(stat -c %Y "$f" 2>/dev/null || echo 0)
    if [[ "$mtime" -gt "$newest" ]]; then
      newest="$mtime"
    fi
  done < <(find plans -maxdepth 1 -type f -name '*.md')
  if [[ "$newest" -lt "$cutoff_epoch" ]]; then
    echo "  - no plan/ closeout within last 90 days" >&2
    return 1
  fi
  echo "  ok (newest plan mtime: $(date -u -d "@$newest" +%Y-%m-%d 2>/dev/null || echo "$newest"))"
  return 0
}

# 4) Refresh a "Last sync" line near the top of AGENTS.md (no-op if absent).
refresh_timestamp() {
  local stamp
  stamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "==> refreshing sync stamp: $stamp"
  if grep -q '^<!-- sync: ' "$agents_md"; then
    # In-place replace the existing sync marker line.
    local tmp
    tmp=$(mktemp)
    awk -v stamp="$stamp" '
      /^<!-- sync: / { print "<!-- sync: " stamp " -->"; next }
      { print }
    ' "$agents_md" > "$tmp"
    mv "$tmp" "$agents_md"
  else
    # Insert the marker as the first line of the file.
    local tmp
    tmp=$(mktemp)
    {
      printf '<!-- sync: %s -->\n' "$stamp"
      cat "$agents_md"
    } > "$tmp"
    mv "$tmp" "$agents_md"
  fi
}

if [[ "$mode" != "refresh" ]]; then
  check_skill_refs        || failures=$((failures + 1))
  check_template_sync_table || failures=$((failures + 1))
  check_recent_plan       || failures=$((failures + 1))
fi

if [[ "$mode" != "check" ]]; then
  refresh_timestamp
fi

if [[ "$failures" -gt 0 ]]; then
  echo "update-all-docs: $failures check(s) failed" >&2
  exit 1
fi

echo "update-all-docs: ok"
