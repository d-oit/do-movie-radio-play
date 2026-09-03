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
            let s = p.to_string_lossy();
            if s.contains("..") {
                return Err(format!("sample path must not contain ..: {s}"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
