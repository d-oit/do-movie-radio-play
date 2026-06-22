use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::goap::narrate::NarrationScript;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrationSegment {
    pub start_sample: usize,
    pub end_sample: usize,
    pub samples: Vec<f32>,
}

pub struct RadioPlayAssembler {
    pub crossfade_samples: usize,
    pub duck_level: f32,
    pub sample_rate: u32,
}

impl RadioPlayAssembler {
    pub fn new(sample_rate: u32, crossfade_ms: u64, duck_level: f32) -> Self {
        let crossfade_samples = (sample_rate as f64 * crossfade_ms as f64 / 1000.0) as usize;
        Self {
            crossfade_samples,
            duck_level: duck_level.clamp(0.0, 1.0),
            sample_rate,
        }
    }

    pub fn assemble(
        &self,
        original: &[f32],
        narrations: &[NarrationSegment],
    ) -> Result<Vec<f32>> {
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
        end: usize,
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

            let fade_in = if i < cf {
                i as f32 / cf as f32
            } else {
                1.0
            };

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
        let start_sample = (script.gap_start_ms as f64 * self.sample_rate as f64 / 1000.0) as usize;
        let duration_samples = (audio_samples.len() as f64 / self.sample_rate as f64 * 1000.0) as usize;
        let end_sample = start_sample + duration_samples;

        NarrationSegment {
            start_sample,
            end_sample,
            samples: audio_samples.to_vec(),
        }
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
        let narrations = vec![
            make_narration(100, 200),
            make_narration(150, 200),
        ];
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
}
