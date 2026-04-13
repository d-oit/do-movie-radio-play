# Fixtures

## Layer 1: Synthetic WAV (Primary)
- Deterministic generation in code
- `testdata/generated/*.wav` with `.truth.json`
- Tolerance: ±50-100ms

## Layer 2: Datasets (Robustness)  
- LibriSpeech, Mozilla Common Voice
- Tolerance: ±100-200ms

## Layer 3: Movies + Subtitles (Realism)
- See [MOVIES.md](./MOVIES.md)
- Public domain films with actual dialogue
- Tolerance: ±200-500ms (subtitle timing mismatch)
