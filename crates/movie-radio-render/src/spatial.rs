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
