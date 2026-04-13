use criterion::{black_box, criterion_group, criterion_main, Criterion};
use movie_nonvoice_timeline::pipeline::{
    features::compute_features,
    framing, segmenter,
    vad::{EnergyVad, VadEngine},
};

fn sample_audio() -> Vec<f32> {
    let mut data = Vec::with_capacity(16000);
    for i in 0..16000 {
        let t = i as f32 / 16000.0;
        let speech = (2.0 * std::f32::consts::PI * 220.0 * t).sin() * 0.3;
        let noise = ((i * 17 % 37) as f32 / 50.0) - 0.3;
        data.push(if (0.25..0.55).contains(&t) {
            speech
        } else {
            noise * 0.05
        });
    }
    data
}

fn bench_framing(c: &mut Criterion) {
    let samples = sample_audio();
    c.bench_function("framing", |b| {
        b.iter(|| framing::build_frames(black_box(&samples), 16000, 20))
    });
}

fn bench_features(c: &mut Criterion) {
    let samples = sample_audio();
    c.bench_function("feature_extraction", |b| {
        b.iter(|| compute_features(black_box(&samples), 16000))
    });
}

fn bench_vad(c: &mut Criterion) {
    let frames = framing::build_frames(&sample_audio(), 16000, 20);
    let vad = EnergyVad::new(0.015);
    c.bench_function("energy_vad", |b| {
        b.iter(|| vad.classify(black_box(&frames)))
    });
}

fn bench_segmenter(c: &mut Criterion) {
    let frames = framing::build_frames(&sample_audio(), 16000, 20);
    let vad = EnergyVad::new(0.015);
    let result = vad.classify(&frames);
    let smoothed = segmenter::smooth_speech(&result.decisions, 20, 300);
    c.bench_function("speech_segments", |b| {
        b.iter(|| segmenter::speech_segments(&smoothed, 20, 120, &result.likelihoods))
    });
    let speech_segments = segmenter::speech_segments(&smoothed, 20, 120, &result.likelihoods);
    c.bench_function("invert_to_non_voice", |b| {
        b.iter(|| {
            segmenter::invert_to_non_voice(
                black_box(&speech_segments),
                1000,
                250,
                20,
                &result.likelihoods,
            )
        })
    });
}

criterion_group!(
    benches,
    bench_framing,
    bench_features,
    bench_vad,
    bench_segmenter
);
criterion_main!(benches);
