use crate::pipeline::segmenter;
use crate::types::frame::Frame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameState {
    Speech,
    MusicLike,
    NoiseLike,
    Ambiguous,
}

const AMBIGUOUS_LOW: f32 = 0.35;
const AMBIGUOUS_HIGH: f32 = 0.65;

pub fn resolve_speech_with_ambiguity(
    speech: &[bool],
    frames: &[Frame],
    frame_likelihoods: &[f32],
    frame_ms: u32,
    hangover_ms: u32,
) -> Vec<bool> {
    let states = classify_frame_states(speech, frames, frame_likelihoods);
    smooth_states(&states, frame_ms, hangover_ms)
}

fn classify_frame_states(
    speech: &[bool],
    frames: &[Frame],
    frame_likelihoods: &[f32],
) -> Vec<FrameState> {
    speech
        .iter()
        .enumerate()
        .map(|(idx, &is_speech)| {
            let frame = frames.get(idx);
            let likelihood =
                frame_likelihoods
                    .get(idx)
                    .copied()
                    .unwrap_or(if is_speech { 1.0 } else { 0.0 });

            if let Some(frame) = frame {
                if is_music_like(frame) {
                    return FrameState::MusicLike;
                }
                if is_noise_like(frame) {
                    return FrameState::NoiseLike;
                }
            }

            if likelihood >= AMBIGUOUS_HIGH {
                FrameState::Speech
            } else if likelihood <= AMBIGUOUS_LOW {
                FrameState::NoiseLike
            } else {
                FrameState::Ambiguous
            }
        })
        .collect()
}

fn is_music_like(frame: &Frame) -> bool {
    frame.low_band_ratio > 0.45
        && frame.high_band_ratio < 0.15
        && frame.spectral_flatness < 0.35
        && frame.spectral_entropy < 5.6
        && frame.zcr < 0.08
}

fn is_noise_like(frame: &Frame) -> bool {
    frame.spectral_flatness > 0.45
        || frame.spectral_entropy > 6.2
        || (frame.high_band_ratio > 0.4 && frame.zcr > 0.35)
}

fn smooth_states(states: &[FrameState], frame_ms: u32, hangover_ms: u32) -> Vec<bool> {
    let hang = (hangover_ms / frame_ms.max(1)) as usize;
    let mut out = vec![false; states.len()];
    let mut hard_non_speech = vec![false; states.len()];
    let mut last_speech: Option<usize> = None;

    for (idx, state) in states.iter().enumerate() {
        match state {
            FrameState::Speech => {
                out[idx] = true;
                last_speech = Some(idx);
            }
            FrameState::Ambiguous => {
                let left_speech = idx > 0 && out[idx - 1];
                let right_speech = matches!(states.get(idx + 1), Some(FrameState::Speech));
                let near_recent_speech = last_speech
                    .map(|last| idx.saturating_sub(last) <= hang)
                    .unwrap_or(false);
                if (left_speech && right_speech) || near_recent_speech {
                    out[idx] = true;
                }
            }
            FrameState::MusicLike | FrameState::NoiseLike => {
                hard_non_speech[idx] = true;
            }
        }
    }

    let mut smoothed = segmenter::smooth_speech(&out, frame_ms, hangover_ms);
    for (idx, hard) in hard_non_speech.iter().enumerate() {
        if *hard {
            smoothed[idx] = false;
        }
    }
    smoothed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambiguous_frames_bridge_neighboring_speech() {
        let speech = vec![true, false, true];
        let frames = vec![
            frame(0.1, 0.2, 4.0, 1800.0, 0.2, 0.2),
            frame(0.1, 0.2, 4.0, 1800.0, 0.2, 0.2),
            frame(0.1, 0.2, 4.0, 1800.0, 0.2, 0.2),
        ];
        let likelihoods = vec![0.8, 0.5, 0.85];
        let resolved = resolve_speech_with_ambiguity(&speech, &frames, &likelihoods, 20, 60);
        assert_eq!(resolved, vec![true, true, true]);
    }

    #[test]
    fn low_likelihood_frames_stay_non_voice() {
        let speech = vec![true, false, false];
        let frames = vec![
            frame(0.1, 0.2, 4.0, 1800.0, 0.2, 0.2),
            frame(0.45, 0.6, 7.0, 7000.0, 0.1, 0.6),
            frame(0.45, 0.6, 7.0, 7000.0, 0.1, 0.6),
        ];
        let likelihoods = vec![0.9, 0.2, 0.1];
        let resolved = resolve_speech_with_ambiguity(&speech, &frames, &likelihoods, 20, 60);
        assert_eq!(resolved, vec![true, false, false]);
    }

    #[test]
    fn music_like_frame_stays_non_speech() {
        let speech = vec![true, false, true];
        let frames = vec![
            frame(0.1, 0.2, 4.0, 1800.0, 0.2, 0.2),
            frame(0.03, 0.2, 4.2, 400.0, 0.6, 0.05),
            frame(0.1, 0.2, 4.0, 1800.0, 0.2, 0.2),
        ];
        let likelihoods = vec![0.8, 0.5, 0.85];
        let resolved = resolve_speech_with_ambiguity(&speech, &frames, &likelihoods, 20, 60);
        assert_eq!(resolved, vec![true, false, true]);
    }

    fn frame(
        zcr: f32,
        flatness: f32,
        entropy: f32,
        centroid_hz: f32,
        low: f32,
        high: f32,
    ) -> Frame {
        Frame {
            rms: 0.01,
            zcr,
            spectral_flux: 0.01,
            spectral_flatness: flatness,
            spectral_entropy: entropy,
            centroid_hz,
            low_band_ratio: low,
            high_band_ratio: high,
        }
    }
}
