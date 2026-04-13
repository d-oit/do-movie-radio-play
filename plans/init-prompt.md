You are Codex running locally in FULL-AUTO mode.

You must create a COMPLETE, production-grade Rust repository from scratch.

DO NOT:
- stop early
- output partial code
- leave TODOs
- ask questions

You MUST iterate until:
- code compiles
- all tests pass
- benchmarks run
- outputs are deterministic
- repository is production-ready

--------------------------------------------------
PROJECT
--------------------------------------------------

Name:
movie-nonvoice-timeline

Goal:
Build a CPU-only Rust CLI that extracts non-voice timeline segments from media, validates against timestamped text, and supports production-level testing and observability.

--------------------------------------------------
CORE CAPABILITIES
--------------------------------------------------

1. Decode media audio
2. Resample to analysis format
3. Detect speech vs non-speech
4. Smooth segmentation
5. Invert to non-voice regions
6. Output deterministic JSON
7. Tag non-voice segments
8. Generate narration prompts
9. Validate against timestamped text
10. Provide full testing + benchmarks

--------------------------------------------------
EXECUTION LOOP (MANDATORY)
--------------------------------------------------

Repeat until complete:

1. PLAN
2. IMPLEMENT
3. TEST
4. FIX
5. HARDEN
6. REPEAT

--------------------------------------------------
REPOSITORY STRUCTURE
--------------------------------------------------

movie-nonvoice-timeline/
├── AGENTS.md
├── README.md
├── Cargo.toml
├── VERSION
├── CHANGELOG.md
├── SECURITY.md
├── .gitignore
├── .agents/
│   └── skills/
│       ├── audio-vad-cpu/
│       ├── nonvoice-segmentation/
│       ├── triz-audio-timeline/
│       └── self-learning-calibration/
├── plans/
│   ├── 000-overview/
│   ├── 010-architecture/
│   ├── 020-triz/
│   ├── 030-implementation/
│   └── 040-validation/
├── analysis/
│   ├── benchmarks/
│   ├── quality/
│   └── learnings/
├── scripts/
│   ├── quality_gate.sh
│   ├── benchmark.sh
│   └── fetch_test_assets.sh
├── src/
│   ├── main.rs
│   ├── cli.rs
│   ├── config.rs
│   ├── error.rs
│   ├── pipeline/
│   │   ├── decode.rs
│   │   ├── resample.rs
│   │   ├── framing.rs
│   │   ├── vad.rs
│   │   ├── segmenter.rs
│   │   ├── features.rs
│   │   ├── tags.rs
│   │   └── prompts.rs
│   ├── validation/
│   │   ├── comparator.rs
│   │   ├── metrics.rs
│   │   └── subtitle.rs
│   ├── fixtures/
│   │   └── generator.rs
│   └── types/
│       ├── segment.rs
│       └── metrics.rs
├── tests/
├── benches/
└── testdata/

--------------------------------------------------
PIPELINE
--------------------------------------------------

decode → resample → frame → VAD → smoothing → segments → invert → non-voice → JSON

Constraints:
- 16kHz mono
- 20ms frames
- f32 internal
- CPU only
- no unwrap/expect

--------------------------------------------------
DATA MODEL
--------------------------------------------------

Segment:
- start_ms: u64
- end_ms: u64
- kind: speech | non_voice
- confidence: f32
- tags: Vec<String>
- prompt: Option<String>

--------------------------------------------------
VALIDATION SYSTEM (CRITICAL)
--------------------------------------------------

Implement THREE layers:

------------------------------------------
LAYER 1 — SYNTHETIC (PRIMARY)
------------------------------------------

Implement deterministic fixture generator:

Generate:
- audio files
- ground truth JSON

Include:
- silence
- speech
- alternating segments
- speech + noise
- speech + music
- impulse-heavy non-voice
- short speech bursts
- long ambience

Tolerance:
±50–100 ms

------------------------------------------
LAYER 2 — DATASETS (ROBUSTNESS)
------------------------------------------

Use:
- LibriSpeech
- Mozilla Common Voice

References:
- https://www.openslr.org/12
- https://commonvoice.mozilla.org/

Convert:
- timestamps → speech segments
- build mixed audio timelines

Tolerance:
±100–200 ms

------------------------------------------
LAYER 3 — MOVIES + SUBTITLES (REALISM)
------------------------------------------

Use:
- movie file
- subtitle (.srt)

Reference:
- https://en.wikipedia.org/wiki/SubRip

Implement:
- SRT parser
- subtitle → speech segments
- invert → non-voice

Tolerance:
±200–500 ms

IMPORTANT:
Subtitles are NOT perfectly aligned → use tolerance.

--------------------------------------------------
VALIDATION METRICS
--------------------------------------------------

Implement:
- overlap ratio
- boundary error (ms)
- speech precision / recall
- non-voice precision / recall

--------------------------------------------------
TAGGING
--------------------------------------------------

Use DSP features:
- RMS
- spectral flux
- centroid
- band ratios

Tags:
- ambience
- music_bed
- impact_heavy
- crowd_like
- machinery_like
- nature_like

--------------------------------------------------
PROMPTS
--------------------------------------------------

Generate for non-voice segments:

Rules:
- short
- deterministic
- safe

Example:
"No dialogue. Ambient environmental sound."

--------------------------------------------------
LOGGING
--------------------------------------------------

Use tracing:

- INFO default
- DEBUG optional

Log:
- pipeline stages
- timings
- config

--------------------------------------------------
ERROR HANDLING
--------------------------------------------------

Global error boundary.

Handle:
- missing file
- decode failure
- invalid JSON
- invalid config
- empty audio

Return non-zero exit codes.

--------------------------------------------------
TESTING
--------------------------------------------------

Unit:
- segmentation
- smoothing
- inversion
- tagging

Integration:
- CLI commands
- fixture validation
- error handling

Regression:
- deterministic outputs
- golden JSON

--------------------------------------------------
BENCHMARKS
--------------------------------------------------

Implement:
- benchmark command
- output to analysis/benchmarks/

--------------------------------------------------
AGENTS SYSTEM
--------------------------------------------------

Use:
https://github.com/d-o-hub/github-template-ai-agents

AGENTS.md:
- rules
- constants
- quality gates

Skills:
- audio-vad-cpu
- nonvoice-segmentation
- triz-analysis
- self-learning-calibration

--------------------------------------------------
AUDIO PROCESSING REFERENCES
--------------------------------------------------

FFmpeg:
https://ffmpeg.org/

libavcodec:
https://www.ffmpeg.org/libavcodec.html

libswresample:
https://ffmpeg.org/libswresample.html

Rust ffmpeg-next:
https://docs.rs/ffmpeg-next

realfft:
https://docs.rs/realfft

rubato:
https://docs.rs/rubato

--------------------------------------------------
VAD REFERENCES
--------------------------------------------------

WebRTC VAD:
https://github.com/wiseman/py-webrtcvad

Silero VAD:
https://github.com/snakers4/silero-vad

--------------------------------------------------
CODEX CLI REFERENCES
--------------------------------------------------

OpenAI Codex:
https://openai.com/index/introducing-codex/

Codex CLI:
https://help.openai.com/en/articles/11096431-openai-codex-ci-getting-started

GitHub:
https://github.com/openai/codex

--------------------------------------------------
QUALITY GATE
--------------------------------------------------

Run:

cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test

Fix ALL issues.

--------------------------------------------------
FINAL OUTPUT
--------------------------------------------------

After everything passes, output ONLY:

- what was built
- what tests passed
- what benchmarks ran
- what remains optional

--------------------------------------------------
BEGIN NOW
--------------------------------------------------

Start with repository creation and execute full loop until complete.
