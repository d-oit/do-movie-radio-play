use crate::agc::apply_agc;
use crate::spatial::StereoPosition;
use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrackInput {
    /// Path to WAV file for this character segment
    pub wav_path: PathBuf,
    /// Character name / ID for tracing
    pub character_id: String,
    /// Stereo position of this character
    pub position: StereoPosition,
    /// Start offset in the final mix (in samples at output_sample_rate)
    pub start_offset_samples: u64,
    /// AGC attack time in seconds
    pub agc_attack: f32,
    /// AGC release time in seconds
    pub agc_release: f32,
    /// Max AGC gain multiplier
    pub agc_max_gain: f32,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RenderConfig {
    pub output_sample_rate: u32,
    pub output_path: PathBuf,
    pub tracks: Vec<TrackInput>,
}

#[derive(Debug)]
pub struct RenderOutput {
    pub output_path: PathBuf,
    pub total_samples: u64,
    pub duration_secs: f64,
}

/// Render all tracks into a stereo WAV output.
/// Steps per track:
///   1. Load WAV via hound
///   2. Apply AGC via rodio
///   3. Apply constant-power stereo pan
///   4. Write into output stereo interleaved buffer at start_offset
pub fn render_mix(config: &RenderConfig) -> Result<RenderOutput> {
    use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
    use tracing::info;

    // Determine total length needed
    let mut total_len: u64 = 0;
    let mut track_data: Vec<(TrackInput, Vec<f32>, u32)> = Vec::new();

    for track in &config.tracks {
        let reader = WavReader::open(&track.wav_path)?;
        let spec = reader.spec();
        let raw: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => reader
                .into_samples::<f32>()
                .collect::<std::result::Result<_, _>>()?,
            hound::SampleFormat::Int => {
                let bits = spec.bits_per_sample;
                let max_val = (1i64 << (bits - 1)) as f32;
                reader
                    .into_samples::<i32>()
                    .map(|s| s.map(|v| v as f32 / max_val))
                    .collect::<std::result::Result<_, _>>()?
            }
        };
        let sample_rate = spec.sample_rate;
        let mono: Vec<f32> = if spec.channels == 2 {
            raw.chunks(2).map(|c| (c[0] + c[1]) * 0.5).collect()
        } else {
            raw
        };
        // AGC
        let normalized = apply_agc(
            mono,
            sample_rate,
            track.agc_attack,
            track.agc_release,
            track.agc_max_gain,
        );
        let end = track.start_offset_samples + normalized.len() as u64;
        if end > total_len {
            total_len = end;
        }
        track_data.push((track.clone(), normalized, sample_rate));
    }

    // Allocate stereo output buffer (interleaved L/R)
    let mut stereo = vec![0.0f32; (total_len * 2) as usize];

    for (track, samples, _sr) in &track_data {
        let (l_gain, r_gain) = track.position.gains();
        for (i, &s) in samples.iter().enumerate() {
            let base = (track.start_offset_samples as usize + i) * 2;
            stereo[base] += s * l_gain;
            stereo[base + 1] += s * r_gain;
        }
    }

    // Normalise to prevent clipping
    let peak = stereo.iter().copied().map(f32::abs).fold(0.0f32, f32::max);
    if peak > 1.0 {
        for s in &mut stereo {
            *s /= peak;
        }
    }

    // Write output WAV
    let spec = WavSpec {
        channels: 2,
        sample_rate: config.output_sample_rate,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer = WavWriter::create(&config.output_path, spec)?;
    for s in &stereo {
        writer.write_sample(*s)?;
    }
    writer.finalize()?;

    let duration_secs = total_len as f64 / config.output_sample_rate as f64;
    info!(
        output = %config.output_path.display(),
        duration_secs,
        tracks = config.tracks.len(),
        "render_mix complete"
    );

    Ok(RenderOutput {
        output_path: config.output_path.clone(),
        total_samples: total_len,
        duration_secs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{SampleFormat, WavSpec, WavWriter};
    use tempfile::tempdir;

    #[test]
    fn test_render_mix_basic() -> Result<()> {
        let dir = tempdir()?;
        let wav1 = dir.path().join("track1.wav");
        let wav2 = dir.path().join("track2.wav");
        let output = dir.path().join("output.wav");

        let spec = WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        };

        // Track 1: 1.0 second of 0.5 amplitude
        let mut writer1 = WavWriter::create(&wav1, spec)?;
        for _ in 0..44100 {
            writer1.write_sample(0.5f32)?;
        }
        writer1.finalize()?;

        // Track 2: 1.0 second of 0.5 amplitude
        let mut writer2 = WavWriter::create(&wav2, spec)?;
        for _ in 0..44100 {
            writer2.write_sample(0.5f32)?;
        }
        writer2.finalize()?;

        let config = RenderConfig {
            output_sample_rate: 44100,
            output_path: output.clone(),
            tracks: vec![
                TrackInput {
                    wav_path: wav1,
                    character_id: "A".to_string(),
                    position: StereoPosition::HARD_LEFT,
                    start_offset_samples: 0,
                    agc_attack: 0.1,
                    agc_release: 0.15,
                    agc_max_gain: 1.0,
                },
                TrackInput {
                    wav_path: wav2,
                    character_id: "B".to_string(),
                    position: StereoPosition::HARD_RIGHT,
                    start_offset_samples: 22050, // 0.5s offset
                    agc_attack: 0.1,
                    agc_release: 0.15,
                    agc_max_gain: 1.0,
                },
            ],
        };

        let result = render_mix(&config)?;
        assert!(result.output_path.exists());
        assert_eq!(result.total_samples, 44100 + 22050);

        // Verify output
        let reader = hound::WavReader::open(output)?;
        let samples: Vec<f32> = reader
            .into_samples::<f32>()
            .collect::<std::result::Result<_, _>>()?;

        // Track 1: L=0.5, R=0.0 (from 0 to 44100)
        // Track 2: L=0.0, R=0.5 (from 22050 to 66150)
        // Overlap (22050 to 44100): L=0.5, R=0.5.

        for i in 0..22050 {
            assert!(samples[i * 2] > 0.0); // L
            assert!(samples[i * 2 + 1].abs() < 1e-6); // R should be 0.0
        }
        for i in 22050..44100 {
            assert!(samples[i * 2] > 0.0); // L
            assert!(samples[i * 2 + 1] > 0.0); // R
        }
        for i in 44100..66150 {
            assert!(samples[i * 2].abs() < 1e-6); // L should be 0.0
            assert!(samples[i * 2 + 1] > 0.0); // R
        }

        Ok(())
    }

    #[test]
    fn test_peak_normalization() -> Result<()> {
        let dir = tempdir()?;
        let wav = dir.path().join("loud.wav");
        let output = dir.path().join("normalized.wav");

        let spec = WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        };

        let mut writer = WavWriter::create(&wav, spec)?;
        writer.write_sample(2.0f32)?;
        writer.finalize()?;

        let config = RenderConfig {
            output_sample_rate: 44100,
            output_path: output.clone(),
            tracks: vec![TrackInput {
                wav_path: wav,
                character_id: "Loud".to_string(),
                position: StereoPosition::CENTRE,
                start_offset_samples: 0,
                agc_attack: 0.1,
                agc_release: 0.15,
                agc_max_gain: 1.0, // Prevent AGC from changing it too much for this test
            }],
        };

        render_mix(&config)?;

        let reader = hound::WavReader::open(output)?;
        let samples: Vec<f32> = reader
            .into_samples::<f32>()
            .collect::<std::result::Result<_, _>>()?;
        for &s in &samples {
            assert!(s.abs() <= 1.0);
        }

        Ok(())
    }
}
