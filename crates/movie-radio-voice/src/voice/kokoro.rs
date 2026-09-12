use anyhow::{Context, Result};
use async_trait::async_trait;
use ort::session::Session;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

use super::{AudioOutput, ProviderCapabilities, SynthesisRequest, VoiceSynthesizer};
use crate::config::KokoroConfig;

const KOKORO_MODEL_URL: &str =
    "https://huggingface.co/Godelaune/Kokoro-82M-ONNX-German-Martin/resolve/main/model.onnx";

const KOKORO_SAMPLE_RATE: u32 = 24000;

pub struct KokoroProvider {
    config: KokoroConfig,
    session: Option<Arc<Mutex<Session>>>,
}

impl KokoroProvider {
    pub fn new(config: KokoroConfig) -> Self {
        Self {
            config,
            session: None,
        }
    }

    fn model_dir(&self) -> PathBuf {
        self.config
            .model_path
            .parent()
            .unwrap_or(&PathBuf::from("."))
            .to_path_buf()
    }

    fn onnx_path(&self) -> PathBuf {
        self.model_dir().join("kokoro-german-martin.onnx")
    }

    async fn ensure_model(&self) -> Result<()> {
        let onnx_path = self.onnx_path();
        if onnx_path.exists() {
            info!(path = %onnx_path.display(), "Kokoro model found");
            return Ok(());
        }

        info!("Downloading Kokoro German Martin model...");
        let model_dir = self.model_dir();
        std::fs::create_dir_all(&model_dir)?;

        let client = reqwest::Client::new();
        let response = client
            .get(KOKORO_MODEL_URL)
            .send()
            .await
            .context("Failed to download model")?;

        let bytes = response
            .bytes()
            .await
            .context("Failed to read model bytes")?;

        std::fs::write(&onnx_path, &bytes)?;
        info!(
            path = %onnx_path.display(),
            size_mb = bytes.len() / (1024 * 1024),
            "Model downloaded"
        );

        Ok(())
    }

    fn load_session(&self) -> Result<Arc<Mutex<Session>>> {
        if let Some(session) = &self.session {
            return Ok(Arc::clone(session));
        }

        let onnx_path = self.onnx_path();
        if !onnx_path.exists() {
            anyhow::bail!("Model not found at {}", onnx_path.display());
        }

        let session = Session::builder()
            .map_err(|e| anyhow::anyhow!("Failed to create session builder: {}", e))?
            .with_intra_threads(2)
            .map_err(|e| anyhow::anyhow!("Failed to set threads: {}", e))?
            .commit_from_file(&onnx_path)
            .map_err(|e| anyhow::anyhow!("Failed to load model: {}", e))?;

        info!(
            inputs = session.inputs().len(),
            outputs = session.outputs().len(),
            "Kokoro session loaded"
        );
        for input in session.inputs() {
            debug!(name = %input.name(), "model input");
        }
        for output in session.outputs() {
            debug!(name = %output.name(), "model output");
        }

        Ok(Arc::new(Mutex::new(session)))
    }

    /// Maps a character / phoneme symbol to its corresponding token ID in the
    /// Kokoro / espeak-ng eSD phoneme vocabulary.
    pub fn phoneme_to_token(c: char) -> Option<i64> {
        let token = match c {
            ';' => Some(1),
            ':' => Some(2),
            ',' => Some(3),
            '.' => Some(4),
            '!' => Some(5),
            '?' => Some(6),
            '—' => Some(9),
            '…' => Some(10),
            '"' => Some(11),
            '(' => Some(12),
            ')' => Some(13),
            '“' => Some(14),
            '”' => Some(15),
            ' ' => Some(16),
            '̃' => Some(17),
            'ʣ' => Some(18),
            'ʥ' => Some(19),
            'ʦ' => Some(20),
            'ʨ' => Some(21),
            'ᵝ' => Some(22),
            'ꭧ' => Some(23),
            'A' => Some(24),
            'I' => Some(25),
            'O' => Some(31),
            'Q' => Some(33),
            'S' => Some(35),
            'T' => Some(36),
            'W' => Some(39),
            'Y' => Some(41),
            'ᵊ' => Some(42),
            'a' => Some(43),
            'b' => Some(44),
            'c' => Some(45),
            'd' => Some(46),
            'e' => Some(47),
            'f' => Some(48),
            'h' => Some(50),
            'i' => Some(51),
            'j' => Some(52),
            'k' => Some(53),
            'l' => Some(54),
            'm' => Some(55),
            'n' => Some(56),
            'o' => Some(57),
            'p' => Some(58),
            'q' => Some(59),
            'r' => Some(60),
            's' => Some(61),
            't' => Some(62),
            'u' => Some(63),
            'v' => Some(64),
            'w' => Some(65),
            'x' => Some(66),
            'y' => Some(67),
            'z' => Some(68),
            'ɑ' => Some(69),
            'ɐ' => Some(70),
            'ɒ' => Some(71),
            'æ' => Some(72),
            'β' => Some(75),
            'ɔ' => Some(76),
            'ɕ' => Some(77),
            'ç' => Some(78),
            'ɖ' => Some(80),
            'ð' => Some(81),
            'ʤ' => Some(82),
            'ə' => Some(83),
            'ɚ' => Some(85),
            'ɛ' => Some(86),
            'ɜ' => Some(87),
            'ɟ' => Some(90),
            'ɡ' => Some(92),
            'ɥ' => Some(99),
            'ɨ' => Some(101),
            'ɪ' => Some(102),
            'ʝ' => Some(103),
            'ɯ' => Some(110),
            'ɰ' => Some(111),
            'ŋ' => Some(112),
            'ɳ' => Some(113),
            'ɲ' => Some(114),
            'ɴ' => Some(115),
            'ø' => Some(116),
            'ɸ' => Some(118),
            'θ' => Some(119),
            'œ' => Some(120),
            'ɹ' => Some(123),
            'ɾ' => Some(125),
            'ɻ' => Some(126),
            'ʁ' => Some(128),
            'ɽ' => Some(129),
            'ʂ' => Some(130),
            'ʃ' => Some(131),
            'ʈ' => Some(132),
            'ʧ' => Some(133),
            'ʊ' => Some(135),
            'ʋ' => Some(136),
            'ʌ' => Some(138),
            'ɣ' => Some(139),
            'ɤ' => Some(140),
            'χ' => Some(142),
            'ʎ' => Some(143),
            'ʒ' => Some(147),
            'ʔ' => Some(148),
            'ˈ' => Some(156),
            'ˌ' => Some(157),
            'ː' => Some(158),
            'ʰ' => Some(162),
            'ʲ' => Some(164),
            '↓' => Some(169),
            '→' => Some(171),
            '↗' => Some(172),
            '↘' => Some(173),
            'ᵻ' => Some(177),
            _ => None,
        };

        token.or_else(|| {
            c.to_lowercase()
                .next()
                .filter(|&lower| lower != c)
                .and_then(Self::phoneme_to_token)
        })
    }

    fn phonemize_german(&self, text: &str) -> String {
        let mut result = String::new();
        for ch in text.chars() {
            match ch {
                'ä' => result.push_str("ae"),
                'ö' => result.push_str("oe"),
                'ü' => result.push_str("ue"),
                'Ä' => result.push_str("Ae"),
                'Ö' => result.push_str("Oe"),
                'Ü' => result.push_str("Ue"),
                'ß' => result.push_str("ss"),
                'é' | 'è' | 'ê' => result.push('e'),
                'á' | 'à' => result.push('a'),
                'ô' => result.push('o'),
                'î' => result.push('i'),
                'û' => result.push('u'),
                _ => result.push(ch),
            }
        }
        result
    }

    fn text_to_tokens(&self, text: &str) -> Vec<i64> {
        let phonemes = self.phonemize_german(text);
        phonemes
            .chars()
            .filter_map(Self::phoneme_to_token)
            .collect()
    }

    fn resample(&self, samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
        if from_rate == to_rate {
            return samples.to_vec();
        }
        let ratio = to_rate as f64 / from_rate as f64;
        let output_len = (samples.len() as f64 * ratio) as usize;
        let mut resampled = Vec::with_capacity(output_len);
        for i in 0..output_len {
            let src_idx = i as f64 / ratio;
            let idx = src_idx as usize;
            let frac = src_idx - idx as f64;
            let s0 = samples[idx.min(samples.len() - 1)];
            let s1 = samples[(idx + 1).min(samples.len() - 1)];
            resampled.push(s0 + (s1 - s0) * frac as f32);
        }
        resampled
    }

    fn synthesize_with_session(
        &self,
        session: &Mutex<Session>,
        request: &SynthesisRequest,
    ) -> Result<AudioOutput> {
        let tokens = self.text_to_tokens(&request.text);
        info!(token_count = tokens.len(), "Running Kokoro inference");

        let mut guard = session
            .lock()
            .map_err(|_| anyhow::anyhow!("Session lock poisoned"))?;

        let input_name = guard
            .inputs()
            .first()
            .context("Model has no inputs")?
            .name()
            .to_owned();

        let output_name = guard
            .outputs()
            .first()
            .context("Model has no outputs")?
            .name()
            .to_owned();

        let input_shape = [1usize, tokens.len()];
        let input_value = ort::value::TensorRef::from_array_view((input_shape, tokens.as_slice()))
            .context("Failed to create input tensor")?;

        let outputs = guard
            .run(ort::inputs![input_name.as_str() => input_value.into_dyn()])
            .context("ONNX inference failed")?;

        let output_tensor = outputs
            .get(output_name.as_str())
            .context("Output tensor not found")?;

        let (shape, data) = output_tensor
            .try_extract_tensor::<f32>()
            .context("Failed to extract f32 output tensor")?;

        let raw_samples: Vec<f32> = data.to_vec();

        if raw_samples.iter().all(|&s| s == 0.0) {
            warn!(
                shape = ?shape,
                "Kokoro produced all-zero output, model may not be loaded correctly"
            );
        }

        let samples = self.resample(&raw_samples, KOKORO_SAMPLE_RATE, request.sample_rate_hz);

        info!(
            raw_len = raw_samples.len(),
            output_len = samples.len(),
            "Kokoro inference complete"
        );

        Ok(AudioOutput {
            samples,
            sample_rate_hz: request.sample_rate_hz,
        })
    }
}

#[async_trait]
impl VoiceSynthesizer for KokoroProvider {
    async fn synthesize(&self, request: &SynthesisRequest) -> Result<AudioOutput> {
        self.ensure_model().await?;
        let session = self.load_session()?;

        self.synthesize_with_session(&session, request)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_emotion: true,
            supports_voice_cloning: false,
            supports_streaming: false,
            max_text_length: 1000,
            languages: vec!["de".to_string()],
            requires_gpu: false,
        }
    }

    fn estimate_cost(&self, _text_len: usize) -> f64 {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phonemize_german() {
        let config = KokoroConfig {
            model_path: "models/dummy.onnx".into(),
            device: "cpu".into(),
        };
        let provider = KokoroProvider::new(config);

        assert_eq!(provider.phonemize_german("Hallo"), "Hallo");
        assert_eq!(provider.phonemize_german("Über"), "Ueber");
        assert_eq!(provider.phonemize_german("über"), "ueber");
        assert_eq!(provider.phonemize_german("Straße"), "Strasse");
        assert_eq!(provider.phonemize_german("schön"), "schoen");
    }

    #[test]
    fn test_text_to_tokens() {
        let config = KokoroConfig {
            model_path: "models/dummy.onnx".into(),
            device: "cpu".into(),
        };
        let provider = KokoroProvider::new(config);

        let tokens = provider.text_to_tokens("Hi");
        // 'H' maps via lowercase fallback to 'h' (50), 'i' maps to (51)
        assert_eq!(tokens, vec![50, 51]);
    }

    #[test]
    fn test_text_to_tokens_german_phrase() {
        let config = KokoroConfig {
            model_path: "models/dummy.onnx".into(),
            device: "cpu".into(),
        };
        let provider = KokoroProvider::new(config);

        let tokens = provider.text_to_tokens("Hallo, Straße!");
        // "Hallo, Straße!" -> phonemize_german -> "Hallo, Strasse!"
        // H(50), a(43), l(54), l(54), o(57), ,(3),  (16), S(35), t(62), r(60), a(43), s(61), s(61), e(47), !(5)
        assert_eq!(
            tokens,
            vec![50, 43, 54, 54, 57, 3, 16, 35, 62, 60, 43, 61, 61, 47, 5]
        );
    }

    #[test]
    fn test_resample() {
        let config = KokoroConfig {
            model_path: "models/dummy.onnx".into(),
            device: "cpu".into(),
        };
        let provider = KokoroProvider::new(config);

        let input = vec![1.0, 2.0, 3.0, 4.0];
        let output = provider.resample(&input, 24000, 16000);
        assert!(!output.is_empty());
        assert!((output.len() as f64 - 4.0 * 16000.0 / 24000.0).abs() < 2.0);
    }
}
