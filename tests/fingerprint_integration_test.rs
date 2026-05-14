use movie_nonvoice_timeline::learning::database::{LearningDb, SpectralFeatures, VerifiedSegment};
use movie_nonvoice_timeline::verification::fingerprint::{
    fingerprint_segment, match_fingerprints, DEFAULT_SAMPLE_RATE,
};
use tempfile::NamedTempFile;

#[tokio::test]
async fn test_fingerprint_matching_logic() {
    // 1. Generate synthetic audio with a repeated pattern
    let sample_rate = DEFAULT_SAMPLE_RATE;
    let pattern_duration_s = 1.0;
    let mut pattern = vec![0.0f32; (sample_rate * pattern_duration_s) as usize];
    for (i, s) in pattern.iter_mut().enumerate() {
        // A chord to have more peaks
        *s = 0.3 * (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / sample_rate).sin()
            + 0.3 * (i as f32 * 2.0 * std::f32::consts::PI * 660.0 / sample_rate).sin()
            + 0.3 * (i as f32 * 2.0 * std::f32::consts::PI * 880.0 / sample_rate).sin();
    }

    let mut audio = vec![0.0f32; (sample_rate * 5.0) as usize];
    // Place pattern at 1s and 3s
    let offset1 = (sample_rate * 1.0) as usize;
    let offset2 = (sample_rate * 3.0) as usize;
    audio[offset1..offset1 + pattern.len()].copy_from_slice(&pattern);
    audio[offset2..offset2 + pattern.len()].copy_from_slice(&pattern);

    // 2. Fingerprint segments
    let fps1 = fingerprint_segment(&audio[offset1..offset1 + pattern.len()], sample_rate);
    let fps2 = fingerprint_segment(&audio[offset2..offset2 + pattern.len()], sample_rate);

    assert!(!fps1.is_empty());
    assert!(!fps2.is_empty());

    // 3. Test matching logic directly
    let mut stored_fps = Vec::new();
    for fp in &fps1 {
        stored_fps.push((1, fp.hash, fp.offset_ms));
    }

    let matches = match_fingerprints(&fps2, stored_fps);
    assert!(matches.contains_key(&1));
    let score = matches[&1];
    assert!(
        score > 10,
        "Score {} should be high for identical pattern",
        score
    );
}

#[tokio::test]
async fn test_database_fingerprint_storage() {
    let temp_db = NamedTempFile::new().unwrap();
    let db = LearningDb::new(temp_db.path()).await.unwrap();

    let segment = VerifiedSegment {
        start_ms: 0,
        end_ms: 1000,
        confidence: 1.0,
        spectral_features: SpectralFeatures::default(),
        was_false_positive: false,
    };
    let seg_id = db.record_verification(segment).await.unwrap();

    let sample_rate = DEFAULT_SAMPLE_RATE;
    let mut samples = vec![0.0f32; (sample_rate * 1.0) as usize];
    for (i, s) in samples.iter_mut().enumerate() {
        *s = (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / sample_rate).sin();
    }
    let fps = fingerprint_segment(&samples, sample_rate);

    db.record_fingerprints(seg_id, &fps).await.unwrap();

    let hashes: Vec<u32> = fps.iter().map(|f| f.hash).collect();
    let stored = db.find_fingerprint_matches(&hashes).await.unwrap();

    assert!(!stored.is_empty());
    assert_eq!(stored[0].0, seg_id);
}
