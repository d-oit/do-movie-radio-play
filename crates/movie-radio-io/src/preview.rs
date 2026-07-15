use anyhow::{Context, Result};
use rodio::source::Source;
#[cfg(feature = "playback")]
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
use std::num::NonZero;
use std::time::Duration;

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
        let source = Decoder::new(cursor)?;
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
}
