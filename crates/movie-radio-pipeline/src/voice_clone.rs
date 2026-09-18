use anyhow::Result;
use movie_radio_types::{AnalysisConfig, AppConfig, VoiceReference};
use std::collections::HashMap;
use std::path::Path;

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

    // Configured minimum wins over the hardcoded floor: changing
    // `voice_clone.min_sample_seconds` must change what is accepted.
    let min_ms = ((f64::from(cfg.voice_clone.min_sample_seconds) * 1000.0).round() as u64)
        .max(MIN_CANDIDATE_MS);
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
        // segment end. `extract_timeline` errors on undecodable input, in
        // which case the fallback candidate below still applies.
        if let Ok((mono, rate)) =
            crate::pipeline::decode::decode_audio(input, AnalysisConfig::default().sample_rate_hz)
        {
            let total_ms = mono.len() as u64 * 1000 / u64::from(rate.max(1));
            if total_ms > last_end_ms {
                gaps.push((last_end_ms, total_ms));
            }
        }
        for (idx, (start, end)) in gaps.iter().enumerate() {
            let duration_ms = end.saturating_sub(*start);
            if (min_ms..=MAX_CANDIDATE_MS).contains(&duration_ms) {
                let mut meta = HashMap::default();
                meta.insert("start_ms".to_string(), serde_json::json!(start));
                meta.insert("end_ms".to_string(), serde_json::json!(end));
                meta.insert("duration_ms".to_string(), serde_json::json!(duration_ms));

                let cand = VoiceReference {
                    id: format!("{character}_candidate_{}", idx + 1),
                    character_name: character.to_string(),
                    sample_paths: vec![input.to_path_buf()],
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
                        break;
                    }
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn extract_valid() -> anyhow::Result<()> {
        let cfg = AppConfig::default();
        let cands = extract_candidates(&PathBuf::from("testdata/movie.mkv"), &cfg, "alice")?;
        assert!(!cands.is_empty());
        assert_eq!(cands[0].character_name, "alice");
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
        let cfg = AppConfig::default();
        // `..` inside a filename is not a parent-dir escape.
        assert!(
            extract_candidates(&PathBuf::from("testdata/movie..final.mkv"), &cfg, "alice").is_ok()
        );
    }

    #[test]
    fn configured_minimum_sample_seconds_enforced() {
        let mut cfg = AppConfig::default();
        // Default floor is 6s; raising it must not admit shorter gaps.
        cfg.voice_clone.min_sample_seconds = 60.0;
        let cands =
            extract_candidates(&PathBuf::from("testdata/movie.mkv"), &cfg, "alice").expect("run");
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
    fn traversal_inputs_rejected() {
        let cfg = AppConfig::default();
        assert!(extract_candidates(&PathBuf::from("../secret/movie.mkv"), &cfg, "alice").is_err());
        assert!(extract_candidates(&PathBuf::from("testdata/a.mkv"), &cfg, "../../pwn").is_err());
        // Absolute paths are accepted: reads use the caller's own privileges.
        let abs = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/movie.mkv"));
        assert!(extract_candidates(&abs, &cfg, "alice").is_ok());
    }
}
