//! Reports throughput for the radio-play assembly stage.
//!
//! Peak RSS is a process high-water mark, so it cannot be compared across
//! durations inside one process. This binary therefore assembles a single
//! duration per run and prints one JSON line; `scripts/benchmark-memory.sh`
//! invokes it once per duration under GNU `time` and merges the measured
//! peak RSS into the result.
//!
//! Usage: assembly_mem <duration_seconds> [sample_rate] [--mode buffered|streaming]

use std::time::Instant;

use clap::Parser;
use movie_radio_goap::assemble::{NarrationSegment, RadioPlayAssembler};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum Mode {
    /// Hold the whole mix in memory (the original implementation).
    Buffered,
    /// Stream the mix to a file, as the pipeline does now.
    Streaming,
}

#[derive(Debug, Parser)]
#[command(about = "Measure assembly peak memory and throughput for one duration")]
struct Args {
    /// Length of the synthetic film to assemble, in seconds.
    #[arg(default_value_t = 60)]
    duration_seconds: u64,
    /// Analysis sample rate in Hz.
    #[arg(default_value_t = 16_000)]
    sample_rate: u32,
    /// Assembly strategy to measure.
    #[arg(long, value_enum, default_value_t = Mode::Streaming)]
    mode: Mode,
}

fn main() {
    let Args {
        duration_seconds: duration_secs,
        sample_rate,
        mode,
    } = Args::parse();

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
    let samples_written = match mode {
        Mode::Buffered => assembler
            .assemble_with_sfx(&original, &narrations, &[])
            .expect("assemble")
            .len(),
        Mode::Streaming => {
            let mut sink = std::io::BufWriter::new(std::io::sink());
            assembler
                .assemble_with_sfx_to_writer(&original, &narrations, &[], &mut sink)
                .expect("stream assemble");
            total_samples
        }
    };
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let audio_seconds = samples_written as f64 / sample_rate as f64;
    let throughput = if elapsed_ms == 0 {
        0.0
    } else {
        audio_seconds / (elapsed_ms as f64 / 1000.0)
    };

    println!(
        "{{\"duration_s\":{duration_secs},\"sample_rate\":{sample_rate},\
         \"mode\":\"{mode:?}\",\"samples\":{samples_written},\"elapsed_ms\":{elapsed_ms},\
         \"throughput_x_realtime\":{throughput:.2}}}"
    );
}
