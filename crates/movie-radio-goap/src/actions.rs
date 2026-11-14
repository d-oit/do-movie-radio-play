use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::info;

mod assemble_action;
mod retag;
mod synthesize;
mod verify;

pub use assemble_action::AssembleRadioPlay;
pub use synthesize::SynthesizeNarrator;
pub use verify::{ApplyLearnings, VerifyQuality};

use crate::gaps::{split_gap_windows, subtract_cues, GapIdentifier, MAX_NARRATION_WINDOW_MS};
use crate::narrate::NarrationGenerator;
use crate::{Action, PipelineContext, WorldState};
use movie_radio_pipeline::pipeline::chunked::extract_timeline_from_raw_f32;
use movie_radio_pipeline::pipeline::extract_timeline;
use retag::{retag_windows, tag_from_source};

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
        let audio = crate::OriginalAudio::decode(&ctx.movie_path, ctx.sample_rate)?;
        info!(samples = audio.len(), "Movie decoded to spool file");
        ctx.original_audio = Some(audio);
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
                extract_timeline_from_raw_f32(original.path(), &ctx.movie_path, &ctx.config)?;
            // Untagged segments never clear the gap-confidence threshold, so
            // without this the narrator silently emits nothing.
            tag_from_source(original, ctx.sample_rate, &mut timeline)?;
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
        if let Some(source) = ctx.original_audio.as_ref() {
            retag_windows(&mut windows, source, ctx.sample_rate, &timeline.file)?;
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
        info!("Generating narration scripts");
        let language = ctx.voice_config.as_ref().map_or_else(
            || movie_radio_voice::config::VoiceSynthesisConfig::from_env().language,
            |cfg| cfg.language.clone(),
        );
        let generator = NarrationGenerator::default().with_language(&language);
        let narrator_config = ctx.visual.narrator_config.clone().unwrap_or_default();
        let outcome = {
            let timeline = ctx.timeline.as_ref().context("Timeline not extracted")?;
            let gaps = &ctx
                .gap_analysis
                .as_ref()
                .context("Gaps not identified")?
                .gaps;
            crate::narrate::stage::generate(
                &generator,
                &narrator_config,
                &ctx.visual.known_characters,
                &ctx.movie_path,
                timeline,
                gaps,
            )
            .await?
        };
        if let Some(report) = &outcome.visual_report {
            let path = report_path(&ctx.output_path, ".visual-review.json");
            write_report(&path, report)?;
            info!(
                path = %path.display(),
                visual = report.visual_windows,
                audio_only = report.audio_only_windows,
                "Visual narration review written"
            );
        }
        if let Some(report) = &outcome.llm_report {
            let path = report_path(&ctx.output_path, ".llm-narration.json");
            write_report(&path, report)?;
            info!(
                path = %path.display(),
                accepted = report.accepted,
                rejected = report.rejected,
                fallback = report.fallback,
                "LLM narration review written"
            );
        }
        info!(
            scripts = outcome.scripts.len(),
            "Narration scripts generated"
        );
        ctx.scripts = Some(outcome.scripts);
        ctx.visual.report = outcome.visual_report;
        Ok(())
    }
}

fn report_path(output: &std::path::Path, suffix: &str) -> std::path::PathBuf {
    let mut path = output.as_os_str().to_os_string();
    path.push(suffix);
    std::path::PathBuf::from(path)
}

fn write_report<T: serde::Serialize>(path: &std::path::Path, report: &T) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let json = serde_json::to_vec_pretty(report).context("serialize visual narration report")?;
    std::fs::write(path, json).with_context(|| format!("cannot write {}", path.display()))
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
    use crate::narrate::NarrationEvidence;
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
            evidence: NarrationEvidence::default(),
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
    async fn test_generate_narration_without_vision_keeps_audio_only_evidence() {
        use movie_radio_types::{
            GapAnalysisOutput, Segment, SegmentKind, TimelineOutput, VisualGap,
        };

        let dir = tempfile::tempdir().expect("tempdir");
        let output = dir.path().join("out.wav");
        let mut ctx =
            crate::PipelineContext::new(std::path::PathBuf::from("movie.mkv"), output.clone());
        ctx.timeline = Some(TimelineOutput {
            file: "movie.mkv".to_string(),
            analysis_sample_rate: 16_000,
            frame_ms: 20,
            segments: vec![Segment {
                start_ms: 0,
                end_ms: 6_000,
                kind: SegmentKind::NonVoice,
                confidence: 0.9,
                tags: vec!["ambience".to_string()],
                prompt: None,
                sfx_trigger: None,
            }],
        });
        ctx.gap_analysis = Some(GapAnalysisOutput {
            file: "movie.mkv".to_string(),
            gaps: vec![VisualGap {
                start_ms: 0,
                end_ms: 6_000,
                confidence: 0.9,
                reason: "long pause".to_string(),
                priority: 1,
                tags: vec!["ambience".to_string()],
                trend: None,
            }],
        });

        GenerateNarration
            .execute(&mut ctx)
            .await
            .expect("audio-only narration");

        let scripts = ctx.scripts.as_ref().expect("scripts");
        assert!(!scripts.is_empty());
        assert!(scripts.iter().all(|s| matches!(
            &s.evidence,
            crate::narrate::NarrationEvidence::AudioOnly { .. }
        )));
        assert!(ctx.visual.report.is_none());
        let report = std::path::PathBuf::from(format!("{}.visual-review.json", output.display()));
        assert!(!report.exists());
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
        // Failed attempt rolls partial entries back so retry cannot leave stale Nones.
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

    #[tokio::test]
    async fn test_synthesize_narrator_passes_voice_reference() {
        let mut ctx = crate::PipelineContext::new(
            std::path::PathBuf::from("movie.mp4"),
            std::path::PathBuf::from("out.wav"),
        );
        let ref_path = std::path::PathBuf::from("voice_samples/alice.wav");
        ctx.voice_reference = Some(ref_path.clone());
        ctx.scripts = Some(vec![script(100)]);
        const MODAL_TTS_ENDPOINT_ENV: &str = "MODAL_TTS_ENDPOINT";
        std::env::remove_var(MODAL_TTS_ENDPOINT_ENV);
        let result = SynthesizeNarrator.execute(&mut ctx).await;
        let err = result.expect_err("synthesis failure expected without endpoint");
        assert!(err.to_string().contains("all 1 narration syntheses failed"));
        assert_eq!(ctx.voice_reference, Some(ref_path));
    }
}

#[cfg(test)]
mod wiring_tests;
