# audio-vad-cpu references
- Determinism and testability first.
- Spectral VAD is now available in CLI (`--vad-engine spectral`).
- Modern CGI fixture (`elephants_dream_2006`) validates well under spectral defaults.
- Legacy film fixtures require stricter profile tuning and likely separate threshold bands.
