use super::*;

fn make_narration(start_sample: usize, len: usize) -> NarrationSegment {
    NarrationSegment {
        start_sample,
        end_sample: start_sample + len,
        samples: vec![0.5; len],
    }
}

#[test]
fn test_assemble_empty_narrations() {
    let assembler = RadioPlayAssembler::new(16000, 50, 0.3);
    let original = vec![0.1; 16000];
    let result = assembler.assemble(&original, &[]).unwrap();
    assert_eq!(result, original);
}

#[test]
fn test_no_overlap_validation() {
    let assembler = RadioPlayAssembler::new(16000, 50, 0.3);
    let narrations = vec![make_narration(100, 200), make_narration(150, 200)];
    assert!(assembler.assemble(&vec![0.0; 1000], &narrations).is_err());
}

#[test]
fn test_assemble_basic() {
    let assembler = RadioPlayAssembler::new(16000, 50, 0.3);
    let original = vec![0.1; 16000];
    let narrations = vec![make_narration(1000, 500)];
    let result = assembler.assemble(&original, &narrations).unwrap();
    assert_eq!(result.len(), original.len());
}

#[test]
fn test_assemble_with_sfx() {
    let assembler = RadioPlayAssembler::new(16000, 50, 0.3);
    let original = vec![0.1; 16000];
    let sfx = vec![SfxSegment {
        start_sample: 2000,
        samples: vec![0.3; 1000],
    }];
    let result = assembler
        .assemble_with_sfx(&original, &[], &sfx)
        .expect("assemble with sfx");
    assert_eq!(result.len(), original.len());
}

#[test]
fn test_time_stretch_fits_natively() {
    let assembler = RadioPlayAssembler::new(16000, 50, 0.3);
    let script = NarrationScript {
        gap_start_ms: 1000,
        gap_end_ms: 3000, // 2000 ms gap = 32000 samples
        text: "Hello".to_string(),
        emotion: movie_radio_voice::Emotion::Neutral,
        word_count: 1,
        duration_ms: 1000,
    };
    let audio_samples = vec![0.2; 16000]; // 1000 ms audio = 16000 samples
    let segment = assembler.narration_to_segment(&script, &audio_samples);
    assert_eq!(segment.samples.len(), 16000);
    assert_eq!(segment.start_sample, 16000);
    assert_eq!(segment.end_sample, 32000);
}

#[test]
fn test_time_stretch_expands_overlong_narration() {
    let assembler = RadioPlayAssembler::new(16000, 50, 0.3);
    let script = NarrationScript {
        gap_start_ms: 1000,
        gap_end_ms: 2000, // 1000 ms gap = 16000 samples
        text: "Overlong narration text".to_string(),
        emotion: movie_radio_voice::Emotion::Neutral,
        word_count: 3,
        duration_ms: 3000,
    };
    let audio_samples = vec![0.2; 48000]; // 3000 ms audio = 48000 samples
    let segment = assembler.narration_to_segment(&script, &audio_samples);

    // Max expansion is 500 ms (8000 samples). So target_samples = 16000 + 8000 = 24000 samples.
    assert!(segment.samples.len() <= 24000);
    assert_eq!(
        segment.end_sample,
        segment.start_sample + segment.samples.len()
    );
}

#[test]
fn test_time_stretch_bounded_by_next_gap_start() {
    let assembler = RadioPlayAssembler::new(16000, 50, 0.3);
    let scripts = vec![
        NarrationScript {
            gap_start_ms: 1000,
            gap_end_ms: 2000, // Gap 1: 16000 to 32000 samples
            text: "First".to_string(),
            emotion: movie_radio_voice::Emotion::Neutral,
            word_count: 1,
            duration_ms: 2000,
        },
        NarrationScript {
            gap_start_ms: 2200, // Gap 2 starts at 35200 samples
            gap_end_ms: 3000,
            text: "Second".to_string(),
            emotion: movie_radio_voice::Emotion::Neutral,
            word_count: 1,
            duration_ms: 800,
        },
    ];
    let narration_audio = vec![
        Some(movie_radio_voice::AudioOutput {
            samples: vec![0.2; 64000], // 4000 ms audio
            sample_rate_hz: 16000,
        }),
        Some(movie_radio_voice::AudioOutput {
            samples: vec![0.3; 8000],
            sample_rate_hz: 16000,
        }),
    ];

    let segments = assembler.build_narration_segments(&scripts, &narration_audio);
    assert_eq!(segments.len(), 2);
    // Segment 0 must not cross Gap 2 start sample (35200).
    assert!(segments[0].end_sample <= segments[1].start_sample);

    // Validate that assemble succeeds without 0% overlap error
    let original = vec![0.1; 64000];
    let assembled = assembler.assemble(&original, &segments);
    assert!(
        assembled.is_ok(),
        "Assembly must succeed with 0% overlap: {:?}",
        assembled.err()
    );
}

#[test]
fn test_time_stretch_disabled() {
    let assembler = RadioPlayAssembler::new(16000, 50, 0.3).with_time_stretch(false, 500);
    let script = NarrationScript {
        gap_start_ms: 1000,
        gap_end_ms: 2000, // 1000 ms gap = 16000 samples
        text: "Overlong".to_string(),
        emotion: movie_radio_voice::Emotion::Neutral,
        word_count: 1,
        duration_ms: 3000,
    };
    let audio_samples = vec![0.2; 48000]; // 3000 ms audio = 48000 samples
    let segment = assembler.narration_to_segment(&script, &audio_samples);
    assert_eq!(segment.samples.len(), 48000);
}

#[test]
fn test_time_stretch_with_out_of_order_scripts() {
    let assembler = RadioPlayAssembler::new(16000, 50, 0.3);
    let scripts = vec![
        NarrationScript {
            gap_start_ms: 2200,
            gap_end_ms: 3000,
            text: "Second".to_string(),
            emotion: movie_radio_voice::Emotion::Neutral,
            word_count: 1,
            duration_ms: 800,
        },
        NarrationScript {
            gap_start_ms: 1000,
            gap_end_ms: 2000,
            text: "First".to_string(),
            emotion: movie_radio_voice::Emotion::Neutral,
            word_count: 1,
            duration_ms: 2000,
        },
    ];
    let narration_audio = vec![
        Some(movie_radio_voice::AudioOutput {
            samples: vec![0.3; 8000],
            sample_rate_hz: 16000,
        }),
        Some(movie_radio_voice::AudioOutput {
            samples: vec![0.2; 64000],
            sample_rate_hz: 16000,
        }),
    ];

    let segments = assembler.build_narration_segments(&scripts, &narration_audio);
    assert_eq!(segments.len(), 2);
    assert!(segments[0].start_sample < segments[1].start_sample);
    assert!(segments[0].end_sample <= segments[1].start_sample);
}
