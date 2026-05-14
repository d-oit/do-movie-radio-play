use crate::types::{Segment, SegmentKind, TimelineOutput};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{info, warn};

pub mod analysis;
pub mod extractor;

pub use analysis::{analyze_audio_features, SegmentAnalysis, SpectralFeatures, VerificationStatus};
pub use extractor::extract_segment_audio;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    pub verified_timeline: TimelineOutput,
    pub segment_results: Vec<SegmentVerification>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedThresholds {
    pub entropy_min: f32,
    pub entropy_max: f32,
    pub flatness_max: f32,
    pub energy_min: f32,
    pub centroid_min: f32,
    pub centroid_max: f32,
}

const DEFAULT_ENTROPY_MIN: f32 = 3.5;
const DEFAULT_ENTROPY_MAX: f32 = 7.0;
const DEFAULT_FLATNESS_MAX: f32 = 0.45;
const DEFAULT_ENERGY_MIN: f32 = 0.001;
const DEFAULT_CENTROID_MIN: f32 = 100.0;
const DEFAULT_CENTROID_MAX: f32 = 6000.0;
const VERIFICATION_HIGH_CONFIDENCE_THRESHOLD: f32 = 0.55;
const SPEECH_ZCR_MIN: f32 = 0.02;
const SPEECH_ZCR_MAX: f32 = 0.35;
const GRAPH_STRUCTURE_WEIGHT: f32 = 0.25;
const DEFAULT_FILTER_SEGMENT_CONFIDENCE_CEILING: f32 = 0.55;

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
) -> Result<VerificationReport> {
    let media_path_buf = media_path.to_path_buf();
    let media_exists = media_path_buf.exists();
    if !media_exists {
        bail!("Media file not found: {}", media_path_buf.display());
    }

    let thresholds = build_thresholds(
        entropy_min,
        entropy_max,
        flatness_max,
        energy_min,
        centroid_min,
        centroid_max,
    );

    let mut segment_results = Vec::new();
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

    for segment in &timeline.segments {
        if segment.kind != SegmentKind::NonVoice {
            verified_segments.push(segment.clone());
            continue;
        }

        let analysis = if media_exists {
            analyze_segment(&media_path_buf, segment, &thresholds)
        } else {
            Ok(SegmentAnalysis {
                status: VerificationStatus::Inconclusive,
                features: SpectralFeatures::default(),
                reason: Some("media not available".to_string()),
            })
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

        let is_verified = matches!(analysis.status, VerificationStatus::Verified);
        let is_suspicious = matches!(
            analysis.status,
            VerificationStatus::Suspicious | VerificationStatus::Inconclusive
        );

        let verification = SegmentVerification {
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
            original_confidence: segment.confidence,
            verification_status: analysis.status,
            spectral_features: analysis.features,
            is_verified,
            is_suspicious,
            reason: analysis.reason,
        };

        segment_results.push(verification);
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

    let thresholds = build_thresholds(None, None, None, None, None, None);
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
    DEFAULT_FILTER_SEGMENT_CONFIDENCE_CEILING
}

fn build_thresholds(
    entropy_min: Option<f32>,
    entropy_max: Option<f32>,
    flatness_max: Option<f32>,
    energy_min: Option<f32>,
    centroid_min: Option<f32>,
    centroid_max: Option<f32>,
) -> AppliedThresholds {
    AppliedThresholds {
        entropy_min: entropy_min.unwrap_or(DEFAULT_ENTROPY_MIN),
        entropy_max: entropy_max.unwrap_or(DEFAULT_ENTROPY_MAX),
        flatness_max: flatness_max.unwrap_or(DEFAULT_FLATNESS_MAX),
        energy_min: energy_min.unwrap_or(DEFAULT_ENERGY_MIN),
        centroid_min: centroid_min.unwrap_or(DEFAULT_CENTROID_MIN),
        centroid_max: centroid_max.unwrap_or(DEFAULT_CENTROID_MAX),
    }
}

fn analyze_segment(
    media_path: &Path,
    segment: &Segment,
    thresholds: &AppliedThresholds,
) -> Result<SegmentAnalysis> {
    let temp_wav = tempfile::Builder::new()
        .prefix("segment_")
        .suffix(".wav")
        .tempfile()
        .context("failed to create temp file")?
        .into_temp_path();

    extract_segment_audio(media_path, segment, &temp_wav)?;

    let samples = crate::io::wav::read_wav_to_f32(&temp_wav)?;

    let features = analyze_audio_features(&samples.0)?;

    let status = determine_verification_status(&features, segment.confidence, thresholds);

    Ok(SegmentAnalysis {
        status,
        features,
        reason: None,
    })
}

fn determine_verification_status(
    features: &SpectralFeatures,
    original_confidence: f32,
    thresholds: &AppliedThresholds,
) -> VerificationStatus {
    let entropy_voice =
        (thresholds.entropy_min..=thresholds.entropy_max).contains(&features.spectral_entropy);
    let flatness_voice = features.spectral_flatness < thresholds.flatness_max;
    let energy_voice = features.rms > thresholds.energy_min;
    let centroid_voice =
        (thresholds.centroid_min..=thresholds.centroid_max).contains(&features.centroid_hz);
    let zcr_voice = (SPEECH_ZCR_MIN..=SPEECH_ZCR_MAX).contains(&features.zcr);

    let voice_indicators = [
        entropy_voice,
        flatness_voice,
        energy_voice,
        centroid_voice,
        zcr_voice,
    ]
    .into_iter()
    .filter(|&b| b)
    .count() as f32;
    let voice_score =
        (((1.0 - original_confidence) * 0.3) + ((voice_indicators / 5.0) * 0.7)).clamp(0.0, 1.0);

    let nonvoice_indicators = [
        features.rms < thresholds.energy_min * 1.2,
        features.spectral_flatness > thresholds.flatness_max,
        !(thresholds.entropy_min..=thresholds.entropy_max).contains(&features.spectral_entropy),
        !(thresholds.centroid_min..=thresholds.centroid_max).contains(&features.centroid_hz),
        features.high_band_ratio > 0.4 && features.zcr > SPEECH_ZCR_MAX,
    ]
    .into_iter()
    .filter(|&b| b)
    .count() as f32;
    let nonvoice_score = (nonvoice_indicators / 5.0).clamp(0.0, 1.0);

    let graph_nonvoice_confidence = graph_structure_nonvoice_confidence(features, thresholds);
    let base_nonvoice_confidence =
        (((1.0 - voice_score) * 0.7) + (nonvoice_score * 0.3)).clamp(0.0, 1.0);
    let nonvoice_confidence = ((base_nonvoice_confidence * (1.0 - GRAPH_STRUCTURE_WEIGHT))
        + (graph_nonvoice_confidence * GRAPH_STRUCTURE_WEIGHT))
        .clamp(0.0, 1.0);

    if nonvoice_confidence >= VERIFICATION_HIGH_CONFIDENCE_THRESHOLD {
        VerificationStatus::Verified
    } else if voice_score >= 0.8 && nonvoice_score <= 0.2 {
        VerificationStatus::Rejected
    } else {
        VerificationStatus::Suspicious
    }
}

fn graph_structure_nonvoice_confidence(
    features: &SpectralFeatures,
    thresholds: &AppliedThresholds,
) -> f32 {
    let speech_region_score = [
        (thresholds.centroid_min..=thresholds.centroid_max).contains(&features.centroid_hz),
        (SPEECH_ZCR_MIN..=SPEECH_ZCR_MAX).contains(&features.zcr),
        features.spectral_flatness < thresholds.flatness_max,
        (thresholds.entropy_min..=thresholds.entropy_max).contains(&features.spectral_entropy),
    ]
    .into_iter()
    .filter(|&x| x)
    .count() as f32
        / 4.0;

    let nonvoice_region_score = [
        features.spectral_flatness > thresholds.flatness_max,
        features.spectral_entropy > thresholds.entropy_max,
        features.high_band_ratio > 0.4,
        features.spectral_flux > 0.02,
    ]
    .into_iter()
    .filter(|&x| x)
    .count() as f32
        / 4.0;

    ((nonvoice_region_score * 0.7) + ((1.0 - speech_region_score) * 0.3)).clamp(0.0, 1.0)
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

        let thresholds = AppliedThresholds {
            entropy_min: DEFAULT_ENTROPY_MIN,
            entropy_max: DEFAULT_ENTROPY_MAX,
            flatness_max: DEFAULT_FLATNESS_MAX,
            energy_min: DEFAULT_ENERGY_MIN,
            centroid_min: DEFAULT_CENTROID_MIN,
            centroid_max: DEFAULT_CENTROID_MAX,
        };

        let status = determine_verification_status(&speech_like_features, 0.9, &thresholds);
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
            determine_verification_status(&nonvoice_like_features, 0.6, &thresholds);
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
            thresholds_applied: AppliedThresholds {
                entropy_min: DEFAULT_ENTROPY_MIN,
                entropy_max: DEFAULT_ENTROPY_MAX,
                flatness_max: DEFAULT_FLATNESS_MAX,
                energy_min: DEFAULT_ENERGY_MIN,
                centroid_min: DEFAULT_CENTROID_MIN,
                centroid_max: DEFAULT_CENTROID_MAX,
            },
        };

        assert_eq!(summary.false_positive_rate, 0.2);
    }

    #[test]
    fn build_thresholds_uses_defaults() {
        let thresholds = build_thresholds(None, None, None, None, None, None);
        assert_eq!(thresholds.entropy_min, DEFAULT_ENTROPY_MIN);
        assert_eq!(thresholds.centroid_max, DEFAULT_CENTROID_MAX);
    }
}
