#![allow(clippy::excessive_precision, clippy::approx_constant)]

/// Reverb preset for a scene or individual track.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReverbConfig {
    /// Echo delay in milliseconds (e.g. 30 for small room, 150 for hall)
    pub delay_ms: u64,
    /// Reverb amplitude multiplier [0.0, 1.0]
    pub amplitude: f32,
}

impl ReverbConfig {
    pub const DRY: Self = Self {
        delay_ms: 0,
        amplitude: 0.0,
    };
    pub const SMALL_ROOM: Self = Self {
        delay_ms: 30,
        amplitude: 0.3,
    };
    pub const MEDIUM_ROOM: Self = Self {
        delay_ms: 60,
        amplitude: 0.4,
    };
    pub const LARGE_HALL: Self = Self {
        delay_ms: 150,
        amplitude: 0.6,
    };
    pub const OUTDOOR: Self = Self {
        delay_ms: 80,
        amplitude: 0.2,
    };
}

/// Pan position from -1.0 (hard left) to 1.0 (hard right)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StereoPosition {
    pos: f32,
    left_gain: f32,
    right_gain: f32,
}

impl serde::Serialize for StereoPosition {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_f32(self.pos)
    }
}

impl<'de> serde::Deserialize<'de> for StereoPosition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let pos = f32::deserialize(deserializer)?;
        Self::new(pos).map_err(serde::de::Error::custom)
    }
}

fn calculate_gains(pos: f32) -> (f32, f32) {
    let angle = (pos + 1.0) * std::f32::consts::FRAC_PI_4;
    let (sin, cos) = angle.sin_cos();
    (cos, sin)
}

impl StereoPosition {
    pub const CENTRE: Self = Self {
        pos: 0.0,
        left_gain: 0.70710678,
        right_gain: 0.70710678,
    };
    pub const LEFT: Self = Self {
        pos: -0.7,
        left_gain: 0.97236992,
        right_gain: 0.23344536,
    };
    pub const RIGHT: Self = Self {
        pos: 0.7,
        left_gain: 0.23344536,
        right_gain: 0.97236992,
    };
    pub const HARD_LEFT: Self = Self {
        pos: -1.0,
        left_gain: 1.0,
        right_gain: 0.0,
    };
    pub const HARD_RIGHT: Self = Self {
        pos: 1.0,
        left_gain: 0.0,
        right_gain: 1.0,
    };

    pub fn new(pos: f32) -> anyhow::Result<Self> {
        if !(-1.0..=1.0).contains(&pos) {
            anyhow::bail!("StereoPosition must be in [-1.0, 1.0], got {pos}");
        }
        let (left_gain, right_gain) = calculate_gains(pos);
        Ok(Self {
            pos,
            left_gain,
            right_gain,
        })
    }

    pub fn pos(&self) -> f32 {
        self.pos
    }

    pub fn gains(self) -> (f32, f32) {
        (self.left_gain, self.right_gain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stereo_position_new_valid() {
        assert!(StereoPosition::new(0.0).is_ok());
        assert!(StereoPosition::new(-1.0).is_ok());
        assert!(StereoPosition::new(1.0).is_ok());
        assert!(StereoPosition::new(0.5).is_ok());
    }

    #[test]
    fn test_stereo_position_new_invalid() {
        assert!(StereoPosition::new(-1.01).is_err());
        assert!(StereoPosition::new(1.01).is_err());
        assert!(StereoPosition::new(-5.0).is_err());
        assert!(StereoPosition::new(5.0).is_err());
    }

    #[test]
    fn test_stereo_position_exact_gains() {
        // CENTRE (0.0): angle = FRAC_PI_4. cos = ~0.70710678, sin = ~0.70710678
        let centre_gains = StereoPosition::CENTRE.gains();
        assert!((centre_gains.0 - 0.70710678).abs() < 1e-6);
        assert!((centre_gains.1 - 0.70710678).abs() < 1e-6);

        // HARD_LEFT (-1.0): angle = 0. cos = 1.0, sin = 0.0
        let hard_left_gains = StereoPosition::HARD_LEFT.gains();
        assert_eq!(hard_left_gains.0, 1.0);
        assert_eq!(hard_left_gains.1, 0.0);

        // HARD_RIGHT (1.0): angle = PI_2. cos = 0.0, sin = 1.0
        let hard_right_gains = StereoPosition::HARD_RIGHT.gains();
        assert_eq!(hard_right_gains.0, 0.0);
        assert_eq!(hard_right_gains.1, 1.0);
    }

    #[test]
    fn test_stereo_position_serde() -> anyhow::Result<()> {
        let original = StereoPosition::new(0.5)?;
        let serialized = serde_json::to_string(&original)?;
        // Must serialize exactly as a plain float
        assert_eq!(serialized, "0.5");

        let deserialized: StereoPosition = serde_json::from_str("0.5")?;
        assert_eq!(deserialized.pos(), 0.5);
        assert_eq!(deserialized.gains(), original.gains());
        Ok(())
    }
}
