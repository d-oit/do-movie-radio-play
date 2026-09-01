use anyhow::{bail, Result};
use movie_radio_types::{SfxCandidate, SfxProviderCapabilities, SfxQuery};

use super::{processor::SfxProcessor, SoundEffectBackend};

pub struct SfxManager {
    backends: Vec<Box<dyn SoundEffectBackend>>,
}

impl SfxManager {
    pub fn new(backends: Vec<Box<dyn SoundEffectBackend>>) -> Self {
        Self { backends }
    }

    pub fn backend_count(&self) -> usize {
        self.backends.len()
    }

    pub fn capabilities(&self) -> Vec<SfxProviderCapabilities> {
        self.backends.iter().map(|b| b.capabilities()).collect()
    }

    pub async fn search_all(&self, query: &SfxQuery) -> Result<Vec<SfxCandidate>> {
        let mut all = Vec::new();
        let mut last_err: Option<anyhow::Error> = None;
        for backend in &self.backends {
            match backend.search(query).await {
                Ok(mut v) => all.append(&mut v),
                Err(e) => {
                    tracing::warn!("SFX search backend failed: {e}");
                    last_err = Some(e);
                }
            }
        }
        if all.is_empty() {
            if let Some(e) = last_err {
                bail!("all SFX backends failed: {e}");
            }
        }
        all.sort_by(|a, b| a.provider.cmp(&b.provider).then_with(|| a.id.cmp(&b.id)));
        Ok(all)
    }

    pub async fn fetch_best(&self, query: &SfxQuery) -> Result<(SfxCandidate, Vec<u8>)> {
        let candidates = self.search_all(query).await?;
        if candidates.is_empty() {
            bail!("no SFX candidates found");
        }
        for cand in candidates {
            for backend in &self.backends {
                if backend.capabilities().is_paid && cand.provider != "ai_generate" {
                    continue;
                }
                match backend.fetch(&cand).await {
                    Ok(bytes) => return Ok((cand, bytes)),
                    Err(e) => {
                        tracing::debug!("fetch via {} failed for {}: {e}", cand.provider, cand.id);
                    }
                }
            }
        }
        bail!("all SFX fetches failed");
    }

    pub fn decode_and_mix_params(
        bytes: &[u8],
        sample_rate: u32,
        duration_secs: Option<f32>,
    ) -> Result<Vec<f32>> {
        SfxProcessor::process(bytes, sample_rate, duration_secs, 0.9, 10)
    }

    pub fn create_faded_track(
        samples: Vec<f32>,
        sample_rate: u32,
        duration_secs: Option<f32>,
    ) -> Vec<f32> {
        if let Some(dur) = duration_secs {
            let target_len = (sample_rate as f32 * dur).round() as usize;
            SfxProcessor::trim_or_pad(samples, target_len)
        } else {
            samples
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use movie_radio_types::{SfxLicense, SfxQuery};

    struct MockBackend {
        candidates: Vec<SfxCandidate>,
        bytes: Vec<u8>,
    }

    #[async_trait::async_trait]
    impl SoundEffectBackend for MockBackend {
        async fn search(&self, _q: &SfxQuery) -> Result<Vec<SfxCandidate>> {
            Ok(self.candidates.clone())
        }
        async fn fetch(&self, _c: &SfxCandidate) -> Result<Vec<u8>> {
            Ok(self.bytes.clone())
        }
        fn capabilities(&self) -> SfxProviderCapabilities {
            SfxProviderCapabilities {
                supports_search: true,
                supports_fetch: true,
                supports_generate: false,
                requires_network: false,
                is_paid: false,
            }
        }
    }

    #[tokio::test]
    async fn test_search_all_deterministic() -> Result<()> {
        let mgr = SfxManager::new(vec![
            Box::new(MockBackend {
                candidates: vec![SfxCandidate {
                    id: "b".to_string(),
                    path_or_url: "b.wav".to_string(),
                    license: SfxLicense::Cc0,
                    duration_secs: None,
                    tags: vec!["rain".to_string()],
                    provider: "local".to_string(),
                }],
                bytes: vec![1, 2, 3],
            }),
            Box::new(MockBackend {
                candidates: vec![SfxCandidate {
                    id: "a".to_string(),
                    path_or_url: "a.wav".to_string(),
                    license: SfxLicense::Cc0,
                    duration_secs: None,
                    tags: Vec::new(),
                    provider: "local".to_string(),
                }],
                bytes: vec![4, 5, 6],
            }),
        ]);
        let res = mgr.search_all(&SfxQuery::default()).await?;
        assert_eq!(res[0].id, "a");
        assert_eq!(res[1].id, "b");
        Ok(())
    }
}
