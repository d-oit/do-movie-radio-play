use anyhow::{bail, Context, Result};
use std::{env, fs, path::PathBuf};

use movie_radio_types::{validation::validate_app_config, AppConfig};

pub fn load_app_config(cli_config: Option<PathBuf>) -> Result<AppConfig> {
    // Load .env before reading env vars (dotenvy is optional)
    let _ = dotenvy::dotenv();

    let mut cfg = AppConfig::default();

    // 1. default.toml
    let default_path = PathBuf::from("config/default.toml");
    if default_path.exists() {
        let data = fs::read_to_string(&default_path).context("read config/default.toml")?;
        let parsed: AppConfig = toml::from_str(&data).context("parse config/default.toml")?;
        cfg = merge_app_config(cfg, parsed);
    }

    // 2. config/local.toml
    let local_path = PathBuf::from("config/local.toml");
    if local_path.exists() {
        let data = fs::read_to_string(&local_path).context("read config/local.toml")?;
        let parsed: AppConfig = toml::from_str(&data).context("parse config/local.toml")?;
        cfg = merge_app_config(cfg, parsed);
    }

    // 3. CLI config override (TOML)
    if let Some(path) = cli_config {
        let data = fs::read_to_string(&path).context("read cli config")?;
        let parsed: AppConfig = toml::from_str(&data).context("parse cli config")?;
        cfg = merge_app_config(cfg, parsed);
    }

    // 4. Env overrides (MRPLAY_ + legacy AUDIO_CPP_*)
    cfg = apply_env_overrides(cfg);

    // Validate (never prints secrets)
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
    serde_json::from_value(merged).unwrap_or(base)
}

fn merge_json(mut base: serde_json::Value, overlay: serde_json::Value) -> serde_json::Value {
    match (base, overlay) {
        (serde_json::Value::Object(mut base_map), serde_json::Value::Object(overlay_map)) => {
            for (k, v) in overlay_map {
                let entry = base_map.remove(&k);
                let merged = if let Some(e) = entry {
                    // If both objects, recurse; if overlay is default-ish, keep base? We simplify: overlay wins if not null/default.
                    // To avoid wiping defaults, skip empty strings/arrays that are defaults unless overlay explicitly sets non-default.
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
    // Canonical MRPLAY_* (also check AUDIO_CPP_* for compat, MRPLAY wins)
    if let Ok(v) = env::var("MRPLAY_AUDIO_CPP_MODE").or_else(|_| env::var("AUDIO_CPP_MODE")) {
        cfg.voice.audio_cpp.mode = v;
    }
    if let Ok(v) =
        env::var("MRPLAY_AUDIO_CPP_LOCAL_URL").or_else(|_| env::var("AUDIO_CPP_LOCAL_URL"))
    {
        cfg.voice.audio_cpp.local.server_url = v;
    }
    if let Ok(v) =
        env::var("MRPLAY_AUDIO_CPP_REMOTE_URL").or_else(|_| env::var("AUDIO_CPP_REMOTE_URL"))
    {
        cfg.voice.audio_cpp.remote.server_url = v;
    }
    if let Ok(v) = env::var("MRPLAY_AUDIO_CPP_FAMILY").or_else(|_| env::var("AUDIO_CPP_FAMILY")) {
        cfg.voice.audio_cpp.family = v;
        cfg.voice_clone.family = cfg.voice.audio_cpp.family.clone();
    }
    if let Ok(v) = env::var("MRPLAY_AUDIO_CPP_MODEL").or_else(|_| env::var("AUDIO_CPP_MODEL")) {
        cfg.voice.audio_cpp.model = v.clone();
        if cfg.voice_clone.model.is_empty() {
            cfg.voice_clone.model = v;
        }
    }
    if let Ok(v) = env::var("MRPLAY_AUDIO_CPP_BACKEND").or_else(|_| env::var("AUDIO_CPP_BACKEND")) {
        cfg.voice.audio_cpp.backend = v;
    }
    if let Ok(v) = env::var("MRPLAY_AUDIO_CPP_LANGUAGE").or_else(|_| env::var("AUDIO_CPP_LANGUAGE"))
    {
        cfg.voice.audio_cpp.language = v.clone();
        cfg.voice_clone.language = v;
    }
    if let Ok(v) =
        env::var("MRPLAY_AUDIO_CPP_TIMEOUT_SECS").or_else(|_| env::var("AUDIO_CPP_TIMEOUT_SECS"))
    {
        if let Ok(n) = v.parse::<u64>() {
            cfg.voice.audio_cpp.timeout_secs = n;
        }
    }
    if let Ok(v) = env::var("MRPLAY_GPU_PREFER_FREE") {
        if let Ok(b) = v.parse::<bool>() {
            cfg.voice.gpu_policy.prefer_free = b;
            cfg.voice_clone.routing.prefer_free = b;
        }
    }
    if let Ok(v) = env::var("MRPLAY_GPU_ALLOW_PAID") {
        if let Ok(b) = v.parse::<bool>() {
            cfg.voice.gpu_policy.allow_paid = b;
            cfg.voice_clone.routing.allow_paid = b;
        }
    }
    if let Ok(v) = env::var("MRPLAY_GPU_MAX_COST_PER_JOB") {
        if let Ok(n) = v.parse::<f64>() {
            cfg.voice.gpu_policy.max_cost_per_job = n;
        }
    }
    if let Ok(v) = env::var("MRPLAY_GPU_MAX_COST_PER_DAY") {
        if let Ok(n) = v.parse::<f64>() {
            cfg.voice.gpu_policy.max_cost_per_day = n;
        }
    }
    if let Ok(v) = env::var("MRPLAY_NARRATOR_BACKEND") {
        cfg.narrator.backend = v;
    }
    if let Ok(v) = env::var("MRPLAY_NARRATOR_LANGUAGE") {
        cfg.narrator.language = v;
    }
    // Secrets via env are read at runtime, not stored in config; no override needed.
    cfg
}

pub fn validate_config_file(path: &PathBuf) -> Result<()> {
    let data = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let cfg: AppConfig = if path.extension().is_some_and(|e| e == "json") {
        serde_json::from_str(&data).context("parse json config")?
    } else {
        toml::from_str(&data).context("parse toml config")?
    };
    validate_app_config(&cfg).map_err(|e| anyhow::anyhow!(e))?;
    // Also check default.toml and local.toml if they exist? Caller handles.
    // Validate URLs etc already.
    if cfg.voice.gpu_policy.allow_paid && cfg.voice.gpu_policy.max_cost_per_job == 0.0 {
        bail!("allow_paid true but max_cost_per_job is 0");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_override_respects_mrplay() {
        // SAFETY: test isolation via temp env
        unsafe { env::set_var("MRPLAY_AUDIO_CPP_MODE", "local") };
        let cfg = apply_env_overrides(AppConfig::default());
        assert_eq!(cfg.voice.audio_cpp.mode, "local");
        unsafe { env::remove_var("MRPLAY_AUDIO_CPP_MODE") };
    }
}
