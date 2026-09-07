#[cfg(feature = "playback")]
use anyhow::Context;
use anyhow::Result;
use std::path::PathBuf;
#[cfg(feature = "playback")]
use std::time::Duration;
#[cfg(feature = "playback")]
use tracing::info;

/// Handle the `preview` subcommand: stream a WAV file to system audio.
///
/// `--skip` (seconds) and `--duration` (seconds) select a playback window and
/// were previously accepted but ignored; they now slice the decoded stream
/// (memory stays proportional to the window, not the file).
pub fn handle_preview(input: PathBuf, skip: f32, duration: Option<f32>) -> Result<()> {
    if !input.exists() {
        anyhow::bail!("no such file: {}", input.display());
    }
    #[cfg(feature = "playback")]
    {
        let skip_duration = duration_from_seconds(skip, "--skip")?;
        let limit_duration = duration
            .map(|d| duration_from_seconds(d, "--duration"))
            .transpose()?;
        let preview = movie_radio_io::preview::PreviewOutput::new()
            .context("failed to initialize audio output (no audio device?)")?;
        preview.play_wav_file(&input, skip_duration, limit_duration)?;
        info!(
            skip_s = f64::from(skip),
            duration_s = duration,
            "preview finished"
        );
        Ok(())
    }
    #[cfg(not(feature = "playback"))]
    {
        // Validate the flag values even though playback cannot run, so CLI
        // misuse is reported consistently across builds.
        if !skip.is_finite() || skip.is_sign_negative() {
            anyhow::bail!("--skip must be a non-negative number of seconds");
        }
        if let Some(d) = duration {
            if !d.is_finite() || d.is_sign_negative() {
                anyhow::bail!("--duration must be a non-negative number of seconds");
            }
        }
        anyhow::bail!("playback feature is disabled; cannot play audio");
    }
}

/// Parse a seconds value into a `Duration` without panicking: rejects
/// non-finite and negative input, and saturates huge-but-finite values.
#[cfg(feature = "playback")]
fn duration_from_seconds(secs: f32, flag: &str) -> Result<Duration> {
    if !secs.is_finite() || secs.is_sign_negative() {
        anyhow::bail!("{flag} must be a non-negative number of seconds");
    }
    let total = f64::from(secs);
    let whole = total.floor();
    let nanos = ((total - whole) * 1_000_000_000.0).round() as u32;
    Ok(Duration::new(whole as u64, nanos.min(999_999_999)))
}

#[cfg(all(test, feature = "playback"))]
mod tests {
    use super::*;

    #[test]
    fn duration_from_seconds_accepts_finite_values() {
        assert_eq!(
            duration_from_seconds(0.0, "--skip").unwrap(),
            Duration::ZERO
        );
        assert_eq!(
            duration_from_seconds(1.5, "--skip").unwrap(),
            Duration::from_millis(1500)
        );
    }

    #[test]
    fn duration_from_seconds_rejects_invalid_values() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0] {
            assert!(duration_from_seconds(bad, "--skip").is_err(), "{bad}");
        }
    }

    #[test]
    fn duration_from_seconds_saturates_huge_values() {
        // f32::MAX is finite but far beyond Duration's u64 second range;
        // it must not panic and must saturate its whole-second part.
        let d = duration_from_seconds(f32::MAX, "--duration").unwrap();
        assert_eq!(d, Duration::from_secs(u64::MAX));
    }
}
