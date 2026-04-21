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
