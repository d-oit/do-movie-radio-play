use criterion::{criterion_group, criterion_main, Criterion};
use movie_radio_render::mixer::{render_mix, TrackInput};
use movie_radio_render::spatial::StereoPosition;

fn bench_render_mix(c: &mut Criterion) {
    let sample_rate = 48000;
    let duration_secs = 5;
    let num_samples = sample_rate * duration_secs;
    let samples = vec![0.1f32; num_samples as usize];

    let tracks = vec![
        TrackInput {
            samples: samples.clone(),
            sample_rate,
            position: StereoPosition::LEFT,
            reverb: None,
            agc_attack: 0.01,
            agc_release: 0.1,
            agc_max_gain: 20.0,
        },
        TrackInput {
            samples: samples.clone(),
            sample_rate,
            position: StereoPosition::RIGHT,
            reverb: None,
            agc_attack: 0.01,
            agc_release: 0.1,
            agc_max_gain: 20.0,
        },
        TrackInput {
            samples: samples.clone(),
            sample_rate,
            position: StereoPosition::CENTRE,
            reverb: None,
            agc_attack: 0.01,
            agc_release: 0.1,
            agc_max_gain: 20.0,
        },
    ];

    c.bench_function("render_mix_3_tracks_5s", |b| {
        b.iter(|| render_mix(std::hint::black_box(tracks.clone())))
    });
}

criterion_group!(benches, bench_render_mix);
criterion_main!(benches);
