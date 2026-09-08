use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::info;

use crate::gaps::GapIdentifier;
use crate::narrate::NarrationGenerator;
use crate::{Action, PipelineContext, WorldState};
use movie_radio_learning::adaptive_thresholds::{
    adjust_thresholds_for_fp_rate, create_learning_state, load_learning_state,
    record_verification_result, save_learning_state,
};
use movie_radio_learning::database::LearningDb;
use movie_radio_pipeline::pipeline::decode::decode_audio;
use movie_radio_pipeline::pipeline::extract_timeline;
use movie_radio_verification::verify_timeline;

/// Six optional spectral thresholds passed to `verify_timeline`.
type ThresholdOptions = (
    Option<f32>,
    Option<f32>,
    Option<f32>,
    Option<f32>,
    Option<f32>,
    Option<f32>,
);

fn threshold_tuple(
    t: &movie_radio_learning::adaptive_thresholds::AdaptiveThresholds,
) -> ThresholdOptions {
    (
        Some(t.entropy_min),
        Some(t.entropy_max),
        Some(t.flatness_max),
        Some(t.energy_min),
        Some(t.centroid_min),
        Some(t.centroid_max),
    )
}

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
        // Keep the pre-action length so a failed attempt can roll its partial
        // `None`/audio entries back: the orchestrator retries failed actions and
        // AssembleRadioPlay zips scripts against this vector from its start.
        let narration_baseline = ctx.narration_audio.len();

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

            if let Err(e) = request.validate() {
                tracing::warn!(i = i + 1, error = %e, "Invalid synthesis request, skipping");
                ctx.narration_audio.push(None);
                continue;
            }

            let cap = provider.capabilities().max_text_length;
            if script.text.chars().count() > cap {
                tracing::warn!(
                    i = i + 1,
                    cap,
                    chars = script.text.chars().count(),
                    "Text exceeds provider cap, skipping"
                );
                ctx.narration_audio.push(None);
                continue;
            }

            match provider.synthesize(&request).await {
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

    async fn execute(&self, ctx: &mut PipelineContext) -> Result<()> {
        let timeline = ctx.timeline.as_ref().context("Timeline not extracted")?;
        if !ctx.movie_path.exists() {
            anyhow::bail!(
                "cannot verify quality: media file not found: {}",
                ctx.movie_path.display()
            );
        }

        // Reuse persisted adaptive thresholds so the learning loop closes:
        // a previous apply_learnings run adjusts what this run verifies with.
        let (entropy_min, entropy_max, flatness_max, energy_min, centroid_min, centroid_max) =
            match &ctx.learning_state_path {
                Some(path) if path.exists() => {
                    let state = load_learning_state(path)?;
                    let t = &state.current_thresholds;
                    threshold_tuple(t)
                }
                _ => match &ctx.learning {
                    Some(t) => threshold_tuple(t),
                    None => (None, None, None, None, None, None),
                },
            };

        // The report JSON is an intermediate artifact: verify into a temp
        // file and keep the typed report in the context for downstream
        // actions (apply_learnings) and replanning decisions.
        let report_path = tempfile::NamedTempFile::new()
            .context("failed to create temporary verification report path")?;
        let report = verify_timeline(
            &ctx.movie_path,
            timeline,
            report_path.path(),
            entropy_min,
            entropy_max,
            flatness_max,
            energy_min,
            centroid_min,
            centroid_max,
            false,
            10,
            None,
        )
        .context("quality verification failed")?;

        let summary = &report.summary;
        info!(
            total = summary.total_segments,
            verified = summary.verified_count,
            suspicious = summary.suspicious_count,
            rejected = summary.rejected_count,
            avg_confidence = format!("{:.3}", summary.average_confidence),
            "Quality verification complete"
        );
        if crate::verification_looks_suspicious(&report) {
            tracing::warn!(
                suspicious = summary.suspicious_count,
                rejected = summary.rejected_count,
                verified = summary.verified_count,
                "Most non-voice segments were not verified; thresholds likely need                  recalibration (apply_learnings)"
            );
        }
        self.check_assembled_output(ctx)?;
        ctx.verification = Some(report);
        Ok(())
    }
}

impl VerifyQuality {
    /// Verify the assembled radio play file itself: it must exist, decode
    /// cleanly, and (when the original audio is in context) stay within the
    /// roadmap's 10% duration tolerance of the original.
    fn check_assembled_output(&self, ctx: &PipelineContext) -> Result<()> {
        if !ctx.output_path.exists() {
            anyhow::bail!("assembled output missing: {}", ctx.output_path.display());
        }
        let (output, output_rate) =
            decode_audio(&ctx.output_path, ctx.sample_rate).with_context(|| {
                format!(
                    "assembled output failed to decode: {}",
                    ctx.output_path.display()
                )
            })?;
        if output.is_empty() {
            anyhow::bail!("assembled output decodes to zero samples");
        }
        if let Some(original) = &ctx.original_audio {
            let original_duration = original.len() as f64 / f64::from(ctx.sample_rate);
            let output_duration = output.len() as f64 / f64::from(output_rate);
            if !durations_within_tolerance(original_duration, output_duration, 0.10) {
                anyhow::bail!(
                    "assembled output duration {output_duration:.2}s deviates from original {original_duration:.2}s by more than 10%"
                );
            }
        }
        tracing::info!("Assembled output verified (decode + duration)");
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

    async fn execute(&self, ctx: &mut PipelineContext) -> Result<()> {
        let report = ctx
            .verification
            .as_ref()
            .context("no verification report available: run verify_quality first")?;

        // Feed every non-voice segment verdict into the adaptive-threshold
        // learning state. A segment verification flagged as suspicious or
        // rejected means the extractor cut a likely speech segment as
        // non-voice - a false positive from the learning loop's viewpoint.
        let mut state = match &ctx.learning_state_path {
            Some(path) if path.exists() => load_learning_state(path)?,
            _ => create_learning_state(20),
        };
        for (i, result) in report.segment_results.iter().enumerate() {
            let feats = &result.spectral_features;
            record_verification_result(
                &mut state,
                i,
                !result.is_verified,
                feats.spectral_entropy,
                feats.spectral_flatness,
                feats.rms,
                feats.centroid_hz,
            );
        }

        // Bounded per-run adjustment: the learning-rate-limited updates in
        // movie-radio-learning keep each parameter move small and clamped.
        if state.total_verifications >= 5 {
            adjust_thresholds_for_fp_rate(&mut state);
        }

        // Persist the state file first: it is the durable record. The
        // threshold-history database write is best-effort telemetry, so a
        // failure there cannot fail the action (which would otherwise retry
        // and re-record the same rows or duplicate state history).
        if let Some(path) = &ctx.learning_state_path {
            save_learning_state(&state, path)?;
        } else {
            tracing::info!(
                "no learning_state_path configured: adjusted thresholds kept in memory only"
            );
        }

        if let Some(db_path) = &ctx.learning_db_path {
            let t = &state.current_thresholds;
            match LearningDb::new(db_path).await {
                Ok(db) => {
                    if let Err(err) = db
                        .record_threshold(
                            f64::from(t.flatness_max),
                            f64::from(t.entropy_min),
                            f64::from(t.centroid_min),
                            f64::from(t.centroid_max),
                        )
                        .await
                    {
                        tracing::warn!(
                            error = %err,
                            "failed to record thresholds in learning database"
                        );
                    }
                }
                Err(err) => {
                    tracing::warn!(error = %err, "failed to open learning database");
                }
            }
        }

        let state_path = ctx
            .learning_state_path
            .as_ref()
            .map_or_else(|| "<memory>".to_string(), |p| p.display().to_string());
        info!(
            verifications = state.total_verifications,
            fp_rate = format!("{:.2}%", state.recent_fp_rate * 100.0),
            flatness_max = state.current_thresholds.flatness_max,
            entropy_min = state.current_thresholds.entropy_min,
            state_path = state_path,
            "Applying learnings complete"
        );
        ctx.learning = Some(state.current_thresholds.clone());
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

fn build_narration_segments(
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
        std::env::remove_var("MODAL_TTS_ENDPOINT");
        let mut ctx = crate::PipelineContext::new(
            std::path::PathBuf::from("movie.mp4"),
            std::path::PathBuf::from("/tmp/opencode/out.wav"),
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
}

/// True when `output` stays within `tolerance` (fraction) of `original`.
fn durations_within_tolerance(original: f64, output: f64, tolerance: f64) -> bool {
    if original <= 0.0 {
        return output > 0.0;
    }
    let ratio = output / original;
    (1.0 - tolerance..=1.0 + tolerance).contains(&ratio)
}

#[cfg(test)]
mod wiring_tests {
    use super::*;
    use crate::actions::{ApplyLearnings, VerifyQuality};
    use crate::test_support::{empty_timeline, suspicious_report};
    use std::path::PathBuf;

    #[tokio::test]
    async fn verify_quality_requires_timeline() {
        let mut ctx = PipelineContext::new(PathBuf::from("movie.mkv"), PathBuf::from("out.wav"));
        let err = VerifyQuality.execute(&mut ctx).await.unwrap_err();
        assert!(err.to_string().contains("Timeline not extracted"), "{err}");
    }

    #[tokio::test]
    async fn verify_quality_bails_when_media_missing() {
        let mut ctx = PipelineContext::new(
            PathBuf::from("/nonexistent/movie.mkv"),
            PathBuf::from("out.wav"),
        );
        ctx.timeline = Some(empty_timeline());
        let err = VerifyQuality.execute(&mut ctx).await.unwrap_err();
        assert!(err.to_string().contains("cannot verify quality"), "{err}");
    }

    #[tokio::test]
    async fn apply_learnings_requires_verification_first() {
        let mut ctx = PipelineContext::new(PathBuf::from("movie.mkv"), PathBuf::from("out.wav"));
        let err = ApplyLearnings.execute(&mut ctx).await.unwrap_err();
        assert!(err.to_string().contains("no verification report"), "{err}");
    }

    #[tokio::test]
    async fn apply_learnings_records_and_persists_state() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state_path = dir.path().join("learning-state.json");
        let db_path = dir.path().join("learn.db");
        let mut ctx = PipelineContext::new(PathBuf::from("movie.mkv"), PathBuf::from("out.wav"));
        ctx.verification = Some(suspicious_report(3));
        ctx.learning_state_path = Some(state_path.clone());
        ctx.learning_db_path = Some(db_path.clone());

        ApplyLearnings
            .execute(&mut ctx)
            .await
            .expect("apply learnings");

        assert!(ctx.learning.is_some(), "thresholds must be exposed on ctx");
        let state = movie_radio_learning::adaptive_thresholds::load_learning_state(&state_path)
            .expect("learning state persisted");
        assert_eq!(state.total_verifications, 3);
        assert!(state.total_false_positives > 0);
        assert!(
            db_path.exists(),
            "learning db must be created when configured"
        );
    }
}
