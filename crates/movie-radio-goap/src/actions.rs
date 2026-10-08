use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::info;

mod assemble_action;
mod verify;

pub use assemble_action::AssembleRadioPlay;
pub use verify::{ApplyLearnings, VerifyQuality};

use crate::gaps::{split_gap_windows, subtract_cues, GapIdentifier, MAX_NARRATION_WINDOW_MS};
use crate::narrate::NarrationGenerator;
use crate::{Action, PipelineContext, WorldState};
use movie_radio_pipeline::pipeline::decode::decode_audio;
use movie_radio_pipeline::pipeline::tags::add_tags_from_samples;
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
            let mut timeline =
                extract_timeline_from_samples_with_path(original, &ctx.movie_path, &ctx.config)?;
            // Untagged segments never clear the gap-confidence threshold, so
            // without this the narrator silently emits nothing.
            add_tags_from_samples(original, ctx.sample_rate, &mut timeline, None);
            timeline
        } else {
            extract_timeline(&ctx.movie_path, &ctx.config)?
        };
        info!(segments = timeline.segments.len(), "Timeline extracted");
        ctx.timeline = Some(timeline);
        Ok(())
    }
}

/// Safety margin around each subtitle cue, and the shortest stretch worth narrating.
const SUBTITLE_PAD_MS: u64 = 300;
/// Speech-to-text timings are rougher than authored subtitles.
const DERIVED_CUE_PAD_MS: u64 = 800;
const MIN_WINDOW_MS: u64 = 1_000;

/// Re-derives each window's tags from its own audio so a long scene is
/// described by what is audible in that stretch, not by the whole segment.
fn retag_windows(
    windows: &mut [movie_radio_types::VisualGap],
    samples: &[f32],
    sr: u32,
    file: &str,
) {
    use movie_radio_types::{Segment, SegmentKind, TimelineOutput};
    let mut tl = TimelineOutput {
        file: file.to_string(),
        analysis_sample_rate: sr,
        frame_ms: 20,
        segments: windows
            .iter()
            .map(|w| Segment {
                start_ms: w.start_ms,
                end_ms: w.end_ms,
                kind: SegmentKind::NonVoice,
                confidence: w.confidence,
                tags: Vec::new(),
                prompt: None,
                sfx_trigger: None,
            })
            .collect(),
    };
    add_tags_from_samples(samples, sr, &mut tl, None);
    for (w, seg) in windows.iter_mut().zip(tl.segments) {
        if !seg.tags.is_empty() {
            w.tags = seg.tags;
        }
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
        let mut identifier = GapIdentifier::new();
        if let Some(threshold) = ctx.gap_confidence {
            identifier.high_confidence_threshold = threshold.clamp(0.0, 1.0);
        }
        let gap_analysis = identifier.identify_gaps(timeline, srt_content.as_deref())?;
        let pad_ms = if ctx.subtitles_derived {
            DERIVED_CUE_PAD_MS
        } else {
            SUBTITLE_PAD_MS
        };
        let clear_gaps = match srt_content.as_deref() {
            Some(srt) => {
                let cues: Vec<(u64, u64)> = movie_radio_validation::srt::parse_srt_segments(srt)?
                    .iter()
                    .map(|c| (c.start_ms, c.end_ms))
                    .collect();
                let kept = subtract_cues(&gap_analysis.gaps, &cues, pad_ms, MIN_WINDOW_MS);
                info!(
                    cues = cues.len(),
                    before = gap_analysis.gaps.len(),
                    after = kept.len(),
                    "gaps clipped against subtitle cues"
                );
                kept
            }
            None => gap_analysis.gaps.clone(),
        };
        let mut windows = split_gap_windows(&clear_gaps, MAX_NARRATION_WINDOW_MS);
        if let Some(samples) = ctx.original_audio.as_deref() {
            retag_windows(&mut windows, samples, ctx.sample_rate, &timeline.file);
        }
        let gap_analysis = movie_radio_types::GapAnalysisOutput {
            gaps: windows,
            ..gap_analysis
        };
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
        let language = ctx.voice_config.as_ref().map_or_else(
            || movie_radio_voice::config::VoiceSynthesisConfig::from_env().language,
            |cfg| cfg.language.clone(),
        );
        let generator = NarrationGenerator::default().with_language(&language);
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
        let emotion_mapping = voice_cfg.emotion_mapping;
        let orchestrator = SynthesisOrchestrator::new(voice_cfg);

        for (i, script) in scripts.iter().enumerate() {
            let emotion = if emotion_mapping {
                script.emotion.clone()
            } else {
                movie_radio_voice::Emotion::Neutral
            };
            info!(
                i = i + 1,
                total = scripts.len(),
                text = %script.text,
                emotion = ?emotion,
                "Synthesizing narration"
            );

            let request = SynthesisRequest {
                text: script.text.clone(),
                emotion,
                voice_id: voice_id.clone(),
                reference_audio: ctx.voice_reference.clone(),
                language: language.clone(),
                // Base speed only: providers resolve `emotion` into their
                // own speed/stability levers, so pre-scaling here would
                // square the tempo factor.
                speed: 1.0,
                sample_rate_hz: ctx.sample_rate,
            };

            if let Err(e) = request.validate() {
                tracing::warn!(i = i + 1, error = %e, "Invalid synthesis request, skipping");
                ctx.narration_audio.push(None);
                ctx.narration_provider.push(None);
                continue;
            }

            match orchestrator.synthesize_with_provider(&request).await {
                Ok((audio, provider_id)) => {
                    info!(
                        i = i + 1,
                        samples = audio.samples.len(),
                        "Narration synthesized"
                    );
                    ctx.narration_audio.push(Some(audio));
                    ctx.narration_provider.push(Some(provider_id));
                }
                Err(e) => {
                    tracing::warn!(i = i + 1, error = %e, "TTS failed, skipping");
                    ctx.narration_audio.push(None);
                    ctx.narration_provider.push(None);
                }
            }
        }

        if !scripts.is_empty() && ctx.narration_audio.iter().all(Option::is_none) {
            ctx.narration_audio.truncate(narration_baseline);
            ctx.narration_provider.truncate(narration_baseline);
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
        ctx.voice_config = Some(movie_radio_voice::VoiceSynthesisConfig {
            language: "en".to_string(),
            voice_id: Some("narrator-custom".to_string()),
            fallback_chain: vec!["modal".to_string()],
            ..movie_radio_voice::VoiceSynthesisConfig::default()
        });
        ctx.scripts = Some(vec![script(100)]);

        std::env::remove_var("MODAL_TTS_ENDPOINT");
        let result = SynthesizeNarrator.execute(&mut ctx).await;
        let err = result.expect_err("synthesis failure expected without endpoint");
        assert!(err.to_string().contains("all 1 narration syntheses failed"));
    }

    #[tokio::test]
    async fn test_synthesize_narrator_passes_voice_reference() {
        let mut ctx = crate::PipelineContext::new(
            std::path::PathBuf::from("movie.mp4"),
            std::path::PathBuf::from("out.wav"),
        );
        let ref_path = std::path::PathBuf::from("voice_samples/alice.wav");
        ctx.voice_reference = Some(ref_path.clone());
        ctx.scripts = Some(vec![script(100)]);

        std::env::remove_var("MODAL_TTS_ENDPOINT");
        let result = SynthesizeNarrator.execute(&mut ctx).await;
        let err = result.expect_err("synthesis failure expected without endpoint");
        assert!(err.to_string().contains("all 1 narration syntheses failed"));
        assert_eq!(ctx.voice_reference, Some(ref_path));
    }
}

#[cfg(test)]
mod wiring_tests;
