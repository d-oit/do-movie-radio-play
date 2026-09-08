use anyhow::{bail, Result};
use std::path::PathBuf;
use tracing::info;

use movie_radio_goap::actions::get_all_actions;
use movie_radio_goap::gaps::GapIdentifier;
use movie_radio_goap::orchestrator::Orchestrator;
use movie_radio_goap::{PipelineContext, WorldState};
use movie_radio_io::json::{read_timeline, write_json_pretty};

pub fn handle_radio_play(
    movie: PathBuf,
    timeline_path: Option<PathBuf>,
    subtitles_path: Option<PathBuf>,
    output_path: Option<PathBuf>,
    analyze_only: bool,
) -> Result<()> {
    if analyze_only {
        info!(movie = %movie.display(), "Running visual gap analysis");

        let timeline = if let Some(p) = timeline_path {
            read_timeline(&p)?
        } else {
            bail!("--timeline is required for --analyze-only in this version");
        };

        let srt_content = if let Some(p) = subtitles_path {
            Some(std::fs::read_to_string(p)?)
        } else {
            None
        };

        let identifier = GapIdentifier::default();
        let srt_ref: Option<&str> = srt_content.as_deref();
        // skipcq: RS-E1015 — DeepSource false positive: srt_ref is Option<&str>, not unit-type
        let gap_analysis = identifier.identify_gaps(&timeline, srt_ref)?; // skipcq: RS-E1015

        if let Some(out) = output_path {
            write_json_pretty(&out, &gap_analysis)?;
            info!(gaps = gap_analysis.gaps.len(), output = %out.display(), "Gap analysis complete");
        } else {
            println!("{}", serde_json::to_string_pretty(&gap_analysis)?);
        }
    } else {
        info!(movie = %movie.display(), "Running GOAP radio-play pipeline");
        run_full_pipeline(movie, timeline_path, subtitles_path, output_path)?;
    }
    Ok(())
}

fn run_full_pipeline(
    movie: PathBuf,
    timeline_path: Option<PathBuf>,
    subtitles_path: Option<PathBuf>,
    output_path: Option<PathBuf>,
) -> Result<()> {
    let final_output_path = output_path.unwrap_or_else(|| {
        let mut out = movie.clone();
        out.set_extension("radio-play.mp3");
        out
    });

    let is_mp3_output = final_output_path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mp3"));

    let assemble_wav_path = if is_mp3_output {
        final_output_path.with_extension("tmp.wav")
    } else {
        final_output_path.clone()
    };

    let mut ctx = PipelineContext::new(movie, assemble_wav_path.clone());
    ctx.subtitles_path = subtitles_path;

    if let Some(p) = timeline_path {
        info!(timeline = %p.display(), "Using provided timeline");
        ctx.timeline = Some(read_timeline(&p)?);
    }

    let start_state = WorldState {
        audio_timeline_extracted: ctx.timeline.is_some(),
        ..WorldState::default()
    };

    let goal_state = WorldState {
        radio_play_assembled: true,
        quality_verified: true,
        learnings_applied: true,
        ..WorldState::default()
    };

    let actions = get_all_actions();
    let mut orchestrator = Orchestrator::new(start_state, goal_state, actions);

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(orchestrator.run(&mut ctx))?;

    if is_mp3_output {
        if assemble_wav_path.exists() {
            encode_to_mp3(&assemble_wav_path, &final_output_path)?;
            let _ = std::fs::remove_file(&assemble_wav_path);
        } else {
            bail!(
                "WAV assembly output not found at {}",
                assemble_wav_path.display()
            );
        }
    }

    info!(
        output = %final_output_path.display(),
        quality_score = ctx.quality_score,
        "GOAP radio-play orchestration completed successfully"
    );

    Ok(())
}

fn encode_to_mp3(wav_path: &std::path::Path, mp3_path: &std::path::Path) -> Result<()> {
    use std::process::Command;

    let status = Command::new("ffmpeg")
        .arg("-nostdin")
        .arg("-protocol_whitelist")
        .arg("file,pipe,fd")
        .args(["-hide_banner", "-loglevel", "error"])
        .arg("-i")
        .arg(wav_path)
        .args(["-codec:a", "libmp3lame", "-b:a", "192k", "-q:a", "2", "-y"])
        .arg(mp3_path)
        .status()?;

    if !status.success() {
        bail!("ffmpeg MP3 encoding failed");
    }
    Ok(())
}
