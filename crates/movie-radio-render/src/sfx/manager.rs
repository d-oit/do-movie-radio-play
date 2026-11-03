use anyhow::{bail, Result};
use movie_radio_types::{
    MissingSfxPolicy, SfxAssetProvenance, SfxCandidate, SfxProviderCapabilities, SfxQuery,
    SfxTransformation, SfxTrigger, SoundEffectsConfig,
};

use super::{
    ai_generate::AiGenerateSfxBackend, cache::SfxCache, freesound::FreesoundBackend,
    local::LocalSfxBackend, processor::SfxProcessor, SoundEffectBackend,
};

/// A rendered effect together with the provenance needed to audit or replay it.
#[derive(Debug, Clone)]
pub struct RenderedSfx {
    pub samples: Vec<f32>,
    pub provenance: SfxAssetProvenance,
}

pub struct SfxManager {
    backends: Vec<Box<dyn SoundEffectBackend>>,
    cache: Option<SfxCache>,
    offline: bool,
    on_missing: MissingSfxPolicy,
    max_asset_bytes: u64,
    max_asset_duration_secs: f32,
}

impl SfxManager {
    pub fn new(backends: Vec<Box<dyn SoundEffectBackend>>) -> Self {
        Self {
            backends,
            cache: None,
            offline: false,
            on_missing: MissingSfxPolicy::Warn,
            max_asset_bytes: 20_000_000,
            max_asset_duration_secs: 300.0,
        }
    }

    pub fn with_cache(mut self, cache: SfxCache) -> Self {
        self.cache = Some(cache);
        self
    }

    pub fn offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }

    pub fn with_on_missing(mut self, policy: MissingSfxPolicy) -> Self {
        self.on_missing = policy;
        self
    }

    pub fn with_limits(mut self, max_asset_bytes: u64, max_asset_duration_secs: f32) -> Self {
        self.max_asset_bytes = max_asset_bytes;
        self.max_asset_duration_secs = max_asset_duration_secs;
        self
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

    pub async fn fetch_best(
        &self,
        query: &SfxQuery,
        sample_rate: u32,
    ) -> Result<(SfxCandidate, Vec<u8>, SfxAssetProvenance)> {
        let transformation = SfxTransformation {
            target_sample_rate_hz: sample_rate,
            target_duration_secs: query.duration_secs,
            ..SfxTransformation::default()
        };
        // A valid cached asset wins over any network call, which keeps a
        // resumed or offline run independent of provider availability.
        if let Some(cache) = &self.cache {
            if let Some(cached) = cache.search(query).into_iter().next() {
                let candidate = candidate_from(&cached.provenance);
                let mut provenance = cached.provenance;
                provenance.transformation = transformation.clone();
                return Ok((candidate, cached.bytes, provenance));
            }
        }
        if self.offline {
            bail!("offline mode: no cached SFX asset matches the query");
        }
        let candidates = self.search_all(query).await?;
        if candidates.is_empty() {
            bail!("no SFX candidates found");
        }
        for cand in candidates {
            if let Some(cache) = &self.cache {
                if let Some(cached) = cache.get(&cand.provider, &cand.id) {
                    let mut provenance = cached.provenance;
                    provenance.transformation = transformation.clone();
                    return Ok((cand, cached.bytes, provenance));
                }
            }
            for backend in &self.backends {
                if backend.capabilities().is_paid && cand.provider != "ai_generate" {
                    continue;
                }
                match backend.fetch(&cand).await {
                    Ok(bytes) => {
                        if bytes.len() as u64 > self.max_asset_bytes {
                            bail!(
                                "SFX asset {} exceeds size limit: {} > {}",
                                cand.id,
                                bytes.len(),
                                self.max_asset_bytes
                            );
                        }
                        let provenance = match &self.cache {
                            Some(cache) => cache.put(
                                &cand,
                                &bytes,
                                transformation.clone(),
                                self.max_asset_bytes,
                            )?,
                            None => {
                                super::cache::provenance_for(&cand, &bytes, transformation.clone())
                            }
                        };
                        return Ok((cand, bytes, provenance));
                    }
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
        if !config.enabled {
            return Ok(Self::new(backends));
        }

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

        Ok(Self::new(backends)
            .with_cache_config(config)
            .offline(config.offline)
            .with_on_missing(config.on_missing)
            .with_limits(config.max_asset_bytes, config.max_asset_duration_secs))
    }

    fn with_cache_config(mut self, config: &SoundEffectsConfig) -> Self {
        if config.cache_dir.is_empty() {
            return self;
        }
        match SfxCache::new(&config.cache_dir) {
            Ok(cache) => self.cache = Some(cache),
            Err(e) => tracing::warn!(error = %e, "SFX cache unavailable; continuing without it"),
        }
        self
    }

    pub async fn render_trigger(
        &self,
        trigger: &SfxTrigger,
        sample_rate: u32,
        duration_secs: Option<f32>,
    ) -> Result<Option<RenderedSfx>> {
        let Some(query) = trigger_to_query(trigger, duration_secs) else {
            return Ok(None);
        };

        if self.backends.is_empty() && self.cache.is_none() {
            return self.missing(trigger, "no SFX backends or cache configured");
        }

        match self.fetch_best(&query, sample_rate).await {
            Ok((_cand, bytes, provenance)) => {
                let samples =
                    Self::decode_and_mix_params(&bytes, sample_rate, query.duration_secs)?;
                let rendered_secs = samples.len() as f32 / sample_rate.max(1) as f32;
                if rendered_secs > self.max_asset_duration_secs {
                    bail!(
                        "SFX asset {} exceeds duration limit: {:.1}s > {:.1}s",
                        provenance.provider_asset_id,
                        rendered_secs,
                        self.max_asset_duration_secs
                    );
                }
                Ok(Some(RenderedSfx {
                    samples,
                    provenance,
                }))
            }
            Err(e) => self.missing(trigger, &e.to_string()),
        }
    }

    fn missing(&self, trigger: &SfxTrigger, reason: &str) -> Result<Option<RenderedSfx>> {
        match self.on_missing {
            MissingSfxPolicy::Fail => {
                bail!("missing required SFX for trigger {trigger:?}: {reason}")
            }
            MissingSfxPolicy::Warn => {
                tracing::warn!("missing SFX for trigger {trigger:?}: {reason}");
                Ok(None)
            }
        }
    }
}

fn candidate_from(provenance: &SfxAssetProvenance) -> SfxCandidate {
    SfxCandidate {
        id: provenance.provider_asset_id.clone(),
        path_or_url: provenance.source_url.clone(),
        license: provenance.license.clone(),
        duration_secs: provenance.transformation.target_duration_secs,
        tags: provenance.tags.clone(),
        provider: provenance.provider.clone(),
        attribution: provenance.attribution.clone(),
    }
}

fn trigger_to_query(trigger: &SfxTrigger, duration_secs: Option<f32>) -> Option<SfxQuery> {
    match trigger {
        SfxTrigger::None => None,
        SfxTrigger::AutoSelect { tags, mood } => Some(SfxQuery {
            tags: tags.clone(),
            mood: mood.clone(),
            duration_secs,
            prompt: None,
        }),
        SfxTrigger::Specific { sfx_id } => Some(SfxQuery {
            tags: vec![sfx_id.clone()],
            mood: None,
            duration_secs,
            prompt: Some(sfx_id.clone()),
        }),
        SfxTrigger::AiGenerate {
            prompt,
            duration_secs: dur,
        } => {
            let d = if *dur > 0.0 {
                Some(*dur)
            } else {
                duration_secs
            };
            Some(SfxQuery {
                tags: Vec::new(),
                mood: None,
                duration_secs: d,
                prompt: Some(prompt.clone()),
            })
        }
    }
}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;
