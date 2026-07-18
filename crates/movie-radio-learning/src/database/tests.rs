use super::*;
use tempfile::NamedTempFile;

fn setup_test_db_path() -> NamedTempFile {
    NamedTempFile::new().unwrap()
}

#[tokio::test]
async fn test_initialize_creates_tables() {
    let temp_file = setup_test_db_path();
    let db = LearningDb::new(temp_file.path()).await.unwrap();
    let stats = db.get_statistics().await.unwrap();
    assert_eq!(stats.total_verifications, 0);
}

#[tokio::test]
async fn test_record_verification() {
    let temp_file = setup_test_db_path();
    let db = LearningDb::new(temp_file.path()).await.unwrap();

    let segment = VerifiedSegment {
        start_ms: 1000,
        end_ms: 2000,
        confidence: 0.85,
        spectral_features: SpectralFeatures {
            rms: 0.01,
            zcr: 0.1,
            spectral_flux: 0.5,
            spectral_flatness: 0.3,
            spectral_entropy: 4.5,
            centroid_hz: 1500.0,
            low_band_ratio: 0.6,
            high_band_ratio: 0.1,
        },
        was_false_positive: false,
    };

    let id = db.record_verification(segment).await.unwrap();
    assert!(id > 0);

    let stats = db.get_statistics().await.unwrap();
    assert_eq!(stats.total_verifications, 1);
    assert_eq!(stats.total_false_positives, 0);
}

#[tokio::test]
async fn test_record_false_positive() {
    let temp_file = setup_test_db_path();
    let db = LearningDb::new(temp_file.path()).await.unwrap();

    let segment = VerifiedSegment {
        start_ms: 3000,
        end_ms: 4000,
        confidence: 0.6,
        spectral_features: SpectralFeatures {
            rms: 0.005,
            zcr: 0.2,
            spectral_flux: 0.3,
            spectral_flatness: 0.7,
            spectral_entropy: 2.5,
            centroid_hz: 800.0,
            low_band_ratio: 0.8,
            high_band_ratio: 0.05,
        },
        was_false_positive: true,
    };

    db.record_verification(segment).await.unwrap();

    let fps = db.get_false_positives().await.unwrap();
    assert_eq!(fps.len(), 1);
    assert_eq!(fps[0].start_ms, 3000);
}

#[tokio::test]
async fn test_threshold_recommendations_low_samples() {
    let temp_file = setup_test_db_path();
    let db = LearningDb::new(temp_file.path()).await.unwrap();

    let rec = db.get_threshold_recommendations().await.unwrap();
    assert_eq!(rec.sample_size, 0);
    assert!(rec.confidence == crate::adaptive_thresholds::RecommendationConfidence::Low);
}

#[tokio::test]
async fn test_threshold_recommendations_from_fps() {
    let temp_file = setup_test_db_path();
    let db = LearningDb::new(temp_file.path()).await.unwrap();

    for i in 0..10 {
        let segment = VerifiedSegment {
            start_ms: i as i64 * 1000,
            end_ms: (i + 1) as i64 * 1000,
            confidence: 0.7,
            spectral_features: SpectralFeatures {
                rms: 0.008 + i as f64 * 0.001,
                zcr: 0.15,
                spectral_flux: 0.4,
                spectral_flatness: 0.55 + i as f64 * 0.01,
                spectral_entropy: 3.0 + i as f64 * 0.1,
                centroid_hz: 1200.0 + i as f64 * 50.0,
                low_band_ratio: 0.65,
                high_band_ratio: 0.08,
            },
            was_false_positive: true,
        };
        db.record_verification(segment).await.unwrap();
    }

    let rec = db.get_threshold_recommendations().await.unwrap();
    assert_eq!(rec.sample_size, 10);
    assert!(rec.suggested_flatness_max > 0.55);
    assert!(rec.suggested_entropy_min < 4.0);
    assert!(rec.confidence == crate::adaptive_thresholds::RecommendationConfidence::Medium);
}

#[tokio::test]
async fn test_false_positive_rate() {
    let temp_file = setup_test_db_path();
    let db = LearningDb::new(temp_file.path()).await.unwrap();

    for i in 0..5 {
        db.record_verification(VerifiedSegment {
            start_ms: i as i64 * 1000,
            end_ms: (i as i64 + 1) * 1000,
            confidence: 0.8,
            spectral_features: SpectralFeatures::default(),
            was_false_positive: i % 2 == 0,
        })
        .await
        .unwrap();
    }

    let rate = db.get_false_positive_rate().await.unwrap();
    assert_eq!(rate, 0.6);
}

#[tokio::test]
async fn test_migration_from_json_to_columns() {
    let temp_file = setup_test_db_path();
    let db_path = temp_file.path().to_string_lossy().to_string();

    let db = libsql::Builder::new_local(&db_path).build().await.unwrap();
    let conn = db.connect().unwrap();
    conn.execute(
        "CREATE TABLE verified_segments (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            start_ms INTEGER NOT NULL,
            end_ms INTEGER NOT NULL,
            confidence REAL NOT NULL,
            spectral_features TEXT NOT NULL,
            was_false_positive INTEGER NOT NULL DEFAULT 0,
            timestamp TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        (),
    )
    .await
    .unwrap();

    let features = SpectralFeatures {
        rms: 0.123,
        zcr: 0.456,
        spectral_flux: 0.789,
        spectral_flatness: 0.111,
        spectral_entropy: 0.222,
        centroid_hz: 333.3,
        low_band_ratio: 0.444,
        high_band_ratio: 0.555,
    };
    let features_json = serde_json::to_string(&features).unwrap();

    conn.execute(
        "INSERT INTO verified_segments (start_ms, end_ms, confidence, spectral_features, was_false_positive)
         VALUES (100, 200, 0.9, ?1, 1)",
        [libsql::Value::Text(features_json)],
    )
    .await
    .unwrap();

    let learning_db = LearningDb::new(temp_file.path()).await.unwrap();

    let fps = learning_db.get_false_positives().await.unwrap();
    assert_eq!(fps.len(), 1);
    let fp = &fps[0];
    assert_eq!(fp.spectral_features.rms, 0.123);
    assert_eq!(fp.spectral_features.zcr, 0.456);
    assert_eq!(fp.spectral_features.centroid_hz, 333.3);

    let mut rows = learning_db
        .conn
        .query("PRAGMA table_info(verified_segments)", ())
        .await
        .unwrap();
    let mut has_spectral_features = false;
    while let Some(row) = rows.next().await.unwrap() {
        let name: String = row.get(1).unwrap();
        if name == "spectral_features" {
            has_spectral_features = true;
        }
    }
    assert!(!has_spectral_features);
}

#[tokio::test]
async fn test_foreign_key_enforcement() {
    let temp_file = setup_test_db_path();
    let db = LearningDb::new(temp_file.path()).await.unwrap();

    let result = db
        .conn
        .execute(
            "INSERT INTO segment_fingerprints (hash, offset_ms, segment_id) VALUES (1, 100, 9999)",
            (),
        )
        .await;

    assert!(
        result.is_err(),
        "Foreign key constraint should have prevented insertion"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("FOREIGN KEY constraint failed"));
}

#[tokio::test]
async fn test_record_fingerprints_atomicity() {
    let temp_file = setup_test_db_path();
    let db = LearningDb::new(temp_file.path()).await.unwrap();

    let segment = VerifiedSegment {
        start_ms: 1000,
        end_ms: 2000,
        confidence: 0.85,
        spectral_features: SpectralFeatures::default(),
        was_false_positive: false,
    };
    let segment_id = db.record_verification(segment).await.unwrap();

    let fingerprints = vec![
        movie_radio_types::Fingerprint {
            hash: 1,
            offset_ms: 100,
        },
        movie_radio_types::Fingerprint {
            hash: 2,
            offset_ms: 200,
        },
    ];

    db.record_fingerprints(segment_id, &fingerprints)
        .await
        .unwrap();

    let mut rows = db
        .conn
        .query(
            "SELECT COUNT(*) FROM segment_fingerprints WHERE segment_id = ?1",
            [libsql::Value::Integer(segment_id)],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let count: i64 = row.get(0).unwrap();
    assert_eq!(count, 2);

    let invalid_segment_id = 9999;
    let mixed_fingerprints = vec![
        movie_radio_types::Fingerprint {
            hash: 3,
            offset_ms: 300,
        },
        movie_radio_types::Fingerprint {
            hash: 4,
            offset_ms: 400,
        },
    ];

    let result = db
        .record_fingerprints(invalid_segment_id, &mixed_fingerprints)
        .await;
    assert!(result.is_err());

    let mut rows = db
        .conn
        .query(
            "SELECT COUNT(*) FROM segment_fingerprints WHERE hash IN (3, 4)",
            (),
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let count: i64 = row.get(0).unwrap();
    assert_eq!(count, 0, "Batch should have rolled back completely");
}

#[tokio::test]
async fn test_pragma_foreign_keys_is_on() {
    let temp_file = setup_test_db_path();
    let db = LearningDb::new(temp_file.path()).await.unwrap();

    let mut rows = db.conn.query("PRAGMA foreign_keys", ()).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let is_on: i64 = row.get(0).unwrap();
    assert_eq!(is_on, 1);
}

#[tokio::test]
async fn test_gap_decisions_storage() {
    let temp_file = setup_test_db_path();
    let db = LearningDb::new(temp_file.path()).await.unwrap();

    let decision = crate::gap_store::GapDecision {
        movie_hash: "movie123".to_string(),
        start_ms: 5000,
        end_ms: 8000,
        confidence: 0.9,
        reason: "long silence".to_string(),
        priority: 5,
        user_approved: Some(true),
    };

    let id = db.record_gap_decision(decision.clone()).await.unwrap();
    assert!(id > 0);

    let results = db.get_gap_decisions("movie123").await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].movie_hash, "movie123");
    assert_eq!(results[0].start_ms, 5000);
    assert_eq!(results[0].user_approved, Some(true));
}

#[tokio::test]
async fn test_experiment_tracking_and_profile_versioning() {
    let temp_file = setup_test_db_path();
    let db = LearningDb::new(temp_file.path()).await.unwrap();

    let exp_id = db
        .record_experiment(
            "exp_001",
            "Model Calibration Suite v2",
            Some("Boundary testing with synthetic fixtures"),
        )
        .await
        .unwrap();
    assert!(exp_id > 0);

    let report_id = db
        .record_calibration_report("radio-play", 1, 150, 5, 12, -0.002, Some("exp_001"))
        .await
        .unwrap();
    assert!(report_id > 0);

    let pv_id = db
        .record_profile_version(
            "radio-play",
            2,
            "{\"energy_threshold\": 0.015}",
            Some(report_id),
        )
        .await
        .unwrap();
    assert!(pv_id > 0);

    let exps = db.list_experiments().await.unwrap();
    assert_eq!(exps.len(), 1);
    assert_eq!(exps[0].experiment_id, "exp_001");

    let profiles = db.list_profile_versions().await.unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].version, 2);

    let reports = db.list_calibration_reports().await.unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].records_seen, 150);
}
