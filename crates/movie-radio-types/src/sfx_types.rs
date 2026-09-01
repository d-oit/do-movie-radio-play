use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum SfxTrigger {
    #[default]
    None,
    AutoSelect {
        tags: Vec<String>,
        mood: Option<String>,
    },
    Specific {
        sfx_id: String,
    },
    AiGenerate {
        prompt: String,
        duration_secs: f32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SfxLicense {
    Cc0,
    CcBy,
    CcByNc,
    CcBySa,
    CcByNcSa,
    SamplingPlus,
    Unknown(String),
}

impl SfxLicense {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "cc0" | "https://creativecommons.org/publicdomain/zero/1.0/" => Self::Cc0,
            "cc-by"
            | "cc by"
            | "attribution"
            | "https://creativecommons.org/licenses/by/3.0/"
            | "https://creativecommons.org/licenses/by/4.0/" => Self::CcBy,
            "cc-by-nc" | "cc by-nc" => Self::CcByNc,
            "cc-by-sa" | "cc by-sa" => Self::CcBySa,
            "cc-by-nc-sa" | "cc by-nc-sa" => Self::CcByNcSa,
            "sampling+" => Self::SamplingPlus,
            other => Self::Unknown(other.to_string()),
        }
    }

    pub fn is_allowed(&self, allowed: &[String]) -> bool {
        if allowed.is_empty() {
            return true;
        }
        let key = match self {
            Self::Cc0 => "cc0",
            Self::CcBy => "cc-by",
            Self::CcByNc => "cc-by-nc",
            Self::CcBySa => "cc-by-sa",
            Self::CcByNcSa => "cc-by-nc-sa",
            Self::SamplingPlus => "sampling+",
            Self::Unknown(s) => s.as_str(),
        };
        allowed.iter().any(|a| a.to_lowercase() == key)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SfxQuery {
    pub tags: Vec<String>,
    pub mood: Option<String>,
    pub duration_secs: Option<f32>,
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SfxCandidate {
    pub id: String,
    pub path_or_url: String,
    pub license: SfxLicense,
    pub duration_secs: Option<f32>,
    pub tags: Vec<String>,
    pub provider: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SfxProviderCapabilities {
    pub supports_search: bool,
    pub supports_fetch: bool,
    pub supports_generate: bool,
    pub requires_network: bool,
    pub is_paid: bool,
}

impl Default for SfxProviderCapabilities {
    fn default() -> Self {
        Self {
            supports_search: true,
            supports_fetch: true,
            supports_generate: false,
            requires_network: false,
            is_paid: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoundEffectsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub local: LocalSfxConfig,
    #[serde(default)]
    pub freesound: FreesoundConfig,
    #[serde(default)]
    pub ai_generate: AiGenerateConfig,
}

impl Default for SoundEffectsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            local: LocalSfxConfig::default(),
            freesound: FreesoundConfig::default(),
            ai_generate: AiGenerateConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSfxConfig {
    #[serde(default)]
    pub root: String,
    #[serde(default = "default_true")]
    pub recursive: bool,
    #[serde(default = "default_max_file_bytes")]
    pub max_file_bytes: u64,
}

impl Default for LocalSfxConfig {
    fn default() -> Self {
        Self {
            root: String::default(),
            recursive: true,
            max_file_bytes: default_max_file_bytes(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreesoundConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub api_key_env: String,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_allowed_licenses")]
    pub allowed_licenses: Vec<String>,
    #[serde(default = "default_max_audio_bytes")]
    pub max_audio_bytes: u64,
}

impl Default for FreesoundConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_key_env: "FREESOUND_API_KEY".to_string(),
            timeout_secs: default_timeout_secs(),
            allowed_licenses: default_allowed_licenses(),
            max_audio_bytes: default_max_audio_bytes(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiGenerateConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_ai_provider")]
    pub provider: String,
    #[serde(default = "default_ai_mode")]
    pub mode: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_ai_endpoint")]
    pub endpoint_url: String,
    #[serde(default)]
    pub auth_env: Option<String>,
    #[serde(default = "default_max_prompt_len")]
    pub max_prompt_len: usize,
    #[serde(default = "default_max_audio_bytes")]
    pub max_audio_bytes: u64,
    #[serde(default)]
    pub gpu_pool: Vec<crate::config::GpuPoolEndpoint>,
    #[serde(default)]
    pub gpu_policy: crate::config::GpuPolicyConfig,
}

impl Default for AiGenerateConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_ai_provider(),
            mode: default_ai_mode(),
            model: String::default(),
            timeout_secs: default_timeout_secs(),
            endpoint_url: default_ai_endpoint(),
            auth_env: None,
            max_prompt_len: default_max_prompt_len(),
            max_audio_bytes: default_max_audio_bytes(),
            gpu_pool: Vec::new(),
            gpu_policy: crate::config::GpuPolicyConfig::default(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_timeout_secs() -> u64 {
    300
}

fn default_max_prompt_len() -> usize {
    1000
}

fn default_max_audio_bytes() -> u64 {
    20_000_000
}

fn default_max_file_bytes() -> u64 {
    50_000_000
}

fn default_ai_provider() -> String {
    "configured_endpoint".to_string()
}

fn default_ai_mode() -> String {
    "auto".to_string()
}

fn default_ai_endpoint() -> String {
    String::default()
}

fn default_allowed_licenses() -> Vec<String> {
    vec!["cc0".to_string(), "cc-by".to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sfx_trigger_serde_roundtrip() {
        let t = SfxTrigger::AutoSelect {
            tags: vec!["ambience".to_string()],
            mood: Some("tense".to_string()),
        };
        let s = serde_json::to_string(&t).expect("serialize");
        let d: SfxTrigger = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(t, d);
    }

    #[test]
    fn license_allowed() {
        assert!(SfxLicense::Cc0.is_allowed(&[]));
        assert!(SfxLicense::Cc0.is_allowed(&["cc0".to_string()]));
        assert!(!SfxLicense::CcByNc.is_allowed(&["cc0".to_string()]));
        assert!(SfxLicense::from_str("cc0") == SfxLicense::Cc0);
    }

    #[test]
    fn sound_effects_config_default() {
        let cfg = SoundEffectsConfig::default();
        assert!(cfg.enabled);
        assert!(!cfg.freesound.enabled);
        assert!(!cfg.ai_generate.enabled);
    }
}
