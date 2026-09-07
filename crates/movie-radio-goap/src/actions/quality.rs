use anyhow::{Context, Result};
use std::path::PathBuf;
use tracing::{info, warn};

use crate::PipelineContext;
use movie_radio_verification::verification::verify_timeline;

pub async fn execute_verify_quality(ctx: &mut PipelineContext) -> Result<()> {
    let timeline = ctx.timeline.as_ref().context("Timeline not extracted")?;

    let media_path = if ctx.movie_path.exists() {
        ctx.movie_path.clone()
    } else if ctx.output_path.exists() {
        ctx.output_path.clone()
    } else {
        ctx.movie_path.clone()
    };

    let report_path = ctx
        .verification_report_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("analysis/verification_report.json"));

    info!(
        media = %media_path.display(),
        report_path = %report_path.display(),
        "Running spectral quality verification and fingerprint scoring"
    );

    let media_path_clone = media_path.clone();
    let timeline_clone = timeline.clone();
    let report_path_clone = report_path.clone();
    let db_path_clone = ctx.learning_db_path.clone();

    let report = tokio::task::spawn_blocking(move || {
        verify_timeline(
            &media_path_clone,
            &timeline_clone,
            &report_path_clone,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            10,
            db_path_clone,
        )
    })
    .await??;

    info!(
        verified = report.summary.verified_count,
        suspicious = report.summary.suspicious_count,
        rejected = report.summary.rejected_count,
        fp_rate = format!("{:.2}%", report.summary.false_positive_rate * 100.0),
        "Quality verification complete"
    );

    if report.summary.false_positive_rate > 0.5
        || (report.summary.total_segments > 0 && report.summary.verified_count == 0)
    {
        warn!(
            fp_rate = report.summary.false_positive_rate,
            verified = report.summary.verified_count,
            "Quality verification failed threshold; requesting replan"
        );
        ctx.replan_requested = true;
    }

    ctx.verification_report = Some(report);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use movie_radio_types::{Segment, SegmentKind, TimelineOutput};
    use tempfile::NamedTempFile;

    fn generate_test_wav() -> NamedTempFile {
        let file = NamedTempFile::new().unwrap();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::new(file.reopen().unwrap(), spec).unwrap();
        for i in 0..16_000 {
            let sample = ((i as f32 * 0.05).sin() * 10000.0) as i16;
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
        file
    }

    #[tokio::test]
    async fn test_verify_quality_execution() {
        let wav = generate_test_wav();
        let wav_path = wav.path().to_path_buf();
        let report_file = NamedTempFile::new().unwrap();
        let report_path = report_file.path().to_path_buf();

        let timeline = TimelineOutput {
            file: wav_path.display().to_string(),
            analysis_sample_rate: 16_000,
            frame_ms: 10,
            segments: vec![Segment {
                start_ms: 0,
                end_ms: 1000,
                kind: SegmentKind::NonVoice,
                confidence: 0.9,
                tags: vec![],
                prompt: None,
                sfx_trigger: None,
            }],
        };

        let mut ctx = PipelineContext::new(wav_path.clone(), wav_path.clone());
        ctx.timeline = Some(timeline);
        ctx.verification_report_path = Some(report_path);

        let result = execute_verify_quality(&mut ctx).await;
        assert!(result.is_ok());
        assert!(ctx.verification_report.is_some());
        let rep = ctx.verification_report.unwrap();
        assert_eq!(rep.summary.total_segments, 1);
    }
}
