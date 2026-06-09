# Milestone C: ONNX Verifier — Deferral Decision

**Status:** Deferred

**Date:** 2026-06-08

## Context

The Radio-Play 95% Readiness Roadmap (`plans/100-radio-play-95/ROADMAP.md`) defines three milestones:

| Milestone | Scope | Status |
|-----------|-------|--------|
| A | Entropy fix + graph-inspired verifier integration | ✅ Complete |
| B | Holdout scoring scripts, CI gate, failure breakdown, readiness reports, bounded merge behavior, tri-state smoothing | ✅ Complete |
| C | Optional ONNX verifier stage for ambiguous segments | ⏸️ **Deferred** |

Milestone C proposes adding an ONNX-based verification model to discriminate speech from music/non-voice in ambiguous windows, with calibration against verified holdout labels.

## Decision

**Milestone C is deferred indefinitely.** The ONNX verifier will not be implemented at this time.

## Reasoning

1. **Modern precision ceiling identified.** The bounded ceiling check (`analysis/optimization/modern-ceiling-check.json`) established that the best guarded modern precision is ~0.7368 — no profile-only tuning candidate reached ≥0.95 P/R/O.

2. **Engine-level improvement is the next step.** As documented in ROADMAP.md line 103: *"pause profile-only micro-tuning and prioritize engine-level speech/non-speech discrimination improvement for modern precision recovery."*

3. **ONNX is not the bottleneck.** Before introducing an ML-dependent ONNX verification stage, the project needs to exhaust engine-level improvements (adaptive thresholds, state models, richer temporal evidence). Adding ONNX complexity before simpler improvements are explored would introduce unnecessary maintenance burden.

4. **Dependency cost.** An ONNX runtime dependency (ort, tract) would add significant compile time, binary size, and CI complexity. The current project philosophy favors CPU-first deterministic Rust workflows.

## Consequences

### Positive
- Keeps the project free of ML runtime dependencies
- Focuses effort on engine-level improvements that benefit all VAD profiles
- Avoids premature optimization of a secondary verification path

### Negative
- Ambiguous segments (speech-over-music, high-noise) will continue to have lower confidence
- The 95% readiness target may not be achievable without the ONNX stage — current modern precision (~0.7368) is below the 0.95 gate

### Neutral
- This decision can be revisited when engine-level improvements reach diminishing returns
- If a future contributor needs ONNX verification, the pipeline's trait-based architecture supports adding it without structural changes
