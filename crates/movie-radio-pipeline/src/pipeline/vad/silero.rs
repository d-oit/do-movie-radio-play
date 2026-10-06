use super::engine::{VadEngine, VadResult};
use anyhow::{bail, Context, Result};
use ort::session::Session;
use ort::value::Tensor;
use std::path::{Path, PathBuf};

const ENV_MODEL: &str = "SILERO_VAD_MODEL";
const ENV_THRESHOLD: &str = "SILERO_VAD_THRESHOLD";
const DEFAULT_MODEL: &str = "models/silero_vad.onnx";
const DEFAULT_THRESHOLD: f32 = 0.3;
const WINDOW: usize = 512;
const CONTEXT: usize = 64;
const STATE_LEN: usize = 2 * 128;

/// Silero VAD v5 (feature `silero-vad`): a neural speech detector that is
/// independent of the spectral statistics the other engines share (#362).
///
/// Needs the ONNX model (`SILERO_VAD_MODEL`, default `models/silero_vad.onnx`,
/// see `scripts/fetch_silero_vad.sh`) and an ONNX Runtime shared library
/// (`ORT_DYLIB_PATH`). The pipeline `threshold` is tuned for energy RMS and is
/// meaningless for a probability, so the cut-off comes from
/// `SILERO_VAD_THRESHOLD` (default 0.3). Each call starts from a zero state,
/// so identical input yields identical output.
pub struct SileroVad {
    session: Session,
    threshold: f32,
}

impl SileroVad {
    pub fn new(sample_rate_hz: u32) -> Result<Self> {
        if sample_rate_hz != 16_000 {
            bail!("silero VAD is wired for 16000 Hz, got {sample_rate_hz}");
        }
        let model =
            std::env::var(ENV_MODEL).map_or_else(|_| PathBuf::from(DEFAULT_MODEL), PathBuf::from);
        let threshold = match std::env::var(ENV_THRESHOLD) {
            Ok(raw) => raw
                .parse::<f32>()
                .ok()
                .filter(|t| (0.0..=1.0).contains(t))
                .with_context(|| {
                    format!("{ENV_THRESHOLD} must be a number in 0..=1, got '{raw}'")
                })?,
            Err(_) => DEFAULT_THRESHOLD,
        };
        Ok(Self {
            session: load_session(&model)?,
            threshold,
        })
    }

    fn window_probabilities(&mut self, samples: &[f32]) -> Result<Vec<f32>> {
        let mut state = vec![0.0f32; STATE_LEN];
        let mut context = [0.0f32; CONTEXT];
        let mut probs = Vec::with_capacity(samples.len() / WINDOW + 1);
        let mut input = vec![0.0f32; CONTEXT + WINDOW];
        for chunk in samples.chunks(WINDOW) {
            input[..CONTEXT].copy_from_slice(&context);
            input[CONTEXT..CONTEXT + chunk.len()].copy_from_slice(chunk);
            input[CONTEXT + chunk.len()..].fill(0.0);
            context.copy_from_slice(&input[WINDOW..]);

            let outputs = self.session.run(ort::inputs![
                "input" => Tensor::from_array(([1usize, CONTEXT + WINDOW], input.clone()))?,
                "state" => Tensor::from_array(([2usize, 1, 128], state.clone()))?,
                "sr" => Tensor::from_array((Vec::<i64>::new(), vec![16_000i64]))?,
            ])?;
            let (_, prob) = outputs["output"].try_extract_tensor::<f32>()?;
            probs.push(*prob.first().context("silero output was empty")?);
            let (_, next) = outputs["stateN"].try_extract_tensor::<f32>()?;
            if next.len() != STATE_LEN {
                bail!(
                    "silero state has {} values, expected {STATE_LEN}",
                    next.len()
                );
            }
            state.copy_from_slice(next);
        }
        Ok(probs)
    }
}

fn load_session(model: &Path) -> Result<Session> {
    if !model.is_file() {
        bail!(
            "silero model not found at {} (set {ENV_MODEL} or run scripts/fetch_silero_vad.sh)",
            model.display()
        );
    }
    Session::builder()?
        .commit_from_file(model)
        .with_context(|| {
            format!(
                "failed to load silero model {} (is ORT_DYLIB_PATH set?)",
                model.display()
            )
        })
}

/// Piecewise-linear remap so `threshold` lands on 0.5, the middle of the
/// tri-state ambiguous band; the threshold then steers the downstream decision.
fn recentre(p: f32, threshold: f32) -> f32 {
    let t = threshold.clamp(1e-3, 1.0 - 1e-3);
    if p <= t {
        0.5 * p / t
    } else {
        0.5 + 0.5 * (p - t) / (1.0 - t)
    }
}

/// Map per-window probabilities onto pipeline frames by frame-centre time.
fn frames_from_windows(
    probs: &[f32],
    n_samples: usize,
    sample_rate_hz: u32,
    frame_ms: u32,
    threshold: f32,
) -> VadResult {
    let frame_len = (sample_rate_hz as usize * frame_ms as usize) / 1000;
    let n_frames = n_samples.div_ceil(frame_len);
    let mut decisions = Vec::with_capacity(n_frames);
    let mut likelihoods = Vec::with_capacity(n_frames);
    for i in 0..n_frames {
        let centre = i * frame_len + frame_len / 2;
        let p = probs
            .get((centre / WINDOW).min(probs.len().saturating_sub(1)))
            .copied()
            .unwrap_or(0.0);
        decisions.push(p >= threshold);
        likelihoods.push(recentre(p, threshold));
    }
    VadResult::new(decisions, likelihoods)
}

impl VadEngine for SileroVad {
    fn classify(&self, _frames: &[movie_radio_types::Frame]) -> VadResult {
        VadResult::new(Vec::new(), Vec::new())
    }

    fn name(&self) -> &'static str {
        "silero"
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
        if sample_rate_hz != 16_000 || frame_ms == 0 {
            bail!("silero VAD needs 16000 Hz and a non-zero frame size, got {sample_rate_hz}/{frame_ms} ms");
        }
        let probs = self.window_probabilities(samples)?;
        Ok(frames_from_windows(
            &probs,
            samples.len(),
            sample_rate_hz,
            frame_ms,
            self.threshold,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_follow_window_probabilities() {
        // 3 windows (96 ms): silence, speech, silence; 20 ms frames.
        let r = frames_from_windows(&[0.1, 0.9, 0.2], 3 * WINDOW, 16_000, 20, 0.5);
        assert_eq!(r.decisions.len(), (3 * WINDOW).div_ceil(320));
        // Frame centres (160, 480, 800, 1120, 1440) fall in windows 0, 0, 1, 2, 2.
        assert_eq!(r.decisions, vec![false, false, true, false, false]);
    }

    #[test]
    fn recentre_maps_threshold_to_half_and_keeps_order() {
        assert!((recentre(0.8, 0.8) - 0.5).abs() < 1e-6);
        assert_eq!((recentre(0.0, 0.3), recentre(1.0, 0.3)), (0.0, 1.0));
        assert!(recentre(0.2, 0.3) < recentre(0.4, 0.3));
    }

    #[test]
    fn empty_probabilities_are_non_speech() {
        let r = frames_from_windows(&[], 640, 16_000, 20, 0.5);
        assert_eq!(r.decisions, vec![false, false]);
    }

    #[test]
    fn missing_model_errors_helpfully() {
        let err = load_session(Path::new("/nonexistent/silero.onnx"))
            .err()
            .map(|e| e.to_string());
        assert!(err.is_some_and(|m| m.contains("fetch_silero_vad.sh")));
    }
}
