use super::engine::{VadEngine, VadResult};
use anyhow::{bail, Result};

/// WebRTC VAD engine (feature `webrtc-vad`).
///
/// Native fit for the default pipeline (16 kHz, 20 ms frames). f32 samples are
/// converted to clamped i16; trailing partial frames are zero-padded so the
/// decision count always matches `framing::build_frames` output.
/// `webrtc_vad::Vad` is `!Send` (raw FFI pointer) and `unsafe` is forbidden
/// in this workspace, so only plain config is stored and a fresh instance is
/// built per classification call. Stateless across calls keeps output
/// deterministic for identical input.
pub struct WebRtcVad {
    threshold: f32,
    sample_rate_hz: u32,
    frame_ms: u32,
}

/// Map pipeline sensitivity to a WebRTC aggressiveness mode.
/// Higher pipeline threshold = less sensitive = more aggressive VAD.
///
/// Calibrated 2026-09-04 against `testdata/generated` fixtures (see
/// `threshold_mapping_matches_fixture_truth`): digital silence scores 0.0 in
/// all modes; a 220 Hz tone at 0.25 amplitude scores 1.0 in Quality/LowBitrate
/// and ~0.02 in Aggressive/VeryAggressive; on `alternating.wav` (36.7% speech
/// by truth file) Quality scores 0.39 while stricter modes under-detect.
/// Synthetic tones are not real voice — treat the boundaries as a sane
/// heuristic, not an optimum. Recalibrate against real-voice fixtures before
/// moving the default (0.015 → LowBitrate).
fn mode_for_threshold(threshold: f32) -> webrtc_vad::VadMode {
    use webrtc_vad::VadMode;
    if threshold < 0.01 {
        VadMode::Quality
    } else if threshold < 0.05 {
        VadMode::LowBitrate
    } else if threshold < 0.2 {
        VadMode::Aggressive
    } else {
        VadMode::VeryAggressive
    }
}

fn rate_for_hz(sample_rate_hz: u32) -> Result<webrtc_vad::SampleRate> {
    use webrtc_vad::SampleRate;
    match sample_rate_hz {
        8000 => Ok(SampleRate::Rate8kHz),
        16000 => Ok(SampleRate::Rate16kHz),
        32000 => Ok(SampleRate::Rate32kHz),
        48000 => Ok(SampleRate::Rate48kHz),
        other => bail!("webrtc VAD supports 8000/16000/32000/48000 Hz, got {other}"),
    }
}

impl WebRtcVad {
    pub fn new(threshold: f32, sample_rate_hz: u32, frame_ms: u32) -> Result<Self> {
        rate_for_hz(sample_rate_hz)?;
        if !matches!(frame_ms, 10 | 20 | 30) {
            bail!("webrtc VAD supports 10/20/30 ms frames, got {frame_ms}");
        }
        Ok(Self {
            threshold,
            sample_rate_hz,
            frame_ms,
        })
    }

    fn frame_len(&self) -> usize {
        (self.sample_rate_hz as usize * self.frame_ms as usize) / 1000
    }
}

impl VadEngine for WebRtcVad {
    fn classify(&self, _frames: &[movie_radio_types::Frame]) -> VadResult {
        // Unreachable through the pipeline router (`uses_raw_samples` is true);
        // return empty rather than panic for direct misuse.
        VadResult::new(Vec::new(), Vec::new())
    }

    fn name(&self) -> &'static str {
        "webrtc"
    }

    fn uses_raw_samples(&self) -> bool {
        true
    }

    fn classify_samples(
        &mut self,
        samples: &[f32],
        sample_rate_hz: u32,
        frame_ms: u32,
    ) -> Result<VadResult> {
        if sample_rate_hz != self.sample_rate_hz || frame_ms != self.frame_ms {
            bail!(
                "webrtc VAD configured for {}/{} ms, got {}/{} ms",
                self.sample_rate_hz,
                self.frame_ms,
                sample_rate_hz,
                frame_ms
            );
        }
        let frame_len = self.frame_len();
        let rate = rate_for_hz(self.sample_rate_hz)?;
        let mut vad =
            webrtc_vad::Vad::new_with_rate_and_mode(rate, mode_for_threshold(self.threshold));
        let mut decisions = Vec::new();
        let mut likelihoods = Vec::new();
        let mut int_buf = vec![0i16; frame_len];
        for chunk in samples.chunks(frame_len) {
            if chunk.is_empty() {
                continue;
            }
            for (dst, &s) in int_buf.iter_mut().zip(chunk.iter()) {
                *dst = (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
            }
            for dst in int_buf.iter_mut().skip(chunk.len()) {
                *dst = 0;
            }
            let speech = vad
                .is_voice_segment(&int_buf)
                .map_err(|()| anyhow::anyhow!("webrtc VAD rejected frame length"))?;
            decisions.push(speech);
            likelihoods.push(if speech { 1.0 } else { 0.0 });
        }
        Ok(VadResult::new(decisions, likelihoods))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsupported_rate() {
        assert!(WebRtcVad::new(0.015, 44100, 20).is_err());
    }

    #[test]
    fn rejects_unsupported_frame_ms() {
        assert!(WebRtcVad::new(0.015, 16000, 25).is_err());
    }

    #[test]
    fn silence_is_deterministic_non_speech() -> Result<()> {
        let mut vad = WebRtcVad::new(0.015, 16000, 20)?;
        let silence = vec![0.0f32; 3200];
        let first = vad.classify_samples(&silence, 16000, 20)?;
        let second = vad.classify_samples(&silence, 16000, 20)?;
        assert_eq!(first.decisions.len(), 10);
        assert_eq!(first.decisions, second.decisions);
        assert!(first.decisions.iter().all(|&d| !d));
        Ok(())
    }

    #[test]
    fn decision_count_matches_framing_with_tail() -> Result<()> {
        let mut vad = WebRtcVad::new(0.015, 16000, 20)?;
        // 330 samples -> framing yields 2 frames (320 + 10 tail).
        let out = vad.classify_samples(&vec![0.0f32; 330], 16000, 20)?;
        assert_eq!(out.decisions.len(), 2);
        assert_eq!(out.likelihoods.len(), 2);
        Ok(())
    }

    fn fixture_samples(name: &str) -> Vec<f32> {
        let path = format!("../../testdata/generated/{name}.wav");
        let mut reader = hound::WavReader::open(&path).expect("fixture wav");
        reader
            .samples::<i16>()
            .map(|s| s.expect("fixture sample") as f32 / f32::from(i16::MAX))
            .collect()
    }

    fn speech_fraction(samples: &[f32], threshold: f32) -> f32 {
        let mut vad = WebRtcVad::new(threshold, 16000, 20).expect("webrtc engine");
        let out = vad.classify_samples(samples, 16000, 20).expect("classify");
        out.decisions.iter().filter(|&&d| d).count() as f32 / out.decisions.len().max(1) as f32
    }

    /// Calibration regression guard (see `mode_for_threshold` docs).
    #[test]
    fn threshold_mapping_matches_fixture_truth() {
        let silence = fixture_samples("silence_only");
        let speech = fixture_samples("speech_only");
        let alternating = fixture_samples("alternating");

        // Digital silence never fires, in any mode.
        for t in [0.005, 0.015, 0.1, 0.5] {
            assert_eq!(speech_fraction(&silence, t), 0.0, "threshold {t}");
        }
        // Loud tone: fully detected by sensitive modes, rejected by strict ones.
        assert_eq!(speech_fraction(&speech, 0.005), 1.0);
        assert_eq!(speech_fraction(&speech, 0.015), 1.0);
        assert!(speech_fraction(&speech, 0.1) < 0.1);
        assert!(speech_fraction(&speech, 0.5) < 0.1);
        // Alternating truth is 36.7% speech; Quality (0.39) tracks it,
        // stricter modes under-detect synthetic tones.
        let quality = speech_fraction(&alternating, 0.005);
        assert!(
            (quality - 0.367).abs() < 0.1,
            "Quality should track alternating truth, got {quality}"
        );
        assert!(speech_fraction(&alternating, 0.5) < quality);
    }
}
