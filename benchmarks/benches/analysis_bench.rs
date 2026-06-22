use criterion::{criterion_group, criterion_main, Criterion};
use movie_radio_verification::verification::analysis::analyze_audio_features;
use std::hint::black_box;

fn bench_analysis(c: &mut Criterion) {
    let mut samples = vec![0.0f32; 32000]; // 2 seconds at 16kHz
    for (i, s) in samples.iter_mut().enumerate() {
        *s = (i as f32 * 0.1).sin();
    }

    c.bench_function("analyze_audio_features_2s", |b| {
        b.iter(|| analyze_audio_features(black_box(&samples)))
    });
}

criterion_group!(benches, bench_analysis);
criterion_main!(benches);
