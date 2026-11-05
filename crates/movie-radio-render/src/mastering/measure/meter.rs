use super::{
    channel_imbalance, integrated_loudness, k_weighting, loudness_from_energy, loudness_range,
    Biquad, LoudnessMeasurement, MeasurementConfig, SILENCE_LUFS,
};

/// Streaming loudness meter.
///
/// `push` may be called with any chunking; the result is identical to a single
/// call over the concatenated samples. Only the 400 ms / 3 s block loudness
/// values are retained, so memory stays bounded for long deliverables.
pub struct LoudnessMeter {
    sample_rate: u32,
    channels: usize,
    config: MeasurementConfig,
    frames: usize,
    block: usize,
    hop: usize,
    short_block: usize,
    short_hop: usize,
    shelf_states: Vec<Biquad>,
    hp_states: Vec<Biquad>,
    block_ring: Vec<f64>,
    block_sum: f64,
    block_pos: usize,
    block_filled: usize,
    block_loudness: Vec<f32>,
    momentary_max: f32,
    short_ring: Vec<f64>,
    short_sum: f64,
    short_pos: usize,
    short_filled: usize,
    short_loudness: Vec<f32>,
    short_term_max: f32,
    sample_peak: f32,
    clipped_samples: usize,
    dc_sum: Vec<f64>,
    channel_sum_sq: Vec<f64>,
    max_discontinuity: f32,
    prev: Vec<f32>,
    has_prev: bool,
    silence_linear: f32,
    silence_run: usize,
    longest_silence: usize,
    true_peak: TruePeakMeter,
}

impl LoudnessMeter {
    pub fn new(sample_rate: u32, channels: u16, config: &MeasurementConfig) -> Self {
        let channels = channels.max(1) as usize;
        let sr = sample_rate as f32;
        let block = ((sr * 0.4).round() as usize).max(1);
        let hop = ((sr * 0.1).round() as usize).max(1);
        let short_block = ((sr * 3.0).round() as usize).max(1);
        let short_hop = ((sr * 1.0).round() as usize).max(1);
        let (shelf, hp) = k_weighting(sr);
        Self {
            sample_rate,
            channels,
            config: *config,
            frames: 0,
            block,
            hop,
            short_block,
            short_hop,
            shelf_states: vec![shelf; channels],
            hp_states: vec![hp; channels],
            block_ring: vec![0.0f64; block],
            block_sum: 0.0,
            block_pos: 0,
            block_filled: 0,
            block_loudness: Vec::new(),
            momentary_max: SILENCE_LUFS,
            short_ring: vec![0.0f64; short_block],
            short_sum: 0.0,
            short_pos: 0,
            short_filled: 0,
            short_loudness: Vec::new(),
            short_term_max: SILENCE_LUFS,
            sample_peak: 0.0,
            clipped_samples: 0,
            dc_sum: vec![0.0f64; channels],
            channel_sum_sq: vec![0.0f64; channels],
            max_discontinuity: 0.0,
            prev: vec![0.0f32; channels],
            has_prev: false,
            silence_linear: 10f32.powf(config.silence_threshold_dbfs / 20.0),
            silence_run: 0,
            longest_silence: 0,
            true_peak: TruePeakMeter::new(channels),
        }
    }

    pub fn push(&mut self, samples: &[f32]) {
        if self.sample_rate == 0 {
            return;
        }
        let channels = self.channels;
        for frame in samples.chunks_exact(channels) {
            let frame_index = self.frames;
            let mut frame_energy = 0.0f64;
            let mut frame_peak = 0.0f32;
            for (c, &x) in frame.iter().enumerate() {
                let abs = x.abs();
                self.sample_peak = self.sample_peak.max(abs);
                frame_peak = frame_peak.max(abs);
                if abs >= self.config.clipping_threshold {
                    self.clipped_samples += 1;
                }
                self.dc_sum[c] += f64::from(x);
                self.channel_sum_sq[c] += f64::from(x) * f64::from(x);
                if self.has_prev {
                    self.max_discontinuity = self.max_discontinuity.max((x - self.prev[c]).abs());
                }
                self.prev[c] = x;
                let filtered = self.hp_states[c].process(self.shelf_states[c].process(x));
                frame_energy += f64::from(filtered) * f64::from(filtered);
            }
            self.has_prev = true;
            self.true_peak.push_frame(frame);

            if frame_peak < self.silence_linear {
                self.silence_run += 1;
                self.longest_silence = self.longest_silence.max(self.silence_run);
            } else {
                self.silence_run = 0;
            }

            self.block_sum += frame_energy - self.block_ring[self.block_pos];
            self.block_ring[self.block_pos] = frame_energy;
            self.block_pos = (self.block_pos + 1) % self.block;
            self.block_filled = (self.block_filled + 1).min(self.block);
            if self.block_filled == self.block
                && (frame_index + 1 - self.block).is_multiple_of(self.hop)
            {
                let loudness = loudness_from_energy(self.block_sum / self.block as f64);
                self.momentary_max = self.momentary_max.max(loudness);
                self.block_loudness.push(loudness);
            }

            self.short_sum += frame_energy - self.short_ring[self.short_pos];
            self.short_ring[self.short_pos] = frame_energy;
            self.short_pos = (self.short_pos + 1) % self.short_block;
            self.short_filled = (self.short_filled + 1).min(self.short_block);
            if self.short_filled == self.short_block
                && (frame_index + 1 - self.short_block).is_multiple_of(self.short_hop)
            {
                let loudness = loudness_from_energy(self.short_sum / self.short_block as f64);
                self.short_term_max = self.short_term_max.max(loudness);
                self.short_loudness.push(loudness);
            }

            self.frames += 1;
        }
    }

    pub fn finish(self) -> LoudnessMeasurement {
        if self.frames == 0 || self.sample_rate == 0 {
            return LoudnessMeasurement::default();
        }
        let frames = self.frames;
        let duration_s = frames as f64 / f64::from(self.sample_rate);
        let dc_offset = self
            .dc_sum
            .iter()
            .map(|&sum| (sum / frames as f64).abs() as f32)
            .fold(0.0f32, f32::max);
        let channel_imbalance_db = channel_imbalance(&self.channel_sum_sq, frames);
        let sample_peak_dbfs = if self.sample_peak > 0.0 {
            20.0 * self.sample_peak.log10()
        } else {
            SILENCE_LUFS
        };
        let true_peak = self.true_peak.finish();
        let true_peak_dbtp = if true_peak > 0.0 {
            20.0 * true_peak.log10()
        } else {
            SILENCE_LUFS
        };
        let mut prev_short: Option<f32> = None;
        let mut max_gain_change_lu = 0.0f32;
        for &loudness in &self.short_loudness {
            if loudness <= -70.0 {
                prev_short = None;
                continue;
            }
            if let Some(previous) = prev_short {
                max_gain_change_lu = max_gain_change_lu.max((loudness - previous).abs());
            }
            prev_short = Some(loudness);
        }

        LoudnessMeasurement {
            integrated_lufs: integrated_loudness(&self.block_loudness),
            loudness_range_lu: loudness_range(&self.short_loudness),
            momentary_max_lufs: self.momentary_max,
            short_term_max_lufs: self.short_term_max,
            sample_peak_dbfs,
            true_peak_dbtp,
            dc_offset,
            clipped_samples: self.clipped_samples,
            longest_silence_ms: (self.longest_silence as f64 * 1000.0 / f64::from(self.sample_rate))
                as u64,
            max_discontinuity: self.max_discontinuity,
            max_gain_change_lu,
            channel_imbalance_db,
            duration_s,
        }
    }
}

/// Streaming inter-sample true-peak estimator.
///
/// Keeps a 17-frame window so a local maximum can be interpolated with the same
/// 4x windowed-sinc kernel as [`true_peak`], without retaining the track.
struct TruePeakMeter {
    channels: usize,
    history: std::collections::VecDeque<Vec<f32>>,
    sample_peak: f32,
    peak: f32,
    threshold: f32,
}

impl TruePeakMeter {
    const HALF: i32 = 8;
    const WINDOW: usize = 17;

    fn new(channels: usize) -> Self {
        Self {
            channels,
            history: std::collections::VecDeque::with_capacity(Self::WINDOW),
            sample_peak: 0.0,
            peak: 0.0,
            threshold: 0.01,
        }
    }

    fn push_frame(&mut self, frame: &[f32]) {
        for &s in frame {
            self.sample_peak = self.sample_peak.max(s.abs());
        }
        self.threshold = (self.sample_peak * 0.5).max(0.01);
        self.history.push_back(frame.to_vec());
        if self.history.len() > Self::WINDOW {
            self.history.pop_front();
        }
        if self.history.len() == Self::WINDOW {
            self.finalize_center();
        }
    }

    fn finalize_center(&mut self) {
        let center = Self::HALF as usize;
        let mut peak = 0.0f32;
        for c in 0..self.channels {
            let x = self.history[center][c].abs();
            if x < self.threshold {
                continue;
            }
            let prev = self.history[center - 1][c].abs();
            let next = self.history[center + 1][c].abs();
            if x < prev || x < next {
                continue;
            }
            peak = peak.max(self.interpolate(center, c));
        }
        self.peak = self.peak.max(peak);
    }

    fn interpolate(&self, center: usize, channel: usize) -> f32 {
        let mut peak = 0.0f32;
        for phase in 1..4 {
            let frac = phase as f32 / 4.0;
            let mut acc = 0.0f32;
            for k in -Self::HALF + 1..=Self::HALF {
                let idx = center as i32 + k;
                if idx < 0 || idx >= self.history.len() as i32 {
                    continue;
                }
                let t = frac - k as f32;
                let sinc = if t.abs() < 1e-6 {
                    1.0
                } else {
                    let arg = std::f32::consts::PI * t;
                    arg.sin() / arg
                };
                let window = if t.abs() < Self::HALF as f32 {
                    0.5 * (1.0 + (std::f32::consts::PI * t / Self::HALF as f32).cos())
                } else {
                    0.0
                };
                acc += self.history[idx as usize][channel] * sinc * window;
            }
            peak = peak.max(acc.abs());
        }
        peak
    }

    fn finish(mut self) -> f32 {
        // Finalize the trailing frames whose look-ahead window is truncated.
        let len = self.history.len();
        let start = len.saturating_sub(Self::HALF as usize);
        for center in start..len {
            let mut peak = 0.0f32;
            for c in 0..self.channels {
                let x = self.history[center][c].abs();
                if x < self.threshold {
                    continue;
                }
                let prev = if center > 0 {
                    self.history[center - 1][c].abs()
                } else {
                    0.0
                };
                let next = if center + 1 < len {
                    self.history[center + 1][c].abs()
                } else {
                    0.0
                };
                if x < prev || x < next {
                    continue;
                }
                peak = peak.max(self.interpolate(center, c));
            }
            self.peak = self.peak.max(peak);
        }
        self.peak.max(self.sample_peak)
    }
}
