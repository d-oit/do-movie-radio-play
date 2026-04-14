use criterion::{black_box, criterion_group, criterion_main, Criterion};
use movie_nonvoice_timeline::pipeline::{
    decode,
    features::compute_features,
    framing, resample, segmenter,
    vad::{EnergyVad, VadEngine},
};
use std::{path::Path, sync::OnceLock, time::Duration};

const BENCH_SAMPLE_RATE_HZ: u32 = 16000;
const BENCH_FRAME_MS: u32 = 20;
const MAX_BENCH_SECONDS: usize = 10;
const MAX_BENCH_SAMPLES: usize = BENCH_SAMPLE_RATE_HZ as usize * MAX_BENCH_SECONDS;

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

fn preferred_media_path() -> Option<&'static str> {
    [
        "testdata/raw/sintel_trailer_2010.mp4",
        "testdata/raw/big_buck_bunny_trailer_2008.mov",
        "testdata/raw/elephants_dream_2006.mp4",
        "testdata/raw/eggs_1970.mp4",
        "testdata/raw/windy_day_1967.mp4",
        "testdata/raw/the_hole_1962.mp4",
        "testdata/raw/dinner_time_1928.webm",
        "testdata/raw/the_singing_fool_1928.webm",
    ]
    .into_iter()
    .find(|path| Path::new(path).exists())
}

fn load_bench_samples() -> Vec<f32> {
    let Some(path) = preferred_media_path() else {
        return sample_audio();
    };

    let Ok((samples, source_rate)) = decode::decode_audio(Path::new(path)) else {
        return sample_audio();
    };

    let mut mono = resample::resample_linear(&samples, source_rate, BENCH_SAMPLE_RATE_HZ);
    mono.truncate(mono.len().min(MAX_BENCH_SAMPLES));
    if mono.is_empty() {
        sample_audio()
    } else {
        mono
    }
}

fn bench_samples() -> &'static [f32] {
    static SAMPLES: OnceLock<Vec<f32>> = OnceLock::new();
    SAMPLES.get_or_init(load_bench_samples).as_slice()
}

fn bench_framing(c: &mut Criterion) {
    let samples = bench_samples();
    c.bench_function("framing", |b| {
        b.iter(|| framing::build_frames(black_box(samples), BENCH_SAMPLE_RATE_HZ, BENCH_FRAME_MS))
    });
}

fn bench_features(c: &mut Criterion) {
    let samples = bench_samples();
    c.bench_function("feature_extraction", |b| {
        b.iter(|| compute_features(black_box(samples), BENCH_SAMPLE_RATE_HZ))
    });
}

fn bench_vad(c: &mut Criterion) {
    let frames = framing::build_frames(bench_samples(), BENCH_SAMPLE_RATE_HZ, BENCH_FRAME_MS);
    let vad = EnergyVad::new(0.015);
    c.bench_function("energy_vad", |b| {
        b.iter(|| vad.classify(black_box(&frames)))
    });
}

fn bench_segmenter(c: &mut Criterion) {
    let frames = framing::build_frames(bench_samples(), BENCH_SAMPLE_RATE_HZ, BENCH_FRAME_MS);
    let vad = EnergyVad::new(0.015);
    let result = vad.classify(&frames);
    let smoothed = segmenter::smooth_speech(&result.decisions, BENCH_FRAME_MS, 300);
    c.bench_function("speech_segments", |b| {
        b.iter(|| segmenter::speech_segments(&smoothed, BENCH_FRAME_MS, 120, &result.likelihoods))
    });
    let speech_segments =
        segmenter::speech_segments(&smoothed, BENCH_FRAME_MS, 120, &result.likelihoods);
    c.bench_function("invert_to_non_voice", |b| {
        b.iter(|| {
            segmenter::invert_to_non_voice(
                black_box(&speech_segments),
                1000,
                250,
                BENCH_FRAME_MS,
                &result.likelihoods,
            )
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(Duration::from_secs(8));
    targets = bench_framing, bench_features, bench_vad, bench_segmenter
}
criterion_main!(benches);
