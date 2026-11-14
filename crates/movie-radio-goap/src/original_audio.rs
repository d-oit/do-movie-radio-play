//! Decoded source audio kept on disk instead of in memory (#418).

use anyhow::{Context, Result};
use movie_radio_verification::{RawF32Source, SampleSource};
use std::io::Write;
use std::path::Path;

/// Mono `f32` samples spooled to a temporary file and read back by window.
/// The file is removed when this value drops.
pub struct OriginalAudio {
    file: tempfile::NamedTempFile,
    source: RawF32Source,
}

impl OriginalAudio {
    /// Decodes `media` to a spool file chunk by chunk.
    pub fn decode(media: &Path, sample_rate: u32) -> Result<Self> {
        let file = tempfile::NamedTempFile::new().context("create decoded audio file")?;
        movie_radio_pipeline::pipeline::decode::decode_audio_to_raw_f32(
            media,
            sample_rate,
            file.path(),
        )?;
        Self::from_file(file)
    }

    pub fn from_samples(samples: &[f32]) -> Result<Self> {
        let mut file = tempfile::NamedTempFile::new().context("create decoded audio file")?;
        let mut bytes = Vec::with_capacity(samples.len().min(1 << 20) * 4);
        for block in samples.chunks(1 << 20) {
            bytes.clear();
            for sample in block {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
            file.write_all(&bytes)?;
        }
        file.flush()?;
        Self::from_file(file)
    }

    fn from_file(file: tempfile::NamedTempFile) -> Result<Self> {
        let source = RawF32Source::open(file.path())?;
        Ok(Self { file, source })
    }

    pub fn path(&self) -> &Path {
        self.file.path()
    }

    pub fn len(&self) -> usize {
        self.source.len()
    }

    pub fn is_empty(&self) -> bool {
        self.source.is_empty()
    }
}

impl SampleSource for OriginalAudio {
    fn len(&self) -> usize {
        self.source.len()
    }

    fn read_window(&self, start: usize, end: usize, out: &mut Vec<f32>) -> Result<()> {
        self.source.read_window(start, end, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_round_trip_and_clamp() {
        let samples: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();
        let audio = OriginalAudio::from_samples(&samples).expect("spool");
        assert_eq!(audio.len(), 100);
        let mut out = Vec::new();
        audio.read_window(10, 20, &mut out).expect("window");
        assert_eq!(out, samples[10..20]);
        audio.read_window(90, 500, &mut out).expect("clamped");
        assert_eq!(out, samples[90..]);
    }
}
