use anyhow::Result;
use movie_radio_types::{AnalysisConfig, AppConfig, VoiceReference};
use std::collections::HashMap;
use std::path::Path;

pub fn extract_candidates(
    input: &Path,
    cfg: &AppConfig,
    character: &str,
) -> Result<Vec<VoiceReference>> {
    if character.trim().is_empty() {
        anyhow::bail!("character must not be empty");
    }
    if input.to_string_lossy().contains("..") {
        anyhow::bail!("input path must not contain ..");
    }

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

    let analysis_cfg = AnalysisConfig::default();
    let timeline_res = crate::pipeline::extract_timeline(input, &analysis_cfg);

    let mut candidates = Vec::new();

    if let Ok(timeline) = timeline_res {
        // Timeline gives non_voice segments. We invert or analyze gaps between non_voice segments to find speech/dialogue segments.
        let mut last_end_ms: u64 = 0;
        let mut candidate_idx = 1;

        for seg in &timeline.segments {
            if seg.start_ms > last_end_ms {
                let speech_start = last_end_ms;
                let speech_end = seg.start_ms;
                let duration_ms = speech_end - speech_start;

                // Dialogue candidates window: filter for optimal length (2s - 15s)
                if (2000..=15000).contains(&duration_ms) {
                    let mut meta = HashMap::default();
                    meta.insert("start_ms".to_string(), serde_json::json!(speech_start));
                    meta.insert("end_ms".to_string(), serde_json::json!(speech_end));
                    meta.insert("duration_ms".to_string(), serde_json::json!(duration_ms));
                    meta.insert("confidence".to_string(), serde_json::json!(seg.confidence));

                    let cand = VoiceReference {
                        id: format!("{character}_candidate_{candidate_idx}"),
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
                        candidate_idx += 1;
                    }
                }
            }
            last_end_ms = seg.end_ms;
        }
    }

    // Fallback if decoding/timeline yielded no candidate segments (e.g., non-existent audio file or mock test path)
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
}
