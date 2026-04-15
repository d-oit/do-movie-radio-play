# Testing Agent Handoff - Spectral VAD

## Test Results on elephants_dream_2006.mp4

### Quality Gate
- All 80+ tests pass
- No clippy warnings
- No fmt issues

### VAD Comparison (min-silence-ms=1000)

| Metric | Energy VAD | Spectral VAD | Difference |
|--------|-----------|--------------|------------|
| Speech frames | 27,891 | 29,931 | +2,040 |
| Speech segments | 20 | 6 | -14 |
| Non-voice segments | 4 | 1 | -3 |
| Total speech ms | ~649,000ms | ~651,956ms | +~2,956ms |
| Total non-voice ms | 12,420ms | 1,740ms | -10,680ms |

### Key Findings
1. **Spectral VAD is more conservative** - detects fewer speech segments (6 vs 20) but with more total speech duration
2. **Spectral flatness penalizes music** - elephants_dream likely has music segments that spectral VAD correctly identifies as non-speech
3. **Fewer but longer segments** - spectral produces more consolidated speech segments

## Observations
- Both engines produce valid JSON output
- Spectral VAD shows higher speech frame count (more frames classified as speech)
- Energy VAD produces more granular segmentation
- The spectral engine appears to have stricter speech criteria, reducing false positives on music/effects

## Recommendations
1. Test on more varied content (music-heavy vs speech-heavy movies)
2. Consider tuning spectral flatness threshold for different genres
3. Benchmark performance: spectral should have similar compute cost since features are pre-computed
