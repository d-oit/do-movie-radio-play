use anyhow::{Context, Result};
use movie_radio_io::wav::read_wav_to_f32;
use movie_radio_types::SfxLicense;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackType {
    SourceFilm,
    Narration,
    Sfx,
    Background,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum GainDuckingOperation {
    None,
    Ducked { level: f32 },
    Gain { db: f32 },
    Crossfade,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentProvenance {
    pub asset_id: String,
    pub source: String,
    pub license: SfxLicense,
    pub prompt: Option<String>,
}

impl Default for ContentProvenance {
    fn default() -> Self {
        Self {
            asset_id: "source_film".to_string(),
            source: "source_film".to_string(),
            license: SfxLicense::Cc0,
            prompt: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackMapEntry {
    pub id: String,
    pub track_type: TrackType,
    pub source_start_ms: u64,
    pub source_end_ms: u64,
    pub output_start_ms: u64,
    pub output_end_ms: u64,
    pub operation: GainDuckingOperation,
    pub provenance: ContentProvenance,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackMap {
    pub entries: Vec<TrackMapEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    pub start_ms: u64,
    pub end_ms: u64,
}

impl TimeRange {
    pub fn new(start_ms: u64, end_ms: u64) -> Self {
        Self { start_ms, end_ms }
    }

    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }

    pub fn overlaps(&self, other: &TimeRange) -> bool {
        self.start_ms < other.end_ms && other.start_ms < self.end_ms
    }

    pub fn overlap_ms(&self, other: &TimeRange) -> u64 {
        if self.overlaps(other) {
            let start = self.start_ms.max(other.start_ms);
            let end = self.end_ms.min(other.end_ms);
            end.saturating_sub(start)
        } else {
            0
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreservationConfig {
    /// Maximum RMS difference allowed between source and output in untouched regions
    pub max_untouched_rms_error: f32,
    /// Maximum allowed attenuation in dB for protected dialogue outside planned ducking
    pub max_dialogue_attenuation_db: f32,
    /// Target duck level expected during narration overlay
    pub duck_level: f32,
    /// Safety padding in ms around protected dialogue cues
    pub safety_padding_ms: u64,
    /// Maximum allowed overlap in ms between SFX and narration segments
    pub max_allowed_sfx_narration_overlap_ms: u64,
    /// Absolute sample peak threshold above which audio is flagged as clipping
    pub peak_clipping_threshold: f32,
    /// List of allowed license identifier strings
    pub allowed_licenses: Vec<String>,
}

impl Default for PreservationConfig {
    fn default() -> Self {
        Self {
            max_untouched_rms_error: 0.05,
            max_dialogue_attenuation_db: 1.0,
            duck_level: 0.2,
            safety_padding_ms: 50,
            max_allowed_sfx_narration_overlap_ms: 0,
            peak_clipping_threshold: 1.0,
            allowed_licenses: vec![
                "cc0".to_string(),
                "cc-by".to_string(),
                "public domain".to_string(),
                "mit".to_string(),
                "apache-2.0".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowType {
    UntouchedSource,
    ProtectedDialogue,
    NarrationPlacement,
    SfxPlacement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowVerificationVerdict {
    pub window_id: String,
    pub window_type: WindowType,
    pub source_start_ms: u64,
    pub source_end_ms: u64,
    pub output_start_ms: u64,
    pub output_end_ms: u64,
    pub passed: bool,
    pub failure_reasons: Vec<String>,
    pub measured_preservation_rms_diff: Option<f32>,
    pub measured_dialogue_attenuation_db: Option<f32>,
    pub measured_overlap_ms: Option<u64>,
    pub measured_peak_level: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseProvenanceVerdict {
    pub asset_id: String,
    pub license: String,
    pub allowed: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileIntegrityResult {
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub codec_valid: bool,
    pub duration_matches_source: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreservationSummary {
    pub total_windows: usize,
    pub passed_windows: usize,
    pub failed_windows: usize,
    pub total_assets: usize,
    pub verified_licenses: usize,
    pub failed_licenses: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreservationReport {
    pub overall_passed: bool,
    pub track_map: TrackMap,
    pub window_verdicts: Vec<WindowVerificationVerdict>,
    pub license_provenance_summary: Vec<LicenseProvenanceVerdict>,
    pub file_integrity: FileIntegrityResult,
    pub summary: PreservationSummary,
}

fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

fn compute_diff_rms(s1: &[f32], s2: &[f32]) -> f32 {
    let len = s1.len().min(s2.len());
    if len == 0 {
        return 0.0;
    }
    let sum_sq: f32 = s1[..len]
        .iter()
        .zip(&s2[..len])
        .map(|(&a, &b)| {
            let diff = a - b;
            diff * diff
        })
        .sum();
    (sum_sq / len as f32).sqrt()
}

fn compute_peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .map(|s| s.abs())
        .fold(0.0f32, |acc, val| acc.max(val))
}

fn ms_to_sample(ms: u64, sample_rate: u32, channels: u16) -> usize {
    let samples_per_channel = (ms as f64 * sample_rate as f64 / 1000.0) as usize;
    samples_per_channel * channels as usize
}

/// Evaluates audio preservation and narration placement on in-memory audio buffers.
pub fn verify_audio_preservation_samples(
    source_samples: &[f32],
    output_samples: &[f32],
    sample_rate: u32,
    channels: u16,
    dialogue_regions: &[TimeRange],
    track_map: &TrackMap,
    config: &PreservationConfig,
) -> PreservationReport {
    let mut window_verdicts = Vec::new();
    let mut license_verdicts = Vec::new();
    let channels = channels.max(1);

    let source_duration_ms =
        ((source_samples.len() as f64 / (sample_rate as f64 * channels as f64)) * 1000.0) as u64;
    let output_duration_ms =
        ((output_samples.len() as f64 / (sample_rate as f64 * channels as f64)) * 1000.0) as u64;

    let duration_diff = (source_duration_ms as i64 - output_duration_ms as i64).abs() as u64;
    let duration_matches = if source_duration_ms > 0 {
        (duration_diff as f64 / source_duration_ms as f64) <= 0.10
    } else {
        output_duration_ms > 0
    };

    let file_integrity = FileIntegrityResult {
        duration_ms: output_duration_ms,
        sample_rate,
        channels,
        codec_valid: !output_samples.is_empty(),
        duration_matches_source: duration_matches,
        error: if output_samples.is_empty() {
            Some("Output samples buffer is empty".to_string())
        } else if !duration_matches {
            Some(format!(
                "Output duration {output_duration_ms}ms deviates from source {source_duration_ms}ms by > 10%"
            ))
        } else {
            None
        },
    };

    // 1. Verify License Provenance for all entries
    for entry in &track_map.entries {
        let is_allowed = entry
            .provenance
            .license
            .is_allowed(&config.allowed_licenses);
        let reason = if is_allowed {
            None
        } else {
            Some(format!(
                "Asset '{}' license '{:?}' is not permitted in allowed_licenses",
                entry.provenance.asset_id, entry.provenance.license
            ))
        };
        license_verdicts.push(LicenseProvenanceVerdict {
            asset_id: entry.provenance.asset_id.clone(),
            license: format!("{:?}", entry.provenance.license),
            allowed: is_allowed,
            reason,
        });
    }

    // 2. Verify Untouched Source Regions
    for entry in &track_map.entries {
        if entry.track_type == TrackType::SourceFilm
            && entry.operation == GainDuckingOperation::None
        {
            let start_sample = ms_to_sample(entry.source_start_ms, sample_rate, channels);
            let end_sample = ms_to_sample(entry.source_end_ms, sample_rate, channels);

            let s_slice = if start_sample < source_samples.len() {
                &source_samples[start_sample..end_sample.min(source_samples.len())]
            } else {
                &[]
            };
            let o_slice = if start_sample < output_samples.len() {
                &output_samples[start_sample..end_sample.min(output_samples.len())]
            } else {
                &[]
            };

            let rms_diff = compute_diff_rms(s_slice, o_slice);
            let peak_level = compute_peak(o_slice);

            let mut passed = true;
            let mut failure_reasons = Vec::new();

            if rms_diff > config.max_untouched_rms_error {
                passed = false;
                failure_reasons.push(format!(
                    "Source film audio modified in untouched region [{}ms..{}ms]: RMS diff {:.4} > max allowed {:.4}",
                    entry.source_start_ms, entry.source_end_ms, rms_diff, config.max_untouched_rms_error
                ));
            }

            if peak_level > config.peak_clipping_threshold {
                passed = false;
                failure_reasons.push(format!(
                    "Peak sample clipping in untouched region [{}ms..{}ms]: peak {:.4} > threshold {:.4}",
                    entry.source_start_ms, entry.source_end_ms, peak_level, config.peak_clipping_threshold
                ));
            }

            window_verdicts.push(WindowVerificationVerdict {
                window_id: entry.id.clone(),
                window_type: WindowType::UntouchedSource,
                source_start_ms: entry.source_start_ms,
                source_end_ms: entry.source_end_ms,
                output_start_ms: entry.output_start_ms,
                output_end_ms: entry.output_end_ms,
                passed,
                failure_reasons,
                measured_preservation_rms_diff: Some(rms_diff),
                measured_dialogue_attenuation_db: None,
                measured_overlap_ms: None,
                measured_peak_level: Some(peak_level),
            });
        }
    }

    // 3. Verify Protected Dialogue Regions
    for (idx, dialogue) in dialogue_regions.iter().enumerate() {
        let dialogue_window_id = format!("protected_dialogue_{idx}");
        let mut passed = true;
        let mut failure_reasons = Vec::new();

        let padded_dialogue = TimeRange::new(
            dialogue.start_ms.saturating_sub(config.safety_padding_ms),
            dialogue.end_ms.saturating_add(config.safety_padding_ms),
        );

        let mut max_overlap = 0u64;

        // Check against narration segments
        for entry in &track_map.entries {
            if entry.track_type == TrackType::Narration {
                let narration_range = TimeRange::new(entry.output_start_ms, entry.output_end_ms);
                let overlap = padded_dialogue.overlap_ms(&narration_range);
                if overlap > 0 {
                    passed = false;
                    max_overlap = max_overlap.max(overlap);
                    failure_reasons.push(format!(
                        "Narration segment '{}' [{}ms..{}ms] encroaches on protected dialogue cue [{}ms..{}ms] with {}ms safety padding (overlap: {}ms)",
                        entry.id, entry.output_start_ms, entry.output_end_ms, dialogue.start_ms, dialogue.end_ms, config.safety_padding_ms, overlap
                    ));
                }
            }
        }

        // Check for dialogue attenuation/deletion
        let start_sample = ms_to_sample(dialogue.start_ms, sample_rate, channels);
        let end_sample = ms_to_sample(dialogue.end_ms, sample_rate, channels);

        let s_slice = if start_sample < source_samples.len() {
            &source_samples[start_sample..end_sample.min(source_samples.len())]
        } else {
            &[]
        };
        let o_slice = if start_sample < output_samples.len() {
            &output_samples[start_sample..end_sample.min(output_samples.len())]
        } else {
            &[]
        };

        let src_rms = compute_rms(s_slice);
        let out_rms = compute_rms(o_slice);

        let attenuation_db = if src_rms > 0.001 {
            if out_rms < 0.0001 {
                999.0 // Completely deleted/muted
            } else {
                20.0 * (src_rms / out_rms).log10()
            }
        } else {
            0.0
        };

        if src_rms > 0.001 && attenuation_db > config.max_dialogue_attenuation_db {
            passed = false;
            failure_reasons.push(format!(
                "Protected dialogue cue [{}ms..{}ms] attenuated by {:.2} dB beyond max limit {:.2} dB",
                dialogue.start_ms, dialogue.end_ms, attenuation_db, config.max_dialogue_attenuation_db
            ));
        }

        window_verdicts.push(WindowVerificationVerdict {
            window_id: dialogue_window_id,
            window_type: WindowType::ProtectedDialogue,
            source_start_ms: dialogue.start_ms,
            source_end_ms: dialogue.end_ms,
            output_start_ms: dialogue.start_ms,
            output_end_ms: dialogue.end_ms,
            passed,
            failure_reasons,
            measured_preservation_rms_diff: None,
            measured_dialogue_attenuation_db: Some(attenuation_db),
            measured_overlap_ms: if max_overlap > 0 {
                Some(max_overlap)
            } else {
                None
            },
            measured_peak_level: Some(compute_peak(o_slice)),
        });
    }

    // 4. Verify Narration & SFX Placement Windows & Overlaps/Clipping
    let narrations: Vec<_> = track_map
        .entries
        .iter()
        .filter(|e| e.track_type == TrackType::Narration)
        .collect();
    let sfx_entries: Vec<_> = track_map
        .entries
        .iter()
        .filter(|e| e.track_type == TrackType::Sfx)
        .collect();

    for narr in &narrations {
        let narr_range = TimeRange::new(narr.output_start_ms, narr.output_end_ms);
        let mut passed = true;
        let mut failure_reasons = Vec::new();

        // Check overlap with SFX
        for sfx in &sfx_entries {
            let sfx_range = TimeRange::new(sfx.output_start_ms, sfx.output_end_ms);
            let sfx_overlap = narr_range.overlap_ms(&sfx_range);
            if sfx_overlap > config.max_allowed_sfx_narration_overlap_ms {
                passed = false;
                failure_reasons.push(format!(
                    "Narration '{}' [{}ms..{}ms] overlaps SFX '{}' [{}ms..{}ms] by {}ms (max allowed {}ms)",
                    narr.id, narr.output_start_ms, narr.output_end_ms, sfx.id, sfx.output_start_ms, sfx.output_end_ms, sfx_overlap, config.max_allowed_sfx_narration_overlap_ms
                ));
            }
        }

        // Check clipping
        let start_sample = ms_to_sample(narr.output_start_ms, sample_rate, channels);
        let end_sample = ms_to_sample(narr.output_end_ms, sample_rate, channels);
        let o_slice = if start_sample < output_samples.len() {
            &output_samples[start_sample..end_sample.min(output_samples.len())]
        } else {
            &[]
        };
        let peak = compute_peak(o_slice);
        if peak > config.peak_clipping_threshold {
            passed = false;
            failure_reasons.push(format!(
                "Audio clipping in narration window '{}' [{}ms..{}ms]: peak level {:.4} > threshold {:.4}",
                narr.id, narr.output_start_ms, narr.output_end_ms, peak, config.peak_clipping_threshold
            ));
        }

        window_verdicts.push(WindowVerificationVerdict {
            window_id: narr.id.clone(),
            window_type: WindowType::NarrationPlacement,
            source_start_ms: narr.source_start_ms,
            source_end_ms: narr.source_end_ms,
            output_start_ms: narr.output_start_ms,
            output_end_ms: narr.output_end_ms,
            passed,
            failure_reasons,
            measured_preservation_rms_diff: None,
            measured_dialogue_attenuation_db: None,
            measured_overlap_ms: None,
            measured_peak_level: Some(peak),
        });
    }

    let failed_windows = window_verdicts.iter().filter(|w| !w.passed).count();
    let passed_windows = window_verdicts.len() - failed_windows;

    let failed_licenses = license_verdicts.iter().filter(|l| !l.allowed).count();
    let verified_licenses = license_verdicts.len() - failed_licenses;

    let overall_passed =
        failed_windows == 0 && failed_licenses == 0 && file_integrity.error.is_none();

    let summary = PreservationSummary {
        total_windows: window_verdicts.len(),
        passed_windows,
        failed_windows,
        total_assets: track_map.entries.len(),
        verified_licenses,
        failed_licenses,
    };

    PreservationReport {
        overall_passed,
        track_map: track_map.clone(),
        window_verdicts,
        license_provenance_summary: license_verdicts,
        file_integrity,
        summary,
    }
}

/// Evaluates audio preservation and narration placement by reading source and output media files.
pub fn verify_audio_preservation_files(
    source_media_path: &Path,
    output_media_path: &Path,
    dialogue_regions: &[TimeRange],
    track_map: &TrackMap,
    config: &PreservationConfig,
    report_output_path: Option<&Path>,
) -> Result<PreservationReport> {
    if !source_media_path.exists() {
        anyhow::bail!(
            "Source media file does not exist: {}",
            source_media_path.display()
        );
    }
    if !output_media_path.exists() {
        anyhow::bail!(
            "Output media file does not exist: {}",
            output_media_path.display()
        );
    }

    let (source_samples, sample_rate) = read_wav_to_f32(source_media_path).with_context(|| {
        format!(
            "Failed to read WAV source media: {}",
            source_media_path.display()
        )
    })?;
    let (output_samples, _out_rate) = read_wav_to_f32(output_media_path).with_context(|| {
        format!(
            "Failed to read WAV output media: {}",
            output_media_path.display()
        )
    })?;

    let report = verify_audio_preservation_samples(
        &source_samples,
        &output_samples,
        sample_rate,
        1,
        dialogue_regions,
        track_map,
        config,
    );

    if let Some(out_path) = report_output_path {
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&report)?;
        std::fs::write(out_path, json)?;
    }

    Ok(report)
}

#[cfg(test)]
pub mod e2e_fixture_tests {
    use super::*;

    pub struct DeterministicE2eFixture {
        pub source_samples: Vec<f32>,
        pub output_samples: Vec<f32>,
        pub sample_rate: u32,
        pub dialogue_regions: Vec<TimeRange>,
        pub track_map: TrackMap,
        pub config: PreservationConfig,
    }

    pub fn generate_deterministic_e2e_fixture() -> DeterministicE2eFixture {
        let sample_rate = 16_000u32;
        let total_duration_sec = 10;
        let total_samples = (total_duration_sec * sample_rate) as usize;

        let mut source_samples = vec![0.01f32; total_samples]; // Low noise baseline

        // Speech 1: 1000ms .. 4000ms (440 Hz sine)
        let s1_start = (1.0 * sample_rate as f32) as usize;
        let s1_end = (4.0 * sample_rate as f32) as usize;
        for i in s1_start..s1_end {
            let t = (i - s1_start) as f32 / sample_rate as f32;
            source_samples[i] = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;
        }

        // Speech 2: 7000ms .. 10000ms (880 Hz sine)
        let s2_start = (7.0 * sample_rate as f32) as usize;
        let s2_end = (10.0 * sample_rate as f32) as usize;
        for i in s2_start..s2_end {
            let t = (i - s2_start) as f32 / sample_rate as f32;
            source_samples[i] = (2.0 * std::f32::consts::PI * 880.0 * t).sin() * 0.5;
        }

        let dialogue_regions = vec![TimeRange::new(1000, 4000), TimeRange::new(7000, 10000)];

        let mut output_samples = source_samples.clone();

        // Narration in Gap 2: 4100ms .. 6900ms (523 Hz sine)
        let n_start = (4.1 * sample_rate as f32) as usize;
        let n_end = (6.9 * sample_rate as f32) as usize;
        for i in n_start..n_end {
            let t = (i - n_start) as f32 / sample_rate as f32;
            output_samples[i] += (2.0 * std::f32::consts::PI * 523.25 * t).sin() * 0.3;
        }

        // SFX in Gap 1: 100ms .. 800ms
        let sfx_start = (0.1 * sample_rate as f32) as usize;
        let sfx_end = (0.8 * sample_rate as f32) as usize;
        for i in sfx_start..sfx_end {
            let t = (i - sfx_start) as f32 / sample_rate as f32;
            output_samples[i] += (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 0.1;
        }

        let track_map = TrackMap {
            entries: vec![
                TrackMapEntry {
                    id: "source_film_untouched_1".to_string(),
                    track_type: TrackType::SourceFilm,
                    source_start_ms: 1000,
                    source_end_ms: 4000,
                    output_start_ms: 1000,
                    output_end_ms: 4000,
                    operation: GainDuckingOperation::None,
                    provenance: ContentProvenance::default(),
                },
                TrackMapEntry {
                    id: "source_film_untouched_2".to_string(),
                    track_type: TrackType::SourceFilm,
                    source_start_ms: 7000,
                    source_end_ms: 10000,
                    output_start_ms: 7000,
                    output_end_ms: 10000,
                    operation: GainDuckingOperation::None,
                    provenance: ContentProvenance::default(),
                },
                TrackMapEntry {
                    id: "narration_gap_2".to_string(),
                    track_type: TrackType::Narration,
                    source_start_ms: 4100,
                    source_end_ms: 6900,
                    output_start_ms: 4100,
                    output_end_ms: 6900,
                    operation: GainDuckingOperation::Gain { db: 0.0 },
                    provenance: ContentProvenance {
                        asset_id: "narr_01".to_string(),
                        source: "synthesized".to_string(),
                        license: SfxLicense::Cc0,
                        prompt: Some("The character looks around the room.".to_string()),
                    },
                },
                TrackMapEntry {
                    id: "sfx_gap_1".to_string(),
                    track_type: TrackType::Sfx,
                    source_start_ms: 100,
                    source_end_ms: 800,
                    output_start_ms: 100,
                    output_end_ms: 800,
                    operation: GainDuckingOperation::Gain { db: 0.0 },
                    provenance: ContentProvenance {
                        asset_id: "sfx_whoosh".to_string(),
                        source: "local".to_string(),
                        license: SfxLicense::Cc0,
                        prompt: None,
                    },
                },
            ],
        };

        let config = PreservationConfig::default();

        DeterministicE2eFixture {
            source_samples,
            output_samples,
            sample_rate,
            dialogue_regions,
            track_map,
            config,
        }
    }

    #[test]
    fn test_e2e_known_good_fixture_passes() {
        let fix = generate_deterministic_e2e_fixture();
        let report = verify_audio_preservation_samples(
            &fix.source_samples,
            &fix.output_samples,
            fix.sample_rate,
            1,
            &fix.dialogue_regions,
            &fix.track_map,
            &fix.config,
        );

        assert!(
            report.overall_passed,
            "Known-good complete radio-play fixture should pass verification. Failures: {:#?}",
            report.window_verdicts
        );
        assert_eq!(report.summary.failed_windows, 0);
        assert_eq!(report.summary.failed_licenses, 0);
        assert!(report.file_integrity.codec_valid);
    }

    #[test]
    fn test_mutation_narration_shifted_into_dialogue_fails() {
        let mut fix = generate_deterministic_e2e_fixture();

        // Shift narration segment output window into dialogue 1 (e.g. 3500ms..5500ms)
        for entry in &mut fix.track_map.entries {
            if entry.id == "narration_gap_2" {
                entry.output_start_ms = 3500;
                entry.output_end_ms = 5500;
            }
        }

        let report = verify_audio_preservation_samples(
            &fix.source_samples,
            &fix.output_samples,
            fix.sample_rate,
            1,
            &fix.dialogue_regions,
            &fix.track_map,
            &fix.config,
        );

        assert!(
            !report.overall_passed,
            "Shifted narration encroaching dialogue should fail verification"
        );
        assert!(report.summary.failed_windows > 0);

        let dialogue_verdict = report
            .window_verdicts
            .iter()
            .find(|w| w.window_id == "protected_dialogue_0")
            .expect("dialogue 0 verdict");

        assert!(
            !dialogue_verdict.passed,
            "Protected dialogue window must fail"
        );
        assert!(
            dialogue_verdict
                .failure_reasons
                .iter()
                .any(|r| r.contains("encroaches on protected dialogue cue")),
            "Failure reason must identify exact dialogue window encroachment: {:?}",
            dialogue_verdict.failure_reasons
        );
    }

    #[test]
    fn test_mutation_missing_source_segment_fails() {
        let fix = generate_deterministic_e2e_fixture();
        let mut output_samples = fix.output_samples.clone();

        // Mute source dialogue in output during dialogue 1 (1000ms..4000ms)
        let s_start = (1.0 * fix.sample_rate as f32) as usize;
        let s_end = (4.0 * fix.sample_rate as f32) as usize;
        for sample in &mut output_samples[s_start..s_end] {
            *sample = 0.0;
        }

        let report = verify_audio_preservation_samples(
            &fix.source_samples,
            &output_samples,
            fix.sample_rate,
            1,
            &fix.dialogue_regions,
            &fix.track_map,
            &fix.config,
        );

        assert!(
            !report.overall_passed,
            "Muted/missing source audio should fail verification"
        );
        assert!(report.summary.failed_windows > 0);

        let untouched_verdict = report
            .window_verdicts
            .iter()
            .find(|w| w.window_id == "source_film_untouched_1")
            .expect("untouched 1 verdict");

        assert!(
            !untouched_verdict.passed,
            "Untouched source window must fail"
        );
        assert!(
            untouched_verdict
                .failure_reasons
                .iter()
                .any(|r| r.contains("Source film audio modified")),
            "Failure reason must report RMS difference in modified region: {:?}",
            untouched_verdict.failure_reasons
        );
    }

    #[test]
    fn test_mutation_exceed_gain_limits_clipping_fails() {
        let fix = generate_deterministic_e2e_fixture();
        let mut output_samples = fix.output_samples.clone();

        // Amplify narration window (4100ms..6900ms) to 10.0x causing sample clipping
        let n_start = (4.1 * fix.sample_rate as f32) as usize;
        let n_end = (6.9 * fix.sample_rate as f32) as usize;
        for sample in &mut output_samples[n_start..n_end] {
            *sample *= 10.0;
        }

        let report = verify_audio_preservation_samples(
            &fix.source_samples,
            &output_samples,
            fix.sample_rate,
            1,
            &fix.dialogue_regions,
            &fix.track_map,
            &fix.config,
        );

        assert!(
            !report.overall_passed,
            "Audio clipping / excessive gain should fail verification"
        );
        assert!(report.summary.failed_windows > 0);

        let narration_verdict = report
            .window_verdicts
            .iter()
            .find(|w| w.window_id == "narration_gap_2")
            .expect("narration verdict");

        assert!(
            !narration_verdict.passed,
            "Narration window with clipping must fail"
        );
        assert!(
            narration_verdict
                .failure_reasons
                .iter()
                .any(|r| r.contains("Audio clipping")),
            "Failure reason must identify audio clipping: {:?}",
            narration_verdict.failure_reasons
        );
    }

    #[test]
    fn test_mutation_disallowed_sfx_license_fails() {
        let mut fix = generate_deterministic_e2e_fixture();

        // Change SFX license to non-commercial CC-BY-NC
        for entry in &mut fix.track_map.entries {
            if entry.id == "sfx_gap_1" {
                entry.provenance.license = SfxLicense::CcByNc;
            }
        }

        // Restrict allowed licenses to CC0 only
        let mut config = fix.config.clone();
        config.allowed_licenses = vec!["cc0".to_string()];

        let report = verify_audio_preservation_samples(
            &fix.source_samples,
            &fix.output_samples,
            fix.sample_rate,
            1,
            &fix.dialogue_regions,
            &fix.track_map,
            &fix.config,
        );

        assert!(
            !report.overall_passed,
            "Disallowed SFX license provenance should fail verification"
        );
        assert_eq!(report.summary.failed_licenses, 1);

        let license_verdict = report
            .license_provenance_summary
            .iter()
            .find(|l| l.asset_id == "sfx_whoosh")
            .expect("sfx_whoosh license verdict");

        assert!(!license_verdict.allowed);
        assert!(
            license_verdict
                .reason
                .as_ref()
                .unwrap()
                .contains("not permitted"),
            "Reason must state license is not permitted: {:?}",
            license_verdict.reason
        );
    }
}
