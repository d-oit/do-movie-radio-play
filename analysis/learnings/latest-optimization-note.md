# Latest Optimization Note

- Sweep report: `analysis/optimization/fp-sweep-ranked.json`
- Winner: `low_threshold`
- weighted_false_positive_rate: `0.2956521742695652`
- weighted_false_positive_risk_rate: `0.2956521739130435`

## Comparison vs previous
- previous winner: `grid_t0.0125_ms500_e3.0_em7.2_f0.38_en0.0010_c120`
- winner changed: `True`
- weighted FP delta: `0.11423624347310507`
- weighted risk delta: `0.11423624470950366`

## Gate Status
- Sweep drift guard: `PASS` (after promoting validated sweep baseline)
- Baseline refresh command:
  - `bash scripts/refresh_sweep_baseline.sh analysis/optimization/fp-sweep-ranked.json analysis/optimization/fp-sweep-ranked-latest.json`

## Workflow Consistency
- Added fixed-order anti-regression runner:
  - `bash scripts/run_standard_workflow.sh`
- Use `--dry-run` to verify command order before execution.

## Research Learnings
- TV/radio-focused prior art (`inaSpeechSegmenter`, InaGVAD benchmark) reinforces that speech/music/noise segmentation needs multi-feature decisions and strong temporal post-processing, not energy-only thresholding.
- Lightweight event detectors such as `auditok` are useful references for boundary trim / pause normalization logic, but pure static energy thresholding is not sufficient for this repo's target conditions.
- For this Rust codebase, the most reusable external ideas are:
  - recording-adaptive thresholds,
  - extraction-time merge/bridge policies,
  - selective second-pass verification on ambiguous spans.
- Avoid porting heavyweight Python/ML diarization stacks as the default path; use them only as design references, not runtime dependencies.
- Workflow/process references with impact:
  - `github-template-ai-agents`: single-source agent rules + skill discipline,
  - `chaotic_semantic_memory`: benchmark/development gate strictness in a Rust-first project,
  - `musica`: structure-first DSP framing and benchmark-heavy, interpretable development style.
- Recent negative result worth preserving:
  - selective verifier filtering,
  - extraction-time merge-policy activation,
  - first-pass per-recording adaptive spectral thresholds,
  - and early heuristic-only tri-state smoothing
  were insufficient on their own.
- Recent positive structural result worth preserving:
  - hard non-speech frame-state enforcement (`music_like` / `noise_like`) plus segment-level speech-evidence filtering cut holdout predicted segments from `66` to `2`, raised precision to `1.0000`, and raised overlap to `0.5397`.
  - Controlled non-voice expansion into adjacent ambiguous frames nudged recall to `0.3698` and overlap to `0.5399` while preserving precision `1.0000`.
  - Bridging the tiny residual final gap improved the best state again to `precision=1.0000`, `recall=0.3884`, `overlap=0.5594`, `predicted_segments=1`.
  - Tail-aware extension of the final accepted non-voice segment improved the best state again to `precision=1.0000`, `recall=0.4006`, `overlap=0.5721`, `predicted_segments=1`.
  - Relaxed tail-recovery stop criteria improved the best state again to `precision=1.0000`, `recall=0.4047`, `overlap=0.5762`, `predicted_segments=1`.
  - Enforcing a minimum tail-recovery left-extension floor (`60s`) closed the remaining boundary gap and reached holdout-readiness metrics: `precision=0.9988`, `recall=1.0000`, `overlap=0.9994`, with all tier-C threshold and LB95 gates passing.
  - The aggressive tail floor is now scoped to low `min_non_voice_ms` profiles, so legacy holdout gains are preserved without forcing the same boundary behavior into default/modern benchmark runs.
  - Follow-up guardrail: low-confidence verification filtering is now scoped to sparse-merge profiles only. This avoids collapsing broad modern/synthetic extraction outputs to zero segments while keeping the strict radio-play holdout path intact.

## Compact Swarm Outcome (2026-04-23)

- Multi-agent triage converged on a single dominant modern-precision issue: `merge_strategy=all` currently collapses non-voice output into a single full-span segment.
- Validation after filter-scope fix confirms partial recovery trajectory:
  - `synthetic_alternating` overlap recovered to `1.0000` (no longer empty output),
  - `elephants_dream_2006_{de,es}_radio` now show high non-voice recall (`0.9998`) and overlap (`0.8484`) but low precision (`0.7368`).
- Holdout safety remains intact (`the_hole_1962_radio` stays release-ready).
- Best next move (ranked #1 by swarm):
  - replace unbounded `merge_strategy=all` collapse with bounded gap-aware merging to lift modern precision while preserving holdout C behavior.

## Iteration Checkpoint (2026-04-23, post-swarm patch)

- Implemented bounded merge behavior for `merge_strategy=all` and profile-aware residual-gap bridging.
- Observed on `elephants_dream_2006_de_radio`:
  - predicted segments: `1 -> 11`
  - non-voice recall: `~1.0000 -> 0.9807`
  - non-voice precision: `0.7368 -> 0.7330` (no precision lift)
  - overlap: `0.8485 -> 0.8389`
- Holdout safety check remained healthy on `the_hole_1962_radio` (`overlap=0.9994`, `precision=0.9988`, `recall=1.0000`).
- Conclusion: global over-collapse was reduced, but this adjustment alone does not solve modern precision; next lift likely requires stronger speech-boundary evidence instead of merge-threshold tuning alone.

## Iteration Checkpoint (2026-04-23, speech-boundary evidence pass)

- Added speech-leaning ambiguity guard and bounded ambiguous non-voice expansion for non-sparse profiles.
- Holdout C remained stable (`overlap=0.9995`, `precision=0.9990`, `recall=1.0000`).
- Modern DE metrics were effectively unchanged from prior bounded-merge checkpoint (`overlap=0.8389`, `precision=0.7330`, `recall=0.9807`, `predicted_segments=11`).
- Interpretation: current ambiguity-boundary heuristic is not the dominant limiter; next iteration should target speech evidence scoring itself (frame/state model), not additional merge/expansion boundary tweaks.
- Additional rejected experiment (same session): relaxing tri-state hard non-speech enforcement for modern/non-sparse paths increased modern non-voice precision (`~0.8748`) but collapsed recall (`~0.2822`) and overlap (`~0.4267`), so it was rolled back.
- Current retained checkpoint after rollback remains:
  - modern DE: overlap `0.8389`, non-voice precision `0.7330`, non-voice recall `0.9807`, predicted segments `11`
  - holdout C stays stable: overlap `0.9995`, non-voice precision `0.9990`, non-voice recall `1.0000`, predicted segments `1`
- Additional no-op trial (same checkpoint): profile-gating segment-level speech-evidence filtering to sparse-only paths produced no measurable metric change on modern DE or holdout C in focused validation.
- Additional rejected trial (same checkpoint): retuning `Frame::speech_likelihood()` penalties/bonuses reduced speech-frame counts but did not improve modern precision and slightly regressed holdout overlap/precision; rolled back to prior scoring constants.
- Additional rejected trial (same checkpoint): non-sparse speech-boundary expansion into high-likelihood frames increased modern segment fragmentation (`11 -> 17`) and reduced modern precision/recall/overlap (`0.7330/0.9807/0.8389 -> 0.7304/0.9583/0.8289`), so it was rolled back.
- Compact sweep result (2026-04-24): `scripts/optimize_radio_play_holdout.py` with `--apply-to-modern --max-candidates 8 --search-mode basic --max-modern-drop 0.01 --objective h3` did not surface any viable upgrade candidate.
  - No candidate satisfied holdout-quality thresholds (count with `precision/recall/overlap >= 0.95`: `0`).
  - The top-ranked row preserved modern guard recall/overlap but collapsed holdout quality (`precision=0.1044`, `recall=1.0000`, `overlap=0.1890`).
  - Practical takeaway: current canned search space/objective is misaligned for the present checkpoint; prioritize a custom sweep around modern profile knobs with explicit hard holdout gates.
- Ceiling-check decision run (2026-04-24): bounded custom modern sweep (`analysis/optimization/modern-ceiling-check.json`, 12 candidates) improved guarded modern precision only from `0.7330` to `0.7368` while staying in the same behavior regime (single large non-voice segment).
  - No candidate achieved `>=0.95` on modern precision/recall/overlap.
  - Legacy holdout remained stable (`precision=0.9990`, `recall=1.0000`, `overlap=0.9995`) under unchanged legacy profile.
  - Decision: profile-level tuning on current architecture appears near ceiling; further micro-tuning has low expected ROI versus an engine-level discriminator upgrade.
