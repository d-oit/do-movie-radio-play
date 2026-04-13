#[derive(Debug, Clone, Copy)]
pub struct FeatureSet {
    pub rms: f32,
    pub zcr: f32,
}

pub fn compute_features(samples: &[f32]) -> FeatureSet {
    if samples.is_empty() {
        return FeatureSet { rms: 0.0, zcr: 0.0 };
    }
    let rms = (samples.iter().map(|v| v * v).sum::<f32>() / samples.len() as f32).sqrt();
    let mut zero_crosses = 0u32;
    for w in samples.windows(2) {
        if (w[0] >= 0.0) != (w[1] >= 0.0) {
            zero_crosses += 1;
        }
    }
    FeatureSet {
        rms,
        zcr: zero_crosses as f32 / samples.len() as f32,
    }
}
