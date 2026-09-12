use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::info;

mod assemble_action;
mod verify;

pub use assemble_action::AssembleRadioPlay;
pub use verify::{ApplyLearnings, VerifyQuality};

use crate::gaps::GapIdentifier;
use crate::narrate::NarrationGenerator;
use crate::{Action, PipelineContext, WorldState};
use movie_radio_pipeline::pipeline::decode::decode_audio;
use movie_radio_pipeline::pipeline::{extract_timeline, extract_timeline_from_samples_with_path};

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
        if ctx.original_audio.is_some() {
            info!("Original audio already present in context");
            return Ok(());
        }
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
        if ctx.timeline.is_some() {
            info!("Timeline already present in context");
            return Ok(());
        }
        info!("Extracting audio timeline");
        let timeline = if let Some(ref original) = ctx.original_audio {
            extract_timeline_from_samples_with_path(original, &ctx.movie_path, &ctx.config)?
        } else {
            extract_timeline(&ctx.movie_path, &ctx.config)?
        };
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
        use movie_radio_voice::voice::{SynthesisOrchestrator, SynthesisRequest};

        let scripts = ctx.scripts.as_ref().context("Scripts not generated")?;
        // Keep the pre-action length so a failed attempt can roll its partial
        // `None`/audio entries back: the orchestrator retries failed actions and
        // AssembleRadioPlay zips scripts against this vector from its start.
        let narration_baseline = ctx.narration_audio.len();

        let voice_cfg = ctx
            .voice_config
            .clone()
            .unwrap_or_else(movie_radio_voice::config::VoiceSynthesisConfig::from_env);

        let language = voice_cfg.language.clone();
        let voice_id = voice_cfg.voice_id.clone();
        let orchestrator = SynthesisOrchestrator::new(voice_cfg);

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
                voice_id: voice_id.clone(),
                language: language.clone(),
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
            ctx.narration_audio.truncate(narration_baseline);
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

#[cfg(test)]
fn build_narration_segments(
    scripts: &[crate::narrate::NarrationScript],
    narration_audio: &[Option<movie_radio_voice::AudioOutput>],
    assembler: &crate::assemble::RadioPlayAssembler,
) -> Vec<crate::assemble::NarrationSegment> {
    assembler.build_narration_segments(scripts, narration_audio)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::narrate::NarrationScript;
    use movie_radio_voice::Emotion;

    fn script(gap_start_ms: u64) -> NarrationScript {
        NarrationScript {
            gap_start_ms,
            gap_end_ms: gap_start_ms + 5_000,
            text: "Hallo Welt".to_string(),
            emotion: Emotion::Neutral,
            word_count: 2,
            duration_ms: 1_000,
        }
    }

    fn audio(len_samples: usize) -> Option<movie_radio_voice::AudioOutput> {
        Some(movie_radio_voice::AudioOutput {
            samples: vec![0.25; len_samples],
            sample_rate_hz: 16_000,
        })
    }

    #[test]
    fn test_segments_keep_script_alignment_across_failures() {
        let scripts = vec![script(1_000), script(2_000), script(3_000)];
        let narration = vec![audio(800), None, audio(1_600)];
        let assembler = crate::assemble::RadioPlayAssembler::new(16_000, 50, 0.3);

        let segments = build_narration_segments(&scripts, &narration, &assembler);

        assert_eq!(segments.len(), 2);
        // First surviving segment belongs to script 0, not shifted by the skip.
        assert_eq!(segments[0].start_sample, 16_000);
        assert_eq!(segments[0].samples.len(), 800);
        // Second surviving segment must pair with script 2 (3_000 ms -> 48_000).
        assert_eq!(segments[1].start_sample, 48_000);
        assert_eq!(segments[1].samples.len(), 1_600);
    }

    #[tokio::test]
    async fn test_synthesize_narrator_bails_when_all_fail() {
        const MODAL_TTS_ENDPOINT_ENV: &str = "MODAL_TTS_ENDPOINT";
        std::env::remove_var(MODAL_TTS_ENDPOINT_ENV);
        let mut ctx = crate::PipelineContext::new(
            std::path::PathBuf::from("movie.mp4"),
            std::path::PathBuf::from("out.wav"),
        );
        ctx.scripts = Some(vec![script(500), script(6_000)]);

        let result = SynthesizeNarrator.execute(&mut ctx).await;

        let err = result.expect_err("total synthesis failure must not pass silently");
        assert!(err.to_string().contains("all 2 narration syntheses failed"));
        // The failed attempt rolls its partial entries back so a retry cannot
        // leave stale Nones that would misalign assemble's zip.
        assert!(
            ctx.narration_audio.is_empty(),
            "failed synthesis must roll back its partial entries"
        );
    }

    #[tokio::test]
    async fn test_synthesize_narrator_uses_custom_voice_config() {
        let mut ctx = crate::PipelineContext::new(
            std::path::PathBuf::from("movie.mp4"),
            std::path::PathBuf::from("out.wav"),
        );
        let voice_cfg = movie_radio_voice::VoiceSynthesisConfig {
            language: "en".to_string(),
            voice_id: Some("narrator-custom".to_string()),
            fallback_chain: vec!["modal".to_string()],
            ..movie_radio_voice::VoiceSynthesisConfig::default()
        };
        ctx.voice_config = Some(voice_cfg);
        ctx.scripts = Some(vec![script(100)]);

        const MODAL_TTS_ENDPOINT_ENV: &str = "MODAL_TTS_ENDPOINT";
        std::env::remove_var(MODAL_TTS_ENDPOINT_ENV);
        let result = SynthesizeNarrator.execute(&mut ctx).await;
        let err = result.expect_err("synthesis failure expected without endpoint");
        assert!(err.to_string().contains("all 1 narration syntheses failed"));
    }
}

#[cfg(test)]
mod wiring_tests;
