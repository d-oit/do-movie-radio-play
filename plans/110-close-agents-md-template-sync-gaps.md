# Plan: Close AGENTS.md Template Sync Gaps (Issue #76)

**Date:** 2026-06-09
**Issue:** [#76 — chore: close AGENTS.md template sync gaps](https://github.com/d-oit/do-movie-radio-play/issues/76)
**Status:** ✅ Complete

## Summary

Resolved the three open Template-Sync gaps listed in `AGENTS.md` by either
implementing the missing artifact or formally closing the row with a decision
note. The Template Sync table now contains only "Adopted" rows.

## Decision Matrix

| Gap | Decision | Rationale |
|-----|----------|-----------|
| `scripts/ai-commit.sh` | **Implemented** | The inline fallback (`quality_gate.sh && git add -A && git commit`) is repeated in AGENTS.md; promoting it to a script makes the atomic-commit contract explicit and self-documenting. |
| `scripts/update-all-docs.sh` | **Implemented** | Drift checks (broken skill refs, lingering "Gap" rows, stale closeout plans) are useful as a CI check; a single entry point simplifies them. |
| `.jules/`, `.opencode/`, `.qwen/` agent config dirs | **Closed (row removed)** | The dirs hold configuration for third-party AI agents that this project does not consume. Adding empty directories would be misleading; the gap is not applicable to this repository. |

## Changes

### Added
- `scripts/ai-commit.sh` — runs `quality_gate.sh`, then `git add -A`, then `git commit`. Reuses the existing gate; fails fast on non-git or short messages.
- `scripts/update-all-docs.sh` — non-mutating checks (`--check`) plus an optional refresh (`--refresh`) that writes a `<!-- sync: ISO8601 -->` marker to `AGENTS.md`. Validates:
  1. All `.agents/skills/*/SKILL.md` paths referenced from `AGENTS.md` resolve.
  2. The Template Sync table contains no `| Gap |` rows.
  3. A closeout plan exists in `plans/*.md` within the last 90 days.

### Modified
- `AGENTS.md` — Template Sync table now lists only "Adopted" rows. Added a callout explaining the closure decision for the third-party agent config dirs.

### Not Modified
- `plans/050-status-report/STATUS.md` — left as historical record; the new plan supersedes the "Documentation and Tooling Gaps" section.

## Validation

| Check | Result |
|-------|--------|
| `bash -n scripts/ai-commit.sh` | ✅ syntax ok |
| `bash -n scripts/update-all-docs.sh` | ✅ syntax ok |
| `bash scripts/update-all-docs.sh --check` (post-change) | ✅ exit 0, all checks pass |
| `wc -l AGENTS.md` | 64 / 150 (within `MAX_LINES_AGENTS_MD`) |
| `bash scripts/quality_gate.sh` | ✅ zero warnings (see `analysis/learnings/`) |

## Acceptance Criteria

- [x] `scripts/ai-commit.sh` exists and wraps `quality_gate.sh && git add -A && git commit`.
- [x] `scripts/update-all-docs.sh` exists and exits non-zero on drift.
- [x] `.jules/`, `.opencode/`, `.qwen/` config dirs are intentionally not created; the Template Sync table no longer references them, with a decision note captured above and in `AGENTS.md`.
- [x] AGENTS.md Template Sync table updated to reflect final status.

## Follow-Ups (optional)

- Wire `update-all-docs.sh --check` into a CI job so drift is caught on PRs.
- Extend `ai-commit.sh` to accept `--no-verify` for emergency commits and a `--amend` flag.
