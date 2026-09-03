use anyhow::{bail, Context, Result};
use std::{env, fs, path::PathBuf};

use movie_radio_types::{validation::validate_app_config, AppConfig};

const ENV_AUDIO_CPP_MODE: &str = "MRPLAY_AUDIO_CPP_MODE";
const ENV_AUDIO_CPP_MODE_LEGACY: &str = "AUDIO_CPP_MODE";
const ENV_AUDIO_CPP_LOCAL_URL: &str = "MRPLAY_AUDIO_CPP_LOCAL_URL";
const ENV_AUDIO_CPP_LOCAL_URL_LEGACY: &str = "AUDIO_CPP_LOCAL_URL";
const ENV_AUDIO_CPP_REMOTE_URL: &str = "MRPLAY_AUDIO_CPP_REMOTE_URL";
const ENV_AUDIO_CPP_REMOTE_URL_LEGACY: &str = "AUDIO_CPP_REMOTE_URL";
const ENV_AUDIO_CPP_FAMILY: &str = "MRPLAY_AUDIO_CPP_FAMILY";
const ENV_AUDIO_CPP_FAMILY_LEGACY: &str = "AUDIO_CPP_FAMILY";
const ENV_AUDIO_CPP_MODEL: &str = "MRPLAY_AUDIO_CPP_MODEL";
const ENV_AUDIO_CPP_MODEL_LEGACY: &str = "AUDIO_CPP_MODEL";
const ENV_AUDIO_CPP_BACKEND: &str = "MRPLAY_AUDIO_CPP_BACKEND";
const ENV_AUDIO_CPP_BACKEND_LEGACY: &str = "AUDIO_CPP_BACKEND";
const ENV_AUDIO_CPP_LANGUAGE: &str = "MRPLAY_AUDIO_CPP_LANGUAGE";
const ENV_AUDIO_CPP_LANGUAGE_LEGACY: &str = "AUDIO_CPP_LANGUAGE";
const ENV_AUDIO_CPP_TIMEOUT: &str = "MRPLAY_AUDIO_CPP_TIMEOUT_SECS";
const ENV_AUDIO_CPP_TIMEOUT_LEGACY: &str = "AUDIO_CPP_TIMEOUT_SECS";
const ENV_GPU_PREFER_FREE: &str = "MRPLAY_GPU_PREFER_FREE";
const ENV_GPU_ALLOW_PAID: &str = "MRPLAY_GPU_ALLOW_PAID";
const ENV_GPU_MAX_JOB: &str = "MRPLAY_GPU_MAX_COST_PER_JOB";
const ENV_GPU_MAX_DAY: &str = "MRPLAY_GPU_MAX_COST_PER_DAY";
const ENV_NARRATOR_BACKEND: &str = "MRPLAY_NARRATOR_BACKEND";
const ENV_NARRATOR_LANGUAGE: &str = "MRPLAY_NARRATOR_LANGUAGE";

fn read_env(primary: &str, legacy: &str) -> Option<String> {
    env::var(primary).or_else(|_| env::var(legacy)).ok()
}

pub fn load_app_config(cli_config: Option<PathBuf>) -> Result<AppConfig> {
    let _ = dotenvy::dotenv();

    let mut cfg = AppConfig::default();

    let default_path = PathBuf::from("config/default.toml");
    if default_path.exists() {
        let data = fs::read_to_string(&default_path).context("read config/default.toml")?;
        let parsed: AppConfig = toml::from_str(&data).context("parse config/default.toml")?;
        cfg = merge_app_config(cfg, parsed);
    }

    let local_path = PathBuf::from("config/local.toml");
    if local_path.exists() {
        let data = fs::read_to_string(&local_path).context("read config/local.toml")?;
        let parsed: AppConfig = toml::from_str(&data).context("parse config/local.toml")?;
        cfg = merge_app_config(cfg, parsed);
    }

    if let Some(path) = cli_config {
        if path.to_string_lossy().contains("..") {
            anyhow::bail!("cli config path must not contain ..");
        }
        let data = fs::read_to_string(&path).context("read cli config")?;
        let parsed: AppConfig = toml::from_str(&data).context("parse cli config")?;
        cfg = merge_app_config(cfg, parsed);
    }

    cfg = apply_env_overrides(cfg);

    validate_app_config(&cfg).map_err(|e| anyhow::anyhow!(e))?;
    Ok(cfg)
}

fn merge_app_config(base: AppConfig, overlay: AppConfig) -> AppConfig {
    let overlay_json = match serde_json::to_value(&overlay) {
        Ok(v) => v,
        Err(_) => return base,
    };
    let base_json = match serde_json::to_value(&base) {
        Ok(v) => v,
        Err(_) => return base,
    };
    let merged = merge_json(base_json, overlay_json);
    serde_json::from_value(merged).unwrap_or_else(|_| base)
}

fn merge_json(base: serde_json::Value, overlay: serde_json::Value) -> serde_json::Value {
    match (base, overlay) {
        (serde_json::Value::Object(mut base_map), serde_json::Value::Object(overlay_map)) => {
            for (k, v) in overlay_map {
                let entry = base_map.remove(&k);
                let merged = if let Some(e) = entry {
                    if should_override(&e, &v) {
                        merge_json(e, v)
                    } else {
                        e
                    }
                } else {
                    v
                };
                base_map.insert(k, merged);
            }
            serde_json::Value::Object(base_map)
        }
        (_, overlay) => overlay,
    }
}

fn should_override(_base: &serde_json::Value, overlay: &serde_json::Value) -> bool {
    match overlay {
        serde_json::Value::Null => false,
        serde_json::Value::String(s) if s.is_empty() => false,
        serde_json::Value::Array(a) if a.is_empty() => false,
        serde_json::Value::Object(m) if m.is_empty() => false,
        _ => true,
    }
}

fn apply_env_overrides(mut cfg: AppConfig) -> AppConfig {
    if let Some(v) = read_env(ENV_AUDIO_CPP_MODE, ENV_AUDIO_CPP_MODE_LEGACY) {
        cfg.voice.audio_cpp.mode = v;
    }
    if let Some(v) = read_env(ENV_AUDIO_CPP_LOCAL_URL, ENV_AUDIO_CPP_LOCAL_URL_LEGACY) {
        cfg.voice.audio_cpp.local.server_url = v;
    }
    if let Some(v) = read_env(ENV_AUDIO_CPP_REMOTE_URL, ENV_AUDIO_CPP_REMOTE_URL_LEGACY) {
        cfg.voice.audio_cpp.remote.server_url = v;
    }
    if let Some(v) = read_env(ENV_AUDIO_CPP_FAMILY, ENV_AUDIO_CPP_FAMILY_LEGACY) {
        cfg.voice.audio_cpp.family.clone_from(&v);
        cfg.voice_clone.family.clone_from(&v);
    }
    if let Some(v) = read_env(ENV_AUDIO_CPP_MODEL, ENV_AUDIO_CPP_MODEL_LEGACY) {
        cfg.voice.audio_cpp.model.clone_from(&v);
        if cfg.voice_clone.model.is_empty() {
            cfg.voice_clone.model.clone_from(&v);
        }
    }
    if let Some(v) = read_env(ENV_AUDIO_CPP_BACKEND, ENV_AUDIO_CPP_BACKEND_LEGACY) {
        cfg.voice.audio_cpp.backend.clone_from(&v);
    }
    if let Some(v) = read_env(ENV_AUDIO_CPP_LANGUAGE, ENV_AUDIO_CPP_LANGUAGE_LEGACY) {
        cfg.voice.audio_cpp.language.clone_from(&v);
        cfg.voice_clone.language.clone_from(&v);
    }
    if let Some(v) = read_env(ENV_AUDIO_CPP_TIMEOUT, ENV_AUDIO_CPP_TIMEOUT_LEGACY) {
        if let Ok(n) = v.parse::<u64>() {
            cfg.voice.audio_cpp.timeout_secs = n;
        }
    }
    if let Ok(v) = env::var(ENV_GPU_PREFER_FREE) {
        if let Ok(b) = v.parse::<bool>() {
            cfg.voice.gpu_policy.prefer_free = b;
            cfg.voice_clone.routing.prefer_free = b;
        }
    }
    if let Ok(v) = env::var(ENV_GPU_ALLOW_PAID) {
        if let Ok(b) = v.parse::<bool>() {
            cfg.voice.gpu_policy.allow_paid = b;
            cfg.voice_clone.routing.allow_paid = b;
        }
    }
    if let Ok(v) = env::var(ENV_GPU_MAX_JOB) {
        if let Ok(n) = v.parse::<f64>() {
            cfg.voice.gpu_policy.max_cost_per_job = n;
        }
    }
    if let Ok(v) = env::var(ENV_GPU_MAX_DAY) {
        if let Ok(n) = v.parse::<f64>() {
            cfg.voice.gpu_policy.max_cost_per_day = n;
        }
    }
    if let Ok(v) = env::var(ENV_NARRATOR_BACKEND) {
        cfg.narrator.backend.clone_from(&v);
    }
    if let Ok(v) = env::var(ENV_NARRATOR_LANGUAGE) {
        cfg.narrator.language.clone_from(&v);
    }
    cfg
}

pub fn validate_config_file(path: &PathBuf) -> Result<()> {
    if path.to_string_lossy().contains("..") {
        bail!("config path must not contain ..");
    }
    let data = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let cfg: AppConfig = if path.extension().is_some_and(|e| e == "json") {
        serde_json::from_str(&data).context("parse json config")?
    } else {
        toml::from_str(&data).context("parse toml config")?
    };
    validate_app_config(&cfg).map_err(|e| anyhow::anyhow!(e))?;
    if cfg.voice.gpu_policy.allow_paid && cfg.voice.gpu_policy.max_cost_per_job == 0.0 {
        bail!("allow_paid true but max_cost_per_job is 0");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_prefers_overlay_values() {
        let base = AppConfig::default();
        let mut overlay = AppConfig::default();
        overlay.voice.audio_cpp.mode = "local".to_string();
        let merged = merge_app_config(base, overlay);
        assert_eq!(merged.voice.audio_cpp.mode, "local");
    }

    #[test]
    fn merge_ignores_empty_overlay() {
        let base = AppConfig::default();
        let overlay = AppConfig::default();
        let merged = merge_app_config(base.clone(), overlay);
        assert_eq!(merged.voice.audio_cpp.mode, base.voice.audio_cpp.mode);
    }
}
