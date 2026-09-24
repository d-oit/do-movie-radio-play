use anyhow::{Context, Result};
use movie_radio_types::{AnalysisConfig, AppConfig, VoiceReference};
use std::collections::HashMap;
use std::path::Path;

/// Historical candidate floor, kept for reference: the effective minimum
/// is the configured `min_sample_seconds`, clamped to the 1s schema floor.
#[allow(dead_code)]
const MIN_CANDIDATE_MS: u64 = 2000;
const MAX_CANDIDATE_MS: u64 = 15000;

/// Reject `..` escapes so persisted sample references stay portable. Reads run
/// with the caller's own privileges and no write path derives from `input`,
/// so absolute paths remain accepted.
fn reject_escape(path: &Path, field: &str) -> Result<()> {
    if path
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        anyhow::bail!("{field} must not contain ..");
    }
    Ok(())
}

/// Validate the character handle: non-empty and filename-safe so it can be
/// used for candidate ids and JSON file names.
fn validate_character(character: &str) -> Result<()> {
    if character.trim().is_empty() {
        anyhow::bail!("character must not be empty");
    }
    if character.contains("..") || character.contains(['/', '\\']) {
        anyhow::bail!("character must not contain path separators or ..");
    }
    Ok(())
}

pub fn extract_candidates(
    input: &Path,
    cfg: &AppConfig,
    character: &str,
) -> Result<Vec<VoiceReference>> {
    validate_character(character)?;
    reject_escape(input, "input path")?;

    let supports_clone = matches!(
        cfg.voice_clone.family.as_str(),
        "qwen3_tts" | "chatterbox" | "pocket_tts"
    );
    if !supports_clone && cfg.voice_clone.enabled {
        anyhow::bail!(
            "selected family {} does not support voice cloning",
            cfg.voice_clone.family
        );
    }
    if !cfg.voice.audio_cpp.remote.server_url.is_empty() && cfg.voice_clone.routing.mode == "auto" {
        tracing::info!(
            "reference audio would be sent to remote endpoint (explicit consent required)"
        );
    }

    let max_candidates = usize::try_from(cfg.voice_clone.max_samples_per_character).unwrap_or(1);
    let mut candidates = Vec::new();
    // Decoded once for trailing-gap math; clip slicing decodes per
    // candidate (cheap relative to extraction, keeps the helper pure).
    let decoded =
        crate::pipeline::decode::decode_audio(input, AnalysisConfig::default().sample_rate_hz).ok();

    // Configured minimum wins: the schema admits values from 1s up, so
    // only clamp to that valid range instead of the historical 2s floor.
    let min_ms =
        ((f64::from(cfg.voice_clone.min_sample_seconds) * 1000.0).round() as u64).max(1000);
    let timeline = crate::pipeline::extract_timeline(input, &AnalysisConfig::default()).ok();
    if let Some(timeline) = timeline.as_ref() {
        // The timeline carries non-voice segments; the gaps between them are
        // the speech/dialogue regions eligible as clone candidates.
        let mut gaps: Vec<(u64, u64)> = Vec::new();
        let mut last_end_ms: u64 = 0;
        for seg in &timeline.segments {
            if seg.start_ms > last_end_ms {
                gaps.push((last_end_ms, seg.start_ms));
            }
            last_end_ms = seg.end_ms.max(last_end_ms);
        }
        // Trailing speech after the final non-voice segment is a candidate
        // too; total duration comes from the decoded audio, not the last
        // segment end.
        if let Some((mono, rate)) = decoded.as_ref() {
            let total_ms = mono.len() as u64 * 1000 / u64::from((*rate).max(1));
            if total_ms > last_end_ms {
                gaps.push((last_end_ms, total_ms));
            }
        }
        // Long, uninterrupted dialogue spans are common in real movies (a
        // single scene can run well past MAX_CANDIDATE_MS). Slice each span
        // into consecutive MAX_CANDIDATE_MS windows instead of discarding it
        // outright, so `voice samples` still yields usable references for
        // every speaking character rather than only ones with frequent
        // music/SFX interruptions. A trailing remainder under `min_ms` is
        // dropped rather than padded, keeping every clip's own duration
        // honest.
        let mut candidate_no: usize = 0;
        'gaps: for (start, end) in &gaps {
            let mut chunk_start = *start;
            while chunk_start < *end {
                let chunk_len = end.saturating_sub(chunk_start).min(MAX_CANDIDATE_MS);
                if chunk_len < min_ms {
                    break;
                }
                let chunk_end = chunk_start + chunk_len;
                candidate_no += 1;

                let mut meta = HashMap::default();
                meta.insert("start_ms".to_string(), serde_json::json!(chunk_start));
                meta.insert("end_ms".to_string(), serde_json::json!(chunk_end));
                meta.insert("duration_ms".to_string(), serde_json::json!(chunk_len));

                // Persist the interval as its own clip: consumers feed
                // `sample_paths` straight to synthesis, so the whole movie
                // here would clone non-voice regions the filter excluded.
                let sample_path = match write_candidate_clip(
                    input,
                    character,
                    candidate_no,
                    chunk_start,
                    chunk_end,
                ) {
                    Ok(path) => path,
                    Err(err) => {
                        tracing::warn!(error = %err, "skipping candidate: clip write failed");
                        chunk_start = chunk_end;
                        continue;
                    }
                };

                let cand = VoiceReference {
                    id: format!("{character}_candidate_{candidate_no}"),
                    character_name: character.to_string(),
                    sample_paths: vec![sample_path],
                    metadata: meta,
                    created_at: None,
                    runtime: cfg.voice_clone.runtime.clone(),
                    family: cfg.voice_clone.family.clone(),
                    model: cfg.voice_clone.model.clone(),
                    language: cfg.voice_clone.language.clone(),
                };
                if cand.validate().is_ok() {
                    candidates.push(cand);
                    if candidates.len() >= max_candidates.max(1) {
                        break 'gaps;
                    }
                }
                chunk_start = chunk_end;
            }
        }
    }

    // Fallback for undecodable inputs only: keep one reviewable candidate so
    // the workflow stays deterministic instead of erroring out. When
    // extraction succeeded but every gap fell outside the duration window,
    // an empty set is the honest answer (no duration metadata to report).
    if candidates.is_empty() && timeline.is_none() {
        let mut meta = HashMap::default();
        meta.insert("fallback".to_string(), serde_json::json!(true));
        let candidate = VoiceReference {
            id: format!("{character}_candidate_1"),
            character_name: character.to_string(),
            sample_paths: vec![input.to_path_buf()],
            metadata: meta,
            created_at: None,
            runtime: cfg.voice_clone.runtime.clone(),
            family: cfg.voice_clone.family.clone(),
            model: cfg.voice_clone.model.clone(),
            language: cfg.voice_clone.language.clone(),
        };
        candidate.validate().map_err(|e| anyhow::anyhow!(e))?;
        candidates.push(candidate);
    }

    Ok(candidates)
}

/// Slice `[start_ms, end_ms)` from the decoded input and persist it as a
/// mono 16-bit WAV next to the source file.
fn write_candidate_clip(
    input: &Path,
    character: &str,
    idx: usize,
    start_ms: u64,
    end_ms: u64,
) -> Result<std::path::PathBuf> {
    let (mono, rate) =
        crate::pipeline::decode::decode_audio(input, AnalysisConfig::default().sample_rate_hz)
            .with_context(|| format!("failed to decode {} for clip slicing", input.display()))?;
    let rate = rate.max(1);
    let start = (start_ms * u64::from(rate) / 1000).min(mono.len() as u64) as usize;
    let end = (end_ms * u64::from(rate) / 1000).min(mono.len() as u64) as usize;
    if end <= start {
        anyhow::bail!("empty candidate interval {start_ms}..{end_ms}");
    }
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("movie");
    let ext = input
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("audio");
    let clip_name = format!("{stem}.{ext}.{character}.candidate{idx}.wav");
    let clip_path = input.with_file_name(clip_name);
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&clip_path, spec)
        .with_context(|| format!("failed to create candidate clip {}", clip_path.display()))?;
    for sample in &mono[start..end] {
        writer
            .write_sample((sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16)
            .with_context(|| format!("failed to write candidate clip {}", clip_path.display()))?;
    }
    writer
        .finalize()
        .with_context(|| format!("failed to finalize candidate clip {}", clip_path.display()))?;
    Ok(clip_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn write_test_wav(path: &std::path::Path, secs: u64) {
        // Speech bursts (440 Hz) separated by silence: silence becomes
        // non-voice segments, the bursts between them become candidates.
        // Bursts at 1-2.5s, 3.5-4.5s, 8-14s (last gap: 6s for the default
        // 6s floor); pure silence yields no speech gaps at all.
        let rate = 16_000u32;
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).expect("create test wav");
        for n in 0..rate as u64 * secs {
            let t = n as f32 / rate as f32;
            let speech =
                (1.0..2.5).contains(&t) || (3.5..4.5).contains(&t) || (8.0..14.0).contains(&t);
            let sample = if speech {
                (f32::sin(t * 440.0 * std::f32::consts::TAU) * 16_000.0) as i16
            } else {
                0i16
            };
            writer.write_sample(sample).expect("write sample");
        }
        writer.finalize().expect("finalize test wav");
    }

    fn write_long_speech_wav(path: &std::path::Path, secs: u64, speech_end_secs: u64) {
        // Continuous "speech" tone from 0 up to `speech_end_secs`, then true
        // silence to `secs`. The silent tail must clear `min_non_voice_ms`
        // (10s default) so the extractor reports it as one non-voice
        // segment, leaving a single long dialogue gap in front of it.
        let rate = 16_000u32;
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).expect("create test wav");
        for n in 0..rate as u64 * secs {
            let t = n as f32 / rate as f32;
            let sample = if (t as u64) < speech_end_secs {
                (f32::sin(t * 440.0 * std::f32::consts::TAU) * 16_000.0) as i16
            } else {
                0i16
            };
            writer.write_sample(sample).expect("write sample");
        }
        writer.finalize().expect("finalize test wav");
    }

    #[test]
    fn long_speech_span_is_chunked_into_multiple_candidates() -> anyhow::Result<()> {
        let dir = tempfile::tempdir().expect("tempdir");
        let input = dir.path().join("movie.wav");
        // 41s of continuous "speech" followed by 11s of silence: one
        // dialogue gap far longer than MAX_CANDIDATE_MS (15s).
        write_long_speech_wav(&input, 52, 41);
        let cfg = AppConfig::default();
        let cands = extract_candidates(&input, &cfg, "alice")?;

        assert!(
            cands.len() >= 2,
            "a >15s span must yield multiple candidates, got {}",
            cands.len()
        );
        for cand in &cands {
            let duration_ms = cand
                .metadata
                .get("duration_ms")
                .and_then(serde_json::Value::as_u64)
                .expect("duration_ms present");
            assert!(duration_ms <= MAX_CANDIDATE_MS, "chunk exceeds cap");
        }
        // Chunks must tile the span back-to-back with no gaps between them.
        let mut sorted: Vec<(u64, u64)> = cands
            .iter()
            .map(|c| {
                let start = c.metadata["start_ms"].as_u64().unwrap();
                let end = c.metadata["end_ms"].as_u64().unwrap();
                (start, end)
            })
            .collect();
        sorted.sort_unstable();
        for pair in sorted.windows(2) {
            assert_eq!(pair[0].1, pair[1].0, "chunks must be contiguous");
        }
        Ok(())
    }

    #[test]
    fn extract_valid() -> anyhow::Result<()> {
        let dir = tempfile::tempdir().expect("tempdir");
        let input = dir.path().join("movie.wav");
        write_test_wav(&input, 30);
        let cfg = AppConfig::default();
        let cands = extract_candidates(&input, &cfg, "alice")?;
        assert!(!cands.is_empty());
        assert_eq!(cands[0].character_name, "alice");
        // Clips are sliced intervals, not the whole movie.
        for cand in &cands {
            if !cand.metadata.contains_key("fallback") {
                let clip = cand.sample_paths.first().expect("clip path");
                assert_ne!(clip, &input, "candidate must point at its clip");
                assert!(clip.exists(), "clip must be written");
            }
        }
        Ok(())
    }

    #[test]
    fn unsupported_family_rejected() {
        let mut cfg = AppConfig::default();
        cfg.voice_clone.family = "unknown_family".to_string();
        assert!(extract_candidates(&PathBuf::from("testdata/a.mkv"), &cfg, "bob").is_err());
    }

    #[test]
    fn empty_character_rejected() {
        let cfg = AppConfig::default();
        assert!(extract_candidates(&PathBuf::from("testdata/a.mkv"), &cfg, "").is_err());
    }

    #[test]
    fn dotted_filenames_accepted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let input = dir.path().join("movie..final.wav");
        write_test_wav(&input, 30);
        let cfg = AppConfig::default();
        // `..` inside a filename is not a parent-dir escape.
        assert!(extract_candidates(&input, &cfg, "alice").is_ok());
    }

    #[test]
    fn configured_minimum_sample_seconds_enforced() {
        let dir = tempfile::tempdir().expect("tempdir");
        let input = dir.path().join("movie.wav");
        write_test_wav(&input, 30);
        let mut cfg = AppConfig::default();
        // Default floor is 6s; raising it must not admit shorter gaps.
        cfg.voice_clone.min_sample_seconds = 60.0;
        let cands = extract_candidates(&input, &cfg, "alice").expect("run");
        for cand in &cands {
            if let Some(serde_json::Value::Number(ms)) = cand.metadata.get("duration_ms") {
                assert!(
                    ms.as_u64().unwrap_or(u64::MAX) >= 60_000
                        || cand.metadata.contains_key("fallback")
                );
            }
        }
    }

    #[test]
    fn one_second_minimum_admitted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let input = dir.path().join("movie.wav");
        write_test_wav(&input, 30);
        let mut cfg = AppConfig::default();
        cfg.voice_clone.min_sample_seconds = 1.0;
        // Schema floor (1s) applies: must not error, whatever gaps decode yields.
        assert!(extract_candidates(&input, &cfg, "alice").is_ok());
    }

    #[test]
    fn traversal_inputs_rejected() {
        let cfg = AppConfig::default();
        assert!(extract_candidates(&PathBuf::from("../secret/movie.wav"), &cfg, "alice").is_err());
        assert!(extract_candidates(&PathBuf::from("testdata/a.wav"), &cfg, "../../pwn").is_err());
        // Absolute paths are accepted: reads use the caller's own privileges.
        let abs = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/movie.mkv"));
        assert!(extract_candidates(&abs, &cfg, "alice").is_ok());
    }
}
