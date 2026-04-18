# audio-vad-cpu references
- Determinism and testability first.
- Spectral VAD is now available in CLI (`--vad-engine spectral`).
- Modern CGI fixture (`elephants_dream_2006`) validates well under spectral defaults.
- Legacy film fixtures require stricter profile tuning and likely separate threshold bands.
- Cohort-aware sweep ranking is available in `analysis/optimization/fp-sweep-ranked.json`.
- Coverage guard (`--min-coverage-ratio`) prevents low-coverage candidate selection.
- Radio-play recovery execution plan is tracked in `plans/100-radio-play-95/ROADMAP.md`.
