/// Stereo pan position for a character voice track.
/// -1.0 = full left, 0.0 = centre, 1.0 = full right
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct StereoPosition(pub f32);

impl StereoPosition {
    pub const CENTRE: Self = Self(0.0);
    pub const LEFT: Self = Self(-0.7);
    pub const RIGHT: Self = Self(0.7);
    pub const HARD_LEFT: Self = Self(-1.0);
    pub const HARD_RIGHT: Self = Self(1.0);

    /// Validate position is in [-1.0, 1.0]
    pub fn new(pos: f32) -> anyhow::Result<Self> {
        anyhow::ensure!(
            (-1.0..=1.0).contains(&pos),
            "position must be in [-1.0, 1.0]"
        );
        Ok(Self(pos))
    }

    /// Constant-power pan: returns (left_gain, right_gain)
    pub fn gains(self) -> (f32, f32) {
        let angle = (self.0 + 1.0) * std::f32::consts::FRAC_PI_4; // 0..pi/2
        (angle.cos(), angle.sin())
    }
}
