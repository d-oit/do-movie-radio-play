use anyhow::{Context, Result};
use movie_radio_io::wav::read_wav_to_f32;
use std::path::Path;

pub use super::preservation_types::*;

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

    let duration_diff = source_duration_ms.abs_diff(output_duration_ms);
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
#[path = "preservation_tests.rs"]
mod tests;
