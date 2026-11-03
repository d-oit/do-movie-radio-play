//! Deterministic synthetic fixtures for mastering validation.

use anyhow::Result;
use std::f32::consts::PI;
use std::path::Path;

const SR: u32 = 48_000;

pub fn generate_fixtures(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    write_wav(&dir.join("mastering_clipping.wav"), &clipping(), 2)?;
    write_wav(&dir.join("mastering_silence.wav"), &silence(), 2)?;
    write_wav(&dir.join("mastering_low_level.wav"), &low_level(), 2)?;
    write_wav(
        &dir.join("mastering_high_dynamic_range.wav"),
        &high_dynamic_range(),
        2,
    )?;
    write_wav(
        &dir.join("mastering_stereo_imbalance.wav"),
        &stereo_imbalance(),
        2,
    )?;
    write_wav(&dir.join("mastering_mono.wav"), &mono(), 1)?;
    write_wav(&dir.join("mastering_stereo.wav"), &stereo(), 2)?;
    Ok(())
}

fn tone(freq: f32, seconds: f32, amplitude: f32) -> Vec<f32> {
    let len = (SR as f32 * seconds) as usize;
    (0..len)
        .map(|i| (2.0 * PI * freq * i as f32 / SR as f32).sin() * amplitude)
        .collect()
}

fn interleave(left: &[f32], right: &[f32]) -> Vec<f32> {
    let len = left.len().min(right.len());
    let mut out = Vec::with_capacity(len * 2);
    for i in 0..len {
        out.push(left[i]);
        out.push(right[i]);
    }
    out
}

fn clipping() -> Vec<f32> {
    let mono = tone(440.0, 1.0, 1.5);
    interleave(&mono, &mono)
}

fn silence() -> Vec<f32> {
    vec![0.0; (SR as f32 * 2.0) as usize * 2]
}

fn low_level() -> Vec<f32> {
    let mono = tone(440.0, 2.0, 0.01);
    interleave(&mono, &mono)
}

fn high_dynamic_range() -> Vec<f32> {
    let mut mono = tone(220.0, 2.0, 0.01);
    mono.extend(tone(220.0, 2.0, 0.9));
    interleave(&mono, &mono)
}

fn stereo_imbalance() -> Vec<f32> {
    let left = tone(440.0, 2.0, 0.5);
    let right = tone(440.0, 2.0, 0.05);
    interleave(&left, &right)
}

fn mono() -> Vec<f32> {
    tone(440.0, 2.0, 0.3)
}

fn stereo() -> Vec<f32> {
    let mono = tone(440.0, 2.0, 0.3);
    interleave(&mono, &mono)
}

fn write_wav(path: &Path, samples: &[f32], channels: u16) -> Result<()> {
    let spec = hound::WavSpec {
        channels,
        sample_rate: SR,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for &s in samples {
        writer.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;
    Ok(())
}
