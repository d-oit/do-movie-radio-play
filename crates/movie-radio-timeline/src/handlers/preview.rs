use anyhow::{Context, Result};
use std::io::Read;
use std::path::PathBuf;
#[cfg(feature = "playback")]
use std::time::Duration;
#[cfg(feature = "playback")]
use tracing::info;

/// Handle the `preview` subcommand: read a WAV file and play it via system audio.
///
/// `--skip` (seconds) and `--duration` (seconds) select a playback window and
/// were previously accepted but ignored; they now slice the decoded file.
pub fn handle_preview(input: PathBuf, skip: f32, duration: Option<f32>) -> Result<()> {
    if !skip.is_finite() || skip.is_sign_negative() {
        anyhow::bail!("--skip must be a non-negative number of seconds");
    }
    if let Some(d) = duration {
        if !d.is_finite() || d.is_sign_negative() {
            anyhow::bail!("--duration must be a non-negative number of seconds");
        }
    }

    let mut file = std::fs::File::open(&input)
        .with_context(|| format!("failed to open: {}", input.display()))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .context("failed to read file")?;

    #[cfg(feature = "playback")]
    {
        let skip_duration = Duration::from_secs_f64(f64::from(skip));
        let limit_duration = duration.map(|d| Duration::from_secs_f64(f64::from(d)));
        let preview = movie_radio_io::preview::PreviewOutput::new()
            .context("failed to initialize audio output (no audio device?)")?;
        preview.play_wav_window(&bytes, skip_duration, limit_duration)?;
    }
    #[cfg(not(feature = "playback"))]
    {
        anyhow::bail!("playback feature is disabled; cannot play audio");
    }
    #[cfg(feature = "playback")]
    {
        info!(
            skip_s = f64::from(skip),
            duration_s = duration,
            "preview finished"
        );
        Ok(())
    }
}
