use anyhow::{bail, Result};
use std::path::Path;

pub fn read_wav_to_f32(path: &Path) -> Result<(Vec<f32>, u32)> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.bits_per_sample != 16 {
        bail!("only 16-bit PCM wav is supported in direct mode");
    }
    let channels = spec.channels as usize;
    let mut mono = Vec::new();
    let mut accum = 0.0f32;
    let mut idx = 0usize;
    for s in reader.samples::<i16>() {
        let sample = (s? as f32) / (i16::MAX as f32);
        accum += sample;
        idx += 1;
        if idx == channels {
            mono.push(accum / channels as f32);
            idx = 0;
            accum = 0.0;
        }
    }
    Ok((mono, spec.sample_rate))
}
