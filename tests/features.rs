use movie_nonvoice_timeline::pipeline::features::compute_features;
use rand::{rngs::StdRng, RngExt, SeedableRng};
use std::f32::consts::PI;

fn generate_sine(freq_hz: f32, sample_rate: u32, duration_samples: usize) -> Vec<f32> {
    (0..duration_samples)
        .map(|i| (2.0 * PI * freq_hz * i as f32 / sample_rate as f32).sin())
        .collect()
}

#[test]
fn known_frequency_signal_1khz() {
    let sample_rate = 16000;
    let samples = generate_sine(1000.0, sample_rate, 4096);
    let features = compute_features(&samples, sample_rate);

    assert!(
        features.centroid_hz > 500.0,
        "1kHz signal should have centroid above 500Hz, got {}",
        features.centroid_hz
    );
    assert!(
        features.centroid_hz < 2000.0,
        "1kHz signal should have centroid below 2kHz, got {}",
        features.centroid_hz
    );
}

#[test]
fn known_frequency_signal_4khz() {
    let sample_rate = 16000;
    let samples = generate_sine(4000.0, sample_rate, 4096);
    let features = compute_features(&samples, sample_rate);

    assert!(
        features.centroid_hz > 3000.0,
        "High freq signal should have high centroid, got {}",
        features.centroid_hz
    );
}

#[test]
fn noise_floor_detection() {
    let mut rng = StdRng::seed_from_u64(123);
    let samples: Vec<f32> = (0..4096).map(|_| rng.random_range(-0.01..0.01)).collect();
    let features = compute_features(&samples, 16000);

    assert!(
        features.high_band_ratio < 0.8,
        "Noise should spread across bands, high_ratio={}",
        features.high_band_ratio
    );
}

#[test]
fn empty_input_edge_case() {
    let features = compute_features(&[], 16000);
    assert_eq!(features.rms, 0.0);
    assert_eq!(features.zcr, 0.0);
    assert_eq!(features.spectral_flux, 0.0);
    assert_eq!(features.centroid_hz, 0.0);
    assert_eq!(features.low_band_ratio, 0.0);
    assert_eq!(features.high_band_ratio, 0.0);
}

#[test]
fn too_short_input_edge_case() {
    let samples = vec![0.1f32; 8];
    let features = compute_features(&samples, 16000);
    assert!(
        features.rms > 0.0,
        "RMS should be computed even for short input"
    );
    assert!(features.zcr >= 0.0);
}

#[test]
fn dc_offset_has_more_low_energy() {
    let samples = vec![0.5f32; 2048];
    let features = compute_features(&samples, 16000);
    assert!(
        features.high_band_ratio < features.low_band_ratio,
        "DC should have more low-band energy, low={} high={}",
        features.low_band_ratio,
        features.high_band_ratio
    );
}

#[test]
fn silent_signal_has_zero_flux() {
    let samples = vec![0.0f32; 2048];
    let features = compute_features(&samples, 16000);
    assert_eq!(
        features.spectral_flux, 0.0,
        "Silent signal should have zero spectral flux"
    );
}

#[test]
fn amplitude_affects_rms() {
    let quiet = vec![0.01f32; 1024];
    let loud = vec![0.5f32; 1024];
    let quiet_features = compute_features(&quiet, 16000);
    let loud_features = compute_features(&loud, 16000);
    assert!(
        loud_features.rms > quiet_features.rms,
        "Louder signal should have higher RMS"
    );
}

#[test]
fn speech_has_lower_entropy_than_noise() {
    let mut rng = StdRng::seed_from_u64(42);
    let speech_samples: Vec<f32> = (0..4096)
        .map(|i| {
            (2.0 * PI * 200.0 * i as f32 / 16000.0).sin() * 0.3
                + (2.0 * PI * 800.0 * i as f32 / 16000.0).sin() * 0.2
                + rng.random_range(-0.02..0.02)
        })
        .collect();
    let noise_samples: Vec<f32> = (0..4096).map(|_| rng.random_range(-0.3..0.3)).collect();

    let speech_features = compute_features(&speech_samples, 16000);
    let noise_features = compute_features(&noise_samples, 16000);

    assert!(
        speech_features.spectral_entropy < noise_features.spectral_entropy,
        "Speech-like (tonal) signal should have lower entropy than noise, speech={:.2} noise={:.2}",
        speech_features.spectral_entropy,
        noise_features.spectral_entropy
    );
}

#[test]
fn spectral_entropy_is_computed() {
    let samples = vec![0.1f32; 2048];
    let features = compute_features(&samples, 16000);
    assert!(
        features.spectral_entropy >= 0.0,
        "Spectral entropy should be non-negative, got {}",
        features.spectral_entropy
    );
    assert!(
        features.spectral_entropy <= 10.0,
        "Spectral entropy should be bounded, got {}",
        features.spectral_entropy
    );
}
