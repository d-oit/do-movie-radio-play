//! Reports throughput for the radio-play assembly stage.
//!
//! Peak RSS is a process high-water mark, so it cannot be compared across
//! durations inside one process. This binary therefore assembles a single
//! duration per run and prints one JSON line; `scripts/benchmark-memory.sh`
//! invokes it once per duration under GNU `time` and merges the measured
//! peak RSS into the result.
//!
//! Usage: assembly_mem <duration_seconds> [sample_rate]

use std::time::Instant;

use movie_radio_goap::assemble::{NarrationSegment, RadioPlayAssembler};

fn main() {
    let mut args = std::env::args().skip(1);
    let duration_secs: u64 = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(60);
    let sample_rate: u32 = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(16_000);

    let total_samples = duration_secs as usize * sample_rate as usize;
    // A quiet bed with periodic louder passages, so the AGC and ducking paths
    // both do real work instead of short-circuiting on silence.
    let original: Vec<f32> = (0..total_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            let envelope = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * 0.1 * t).sin();
            0.2 * envelope * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
        })
        .collect();

    // One narration every 20 s, each 2 s long, placed inside a gap.
    let narrations: Vec<NarrationSegment> = (0..duration_secs / 20)
        .map(|index| {
            let start_sample = (index as usize * 20 + 5) * sample_rate as usize;
            let len = 2 * sample_rate as usize;
            let samples = (0..len)
                .map(|i| {
                    let t = i as f32 / sample_rate as f32;
                    0.3 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
                })
                .collect();
            NarrationSegment {
                start_sample,
                end_sample: start_sample + len,
                samples,
            }
        })
        .collect();

    let assembler = RadioPlayAssembler::new(sample_rate, 50, 0.3)
        .with_time_stretch(true, 0)
        .with_dialogue_safety(250, 1.25);

    let started = Instant::now();
    let assembled = assembler
        .assemble_with_sfx(&original, &narrations, &[])
        .expect("assemble");
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let audio_seconds = assembled.len() as f64 / sample_rate as f64;
    let throughput = if elapsed_ms == 0 {
        0.0
    } else {
        audio_seconds / (elapsed_ms as f64 / 1000.0)
    };

    println!(
        "{{\"duration_s\":{duration_secs},\"sample_rate\":{sample_rate},\
         \"samples\":{},\"elapsed_ms\":{elapsed_ms},\
         \"throughput_x_realtime\":{throughput:.2}}}",
        assembled.len()
    );
}
