# Learning Integration - Issues and Next Steps

## Current State

The learning system is now wired between components:
1. ✅ Learning thresholds configured in config (spectral_flatness_max, etc.)
2. ✅ Thresholds passed from config to SpectralVad engine
3. ✅ Verification records suspicious segments
4. ✅ Learning state created from verification results
5. ✅ Threshold recommendations generated from learning state

## Known Issues

### 1. False Positive Detection Logic Incomplete
- **Problem**: The verification system marks segments as "suspicious" but doesn't distinguish between:
  - Verified false positive (user marked as voice)
  - Verified true positive (user confirmed as non-voice)
  - System-suspicious (algorithm flagged as suspicious)
- **Location**: `src/verification/mod.rs`
- **Fix**: Add explicit false positive tracking in review player that feeds back to learning
- **Status**: Partially addressed - excluded segments tracked in review, suspicious flagged in verification

## What Works

1. ✅ Ctrl+S saves reviewed HTML
2. ✅ Learning state file created with verification results
3. ✅ Threshold recommendations generated
4. ✅ Spectral VAD uses configurable thresholds from config
5. ✅ Play All shows "Done!" message
6. ✅ Filter/Sort UI in segment list