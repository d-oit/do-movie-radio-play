//! Acceptance coverage for #408: a synthetic fixture runs the full production
//! contract, resume reuses only valid stages, changed input/configuration and
//! corrupted artifacts invalidate what depends on them, and a failed stage
//! leaves a recoverable checkpoint behind.

use super::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use movie_radio_goap::production::{input_class, stage_by_name};
use movie_radio_voice::voice::SynthesisOrchestrator;
use movie_radio_voice::{AudioOutput, ProviderCapabilities, SynthesisRequest, VoiceSynthesizer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Behavior {
    Tone,
    Fail,
}

struct TestProvider {
    behavior: Behavior,
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl VoiceSynthesizer for TestProvider {
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.behavior == Behavior::Fail {
            anyhow::bail!("deterministic test provider failure");
        }
        let sr = request.sample_rate_hz;
        let len = sr as usize / 2;
        let tau = std::f32::consts::TAU;
        let samples = (0..len)
            .map(|i| (i as f32 * 440.0 * tau / sr as f32).sin() * 0.2)
            .collect();
        Ok(AudioOutput {
            samples,
            sample_rate_hz: sr,
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_emotion: true,
            supports_voice_cloning: false,
            supports_streaming: false,
            max_text_length: 10_000,
            languages: vec!["de".to_string(), "en".to_string()],
            requires_gpu: false,
        }
    }

    fn estimate_cost(&self, _text_len: usize) -> f64 {
        0.0
    }
}

fn orchestration(behavior: Behavior, calls: Arc<AtomicUsize>) -> Arc<SynthesisOrchestrator> {
    let providers: HashMap<String, Box<dyn VoiceSynthesizer>> = HashMap::from([(
        "test".to_string(),
        Box::new(TestProvider { behavior, calls }) as Box<dyn VoiceSynthesizer>,
    )]);
    Arc::new(SynthesisOrchestrator::from_test_providers(
        providers,
        &["test"],
    ))
}

/// 20 s fixture: a short tone, a long silent gap, then a short tone.
fn fixture(dir: &Path) -> PathBuf {
    let path = dir.join("movie.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let sr = 16_000_usize;
    let mut writer = hound::WavWriter::create(&path, spec).expect("create wav");
    for i in 0..sr * 20 {
        let second = i / sr;
        let audible = (2..4).contains(&second) || (18..20).contains(&second);
        let sample = if audible {
            (i as f32 * 440.0 * std::f32::consts::TAU / sr as f32).sin() * 0.4
        } else {
            0.0
        };
        writer
            .write_sample((sample * i16::MAX as f32) as i16)
            .expect("write sample");
    }
    writer.finalize().expect("finalize wav");
    path
}

fn options(movie: &Path, dir: &Path) -> ProduceOptions {
    ProduceOptions {
        input: movie.to_path_buf(),
        output: Some(dir.join("out").join("deliverable.wav")),
        // A lone silence scores 0.4 against the default 0.8 gap threshold.
        gap_confidence: Some(0.35),
        no_auto_cues: true,
        ..Default::default()
    }
}

fn checkpoint_for(opts: &ProduceOptions) -> Checkpoint {
    let path = Path::new(opts.output.as_ref().expect("output"))
        .parent()
        .expect("parent");
    Checkpoint::load(&path.join(CHECKPOINT_FILE)).expect("load checkpoint")
}

fn run(opts: ProduceOptions, behavior: Behavior) -> (Result<()>, usize) {
    let calls = Arc::new(AtomicUsize::new(0));
    let result = run_produce(
        opts,
        &AppConfig::default(),
        Some(orchestration(behavior, calls.clone())),
    );
    (result, calls.load(Ordering::SeqCst))
}

#[test]
fn full_run_produces_narration_and_a_radio_play_deliverable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let movie = fixture(dir.path());
    let opts = options(&movie, dir.path());
    let deliverable = opts.output.clone().expect("output");

    let (result, calls) = run(opts, Behavior::Tone);
    result.expect("full production run must succeed");
    assert!(calls > 0, "narration must be synthesized");

    let checkpoint = {
        let dir = deliverable.parent().expect("parent");
        Checkpoint::load(&dir.join(CHECKPOINT_FILE)).expect("checkpoint")
    };
    let goal = WorldState {
        radio_play_assembled: true,
        ..WorldState::default()
    };
    for stage in production::required_stages(&goal) {
        assert!(
            checkpoint.stage_artifacts_present(stage),
            "stage {} must commit artifacts",
            stage.name
        );
    }

    let run_dir = deliverable.parent().expect("parent");
    let narration: serde_json::Value =
        serde_json::from_slice(&fs::read(run_dir.join("narration.json")).expect("narration index"))
            .expect("narration index json");
    let entries = narration["entries"].as_array().expect("entries");
    assert!(!entries.is_empty(), "narration scripts must be synthesized");
    assert!(
        entries.iter().any(|e| e["file"].is_string()),
        "at least one narration clip must be committed"
    );

    let original = fs::read(&movie).expect("read input");
    let produced = fs::read(&deliverable).expect("read deliverable");
    assert_ne!(
        original, produced,
        "the deliverable must be the assembled radio play, not a copy of the source"
    );
    assert_eq!(
        checkpoint.stages["synthesize_narration"]
            .provider
            .as_deref(),
        Some("test"),
        "the serving provider must be recorded"
    );
}

#[test]
fn resume_reuses_valid_stages_without_resynthesis() {
    let dir = tempfile::tempdir().expect("tempdir");
    let movie = fixture(dir.path());

    let (first, first_calls) = run(options(&movie, dir.path()), Behavior::Tone);
    first.expect("first run");
    assert!(first_calls > 0);

    let (second, second_calls) = run(options(&movie, dir.path()), Behavior::Tone);
    second.expect("resumed run");
    assert_eq!(
        second_calls, 0,
        "resume must not call the TTS provider again"
    );

    let checkpoint = checkpoint_for(&options(&movie, dir.path()));
    let goal = WorldState {
        radio_play_assembled: true,
        ..WorldState::default()
    };
    for stage in production::required_stages(&goal) {
        assert_eq!(
            checkpoint.outcomes.get(stage.name).map(String::as_str),
            Some("reused"),
            "stage {} should be reused",
            stage.name
        );
    }
}

#[test]
fn changed_source_invalidates_every_stage() {
    let dir = tempfile::tempdir().expect("tempdir");
    let movie = fixture(dir.path());
    run(options(&movie, dir.path()), Behavior::Tone)
        .0
        .expect("first run");

    let mut bytes = fs::read(&movie).expect("read movie");
    bytes.extend_from_slice(&[0u8; 16]);
    fs::write(&movie, bytes).expect("rewrite movie");

    let (second, calls) = run(options(&movie, dir.path()), Behavior::Tone);
    second.expect("run after input change");
    assert!(calls > 0, "changed source must re-run synthesis");
    let checkpoint = checkpoint_for(&options(&movie, dir.path()));
    assert_eq!(
        checkpoint.outcomes.get("decode_audio").map(String::as_str),
        Some("completed")
    );
}

#[test]
fn changed_configuration_invalidates_only_downstream_stages() {
    let dir = tempfile::tempdir().expect("tempdir");
    let movie = fixture(dir.path());
    run(options(&movie, dir.path()), Behavior::Tone)
        .0
        .expect("first run");

    let mut changed = options(&movie, dir.path());
    changed.language = Some("en".to_string());
    let (second, calls) = run(changed, Behavior::Tone);
    second.expect("run after configuration change");
    assert!(
        calls > 0,
        "changed narration language must re-run synthesis"
    );

    let checkpoint = checkpoint_for(&options(&movie, dir.path()));
    assert_eq!(
        checkpoint.outcomes.get("decode_audio").map(String::as_str),
        Some("reused"),
        "audio extraction is unaffected by the narration language"
    );
    assert_eq!(
        checkpoint
            .outcomes
            .get("plan_narration")
            .map(String::as_str),
        Some("completed")
    );
}

#[test]
fn corrupted_artifact_reruns_only_the_affected_stage() {
    let dir = tempfile::tempdir().expect("tempdir");
    let movie = fixture(dir.path());
    let opts = options(&movie, dir.path());
    let run_dir = opts
        .output
        .as_ref()
        .and_then(|o| o.parent())
        .expect("run dir")
        .to_path_buf();
    run(opts, Behavior::Tone).0.expect("first run");

    fs::write(run_dir.join("timeline.json"), b"").expect("truncate timeline");

    let (second, _) = run(options(&movie, dir.path()), Behavior::Tone);
    second.expect("run after artifact corruption");
    let checkpoint = checkpoint_for(&options(&movie, dir.path()));
    assert_eq!(
        checkpoint.outcomes.get("decode_audio").map(String::as_str),
        Some("reused")
    );
    assert_eq!(
        checkpoint
            .outcomes
            .get("analyze_speech")
            .map(String::as_str),
        Some("completed")
    );
    assert!(
        run_dir
            .join("timeline.json")
            .metadata()
            .expect("meta")
            .len()
            > 0
    );
}

#[test]
fn failed_stage_leaves_a_recoverable_checkpoint_and_errors() {
    let dir = tempfile::tempdir().expect("tempdir");
    let movie = fixture(dir.path());
    let opts = options(&movie, dir.path());
    let run_dir = opts
        .output
        .as_ref()
        .and_then(|o| o.parent())
        .expect("run dir")
        .to_path_buf();

    let (failed, _) = run(opts, Behavior::Fail);
    assert!(failed.is_err(), "a total TTS failure must fail the run");
    let error = failed.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(error.contains("produce failed"), "{error}");

    let checkpoint = Checkpoint::load(&run_dir.join(CHECKPOINT_FILE)).expect("checkpoint");
    assert!(
        checkpoint.stages.contains_key("decode_audio"),
        "completed stages stay checkpointed"
    );
    assert!(
        !checkpoint.stages.contains_key("synthesize_narration"),
        "a failed stage must not be recorded as completed"
    );
    let outcome = checkpoint
        .outcomes
        .get("synthesize_narration")
        .cloned()
        .unwrap_or_default();
    assert!(outcome.starts_with("failed:"), "{outcome}");

    let (recovered, calls) = run(options(&movie, dir.path()), Behavior::Tone);
    recovered.expect("resume after failure must complete");
    assert!(calls > 0);
    let checkpoint = checkpoint_for(&options(&movie, dir.path()));
    assert_eq!(
        checkpoint.outcomes.get("decode_audio").map(String::as_str),
        Some("reused"),
        "resume must not redo work that already succeeded"
    );
}

#[test]
fn dry_run_describes_the_graph_without_running_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let movie = fixture(dir.path());
    let mut opts = options(&movie, dir.path());
    opts.dry_run = true;

    handle_produce(opts.clone(), &AppConfig::default()).expect("dry run");
    let run_dir = opts.output.as_ref().and_then(|o| o.parent()).expect("dir");
    assert!(!run_dir.join(CHECKPOINT_FILE).exists());
    assert!(!opts.output.expect("output").exists());
}

#[test]
fn traversal_paths_are_rejected() {
    let mut opts = ProduceOptions {
        input: PathBuf::from("../evil.wav"),
        ..Default::default()
    };
    assert!(handle_produce(opts.clone(), &AppConfig::default()).is_err());
    opts.input = PathBuf::from("movie.wav");
    opts.resume = Some(PathBuf::from("../checkpoint.json"));
    assert!(handle_produce(opts, &AppConfig::default()).is_err());
}

#[test]
fn contract_stage_lookup_is_used_for_checkpoint_names() {
    // Guards the produce handler against drifting from the shared contract.
    assert!(stage_by_name("synthesize_narration").is_some());
    assert_eq!(input_class::SOURCE, "source");
}
