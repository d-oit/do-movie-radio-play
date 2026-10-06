use crate::assemble::RadioPlayAssembler;
use crate::narrate::NarrationScript;
use movie_radio_voice::Emotion;

const SR: u32 = 16_000;

fn script(start_ms: u64, end_ms: u64) -> NarrationScript {
    NarrationScript {
        gap_start_ms: start_ms,
        gap_end_ms: end_ms,
        text: "x".into(),
        emotion: Emotion::Neutral,
        word_count: 1,
        duration_ms: 1_000,
    }
}

fn audio_ms(ms: usize) -> Vec<f32> {
    vec![0.5; ms * SR as usize / 1000]
}

fn safe() -> RadioPlayAssembler {
    RadioPlayAssembler::new(SR, 50, 0.3)
        .with_time_stretch(true, 0)
        .with_dialogue_safety(250, 1.25)
}

#[test]
fn fitting_line_is_inset_from_the_gap_start() {
    let seg = safe().narration_to_segment_ext(&script(10_000, 16_000), &audio_ms(2_000), None);
    assert_eq!(seg.start_sample, 10_250 * 16);
    assert_eq!(seg.samples.len(), 2_000 * 16);
    assert!(seg.end_sample <= 15_750 * 16);
}

#[test]
fn line_slightly_too_long_is_sped_up_to_end_before_the_gap_end() {
    // Window is 5 000 ms inset; 5 800 ms of speech needs 1.16x.
    let seg = safe().narration_to_segment_ext(&script(0, 5_500), &audio_ms(5_800), None);
    assert!(seg.end_sample <= 5_250 * 16, "end {}", seg.end_sample);
    assert!(!seg.samples.is_empty());
}

#[test]
fn line_needing_more_than_max_speedup_is_dropped() {
    let seg = safe().narration_to_segment_ext(&script(0, 4_000), &audio_ms(6_000), None);
    assert!(seg.samples.is_empty());
}

#[test]
fn defaults_keep_legacy_placement() {
    let legacy = RadioPlayAssembler::new(SR, 50, 0.3);
    let seg = legacy.narration_to_segment_ext(&script(1_000, 4_000), &audio_ms(3_300), None);
    assert_eq!(seg.start_sample, 1_000 * 16);
    assert_eq!(seg.samples.len(), 3_300 * 16);
}

#[test]
fn tiny_gap_margin_is_capped_at_a_quarter_of_the_gap() {
    let seg = safe().narration_to_segment_ext(&script(0, 800), &audio_ms(300), None);
    assert_eq!(seg.start_sample, 200 * 16);
}
