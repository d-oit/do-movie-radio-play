use super::*;
use movie_radio_types::{SfxLicense, SfxQuery};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct MockBackend {
    candidates: Vec<SfxCandidate>,
    bytes: Vec<u8>,
    searches: Arc<AtomicUsize>,
    fetches: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl SoundEffectBackend for MockBackend {
    async fn search(&self, _q: &SfxQuery) -> Result<Vec<SfxCandidate>> {
        self.searches.fetch_add(1, Ordering::SeqCst);
        Ok(self.candidates.clone())
    }
    async fn fetch(&self, _c: &SfxCandidate) -> Result<Vec<u8>> {
        self.fetches.fetch_add(1, Ordering::SeqCst);
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

fn candidate(id: &str, tags: &[&str]) -> SfxCandidate {
    SfxCandidate {
        id: id.to_string(),
        path_or_url: format!("https://example.test/{id}.wav"),
        license: SfxLicense::Cc0,
        duration_secs: None,
        tags: tags.iter().map(|t| (*t).to_string()).collect(),
        provider: "freesound".to_string(),
        attribution: None,
    }
}

#[tokio::test]
async fn test_search_all_deterministic() -> Result<()> {
    let counters = Arc::new(AtomicUsize::new(0));
    let mgr = SfxManager::new(vec![
        Box::new(MockBackend {
            candidates: vec![candidate("b", &["rain"])],
            bytes: vec![1, 2, 3],
            searches: Arc::clone(&counters),
            fetches: Arc::clone(&counters),
        }),
        Box::new(MockBackend {
            candidates: vec![candidate("a", &[])],
            bytes: vec![4, 5, 6],
            searches: Arc::clone(&counters),
            fetches: Arc::clone(&counters),
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
    let counters = Arc::new(AtomicUsize::new(0));
    let mgr = SfxManager::new(vec![Box::new(MockBackend {
        candidates: vec![candidate("rain_sfx", &["rain"])],
        bytes: wav,
        searches: Arc::clone(&counters),
        fetches: Arc::clone(&counters),
    })]);

    let trigger = SfxTrigger::AutoSelect {
        tags: vec!["rain".to_string()],
        mood: None,
    };
    let res = mgr
        .render_trigger(&trigger, 16000, Some(0.01))
        .await?
        .expect("rendered audio");
    assert!(!res.samples.is_empty());
    assert_eq!(res.provenance.provider_asset_id, "rain_sfx");
    Ok(())
}

#[tokio::test]
async fn offline_run_uses_cache_without_provider_calls() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let cache = SfxCache::new(dir.path())?;
    let wav = make_test_wav_bytes();
    cache.put(
        &candidate("rain_sfx", &["rain"]),
        &wav,
        SfxTransformation::default(),
        1_000_000,
    )?;
    let searches = Arc::new(AtomicUsize::new(0));
    let fetches = Arc::new(AtomicUsize::new(0));
    let mgr = SfxManager::new(vec![Box::new(MockBackend {
        candidates: vec![candidate("rain_sfx", &["rain"])],
        bytes: wav,
        searches: Arc::clone(&searches),
        fetches: Arc::clone(&fetches),
    })])
    .with_cache(cache)
    .offline(true);

    let trigger = SfxTrigger::AutoSelect {
        tags: vec!["rain".to_string()],
        mood: None,
    };
    let rendered = mgr
        .render_trigger(&trigger, 16000, Some(0.01))
        .await?
        .expect("cached audio");
    assert!(!rendered.samples.is_empty());
    assert_eq!(searches.load(Ordering::SeqCst), 0);
    assert_eq!(fetches.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn offline_mode_fails_clearly_when_required_asset_is_absent() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mgr = SfxManager::new(Vec::new())
        .with_cache(SfxCache::new(dir.path())?)
        .offline(true)
        .with_on_missing(MissingSfxPolicy::Fail);
    let trigger = SfxTrigger::AutoSelect {
        tags: vec!["rain".to_string()],
        mood: None,
    };
    let err = mgr
        .render_trigger(&trigger, 16000, None)
        .await
        .expect_err("missing asset must fail");
    assert!(err.to_string().contains("missing required SFX"));
    Ok(())
}

#[tokio::test]
async fn malformed_cached_asset_is_an_explicit_error() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let cache = SfxCache::new(dir.path())?;
    cache.put(
        &candidate("broken", &["rain"]),
        b"not audio",
        SfxTransformation::default(),
        1024,
    )?;
    let mgr = SfxManager::new(Vec::new())
        .with_cache(cache)
        .offline(true)
        .with_on_missing(MissingSfxPolicy::Fail);
    let trigger = SfxTrigger::AutoSelect {
        tags: vec!["rain".to_string()],
        mood: None,
    };
    assert!(mgr.render_trigger(&trigger, 16000, None).await.is_err());
    Ok(())
}
