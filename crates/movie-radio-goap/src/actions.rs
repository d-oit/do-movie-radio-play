use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::info;

use crate::gaps::GapIdentifier;
use crate::narrate::NarrationGenerator;
use crate::{Action, PipelineContext, WorldState};
use movie_radio_pipeline::pipeline::decode::decode_audio;
use movie_radio_pipeline::pipeline::extract_timeline;

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
        use movie_radio_voice::config::ModalConfig;
        use movie_radio_voice::voice::modal::ModalTtsProvider;
        use movie_radio_voice::voice::{SynthesisRequest, VoiceSynthesizer};

        let scripts = ctx.scripts.as_ref().context("Scripts not generated")?;

        let modal_config = ModalConfig {
            endpoint_url_env: "MODAL_TTS_ENDPOINT".to_string(),
            max_monthly_cost: 25.0,
        };
        let provider = ModalTtsProvider::new(modal_config);

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

            match provider.synthesize(&request).await {
                Ok(audio) => {
                    info!(
                        i = i + 1,
                        samples = audio.samples.len(),
                        "Narration synthesized"
                    );
                    ctx.narration_audio.push(audio);
                }
                Err(e) => {
                    tracing::warn!(i = i + 1, error = %e, "TTS failed, skipping");
                }
            }
        }

        info!(
            count = ctx.narration_audio.len(),
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
        let mut narration_segments = Vec::new();

        for (i, (script, audio)) in scripts.iter().zip(ctx.narration_audio.iter()).enumerate() {
            let segment = assembler.narration_to_segment(script, &audio.samples);
            narration_segments.push(segment);
            info!(i = i + 1, "Narration segment prepared");
        }

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

#[derive(Debug, Default)]
pub struct VerifyQuality;

#[async_trait]
impl Action for VerifyQuality {
    fn name(&self) -> &str {
        "verify_quality"
    }
    fn preconditions(&self) -> WorldState {
        WorldState {
            radio_play_assembled: true,
            ..WorldState::default()
        }
    }
    fn effects(&self) -> WorldState {
        WorldState {
            quality_verified: true,
            ..WorldState::default()
        }
    }
    fn cost(&self, _state: &WorldState) -> f32 {
        2.0
    }

    async fn execute(&self, _ctx: &mut PipelineContext) -> Result<()> {
        info!("Quality verification (placeholder)");
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct ApplyLearnings;

#[async_trait]
impl Action for ApplyLearnings {
    fn name(&self) -> &str {
        "apply_learnings"
    }
    fn preconditions(&self) -> WorldState {
        WorldState {
            quality_verified: true,
            ..WorldState::default()
        }
    }
    fn effects(&self) -> WorldState {
        WorldState {
            learnings_applied: true,
            ..WorldState::default()
        }
    }
    fn cost(&self, _state: &WorldState) -> f32 {
        0.5
    }

    async fn execute(&self, _ctx: &mut PipelineContext) -> Result<()> {
        info!("Applying learnings (placeholder)");
        Ok(())
    }
}

pub fn get_all_actions() -> Vec<Box<dyn Action>> {
    vec![
        Box::new(DecodeMovie),
        Box::new(ExtractTimeline),
        Box::new(IdentifyVisualGaps),
        Box::new(GenerateNarration),
        Box::new(SynthesizeNarrator),
        Box::new(AssembleRadioPlay),
        Box::new(VerifyQuality),
        Box::new(ApplyLearnings),
    ]
}
