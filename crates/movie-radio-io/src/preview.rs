use anyhow::{Context, Result};
use rodio::source::Source;
#[cfg(feature = "playback")]
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
#[cfg(feature = "playback")]
use std::io::{Read, Seek};
use std::num::NonZero;
#[cfg(feature = "playback")]
use std::path::Path;
use std::time::Duration;

#[cfg(any(feature = "playback", test))]
fn frames_for(duration: Duration, sample_rate_hz: u32) -> u64 {
    if sample_rate_hz == 0 {
        return 0;
    }
    (duration.as_secs_f64() * f64::from(sample_rate_hz)).min(u64::MAX as f64) as u64
}

/// Streams a `Decoder` but yields only the `[skip, skip+limit)` playback
/// window, so memory stays proportional to the window, not the file.
#[cfg(feature = "playback")]
struct WindowedDecoder<R: Read + Seek> {
    inner: Decoder<R>,
    skip_samples: usize,
    remaining: Option<usize>,
    channels: NonZero<u16>,
    sample_rate: NonZero<u32>,
}

#[cfg(feature = "playback")]
impl<R: Read + Seek> WindowedDecoder<R> {
    fn new(inner: Decoder<R>, skip_frames: u64, limit_frames: Option<u64>) -> Self {
        let channels = inner.channels();
        let sample_rate = inner.sample_rate();
        let ch = u64::from(channels.get());
        let skip_samples = skip_frames.saturating_mul(ch).min(usize::MAX as u64) as usize;
        let remaining = limit_frames.map(|f| f.saturating_mul(ch).min(usize::MAX as u64) as usize);
        Self {
            inner,
            skip_samples,
            remaining,
            channels,
            sample_rate,
        }
    }
}

#[cfg(feature = "playback")]
impl<R: Read + Seek> Iterator for WindowedDecoder<R> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        loop {
            let sample = self.inner.next()?;
            if self.skip_samples > 0 {
                self.skip_samples -= 1;
                continue;
            }
            match self.remaining.as_mut() {
                Some(0) => return None,
                Some(left) => {
                    *left -= 1;
                    return Some(sample);
                }
                None => return Some(sample),
            }
        }
    }
}

#[cfg(feature = "playback")]
impl<R: Read + Seek> Source for WindowedDecoder<R> {
    fn current_span_len(&self) -> Option<usize> {
        // Only knowable once the finite window is fully consumed; the window
        // ends when the iterator is exhausted.
        if self.skip_samples == 0 && self.remaining == Some(0) {
            Some(0)
        } else {
            None
        }
    }

    fn channels(&self) -> NonZero<u16> {
        self.channels
    }

    fn sample_rate(&self) -> NonZero<u32> {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
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
        let cursor = std::io::Cursor::new(wav_bytes.to_vec());
        let decoder = Decoder::new(cursor)?;
        self.play_decoder_window(decoder, Duration::ZERO, None)
    }

    /// Play a WAV window from `path`: skip the first `skip` seconds and play
    /// at most `limit` seconds (the remainder when `limit` is `None`).
    pub fn play_wav_file(
        &self,
        path: &Path,
        skip: Duration,
        limit: Option<Duration>,
    ) -> Result<()> {
        let file = std::fs::File::open(path)
            .with_context(|| format!("failed to open: {}", path.display()))?;
        let decoder = Decoder::new(file)?;
        self.play_decoder_window(decoder, skip, limit)
    }

    fn play_decoder_window<R: Read + Seek>(
        &self,
        decoder: Decoder<R>,
        skip: Duration,
        limit: Option<Duration>,
    ) -> Result<()> {
        let sample_rate = decoder.sample_rate().get();
        let skip_frames = frames_for(skip, sample_rate);
        let limit_frames = limit.map(|d| frames_for(d, sample_rate));
        let source = WindowedDecoder::new(decoder, skip_frames, limit_frames);
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
    use super::*;

    #[cfg(feature = "playback")]
    const CI_ENV: &str = "CI";

    #[test]
    #[cfg(feature = "playback")]
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
    fn frames_for_skip_converts_to_frames() {
        assert_eq!(frames_for(Duration::from_secs(1), 16_000), 16_000);
        assert_eq!(frames_for(Duration::from_millis(500), 48_000), 24_000);
    }

    #[test]
    fn frames_for_clamps_to_u64() {
        // Huge durations must not overflow; the result saturates.
        assert_eq!(frames_for(Duration::MAX, 48_000), u64::MAX);
    }

    #[test]
    fn frames_for_zero_rate_yields_zero() {
        assert_eq!(frames_for(Duration::from_secs(5), 0), 0);
    }
}
