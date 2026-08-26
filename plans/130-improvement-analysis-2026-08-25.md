# Improvement Analysis — 2026-08-25

Workspace-wide analysis covering correctness bugs, architecture debt, infrastructure gaps,
test coverage, and new-feature candidates.

**Method:** Full sweep of `crates/`, `scripts/`, `.github/workflows/`, and `benchmarks/`
(~15.7k LOC across 10 crates), cross-referenced against `GOAP_STATE.md`, `FOLLOWUPS.md`,
`050-status-report/GAPS.md`, `060-next-features/PHASE-06-new-capabilities.md`, and
`100-radio-play-95/ROADMAP.md` to exclude already-tracked work. Baseline: zero open
issues/PRs, FOLLOWUPS open = none, current GOAP goal complete.

---

## A. Correctness Fixes

### A1 (P0): Slice-index panic in speech evidence filter
- **Location:** `crates/movie-radio-pipeline/src/pipeline/speech_evidence.rs:43-47`
- **Bug:** `end_idx = (...).min(frames.len()).max(start_idx + 1)` — when a segment
  timestamp maps beyond the decoded frame count (`start_idx >= frames.len()`),
  `end_idx > frames.len()` and the subsequent slice panics.
- **Fix:** Mirror the clamp pattern from the twin implementation in
  `pipeline/segmenter/confidence.rs:53-58`: `end_idx.clamp(start_idx + 1, frames.len())`.
- **Test:** Regression case with segment timestamps exceeding total decoded duration.

### A2 (P1): Library-code `.expect()` violation
- **Location:** `crates/movie-radio-voice/src/voice/openai.rs:91`
- **Bug:** `last_err.expect("retry loop runs at least once")` — the only non-test
  `expect()` in library code; violates the AGENTS.md no-unwrap rule. All other 87 raw
  hits live inside `#[cfg(test)]` modules.
- **Fix:** Propagate via typed error instead of panicking (invariant is real but must
  not have a panic path).

### A3 (P2): Defensive guards in gap scoring
- **Location:** `crates/movie-radio-goap/src/gaps.rs:48,149,165`
- **Bug:** `segments.len() - 1` underflows on an empty slice; `seg.end_ms -
  seg.start_ms` underflows on inverted segments. Unreachable via current call sites,
  but the private API accepts arbitrary inputs.
- **Fix:** Bail on empty slices; saturating subtraction for durations.

### A4 (P2): Modal WAV parsing lacks container validation
- **Location:** `crates/movie-radio-voice/src/voice/modal.rs:54-63`
- **Bug:** Response parsed by blindly skipping a 44-byte header, assuming
  little-endian i16 mono PCM; only `len >= 44` is validated.
- **Fix:** Validate RIFF/WAVE magic, format tag, and channel count before conversion;
  fall through the provider chain on mismatch.

## B. Architecture Debt

### B1: Feature-gate local TTS inference dependencies
- **Location:** `crates/movie-radio-voice/Cargo.toml:17,20,23-24`
- llama-cpp-2 (full llama.cpp C++ build), candle-core (+nn/transformers), qwen_tts,
  and ort are unconditional → every workspace-wide `cargo build/test/clippy` pays the
  full native compile cost.
- **Proposal:** HTTP providers (openai/modal/elevenlabs) in the default build; gate
  native deps behind per-provider features (`kokoro`, `orpheus`, `qwen3`) or one
  `local-tts` umbrella feature.
- **Related caveat:** Kokoro tokenization (`kokoro.rs:128-131`) maps raw codepoints to
  token IDs instead of an eSD phoneme vocabulary — acoustically suspect even though
  ONNX inference is live. Fix alongside B1.
- **Related:** `ort = "2.0.0-rc.9"` is a pre-release pin in a production decode path;
  track its stable release.

### B2: Remove PocketTts stub (recommended)
- **Location:** `crates/movie-radio-voice/src/voice/pockettts.rs:19-31`
- Returns one second of silence ignoring request text while capabilities falsely
  advertise voice cloning and streaming.
- **Recommendation:** Remove the provider and its config plumbing entirely — consistent
  with the VAD-engine precedent ("only expose what exists", see
  `050-status-report/GAPS.md`). Reintroduce when a real implementation lands.

### B3: Fix dead `high-quality-resample` feature
- Declared as empty feature `[]`; `rubato` dependency is unconditional; no consumer or
  script enables it; CI tests without `--all-features` (ci.yml:178) so the sinc path
  (`resample.rs:3-36`) is compiled under clippy but never functionally tested.
- **Fix:** Make `rubato` optional behind the feature; add a CI leg testing with
  `--features movie-radio-pipeline/high-quality-resample`.

## C. Infrastructure / CI Gaps

| Gap | Evidence | Recommendation |
|-----|----------|----------------|
| MSRV enforcement orphaned | `scripts/audit-msrv.sh` defines MSRV=1.88 but is invoked by no workflow; `rust-toolchain.toml` pins only `stable`; no `rust-version` keys | Wire script into `ci.yml`; add `rust-version` to workspace manifest |
| Benchmark regression tracking unwired | `scripts/check_benchmark_regression.py` + `refresh_benchmark_baseline.sh` exist; `benchmark` job runs only on PR/push (fulfills PHASE-06 §6.7 once scheduled) | Weekly cron job comparing stage timings against stored baseline |
| Coverage collection missing | Root `.codecov.yml` exists; no workflow uploads coverage | Add cargo-llvm-cov step uploading to Codecov |
| No release automation | No tag-triggered build/publish/artifact workflow | Backlog: tag-triggered binary artifact workflow |
| Single-platform CI | Linux x86_64 only | Optional macOS leg if cross-platform support is claimed |
| Duplicate dep majors | symphonia 0.5+0.6 (rodio), two reqwest versions; `deny.toml:42-45` carries a skip | Track rodio upgrade to drop the symphonia skip |

## D. Test Coverage Gaps

- **movie-radio-types: 0 tests** across ~404 LOC of shared serde/domain types
  (Segment, Frame, Emotion, AudioOutput round-trips).
- movie-radio-io / movie-radio-validation parsers: EDL, SRT, VTT each carry exactly
  one test despite being parse-critical.
- No crate has an integration `tests/` directory; recommend starting with one
  end-to-end decode→framing fixture test.

## E. New-Feature Candidates (ranked)

Anchored to the ROADMAP decision at `100-radio-play-95/ROADMAP.md:103`: engine-level
speech/non-speech discrimination takes priority over profile micro-tuning (modern
precision plateaued at ~0.7368 vs the 0.95 gate).

1. **Multi-feature VAD tuning** (PHASE-06 §6.8) — feed already-computed spectral
   features (flux, centroid, band ratios) into classification with profile-aware
   thresholds. The concrete engine-level path to modern precision recovery; no ML.
2. **Streaming/chunked processing** (§6.4) — today the pipeline materializes entire
   soundtracks: 2 h @ 16 kHz mono f32 ≈ 460 MB per copy; `handle_radio_play` decodes
   the full movie up to three times (`radio_play.rs:90,104,190`);
   `decode_audio_chunked` accumulates all chunks anyway (`decode.rs:38-44,102`). Frame
   while decoding instead of handing off `Vec<f32>` boundaries.
3. **WAV format extension** (§6.5) — direct 24-bit/32-bit-float decode; small win.
4. **Validation/reporting UX** (§6.6); benchmark baselines covered by C above.
5. **Silero/WebRTC VAD** (§6.2) — stays deferred per `MILESTONE-C-DECISION.md`;
   revisit after engine-level DSP improvements plateau.

## Priority Matrix

| ID | Item | Priority | Effort |
|----|------|----------|--------|
| A1 | speech_evidence panic fix + regression test | P0 | S |
| A2 | openai.rs expect removal | P1 | S |
| A3 | gaps.rs defensive guards | P2 | S |
| A4 | modal.rs RIFF validation | P2 | S |
| B1 | voice crate feature-gating (+ Kokoro tokenizer fix) | P2 | M/L |
| B2 | PocketTts removal | P1 | S/M |
| B3 | high-quality-resample feature repair | P2 | S |
| C  | CI wiring (MSRV, benchmark baseline, coverage) | P2 | M |
| D  | types/io/validation test coverage | P2 | M |
| E1 | Multi-feature VAD tuning (engine-level lift) | P1 strategic | L |

## Recommended Next Actions

Per the Standard Workflow Loop (AGENTS.md), file issues before editing:

1. 🛠️ Coding Change issue (labels: `coding`, `radio-play`) for **A1** — smallest P0,
   immediate correctness win; bundle A2–A4 as follow-up scoped PRs.
2. 🛠️ Coding Change issue for **B2** (PocketTts removal) and **B3**.
3. ⚡ Performance Change issue (labels: `perf`, `learning`) for **B1** and **C**.
4. Start a GOAP goal in `plans/GOAP_STATE.md` when execution begins (A1 first).
