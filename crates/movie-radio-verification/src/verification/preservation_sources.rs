//! Random-access sample sources for preservation verification.
//!
//! Verification only inspects bounded windows, so these let it run without
//! holding a whole track in memory (#418).

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Random-access mono sample source for preservation verification.
pub trait SampleSource {
    /// Number of mono frames in the source.
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Copies `[start, end)` clamped to the source length into `out`.
    fn read_window(&self, start: usize, end: usize, out: &mut Vec<f32>) -> Result<()>;
}

impl SampleSource for [f32] {
    fn len(&self) -> usize {
        <[f32]>::len(self)
    }

    fn read_window(&self, start: usize, end: usize, out: &mut Vec<f32>) -> Result<()> {
        out.clear();
        let end = end.min(self.len());
        if start < end {
            out.extend_from_slice(&self[start..end]);
        }
        Ok(())
    }
}

/// A 16-bit PCM WAV read on demand and folded to mono.
pub struct WavSampleSource {
    path: PathBuf,
    frames: usize,
    sample_rate: u32,
}

impl WavSampleSource {
    pub fn open(path: &Path) -> Result<Self> {
        let reader = hound::WavReader::open(path)
            .with_context(|| format!("cannot open {}", path.display()))?;
        let spec = reader.spec();
        if spec.bits_per_sample != 16 {
            anyhow::bail!("only 16-bit PCM wav is supported in direct mode");
        }
        let channels = spec.channels.max(1) as usize;
        Ok(Self {
            path: path.to_path_buf(),
            frames: reader.len() as usize / channels,
            sample_rate: spec.sample_rate,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

impl SampleSource for WavSampleSource {
    fn len(&self) -> usize {
        self.frames
    }

    fn read_window(&self, start: usize, end: usize, out: &mut Vec<f32>) -> Result<()> {
        out.clear();
        let end = end.min(self.frames);
        if start >= end {
            return Ok(());
        }
        let mut reader = hound::WavReader::open(&self.path)
            .with_context(|| format!("cannot open {}", self.path.display()))?;
        let channels = reader.spec().channels.max(1) as usize;
        // `seek` takes a frame position, so the channel fold stays aligned.
        reader
            .seek(start as u32)
            .with_context(|| format!("cannot seek {}", self.path.display()))?;
        out.reserve(end - start);
        let mut accum = 0.0f32;
        let mut idx = 0usize;
        for sample in reader.samples::<i16>().take((end - start) * channels) {
            accum += (sample? as f32) / (i16::MAX as f32);
            idx += 1;
            if idx == channels {
                out.push(accum / channels as f32);
                idx = 0;
                accum = 0.0;
            }
        }
        Ok(())
    }
}

/// A raw little-endian `f32` mono stream read on demand.
///
/// This is the shape assembly streams the pre-mastering mix in, so
/// verification can read its windows without loading the mix back into memory.
pub struct RawF32Source {
    path: PathBuf,
    frames: usize,
}

impl RawF32Source {
    pub fn open(path: &Path) -> Result<Self> {
        let bytes = std::fs::metadata(path)
            .with_context(|| format!("cannot stat {}", path.display()))?
            .len();
        Ok(Self {
            path: path.to_path_buf(),
            frames: (bytes / 4) as usize,
        })
    }
}

impl SampleSource for RawF32Source {
    fn len(&self) -> usize {
        self.frames
    }

    fn read_window(&self, start: usize, end: usize, out: &mut Vec<f32>) -> Result<()> {
        use std::io::{Read, Seek, SeekFrom};

        out.clear();
        let end = end.min(self.frames);
        if start >= end {
            return Ok(());
        }
        let mut file = std::fs::File::open(&self.path)
            .with_context(|| format!("cannot open {}", self.path.display()))?;
        file.seek(SeekFrom::Start((start * 4) as u64))
            .with_context(|| format!("cannot seek {}", self.path.display()))?;
        let mut bytes = vec![0u8; (end - start) * 4];
        file.read_exact(&mut bytes)
            .with_context(|| format!("cannot read {}", self.path.display()))?;
        out.extend(
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|chunk| f32::from_le_bytes(*chunk)),
        );
        Ok(())
    }
}
