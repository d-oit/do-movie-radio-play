# Implementation Gaps

Gaps between the current specification and the implemented runtime behavior.

## Future Capability Gap: True Alternative VAD Engines

**Affected:** Feature completeness  
**Location:** `src/pipeline/vad/mod.rs`

**Spec:** Non-energy engines should either exist as real implementations or remain
clearly unavailable.

**Actual:** The shipped CLI now exposes only the implemented energy engine, but real
WebRTC and Silero implementations still do not exist.

**Fix:** Implement those engines behind explicit feature flags and reintroduce them to
the CLI only when the implementations exist.
