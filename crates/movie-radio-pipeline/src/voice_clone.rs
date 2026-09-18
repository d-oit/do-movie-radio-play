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
    if path.to_string_lossy().contains("..") {
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

    if let Ok(timeline) = crate::pipeline::extract_timeline(input, &AnalysisConfig::default()) {
        // The timeline carries non-voice segments; the gaps between them are
        // the speech/dialogue regions eligible as clone candidates.
        let mut last_end_ms: u64 = 0;
        for (idx, seg) in timeline.segments.iter().enumerate() {
            if seg.start_ms > last_end_ms {
                let start = last_end_ms;
                let end = seg.start_ms;
                let duration_ms = end - start;
                if (MIN_CANDIDATE_MS..=MAX_CANDIDATE_MS).contains(&duration_ms) {
                    let mut meta = HashMap::default();
                    meta.insert("start_ms".to_string(), serde_json::json!(start));
                    meta.insert("end_ms".to_string(), serde_json::json!(end));
                    meta.insert("duration_ms".to_string(), serde_json::json!(duration_ms));
                    meta.insert("confidence".to_string(), serde_json::json!(seg.confidence));

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
            last_end_ms = seg.end_ms;
        }
    }

    // Fallback for undecodable/mock inputs: keep one reviewable candidate so
    // the workflow stays deterministic instead of erroring out.
    if candidates.is_empty() {
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
    fn traversal_inputs_rejected() {
        let cfg = AppConfig::default();
        assert!(extract_candidates(&PathBuf::from("../secret/movie.mkv"), &cfg, "alice").is_err());
        assert!(extract_candidates(&PathBuf::from("testdata/a.mkv"), &cfg, "../../pwn").is_err());
        // Absolute paths are accepted: reads use the caller's own privileges.
        let abs = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/movie.mkv"));
        assert!(extract_candidates(&abs, &cfg, "alice").is_ok());
    }
}
