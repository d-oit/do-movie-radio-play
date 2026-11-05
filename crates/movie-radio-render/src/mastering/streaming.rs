//! Bounded-memory mastering.
//!
//! [`master`](super::master) holds the whole deliverable in memory and returns
//! a second full-length buffer. This module applies the same gain and limiter
//! passes while streaming the output to disk, so peak memory is one source
//! buffer plus a fixed chunk, independent of the film length.

use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use super::measure::{LoudnessMeasurement, LoudnessMeter, MeasurementConfig, SILENCE_LUFS};
use super::report::validate_measurement;
use super::MasteringProfile;

/// Samples processed per streaming step. 1M mono samples is 4 MB.
const CHUNK_SAMPLES: usize = 1 << 20;

#[derive(Debug, Clone)]
pub struct StreamingMasteringOutcome {
    pub report: super::MasteringReport,
    pub applied_gain_db: f32,
    pub limiting_reduction_db: f32,
}

/// Masters `source` into a 16-bit WAV at `output_path` without materializing
/// the mastered track.
///
/// The pass order matches [`master`](super::master): normalize to the profile
/// target, limit to the true-peak ceiling, then top up loudness up to three
/// times. Intermediate passes are raw f32 temp files.
pub fn master_to_wav(
    source: &[f32],
    output_path: &Path,
    sample_rate: u32,
    channels: u16,
    profile: &MasteringProfile,
) -> Result<StreamingMasteringOutcome> {
    let channels = channels.max(1);
    let config = MeasurementConfig {
        silence_threshold_dbfs: profile.silence_threshold_dbfs,
        clipping_threshold: profile.clipping_threshold,
    };

    if source.is_empty() || sample_rate == 0 {
        write_wav_from_f32_file(None, output_path, sample_rate, channels)?;
        let report = validate_measurement(
            LoudnessMeasurement::default(),
            sample_rate,
            channels,
            profile,
        );
        return Ok(StreamingMasteringOutcome {
            report,
            applied_gain_db: 0.0,
            limiting_reduction_db: 0.0,
        });
    }

    let before = super::measure::measure(source, sample_rate, channels, &config);
    let initial_gain_db = if before.integrated_lufs <= SILENCE_LUFS + 1.0 {
        0.0
    } else {
        (profile.target_lufs - before.integrated_lufs).clamp(-24.0, 24.0)
    };
    let mut applied_gain_db = initial_gain_db;
    let mut limiting_reduction_db = 0.0;
    let ceiling = 10f32.powf(profile.max_true_peak_dbtp / 20.0);

    let mut current = tempfile::NamedTempFile::new().context("create mastering temp file")?;
    let mut measurement = process_slice(
        source,
        current.path(),
        sample_rate,
        channels,
        &config,
        10f32.powf(initial_gain_db / 20.0),
        false,
    )?;

    for _ in 0..3 {
        let peak_before = linear_peak(measurement.true_peak_dbtp);
        if peak_before > ceiling {
            limiting_reduction_db += 20.0 * (ceiling / peak_before).log10();
            let (next, next_measurement) =
                process_file(current.path(), sample_rate, channels, &config, 1.0, true)?;
            current = next;
            measurement = next_measurement;
            let peak_after = linear_peak(measurement.true_peak_dbtp);
            if peak_after > ceiling {
                let safety = ceiling / peak_after;
                let (next, next_measurement) = process_file(
                    current.path(),
                    sample_rate,
                    channels,
                    &config,
                    safety,
                    false,
                )?;
                current = next;
                measurement = next_measurement;
                limiting_reduction_db += 20.0 * safety.log10();
            }
        }

        let measured = measurement.integrated_lufs;
        if measured <= SILENCE_LUFS + 1.0 {
            break;
        }
        let delta = profile.target_lufs - measured;
        if delta.abs() <= 0.3 {
            break;
        }
        let extra = delta.clamp(-6.0, 6.0);
        applied_gain_db += extra;
        let (next, next_measurement) = process_file(
            current.path(),
            sample_rate,
            channels,
            &config,
            10f32.powf(extra / 20.0),
            false,
        )?;
        current = next;
        measurement = next_measurement;
    }

    let final_peak = linear_peak(measurement.true_peak_dbtp);
    if final_peak > ceiling {
        limiting_reduction_db += 20.0 * (ceiling / final_peak).log10();
        let (next, next_measurement) =
            process_file(current.path(), sample_rate, channels, &config, 1.0, true)?;
        current = next;
        measurement = next_measurement;
        let after = linear_peak(measurement.true_peak_dbtp);
        if after > ceiling {
            let safety = ceiling / after;
            let (next, next_measurement) = process_file(
                current.path(),
                sample_rate,
                channels,
                &config,
                safety,
                false,
            )?;
            current = next;
            measurement = next_measurement;
            limiting_reduction_db += 20.0 * safety.log10();
        }
    }

    write_wav_from_f32_file(Some(current.path()), output_path, sample_rate, channels)?;

    let mut report = validate_measurement(measurement, sample_rate, channels, profile);
    report.applied_gain_db = applied_gain_db;
    report.limiting_reduction_db = limiting_reduction_db;
    Ok(StreamingMasteringOutcome {
        report,
        applied_gain_db,
        limiting_reduction_db,
    })
}

fn linear_peak(dbtp: f32) -> f32 {
    if dbtp <= SILENCE_LUFS {
        0.0
    } else {
        10f32.powf(dbtp / 20.0)
    }
}

fn process_slice(
    source: &[f32],
    path: &Path,
    sample_rate: u32,
    channels: u16,
    config: &MeasurementConfig,
    gain: f32,
    limit: bool,
) -> Result<LoudnessMeasurement> {
    let mut processor = Processor::new(sample_rate, channels, config, gain, limit);
    let mut sink = BufWriter::new(File::create(path).context("create mastering temp file")?);
    let chunk = CHUNK_SAMPLES * channels.max(1) as usize;
    for samples in source.chunks(chunk) {
        processor.process(samples, &mut sink)?;
    }
    let measurement = processor.finish(&mut sink)?;
    sink.flush()?;
    Ok(measurement)
}

fn process_file(
    input: &Path,
    sample_rate: u32,
    channels: u16,
    config: &MeasurementConfig,
    gain: f32,
    limit: bool,
) -> Result<(tempfile::NamedTempFile, LoudnessMeasurement)> {
    let mut processor = Processor::new(sample_rate, channels, config, gain, limit);
    let output = tempfile::NamedTempFile::new().context("create mastering temp file")?;
    let mut sink = BufWriter::new(File::create(output.path()).context("create temp output")?);
    let mut reader = BufReader::new(File::open(input).context("open mastering temp input")?);
    let mut bytes = vec![0u8; CHUNK_SAMPLES * 4];
    let mut samples = Vec::with_capacity(CHUNK_SAMPLES);
    loop {
        let read = reader.read(&mut bytes)?;
        if read == 0 {
            break;
        }
        samples.clear();
        for chunk in bytes[..read].as_chunks::<4>().0 {
            samples.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        processor.process(&samples, &mut sink)?;
    }
    let measurement = processor.finish(&mut sink)?;
    sink.flush()?;
    Ok((output, measurement))
}

struct Processor {
    gain: f32,
    limiter: Option<StreamingLimiter>,
    meter: LoudnessMeter,
    bytes: Vec<u8>,
}

impl Processor {
    fn new(
        sample_rate: u32,
        channels: u16,
        config: &MeasurementConfig,
        gain: f32,
        limit: bool,
    ) -> Self {
        let channels = channels.max(1) as usize;
        Self {
            gain,
            limiter: limit.then(|| StreamingLimiter::new(sample_rate, channels, 1.0)),
            meter: LoudnessMeter::new(sample_rate, channels as u16, config),
            bytes: Vec::new(),
        }
    }

    fn process(&mut self, input: &[f32], sink: &mut impl Write) -> Result<()> {
        let mut gained = Vec::with_capacity(input.len());
        for &s in input {
            gained.push(s * self.gain);
        }
        let mut output = Vec::with_capacity(input.len());
        if let Some(limiter) = &mut self.limiter {
            limiter.process(&gained, &mut output);
        } else {
            output = gained;
        }
        self.meter.push(&output);
        write_samples(&mut self.bytes, &output, sink)
    }

    fn finish(mut self, sink: &mut impl Write) -> Result<LoudnessMeasurement> {
        if let Some(limiter) = &mut self.limiter {
            let mut output = Vec::new();
            limiter.flush(&mut output);
            self.meter.push(&output);
            write_samples(&mut self.bytes, &output, sink)?;
        }
        Ok(self.meter.finish())
    }
}

fn write_samples(bytes: &mut Vec<u8>, samples: &[f32], sink: &mut impl Write) -> Result<()> {
    bytes.clear();
    bytes.reserve(samples.len() * 4);
    for &s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    sink.write_all(bytes)?;
    Ok(())
}

/// Streaming equivalent of the reference limiter: instant attack, exponential
/// release, then a forward minimum over the look-ahead window.
struct StreamingLimiter {
    channels: usize,
    ceiling: f32,
    lookahead: usize,
    release: usize,
    gain: f32,
    window: VecDeque<f32>,
    pending: VecDeque<Vec<f32>>,
}

impl StreamingLimiter {
    fn new(sample_rate: u32, channels: usize, ceiling: f32) -> Self {
        let lookahead = ((sample_rate as f32 * 0.005).round() as usize).max(1);
        let release = ((sample_rate as f32 * 0.05).round() as usize).max(1);
        Self {
            channels,
            ceiling,
            lookahead,
            release,
            gain: 1.0,
            window: VecDeque::with_capacity(lookahead + 1),
            pending: VecDeque::with_capacity(lookahead + 1),
        }
    }

    fn process(&mut self, input: &[f32], output: &mut Vec<f32>) {
        for frame in input.chunks_exact(self.channels) {
            let peak = frame.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
            let target = if peak > self.ceiling {
                self.ceiling / peak
            } else {
                1.0
            };
            if target < self.gain {
                self.gain = target;
            } else {
                self.gain += (target - self.gain) / self.release as f32;
            }
            self.window.push_back(self.gain);
            self.pending.push_back(frame.to_vec());
            if self.window.len() > self.lookahead + 1 {
                self.window.pop_front();
            }
            if self.pending.len() > self.lookahead {
                let gain = self.window.iter().copied().fold(1.0f32, f32::min);
                let mut frame = self.pending.pop_front().expect("pending frame");
                for s in &mut frame {
                    *s *= gain;
                }
                output.extend_from_slice(&frame);
            }
        }
    }

    fn flush(&mut self, output: &mut Vec<f32>) {
        while let Some(mut frame) = self.pending.pop_front() {
            let gain = self.window.iter().copied().fold(1.0f32, f32::min);
            for s in &mut frame {
                *s *= gain;
            }
            output.extend_from_slice(&frame);
            self.window.pop_front();
        }
    }
}

fn write_wav_from_f32_file(
    input: Option<&Path>,
    output_path: &Path,
    sample_rate: u32,
    channels: u16,
) -> Result<()> {
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(output_path, spec)
        .with_context(|| format!("cannot create {}", output_path.display()))?;
    if let Some(input) = input {
        let mut reader = BufReader::new(File::open(input).context("open mastered temp file")?);
        let mut bytes = vec![0u8; CHUNK_SAMPLES * 4];
        loop {
            let read = reader.read(&mut bytes)?;
            if read == 0 {
                break;
            }
            for chunk in bytes[..read].as_chunks::<4>().0 {
                let sample = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                writer.write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
            }
        }
    }
    writer.finalize()?;
    Ok(())
}
