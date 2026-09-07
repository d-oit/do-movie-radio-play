use anyhow::{Context, Result};
use std::path::PathBuf;
use tracing::info;

use crate::PipelineContext;

pub async fn execute_apply_learnings(ctx: &mut PipelineContext) -> Result<()> {
    info!("Applying learnings and persisting execution trace");

    // 1. Calibration profile updates
    let cal_path = ctx
        .calibration_report_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("analysis/learnings/latest-calibration.json"));

    if cal_path.exists() {
        let out_profile = ctx
            .updated_profile_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("analysis/profiles/updated-profile.json"));
        movie_radio_learning::calibrator::apply_calibration_report(&cal_path, &out_profile)?;
        info!(
            cal_report = %cal_path.display(),
            out_profile = %out_profile.display(),
            "Applied calibration report update"
        );
    }

    // 2. Adaptive threshold updates
    let state_path = ctx
        .learning_state_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("analysis/thresholds/learning-state.json"));
    let mut state = movie_radio_learning::adaptive_thresholds::load_learning_state(&state_path)
        .unwrap_or_else(|_| movie_radio_learning::adaptive_thresholds::create_learning_state(20));

    if let Some(ref report) = ctx.verification_report {
        for (i, result) in report.segment_results.iter().enumerate() {
            movie_radio_learning::adaptive_thresholds::record_verification_result(
                &mut state,
                i,
                result.is_suspicious,
                result.spectral_features.spectral_entropy,
                result.spectral_features.spectral_flatness,
                result.spectral_features.rms,
                result.spectral_features.centroid_hz,
            );
        }
        movie_radio_learning::adaptive_thresholds::adjust_thresholds_for_fp_rate(&mut state);
        movie_radio_learning::adaptive_thresholds::save_learning_state(&state, &state_path)?;
    }

    // 3. LearningDb run trace persistence
    let db_path = ctx
        .learning_db_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("analysis/thresholds/learning.db"));

    let report_opt = ctx.verification_report.clone();
    let db_path_clone = db_path.clone();

    tokio::task::spawn_blocking(move || {
        if let Some(parent) = db_path_clone.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("Failed to create runtime for LearningDb interaction")?;

        let db = rt.block_on(movie_radio_learning::database::LearningDb::new(
            &db_path_clone,
        ))?;

        let run_id = format!(
            "goap_run_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        );

        let _exp_id = rt
            .block_on(db.record_experiment(&run_id, "GOAP Execution", Some("Pipeline run trace")))
            .context("Failed to record GOAP experiment run trace")?;

        if let Some(ref report) = report_opt {
            for (i, result) in report.segment_results.iter().enumerate() {
                let segment = movie_radio_learning::database::VerifiedSegment {
                    start_ms: result.start_ms as i64,
                    end_ms: result.end_ms as i64,
                    confidence: result.original_confidence as f64,
                    spectral_features: movie_radio_learning::database::SpectralFeatures {
                        rms: result.spectral_features.rms as f64,
                        zcr: result.spectral_features.zcr as f64,
                        spectral_flux: result.spectral_features.spectral_flux as f64,
                        spectral_flatness: result.spectral_features.spectral_flatness as f64,
                        spectral_entropy: result.spectral_features.spectral_entropy as f64,
                        centroid_hz: result.spectral_features.centroid_hz as f64,
                        low_band_ratio: result.spectral_features.low_band_ratio as f64,
                        high_band_ratio: result.spectral_features.high_band_ratio as f64,
                    },
                    was_false_positive: result.is_suspicious,
                };

                let seg_id = rt.block_on(db.record_verification(segment))?;

                if let Some(fps) = report.segment_fingerprints.get(i) {
                    rt.block_on(db.record_fingerprints(seg_id, fps))?;
                }
            }
        }

        info!(
            db_path = %db_path_clone.display(),
            run_id = %run_id,
            "Learnings and execution trace persisted to LearningDb"
        );

        Ok::<(), anyhow::Error>(())
    })
    .await??;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use movie_radio_types::TimelineOutput;
    use movie_radio_verification::verification::{
        AppliedThresholds, SegmentVerification, SpectralFeatures, VerificationReport,
        VerificationStatus, VerificationSummary,
    };
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_apply_learnings_persists_to_db() {
        let db_file = NamedTempFile::new().unwrap();
        let db_path = db_file.path().to_path_buf();

        let state_file = NamedTempFile::new().unwrap();
        let state_path = state_file.path().to_path_buf();

        let mut ctx = PipelineContext::new(PathBuf::from("movie.mp4"), PathBuf::from("out.wav"));
        ctx.learning_db_path = Some(db_path.clone());
        ctx.learning_state_path = Some(state_path.clone());

        let report = VerificationReport {
            verified_timeline: TimelineOutput {
                file: "test.wav".to_string(),
                analysis_sample_rate: 16_000,
                frame_ms: 10,
                segments: vec![],
            },
            segment_results: vec![SegmentVerification {
                start_ms: 0,
                end_ms: 1000,
                original_confidence: 0.8,
                verification_status: VerificationStatus::Verified,
                spectral_features: SpectralFeatures::default(),
                is_verified: true,
                is_suspicious: false,
                reason: None,
            }],
            segment_fingerprints: vec![vec![]],
            summary: VerificationSummary {
                total_segments: 1,
                verified_count: 1,
                suspicious_count: 0,
                rejected_count: 0,
                false_positive_rate: 0.0,
                average_confidence: 0.8,
                thresholds_applied: AppliedThresholds {
                    entropy_min: 3.5,
                    entropy_max: 7.0,
                    flatness_max: 0.45,
                    energy_min: 0.001,
                    centroid_min: 100.0,
                    centroid_max: 6000.0,
                },
            },
        };

        ctx.verification_report = Some(report);

        let result = execute_apply_learnings(&mut ctx).await;
        assert!(result.is_ok());

        let db = movie_radio_learning::database::LearningDb::new(&db_path)
            .await
            .unwrap();

        let total_verifications = db.get_total_verifications().await.unwrap();
        assert_eq!(total_verifications, 1);

        let experiments = db.list_experiments().await.unwrap();
        assert!(!experiments.is_empty());
        assert!(experiments[0].experiment_id.starts_with("goap_run_"));
    }
}
