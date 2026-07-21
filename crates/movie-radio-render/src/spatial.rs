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
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StereoPosition(pub f32);

impl StereoPosition {
    pub const CENTRE: Self = Self(0.0);
    pub const LEFT: Self = Self(-0.7);
    pub const RIGHT: Self = Self(0.7);
    pub const HARD_LEFT: Self = Self(-1.0);
    pub const HARD_RIGHT: Self = Self(1.0);

    pub fn new(pos: f32) -> anyhow::Result<Self> {
        if !(-1.0..=1.0).contains(&pos) {
            anyhow::bail!("StereoPosition must be in [-1.0, 1.0], got {pos}");
        }
        Ok(Self(pos))
    }

    pub fn gains(self) -> (f32, f32) {
        let angle = (self.0 + 1.0) * std::f32::consts::FRAC_PI_4;
        let (sin, cos) = angle.sin_cos();
        (cos, sin)
    }
}
