use anyhow::{bail, Result};
use movie_radio_types::{
    SfxCandidate, SfxProviderCapabilities, SfxQuery, SfxTrigger, SoundEffectsConfig,
};

use super::{
    ai_generate::AiGenerateSfxBackend, freesound::FreesoundBackend, local::LocalSfxBackend,
    processor::SfxProcessor, SoundEffectBackend,
};

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

    pub fn from_config(config: &SoundEffectsConfig) -> Result<Self> {
        let mut backends: Vec<Box<dyn SoundEffectBackend>> = Vec::new();

        if config.enabled {
            if let Ok(local) = LocalSfxBackend::new(config.local.clone()) {
                backends.push(Box::new(local));
            }
            if config.freesound.enabled {
                if let Ok(freesound) = FreesoundBackend::new(config.freesound.clone()) {
                    backends.push(Box::new(freesound));
                }
            }
            if config.ai_generate.enabled {
                if let Ok(ai) = AiGenerateSfxBackend::new(config.ai_generate.clone()) {
                    backends.push(Box::new(ai));
                }
            }
        }

        Ok(Self { backends })
    }

    pub async fn render_trigger(
        &self,
        trigger: &SfxTrigger,
        sample_rate: u32,
        duration_secs: Option<f32>,
    ) -> Result<Option<Vec<f32>>> {
        let query = match trigger {
            SfxTrigger::None => return Ok(None),
            SfxTrigger::AutoSelect { tags, mood } => SfxQuery {
                tags: tags.clone(),
                mood: mood.clone(),
                duration_secs,
                prompt: None,
            },
            SfxTrigger::Specific { sfx_id } => SfxQuery {
                tags: vec![sfx_id.clone()],
                mood: None,
                duration_secs,
                prompt: Some(sfx_id.clone()),
            },
            SfxTrigger::AiGenerate {
                prompt,
                duration_secs: dur,
            } => {
                let d = if *dur > 0.0 {
                    Some(*dur)
                } else {
                    duration_secs
                };
                SfxQuery {
                    tags: Vec::new(),
                    mood: None,
                    duration_secs: d,
                    prompt: Some(prompt.clone()),
                }
            }
        };

        if self.backends.is_empty() {
            return Ok(None);
        }

        match self.fetch_best(&query).await {
            Ok((_cand, bytes)) => {
                let samples =
                    Self::decode_and_mix_params(&bytes, sample_rate, query.duration_secs)?;
                Ok(Some(samples))
            }
            Err(e) => {
                tracing::warn!("Failed to fetch SFX for trigger {:?}: {e}", trigger);
                Ok(None)
            }
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

    fn make_test_wav_bytes() -> Vec<u8> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut buf = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut buf);
        let mut w = hound::WavWriter::new(&mut cursor, spec).expect("writer");
        for i in 0..160 {
            let s = (i as f32 * 0.1).sin() * 1000.0;
            w.write_sample(s as i16).expect("sample");
        }
        w.finalize().expect("finalize");
        buf
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

    #[test]
    fn test_from_config() -> Result<()> {
        let cfg = SoundEffectsConfig::default();
        let mgr = SfxManager::from_config(&cfg)?;
        // Default config has local enabled (if assets dir exists or default) and others disabled
        assert!(mgr.backend_count() >= 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_render_trigger_none() -> Result<()> {
        let mgr = SfxManager::new(Vec::new());
        let res = mgr.render_trigger(&SfxTrigger::None, 16000, None).await?;
        assert!(res.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_render_trigger_success() -> Result<()> {
        let wav = make_test_wav_bytes();
        let mgr = SfxManager::new(vec![Box::new(MockBackend {
            candidates: vec![SfxCandidate {
                id: "rain_sfx".to_string(),
                path_or_url: "rain.wav".to_string(),
                license: SfxLicense::Cc0,
                duration_secs: Some(1.0),
                tags: vec!["rain".to_string()],
                provider: "local".to_string(),
            }],
            bytes: wav,
        })]);

        let trigger = SfxTrigger::AutoSelect {
            tags: vec!["rain".to_string()],
            mood: None,
        };
        let res = mgr
            .render_trigger(&trigger, 16000, Some(0.01))
            .await?
            .expect("rendered audio");
        assert!(!res.is_empty());
        Ok(())
    }
}
