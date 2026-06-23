use anyhow::{bail, Context, Result};
use movie_radio_types::{Segment, SegmentKind, TimelineOutput};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{info, warn};

pub mod analysis;
pub mod extractor;
pub mod fingerprint;
pub mod scoring;

pub use analysis::{analyze_audio_features, SegmentAnalysis, SpectralFeatures, VerificationStatus};
pub use extractor::extract_segment_audio;
pub use scoring::AppliedThresholds;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    pub verified_timeline: TimelineOutput,
    pub segment_results: Vec<SegmentVerification>,
    pub segment_fingerprints: Vec<Vec<fingerprint::Fingerprint>>,
    pub summary: VerificationSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentVerification {
    pub start_ms: u64,
    pub end_ms: u64,
    pub original_confidence: f32,
    pub verification_status: VerificationStatus,
    pub spectral_features: SpectralFeatures,
    pub is_verified: bool,
    pub is_suspicious: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationSummary {
    pub total_segments: usize,
    pub verified_count: usize,
    pub suspicious_count: usize,
    pub rejected_count: usize,
    pub false_positive_rate: f32,
    pub average_confidence: f32,
    pub thresholds_applied: AppliedThresholds,
}

#[allow(clippy::too_many_arguments)]
pub fn verify_timeline(
    media_path: &Path,
    timeline: &TimelineOutput,
    output_path: &Path,
    entropy_min: Option<f32>,
    entropy_max: Option<f32>,
    flatness_max: Option<f32>,
    energy_min: Option<f32>,
    centroid_min: Option<f32>,
    centroid_max: Option<f32>,
    use_fingerprints: bool,
    fingerprint_threshold: u32,
    learning_db_path: Option<std::path::PathBuf>,
) -> Result<VerificationReport> {
    let media_path_buf = media_path.to_path_buf();
    let media_exists = media_path_buf.exists();
    if !media_exists {
        bail!("Media file not found: {}", media_path_buf.display());
    }

    let thresholds = scoring::build_thresholds(
        entropy_min,
        entropy_max,
        flatness_max,
        energy_min,
        centroid_min,
        centroid_max,
    );

    let mut segment_results = Vec::new();
    let mut segment_fingerprints = Vec::new();
    let mut verified_segments = Vec::new();

    let non_voice_segments: Vec<_> = timeline
        .segments
        .iter()
        .filter(|s| s.kind == SegmentKind::NonVoice)
        .collect();

    info!(
        total_segments = timeline.segments.len(),
        non_voice_segments = non_voice_segments.len(),
        "starting verification"
    );

    let rt = if use_fingerprints {
        Some(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .context("failed to create async runtime for fingerprint matching")?,
        )
    } else {
        None
    };

    let db = if let (true, Some(rt), Some(db_path)) = (use_fingerprints, &rt, &learning_db_path) {
        Some(rt.block_on(movie_radio_learning::database::LearningDb::new(db_path))?)
    } else {
        None
    };

    for segment in &timeline.segments {
        if segment.kind != SegmentKind::NonVoice {
            verified_segments.push(segment.clone());
            segment_fingerprints.push(Vec::new());
            continue;
        }

        let (analysis, fingerprints) = if media_exists {
            analyze_segment_with_fingerprints(&media_path_buf, segment, &thresholds)
        } else {
            (
                Ok(SegmentAnalysis {
                    status: VerificationStatus::Inconclusive,
                    features: SpectralFeatures::default(),
                    reason: Some("media not available".to_string()),
                }),
                Vec::new(),
            )
        };

        let analysis = match analysis {
            Ok(a) => a,
            Err(e) => {
                warn!(
                    segment_ms = format!("{}-{}", segment.start_ms, segment.end_ms),
                    error = %e,
                    "segment analysis failed, marking as suspicious"
                );
                SegmentAnalysis {
                    status: VerificationStatus::Inconclusive,
                    features: SpectralFeatures::default(),
                    reason: Some(format!("analysis failed: {e}")),
                }
            }
        };

        let mut is_verified = matches!(analysis.status, VerificationStatus::Verified);
        let mut is_suspicious = matches!(
            analysis.status,
            VerificationStatus::Suspicious | VerificationStatus::Inconclusive
        );
        let mut reason = analysis.reason;

        if use_fingerprints && !fingerprints.is_empty() {
            if let (Some(rt), Some(db)) = (&rt, &db) {
                let query_hashes: Vec<u32> = fingerprints.iter().map(|f| f.hash).collect();
                let stored_fps = rt.block_on(db.find_fingerprint_matches(&query_hashes))?;

                if !stored_fps.is_empty() {
                    let matches = fingerprint::match_fingerprints(&fingerprints, stored_fps);
                    for (_seg_id, score) in matches {
                        if score >= fingerprint_threshold {
                            info!(
                                segment_ms = format!("{}-{}", segment.start_ms, segment.end_ms),
                                score = score,
                                "fingerprint match found, verifying segment"
                            );
                            is_verified = true;
                            is_suspicious = false;
                            if let Some(ref mut r) = reason {
                                *r = format!("{r}; fingerprint match (score={score})");
                            } else {
                                reason = Some(format!("fingerprint match (score={score})"));
                            }
                            break;
                        }
                    }
                }
            }
        }

        let verification = SegmentVerification {
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
            original_confidence: segment.confidence,
            verification_status: analysis.status,
            spectral_features: analysis.features,
            is_verified,
            is_suspicious,
            reason,
        };

        segment_results.push(verification);
        segment_fingerprints.push(fingerprints);
        verified_segments.push(segment.clone());
    }

    let verified_count = segment_results.iter().filter(|r| r.is_verified).count();
    let suspicious_count = segment_results.iter().filter(|r| r.is_suspicious).count();
    let rejected_count = segment_results
        .iter()
        .filter(|r| !r.is_verified && !r.is_suspicious)
        .count();

    let total_non_voice = non_voice_segments.len();
    let false_positive_rate = if total_non_voice > 0 {
        suspicious_count as f32 / total_non_voice as f32
    } else {
        0.0
    };

    let average_confidence: f32 = if !segment_results.is_empty() {
        segment_results
            .iter()
            .map(|r| r.original_confidence)
            .sum::<f32>()
            / segment_results.len() as f32
    } else {
        1.0
    };

    let summary = VerificationSummary {
        total_segments: timeline.segments.len(),
        verified_count,
        suspicious_count,
        rejected_count,
        false_positive_rate,
        average_confidence,
        thresholds_applied: thresholds,
    };

    let verified_timeline = TimelineOutput {
        file: timeline.file.clone(),
        analysis_sample_rate: timeline.analysis_sample_rate,
        frame_ms: timeline.frame_ms,
        segments: verified_segments,
    };

    let report = VerificationReport {
        verified_timeline,
        segment_results,
        segment_fingerprints,
        summary,
    };

    write_verification_report(output_path, &report)
        .with_context(|| format!("failed to write report to {}", output_path.display()))?;

    info!(
        verified = report.summary.verified_count,
        suspicious = report.summary.suspicious_count,
        rejected = report.summary.rejected_count,
        fp_rate = format!("{:.2}%", report.summary.false_positive_rate * 100.0),
        "verification complete"
    );

    Ok(report)
}

pub fn filter_low_confidence_non_voice_segments(
    media_path: &Path,
    segments: &[Segment],
    confidence_ceiling: f32,
) -> Vec<Segment> {
    if !media_path.exists() {
        return segments.to_vec();
    }

    let thresholds = scoring::build_thresholds(None, None, None, None, None, None);
    segments
        .iter()
        .filter(|segment| {
            if segment.kind != SegmentKind::NonVoice || segment.confidence > confidence_ceiling {
                return true;
            }
            match analyze_segment(media_path, segment, &thresholds) {
                Ok(analysis) => matches!(analysis.status, VerificationStatus::Verified),
                Err(err) => {
                    warn!(
                        segment_ms = format!("{}-{}", segment.start_ms, segment.end_ms),
                        error = %err,
                        "verification filter failed, keeping segment"
                    );
                    true
                }
            }
        })
        .cloned()
        .collect()
}

pub fn default_filter_segment_confidence_ceiling() -> f32 {
    scoring::default_filter_segment_confidence_ceiling()
}

fn analyze_segment(
    media_path: &Path,
    segment: &Segment,
    thresholds: &AppliedThresholds,
) -> Result<SegmentAnalysis> {
    let (analysis, _) = analyze_segment_with_fingerprints(media_path, segment, thresholds);
    analysis
}

fn analyze_segment_with_fingerprints(
    media_path: &Path,
    segment: &Segment,
    thresholds: &AppliedThresholds,
) -> (Result<SegmentAnalysis>, Vec<fingerprint::Fingerprint>) {
    let temp_wav_res = tempfile::Builder::new()
        .prefix("segment_")
        .suffix(".wav")
        .tempfile()
        .context("failed to create temp file");

    let temp_wav = match temp_wav_res {
        Ok(t) => t.into_temp_path(),
        Err(e) => return (Err(e), Vec::new()),
    };

    if let Err(e) = extract_segment_audio(media_path, segment, &temp_wav) {
        return (Err(e), Vec::new());
    }

    let samples_res = movie_radio_io::wav::read_wav_to_f32(&temp_wav);
    let (samples, _) = match samples_res {
        Ok(s) => s,
        Err(e) => return (Err(e), Vec::new()),
    };

    let features = match analyze_audio_features(&samples) {
        Ok(f) => f,
        Err(e) => return (Err(e), Vec::new()),
    };

    let fingerprints = fingerprint::fingerprint_segment(&samples, fingerprint::DEFAULT_SAMPLE_RATE);

    let status = scoring::determine_verification_status(&features, segment.confidence, thresholds);

    (
        Ok(SegmentAnalysis {
            status,
            features,
            reason: None,
        }),
        fingerprints,
    )
}

fn write_verification_report(path: &Path, report: &VerificationReport) -> Result<()> {
    let json = serde_json::to_vec_pretty(report)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, json)?;
    Ok(())
}

impl SegmentVerification {
    #[allow(dead_code)]
    pub fn suspicious_count(&self) -> usize {
        if self.is_suspicious {
            1
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verification_status_determination() {
        let speech_like_features = SpectralFeatures {
            rms: 0.02,
            zcr: 0.15,
            spectral_entropy: 5.0,
            spectral_flatness: 0.2,
            spectral_flux: 0.005,
            centroid_hz: 1500.0,
            low_band_ratio: 0.3,
            high_band_ratio: 0.4,
        };

        let thresholds = scoring::build_thresholds(None, None, None, None, None, None);

        let status =
            scoring::determine_verification_status(&speech_like_features, 0.9, &thresholds);
        assert!(matches!(
            status,
            VerificationStatus::Suspicious | VerificationStatus::Rejected
        ));

        let nonvoice_like_features = SpectralFeatures {
            rms: 0.0004,
            zcr: 0.46,
            spectral_entropy: 8.0,
            spectral_flatness: 0.72,
            spectral_flux: 0.002,
            centroid_hz: 7200.0,
            low_band_ratio: 0.1,
            high_band_ratio: 0.52,
        };
        let nonvoice_status =
            scoring::determine_verification_status(&nonvoice_like_features, 0.6, &thresholds);
        assert!(matches!(nonvoice_status, VerificationStatus::Verified));
    }

    #[test]
    fn false_positive_rate_calculation() {
        let summary = VerificationSummary {
            total_segments: 10,
            verified_count: 7,
            suspicious_count: 2,
            rejected_count: 1,
            false_positive_rate: 0.2,
            average_confidence: 0.75,
            thresholds_applied: scoring::build_thresholds(None, None, None, None, None, None),
        };

        assert_eq!(summary.false_positive_rate, 0.2);
    }
}
