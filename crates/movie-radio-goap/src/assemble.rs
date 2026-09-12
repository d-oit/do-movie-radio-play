use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::narrate::NarrationScript;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrationSegment {
    pub start_sample: usize,
    pub end_sample: usize,
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SfxSegment {
    pub start_sample: usize,
    pub samples: Vec<f32>,
}

pub struct RadioPlayAssembler {
    pub crossfade_samples: usize,
    pub duck_level: f32,
    pub sample_rate: u32,
    pub allow_time_stretch: bool,
    pub max_expansion_ms: u64,
}

impl RadioPlayAssembler {
    pub fn new(sample_rate: u32, crossfade_ms: u64, duck_level: f32) -> Self {
        let crossfade_samples = (sample_rate as f64 * crossfade_ms as f64 / 1000.0) as usize;
        Self {
            crossfade_samples,
            duck_level: duck_level.clamp(0.0, 1.0),
            sample_rate,
            allow_time_stretch: true,
            max_expansion_ms: 500,
        }
    }

    pub fn with_time_stretch(mut self, allow: bool, max_expansion_ms: u64) -> Self {
        self.allow_time_stretch = allow;
        self.max_expansion_ms = max_expansion_ms;
        self
    }

    pub fn assemble(&self, original: &[f32], narrations: &[NarrationSegment]) -> Result<Vec<f32>> {
        if narrations.is_empty() {
            return Ok(original.to_vec());
        }

        self.validate_no_overlaps(narrations)?;
        let mut output = original.to_vec();
        let total_len = output.len();
        for narration in narrations {
            let start = narration.start_sample.min(total_len);
            let end = narration.end_sample.min(total_len);
            let narr_len = end.saturating_sub(start);
            if narr_len == 0 {
                continue;
            }
            let narr_samples = &narration.samples[..narr_len.min(narration.samples.len())];
            self.apply_crossfade_duck(&mut output, start, end, narr_samples);
        }
        Ok(output)
    }

    pub fn assemble_with_sfx(
        &self,
        original: &[f32],
        narrations: &[NarrationSegment],
        sfx_segments: &[SfxSegment],
    ) -> Result<Vec<f32>> {
        let narr_ducked = self.assemble(original, narrations)?;
        if sfx_segments.is_empty() {
            return Ok(narr_ducked);
        }
        self.mix_sfx_track(narr_ducked, sfx_segments)
    }

    fn mix_sfx_track(
        &self,
        narr_ducked: Vec<f32>,
        sfx_segments: &[SfxSegment],
    ) -> Result<Vec<f32>> {
        use movie_radio_render::mixer::{Mixer, TrackInput};
        use movie_radio_render::spatial::StereoPosition;

        let total_len = narr_ducked.len();
        let mut sfx_track = vec![0.0f32; total_len];
        for seg in sfx_segments {
            add_sfx_segment(&mut sfx_track, seg);
        }

        let track_main = TrackInput {
            samples: narr_ducked,
            sample_rate: self.sample_rate,
            position: StereoPosition::CENTRE,
            reverb: None,
            agc_attack: 0.01,
            agc_release: 0.1,
            agc_max_gain: 1.0,
        };

        let track_sfx = TrackInput {
            samples: sfx_track,
            sample_rate: self.sample_rate,
            position: StereoPosition::CENTRE,
            reverb: None,
            agc_attack: 0.01,
            agc_release: 0.1,
            agc_max_gain: 20.0,
        };

        let mut mixer = Mixer::new();
        let stereo = mixer.render_mix(vec![track_main, track_sfx])?;

        let mono: Vec<f32> = stereo
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[l, r]| (l + r) * 0.5 * std::f32::consts::SQRT_2)
            .collect();

        Ok(mono)
    }
}

fn add_sfx_segment(track: &mut [f32], seg: &SfxSegment) {
    let start = seg.start_sample;
    if start >= track.len() {
        return;
    }
    let copy_len = seg.samples.len().min(track.len() - start);
    for (t, s) in track[start..start + copy_len].iter_mut().zip(&seg.samples) {
        *t += *s;
    }
}

impl RadioPlayAssembler {
    fn validate_no_overlaps(&self, narrations: &[NarrationSegment]) -> Result<()> {
        for i in 0..narrations.len() {
            for j in (i + 1)..narrations.len() {
                let a = &narrations[i];
                let b = &narrations[j];
                if a.start_sample < b.end_sample && b.start_sample < a.end_sample {
                    bail!(
                        "Narration overlap: segment {} ({}-{}) overlaps with segment {} ({}-{})",
                        i,
                        a.start_sample,
                        a.end_sample,
                        j,
                        b.start_sample,
                        b.end_sample,
                    );
                }
            }
        }
        Ok(())
    }

    fn apply_crossfade_duck(
        &self,
        output: &mut [f32],
        start: usize,
        _end: usize,
        narration: &[f32],
    ) {
        let cf = self.crossfade_samples;
        let total_len = output.len();

        for i in 0..narration.len() {
            let pos = start + i;
            if pos >= total_len {
                break;
            }

            let sample = narration[i];

            let fade_in = if i < cf { i as f32 / cf as f32 } else { 1.0 };

            let remaining = narration.len() - i;
            let fade_out = if remaining <= cf {
                remaining as f32 / cf as f32
            } else {
                1.0
            };

            let envelope = fade_in.min(fade_out);

            let duck_start = if i >= cf { i - cf } else { 0 };
            let duck_end = (i + cf).min(narration.len());
            let is_ducking = i >= duck_start && i < duck_end;

            if is_ducking {
                let duck_ramp = if i < cf {
                    self.duck_level + (1.0 - self.duck_level) * (i as f32 / cf as f32)
                } else if remaining <= cf {
                    self.duck_level + (1.0 - self.duck_level) * (remaining as f32 / cf as f32)
                } else {
                    self.duck_level
                };

                output[pos] = output[pos] * duck_ramp + sample * envelope;
            } else {
                output[pos] = output[pos] * self.duck_level + sample * envelope;
            }
        }
    }

    pub fn narration_to_segment(
        &self,
        script: &NarrationScript,
        audio_samples: &[f32],
    ) -> NarrationSegment {
        self.narration_to_segment_ext(script, audio_samples, None)
    }

    pub fn narration_to_segment_ext(
        &self,
        script: &NarrationScript,
        audio_samples: &[f32],
        next_gap_start_ms: Option<u64>,
    ) -> NarrationSegment {
        let start_sample = (script.gap_start_ms as f64 * self.sample_rate as f64 / 1000.0) as usize;
        let gap_end_sample = (script.gap_end_ms as f64 * self.sample_rate as f64 / 1000.0) as usize;
        let gap_samples = gap_end_sample.saturating_sub(start_sample);

        let mut samples = audio_samples.to_vec();

        if self.allow_time_stretch && samples.len() > gap_samples && gap_samples > 0 {
            let max_expansion_samples =
                (self.max_expansion_ms as f64 * self.sample_rate as f64 / 1000.0) as usize;
            let max_allowed_from_gap = gap_samples + max_expansion_samples;
            let max_allowed_from_next = if let Some(next_ms) = next_gap_start_ms {
                let next_start = (next_ms as f64 * self.sample_rate as f64 / 1000.0) as usize;
                next_start.saturating_sub(start_sample)
            } else {
                usize::MAX
            };

            let target_samples = max_allowed_from_gap.min(max_allowed_from_next).max(1);

            if samples.len() > target_samples {
                let ratio = target_samples as f64 / samples.len() as f64;
                let dst_rate = (self.sample_rate as f64 * ratio).round() as u32;
                let dst_rate = dst_rate.max(1);

                if let Ok(resampled) = movie_radio_pipeline::pipeline::resample::resample(
                    &samples,
                    self.sample_rate,
                    dst_rate,
                ) {
                    samples = resampled;
                    if samples.len() > target_samples {
                        samples.truncate(target_samples);
                    }
                }
            }
        }

        let end_sample = start_sample + samples.len();

        NarrationSegment {
            start_sample,
            end_sample,
            samples,
        }
    }

    pub fn build_narration_segments(
        &self,
        scripts: &[NarrationScript],
        narration_audio: &[Option<movie_radio_voice::AudioOutput>],
    ) -> Vec<NarrationSegment> {
        let valid_entries: Vec<(&NarrationScript, &movie_radio_voice::AudioOutput)> = scripts
            .iter()
            .zip(narration_audio.iter())
            .filter_map(|(s, a)| a.as_ref().map(|audio| (s, audio)))
            .collect();

        let mut segments = Vec::new();

        for (idx, (script, audio)) in valid_entries.iter().enumerate() {
            let next_gap_start_ms = valid_entries
                .get(idx + 1)
                .map(|(next_script, _)| next_script.gap_start_ms);

            let seg = self.narration_to_segment_ext(script, &audio.samples, next_gap_start_ms);
            segments.push(seg);
        }

        segments
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_narration(start_sample: usize, len: usize) -> NarrationSegment {
        NarrationSegment {
            start_sample,
            end_sample: start_sample + len,
            samples: vec![0.5; len],
        }
    }

    #[test]
    fn test_assemble_empty_narrations() {
        let assembler = RadioPlayAssembler::new(16000, 50, 0.3);
        let original = vec![0.1; 16000];
        let result = assembler.assemble(&original, &[]).unwrap();
        assert_eq!(result, original);
    }

    #[test]
    fn test_no_overlap_validation() {
        let assembler = RadioPlayAssembler::new(16000, 50, 0.3);
        let narrations = vec![make_narration(100, 200), make_narration(150, 200)];
        assert!(assembler.assemble(&vec![0.0; 1000], &narrations).is_err());
    }

    #[test]
    fn test_assemble_basic() {
        let assembler = RadioPlayAssembler::new(16000, 50, 0.3);
        let original = vec![0.1; 16000];
        let narrations = vec![make_narration(1000, 500)];
        let result = assembler.assemble(&original, &narrations).unwrap();
        assert_eq!(result.len(), original.len());
    }

    #[test]
    fn test_assemble_with_sfx() {
        let assembler = RadioPlayAssembler::new(16000, 50, 0.3);
        let original = vec![0.1; 16000];
        let sfx = vec![SfxSegment {
            start_sample: 2000,
            samples: vec![0.3; 1000],
        }];
        let result = assembler
            .assemble_with_sfx(&original, &[], &sfx)
            .expect("assemble with sfx");
        assert_eq!(result.len(), original.len());
    }

    #[test]
    fn test_time_stretch_fits_natively() {
        let assembler = RadioPlayAssembler::new(16000, 50, 0.3);
        let script = NarrationScript {
            gap_start_ms: 1000,
            gap_end_ms: 3000, // 2000 ms gap = 32000 samples
            text: "Hello".to_string(),
            emotion: movie_radio_voice::Emotion::Neutral,
            word_count: 1,
            duration_ms: 1000,
        };
        let audio_samples = vec![0.2; 16000]; // 1000 ms audio = 16000 samples
        let segment = assembler.narration_to_segment(&script, &audio_samples);
        assert_eq!(segment.samples.len(), 16000);
        assert_eq!(segment.start_sample, 16000);
        assert_eq!(segment.end_sample, 32000);
    }

    #[test]
    fn test_time_stretch_expands_overlong_narration() {
        let assembler = RadioPlayAssembler::new(16000, 50, 0.3);
        let script = NarrationScript {
            gap_start_ms: 1000,
            gap_end_ms: 2000, // 1000 ms gap = 16000 samples
            text: "Overlong narration text".to_string(),
            emotion: movie_radio_voice::Emotion::Neutral,
            word_count: 3,
            duration_ms: 3000,
        };
        let audio_samples = vec![0.2; 48000]; // 3000 ms audio = 48000 samples
        let segment = assembler.narration_to_segment(&script, &audio_samples);

        // Max expansion is 500 ms (8000 samples). So target_samples = 16000 + 8000 = 24000 samples.
        assert!(segment.samples.len() <= 24000);
        assert_eq!(
            segment.end_sample,
            segment.start_sample + segment.samples.len()
        );
    }

    #[test]
    fn test_time_stretch_bounded_by_next_gap_start() {
        let assembler = RadioPlayAssembler::new(16000, 50, 0.3);
        let scripts = vec![
            NarrationScript {
                gap_start_ms: 1000,
                gap_end_ms: 2000, // Gap 1: 16000 to 32000 samples
                text: "First".to_string(),
                emotion: movie_radio_voice::Emotion::Neutral,
                word_count: 1,
                duration_ms: 2000,
            },
            NarrationScript {
                gap_start_ms: 2200, // Gap 2 starts at 35200 samples
                gap_end_ms: 3000,
                text: "Second".to_string(),
                emotion: movie_radio_voice::Emotion::Neutral,
                word_count: 1,
                duration_ms: 800,
            },
        ];
        let narration_audio = vec![
            Some(movie_radio_voice::AudioOutput {
                samples: vec![0.2; 64000], // 4000 ms audio
                sample_rate_hz: 16000,
            }),
            Some(movie_radio_voice::AudioOutput {
                samples: vec![0.3; 8000],
                sample_rate_hz: 16000,
            }),
        ];

        let segments = assembler.build_narration_segments(&scripts, &narration_audio);
        assert_eq!(segments.len(), 2);
        // Segment 0 must not cross Gap 2 start sample (35200).
        assert!(segments[0].end_sample <= segments[1].start_sample);

        // Validate that assemble succeeds without 0% overlap error
        let original = vec![0.1; 64000];
        let assembled = assembler.assemble(&original, &segments);
        assert!(
            assembled.is_ok(),
            "Assembly must succeed with 0% overlap: {:?}",
            assembled.err()
        );
    }

    #[test]
    fn test_time_stretch_disabled() {
        let assembler = RadioPlayAssembler::new(16000, 50, 0.3).with_time_stretch(false, 500);
        let script = NarrationScript {
            gap_start_ms: 1000,
            gap_end_ms: 2000, // 1000 ms gap = 16000 samples
            text: "Overlong".to_string(),
            emotion: movie_radio_voice::Emotion::Neutral,
            word_count: 1,
            duration_ms: 3000,
        };
        let audio_samples = vec![0.2; 48000]; // 3000 ms audio = 48000 samples
        let segment = assembler.narration_to_segment(&script, &audio_samples);
        assert_eq!(segment.samples.len(), 48000);
    }
}
