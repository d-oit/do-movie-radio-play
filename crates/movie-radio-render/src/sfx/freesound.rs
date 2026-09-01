use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use movie_radio_types::{
    FreesoundConfig, SfxCandidate, SfxLicense, SfxProviderCapabilities, SfxQuery,
};
use reqwest::Client;
use serde::Deserialize;
use std::env;
use std::time::Duration;

use super::SoundEffectBackend;

const FREESOUND_SEARCH_URL: &str = "https://freesound.org/apiv2/search/";
const FREESOUND_SOUND_URL: &str = "https://freesound.org/apiv2/sounds/";

#[derive(Debug, Deserialize)]
struct FreesoundSearchResponse {
    #[allow(dead_code)]
    count: u32,
    results: Vec<FreesoundResult>,
}

#[derive(Debug, Deserialize)]
struct FreesoundResult {
    id: u64,
    #[allow(dead_code)]
    name: String,
    tags: Vec<String>,
    license: String,
    #[serde(default)]
    previews: Option<FreesoundPreviews>,
}

#[derive(Debug, Deserialize)]
struct FreesoundPreviews {
    #[serde(rename = "preview-hq-mp3")]
    preview_hq_mp3: Option<String>,
    #[serde(rename = "preview-lq-mp3")]
    preview_lq_mp3: Option<String>,
}

pub struct FreesoundBackend {
    config: FreesoundConfig,
    client: Client,
}

impl FreesoundBackend {
    pub fn new(config: FreesoundConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .context("build freesound client")?;
        Ok(Self { config, client })
    }

    #[cfg(test)]
    pub fn with_client_for_test(config: FreesoundConfig, client: Client) -> Self {
        Self { config, client }
    }

    fn resolve_token(&self) -> Option<String> {
        if self.config.api_key_env.is_empty() {
            return None;
        }
        env::var(&self.config.api_key_env)
            .ok()
            .filter(|v| !v.is_empty())
    }

    fn sanitize(&self, msg: &str) -> String {
        let mut clean = msg.to_string();
        if let Some(token) = self.resolve_token() {
            clean = clean.replace(&token, "[REDACTED]");
        }
        clean
    }

    fn is_license_allowed(&self, license: &SfxLicense) -> bool {
        license.is_allowed(&self.config.allowed_licenses)
    }

    fn https_enforce(&self, url: &str) -> Result<()> {
        if !url.starts_with("https://") {
            bail!("Freesound: HTTPS required, got {url}");
        }
        Ok(())
    }
}

fn build_query_string(query: &SfxQuery) -> String {
    let mut parts = Vec::new();
    for tag in &query.tags {
        if !tag.is_empty() {
            parts.push(tag.clone());
        }
    }
    if let Some(mood) = &query.mood {
        if !mood.is_empty() {
            parts.push(mood.clone());
        }
    }
    if let Some(prompt) = &query.prompt {
        if !prompt.is_empty() {
            for w in prompt.split_whitespace().take(5) {
                parts.push(w.to_string());
            }
        }
    }
    if parts.is_empty() {
        "ambience".to_string()
    } else {
        parts.join(" ")
    }
}

#[async_trait]
impl SoundEffectBackend for FreesoundBackend {
    async fn search(&self, query: &SfxQuery) -> Result<Vec<SfxCandidate>> {
        if !self.config.enabled {
            bail!("Freesound backend disabled");
        }
        let token = self.resolve_token();
        if token.is_none() {
            bail!(
                "Freesound API key not configured (env {})",
                self.config.api_key_env
            );
        }
        self.https_enforce(FREESOUND_SEARCH_URL)?;
        let q = build_query_string(query);
        let mut req = self.client.get(FREESOUND_SEARCH_URL).query(&[
            ("query", q.as_str()),
            ("page_size", "10"),
            ("fields", "id,name,tags,license,previews"),
        ]);
        if let Some(t) = token.as_deref() {
            req = req.header("Authorization", format!("Token {t}"));
        }
        req = req.header("Accept", "application/json");
        let resp = req.send().await.map_err(|e| {
            let m = self.sanitize(&e.to_string());
            anyhow::anyhow!("freesound search connect failed: {m}")
        })?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let clean = self.sanitize(&body);
            bail!("freesound search http {status}: {clean}");
        }
        let body: FreesoundSearchResponse = resp.json().await.context("freesound search decode")?;
        let mut out = Vec::new();
        for r in body.results {
            let lic = SfxLicense::from_str(&r.license);
            if !self.is_license_allowed(&lic) {
                continue;
            }
            let preview = r
                .previews
                .as_ref()
                .and_then(|p| {
                    p.preview_hq_mp3
                        .clone()
                        .or_else(|| p.preview_lq_mp3.clone())
                })
                .unwrap_or_else(|| format!("{}/{}/", FREESOUND_SOUND_URL, r.id));
            out.push(SfxCandidate {
                id: r.id.to_string(),
                path_or_url: preview,
                license: lic,
                duration_secs: None,
                tags: r.tags,
                provider: "freesound".to_string(),
            });
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    async fn fetch(&self, candidate: &SfxCandidate) -> Result<Vec<u8>> {
        if !candidate.path_or_url.starts_with("https://") {
            bail!(
                "Freesound fetch requires HTTPS, got {}",
                candidate.path_or_url
            );
        }
        if !self.is_license_allowed(&candidate.license) {
            bail!("license {:?} not allowed", candidate.license);
        }
        let token = self.resolve_token();
        let mut req = self.client.get(candidate.path_or_url.as_str());
        if let Some(t) = token.as_deref() {
            req = req.header("Authorization", format!("Token {t}"));
        }
        let resp = req.send().await.map_err(|e| {
            let m = self.sanitize(&e.to_string());
            anyhow::anyhow!("freesound fetch connect failed: {m}")
        })?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let clean = self.sanitize(&body);
            bail!("freesound fetch http {status}: {clean}");
        }
        let bytes = resp.bytes().await.context("freesound fetch bytes")?;
        if bytes.len() as u64 > self.config.max_audio_bytes {
            bail!(
                "freesound audio too large: {} > {}",
                bytes.len(),
                self.config.max_audio_bytes
            );
        }
        Ok(bytes.to_vec())
    }

    fn capabilities(&self) -> SfxProviderCapabilities {
        SfxProviderCapabilities {
            supports_search: true,
            supports_fetch: true,
            supports_generate: false,
            requires_network: true,
            is_paid: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_license_filtering() {
        let cfg = FreesoundConfig {
            enabled: true,
            allowed_licenses: vec!["cc0".to_string()],
            ..FreesoundConfig::default()
        };
        let backend = FreesoundBackend::new(cfg).expect("backend");
        assert!(backend.is_license_allowed(&SfxLicense::Cc0));
        assert!(!backend.is_license_allowed(&SfxLicense::CcByNc));
    }

    #[test]
    fn test_https_enforce() {
        let backend = FreesoundBackend::new(FreesoundConfig::default()).expect("backend");
        assert!(backend.https_enforce("https://example.com/a").is_ok());
        assert!(backend.https_enforce("http://example.com/a").is_err());
    }

    const ENV_FRESOUND_TEST_KEY: &str = "FREESOUND_API_KEY_TEST_REDACT";

    #[test]
    fn test_sanitize_redacts_token() {
        env::set_var(ENV_FRESOUND_TEST_KEY, "secret-token-xyz-123");
        let cfg = FreesoundConfig {
            api_key_env: ENV_FRESOUND_TEST_KEY.to_string(),
            ..FreesoundConfig::default()
        };
        let backend = FreesoundBackend::new(cfg).expect("backend");
        let msg = "failed with secret-token-xyz-123 inside";
        let clean = backend.sanitize(msg);
        assert!(!clean.contains("secret-token-xyz-123"));
        assert!(clean.contains("[REDACTED]"));
        env::remove_var(ENV_FRESOUND_TEST_KEY);
    }

    #[test]
    fn test_build_query_string_deterministic() {
        let q = SfxQuery {
            tags: vec!["rain".to_string(), "forest".to_string()],
            mood: Some("calm".to_string()),
            duration_secs: None,
            prompt: Some("gentle evening".to_string()),
        };
        assert_eq!(build_query_string(&q), "rain forest calm gentle evening");
        assert_eq!(build_query_string(&SfxQuery::default()), "ambience");
    }

    #[tokio::test]
    async fn test_fetch_rejects_http() {
        let backend = FreesoundBackend::new(FreesoundConfig::default()).expect("backend");
        let cand = SfxCandidate {
            id: "1".to_string(),
            path_or_url: "http://freesound.org/preview.mp3".to_string(),
            license: SfxLicense::Cc0,
            duration_secs: None,
            tags: Vec::new(),
            provider: "freesound".to_string(),
        };
        assert!(backend.fetch(&cand).await.is_err());
    }
}
