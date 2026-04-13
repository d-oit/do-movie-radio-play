# Feature Plan: Profile-Driven Tag Calibration

## Why This Next

This is the strongest next feature because it reuses the current deterministic,
offline pipeline instead of adding a new runtime class of dependency.

- `CalibrationProfile` already exists in `src/learning/profiles.rs`
- Correction data already captures tag edits in `src/learning/corrections.rs`
- Tagging is already rule-based in `src/pipeline/tags.rs`
- Prompt quality already depends on tag quality in `src/pipeline/prompts.rs`

## Problem

Profiles currently tune only `energy_threshold_delta`. Tag classification remains
globally hardcoded, so action, documentary, and drama profiles cannot express
different non-voice tag sensitivity.

## Proposed Scope

### Data Model
- Extend `CalibrationProfile` with bounded per-tag rule deltas
- Version the schema explicitly to keep profile loading stable

### Calibration
- Summarize `original_tags` to `corrected_tags` drift from correction records
- Emit recommended tag-rule adjustments alongside energy threshold updates
- Keep recommendations bounded and deterministic

### Runtime Application
- Replace hardcoded thresholds in `src/pipeline/tags.rs` with a small profile-aware
  rule struct
- Keep default behavior identical when no tag calibration data is present

### Validation
- Add unit tests for threshold boundaries and profile deltas
- Add integration tests showing the same media produces different tags under
  different profiles for justified cases
- Verify prompt changes only when tag changes justify them

## Risks

- Too many knobs can make profiles hard to reason about
- Tag calibration can overfit synthetic or narrow fixture sets
- Prompt output can drift if tag rules change too aggressively

## Guardrails

- Prefer additive deltas over fully custom per-profile rule sets
- Keep the number of exposed tag-tuning parameters small
- Cap learned adjustments so corrections cannot destabilize the rules

## Initial Implementation Slice

1. Add profile fields for two or three high-value tag thresholds.
2. Refactor `map_tags()` to consume a small `TagRules` struct.
3. Thread loaded profile rules into the `tag` command.
4. Add deterministic tests covering profile-sensitive tag outcomes.
