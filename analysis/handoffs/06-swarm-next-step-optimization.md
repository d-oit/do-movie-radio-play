# Swarm Handoff: Next Optimization Move (2026-04-23)

## Scope

- Objective: identify the best next action after restoring non-empty modern outputs while keeping holdout tier C release-ready.
- Method: parallel swarm triage across metrics, pipeline code paths, and optimization loop strategy.

## Converged Findings

1. Modern precision remains the main blocker; recall is already near-saturated.
2. Primary structural cause: `merge_strategy=all` currently performs global non-voice collapse.
3. Holdout C safety currently depends on profile-scoped controls (`sparse` merge + low `min_non_voice_ms` filter path + tail recovery floor).

## Recommended Next Change

- Implement bounded non-voice merging for `merge_strategy=all` (gap-aware), instead of unbounded full-span collapse.
- Keep holdout-specific protections unchanged during this iteration.

## Validation Gate for Next PR

- Re-run `radio-play-manifest` tiers A/B/C and regenerate readiness/failure artifacts.
- Must keep holdout C gate pass (`precision/recall/overlap/LB95 >= 0.95`).
- Target modern improvement: raise min(DE, ES) non-voice precision above current ~`0.7368` without major recall collapse.

## Ownership / Next Action

- Owner: implementation pass in `src/pipeline/segmenter.rs` merge policy branch.
- Next action: patch merge behavior, run targeted sweep, compare deltas, and update compact learnings.
