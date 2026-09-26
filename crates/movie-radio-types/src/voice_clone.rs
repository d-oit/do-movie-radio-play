use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceReference {
    pub id: String,
    pub character_name: String,
    pub sample_paths: Vec<PathBuf>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub runtime: String,
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VoiceReferenceParams {
    pub reference_id: Option<String>,
    pub reference_audio: Option<PathBuf>,
    pub voice_id: Option<String>,
}

impl VoiceReference {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("voice reference id must not be empty".to_string());
        }
        if self.character_name.trim().is_empty() {
            return Err("character_name must not be empty".to_string());
        }
        if self.sample_paths.is_empty() {
            return Err("sample_paths must not be empty".to_string());
        }
        for p in &self.sample_paths {
            if p.components().any(|c| c == std::path::Component::ParentDir) {
                return Err(format!(
                    "sample path must not contain ..: {}",
                    p.to_string_lossy()
                ));
            }
        }
        Ok(())
    }
}

/// Load the first usable clone reference from a `voice samples` JSON file.
///
/// Every failure is fatal: a clone test without a clone reference silently
/// degrades to plain synthesis, so stale entries are skipped and an empty,
/// malformed, or reference-less file errors with a recovery hint.
pub fn load_reference_audio(sample_file: &std::path::Path, character: &str) -> Result<PathBuf> {
    let content = std::fs::read_to_string(sample_file).with_context(|| {
        format!(
            "no voice samples for character '{character}': cannot read {} — run `voice samples --character {character} --input <movie>` first or pass --samples-from <file>",
            sample_file.display()
        )
    })?;
    let cands: Vec<VoiceReference> = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse voice samples {}", sample_file.display()))?;
    if cands.is_empty() {
        anyhow::bail!(
            "voice samples {} contain no candidates — re-run `voice samples --character {character} --input <movie>`",
            sample_file.display()
        );
    }
    // First candidate whose stored reference still points at a real file;
    // stale entries (moved/deleted clips) are skipped, and a file with no
    // usable reference at all fails instead of testing the wrong voice.
    // Candidates are scoped to the requested character: `--samples-from`
    // accepts an arbitrary file, so without this check
    // `--character alice --samples-from bob.json` would clone Bob's voice
    // while reporting Alice.
    let matching: Vec<&VoiceReference> = cands
        .iter()
        .filter(|c| c.validate().is_ok() && c.character_name == character)
        .collect();
    if !cands.iter().any(|c| c.character_name == character) {
        anyhow::bail!(
            "voice samples {} contain no candidates for character '{character}' — re-run `voice samples --character {character} --input <movie>`",
            sample_file.display()
        );
    }
    matching
        .iter()
        .flat_map(|c| c.sample_paths.iter())
        .find(|p| p.is_file())
        .cloned()
        .with_context(|| {
            format!(
                "voice samples {} contain no usable reference audio for character '{character}' (all sample paths missing or stale) — re-run `voice samples --character {character} --input <movie>`",
                sample_file.display()
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotted_filename_accepted() {
        let vr = VoiceReference {
            id: "protagonist_v1".to_string(),
            character_name: "protagonist".to_string(),
            sample_paths: vec![PathBuf::from("testdata/movie..final.mkv")],
            metadata: HashMap::new(),
            created_at: None,
            runtime: "audio_cpp".to_string(),
            family: "qwen3_tts".to_string(),
            model: "models/Qwen3-TTS-12Hz-1.7B-Base".to_string(),
            language: "de".to_string(),
        };
        assert!(vr.validate().is_ok());
    }

    #[test]
    fn voice_reference_validation() {
        let vr = VoiceReference {
            id: "protagonist_v1".to_string(),
            character_name: "protagonist".to_string(),
            sample_paths: vec![PathBuf::from("testdata/sample.wav")],
            metadata: HashMap::new(),
            created_at: None,
            runtime: "audio_cpp".to_string(),
            family: "qwen3_tts".to_string(),
            model: "models/Qwen3-TTS-12Hz-1.7B-Base".to_string(),
            language: "de".to_string(),
        };
        assert!(vr.validate().is_ok());
    }

    #[test]
    fn empty_id_rejected() {
        let vr = VoiceReference {
            id: "".to_string(),
            character_name: "x".to_string(),
            sample_paths: vec![PathBuf::from("testdata/a.wav")],
            metadata: HashMap::new(),
            created_at: None,
            runtime: "audio_cpp".to_string(),
            family: "qwen3_tts".to_string(),
            model: "".to_string(),
            language: "de".to_string(),
        };
        assert!(vr.validate().is_err());
    }

    fn write_reference_file(dir: &std::path::Path, paths: &[PathBuf]) -> PathBuf {
        let file = dir.join("refs.json");
        let refs = vec![VoiceReference {
            id: "alice_candidate_1".to_string(),
            character_name: "alice".to_string(),
            sample_paths: paths.to_vec(),
            metadata: HashMap::new(),
            created_at: None,
            runtime: "audio_cpp".to_string(),
            family: "qwen3_tts".to_string(),
            model: "models/Qwen3-TTS-12Hz-1.7B-Base".to_string(),
            language: "de".to_string(),
        }];
        std::fs::write(&file, serde_json::to_string_pretty(&refs).unwrap()).unwrap();
        file
    }

    #[test]
    fn missing_samples_file_fails() {
        let dir = tempfile::TempDir::new().unwrap();
        let missing = dir.path().join("missing.json");
        assert!(load_reference_audio(&missing, "alice").is_err());
    }

    #[test]
    fn other_character_reference_rejected() -> Result<()> {
        let dir = tempfile::TempDir::new()?;
        let live = dir.path().join("bob.wav");
        std::fs::write(&live, b"RIFF")?;
        let file = write_reference_file(dir.path(), &[live]);
        assert!(load_reference_audio(&file, "bob").is_err());
        Ok(())
    }

    #[test]
    fn stale_reference_fails_instead_of_plain_synthesis() -> Result<()> {
        let dir = tempfile::TempDir::new()?;
        let file = write_reference_file(dir.path(), &[PathBuf::from("voice_samples/gone.wav")]);
        assert!(load_reference_audio(&file, "alice").is_err());
        Ok(())
    }

    #[test]
    fn skips_stale_entry_for_live_reference() -> Result<()> {
        let dir = tempfile::TempDir::new()?;
        let live = dir.path().join("live.wav");
        std::fs::write(&live, b"RIFF")?;
        let file = dir.path().join("refs.json");
        let refs = vec![
            VoiceReference {
                id: "alice_candidate_1".to_string(),
                character_name: "alice".to_string(),
                sample_paths: vec![PathBuf::from("voice_samples/gone.wav")],
                metadata: HashMap::new(),
                created_at: None,
                runtime: "audio_cpp".to_string(),
                family: "qwen3_tts".to_string(),
                model: "models/Qwen3-TTS-12Hz-1.7B-Base".to_string(),
                language: "de".to_string(),
            },
            VoiceReference {
                id: "alice_candidate_2".to_string(),
                character_name: "alice".to_string(),
                sample_paths: vec![live.clone()],
                metadata: HashMap::new(),
                created_at: None,
                runtime: "audio_cpp".to_string(),
                family: "qwen3_tts".to_string(),
                model: "models/Qwen3-TTS-12Hz-1.7B-Base".to_string(),
                language: "de".to_string(),
            },
        ];
        std::fs::write(&file, serde_json::to_string_pretty(&refs)?)?;
        assert_eq!(load_reference_audio(&file, "alice")?, live);
        Ok(())
    }
}
