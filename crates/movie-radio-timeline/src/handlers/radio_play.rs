use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use tracing::info;

use movie_radio_goap::actions::get_all_actions;
use movie_radio_goap::orchestrator::Orchestrator;
use movie_radio_goap::{PipelineContext, WorldState};
use movie_radio_io::json::{read_timeline, write_json_pretty};
use movie_radio_voice::config::VoiceSynthesisConfig;

#[derive(Debug, Default)]
pub struct RadioPlayOptions {
    pub timeline: Option<PathBuf>,
    pub subtitles: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub analyze_only: bool,
    pub verify_quality: bool,
    pub apply_learnings: bool,
    pub learning_state: Option<PathBuf>,
    pub learning_db: Option<PathBuf>,
    pub no_learn: bool,
}

pub fn handle_radio_play(movie: PathBuf, opts: RadioPlayOptions) -> Result<()> {
    let output_path = opts.output.clone().unwrap_or_else(|| {
        let mut out = movie.clone();
        out.set_extension("radio-play.mp3");
        out
    });

    let mut ctx = PipelineContext::new(movie, output_path);
    ctx.subtitles_path = opts.subtitles;
    ctx.voice_config = Some(VoiceSynthesisConfig::from_env());
    ctx.learning_state_path = opts.learning_state;
    ctx.learning_db_path = opts
        .learning_db
        .or_else(|| Some(PathBuf::from("analysis/thresholds/learning.db")));
    ctx.no_learn = opts.no_learn;

    let mut start_state = WorldState::default();
    if let Some(ref p) = opts.timeline {
        info!(timeline = %p.display(), "Using provided timeline");
        ctx.timeline = Some(read_timeline(p)?);
        start_state.audio_timeline_extracted = true;
    }

    if opts.analyze_only {
        if ctx.timeline.is_none() {
            bail!("--timeline is required for --analyze-only in this version");
        }
        info!("Running visual gap analysis via GOAP planner");
        let goal_state = WorldState {
            visual_gaps_identified: true,
            ..WorldState::default()
        };

        let mut orchestrator = Orchestrator::new(start_state, goal_state, get_all_actions());
        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(orchestrator.run(&mut ctx))?;

        let gap_analysis = ctx
            .gap_analysis
            .as_ref()
            .context("Gap analysis not produced by GOAP orchestrator")?;

        if let Some(ref out) = opts.output {
            write_json_pretty(out, gap_analysis)?;
            info!(gaps = gap_analysis.gaps.len(), output = %out.display(), "Gap analysis complete");
        } else {
            println!("{}", serde_json::to_string_pretty(gap_analysis)?);
        }
        return Ok(());
    }

    info!("Running full radio-play pipeline via GOAP orchestrator");
    let goal_state = WorldState {
        radio_play_assembled: true,
        quality_verified: opts.verify_quality,
        learnings_applied: opts.apply_learnings && !opts.no_learn,
        ..WorldState::default()
    };

    let mut orchestrator = Orchestrator::new(start_state, goal_state, get_all_actions());
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(orchestrator.run(&mut ctx))?;

    info!(
        output = %ctx.output_path.display(),
        "Radio play pipeline completed via GOAP orchestrator"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use movie_radio_types::{Segment, SegmentKind, TimelineOutput};
    use tempfile::tempdir;

    #[test]
    fn test_radio_play_options_default() {
        let opts = RadioPlayOptions::default();
        assert!(!opts.analyze_only);
        assert!(!opts.verify_quality);
        assert!(!opts.apply_learnings);
        assert!(opts.timeline.is_none());
        assert!(opts.subtitles.is_none());
        assert!(opts.output.is_none());
    }

    #[test]
    fn test_handle_radio_play_requires_timeline_for_analyze_only() -> Result<()> {
        let dir = tempdir()?;
        let movie = dir.path().join("movie.mp4");
        let opts = RadioPlayOptions {
            analyze_only: true,
            ..Default::default()
        };
        let res = handle_radio_play(movie, opts);
        assert!(res.is_err());
        let err_msg = res.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err_msg.contains("--timeline is required"));
        Ok(())
    }

    #[test]
    fn test_handle_radio_play_analyze_only_with_timeline() -> Result<()> {
        let dir = tempdir()?;
        let movie = dir.path().join("movie.mp4");
        let timeline_file = dir.path().join("timeline.json");
        let output_file = dir.path().join("gaps.json");

        let timeline = TimelineOutput {
            file: "movie.mp4".to_string(),
            analysis_sample_rate: 16_000,
            frame_ms: 20,
            segments: vec![Segment {
                start_ms: 0,
                end_ms: 10_000,
                kind: SegmentKind::NonVoice,
                confidence: 0.9,
                tags: vec![],
                prompt: None,
                sfx_trigger: None,
            }],
        };
        movie_radio_io::json::write_json_pretty(&timeline_file, &timeline)?;

        let opts = RadioPlayOptions {
            timeline: Some(timeline_file),
            output: Some(output_file.clone()),
            analyze_only: true,
            ..Default::default()
        };

        handle_radio_play(movie, opts)?;
        assert!(output_file.exists());
        Ok(())
    }
}
