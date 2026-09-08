use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::info;

use crate::gaps::GapIdentifier;
use crate::narrate::NarrationGenerator;
use crate::{Action, PipelineContext, WorldState};
use movie_radio_pipeline::pipeline::decode::decode_audio;
use movie_radio_pipeline::pipeline::extract_timeline;
use movie_radio_voice::config::{
    ElevenLabsConfig, ModalConfig, OpenAiConfig, VoiceProvidersConfig, VoiceSynthesisConfig,
};
use movie_radio_voice::voice::{SynthesisOrchestrator, SynthesisRequest};

#[derive(Debug, Default)]
pub struct DecodeMovie;

#[async_trait]
impl Action for DecodeMovie {
    fn name(&self) -> &str {
        "decode_movie"
    }
    fn preconditions(&self) -> WorldState {
        WorldState::default()
    }
    fn effects(&self) -> WorldState {
        WorldState {
            movie_decoded: true,
            ..WorldState::default()
        }
    }
    fn cost(&self, _state: &WorldState) -> f32 {
        1.0
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> Result<()> {
        info!(movie = %ctx.movie_path.display(), "Decoding movie audio");
        let (samples, sample_rate) = decode_audio(&ctx.movie_path, ctx.sample_rate)?;
        ctx.original_audio = Some(samples);
        ctx.sample_rate = sample_rate;
        info!(
            samples = ctx.original_audio.as_ref().map_or(0, |s| s.len()),
            "Movie decoded"
        );
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct ExtractTimeline;

#[async_trait]
impl Action for ExtractTimeline {
    fn name(&self) -> &str {
        "extract_timeline"
    }
    fn preconditions(&self) -> WorldState {
        WorldState {
            movie_decoded: true,
            ..WorldState::default()
        }
    }
    fn effects(&self) -> WorldState {
        WorldState {
            audio_timeline_extracted: true,
            ..WorldState::default()
        }
    }
    fn cost(&self, _state: &WorldState) -> f32 {
        2.0
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> Result<()> {
        info!("Extracting audio timeline");
        let timeline = extract_timeline(&ctx.movie_path, &ctx.config)?;
        info!(segments = timeline.segments.len(), "Timeline extracted");
        ctx.timeline = Some(timeline);
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct IdentifyVisualGaps;

#[async_trait]
impl Action for IdentifyVisualGaps {
    fn name(&self) -> &str {
        "identify_visual_gaps"
    }
    fn preconditions(&self) -> WorldState {
        WorldState {
            audio_timeline_extracted: true,
            ..WorldState::default()
        }
    }
    fn effects(&self) -> WorldState {
        WorldState {
            visual_gaps_identified: true,
            ..WorldState::default()
        }
    }
    fn cost(&self, _state: &WorldState) -> f32 {
        3.0
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> Result<()> {
        let timeline = ctx.timeline.as_ref().context("Timeline not extracted")?;
        let srt_content = ctx
            .subtitles_path
            .as_ref()
            .map(std::fs::read_to_string)
            .transpose()?;

        info!("Identifying visual gaps");
        let identifier = GapIdentifier::new();
        let gap_analysis = identifier.identify_gaps(timeline, srt_content.as_deref())?;
        info!(gaps = gap_analysis.gaps.len(), "Gaps identified");
        ctx.gap_analysis = Some(gap_analysis);
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct GenerateNarration;

#[async_trait]
impl Action for GenerateNarration {
    fn name(&self) -> &str {
        "generate_narration"
    }
    fn preconditions(&self) -> WorldState {
        WorldState {
            visual_gaps_identified: true,
            ..WorldState::default()
        }
    }
    fn effects(&self) -> WorldState {
        WorldState {
            narration_scripts_generated: true,
            ..WorldState::default()
        }
    }
    fn cost(&self, _state: &WorldState) -> f32 {
        2.0
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> Result<()> {
        let timeline = ctx.timeline.as_ref().context("Timeline not extracted")?;
        let gaps = &ctx
            .gap_analysis
            .as_ref()
            .context("Gaps not identified")?
            .gaps;

        info!("Generating narration scripts");
        let generator = NarrationGenerator::default();
        let scripts = generator.generate(timeline, gaps)?;
        info!(scripts = scripts.len(), "Narration scripts generated");
        ctx.scripts = Some(scripts);
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct SynthesizeNarrator;

#[async_trait]
impl Action for SynthesizeNarrator {
    fn name(&self) -> &str {
        "synthesize_narrator"
    }
    fn preconditions(&self) -> WorldState {
        WorldState {
            narration_scripts_generated: true,
            ..WorldState::default()
        }
    }
    fn effects(&self) -> WorldState {
        WorldState {
            narrator_voice_synthesized: true,
            ..WorldState::default()
        }
    }
    fn cost(&self, _state: &WorldState) -> f32 {
        5.0
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> Result<()> {
        let scripts = ctx.scripts.as_ref().context("Scripts not generated")?;
        let voice_config = voice_config_from_env();
        let orchestrator = SynthesisOrchestrator::new(voice_config);

        for (i, script) in scripts.iter().enumerate() {
            info!(
                i = i + 1,
                total = scripts.len(),
                text = %script.text,
                "Synthesizing narration"
            );

            let request = SynthesisRequest {
                text: script.text.clone(),
                emotion: script.emotion.clone(),
                voice_id: None,
                language: "de".to_string(),
                speed: 1.0,
                sample_rate_hz: ctx.sample_rate,
            };

            if let Err(e) = request.validate() {
                tracing::warn!(i = i + 1, error = %e, "Invalid synthesis request, skipping");
                ctx.narration_audio.push(None);
                continue;
            }

            match orchestrator.synthesize(&request).await {
                Ok(audio) => {
                    info!(
                        i = i + 1,
                        samples = audio.samples.len(),
                        "Narration synthesized"
                    );
                    ctx.narration_audio.push(Some(audio));
                }
                Err(e) => {
                    tracing::warn!(i = i + 1, error = %e, "TTS failed, skipping");
                    ctx.narration_audio.push(None);
                }
            }
        }

        if !scripts.is_empty() && ctx.narration_audio.iter().all(Option::is_none) {
            anyhow::bail!(
                "all {} narration syntheses failed; check TTS provider configuration \
                 (e.g. OPENAI_API_KEY / OPENAI_TTS_BASE_URL / MODAL_TTS_ENDPOINT)",
                scripts.len()
            );
        }

        info!(
            count = ctx.narration_audio.iter().filter(|a| a.is_some()).count(),
            total = scripts.len(),
            "Narration synthesis complete"
        );
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct AssembleRadioPlay;

#[async_trait]
impl Action for AssembleRadioPlay {
    fn name(&self) -> &str {
        "assemble_radio_play"
    }
    fn preconditions(&self) -> WorldState {
        WorldState {
            narrator_voice_synthesized: true,
            movie_decoded: true,
            ..WorldState::default()
        }
    }
    fn effects(&self) -> WorldState {
        WorldState {
            radio_play_assembled: true,
            ..WorldState::default()
        }
    }
    fn cost(&self, _state: &WorldState) -> f32 {
        1.5
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> Result<()> {
        use crate::assemble::RadioPlayAssembler;

        let original = ctx
            .original_audio
            .as_ref()
            .context("Original audio not decoded")?;
        let scripts = ctx.scripts.as_ref().context("Scripts not generated")?;

        let assembler = RadioPlayAssembler::new(ctx.sample_rate, 50, 0.3);
        let narration_segments =
            build_narration_segments(scripts, &ctx.narration_audio, &assembler);

        let radio_play = assembler.assemble(original, &narration_segments)?;

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: ctx.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = hound::WavWriter::create(&ctx.output_path, spec)?;
        for &s in &radio_play {
            let clamped = s.clamp(-1.0, 1.0);
            let sample = (clamped * i16::MAX as f32) as i16;
            writer.write_sample(sample)?;
        }
        writer.finalize()?;

        info!(
            output = %ctx.output_path.display(),
            duration_s = radio_play.len() as f64 / ctx.sample_rate as f64,
            "Radio play assembled"
        );
        Ok(())
    }
}

pub(crate) fn build_narration_segments(
    scripts: &[crate::narrate::NarrationScript],
    narration_audio: &[Option<movie_radio_voice::AudioOutput>],
    assembler: &crate::assemble::RadioPlayAssembler,
) -> Vec<crate::assemble::NarrationSegment> {
    scripts
        .iter()
        .zip(narration_audio.iter())
        .filter_map(|(script, audio)| {
            audio
                .as_ref()
                .map(|audio| assembler.narration_to_segment(script, &audio.samples))
        })
        .collect()
}

fn voice_config_from_env() -> VoiceSynthesisConfig {
    const ENV_ELEVENLABS_API_KEY: &str = "ELEVENLABS_API_KEY";
    const ENV_OPENAI_API_KEY: &str = "OPENAI_API_KEY";
    const ENV_OPENAI_TTS_BASE_URL: &str = "OPENAI_TTS_BASE_URL";

    let openai_cfg = if std::env::var(ENV_OPENAI_API_KEY).is_ok() {
        Some(OpenAiConfig {
            api_key_env: Some(ENV_OPENAI_API_KEY.to_string()),
            base_url: movie_radio_voice::config::default_openai_base_url(),
            model: "tts-1-hd".to_string(),
            voice: "onyx".to_string(),
            response_format: "mp3".to_string(),
        })
    } else {
        std::env::var(ENV_OPENAI_TTS_BASE_URL)
            .ok()
            .map(|base_url| OpenAiConfig {
                api_key_env: None,
                base_url,
                model: "pocket-tts".to_string(),
                voice: "alba".to_string(),
                response_format: "wav".to_string(),
            })
    };

    VoiceSynthesisConfig {
        provider: "modal".to_string(),
        fallback_chain: vec![
            "modal".to_string(),
            "elevenlabs".to_string(),
            "openai".to_string(),
        ],
        emotion_mapping: true,
        language: "de".to_string(),
        voice_id: None,
        max_cost_per_run_usd: 25.0,
        providers: VoiceProvidersConfig {
            kokoro: None,
            qwen3: None,
            orpheus: None,
            elevenlabs: std::env::var(ENV_ELEVENLABS_API_KEY)
                .ok()
                .map(|_| ElevenLabsConfig {
                    api_key_env: ENV_ELEVENLABS_API_KEY.to_string(),
                    voice_id: "pNInz6obpgDQGcFmaJgB".to_string(),
                    model: "eleven_multilingual_v2".to_string(),
                    stability: 0.5,
                    similarity_boost: 0.75,
                }),
            modal: Some(ModalConfig {
                endpoint_url_env: "MODAL_TTS_ENDPOINT".to_string(),
                max_monthly_cost: 25.0,
            }),
            openai: openai_cfg,
            audio_cpp: None,
        },
    }
}
