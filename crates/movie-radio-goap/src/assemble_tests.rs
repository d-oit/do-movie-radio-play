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

fn streamed(
    assembler: &RadioPlayAssembler,
    original: &[f32],
    narrations: &[NarrationSegment],
) -> Vec<f32> {
    let mut bytes = Vec::new();
    assembler
        .assemble_to_writer(original, narrations, &mut bytes)
        .expect("stream assembly");
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect()
}

fn varied_source(len: usize) -> Vec<f32> {
    (0..len)
        .map(|i| {
            let t = i as f32;
            (t * 0.013).sin() * 0.3 + (t * 0.0007).cos() * 0.2
        })
        .collect()
}

#[test]
fn streaming_assembly_matches_buffered_assembly() {
    let assembler = RadioPlayAssembler::new(16_000, 50, 0.3);
    let original = varied_source(200_000);
    let narrations = vec![
        make_narration(1_000, 4_000),
        make_narration(50_000, 9_000),
        make_narration(150_000, 2_500),
    ];
    let buffered = assembler
        .assemble(&original, &narrations)
        .expect("buffered");
    let streamed = streamed(&assembler, &original, &narrations);
    assert_eq!(buffered.len(), streamed.len());
    assert_eq!(buffered, streamed);
}

#[test]
fn streaming_assembly_matches_across_chunk_boundaries() {
    // Narrations deliberately straddle the 1M-sample chunk boundary so the
    // overlap arithmetic is exercised on both sides of a split.
    let assembler = RadioPlayAssembler::new(16_000, 50, 0.3);
    let boundary = ASSEMBLY_CHUNK_SAMPLES;
    let original = varied_source(boundary * 2 + 5_000);
    let narrations = vec![
        make_narration(boundary - 2_000, 4_000),
        make_narration(boundary * 2 - 500, 3_000),
    ];
    let buffered = assembler
        .assemble(&original, &narrations)
        .expect("buffered");
    let streamed = streamed(&assembler, &original, &narrations);
    assert_eq!(buffered, streamed);
}

#[test]
fn streaming_assembly_matches_with_zero_crossfade() {
    // A zero crossfade takes the branch the old inline code handled
    // separately, so it must still agree.
    let assembler = RadioPlayAssembler::new(16_000, 0, 0.3);
    let original = varied_source(20_000);
    let narrations = vec![make_narration(1_000, 3_000)];
    let buffered = assembler
        .assemble(&original, &narrations)
        .expect("buffered");
    let streamed = streamed(&assembler, &original, &narrations);
    assert_eq!(buffered, streamed);
}

#[test]
fn streaming_assembly_truncates_narration_past_end_like_buffered() {
    let assembler = RadioPlayAssembler::new(16_000, 50, 0.3);
    let original = varied_source(10_000);
    // Narration runs past the end of the source and carries more samples than
    // its declared span.
    let mut narration = make_narration(9_000, 5_000);
    narration.samples = vec![0.4; 8_000];
    let narrations = vec![narration];
    let buffered = assembler
        .assemble(&original, &narrations)
        .expect("buffered");
    let streamed = streamed(&assembler, &original, &narrations);
    assert_eq!(buffered, streamed);
}

#[test]
fn streaming_assembly_rejects_overlaps() {
    let assembler = RadioPlayAssembler::new(16_000, 50, 0.3);
    let narrations = vec![make_narration(100, 200), make_narration(150, 200)];
    let mut bytes = Vec::new();
    assert!(assembler
        .assemble_to_writer(&vec![0.0; 1_000], &narrations, &mut bytes)
        .is_err());
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
        provenance: None,
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
        evidence: crate::narrate::NarrationEvidence::default(),
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
        evidence: crate::narrate::NarrationEvidence::default(),
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
            evidence: crate::narrate::NarrationEvidence::default(),
        },
        NarrationScript {
            gap_start_ms: 2200, // Gap 2 starts at 35200 samples
            gap_end_ms: 3000,
            text: "Second".to_string(),
            emotion: movie_radio_voice::Emotion::Neutral,
            word_count: 1,
            duration_ms: 800,
            evidence: crate::narrate::NarrationEvidence::default(),
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
        evidence: crate::narrate::NarrationEvidence::default(),
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
            evidence: crate::narrate::NarrationEvidence::default(),
        },
        NarrationScript {
            gap_start_ms: 1000,
            gap_end_ms: 2000,
            text: "First".to_string(),
            emotion: movie_radio_voice::Emotion::Neutral,
            word_count: 1,
            duration_ms: 2000,
            evidence: crate::narrate::NarrationEvidence::default(),
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

fn streamed_with_sfx(
    assembler: &RadioPlayAssembler,
    original: &[f32],
    narrations: &[NarrationSegment],
    sfx: &[SfxSegment],
) -> Vec<f32> {
    let mut bytes = Vec::new();
    assembler
        .assemble_with_sfx_to_writer(original, narrations, sfx, &mut bytes)
        .expect("stream assembly with sfx");
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect()
}

fn sfx_segment(start_sample: usize, len: usize) -> SfxSegment {
    SfxSegment {
        start_sample,
        samples: (0..len).map(|i| (i as f32 * 0.02).sin() * 0.25).collect(),
        provenance: None,
    }
}

#[test]
fn streaming_sfx_mix_matches_buffered_sfx_mix() {
    let assembler = RadioPlayAssembler::new(16_000, 50, 0.3);
    let original = varied_source(120_000);
    let narrations = vec![make_narration(1_000, 4_000), make_narration(60_000, 9_000)];
    let sfx = vec![sfx_segment(2_000, 6_000), sfx_segment(70_000, 3_000)];

    let buffered = assembler
        .assemble_with_sfx(&original, &narrations, &sfx)
        .expect("buffered");
    let streamed = streamed_with_sfx(&assembler, &original, &narrations, &sfx);
    assert_eq!(buffered.len(), streamed.len());
    assert_eq!(buffered, streamed);
}

#[test]
fn streaming_sfx_mix_matches_when_normalization_clips() {
    // A loud source drives the summed peak above 1.0, so the normalization
    // branch runs on both paths.
    let assembler = RadioPlayAssembler::new(16_000, 50, 0.3);
    let mut original = varied_source(40_000);
    for sample in &mut original {
        *sample *= 4.0;
    }
    let narrations = vec![make_narration(500, 3_000)];
    let sfx = vec![sfx_segment(1_000, 5_000)];

    let buffered = assembler
        .assemble_with_sfx(&original, &narrations, &sfx)
        .expect("buffered");
    let streamed = streamed_with_sfx(&assembler, &original, &narrations, &sfx);
    assert_eq!(buffered, streamed);
    assert!(
        buffered.iter().any(|s| s.abs() > 1.0),
        "fixture should be loud enough to need normalization"
    );
}

#[test]
fn streaming_sfx_mix_matches_across_chunk_boundaries() {
    let assembler = RadioPlayAssembler::new(16_000, 50, 0.3);
    let boundary = ASSEMBLY_CHUNK_SAMPLES;
    let original = varied_source(boundary + 20_000);
    let narrations = vec![make_narration(boundary - 1_000, 5_000)];
    let sfx = vec![
        sfx_segment(boundary - 3_000, 8_000),
        sfx_segment(1_000, 2_000),
    ];

    let buffered = assembler
        .assemble_with_sfx(&original, &narrations, &sfx)
        .expect("buffered");
    let streamed = streamed_with_sfx(&assembler, &original, &narrations, &sfx);
    assert_eq!(buffered, streamed);
}

#[test]
fn streaming_sfx_mix_without_effects_matches_ducked_mix() {
    // `assemble_with_sfx` skips AGC, fold and normalization when there are no
    // effects; the streaming path must do the same.
    let assembler = RadioPlayAssembler::new(16_000, 50, 0.3);
    let original = varied_source(30_000);
    let narrations = vec![make_narration(1_000, 3_000)];

    let buffered = assembler
        .assemble_with_sfx(&original, &narrations, &[])
        .expect("buffered");
    let mixed = streamed_with_sfx(&assembler, &original, &narrations, &[]);
    assert_eq!(buffered, mixed);
    assert_eq!(mixed, streamed(&assembler, &original, &narrations));
}
