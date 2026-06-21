# ADR-121: AI Voice Synthesis with Emotion — Provider Architecture

**Date:** 2026-06-21
**Status:** Proposed
**Depends on:** ADR-120 (GOAP pipeline)

## Context

Radio play listeners need narrated descriptions of visual scenes that carry appropriate emotion (tense, joyful, mysterious). The system must support:
- Free/local models for offline, cost-free operation (CPU or GPU)
- Paid APIs for maximum quality when budget allows
- User-configurable provider selection with fallback chains
- Emotion control: the synthesized voice must convey scene mood

### 2026 TTS Landscape (Researched)

| Provider | Type | Size | Emotion Control | Language | License | Rust Support |
|----------|------|------|-----------------|----------|---------|--------------|
| **Kokoro-82M** | Local/ONNX | 82M params | Style tokens per voice | EN (+community langs) | Apache 2.0 | `tts-rs` crate (ONNX) |
| **Qwen3-TTS** | Local/GGML+ONNX | ~1.5B | Description-based ("speak sadly"), voice cloning | 20+ languages | Apache 2.0 | `qts`, `qwen3-tts-rs`, `rlx-qwen3-tts` |
| **Orpheus-3B** | Local/GGUF | 3B | Inline tags: `[whispers]`, `[excited]`, `[laughs]`, `[sad]` | EN, DE, multilingual | Apache 2.0 | llama.cpp via FFI |
| **Supertonic** | Local/ONNX | 99M | Voice style selection | EN | — | `supertonic-ort-backend` |
| **ElevenLabs v3** | API | — | Emotion via style + stability params, voice cloning | 70+ languages | Paid ($5-22/mo) | HTTP REST |
| **OpenAI TTS-1 HD** | API | — | Limited (voice selection) | Multi | Paid (per-char) | HTTP REST |
| **Azure Neural Voice** | API | — | SSML prosody + emotion tags | 100+ languages | Paid (per-char) | HTTP REST |

## Decision

Implement a `VoiceSynthesizer` trait with pluggable providers, selected via user configuration. Default: Kokoro (fast, free, CPU). Recommended for quality: Qwen3-TTS (emotion-rich, local) or ElevenLabs (highest fidelity, paid).

### Provider Trait

```rust
#[async_trait]
pub trait VoiceSynthesizer: Send + Sync {
    /// Synthesize text with emotion to audio samples
    fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput>;
    /// Provider capabilities
    fn capabilities(&self) -> ProviderCapabilities;
    /// Estimated cost for a request (0.0 for local)
    fn estimate_cost(&self, text_len: usize) -> f64;
}

pub struct SynthesisRequest {
    pub text: String,
    pub emotion: Emotion,
    pub voice_id: Option<String>,
    pub language: String,
    pub speed: f32,          // 0.5 - 2.0
    pub sample_rate_hz: u32, // output sample rate
}

pub enum Emotion {
    Neutral,
    Excited,
    Sad,
    Tense,
    Mysterious,
    Joyful,
    Whisper,
    Angry,
    Custom(String), // pass-through for provider-specific tags
}

pub struct ProviderCapabilities {
    pub supports_emotion: bool,
    pub supports_voice_cloning: bool,
    pub supports_streaming: bool,
    pub max_text_length: usize,
    pub languages: Vec<String>,
    pub requires_gpu: bool,
}
```

### User Configuration

```json
{
  "voice_synthesis": {
    "provider": "qwen3",
    "fallback_chain": ["kokoro", "elevenlabs"],
    "emotion_mapping": true,
    "language": "de",
    "voice_id": null,
    "max_cost_per_run_usd": 5.0,
    "providers": {
      "kokoro": {
        "model_path": "models/kokoro-82m.onnx",
        "device": "cpu"
      },
      "qwen3": {
        "model_path": "models/qwen3-tts-0.6b.gguf",
        "vocoder_path": "models/qwen3-vocoder.onnx",
        "device": "auto",
        "voice_description": "A calm male narrator with warm tone"
      },
      "orpheus": {
        "model_path": "models/orpheus-3b-q8.gguf",
        "device": "auto"
      },
      "elevenlabs": {
        "api_key_env": "ELEVENLABS_API_KEY",
        "voice_id": "narrator-deep",
        "model": "eleven_multilingual_v3",
        "stability": 0.6,
        "similarity_boost": 0.8
      },
      "openai": {
        "api_key_env": "OPENAI_API_KEY",
        "voice": "onyx",
        "model": "tts-1-hd"
      }
    }
  }
}
```

### Emotion Mapping Strategy

The GOAP pipeline's `prepare_voice_scripts` action maps segment tags to emotions:

| Segment Tag | Scene Context | Emotion |
|-------------|--------------|---------|
| `music_dramatic` | Action scene | Excited/Tense |
| `ambience_quiet` | Transition | Neutral |
| `sfx_explosion` | Combat | Excited |
| `music_sad` | Emotional scene | Sad |
| `silence` | Suspense | Whisper/Mysterious |

For Orpheus-3B, emotions map to inline tags:
```
[excited] The hero charges through the flames [laughs]
[sad] She watches the train disappear into the fog
[whispers] Something moves in the shadows behind them
```

For Qwen3-TTS, emotions map to voice descriptions:
```
"Speak with urgency and excitement, as if describing a chase scene"
"Speak softly and sadly, narrating a farewell"
```

### Device Selection (CPU/GPU)

```rust
pub enum Device {
    Cpu,
    Gpu(usize),  // GPU index
    Auto,        // detect best available
}

impl Device {
    pub fn resolve(&self) -> Device {
        match self {
            Device::Auto => {
                if gpu_available() { Device::Gpu(0) }
                else { Device::Cpu }
            }
            other => other.clone(),
        }
    }
}
```

- **CPU path**: ONNX Runtime (Kokoro, Supertonic) or GGML (Qwen3, Orpheus via llama.cpp)
- **GPU path**: ONNX Runtime with CUDA/DirectML EP, or GGML with CUDA offload
- **Fallback**: if GPU OOM, GOAP replanner triggers CPU fallback automatically

### Model Download & Management

```bash
# CLI command for model management
timeline models download kokoro    # downloads ~160MB ONNX model
timeline models download qwen3     # downloads ~1.2GB GGUF + vocoder
timeline models download orpheus   # downloads ~3.2GB GGUF (Q8_0)
timeline models list               # shows installed models + sizes
timeline models verify             # checksum verification
```

Models stored in `$XDG_DATA_HOME/do-movie-radio-play/models/` (Linux) or user-configured path.

## Implementation

### Phase 7.3.1: Provider Trait + Kokoro (3 days)
- Trait definition in `src/voice/mod.rs`
- Kokoro provider via `ort` crate (ONNX Runtime)
- CPU-only, fastest inference, basic quality

### Phase 7.3.2: Qwen3-TTS Provider (4 days)
- Integration via `qts` or `qwen3-tts-rs` crate
- GGML main model + ONNX vocoder
- Emotion via description-based control
- Voice cloning support

### Phase 7.3.3: Orpheus Provider (3 days)
- GGUF loading via `llama-cpp-2` crate
- Inline emotion tag injection
- German language support (important for this project's test corpus)

### Phase 7.3.4: API Providers (2 days)
- ElevenLabs REST client (reqwest)
- OpenAI TTS client
- Cost tracking per request

### Phase 7.3.5: Fallback Chain + GOAP Integration (2 days)
- Provider selection in GOAP action cost calculation
- Automatic fallback on failure
- Budget-aware provider switching

## Consequences

**Positive:**
- Users choose quality/cost/privacy tradeoff via config
- Works fully offline with local models (no internet required)
- GOAP replanning handles provider failures gracefully
- Emotion-rich narration improves radio play listener experience
- Rust-native crates exist for all recommended local providers

**Negative:**
- Local models require disk space (160MB - 3.2GB per model)
- CPU inference for Orpheus-3B is slow (~5x realtime on modern CPU)
- Quality varies significantly between free and paid providers
- Emotion mapping from segment tags is heuristic (improvable via learning)

**Risks:**
- Rust TTS crates are young (2025-2026); API stability uncertain
- Voice cloning raises legal/ethical considerations (user responsibility)
- Long movies need batched synthesis to avoid memory issues
