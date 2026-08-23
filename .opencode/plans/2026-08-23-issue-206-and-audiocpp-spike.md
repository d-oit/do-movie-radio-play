# Execution Plan — #206 Completion + audio.cpp Spike

**Status**: Approved by maintainer, awaiting execution unlock (plan-mode `edit` deny is active).
**Branches**: work continues on `fix/synthesis-request-validation` (#206 diff already staged locally).

## Task 1 — #206 completion

### R1 (crates/movie-radio-voice/src/voice/mod.rs)
- `SPEED_RANGE`: `0.5..=2.0` → `0.25..=4.0` (issue #206 contract; sole consumer openai.rs payload documents 0.25–4.0)
- Error string `SpeedOutOfRange` → `"outside supported finite range 0.25..=4.0"`
- Struct field comment → `// 0.25 - 4.0`

### R2 (tests in same file)
- `test_speed_boundaries`: absolute pins — pass `0.25`, `4.0`; reject `0.24`, `4.5`
- `test_sample_rate_boundaries`: keep absolute pins (8000/48000 pass, 7999/48001 reject)
- `test_speed_must_be_finite`: assert NaN/+inf/-inf rejected **through `validate()`**, not via std `RangeInclusive::contains`

### Deliberately NOT doing (→ plans/FOLLOWUPS.md Open table)
1. All-narrations-skipped → exit 0 silent output (actions.rs ~234): pre-existing semantic; validation guard only adds deterministic trigger
2. `actions.rs:279` zip misalignment on middle-skip: pre-existing
3. Per-provider `max_text_length` enforcement: future hardening
4. Config-side 8k–48k validation for TIMELINE_SAMPLE_RATE: latent-only today (env never wired into radio-play synthesis path)

### Gates & landing
1. `BINDGEN_EXTRA_CLANG_ARGS="-I/usr/lib/gcc/x86_64-linux-gnu/13/include"` + `cargo fmt && cargo fmt --check`
2. `cargo clippy -p movie-radio-voice -p movie-radio-goap --all-targets -- -D warnings`
3. `cargo test -p movie-radio-voice -p movie-radio-goap` (expect 12+17 pass)
4. Update plans/GOAP_STATE.md evidence (speed decision rationale + audio.cpp analysis outcome)
5. Commit: `fix(voice): enforce 0.25..=4.0 speed bounds per issue contract; pin boundary tests`
6. Push branch → PR → poll CI (rate-limit aware) → require full green on verified headRefOid → manual squash merge (NO automerge)
7. Close #206 with comment incl. speed-range rationale; pull main; delete merged branches if policy allows

## Task 2 — audio.cpp spike (no repo code changes)

1. `git clone https://github.com/0xShug0/audio.cpp /tmp/opencode/audio.cpp` (checkout latest release tag, e.g. v0.6.1)
2. Build CPU-only, minimal model set:
   `scripts/build_linux.sh --backend cpu --model-set custom --models pocket_tts --target audiocpp_cli`
3. Obtain PocketTTS Q8 GGUF package (model manager or HF audio-cpp/audio.cpp-gguf)
4. Synthesize fixed German narration corpus (same lines as repo test scripts), output 16 kHz WAV
5. Baseline A/B: existing Kokoro German ONNX (`kokoro-german-martin.onnx`) via KokoroProvider at same text/rate
6. Metrics: audiocpp `--metrics` (RTF, wall time) vs kokoro wall time; subjective listening notes
7. Write findings to `plans/audiocpp-tts-spike.md` (verdict + numbers); decision gate: de quality ≥ Kokoro AND acceptable CPU RTF → draft Phase 2 `AudioCppProvider` design (HTTP `/v1/audio/speech`, base_url config, lazy health check, mocked unit tests)

## Guardrails
- No automerge; head-SHA check before every merge; no blind CI retries
- Spike stays outside workspace; nothing committed from Task 2 except the plans/ report doc
