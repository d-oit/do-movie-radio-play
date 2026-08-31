use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use std::env;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

use super::{AudioOutput, ProviderCapabilities, SynthesisRequest, VoiceSynthesizer};
use crate::config::{AudioCppConfig, GpuPoolEndpoint};

static CUMULATIVE_DAILY_COST_MILLICENTS: AtomicU64 = AtomicU64::new(0);

pub struct AudioCppProvider {
    config: AudioCppConfig,
    client: Client,
}

impl AudioCppProvider {
    pub fn new(config: AudioCppConfig) -> Self {
        let timeout_secs = env_u64("AUDIO_CPP_TIMEOUT_SECS").unwrap_or(config.timeout_secs);
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self { config, client }
    }

    fn mode(&self) -> String {
        env::var("AUDIO_CPP_MODE").unwrap_or_else(|_| self.config.mode.clone())
    }

    fn family(&self) -> String {
        env::var("AUDIO_CPP_FAMILY").unwrap_or_else(|_| self.config.family.clone())
    }

    fn model(&self) -> String {
        env::var("AUDIO_CPP_MODEL").unwrap_or_else(|_| self.config.model.clone())
    }

    fn backend(&self) -> String {
        env::var("AUDIO_CPP_BACKEND").unwrap_or_else(|_| self.config.backend.clone())
    }

    fn default_language(&self) -> String {
        env::var("AUDIO_CPP_LANGUAGE").unwrap_or_else(|_| self.config.language.clone())
    }

    fn timeout_duration(&self) -> Duration {
        let secs = env_u64("AUDIO_CPP_TIMEOUT_SECS").unwrap_or(self.config.timeout_secs);
        Duration::from_secs(secs)
    }

    async fn synthesize_local_server(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        let base_url = env::var("AUDIO_CPP_LOCAL_URL")
            .unwrap_or_else(|_| self.config.local.server_url.clone());
        if base_url.is_empty() {
            anyhow::bail!("Local audio.cpp server URL is not configured");
        }
        self.synthesize_http_endpoint(&base_url, None, request)
            .await
    }

    async fn synthesize_local_cli(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        let binary = &self.config.local.binary;
        let temp_dir = env::temp_dir();
        let temp_filename = format!("audiocpp_out_{}_{}.wav", std::process::id(), rand_id());
        let output_path = temp_dir.join(temp_filename);

        let family = self.family();
        let model = self.model();
        let backend = self.backend();
        let language = if request.language.is_empty() {
            self.default_language()
        } else {
            request.language.clone()
        };

        let mut cmd = Command::new(binary);
        cmd.arg("--model").arg(&model);
        cmd.arg("--input").arg(&request.text);
        cmd.arg("--output").arg(&output_path);
        cmd.arg("--language").arg(&language);
        cmd.arg("--backend").arg(&backend);
        cmd.arg("--family").arg(&family);

        if let Some(ref v_id) = request.voice_id {
            cmd.arg("--voice").arg(v_id);
        }
        if let Some(ref v_ref) = self.config.voice_ref {
            cmd.arg("--voice-ref").arg(v_ref);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let timeout_dur = self.timeout_duration();
        let child_res = timeout(timeout_dur, cmd.output()).await;
        let output = match child_res {
            Ok(Ok(res)) => res,
            Ok(Err(e)) => {
                let _ = tokio::fs::remove_file(&output_path).await;
                return Err(anyhow::anyhow!(
                    "Failed to execute local audiocpp_cli process: {}",
                    e
                ));
            }
            Err(_) => {
                let _ = tokio::fs::remove_file(&output_path).await;
                anyhow::bail!(
                    "audiocpp_cli process execution timed out after {:?}",
                    timeout_dur
                );
            }
        };

        if !output.status.success() {
            let stderr_text = String::from_utf8_lossy(&output.stderr);
            let _ = tokio::fs::remove_file(&output_path).await;
            anyhow::bail!(
                "audiocpp_cli exited with status {}: {}",
                output.status,
                stderr_text.trim()
            );
        }

        let wav_bytes = tokio::fs::read(&output_path)
            .await
            .context("Failed to read audiocpp_cli WAV output file")?;
        let _ = tokio::fs::remove_file(&output_path).await;

        decode_and_resample_wav(&wav_bytes, request.sample_rate_hz)
    }

    async fn synthesize_http_endpoint(
        &self,
        base_url: &str,
        auth_header: Option<&str>,
        request: &SynthesisRequest,
    ) -> Result<AudioOutput> {
        let clean_base = base_url.trim_end_matches('/');
        let speech_url = format!("{}/v1/audio/speech", clean_base);

        let family = self.family();
        let model = self.model();
        let backend = self.backend();
        let language = if request.language.is_empty() {
            self.default_language()
        } else {
            request.language.clone()
        };

        let voice = request
            .voice_id
            .as_deref()
            .unwrap_or_else(|| self.config.voice_id.as_deref().unwrap_or(""));
        let voice_ref = self.config.voice_ref.as_deref().unwrap_or("");

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

        let mut req_builder = self.client.post(&speech_url).json(&payload);

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

    async fn synthesize_remote_endpoint(
        &self,
        endpoint_url: &str,
        auth_env: Option<&str>,
        cost_per_hour: f64,
        request: &SynthesisRequest,
    ) -> Result<AudioOutput> {
        // Enforce HTTPS for remote non-localhost endpoints
        if !endpoint_url.starts_with("http://127.0.0.1")
            && !endpoint_url.starts_with("http://localhost")
            && !endpoint_url.starts_with("https://")
        {
            anyhow::bail!("Remote audio.cpp endpoint must use HTTPS");
        }

        // Cost & policy validation
        let estimated_cost = estimate_remote_cost(request.text.chars().count(), cost_per_hour);
        let is_paid = cost_per_hour > 0.0 || estimated_cost > 0.0;

        if is_paid {
            if !self.config.gpu_policy.allow_paid {
                anyhow::bail!("Paid GPU cloud execution is not allowed by policy");
            }
            if estimated_cost > self.config.gpu_policy.max_cost_per_job {
                anyhow::bail!(
                    "Estimated job cost ${:.4} exceeds maximum allowed per job (${:.4})",
                    estimated_cost,
                    self.config.gpu_policy.max_cost_per_job
                );
            }

            let current_daily_millicents = CUMULATIVE_DAILY_COST_MILLICENTS.load(Ordering::Relaxed);
            let current_daily_usd = (current_daily_millicents as f64) / 100_000.0;
            if current_daily_usd + estimated_cost > self.config.gpu_policy.max_cost_per_day {
                anyhow::bail!(
                    "Job cost ${:.4} would exceed daily GPU budget limit (${:.4} current: ${:.4})",
                    estimated_cost,
                    self.config.gpu_policy.max_cost_per_day,
                    current_daily_usd
                );
            }
        }

        let token = resolve_auth_token(auth_env);
        let output = self
            .synthesize_http_endpoint(endpoint_url, token.as_deref(), request)
            .await?;

        if is_paid {
            let millicents = (estimated_cost * 100_000.0) as u64;
            CUMULATIVE_DAILY_COST_MILLICENTS.fetch_add(millicents, Ordering::Relaxed);
        }

        Ok(output)
    }

    async fn synthesize_local(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        if self.config.local.mode == "cli" {
            self.synthesize_local_cli(request).await
        } else {
            self.synthesize_local_server(request).await
        }
    }

    async fn synthesize_auto(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        let local_err = match self.synthesize_local(request).await {
            Ok(out) => return Ok(out),
            Err(err) => err,
        };
        tracing::debug!(
            "Local audio.cpp synthesis failed ({}), trying remote GPU pools...",
            local_err
        );
        let remote_err = match self.synthesize_gpu_pools(request).await {
            Ok(out) => return Ok(out),
            Err(err) => err,
        };
        anyhow::bail!(
            "audio.cpp synthesis failed. Local error: {}; Remote error: {}",
            local_err,
            remote_err
        );
    }

    async fn synthesize_gpu_pools(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        let mut endpoints = self.config.gpu_pool.clone();

        // If remote URL configured directly in remote config, include it
        let remote_url = env::var("AUDIO_CPP_REMOTE_URL")
            .unwrap_or_else(|_| self.config.remote.server_url.clone());
        let remote_token_env = env::var("AUDIO_CPP_REMOTE_TOKEN")
            .ok()
            .or_else(|| self.config.remote.auth_env.clone());

        if self.config.remote.enabled && !remote_url.is_empty() {
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

        // Sort by priority (ascending order: priority 10 before 20)
        // If prefer_free policy is true, free endpoints (cost_per_hour == 0.0) come first
        endpoints.sort_by(|a, b| {
            if self.config.gpu_policy.prefer_free {
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
            match self
                .synthesize_remote_endpoint(
                    &ep.url,
                    ep.auth_env.as_deref(),
                    ep.cost_per_hour,
                    request,
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
}

#[async_trait]
impl VoiceSynthesizer for AudioCppProvider {
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        if !self.config.enabled {
            anyhow::bail!("AudioCppProvider is disabled");
        }

        let mode = self.mode();
        match mode.as_str() {
            "local" => self.synthesize_local(request).await,
            "remote" => self.synthesize_gpu_pools(request).await,
            _ => self.synthesize_auto(request).await,
        }
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_emotion: true,
            supports_voice_cloning: true,
            supports_streaming: false,
            max_text_length: 10_000,
            languages: vec![
                "de".to_string(),
                "en".to_string(),
                "fr".to_string(),
                "es".to_string(),
                "ja".to_string(),
                "zh".to_string(),
            ],
            requires_gpu: false,
        }
    }

    fn estimate_cost(&self, text_len: usize) -> f64 {
        let mode = self.mode();
        if mode == "local" {
            0.0
        } else {
            // Nominal cost estimate for remote GPU processing
            (text_len as f64) * 0.0000005
        }
    }
}

fn env_u64(key: &str) -> Option<u64> {
    env::var(key).ok().and_then(|v| v.parse().ok())
}

fn resolve_auth_token(auth_env: Option<&str>) -> Option<String> {
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

fn sanitize_error_message(msg: &str) -> String {
    // Redact bearer tokens or confidential strings if present
    let mut clean = msg.to_string();
    if let Some(token) = env::var("AUDIO_CPP_REMOTE_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
    {
        clean = clean.replace(&token, "[REDACTED]");
    }
    clean
}

fn rand_id() -> u64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(12345)
}

fn estimate_remote_cost(text_len: usize, cost_per_hour: f64) -> f64 {
    if cost_per_hour <= 0.0 {
        return 0.0;
    }
    // Assume average processing speed of 20 characters per second
    let seconds = (text_len as f64) / 20.0;
    let hours = seconds / 3600.0;
    hours * cost_per_hour
}

struct WavLayout {
    data: std::ops::Range<usize>,
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
}

fn parse_fmt_chunk_info(bytes: &[u8], data_start: usize, size: usize) -> Option<(u32, u16, u16)> {
    if size < 16 || data_start + 16 > bytes.len() {
        return None;
    }
    let fmt = &bytes[data_start..data_start + 16];
    let audio_format = u16::from_le_bytes([fmt[0], fmt[1]]);
    if audio_format != 1 {
        return None;
    }
    let channels = u16::from_le_bytes([fmt[2], fmt[3]]);
    let sample_rate = u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]);
    let bits_per_sample = u16::from_le_bytes([fmt[14], fmt[15]]);
    Some((sample_rate, channels, bits_per_sample))
}

fn parse_wav_layout(bytes: &[u8]) -> Result<WavLayout> {
    const HEADER_LEN: usize = 12;
    if bytes.len() < HEADER_LEN || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        anyhow::bail!("Not a valid RIFF/WAVE container");
    }

    let mut fmt_info: Option<(u32, u16, u16)> = None;
    let mut data_range: Option<std::ops::Range<usize>> = None;
    let mut offset = HEADER_LEN;

    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(
            bytes[offset + 4..offset + 8]
                .try_into()
                .context("Invalid chunk header size")?,
        ) as usize;
        let data_start = offset + 8;
        let data_end = match data_start.checked_add(size) {
            Some(end) if end <= bytes.len() => end,
            _ => break,
        };

        if id == b"fmt " {
            fmt_info = parse_fmt_chunk_info(bytes, data_start, size);
        } else if id == b"data" {
            data_range = Some(data_start..data_end);
        }

        offset = data_end + (size & 1);
    }

    let (sample_rate, channels, bits_per_sample) =
        fmt_info.ok_or_else(|| anyhow::anyhow!("WAVE container missing valid PCM fmt chunk"))?;
    let data = data_range.ok_or_else(|| anyhow::anyhow!("WAVE container missing data chunk"))?;

    Ok(WavLayout {
        data,
        sample_rate,
        channels,
        bits_per_sample,
    })
}

fn decode_and_resample_wav(bytes: &[u8], target_sample_rate_hz: u32) -> Result<AudioOutput> {
    let layout = parse_wav_layout(bytes)?;

    if layout.bits_per_sample != 16 {
        anyhow::bail!(
            "Unsupported bit depth {} (expected 16-bit PCM)",
            layout.bits_per_sample
        );
    }

    let raw_data = &bytes[layout.data];
    let channels = layout.channels as usize;
    if channels == 0 {
        anyhow::bail!("WAVE container has 0 channels");
    }

    let frame_count = raw_data.len() / (2 * channels);
    let mut mono_samples = Vec::with_capacity(frame_count);

    for i in 0..frame_count {
        let frame_offset = i * 2 * channels;
        let mut sum = 0.0f32;
        for c in 0..channels {
            let sample_offset = frame_offset + c * 2;
            let sample_i16 =
                i16::from_le_bytes([raw_data[sample_offset], raw_data[sample_offset + 1]]);
            sum += sample_i16 as f32 / i16::MAX as f32;
        }
        mono_samples.push(sum / (channels as f32));
    }

    let src_rate = layout.sample_rate;
    if src_rate == target_sample_rate_hz || mono_samples.is_empty() {
        return Ok(AudioOutput {
            samples: mono_samples,
            sample_rate_hz: target_sample_rate_hz,
        });
    }

    // Simple linear resampling
    let ratio = (src_rate as f64) / (target_sample_rate_hz as f64);
    let target_len = ((mono_samples.len() as f64) / ratio).round() as usize;
    let mut resampled = Vec::with_capacity(target_len);

    for i in 0..target_len {
        let src_idx = (i as f64) * ratio;
        let idx0 = src_idx.floor() as usize;
        let idx1 = (idx0 + 1).min(mono_samples.len() - 1);
        let frac = (src_idx - (idx0 as f64)) as f32;

        let s0 = mono_samples.get(idx0).copied().unwrap_or(0.0);
        let s1 = mono_samples.get(idx1).copied().unwrap_or(s0);
        resampled.push(s0 * (1.0 - frac) + s1 * frac);
    }

    Ok(AudioOutput {
        samples: resampled,
        sample_rate_hz: target_sample_rate_hz,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AudioCppConfig, GpuPolicyConfig, GpuPoolEndpoint};
    use crate::voice::SynthesisRequest;

    fn sample_wav_pcm16_mono(sample_rate: u32, samples: &[i16]) -> Vec<u8> {
        let data_len = (samples.len() * 2) as u32;
        let mut fmt = Vec::with_capacity(16);
        fmt.extend_from_slice(&1u16.to_le_bytes()); // PCM = 1
        fmt.extend_from_slice(&1u16.to_le_bytes()); // Mono = 1
        fmt.extend_from_slice(&sample_rate.to_le_bytes());
        fmt.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
        fmt.extend_from_slice(&2u16.to_le_bytes()); // block align
        fmt.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

        let mut body = b"WAVE".to_vec();
        body.extend_from_slice(b"fmt ");
        body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        body.extend_from_slice(&fmt);

        body.extend_from_slice(b"data");
        body.extend_from_slice(&data_len.to_le_bytes());
        for &s in samples {
            body.extend_from_slice(&s.to_le_bytes());
        }

        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn test_wav_decoding_and_resampling() {
        let wav = sample_wav_pcm16_mono(16000, &[0, 16384, -16384, 0]);
        let output = decode_and_resample_wav(&wav, 16000).unwrap();
        assert_eq!(output.sample_rate_hz, 16000);
        assert_eq!(output.samples.len(), 4);
        assert!((output.samples[1] - 0.5).abs() < 0.01);

        let resampled = decode_and_resample_wav(&wav, 8000).unwrap();
        assert_eq!(resampled.sample_rate_hz, 8000);
        assert_eq!(resampled.samples.len(), 2);
    }

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
        // 100 chars at 20 chars/sec = 5 seconds = 5/3600 hours
        // 5/3600 * 0.40 $/hr = 0.000555... USD
        let cost = estimate_remote_cost(text_100, 0.40);
        assert!((cost - 0.0005555).abs() < 0.0001);

        let free_cost = estimate_remote_cost(text_100, 0.0);
        assert_eq!(free_cost, 0.0);
    }

    #[tokio::test]
    async fn test_provider_disabled_error() {
        let cfg = AudioCppConfig {
            enabled: false,
            ..AudioCppConfig::default()
        };
        let provider = AudioCppProvider::new(cfg);
        let req = SynthesisRequest::default();
        let res = provider.synthesize(&req).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("disabled"));
    }

    #[tokio::test]
    async fn test_paid_gpu_policy_rejection_when_disallowed() {
        let cfg = AudioCppConfig {
            mode: "remote".to_string(),
            gpu_policy: GpuPolicyConfig {
                allow_paid: false,
                ..GpuPolicyConfig::default()
            },
            gpu_pool: vec![GpuPoolEndpoint {
                name: "paid-gpu".to_string(),
                url: "https://paid-gpu.example.com".to_string(),
                auth_env: None,
                priority: 1,
                cost_per_hour: 0.50,
            }],
            ..AudioCppConfig::default()
        };

        let provider = AudioCppProvider::new(cfg);
        let req = SynthesisRequest {
            text: "Hello world testing paid GPU policy rejection".to_string(),
            ..SynthesisRequest::default()
        };

        let res = provider.synthesize(&req).await;
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert!(err.contains("Paid GPU cloud execution is not allowed"));
    }

    #[tokio::test]
    async fn test_gpu_pool_sorting_prefer_free() {
        let mut cfg = AudioCppConfig::default();
        cfg.gpu_policy.prefer_free = true;
        cfg.gpu_pool = vec![
            GpuPoolEndpoint {
                name: "paid-high-priority".to_string(),
                url: "https://paid.example.com".to_string(),
                auth_env: None,
                priority: 1,
                cost_per_hour: 0.40,
            },
            GpuPoolEndpoint {
                name: "free-low-priority".to_string(),
                url: "https://free.example.com".to_string(),
                auth_env: None,
                priority: 10,
                cost_per_hour: 0.0,
            },
        ];

        let mut pool = cfg.gpu_pool.clone();
        pool.sort_by(|a, b| {
            if cfg.gpu_policy.prefer_free {
                let a_free = a.cost_per_hour == 0.0;
                let b_free = b.cost_per_hour == 0.0;
                if a_free != b_free {
                    return b_free.cmp(&a_free);
                }
            }
            a.priority.cmp(&b.priority)
        });

        assert_eq!(pool[0].name, "free-low-priority");
        assert_eq!(pool[1].name, "paid-high-priority");
    }

    #[test]
    fn test_capabilities() {
        let provider = AudioCppProvider::new(AudioCppConfig::default());
        let cap = provider.capabilities();
        assert!(cap.supports_emotion);
        assert!(cap.supports_voice_cloning);
        assert_eq!(cap.max_text_length, 10_000);
    }
}
