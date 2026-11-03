//! The visual narration path, exercised on a real film with a stub vision model.
//!
//! This isolates the one thing that needs a paid API key (model *quality*) from
//! everything else in the path: sampling real frames out of a real movie,
//! building the prompt, parsing and validating the response, and emitting a
//! narration script. If this passes, a provider key is the only missing piece.
//!
//! Run with:
//! ```text
//! VISUAL_MOVIE=testdata/raw/sprite_fright_2021.webm \
//! VISUAL_TIMELINE=/tmp/sf_tagged.json \
//! cargo test -p movie-radio-goap --test visual_narration_pipeline -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use movie_radio_goap::gaps::GapIdentifier;
use movie_radio_goap::narrate::visual::{
    FfmpegFrameExtractor, SamplingPolicy, VisionError, VisionErrorKind, VisionProvider,
    VisionRequest, VisualNarrator,
};
use movie_radio_types::{TimelineOutput, VisualConfig};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

/// Records what it was asked and replies with a schema-valid observation whose
/// `frame_ref` matches the first frame it actually received.
struct EchoProvider {
    seen: Arc<Mutex<Vec<(usize, usize)>>>,
}

#[async_trait]
impl VisionProvider for EchoProvider {
    fn name(&self) -> &'static str {
        "echo"
    }
    fn model(&self) -> &'static str {
        "stub"
    }
    async fn describe(&self, request: &VisionRequest<'_>) -> Result<String, VisionError> {
        self.seen
            .lock()
            .expect("seen")
            .push((request.frames.len(), request.max_words));
        let first = request.frames.first().ok_or_else(|| {
            VisionError::new(VisionErrorKind::MalformedOutput, "no frames supplied")
        })?;
        Ok(format!(
            r#"{{"frames":[{{"frame_ref":"{}","timestamp_ms":{},
                "description":"Stub observation for a sampled frame.",
                "entities":["stub"],"actions":["stands"],
                "confidence":0.9,"uncertainty":""}}]}}"#,
            first.frame_ref, first.timestamp_ms
        ))
    }
}

#[tokio::test]
#[ignore = "needs a downloaded film plus ffmpeg; research tool, not a CI gate"]
async fn visual_path_produces_a_script_from_real_frames() {
    let movie = PathBuf::from(
        std::env::var("VISUAL_MOVIE")
            .unwrap_or_else(|_| "testdata/raw/sprite_fright_2021.webm".to_string()),
    );
    let movie = if movie.is_absolute() {
        movie
    } else {
        repo_root().join(movie)
    };
    if !movie.exists() {
        println!("skipping: {movie:?} not present");
        return;
    }
    let timeline_path = PathBuf::from(
        std::env::var("VISUAL_TIMELINE").unwrap_or_else(|_| "/tmp/sf_tagged.json".to_string()),
    );
    let timeline: TimelineOutput =
        serde_json::from_str(&std::fs::read_to_string(&timeline_path).expect("read timeline"))
            .expect("parse timeline");

    let mut identifier = GapIdentifier::new();
    identifier.high_confidence_threshold = 0.4;
    let analysis = identifier
        .identify_gaps(&timeline, None)
        .expect("identify gaps");

    let cache = tempfile::tempdir().expect("tempdir");
    let mut config = VisualConfig {
        provider: "openai".to_string(),
        ..VisualConfig::default()
    };
    config.cache_dir = Some(
        cache
            .path()
            .to_str()
            .expect("cache path is utf-8")
            .to_string(),
    );

    let seen = Arc::new(Mutex::new(Vec::new()));
    let narrator = VisualNarrator::new(config, "en")
        .with_extractor(Arc::new(FfmpegFrameExtractor::default()))
        .with_provider(Arc::new(EchoProvider {
            seen: Arc::clone(&seen),
        }));

    let outcome = narrator
        .generate(&movie, &timeline, &analysis.gaps)
        .await
        .expect("visual narration");

    println!(
        "\n  gaps={} scripts={} visual_windows={} audio_only_windows={}",
        analysis.gaps.len(),
        outcome.scripts.len(),
        outcome.report.visual_windows,
        outcome.report.audio_only_windows
    );
    let seen = seen.lock().expect("seen");
    for (frames, words) in seen.iter() {
        println!("  provider saw {frames} frame(s), word budget {words}");
    }
    assert!(
        !seen.is_empty(),
        "the stub provider was never called: frame sampling produced nothing"
    );
    assert!(
        seen.iter().any(|(frames, _)| *frames > 0),
        "frames were sampled but the provider received none"
    );
    assert!(
        !outcome.scripts.is_empty(),
        "real frames + valid model output produced no narration script"
    );
}

#[test]
#[ignore = "research tool"]
fn sampling_policy_defaults_are_bounded() {
    // Cost guard: the whole film must not fan out into unbounded API calls.
    let config = VisualConfig::default();
    let policy = SamplingPolicy::from_config(&config);
    println!("{policy:?}");
    assert!(
        config.max_total_frames <= 60,
        "per-film frame budget is unbounded"
    );
    assert!(
        config.max_frames_per_window <= 3,
        "per-window frame budget is too high"
    );
}
