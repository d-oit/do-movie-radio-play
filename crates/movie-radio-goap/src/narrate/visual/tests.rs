use super::*;
use crate::narrate::NarrationEvidence;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use movie_radio_types::{Segment, SegmentKind, VisualConfig, VisualGap};

const GAP_START: u64 = 2_000;
const GAP_END: u64 = 8_000;

fn config(provider: &str, cache: &std::path::Path) -> VisualConfig {
    VisualConfig {
        provider: provider.to_string(),
        model: "test-vision".to_string(),
        base_url: "http://localhost:1234/v1".to_string(),
        cache_dir: Some(cache.to_string_lossy().to_string()),
        timeout_secs: 5,
        ..VisualConfig::default()
    }
}

fn timeline() -> TimelineOutput {
    TimelineOutput {
        file: "movie.mkv".to_string(),
        analysis_sample_rate: 16_000,
        frame_ms: 20,
        segments: vec![
            Segment {
                start_ms: 0,
                end_ms: 2_000,
                kind: SegmentKind::Speech,
                confidence: 0.9,
                tags: vec!["speech".to_string()],
                prompt: None,
                sfx_trigger: None,
            },
            Segment {
                start_ms: GAP_START,
                end_ms: GAP_END,
                kind: SegmentKind::NonVoice,
                confidence: 0.9,
                tags: vec!["ambience".to_string()],
                prompt: None,
                sfx_trigger: None,
            },
            Segment {
                start_ms: GAP_END,
                end_ms: 10_000,
                kind: SegmentKind::Speech,
                confidence: 0.9,
                tags: vec!["speech".to_string()],
                prompt: None,
                sfx_trigger: None,
            },
        ],
    }
}

fn gaps() -> Vec<VisualGap> {
    vec![VisualGap {
        start_ms: GAP_START,
        end_ms: GAP_END,
        confidence: 0.9,
        reason: "long pause".to_string(),
        priority: 1,
        tags: vec!["ambience".to_string()],
        trend: None,
    }]
}

struct FakeExtractor {
    frames: Vec<(u64, Vec<u8>)>,
    calls: Arc<AtomicUsize>,
}

impl FakeExtractor {
    fn new(frames: &[(u64, &[u8])]) -> (Self, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        (
            Self {
                frames: frames
                    .iter()
                    .map(|(ts, bytes)| (*ts, bytes.to_vec()))
                    .collect(),
                calls: Arc::clone(&calls),
            },
            calls,
        )
    }
}

#[async_trait::async_trait]
impl FrameExtractor for FakeExtractor {
    async fn extract_window(
        &self,
        _movie: &Path,
        _window: NarrationWindow,
        _policy: &SamplingPolicy,
        budget: u32,
        out_dir: &Path,
    ) -> Result<Vec<FrameSample>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        std::fs::create_dir_all(out_dir)?;
        self.frames
            .iter()
            .take(budget as usize)
            .enumerate()
            .map(|(index, (timestamp_ms, bytes))| {
                let path = out_dir.join(format!("frame-{index:03}.jpg"));
                std::fs::write(&path, bytes)?;
                Ok(FrameSample {
                    frame_ref: format!("frame-{timestamp_ms}"),
                    timestamp_ms: *timestamp_ms,
                    path,
                    bytes: bytes.clone(),
                })
            })
            .collect()
    }
}

#[derive(Clone, Default)]
struct SeenRequest {
    language: String,
    start_ms: u64,
    end_ms: u64,
    frame_refs: Vec<String>,
}

struct FakeProvider {
    model: String,
    reply: String,
    calls: Arc<AtomicUsize>,
    seen: Arc<Mutex<Vec<SeenRequest>>>,
}

impl FakeProvider {
    fn new(reply: impl Into<String>) -> (Self, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        (
            Self {
                model: "test-vision".to_string(),
                reply: reply.into(),
                calls: Arc::clone(&calls),
                seen: Arc::new(Mutex::new(Vec::new())),
            },
            calls,
        )
    }
}

#[async_trait::async_trait]
impl VisionProvider for FakeProvider {
    fn name(&self) -> &str {
        "fake"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn describe(&self, request: &VisionRequest<'_>) -> Result<String, VisionError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut seen) = self.seen.lock() {
            seen.push(SeenRequest {
                language: request.language.to_string(),
                start_ms: request.window_start_ms,
                end_ms: request.window_end_ms,
                frame_refs: request.frames.iter().map(|f| f.frame_ref.clone()).collect(),
            });
        }
        Ok(self.reply.clone())
    }
}

struct SlowProvider;

#[async_trait::async_trait]
impl VisionProvider for SlowProvider {
    fn name(&self) -> &str {
        "slow"
    }

    fn model(&self) -> &str {
        "slow-vision"
    }

    async fn describe(&self, _request: &VisionRequest<'_>) -> Result<String, VisionError> {
        tokio::time::sleep(Duration::from_millis(1_500)).await;
        Ok(String::new())
    }
}

fn grounded_reply() -> String {
    r#"{"frames":[{"frame_ref":"frame-2500","timestamp_ms":2500,
        "description":"Eine Frau öffnet die Tür.",
        "entities":["Frau","Tür"],"actions":["öffnet die Tür"],
        "confidence":0.92,"uncertainty":""}]}"#
        .to_string()
}

fn narrator(
    cfg: VisualConfig,
    extractor: Arc<dyn FrameExtractor>,
    provider: Arc<dyn VisionProvider>,
) -> VisualNarrator {
    VisualNarrator::new(cfg, "de")
        .with_extractor(extractor)
        .with_provider(provider)
}

#[tokio::test]
async fn known_visual_events_produce_grounded_description() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (extractor, _) = FakeExtractor::new(&[(2_500, b"jpeg")]);
    let (provider, _) = FakeProvider::new(grounded_reply());
    let seen = Arc::clone(&provider.seen);
    let narrator = narrator(
        config("openai", dir.path()),
        Arc::new(extractor),
        Arc::new(provider),
    );

    let outcome = narrator
        .generate(Path::new("movie.mkv"), &timeline(), &gaps())
        .await
        .expect("visual narration");

    assert_eq!(outcome.scripts.len(), 1);
    assert!(
        outcome.scripts[0].text.contains("Tür"),
        "{:?}",
        outcome.scripts
    );
    assert!(matches!(
        outcome.scripts[0].evidence,
        NarrationEvidence::Visual { .. }
    ));
    assert!(outcome.scripts[0].gap_start_ms >= GAP_START);
    assert!(outcome.scripts[0].gap_end_ms <= GAP_END);
    assert_eq!(outcome.report.visual_windows, 1);
    assert_eq!(outcome.report.audio_only_windows, 0);
    assert_eq!(outcome.report.provider, "fake");
    let window = &outcome.report.windows[0];
    assert_eq!(window.source, EvidenceSource::Visual);
    assert_eq!(window.frames[0].timestamp_ms, 2_500);
    assert!(window.frames[0].description.contains("Tür"));

    let seen = seen.lock().expect("seen request");
    assert_eq!(seen[0].language, "de");
    assert_eq!(seen[0].start_ms, GAP_START);
    assert_eq!(seen[0].end_ms, GAP_END);
    assert_eq!(seen[0].frame_refs, vec!["frame-2500".to_string()]);
}

#[tokio::test]
async fn ambiguous_frame_yields_uncertainty_and_audio_fallback() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (extractor, _) = FakeExtractor::new(&[(2_500, b"jpeg")]);
    let (provider, _) = FakeProvider::new(
        r#"{"frames":[{"frame_ref":"frame-2500","timestamp_ms":2500,
            "description":"Ein Mann ist traurig.","confidence":0.2,
            "uncertainty":"Das Bild ist dunkel und verdeckt."}]}"#,
    );
    let narrator = narrator(
        config("openai", dir.path()),
        Arc::new(extractor),
        Arc::new(provider),
    );

    let outcome = narrator
        .generate(Path::new("movie.mkv"), &timeline(), &gaps())
        .await
        .expect("visual narration");

    let window = &outcome.report.windows[0];
    assert_eq!(window.source, EvidenceSource::AudioOnly);
    assert!(window
        .frames
        .iter()
        .any(|f| f.uncertainty.iter().any(|u| u.contains("dunkel"))));
    assert!(window
        .rejected_claims
        .iter()
        .any(|c| c.reason == GroundingIssue::EmotionClaim));
    assert!(
        outcome.scripts[0].text.to_lowercase().contains("geräusch")
            || !outcome.scripts[0].text.to_lowercase().contains("traurig")
    );
    assert!(matches!(
        outcome.scripts[0].evidence,
        NarrationEvidence::AudioOnly { .. }
    ));
}

#[tokio::test]
async fn unsupported_identity_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (extractor, _) = FakeExtractor::new(&[(2_500, b"jpeg")]);
    let (provider, _) = FakeProvider::new(
        r#"{"frames":[{"frame_ref":"frame-2500","description":"Der Mann heißt Karl und öffnet die Tür.","confidence":0.95}]}"#,
    );
    let narrator = narrator(
        config("openai", dir.path()),
        Arc::new(extractor),
        Arc::new(provider),
    );

    let outcome = narrator
        .generate(Path::new("movie.mkv"), &timeline(), &gaps())
        .await
        .expect("visual narration");

    assert!(outcome.report.windows[0]
        .rejected_claims
        .iter()
        .any(|c| c.reason == GroundingIssue::UnsupportedIdentity));
}

#[tokio::test]
async fn no_provider_keeps_audio_only_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (extractor, extractor_calls) = FakeExtractor::new(&[(2_500, b"jpeg")]);
    let narrator =
        VisualNarrator::new(config("none", dir.path()), "de").with_extractor(Arc::new(extractor));

    let outcome = narrator
        .generate(Path::new("movie.mkv"), &timeline(), &gaps())
        .await
        .expect("audio narration");

    assert_eq!(extractor_calls.load(Ordering::SeqCst), 0);
    assert_eq!(outcome.scripts.len(), 1);
    assert!(matches!(
        &outcome.scripts[0].evidence,
        NarrationEvidence::AudioOnly { reason } if reason.contains("no visual provider")
    ));
    assert_eq!(outcome.report.provider, "none");
    assert_eq!(outcome.report.audio_only_windows, 1);
}

#[tokio::test]
async fn malformed_provider_output_falls_back() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (extractor, _) = FakeExtractor::new(&[(2_500, b"jpeg")]);
    let (provider, _) = FakeProvider::new("I cannot describe that.");
    let narrator = narrator(
        config("openai", dir.path()),
        Arc::new(extractor),
        Arc::new(provider),
    );

    let outcome = narrator
        .generate(Path::new("movie.mkv"), &timeline(), &gaps())
        .await
        .expect("fallback");

    assert_eq!(outcome.report.windows[0].source, EvidenceSource::AudioOnly);
    assert!(outcome.report.windows[0]
        .fallback_reason
        .as_deref()
        .is_some_and(|r| r.contains("vision stage failed")));
    assert!(matches!(
        &outcome.scripts[0].evidence,
        NarrationEvidence::AudioOnly { reason } if reason.contains("vision stage failed")
    ));
}

#[tokio::test]
async fn provider_timeout_falls_back() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (extractor, _) = FakeExtractor::new(&[(2_500, b"jpeg")]);
    let mut cfg = config("openai", dir.path());
    cfg.timeout_secs = 1;
    let narrator = VisualNarrator::new(cfg, "de")
        .with_extractor(Arc::new(extractor))
        .with_provider(Arc::new(SlowProvider));

    let outcome = narrator
        .generate(Path::new("movie.mkv"), &timeline(), &gaps())
        .await
        .expect("timeout fallback");

    let reason = outcome.report.windows[0]
        .fallback_reason
        .as_deref()
        .unwrap_or_default();
    assert!(reason.contains("timed out"), "{reason}");
}

#[tokio::test]
async fn empty_frames_fall_back_without_calling_provider() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (extractor, _) = FakeExtractor::new(&[]);
    let (provider, provider_calls) = FakeProvider::new(grounded_reply());
    let narrator = narrator(
        config("openai", dir.path()),
        Arc::new(extractor),
        Arc::new(provider),
    );

    let outcome = narrator
        .generate(Path::new("movie.mkv"), &timeline(), &gaps())
        .await
        .expect("empty frame fallback");

    assert_eq!(provider_calls.load(Ordering::SeqCst), 0);
    assert!(outcome.report.windows[0]
        .fallback_reason
        .as_deref()
        .is_some_and(|r| r.contains("no representative frames")));
}

#[tokio::test]
async fn analysis_cache_reuses_frames_and_provider_result() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (extractor, extractor_calls) = FakeExtractor::new(&[(2_500, b"jpeg")]);
    let (provider, provider_calls) = FakeProvider::new(grounded_reply());
    let narrator = narrator(
        config("openai", dir.path()),
        Arc::new(extractor),
        Arc::new(provider),
    );

    let first = narrator
        .generate(Path::new("movie.mkv"), &timeline(), &gaps())
        .await
        .expect("first run");
    let second = narrator
        .generate(Path::new("movie.mkv"), &timeline(), &gaps())
        .await
        .expect("second run");

    assert_eq!(first.report.visual_windows, 1);
    assert_eq!(second.report.visual_windows, 1);
    assert_eq!(extractor_calls.load(Ordering::SeqCst), 1);
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);
}
