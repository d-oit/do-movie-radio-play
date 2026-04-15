# Learning Integration - Issues and Next Steps

## Current State

The learning system is partially wired but not fully connected between components.

## Known Issues

### 1. Learning Thresholds Not Applied to Spectral VAD
- **Problem**: The adaptive thresholds learned from verification results are not passed to the SpectralVad engine during extraction
- **Impact**: The VAD continues using default thresholds even after learning
- **Location**: `src/pipeline/vad/spectral.rs` - needs threshold parameters
- **Fix**: Pass learned thresholds to create_engine() or use separate config loading

### 2. False Positive Detection Logic Incomplete
- **Problem**: The verification system marks segments as "suspicious" but doesn't distinguish between:
  - Verified false positive (user marked as voice)
  - Verified true positive (user confirmed as non-voice)
  - System-suspicious (algorithm flagged as suspicious)
- **Location**: `src/verification/mod.rs`
- **Fix**: Add explicit false positive tracking in review player that feeds back to learning

### 3. Review Player - Play All End Handling
- **Problem**: When Play All reaches the last segment, it doesn't indicate completion or loop
- **Location**: `src/review.rs:654-660`
- **Fix**: Add end-of-playlist indicator or auto-reset

### 4. Review Player - Segment Filtering/Sorting UI
- **Problem**: No UI controls to filter segments (e.g., by confidence, duration) or sort
- **Location**: `src/review.rs`
- **Fix**: Add filter dropdown and sort controls

## What Works

1. ✅ Ctrl+S saves reviewed HTML
2. ✅ Learning state file created with verification results
3. ✅ Threshold recommendations generated
4. ✅ Spectral VAD exists with enhanced features

## Next Steps Priority

1. Wire learned thresholds into spectral VAD via config
2. Add explicit false positive feedback from review to learning
3. Fix Play All end handling
4. Add segment filtering UI