use anyhow::{Context, Result};
use std::io::Read;
use std::path::PathBuf;
use tracing::info;

/// Handle the `preview` subcommand: read a WAV file and play it via system audio.
pub fn handle_preview(input: PathBuf, _skip: f32, _duration: Option<f32>) -> Result<()> {
    let mut file = std::fs::File::open(&input)
        .with_context(|| format!("failed to open: {}", input.display()))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .context("failed to read file")?;

    if _skip > 0.0 {
        info!("--skip={_skip} is not yet implemented; playing from start");
    }
    if _duration.is_some() {
        info!("--duration is not yet implemented; playing full file");
    }

    #[cfg(feature = "playback")]
    {
        let preview = movie_radio_io::preview::PreviewOutput::new()
            .context("failed to initialize audio output (no audio device?)")?;
        preview.play_wav(&bytes)?;
    }
    #[cfg(not(feature = "playback"))]
    {
        anyhow::bail!("playback feature is disabled; cannot play audio");
    }
    #[cfg(feature = "playback")]
    {
        info!("preview finished");
        Ok(())
    }
}
