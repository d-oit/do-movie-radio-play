#[derive(Debug, Clone, Copy)]
pub struct FeatureSet {
    pub rms: f32,
    pub zcr: f32,
    pub spectral_flux: f32,
    pub spectral_flatness: f32,
    pub spectral_entropy: f32,
    pub centroid_hz: f32,
    pub low_band_ratio: f32,
    pub high_band_ratio: f32,
}
