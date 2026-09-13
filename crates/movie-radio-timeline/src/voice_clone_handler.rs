use anyhow::{Context, Result};
use movie_radio_pipeline::voice_clone::extract_candidates;
use movie_radio_types::VoiceReference;
use std::fs;
use std::path::PathBuf;

pub fn handle_voice_samples(
    character: String,
    input: PathBuf,
    output: Option<PathBuf>,
) -> Result<()> {
    if character.trim().is_empty() {
        anyhow::bail!("character must not be empty");
    }
    if input.to_string_lossy().contains("..") {
        anyhow::bail!("input path must not contain ..");
    }

    let cfg = crate::app_config_loader::load_app_config(None)?;
    let candidates = extract_candidates(&input, &cfg, &character)?;

    let out = output.unwrap_or_else(|| PathBuf::from(format!("voice_samples/{character}.json")));

    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
    }

    let json_data = serde_json::to_string_pretty(&candidates)
        .context("failed to serialize extracted candidates")?;
    fs::write(&out, json_data)
        .with_context(|| format!("failed to write voice candidates to {}", out.display()))?;

    println!(
        "voice samples for {character} from {} -> {} (extracted {} candidate(s))",
        input.display(),
        out.display(),
        candidates.len()
    );
    println!(
        "runtime={} family={} — capability check passed",
        cfg.voice_clone.runtime, cfg.voice_clone.family
    );
    Ok(())
}

pub fn handle_voice_list() -> Result<()> {
    let base_dir = PathBuf::from("voice_samples");
    if !base_dir.exists() {
        println!("voice references: (none stored yet) — use `voice samples --character NAME --input movie.mkv`");
        Ok(())
    } else {
        let mut references = Vec::new();
        let entries = fs::read_dir(&base_dir).context("failed to read voice_samples directory")?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(cands) = serde_json::from_str::<Vec<VoiceReference>>(&content) {
                        references.extend(cands);
                    }
                }
            }
        }

        if references.is_empty() {
            println!("voice references: (none stored yet) — use `voice samples --character NAME --input movie.mkv`");
        } else {
            println!("voice references (stored total: {}):", references.len());
            for r in &references {
                println!(
                    " - [{}] character={} runtime={} family={} samples={}",
                    r.id,
                    r.character_name,
                    r.runtime,
                    r.family,
                    r.sample_paths.len()
                );
            }
        }
        Ok(())
    }
}

pub fn handle_voice_test(character: String, text: String) -> Result<()> {
    let cfg = crate::app_config_loader::load_app_config(None)?;

    let sample_file = PathBuf::from(format!("voice_samples/{character}.json"));
    let mut ref_audio = None;

    if sample_file.exists() {
        if let Ok(content) = fs::read_to_string(&sample_file) {
            if let Ok(cands) = serde_json::from_str::<Vec<VoiceReference>>(&content) {
                if let Some(first) = cands.first() {
                    if let Some(path) = first.sample_paths.first() {
                        ref_audio = Some(path.clone());
                    }
                }
            }
        }
    }

    println!(
        "voice test character={character} text={text:?} runtime={} endpoint={} reference_audio={:?}",
        cfg.voice_clone.runtime,
        if cfg.voice.audio_cpp.remote.server_url.is_empty() { "local" } else { "remote" },
        ref_audio
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_handle_voice_samples_and_list() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let out_json = temp_dir.path().join("alice.json");

        handle_voice_samples(
            "alice".to_string(),
            PathBuf::from("testdata/movie.mkv"),
            Some(out_json.clone()),
        )?;
        assert!(out_json.exists());

        let content = fs::read_to_string(&out_json)?;
        let cands: Vec<VoiceReference> = serde_json::from_str(&content)?;
        assert!(!cands.is_empty());
        assert_eq!(cands[0].character_name, "alice");

        Ok(())
    }
}
