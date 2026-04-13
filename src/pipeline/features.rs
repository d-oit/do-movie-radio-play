#[derive(Debug, Clone, Copy)]
pub struct FeatureSet {
    pub rms: f32,
    pub zcr: f32,
    pub spectral_flux: f32,
    pub centroid_hz: f32,
    pub low_band_ratio: f32,
    pub high_band_ratio: f32,
}

pub fn compute_features(samples: &[f32], sample_rate: u32) -> FeatureSet {
    if samples.is_empty() {
        return FeatureSet {
            rms: 0.0,
            zcr: 0.0,
            spectral_flux: 0.0,
            centroid_hz: 0.0,
            low_band_ratio: 0.0,
            high_band_ratio: 0.0,
        };
    }
    let rms = (samples.iter().map(|v| v * v).sum::<f32>() / samples.len() as f32).sqrt();
    let mut zero_crosses = 0u32;
    for w in samples.windows(2) {
        if (w[0] >= 0.0) != (w[1] >= 0.0) {
            zero_crosses += 1;
        }
    }
    let zcr = zero_crosses as f32 / samples.len() as f32;

    let mut flux_acc = 0.0;
    let win = 256usize;
    let mut prev_energy = 0.0;
    let mut weighted_bin_sum = 0.0;
    let mut mag_sum = 0.0;
    let mut low = 0.0;
    let mut high = 0.0;
    for chunk in samples.chunks(win) {
        let energy = chunk.iter().map(|v| v.abs()).sum::<f32>() / chunk.len() as f32;
        flux_acc += (energy - prev_energy).max(0.0);
        prev_energy = energy;

        for (i, s) in chunk.iter().enumerate() {
            let freq = i as f32 * sample_rate as f32 / win as f32;
            let mag = s.abs();
            weighted_bin_sum += freq * mag;
            mag_sum += mag;
            if freq < 300.0 {
                low += mag;
            }
            if freq > 2000.0 {
                high += mag;
            }
        }
    }

    FeatureSet {
        rms,
        zcr,
        spectral_flux: flux_acc / samples.len().max(1) as f32,
        centroid_hz: if mag_sum > 0.0 {
            weighted_bin_sum / mag_sum
        } else {
            0.0
        },
        low_band_ratio: if mag_sum > 0.0 { low / mag_sum } else { 0.0 },
        high_band_ratio: if mag_sum > 0.0 { high / mag_sum } else { 0.0 },
    }
}
