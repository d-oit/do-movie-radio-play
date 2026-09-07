use anyhow::{Context, Result};
use rodio::source::Source;
#[cfg(feature = "playback")]
use rodio::{buffer::SamplesBuffer, Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
use std::num::NonZero;
use std::time::Duration;

/// Playback window expressed in per-channel sample frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    /// First per-channel frame to play.
    pub start_frame: u64,
    /// Number of per-channel frames to play; `None` plays to the end.
    pub frame_len: Option<u64>,
}

fn frames_for(duration: Duration, sample_rate_hz: u32) -> u64 {
    if sample_rate_hz == 0 {
        return 0;
    }
    (duration.as_secs_f64() * f64::from(sample_rate_hz)).min(u64::MAX as f64) as u64
}

/// Compute a clamped playback window over `total_frames` per-channel frames.
pub fn window_bounds(
    total_frames: u64,
    sample_rate_hz: u32,
    skip: Duration,
    limit: Option<Duration>,
) -> Window {
    let start_frame = frames_for(skip, sample_rate_hz).min(total_frames);
    let remaining = total_frames - start_frame;
    let frame_len = limit.map(|limit| frames_for(limit, sample_rate_hz).min(remaining));
    Window {
        start_frame,
        frame_len,
    }
}

#[cfg(feature = "playback")]
pub struct PreviewOutput {
    sink: std::sync::Arc<MixerDeviceSink>,
}

#[cfg(feature = "playback")]
impl PreviewOutput {
    pub fn new() -> Result<Self> {
        let sink = DeviceSinkBuilder::open_default_sink().context("no audio device available")?;
        Ok(Self {
            sink: std::sync::Arc::new(sink),
        })
    }

    pub fn play_wav(&self, wav_bytes: &[u8]) -> Result<()> {
        self.play_wav_window(wav_bytes, Duration::ZERO, None)
    }

    /// Play a WAV window: skip the first `skip` seconds and play at most
    /// `limit` seconds (the whole remainder when `limit` is `None`).
    pub fn play_wav_window(
        &self,
        wav_bytes: &[u8],
        skip: Duration,
        limit: Option<Duration>,
    ) -> Result<()> {
        let cursor = std::io::Cursor::new(wav_bytes.to_vec());
        let decoder = Decoder::new(cursor)?;
        let channels = decoder.channels();
        let sample_rate = decoder.sample_rate();
        let ch = usize::from(channels.get());
        if ch == 0 {
            anyhow::bail!("WAV reports zero channels");
        }
        // rodio decodes to f32 interleaved samples; trim any partial frame.
        let mut samples: Vec<f32> = decoder.collect();
        samples.truncate((samples.len() / ch) * ch);
        let total_frames = samples.len() / ch;
        let window = window_bounds(total_frames as u64, sample_rate.get(), skip, limit);
        let start = window.start_frame as usize * ch;
        let end = window
            .frame_len
            .map(|len| start + len as usize * ch)
            .unwrap_or(samples.len())
            .min(samples.len());
        let clipped = samples[start..end].to_vec();
        if clipped.is_empty() {
            anyhow::bail!("no audio left in the requested window (skip exceeds file duration?)");
        }
        let source = SamplesBuffer::new(channels, sample_rate, clipped);
        let player = Player::connect_new(self.sink.mixer());
        player.append(source);
        player.sleep_until_end();
        Ok(())
    }

    pub fn play_sequence(&self, buffers: &[&[u8]], gap_ms: u32) -> Result<()> {
        for (i, buf) in buffers.iter().enumerate() {
            if i > 0 && gap_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(gap_ms as u64));
            }
            self.play_wav(buf)?;
        }
        Ok(())
    }
}

pub struct PcmSource {
    data: Vec<f32>,
    pos: usize,
    sample_rate: NonZero<u32>,
    channels: NonZero<u16>,
}

impl PcmSource {
    pub fn new(data: Vec<f32>, sample_rate: u32, channels: u16) -> Result<Self> {
        let sample_rate = NonZero::new(sample_rate).context("sample_rate must be non-zero")?;
        let channels = NonZero::new(channels).context("channels must be non-zero")?;
        Ok(Self {
            data,
            pos: 0,
            sample_rate,
            channels,
        })
    }
}

impl Iterator for PcmSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.pos < self.data.len() {
            let sample = self.data[self.pos];
            self.pos += 1;
            Some(sample)
        } else {
            None
        }
    }
}

impl Source for PcmSource {
    fn current_span_len(&self) -> Option<usize> {
        let remaining = self.data.len().saturating_sub(self.pos);
        if remaining == 0 {
            Some(0)
        } else {
            Some(remaining)
        }
    }

    fn channels(&self) -> NonZero<u16> {
        self.channels
    }

    fn sample_rate(&self) -> NonZero<u32> {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        let total_samples = self.data.len() as u64;
        let rate = self.sample_rate.get() as u64;
        let ch = self.channels.get() as u64;
        if rate == 0 || ch == 0 {
            return None;
        }
        let secs = total_samples / (rate * ch);
        let nanos = if secs > 0 {
            ((total_samples % (rate * ch)) * 1_000_000_000) / (rate * ch)
        } else {
            (total_samples * 1_000_000_000) / (rate * ch)
        };
        Some(Duration::new(secs, nanos as u32))
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "playback")]
    use super::*;

    #[cfg(feature = "playback")]
    const CI_ENV: &str = "CI";

    #[test]
    #[cfg(feature = "playback")]
    #[allow(unused_imports)]
    fn test_preview_output_init() {
        if std::env::var(CI_ENV).is_ok() {
            eprintln!("skipping audio test in CI");
            return;
        }
        // May fail if no audio device is available (e.g., headless server).
        let result = PreviewOutput::new();
        if result.is_err() {
            eprintln!("no audio device available, skipping: {:?}", result.err());
        }
    }

    #[test]
    fn window_bounds_skip_converts_to_frames() {
        let w = super::window_bounds(32_000, 16_000, std::time::Duration::from_secs(1), None);
        assert_eq!(w.start_frame, 16_000);
        assert_eq!(w.frame_len, None);
    }

    #[test]
    fn window_bounds_limit_clamped_to_remaining() {
        // Total 32k frames; skip 1 s (16k); limit 10 s clamps to remaining 16k.
        let w = super::window_bounds(
            32_000,
            16_000,
            std::time::Duration::from_secs(1),
            Some(std::time::Duration::from_secs(10)),
        );
        assert_eq!(w.start_frame, 16_000);
        assert_eq!(w.frame_len, Some(16_000));
    }

    #[test]
    fn window_bounds_skip_beyond_end_yields_empty() {
        // Skipping past the end clamps start to total; the window is empty.
        let w = super::window_bounds(
            8_000,
            16_000,
            std::time::Duration::from_secs(60),
            Some(std::time::Duration::from_secs(1)),
        );
        assert_eq!(w.start_frame, 8_000);
        assert_eq!(w.frame_len, Some(0));
    }

    #[test]
    fn window_bounds_zero_skip_zero_limit() {
        let w = super::window_bounds(
            0,
            16_000,
            std::time::Duration::ZERO,
            Some(std::time::Duration::ZERO),
        );
        assert_eq!(w.start_frame, 0);
        assert_eq!(w.frame_len, Some(0));
    }
}
