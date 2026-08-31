use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use std::env;
use std::time::Duration;

use super::{AudioOutput, ProviderCapabilities, SynthesisRequest, VoiceSynthesizer};
use crate::config::AudioCppConfig;

pub(crate) mod cli;
pub(crate) mod http;
pub(crate) mod wav;

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
        http::synthesize_http_endpoint(
            &self.client,
            &self.config,
            &base_url,
            None,
            request,
            &self.family(),
            &self.model(),
            &self.backend(),
            &self.default_language(),
        )
        .await
    }

    async fn synthesize_local(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        if self.config.local.mode == "cli" {
            cli::synthesize_local_cli(
                &self.config,
                request,
                &self.family(),
                &self.model(),
                &self.backend(),
                &self.default_language(),
                self.timeout_duration(),
            )
            .await
        } else {
            self.synthesize_local_server(request).await
        }
    }

    async fn synthesize_gpu_pools(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        http::synthesize_gpu_pools(
            &self.client,
            &self.config,
            request,
            &self.family(),
            &self.model(),
            &self.backend(),
            &self.default_language(),
        )
        .await
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
            (text_len as f64) * 0.0000005
        }
    }
}

fn env_u64(key: &str) -> Option<u64> {
    env::var(key).ok().and_then(|v| v.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AudioCppConfig, GpuPolicyConfig, GpuPoolEndpoint};

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
