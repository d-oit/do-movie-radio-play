# nonvoice-segmentation references
- Determinism and testability first.
- Use `min_non_voice_ms >= 500` for radio-play/movie review sweeps.
- Segment confidence now includes duration adjustment; very short segments are down-weighted.
- Use review flow (`x`, `e`, `Ctrl+S`) to capture false positives and feed learning DB.
- For production runs, prefer generated profiles from sweep policy (`modern-optimized`, `legacy-optimized`).
