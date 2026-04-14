# Implementation Gaps

Gaps between the current specification and the implemented runtime behavior.

## Coverage Scope Gap: Full Raw Fixture Output Parity

**Affected:** Production eval breadth  
**Location:** `testdata/raw/` vs `testdata/validation/manifest.json`

**Spec intent:** Every fixture used for production evaluation should have explicit,
testable output coverage.

**Actual:** Manifest tiers A/B/C are enforced, but not every raw media file is part
of the active evaluation manifest yet.

**Fix:** Expand the manifest intentionally (with truth source + output path per
fixture) and keep scheduled sweep runtime within CI limits.

## Future Capability Gap: True Alternative VAD Engines

**Affected:** Feature completeness  
**Location:** `src/pipeline/vad/mod.rs`

**Spec:** Non-energy engines should either exist as real implementations or remain
clearly unavailable.

**Actual:** The shipped CLI now exposes only the implemented energy engine, but real
WebRTC and Silero implementations still do not exist.

**Fix:** Implement those engines behind explicit feature flags and reintroduce them to
the CLI only when the implementations exist.
