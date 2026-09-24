use super::{Emotion, SPEED_RANGE};

impl Emotion {
    /// Deterministic tempo multiplier for this emotion. All factors stay
    /// well inside `SPEED_RANGE` so every sidecar/API `speed` lever accepts
    /// them directly (Kokoro, OpenAI-compatible, Modal/Piper, audio.cpp).
    pub fn tempo_factor(&self) -> f32 {
        match self {
            Emotion::Neutral | Emotion::Custom(_) => 1.0,
            Emotion::Excited => 1.15,
            Emotion::Joyful => 1.1,
            Emotion::Angry => 1.12,
            Emotion::Tense => 1.05,
            Emotion::Sad => 0.9,
            Emotion::Mysterious => 0.95,
            Emotion::Whisper => 0.85,
        }
    }

    /// Caller base speed scaled by this emotion's tempo, clamped to
    /// `SPEED_RANGE`. Non-finite bases fall back to the pure factor.
    pub fn effective_speed(&self, base: f32) -> f32 {
        let base = if base.is_finite() { base } else { 1.0 };
        (base * self.tempo_factor()).clamp(*SPEED_RANGE.start(), *SPEED_RANGE.end())
    }

    /// ElevenLabs `voice_settings` for this emotion as `(stability, style)`,
    /// both `0.0..=1.0`. Lower stability widens the emotional range, higher
    /// style exaggerates delivery (v2+ models, the repo default). The config
    /// stability is the Neutral anchor; every other emotion offsets from it.
    pub fn elevenlabs_settings(&self, base_stability: f32) -> (f32, f32) {
        let base = if base_stability.is_finite() {
            base_stability
        } else {
            0.5
        };
        let (delta, style) = match self {
            Emotion::Neutral | Emotion::Custom(_) => (0.0, 0.0),
            Emotion::Excited => (-0.2, 0.7),
            Emotion::Sad => (0.1, 0.3),
            Emotion::Tense => (-0.2, 0.6),
            Emotion::Mysterious => (0.1, 0.4),
            Emotion::Joyful => (-0.1, 0.6),
            Emotion::Whisper => (0.3, 0.1),
            Emotion::Angry => (-0.25, 0.8),
        };
        ((base + delta).clamp(0.0, 1.0), style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_emotions() -> Vec<Emotion> {
        vec![
            Emotion::Neutral,
            Emotion::Excited,
            Emotion::Sad,
            Emotion::Tense,
            Emotion::Mysterious,
            Emotion::Joyful,
            Emotion::Whisper,
            Emotion::Angry,
            Emotion::Custom("[laughs]".to_string()),
        ]
    }

    #[test]
    fn tempo_factors_stay_inside_speed_range() {
        for emotion in all_emotions() {
            let factor = emotion.tempo_factor();
            assert!(factor.is_finite(), "{emotion:?}");
            assert!(SPEED_RANGE.contains(&factor), "{emotion:?} -> {factor}");
        }
        assert_eq!(Emotion::Neutral.tempo_factor(), 1.0);
        assert_eq!(Emotion::Custom("anything".to_string()).tempo_factor(), 1.0);
    }

    #[test]
    fn effective_speed_clamps_to_range() {
        assert_eq!(Emotion::Excited.effective_speed(4.0), 4.0);
        // 0.25 * 0.85 underflows the range, so it clamps back to 0.25.
        assert_eq!(Emotion::Whisper.effective_speed(0.25), 0.25);
        assert_eq!(Emotion::Neutral.effective_speed(1.0), 1.0);
        // Non-finite bases degrade to the pure tempo factor.
        assert_eq!(
            Emotion::Sad.effective_speed(f32::NAN),
            Emotion::Sad.tempo_factor()
        );
    }

    #[test]
    fn elevenlabs_settings_stay_in_unit_range() {
        for emotion in all_emotions() {
            for base in [0.0, 0.5, 1.0] {
                let (stability, style) = emotion.elevenlabs_settings(base);
                assert!((0.0..=1.0).contains(&stability), "{emotion:?}");
                assert!((0.0..=1.0).contains(&style), "{emotion:?}");
            }
        }
        assert_eq!(Emotion::Neutral.elevenlabs_settings(0.5), (0.5, 0.0));
        // Non-finite config stability degrades to the 0.5 anchor.
        assert_eq!(
            Emotion::Neutral.elevenlabs_settings(f32::INFINITY),
            (0.5, 0.0)
        );
    }

    #[test]
    fn intense_emotions_lower_stability_and_raise_style() {
        let (tense_stability, tense_style) = Emotion::Tense.elevenlabs_settings(0.5);
        let (angry_stability, angry_style) = Emotion::Angry.elevenlabs_settings(0.5);
        assert!(tense_stability < 0.5 && tense_style > 0.0);
        assert!(angry_stability < tense_stability && angry_style > tense_style);
        let (whisper_stability, _) = Emotion::Whisper.elevenlabs_settings(0.5);
        assert!(whisper_stability > 0.5);
    }
}
