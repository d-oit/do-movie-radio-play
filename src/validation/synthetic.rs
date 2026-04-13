use anyhow::Result;
use std::f32::consts::PI;
use std::path::Path;

use crate::io::json::write_json_pretty;
use crate::types::{Segment, SegmentKind, TimelineOutput};

const SR: u32 = 16_000;

pub fn generate_suite(output_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(output_dir)?;
    let cases = [
        fixture_silence_only(),
        fixture_speech_only(),
        fixture_alternating(),
        fixture_speech_noise(),
        fixture_speech_music_like(),
        fixture_impulse_heavy(),
        fixture_short_bursts(),
        fixture_long_ambience(),
    ];

    for case in cases {
        let wav_path = output_dir.join(format!("{}.wav", case.name));
        let json_path = output_dir.join(format!("{}.truth.json", case.name));
        write_wav(&wav_path, &case.samples)?;
        write_json_pretty(&json_path, &case.truth)?;
    }
    Ok(())
}

struct FixtureCase {
    name: &'static str,
    samples: Vec<f32>,
    truth: TimelineOutput,
}

fn fixture_silence_only() -> FixtureCase {
    let ms = 5_000;
    let samples = vec![0.0; ms_to_len(ms)];
    let truth = timeline(
        "silence_only.wav",
        vec![segment(0, ms as u64, SegmentKind::NonVoice)],
    );
    FixtureCase {
        name: "silence_only",
        samples,
        truth,
    }
}

fn fixture_speech_only() -> FixtureCase {
    let ms = 5_000;
    let samples = tone(220.0, ms, 0.25);
    let truth = timeline(
        "speech_only.wav",
        vec![segment(0, ms as u64, SegmentKind::Speech)],
    );
    FixtureCase {
        name: "speech_only",
        samples,
        truth,
    }
}

fn fixture_alternating() -> FixtureCase {
    let a = vec![0.0; ms_to_len(2_000)];
    let b = tone(220.0, 2_200, 0.3);
    let c = vec![0.0; ms_to_len(1_800)];
    let mut samples = a;
    samples.extend(b);
    samples.extend(c);
    let truth = timeline(
        "alternating.wav",
        vec![
            segment(0, 2000, SegmentKind::NonVoice),
            segment(2000, 4200, SegmentKind::Speech),
            segment(4200, 6000, SegmentKind::NonVoice),
        ],
    );
    FixtureCase {
        name: "alternating",
        samples,
        truth,
    }
}

fn fixture_speech_noise() -> FixtureCase {
    let mut samples = tone(250.0, 2_500, 0.22);
    samples.extend(noise(2_500, 13));
    let truth = timeline(
        "speech_noise.wav",
        vec![
            segment(0, 2500, SegmentKind::Speech),
            segment(2500, 5000, SegmentKind::NonVoice),
        ],
    );
    FixtureCase {
        name: "speech_noise",
        samples,
        truth,
    }
}

fn fixture_speech_music_like() -> FixtureCase {
    let mut samples = tone(240.0, 2_500, 0.25);
    samples.extend(music_like(2_500));
    let truth = timeline(
        "speech_music_like.wav",
        vec![
            segment(0, 2500, SegmentKind::Speech),
            segment(2500, 5000, SegmentKind::NonVoice),
        ],
    );
    FixtureCase {
        name: "speech_music_like",
        samples,
        truth,
    }
}

fn fixture_impulse_heavy() -> FixtureCase {
    let mut samples = vec![0.0; ms_to_len(5000)];
    for idx in (0..samples.len()).step_by(1500) {
        samples[idx] = 0.95;
    }
    let truth = timeline(
        "impulse_heavy.wav",
        vec![segment(0, 5000, SegmentKind::NonVoice)],
    );
    FixtureCase {
        name: "impulse_heavy",
        samples,
        truth,
    }
}

fn fixture_short_bursts() -> FixtureCase {
    let mut samples = vec![0.0; ms_to_len(5000)];
    for start_ms in [800, 1600, 2500, 3300] {
        let burst = tone(300.0, 120, 0.3);
        let start = ms_to_len(start_ms);
        let end = (start + burst.len()).min(samples.len());
        samples[start..end].copy_from_slice(&burst[..(end - start)]);
    }
    let truth = timeline(
        "short_bursts.wav",
        vec![segment(0, 5000, SegmentKind::NonVoice)],
    );
    FixtureCase {
        name: "short_bursts",
        samples,
        truth,
    }
}

fn fixture_long_ambience() -> FixtureCase {
    let samples = noise(12_000, 42).into_iter().map(|v| v * 0.02).collect();
    let truth = timeline(
        "long_ambience.wav",
        vec![segment(0, 12_000, SegmentKind::NonVoice)],
    );
    FixtureCase {
        name: "long_ambience",
        samples,
        truth,
    }
}

fn timeline(file: &str, segments: Vec<Segment>) -> TimelineOutput {
    TimelineOutput {
        file: file.to_string(),
        analysis_sample_rate: SR,
        frame_ms: 20,
        segments,
    }
}

fn segment(start_ms: u64, end_ms: u64, kind: SegmentKind) -> Segment {
    Segment {
        start_ms,
        end_ms,
        kind,
        confidence: 1.0,
        tags: vec![],
        prompt: None,
    }
}

fn tone(freq: f32, dur_ms: usize, amp: f32) -> Vec<f32> {
    (0..ms_to_len(dur_ms))
        .map(|i| {
            let t = i as f32 / SR as f32;
            (2.0 * PI * freq * t).sin() * amp
        })
        .collect()
}

fn music_like(dur_ms: usize) -> Vec<f32> {
    (0..ms_to_len(dur_ms))
        .map(|i| {
            let t = i as f32 / SR as f32;
            ((2.0 * PI * 220.0 * t).sin() * 0.08) + ((2.0 * PI * 440.0 * t).sin() * 0.04)
        })
        .collect()
}

fn noise(dur_ms: usize, seed: u32) -> Vec<f32> {
    let mut x = seed;
    (0..ms_to_len(dur_ms))
        .map(|_| {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
            ((x >> 8) as f32 / u32::MAX as f32 - 0.5) * 0.2
        })
        .collect()
}

fn ms_to_len(ms: usize) -> usize {
    ms * SR as usize / 1000
}

fn write_wav(path: &Path, samples: &[f32]) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SR,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer.write_sample(v)?;
    }
    writer.finalize()?;
    Ok(())
}
