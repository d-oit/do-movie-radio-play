use crate::app_config::AppConfig;

const VALID_AUDIO_CPP_MODES: [&str; 3] = ["auto", "local", "remote"];
const VALID_LOCAL_MODES: [&str; 2] = ["server", "cli"];
const VALID_BACKENDS: [&str; 6] = ["best", "cpu", "cuda", "vulkan", "metal", "hip"];
const VALID_NARRATOR_BACKENDS: [&str; 4] = ["openai", "ollama_local", "anthropic", "mistral_local"];

pub fn validate_app_config(cfg: &AppConfig) -> Result<(), String> {
    // voice.audio_cpp
    let ac = &cfg.voice.audio_cpp;
    if !VALID_AUDIO_CPP_MODES.contains(&ac.mode.as_str()) {
        return Err(format!("invalid voice.audio_cpp.mode: {}", ac.mode));
    }
    if !VALID_BACKENDS.contains(&ac.backend.as_str()) {
        return Err(format!("invalid voice.audio_cpp.backend: {}", ac.backend));
    }
    if !VALID_LOCAL_MODES.contains(&ac.local.mode.as_str()) {
        return Err(format!(
            "invalid voice.audio_cpp.local.mode: {}",
            ac.local.mode
        ));
    }
    if ac.timeout_secs == 0 || ac.timeout_secs > 3600 {
        return Err(format!(
            "voice.audio_cpp.timeout_secs out of range: {}",
            ac.timeout_secs
        ));
    }
    if !ac.local.server_url.is_empty() {
        validate_url(&ac.local.server_url, "voice.audio_cpp.local.server_url")?;
    }
    if !ac.remote.server_url.is_empty() {
        validate_url(&ac.remote.server_url, "voice.audio_cpp.remote.server_url")?;
        if ac.remote.server_url.starts_with("http://") {
            return Err("voice.audio_cpp.remote.server_url must be https".to_string());
        }
    }
    if ac.remote.auth_env.is_some() && ac.remote.server_url.is_empty() {
        return Err("auth_env requires server_url".to_string());
    }
    // gpu policy
    if cfg.voice.gpu_policy.max_cost_per_job < 0.0
        || !cfg.voice.gpu_policy.max_cost_per_job.is_finite()
    {
        return Err("max_cost_per_job must be finite >=0".to_string());
    }
    if cfg.voice.gpu_policy.max_cost_per_day < 0.0
        || !cfg.voice.gpu_policy.max_cost_per_day.is_finite()
    {
        return Err("max_cost_per_day must be finite >=0".to_string());
    }
    // gpu pool
    for ep in &cfg.voice.gpu_pool {
        if ep.name.trim().is_empty() {
            return Err("gpu_pool name must not be empty".to_string());
        }
        validate_url(&ep.url, "gpu_pool.url")?;
        if ep.cost_per_hour < 0.0 || !ep.cost_per_hour.is_finite() {
            return Err("gpu_pool cost_per_hour must be finite >=0".to_string());
        }
    }
    if !VALID_AUDIO_CPP_MODES.contains(&cfg.voice_clone.routing.mode.as_str()) {
        return Err(format!(
            "invalid voice_clone.routing.mode: {}",
            cfg.voice_clone.routing.mode
        ));
    }
    if cfg.voice_clone.min_sample_seconds < 1.0 {
        return Err("voice_clone.min_sample_seconds must be >=1".to_string());
    }
    // narrator
    if !VALID_NARRATOR_BACKENDS.contains(&cfg.narrator.backend.as_str()) {
        return Err(format!(
            "invalid narrator.backend: {}",
            cfg.narrator.backend
        ));
    }
    if cfg.narrator.max_tokens == 0 {
        return Err("narrator.max_tokens must be >0".to_string());
    }
    if !(0.0..=2.0).contains(&cfg.narrator.temperature) {
        return Err("narrator.temperature must be in [0,2]".to_string());
    }
    if cfg.narrator.prompt_template.trim().is_empty() {
        return Err("narrator.prompt_template must not be empty".to_string());
    }
    // Check conflicting local/remote
    if ac.mode == "local" && !ac.remote.server_url.is_empty() && cfg.voice.gpu_policy.allow_paid {
        // allowed but warn — no error
    }
    Ok(())
}

fn validate_url(s: &str, field: &str) -> Result<(), String> {
    let parsed = url::Url::parse(s).map_err(|e| format!("invalid {field} {s:?}: {e}"))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(format!("{field} must be http or https: {s}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_config::AppConfig;

    #[test]
    fn default_valid() {
        let cfg = AppConfig::default();
        assert!(validate_app_config(&cfg).is_ok());
    }

    #[test]
    fn invalid_mode_rejected() {
        let mut cfg = AppConfig::default();
        cfg.voice.audio_cpp.mode = "bogus".to_string();
        assert!(validate_app_config(&cfg).is_err());
    }

    #[test]
    fn remote_http_rejected() {
        let mut cfg = AppConfig::default();
        cfg.voice.audio_cpp.remote.server_url = "http://gpu.example.com".to_string();
        assert!(validate_app_config(&cfg).is_err());
    }

    #[test]
    fn cost_negative_rejected() {
        let mut cfg = AppConfig::default();
        cfg.voice.gpu_policy.max_cost_per_job = -1.0;
        assert!(validate_app_config(&cfg).is_err());
    }
}
