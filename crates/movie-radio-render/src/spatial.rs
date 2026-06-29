/// Reverb preset for a scene or individual track.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReverbConfig {
    /// Echo delay in milliseconds (e.g. 30 for small room, 150 for hall)
    pub delay_ms: u64,
    /// Reverb amplitude multiplier [0.0, 1.0]
    pub amplitude: f32,
}

impl ReverbConfig {
    pub const DRY: Self = Self { delay_ms: 0, amplitude: 0.0 };
    pub const SMALL_ROOM: Self = Self { delay_ms: 30, amplitude: 0.3 };
    pub const MEDIUM_ROOM: Self = Self { delay_ms: 60, amplitude: 0.4 };
    pub const LARGE_HALL: Self = Self { delay_ms: 150, amplitude: 0.6 };
    pub const OUTDOOR: Self = Self { delay_ms: 80, amplitude: 0.2 };
}

/// Minimal placeholder for stereo positioning
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum StereoPosition {
    Center,
    Left,
    Right,
}
