use anyhow::Result;
use movie_radio_types::{AppConfig, VoiceReference};

pub fn extract_candidates(
    input: &std::path::Path,
    cfg: &AppConfig,
    character: &str,
) -> Result<Vec<VoiceReference>> {
    if character.trim().is_empty() {
        anyhow::bail!("character must not be empty");
    }
    let candidate = VoiceReference {
        id: format!("{character}_candidate_1"),
        character_name: character.to_string(),
        sample_paths: vec![input.to_path_buf()],
        metadata: std::collections::HashMap::default(),
        created_at: None,
        runtime: cfg.voice_clone.runtime.clone(),
        family: cfg.voice_clone.family.clone(),
        model: cfg.voice_clone.model.clone(),
        language: cfg.voice_clone.language.clone(),
    };
    candidate.validate().map_err(|e| anyhow::anyhow!(e))?;
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
    Ok(vec![candidate])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn extract_valid() -> anyhow::Result<()> {
        let cfg = AppConfig::default();
        let cands = extract_candidates(&PathBuf::from("testdata/movie.mkv"), &cfg, "alice")?;
        assert_eq!(cands.len(), 1);
        Ok(())
    }
    #[test]
    fn unsupported_family_rejected() {
        let mut cfg = AppConfig::default();
        cfg.voice_clone.family = "unknown_family".to_string();
        assert!(extract_candidates(&PathBuf::from("testdata/a.mkv"), &cfg, "bob").is_err());
    }
}
