#![allow(clippy::needless_range_loop)]

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use movie_radio_render::mixer::{render_mix, Mixer, TrackInput};
use movie_radio_render::noise::{generate_noise_samples, NoiseTrackConfig, NoiseType};
use movie_radio_render::spatial::{ReverbConfig, StereoPosition};

fn bench_render_mix_reusable(c: &mut Criterion) {
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

    let mut group = c.benchmark_group("render_reusable");
    group.throughput(Throughput::Elements(num_samples as u64));

    let mut mixer = Mixer::new();
    group.bench_function("Mixer_render_mix_reusing_buffers_3_tracks_5s", |b| {
        b.iter(|| {
            let _ = mixer
                .render_mix(std::hint::black_box(tracks.clone()))
                .unwrap();
        })
    });

    group.finish();
}

fn bench_focused_stereo_render(c: &mut Criterion) {
    let sample_rate = 48000;
    let duration_secs = 1;
    let num_samples = sample_rate * duration_secs;
    let samples = vec![0.15f32; num_samples as usize];

    let mut group = c.benchmark_group("focused_stereo_render");

    // Varying number of tracks: 2, 4, 8 tracks
    for num_tracks in [2, 4, 8] {
        let mut tracks = Vec::new();
        for i in 0..num_tracks {
            // Varying positions: LEFT, RIGHT, CENTRE, HARD_LEFT, HARD_RIGHT
            let position = match i % 5 {
                0 => StereoPosition::LEFT,
                1 => StereoPosition::RIGHT,
                2 => StereoPosition::CENTRE,
                3 => StereoPosition::HARD_LEFT,
                _ => StereoPosition::HARD_RIGHT,
            };

            tracks.push(TrackInput {
                samples: samples.clone(),
                sample_rate,
                position,
                reverb: None,
                agc_attack: 0.01,
                agc_release: 0.1,
                agc_max_gain: 20.0,
            });
        }

        group.throughput(Throughput::Elements((num_samples * num_tracks) as u64));
        group.bench_function(
            format!(
                "render_mix_varying_tracks_{}_samples_{}",
                num_tracks, num_samples
            ),
            |b| b.iter(|| render_mix(std::hint::black_box(tracks.clone()))),
        );
    }

    group.finish();
}

fn bench_complex_radio_play(c: &mut Criterion) {
    let sample_rate: u32 = 48000;
    let duration_secs: u64 = 5;
    let num_samples = (sample_rate as u64 * duration_secs) as usize;

    // 1. Ambient noise background track (White noise, volume 0.15)
    let ambient_cfg = NoiseTrackConfig {
        noise_type: NoiseType::Pink,
        duration_ms: duration_secs * 1000,
        sample_rate,
        volume: 0.15,
        low_pass_hz: Some(8000),
    };
    let ambient_samples = generate_noise_samples(&ambient_cfg).unwrap();

    // 2. Simulated music track (sine wave melody)
    let mut music_samples = vec![0.0f32; num_samples];
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        // Two sine waves mixed (melody + harmony)
        music_samples[i] = ((t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.2)
            + ((t * 554.37 * 2.0 * std::f32::consts::PI).sin() * 0.1);
    }

    // 3. Simulated dialogue segments (intermittent speech bursts)
    let mut dialogue_samples = vec![0.0f32; num_samples];
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        // Periodic voice-like signals
        if (1.0..2.5).contains(&t) || (3.5..4.8).contains(&t) {
            dialogue_samples[i] = (t * 800.0 * 2.0 * std::f32::consts::PI).sin() * 0.4;
        }
    }

    let tracks = vec![
        TrackInput {
            samples: ambient_samples,
            sample_rate,
            position: StereoPosition::CENTRE,
            reverb: Some(ReverbConfig::LARGE_HALL),
            agc_attack: 0.05,
            agc_release: 0.5,
            agc_max_gain: 10.0,
        },
        TrackInput {
            samples: music_samples,
            sample_rate,
            position: StereoPosition::LEFT,
            reverb: Some(ReverbConfig::MEDIUM_ROOM),
            agc_attack: 0.01,
            agc_release: 0.2,
            agc_max_gain: 15.0,
        },
        TrackInput {
            samples: dialogue_samples,
            sample_rate,
            position: StereoPosition::RIGHT,
            reverb: None,
            agc_attack: 0.01,
            agc_release: 0.1,
            agc_max_gain: 20.0,
        },
    ];

    let mut group = c.benchmark_group("complex_radio_play");
    group.throughput(Throughput::Elements((num_samples * 3) as u64));

    let mut mixer = Mixer::new();
    group.bench_function("Mixer_render_complex_radio_play_scenario_5s", |b| {
        b.iter(|| {
            let _ = mixer
                .render_mix(std::hint::black_box(tracks.clone()))
                .unwrap();
        })
    });

    group.bench_function("render_mix_complex_radio_play_scenario_5s", |b| {
        b.iter(|| {
            let _ = render_mix(std::hint::black_box(tracks.clone())).unwrap();
        })
    });

    group.finish();
}

fn bench_render_mix_legacy(c: &mut Criterion) {
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

    let mut group = c.benchmark_group("render_legacy");
    group.throughput(Throughput::Elements(num_samples as u64));

    group.bench_function("render_mix_3_tracks_5s", |b| {
        b.iter(|| render_mix(std::hint::black_box(tracks.clone())))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_render_mix_reusable,
    bench_focused_stereo_render,
    bench_complex_radio_play,
    bench_render_mix_legacy
);
criterion_main!(benches);
