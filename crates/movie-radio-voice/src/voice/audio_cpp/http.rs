use anyhow::{Context, Result};
use reqwest::Client;
use std::env;
use std::sync::atomic::{AtomicU64, Ordering};

use super::wav::decode_and_resample_wav;
use super::AudioOutput;
use crate::config::{AudioCppConfig, GpuPoolEndpoint};
use crate::voice::SynthesisRequest;

static CUMULATIVE_DAILY_COST_MILLICENTS: AtomicU64 = AtomicU64::new(0);

pub(crate) fn resolve_auth_token(auth_env: Option<&str>) -> Option<String> {
    if let Ok(token) = env::var("AUDIO_CPP_REMOTE_TOKEN") {
        if !token.is_empty() {
            return Some(token);
        }
    }
    if let Some(env_key) = auth_env {
        if let Ok(val) = env::var(env_key) {
            if !val.is_empty() {
                return Some(val);
            }
        }
    }
    None
}

pub(crate) fn sanitize_error_message(msg: &str) -> String {
    let mut clean = msg.to_string();
    if let Some(token) = env::var("AUDIO_CPP_REMOTE_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
    {
        clean = clean.replace(&token, "[REDACTED]");
    }
    clean
}

pub(crate) fn estimate_remote_cost(text_len: usize, cost_per_hour: f64) -> f64 {
    if cost_per_hour <= 0.0 {
        return 0.0;
    }
    let seconds = (text_len as f64) / 20.0;
    let hours = seconds / 3600.0;
    hours * cost_per_hour
}

pub(crate) async fn synthesize_http_endpoint(
    client: &Client,
    config: &AudioCppConfig,
    base_url: &str,
    auth_header: Option<&str>,
    request: &SynthesisRequest,
    family: &str,
    model: &str,
    backend: &str,
    default_language: &str,
) -> Result<AudioOutput> {
    let clean_base = base_url.trim_end_matches('/');
    let speech_url = format!("{}/v1/audio/speech", clean_base);

    let language = if request.language.is_empty() {
        default_language
    } else {
        &request.language
    };

    let voice = request
        .voice_id
        .as_deref()
        .unwrap_or_else(|| config.voice_id.as_deref().unwrap_or(""));
    let voice_ref = config.voice_ref.as_deref().unwrap_or("");

    let payload = serde_json::json!({
        "model": model,
        "input": request.text,
        "voice": voice,
        "language": language,
        "backend": backend,
        "family": family,
        "voice_ref": voice_ref,
        "response_format": "wav",
        "speed": request.speed,
    });

    let mut req_builder = client.post(&speech_url).json(&payload);

    if let Some(token) = auth_header {
        if !token.is_empty() {
            let header_val = if token.starts_with("Bearer ") {
                token.to_string()
            } else {
                format!("Bearer {}", token)
            };
            req_builder = req_builder.header("Authorization", header_val);
        }
    }

    let response = req_builder.send().await.map_err(|e| {
        let sanitized_err = sanitize_error_message(&e.to_string());
        anyhow::anyhow!(
            "Failed to connect to audio.cpp HTTP endpoint: {}",
            sanitized_err
        )
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let err_body = response.text().await.unwrap_or_default();
        let sanitized = sanitize_error_message(&err_body);
        anyhow::bail!(
            "audio.cpp server returned HTTP status {}: {}",
            status,
            sanitized
        );
    }

    let bytes = response
        .bytes()
        .await
        .context("Failed to read audio.cpp HTTP response body")?;

    decode_and_resample_wav(&bytes, request.sample_rate_hz)
}

pub(crate) async fn synthesize_remote_endpoint(
    client: &Client,
    config: &AudioCppConfig,
    endpoint_url: &str,
    auth_env: Option<&str>,
    cost_per_hour: f64,
    request: &SynthesisRequest,
    family: &str,
    model: &str,
    backend: &str,
    default_language: &str,
) -> Result<AudioOutput> {
    if !endpoint_url.starts_with("http://127.0.0.1")
        && !endpoint_url.starts_with("http://localhost")
        && !endpoint_url.starts_with("https://")
    {
        anyhow::bail!("Remote audio.cpp endpoint must use HTTPS");
    }

    let estimated_cost = estimate_remote_cost(request.text.chars().count(), cost_per_hour);
    let is_paid = cost_per_hour > 0.0 || estimated_cost > 0.0;

    if is_paid {
        if !config.gpu_policy.allow_paid {
            anyhow::bail!("Paid GPU cloud execution is not allowed by policy");
        }
        if estimated_cost > config.gpu_policy.max_cost_per_job {
            anyhow::bail!(
                "Estimated job cost ${:.4} exceeds maximum allowed per job (${:.4})",
                estimated_cost,
                config.gpu_policy.max_cost_per_job
            );
        }

        let current_daily_millicents = CUMULATIVE_DAILY_COST_MILLICENTS.load(Ordering::Relaxed);
        let current_daily_usd = (current_daily_millicents as f64) / 100_000.0;
        if current_daily_usd + estimated_cost > config.gpu_policy.max_cost_per_day {
            anyhow::bail!(
                "Job cost ${:.4} would exceed daily GPU budget limit (${:.4} current: ${:.4})",
                estimated_cost,
                config.gpu_policy.max_cost_per_day,
                current_daily_usd
            );
        }
    }

    let token = resolve_auth_token(auth_env);
    let output = synthesize_http_endpoint(
        client,
        config,
        endpoint_url,
        token.as_deref(),
        request,
        family,
        model,
        backend,
        default_language,
    )
    .await?;

    if is_paid {
        let millicents = (estimated_cost * 100_000.0) as u64;
        CUMULATIVE_DAILY_COST_MILLICENTS.fetch_add(millicents, Ordering::Relaxed);
    }

    Ok(output)
}

pub(crate) async fn synthesize_gpu_pools(
    client: &Client,
    config: &AudioCppConfig,
    request: &SynthesisRequest,
    family: &str,
    model: &str,
    backend: &str,
    default_language: &str,
) -> Result<AudioOutput> {
    let mut endpoints = config.gpu_pool.clone();

    let remote_url =
        env::var("AUDIO_CPP_REMOTE_URL").unwrap_or_else(|_| config.remote.server_url.clone());
    let remote_token_env = env::var("AUDIO_CPP_REMOTE_TOKEN")
        .ok()
        .or_else(|| config.remote.auth_env.clone());

    if config.remote.enabled && !remote_url.is_empty() {
        let auth_env_key = if env::var("AUDIO_CPP_REMOTE_TOKEN").is_ok() {
            Some("AUDIO_CPP_REMOTE_TOKEN".to_string())
        } else {
            remote_token_env
        };
        endpoints.push(GpuPoolEndpoint {
            name: "default_remote".to_string(),
            url: remote_url,
            auth_env: auth_env_key,
            priority: 15,
            cost_per_hour: 0.0,
        });
    }

    if endpoints.is_empty() {
        anyhow::bail!("No remote audio.cpp GPU pool endpoints configured");
    }

    endpoints.sort_by(|a, b| {
        if config.gpu_policy.prefer_free {
            let a_free = a.cost_per_hour == 0.0;
            let b_free = b.cost_per_hour == 0.0;
            if a_free != b_free {
                return b_free.cmp(&a_free);
            }
        }
        a.priority.cmp(&b.priority)
    });

    let mut last_err = anyhow::anyhow!("No GPU pool endpoint available");

    for ep in &endpoints {
        match synthesize_remote_endpoint(
            client,
            config,
            &ep.url,
            ep.auth_env.as_deref(),
            ep.cost_per_hour,
            request,
            family,
            model,
            backend,
            default_language,
        )
        .await
        {
            Ok(output) => return Ok(output),
            Err(e) => {
                tracing::warn!("GPU pool endpoint '{}' failed: {}", ep.name, e);
                last_err = e;
            }
        }
    }

    Err(last_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_sanitization() {
        env::set_var("AUDIO_CPP_REMOTE_TOKEN", "super-secret-token-123");
        let err_msg = "Error connecting with Authorization Bearer super-secret-token-123 failed";
        let clean = sanitize_error_message(err_msg);
        assert!(!clean.contains("super-secret-token-123"));
        assert!(clean.contains("[REDACTED]"));
        env::remove_var("AUDIO_CPP_REMOTE_TOKEN");
    }

    #[test]
    fn test_remote_cost_estimation() {
        let text_100 = 100;
        let cost = estimate_remote_cost(text_100, 0.40);
        assert!((cost - 0.0005555).abs() < 0.0001);

        let free_cost = estimate_remote_cost(text_100, 0.0);
        assert_eq!(free_cost, 0.0);
    }
}
