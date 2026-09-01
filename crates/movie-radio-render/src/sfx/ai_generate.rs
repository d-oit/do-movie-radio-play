use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use movie_radio_types::{
    AiGenerateConfig, SfxCandidate, SfxLicense, SfxProviderCapabilities, SfxQuery,
};
use reqwest::Client;
use std::env;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use super::SoundEffectBackend;

static CUMULATIVE_DAILY_COST_MILLICENTS: AtomicU64 = AtomicU64::new(0);

pub struct AiGenerateSfxBackend {
    config: AiGenerateConfig,
    client: Client,
}

impl AiGenerateSfxBackend {
    pub fn new(config: AiGenerateConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .context("build ai generate client")?;
        Ok(Self { config, client })
    }

    #[cfg(test)]
    pub fn with_client_for_test(config: AiGenerateConfig, client: Client) -> Self {
        Self { config, client }
    }

    pub fn reset_daily_cost_for_test() {
        CUMULATIVE_DAILY_COST_MILLICENTS.store(0, Ordering::Relaxed);
    }

    fn resolve_token(&self) -> Option<String> {
        if let Some(env_key) = self.config.auth_env.as_deref() {
            if let Ok(val) = env::var(env_key) {
                if !val.is_empty() {
                    return Some(val);
                }
            }
        }
        None
    }

    fn sanitize(&self, msg: &str) -> String {
        if let Some(token) = self.resolve_token() {
            msg.replace(&token, "[REDACTED]")
        } else {
            msg.to_string()
        }
    }

    fn estimate_cost(&self, prompt_len: usize, cost_per_hour: f64) -> f64 {
        if cost_per_hour <= 0.0 {
            return 0.0;
        }
        let seconds = (prompt_len as f64) / 10.0;
        seconds / 3600.0 * cost_per_hour
    }

    fn https_required(&self, url: &str) -> Result<()> {
        if url.starts_with("http://127.0.0.1")
            || url.starts_with("http://localhost")
            || url.starts_with("https://")
        {
            Ok(())
        } else {
            bail!("AI SFX endpoint must use HTTPS or localhost, got {url}");
        }
    }

    async fn generate_via_http(
        &self,
        prompt: &str,
        duration_secs: Option<f32>,
        endpoint: &str,
        token: Option<&str>,
    ) -> Result<Vec<u8>> {
        self.https_required(endpoint)?;
        let prompt_len = prompt.chars().count();
        if prompt_len > self.config.max_prompt_len {
            bail!(
                "prompt too long: {} > {}",
                prompt_len,
                self.config.max_prompt_len
            );
        }
        let url = format!("{}/generate", endpoint.trim_end_matches('/'));
        let mut payload = serde_json::json!({
            "prompt": prompt,
            "model": self.config.model,
            "duration_secs": duration_secs.unwrap_or(3.0),
        });
        if let Some(d) = duration_secs {
            payload["duration_secs"] = serde_json::json!(d);
        }
        let mut req = self.client.post(&url).json(&payload);
        if let Some(t) = token {
            if !t.is_empty() {
                let hv = if t.starts_with("Bearer ") {
                    t.to_string()
                } else {
                    format!("Bearer {t}")
                };
                req = req.header("Authorization", hv);
            }
        }
        let resp = req.send().await.map_err(|e| {
            let s = self.sanitize(&e.to_string());
            anyhow::anyhow!("ai sfx connect failed: {s}")
        })?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let clean = self.sanitize(&body);
            bail!("ai sfx http {status}: {clean}");
        }
        let bytes = resp.bytes().await.context("ai sfx read bytes")?;
        if bytes.len() as u64 > self.config.max_audio_bytes {
            bail!(
                "ai sfx audio too large: {} > {}",
                bytes.len(),
                self.config.max_audio_bytes
            );
        }
        Ok(bytes.to_vec())
    }

    fn budget_check(&self, estimated_cost: f64) -> Result<()> {
        let is_paid =
            estimated_cost > 0.0 || self.config.gpu_pool.iter().any(|e| e.cost_per_hour > 0.0);
        if !is_paid {
            return Ok(());
        }
        if !self.config.gpu_policy.allow_paid {
            bail!("Paid GPU SFX generation not allowed by policy");
        }
        if estimated_cost > self.config.gpu_policy.max_cost_per_job {
            bail!(
                "Estimated SFX cost ${:.4} exceeds per-job limit ${:.4}",
                estimated_cost,
                self.config.gpu_policy.max_cost_per_job
            );
        }
        let current_milli = CUMULATIVE_DAILY_COST_MILLICENTS.load(Ordering::Relaxed);
        let current_usd = (current_milli as f64) / 100_000.0;
        if current_usd + estimated_cost > self.config.gpu_policy.max_cost_per_day {
            bail!(
                "SFX cost ${:.4} would exceed daily limit ${:.4} (current ${:.4})",
                estimated_cost,
                self.config.gpu_policy.max_cost_per_day,
                current_usd
            );
        }
        Ok(())
    }

    fn sorted_endpoints(&self) -> Vec<(String, Option<String>, f64)> {
        let mut eps: Vec<(String, Option<String>, f64)> = Vec::new();
        if !self.config.endpoint_url.is_empty() {
            eps.push((
                self.config.endpoint_url.clone(),
                self.config.auth_env.clone(),
                0.0,
            ));
        }
        for ep in &self.config.gpu_pool {
            eps.push((ep.url.clone(), ep.auth_env.clone(), ep.cost_per_hour));
        }
        eps.sort_by(|a, b| {
            if self.config.gpu_policy.prefer_free {
                let a_free = a.2 == 0.0;
                let b_free = b.2 == 0.0;
                if a_free != b_free {
                    return b_free.cmp(&a_free);
                }
            }
            a.0.cmp(&b.0)
        });
        eps
    }
}

#[async_trait]
impl SoundEffectBackend for AiGenerateSfxBackend {
    async fn search(&self, query: &SfxQuery) -> Result<Vec<SfxCandidate>> {
        if !self.config.enabled {
            bail!("AI SFX backend disabled");
        }
        let prompt = query.prompt.clone().unwrap_or_else(|| {
            let mut p = query.tags.join(", ");
            if let Some(mood) = &query.mood {
                p.push_str(&format!(" mood:{mood}"));
            }
            if p.is_empty() {
                "ambient background".to_string()
            } else {
                p
            }
        });
        let estimated = self.estimate_cost(prompt.chars().count(), 0.40);
        self.budget_check(estimated)?;
        Ok(vec![SfxCandidate {
            id: format!("ai:{}", prompt.chars().take(20).collect::<String>()),
            path_or_url: format!("ai_generate://{}", prompt),
            license: SfxLicense::Cc0,
            duration_secs: query.duration_secs,
            tags: query.tags.clone(),
            provider: "ai_generate".to_string(),
        }])
    }

    async fn fetch(&self, candidate: &SfxCandidate) -> Result<Vec<u8>> {
        if !self.config.enabled {
            bail!("AI SFX backend disabled");
        }
        let prompt = if let Some(stripped) = candidate.path_or_url.strip_prefix("ai_generate://") {
            stripped.to_string()
        } else {
            candidate.id.clone()
        };
        let estimated = self.estimate_cost(prompt.chars().count(), 0.40);
        self.budget_check(estimated)?;

        let endpoints = self.sorted_endpoints();
        if endpoints.is_empty() {
            bail!("no AI SFX endpoints configured");
        }
        let mut last_err = anyhow::anyhow!("no endpoint");
        for (url, auth_env, _cost) in endpoints {
            let token = auth_env
                .as_deref()
                .and_then(|k| env::var(k).ok())
                .filter(|v| !v.is_empty());
            match self
                .generate_via_http(&prompt, candidate.duration_secs, &url, token.as_deref())
                .await
            {
                Ok(bytes) => {
                    if estimated > 0.0 {
                        let milli = (estimated * 100_000.0) as u64;
                        CUMULATIVE_DAILY_COST_MILLICENTS.fetch_add(milli, Ordering::Relaxed);
                    }
                    return Ok(bytes);
                }
                Err(e) => {
                    tracing::warn!("AI SFX endpoint {url} failed: {e}");
                    last_err = e;
                }
            }
        }
        Err(last_err)
    }

    fn capabilities(&self) -> SfxProviderCapabilities {
        let is_paid = self.config.gpu_pool.iter().any(|e| e.cost_per_hour > 0.0);
        SfxProviderCapabilities {
            supports_search: true,
            supports_fetch: true,
            supports_generate: true,
            requires_network: self.config.mode != "local",
            is_paid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use movie_radio_types::{GpuPolicyConfig, GpuPoolEndpoint};

    #[test]
    fn test_https_enforce() {
        let b = AiGenerateSfxBackend::new(AiGenerateConfig {
            endpoint_url: "https://example.com".to_string(),
            ..AiGenerateConfig::default()
        })
        .expect("new");
        assert!(b.https_required("https://example.com").is_ok());
        assert!(b.https_required("http://127.0.0.1:8080").is_ok());
        assert!(b.https_required("http://example.com").is_err());
    }

    #[test]
    fn test_sort_prefer_free() {
        let cfg = AiGenerateConfig {
            gpu_policy: GpuPolicyConfig {
                prefer_free: true,
                allow_paid: true,
                ..GpuPolicyConfig::default()
            },
            gpu_pool: vec![
                GpuPoolEndpoint {
                    name: "paid".to_string(),
                    url: "https://paid.example.com".to_string(),
                    auth_env: None,
                    priority: 1,
                    cost_per_hour: 0.5,
                },
                GpuPoolEndpoint {
                    name: "free".to_string(),
                    url: "https://free.example.com".to_string(),
                    auth_env: None,
                    priority: 10,
                    cost_per_hour: 0.0,
                },
            ],
            ..AiGenerateConfig::default()
        };
        let b = AiGenerateSfxBackend::new(cfg).expect("new");
        let sorted = b.sorted_endpoints();
        assert_eq!(sorted[0].0, "https://free.example.com");
    }

    #[tokio::test]
    async fn test_paid_rejected_when_not_allowed() {
        AiGenerateSfxBackend::reset_daily_cost_for_test();
        let cfg = AiGenerateConfig {
            enabled: true,
            gpu_policy: GpuPolicyConfig {
                allow_paid: false,
                max_cost_per_job: 10.0,
                max_cost_per_day: 10.0,
                prefer_free: true,
            },
            gpu_pool: vec![GpuPoolEndpoint {
                name: "paid".to_string(),
                url: "https://paid.example.com".to_string(),
                auth_env: None,
                priority: 1,
                cost_per_hour: 0.5,
            }],
            ..AiGenerateConfig::default()
        };
        let b = AiGenerateSfxBackend::new(cfg).expect("new");
        let q = SfxQuery {
            tags: vec!["rain".to_string()],
            mood: None,
            duration_secs: None,
            prompt: Some("rain prompt that is long enough to cost".to_string()),
        };
        let res = b.search(&q).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("not allowed"));
    }

    #[tokio::test]
    async fn test_cost_exceeds_per_job() {
        AiGenerateSfxBackend::reset_daily_cost_for_test();
        let cfg = AiGenerateConfig {
            enabled: true,
            gpu_policy: GpuPolicyConfig {
                allow_paid: true,
                max_cost_per_job: 0.00001,
                max_cost_per_day: 10.0,
                prefer_free: true,
            },
            gpu_pool: vec![GpuPoolEndpoint {
                name: "paid".to_string(),
                url: "https://paid.example.com".to_string(),
                auth_env: None,
                priority: 1,
                cost_per_hour: 10.0,
            }],
            ..AiGenerateConfig::default()
        };
        let b = AiGenerateSfxBackend::new(cfg).expect("new");
        let q = SfxQuery {
            tags: Vec::new(),
            mood: None,
            duration_secs: None,
            prompt: Some("a".repeat(200)),
        };
        let res = b.search(&q).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("per-job"));
    }

    #[tokio::test]
    async fn test_prompt_too_long_fetch() {
        let cfg = AiGenerateConfig {
            enabled: true,
            endpoint_url: "http://127.0.0.1:9".to_string(),
            max_prompt_len: 5,
            ..AiGenerateConfig::default()
        };
        let b = AiGenerateSfxBackend::new(cfg).expect("new");
        let cand = SfxCandidate {
            id: "x".to_string(),
            path_or_url: "ai_generate://123456".to_string(),
            license: SfxLicense::Cc0,
            duration_secs: None,
            tags: Vec::new(),
            provider: "ai_generate".to_string(),
        };
        let res = b.fetch(&cand).await;
        assert!(res.is_err());
    }
}
