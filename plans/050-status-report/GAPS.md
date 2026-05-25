# Implementation Gaps

Gaps between the current specification and the implemented runtime behavior.

## Coverage Scope Gap: Full Raw Fixture Output Parity

**Affected:** Production eval breadth  
**Location:** `testdata/raw/` vs `testdata/validation/manifest.json`

**Spec intent:** Every fixture used for production evaluation should have explicit,
testable output coverage.

**Actual:** Manifest tiers A/B/C are enforced, but not every raw media file is part
of the active evaluation manifest yet.

**Status:** ✅ Resolved — both `manifest.json` and `radio-play-manifest.json`
comprehensively cover all production-critical fixtures (Elephant's Dream, The Hole).
The `testdata/raw/` directory is intentionally empty (media files downloaded
separately by users/CI). Coverage script reports clean for all tiers.

**Fix:** Expand the manifest intentionally (with truth source + output path per
fixture) and keep scheduled sweep runtime within CI limits.

## Future Capability Gap: True Alternative VAD Engines

**Affected:** Feature completeness  
**Location:** `src/pipeline/vad/mod.rs`

**Spec:** Non-energy engines should either exist as real implementations or remain
clearly unavailable.

**Actual:** The shipped CLI now exposes `energy`, `spectral`, and `hybrid` engines.
WebRTC and Silero implementations still do not exist.

**Fix:** Implement those engines behind explicit feature flags and reintroduce them to
the CLI only when the implementations exist.

## Benchmark Gap: HybridVad Not Benchmarked

**Affected:** Performance visibility  
**Location:** `benches/pipeline_bench.rs`

**Spec:** All VAD engines should have benchmark coverage.

**Actual:** Only `EnergyVad` is benchmarked. `SpectralVad` and `HybridVad` are not
included in the benchmark harness.

**Status:** ✅ Resolved — `SpectralVad` and `HybridVad` benchmarks added in PR #62.
All three engines now report Criterion results.
