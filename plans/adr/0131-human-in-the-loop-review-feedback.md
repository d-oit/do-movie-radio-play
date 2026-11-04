# ADR-131: Human-in-the-Loop Review Feedback

**Status**: Accepted
**Date**: 2026-11-04
**Issues**: #415

## Context

The learning database already stores verification results, corrections, gap
decisions, execution traces and cross-movie pattern similarity. It had no
auditable path from a human review of the final radio-play narration back to
future gap and narration decisions, and no guardrail preventing review data
from leaking into evaluation holdouts or silently changing production
thresholds.

## Decision

- A new `review_feedback` module in `movie-radio-learning` owns the contract.
  A `ReviewDecision` links a stable decision ID to the source media
  fingerprint, run ID, gap interval, acoustic/visual evidence, generated
  narration, TTS output, final assembly position and model/provider versions.
- The decision ID is a deterministic FNV-1a digest over source fingerprint,
  run ID and gap interval. A caller-supplied ID must match it, so a decision
  cannot be re-pointed at another run or artifact.
- Reviewer labels are explicit: `accept`, `reject`, `edit_narration`,
  `dialogue_overlap`, `missing_description`, `incorrect_sound_inference`,
  `wrong_timing` and `unacceptable_voice`.
- Reviewer changes are append-only `correction_events` carrying timestamp,
  reviewer, source and model/provider versions. Database triggers reject
  `UPDATE` and `DELETE`, and the original narration is never overwritten.
- `holdout_fixtures` registers evaluation fixtures by ID, path and checksum.
  Import and proposal application fail closed when a decision or its evidence
  references a registered holdout.
- Proposals are derived from correction events and carry their evidence,
  expected effect and `origin = learned`. Seeded parameters are
  `origin = manual`, so reports distinguish learned changes from
  configuration.
- Applying a proposal requires an explicit `--confirm`, rejects stale
  proposals, and records the prior value and origin in `learning_versions`.
  Rollback restores that version and records the rollback event.
- Outcome metrics are grouped by film, language, genre, VAD engine and cue
  source.
- Privacy is default-deny: source media, voice samples and review artifacts
  are never uploaded automatically, and exports redact artifact paths to file
  names unless explicitly allowed.
- CLI workflow: `import-review-feedback`, `review-proposals`,
  `apply-review-proposal`, `rollback-review-proposal`, `review-metrics`,
  `register-holdout` and `export-review-feedback`.

## Consequences

- Every reviewed decision is traceable to its run and artifact, and every
  reviewer edit is auditable without rewriting prior output.
- Holdout fixtures are immutable to the learning path; a fixture registered
  after import also blocks applying proposals derived from it.
- Learned changes are advisory until applied, reversible, and visibly
  distinct from manual configuration.
- Duplicate imports are idempotent, and corrupted batches are rejected before
  any write.

## Acceptance Mapping

| Criterion | Evidence |
| --- | --- |
| Decision traceable to run and artifact | `ReviewDecision` fields + stable ID validation |
| Edits auditable, original preserved | Append-only `correction_events` + immutability triggers |
| Holdouts immutable to import/calibration | `holdout_fixtures` checks in import and apply |
| Explicit apply records prior version | `learning_versions` apply/rollback rows |
| Learned vs manual distinguished | `origin` on parameters and proposals |
| Duplicate, corrupted, rollback, holdout tests | `review_feedback/tests.rs`, `review_feedback_cli.rs` |
