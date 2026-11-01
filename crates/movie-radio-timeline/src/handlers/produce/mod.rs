//! `produce`: the resumable entry point for the complete radio-play workflow.
//!
//! Every stage of the shared production contract
//! (`movie_radio_goap::production::STAGES`) commits its artifacts to the run
//! directory and is recorded in `checkpoint.json` only after those artifacts
//! exist on disk. `--resume` therefore re-runs exactly the stages whose input
//! classes, stage version or artifacts changed.

use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

use movie_radio_goap::production::{self, artifacts, identify_input, Checkpoint, InputIdentity};
use movie_radio_goap::{PipelineContext, WorldState};
use movie_radio_types::AppConfig;
use movie_radio_voice::config::VoiceSynthesisConfig;

mod graph;
mod observer;

#[cfg(test)]
mod tests;

use graph::{print_graph, run_fingerprints};
use observer::{ensure_complete, print_summary, CheckpointObserver};

const CHECKPOINT_FILE: &str = "checkpoint.json";
const LEARNING_STATE_FILE: &str = "learning_state.json";

/// Options for a `produce` run.
#[derive(Debug, Default, Clone)]
pub struct ProduceOptions {
    pub input: PathBuf,
    /// Deliverable path; defaults to `<run dir>/<stem>.radio-play.wav`.
    pub output: Option<PathBuf>,
    /// Existing checkpoint to continue from.
    pub resume: Option<PathBuf>,
    pub dry_run: bool,
    /// Adds the `verify_quality` (and `apply_learnings`) stages to the goal.
    pub verify_quality: bool,
    /// Reuse an existing timeline instead of extracting one.
    pub timeline: Option<PathBuf>,
    /// Dialogue cues for gap clipping.
    pub subtitles: Option<PathBuf>,
    /// Narration language (text and TTS).
    pub language: Option<String>,
    /// Gap confidence threshold (lower narrates more gaps).
    pub gap_confidence: Option<f32>,
    /// Do not derive dialogue cues when no subtitles are given.
    pub no_auto_cues: bool,
    /// Freeze learning adaptations.
    pub no_learn: bool,
}

pub fn handle_produce(opts: ProduceOptions, cfg: &AppConfig) -> Result<()> {
    run_produce(opts, cfg, None)
}

/// Shared body. `voice_orchestrator` replaces the chain built from the
/// environment, which lets tests and offline runs use deterministic providers.
pub(crate) fn run_produce(
    opts: ProduceOptions,
    cfg: &AppConfig,
    voice_orchestrator: Option<std::sync::Arc<movie_radio_voice::voice::SynthesisOrchestrator>>,
) -> Result<()> {
    validate_paths(&opts)?;
    let run_dir = run_dir(&opts);
    let checkpoint_path = opts
        .resume
        .clone()
        .unwrap_or_else(|| run_dir.join(CHECKPOINT_FILE));
    let deliverable = deliverable_path(&opts, &run_dir);
    let input = identify_input(&opts.input)?;
    let voice = build_voice_config(&opts);
    let goal = goal_state(&opts);

    if opts.dry_run {
        let fingerprints = run_fingerprints(
            &opts,
            &input,
            &voice,
            &deliverable,
            None,
            cfg,
            &movie_radio_types::AnalysisConfig::default(),
        )?;
        print_graph(
            &opts,
            &run_dir,
            &checkpoint_path,
            &deliverable,
            &fingerprints,
            &voice,
        );
        return Ok(());
    }

    fs::create_dir_all(&run_dir)
        .with_context(|| format!("cannot create run directory {}", run_dir.display()))?;

    let mut ctx = build_context(
        &opts,
        &run_dir,
        deliverable.clone(),
        voice.clone(),
        cfg,
        voice_orchestrator,
    )?;
    let fingerprints = run_fingerprints(
        &opts,
        &input,
        &voice,
        &deliverable,
        ctx.subtitles_path.as_deref(),
        cfg,
        &ctx.config,
    )?;
    let mut checkpoint = open_checkpoint(&checkpoint_path, input, fingerprints.clone())?;
    let start = restore_reusable(&mut ctx, &mut checkpoint, &fingerprints, &goal);
    checkpoint.save(&checkpoint_path)?;

    info!("produce: executing the radio-play production contract");
    let observer = Box::new(CheckpointObserver::new(
        checkpoint,
        checkpoint_path.clone(),
        run_dir.clone(),
    ));
    let runtime = tokio::runtime::Runtime::new().context("cannot start async runtime")?;
    let outcome = runtime.block_on(production::run_contract(
        &mut ctx,
        start,
        goal,
        Some(observer),
    ));

    let saved = Checkpoint::load(&checkpoint_path).ok();
    if let Some(ref saved) = saved {
        print_summary(saved, &goal, &deliverable);
    }
    if let Some(err) = outcome.err() {
        bail!(
            "produce failed: {err:#}\n  checkpoint kept at {}; continue with \
             `timeline produce --input {} --resume {}`",
            checkpoint_path.display(),
            opts.input.display(),
            checkpoint_path.display()
        );
    }
    let saved = saved.context("checkpoint disappeared during a successful run")?;
    ensure_complete(&saved, &goal)?;
    report_output(&ctx)?;
    println!("produce complete: {}", deliverable.display());
    Ok(())
}

/// Reports what the deliverable actually contains, so a run that found nothing
/// to narrate cannot be mistaken for a fully narrated radio play.
fn report_output(ctx: &PipelineContext) -> Result<()> {
    let gaps = ctx.gap_analysis.as_ref().map_or(0, |a| a.gaps.len());
    let scripts = ctx.scripts.as_ref().map_or(0, Vec::len);
    let clips = ctx.narration_audio.iter().filter(|a| a.is_some()).count();
    println!("produce output: gaps={gaps} narration_scripts={scripts} narration_clips={clips}");
    if scripts > 0 && clips == 0 {
        bail!("produce produced {scripts} narration scripts but no synthesized audio");
    }
    if scripts == 0 {
        println!(
            "  note: no gap met the confidence threshold; the deliverable keeps the source \
             audio, lower --gap-confidence to narrate more gaps"
        );
    }
    Ok(())
}

fn goal_state(opts: &ProduceOptions) -> WorldState {
    WorldState {
        radio_play_assembled: true,
        quality_verified: opts.verify_quality,
        learnings_applied: opts.verify_quality && !opts.no_learn,
        ..WorldState::default()
    }
}

fn validate_paths(opts: &ProduceOptions) -> Result<()> {
    let candidates = [
        ("input", Some(&opts.input)),
        ("output", opts.output.as_ref()),
        ("resume", opts.resume.as_ref()),
        ("timeline", opts.timeline.as_ref()),
        ("subtitles", opts.subtitles.as_ref()),
    ];
    for (field, path) in candidates {
        if let Some(path) = path {
            if path.to_string_lossy().contains("..") {
                bail!("{field} path must not contain ..");
            }
        }
    }
    Ok(())
}

fn run_dir(opts: &ProduceOptions) -> PathBuf {
    if let Some(resume) = &opts.resume {
        return resume
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    }
    if let Some(parent) = opts
        .output
        .as_ref()
        .and_then(|o| o.parent())
        .filter(|p| !p.as_os_str().is_empty())
    {
        return parent.to_path_buf();
    }
    let stem = opts
        .input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("out");
    opts.input
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
        .join(format!("produce_{stem}"))
}

fn deliverable_path(opts: &ProduceOptions, run_dir: &Path) -> PathBuf {
    opts.output.clone().unwrap_or_else(|| {
        let stem = opts
            .input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("movie");
        run_dir.join(format!("{stem}.radio-play.wav"))
    })
}

fn build_voice_config(opts: &ProduceOptions) -> VoiceSynthesisConfig {
    let mut voice = VoiceSynthesisConfig::from_env();
    if let Some(language) = opts.language.as_ref().filter(|l| !l.trim().is_empty()) {
        voice.language = language.clone();
    }
    voice
}

fn build_context(
    opts: &ProduceOptions,
    run_dir: &Path,
    deliverable: PathBuf,
    voice: VoiceSynthesisConfig,
    cfg: &AppConfig,
    voice_orchestrator: Option<std::sync::Arc<movie_radio_voice::voice::SynthesisOrchestrator>>,
) -> Result<PipelineContext> {
    let mut ctx = PipelineContext::new(opts.input.clone(), deliverable);
    ctx.run_id = Some(movie_radio_goap::fallback_run_id(&ctx.movie_path));
    ctx.voice_config = Some(voice);
    ctx.voice_orchestrator = voice_orchestrator;
    ctx.gap_confidence = opts.gap_confidence;
    ctx.no_learn = opts.no_learn;
    ctx.learning_state_path = Some(run_dir.join(LEARNING_STATE_FILE));
    ctx.config.sound_effects = Some(cfg.sound_effects.clone());
    if let Some(timeline) = &opts.timeline {
        info!(timeline = %timeline.display(), "using provided timeline");
        ctx.timeline = Some(movie_radio_io::json::read_timeline(timeline)?);
    }
    let cache = run_dir.join("cues.srt");
    let cues = super::cues::resolve_cues(
        &ctx.movie_path,
        opts.subtitles.clone(),
        opts.language.as_deref(),
        &cache,
        !opts.no_auto_cues,
    );
    ctx.subtitles_derived = cues.as_ref().is_some_and(|c| c.derived);
    ctx.subtitles_path = cues.map(|c| c.path);
    Ok(ctx)
}

fn open_checkpoint(
    path: &Path,
    input: InputIdentity,
    fingerprints: BTreeMap<String, String>,
) -> Result<Checkpoint> {
    if !path.exists() {
        return Ok(Checkpoint::fresh(input, fingerprints));
    }
    let checkpoint = match Checkpoint::load(path) {
        Ok(checkpoint) => checkpoint,
        Err(err) => {
            warn!(error = %err, "unreadable checkpoint; starting a fresh run");
            return Ok(Checkpoint::fresh(input, fingerprints));
        }
    };
    if checkpoint.input != input {
        warn!("source media changed since the checkpoint; re-running every stage");
        return Ok(Checkpoint::fresh(input, fingerprints));
    }
    Ok(checkpoint)
}

/// Restores every checkpointed stage that is still valid and returns the world
/// state the run continues from.
fn restore_reusable(
    ctx: &mut PipelineContext,
    checkpoint: &mut Checkpoint,
    fingerprints: &BTreeMap<String, String>,
    goal: &WorldState,
) -> WorldState {
    let required: Vec<&'static str> = production::required_stages(goal)
        .iter()
        .map(|stage| stage.name)
        .collect();
    let mut start = WorldState::default();
    for stage in production::STAGES {
        if !required.contains(&stage.name) {
            checkpoint.record_outcome(stage.name, "skipped: not required by this run");
            continue;
        }
        if checkpoint.is_stage_valid(stage, fingerprints) {
            let usable = checkpoint
                .stages
                .get(stage.name)
                .map(|record| artifacts::restore_stage_artifacts(ctx, stage, &record.artifacts));
            match usable {
                Some(Ok(())) => {
                    checkpoint.record_outcome(stage.name, "reused");
                    stage.flag.set_in(&mut start);
                    continue;
                }
                Some(Err(err)) => {
                    warn!(stage = stage.name, error = %err, "checkpointed artifact unusable; re-running stage");
                }
                None => {}
            }
        }
        checkpoint.invalidate_from(stage.name);
        break;
    }
    start
}
