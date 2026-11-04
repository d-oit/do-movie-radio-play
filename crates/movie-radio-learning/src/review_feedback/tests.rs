use super::*;
use crate::database::LearningDb;
use tempfile::NamedTempFile;

fn decision(label: ReviewLabel, start: u64, end: u64) -> ReviewDecision {
    ReviewDecision {
        decision_id: String::new(),
        run_id: "run-1".to_string(),
        source_fingerprint: "src-abc".to_string(),
        gap_start_ms: start,
        gap_end_ms: end,
        film: "film-a".to_string(),
        language: "de".to_string(),
        genre: Some("drama".to_string()),
        cue_source: "vad".to_string(),
        evidence: ReviewEvidence {
            acoustic_tags: vec!["music".to_string()],
            visual_evidence: Some("empty street".to_string()),
            confidence: 0.7,
        },
        narration: Some("Original narration.".to_string()),
        tts_output: Some("/runs/run-1/tts/seg-1.wav".to_string()),
        assembly_position_ms: Some(12_000),
        model_versions: ModelVersions {
            vad_engine: "energy".to_string(),
            vad_model: "v1".to_string(),
            narrator_model: "narrator-v2".to_string(),
            tts_provider: "kokoro".to_string(),
            tts_model: "kokoro-v1".to_string(),
        },
        label,
        edited_narration: None,
        reviewer: "reviewer-1".to_string(),
        source: "review-html".to_string(),
    }
}

fn batch(decisions: Vec<ReviewDecision>) -> ReviewFeedbackBatch {
    ReviewFeedbackBatch {
        schema_version: REVIEW_FEEDBACK_SCHEMA_VERSION,
        decisions,
    }
}

async fn test_db() -> (NamedTempFile, LearningDb) {
    let file = NamedTempFile::new().expect("temp db file");
    let db = LearningDb::new(file.path()).await.expect("learning db");
    (file, db)
}

fn holdout(checksum: &str) -> HoldoutFixture {
    HoldoutFixture {
        fixture_id: "holdout-1".to_string(),
        path: "/fixtures/holdout".to_string(),
        checksum: checksum.to_string(),
        registered_at: String::new(),
    }
}

#[test]
fn decision_id_is_stable() {
    let a = decision(ReviewLabel::Accept, 1000, 2000);
    let b = decision(ReviewLabel::Accept, 1000, 2000);
    let c = decision(ReviewLabel::Accept, 1000, 2001);
    assert_eq!(a.stable_id(), b.stable_id());
    assert_ne!(a.stable_id(), c.stable_id());
}

#[test]
fn unknown_label_is_rejected() {
    let json = r#"{"schema_version":1,"decisions":[{"run_id":"r","source_fingerprint":"s","gap_start_ms":1,"gap_end_ms":2,"label":"bogus"}]}"#;
    assert!(serde_json::from_str::<ReviewFeedbackBatch>(json).is_err());
}

#[tokio::test]
async fn import_is_idempotent() {
    let (_file, db) = test_db().await;
    let batch = batch(vec![decision(ReviewLabel::Reject, 1000, 2000)]);

    let first = db
        .import_review_feedback(&batch, &ImportOptions::default())
        .await
        .expect("first import");
    assert_eq!(first.decisions_imported, 1);
    assert_eq!(first.events_recorded, 1);

    let second = db
        .import_review_feedback(&batch, &ImportOptions::default())
        .await
        .expect("second import");
    assert_eq!(second.decisions_imported, 0);
    assert_eq!(second.decisions_duplicate, 1);
    assert_eq!(second.events_recorded, 0);
    assert_eq!(second.events_duplicate, 1);
    assert_eq!(db.get_correction_events(10).await.expect("events").len(), 1);
}

#[tokio::test]
async fn corrupted_feedback_is_rejected_without_writes() {
    let (_file, db) = test_db().await;

    let inverted = decision(ReviewLabel::Reject, 2000, 1000);
    let err = db
        .import_review_feedback(&batch(vec![inverted]), &ImportOptions::default())
        .await
        .expect_err("inverted interval must fail");
    assert!(err.to_string().contains("empty or inverted"));
    assert!(db
        .get_review_decisions(10)
        .await
        .expect("decisions")
        .is_empty());

    let mut edit = decision(ReviewLabel::EditNarration, 1000, 2000);
    edit.edited_narration = None;
    let err = db
        .import_review_feedback(&batch(vec![edit]), &ImportOptions::default())
        .await
        .expect_err("edit without text must fail");
    assert!(err.to_string().contains("edited_narration"));
}

#[tokio::test]
async fn mismatched_decision_id_is_rejected() {
    let (_file, db) = test_db().await;
    let mut spoofed = decision(ReviewLabel::Reject, 1000, 2000);
    spoofed.decision_id = "spoofed".to_string();
    let err = db
        .import_review_feedback(&batch(vec![spoofed]), &ImportOptions::default())
        .await
        .expect_err("spoofed id must fail");
    assert!(err.to_string().contains("does not match"));
}

#[tokio::test]
async fn reviewer_edit_does_not_overwrite_original() {
    let (_file, db) = test_db().await;
    let mut edit = decision(ReviewLabel::EditNarration, 1000, 2000);
    edit.edited_narration = Some("Corrected narration.".to_string());
    db.import_review_feedback(&batch(vec![edit]), &ImportOptions::default())
        .await
        .expect("import edit");

    let decisions = db.get_review_decisions(10).await.expect("decisions");
    assert_eq!(
        decisions[0].narration.as_deref(),
        Some("Original narration.")
    );
    let events = db.get_correction_events(10).await.expect("events");
    assert_eq!(
        events[0].original_narration.as_deref(),
        Some("Original narration.")
    );
    assert_eq!(
        events[0].corrected_narration.as_deref(),
        Some("Corrected narration.")
    );
}

#[tokio::test]
async fn reviewer_override_fills_missing_reviewer() {
    let (_file, db) = test_db().await;
    let mut anonymous = decision(ReviewLabel::Accept, 1000, 2000);
    anonymous.reviewer = String::new();
    let options = ImportOptions {
        reviewer_override: Some("cli-reviewer".to_string()),
        dry_run: false,
    };
    db.import_review_feedback(&batch(vec![anonymous]), &options)
        .await
        .expect("import with override");
    let events = db.get_correction_events(10).await.expect("events");
    assert_eq!(events[0].reviewer, "cli-reviewer");
}

#[tokio::test]
async fn dry_run_does_not_write() {
    let (_file, db) = test_db().await;
    let options = ImportOptions {
        dry_run: true,
        ..ImportOptions::default()
    };
    let summary = db
        .import_review_feedback(
            &batch(vec![decision(ReviewLabel::Reject, 1000, 2000)]),
            &options,
        )
        .await
        .expect("dry run");
    assert!(summary.dry_run);
    assert_eq!(summary.decisions_imported, 1);
    assert!(db
        .get_review_decisions(10)
        .await
        .expect("decisions")
        .is_empty());
}

#[tokio::test]
async fn holdout_import_is_rejected() {
    let (_file, db) = test_db().await;
    db.register_holdout_fixture(&holdout("src-abc"))
        .await
        .expect("register holdout");

    let err = db
        .import_review_feedback(
            &batch(vec![decision(ReviewLabel::Reject, 1000, 2000)]),
            &ImportOptions::default(),
        )
        .await
        .expect_err("holdout import must fail");
    assert!(err.to_string().contains("holdout fixture"));
    assert!(db
        .get_review_decisions(10)
        .await
        .expect("decisions")
        .is_empty());
}

#[tokio::test]
async fn holdout_proposal_apply_is_rejected() {
    let (_file, db) = test_db().await;
    db.import_review_feedback(
        &batch(vec![decision(ReviewLabel::Reject, 1000, 2000)]),
        &ImportOptions::default(),
    )
    .await
    .expect("import");
    let proposals = db.derive_learning_proposals().await.expect("proposals");
    let proposal = proposals
        .iter()
        .find(|proposal| proposal.parameter == "gap_confidence_threshold")
        .expect("gap proposal");

    db.register_holdout_fixture(&holdout("src-abc"))
        .await
        .expect("register holdout");
    let err = db
        .apply_learning_proposal(&proposal.proposal_id)
        .await
        .expect_err("holdout proposal must not apply");
    assert!(err.to_string().contains("holdout fixture"));

    let parameters = db.list_learning_parameters().await.expect("parameters");
    let gap = parameters
        .iter()
        .find(|parameter| parameter.parameter == "gap_confidence_threshold")
        .expect("gap parameter");
    assert_eq!(gap.value, "0.50");
}

#[tokio::test]
async fn apply_and_rollback_restore_prior_version() {
    let (_file, db) = test_db().await;
    db.import_review_feedback(
        &batch(vec![
            decision(ReviewLabel::Reject, 1000, 2000),
            decision(ReviewLabel::Reject, 3000, 4000),
        ]),
        &ImportOptions::default(),
    )
    .await
    .expect("import");
    let proposals = db.derive_learning_proposals().await.expect("proposals");
    let proposal = proposals
        .iter()
        .find(|proposal| proposal.parameter == "gap_confidence_threshold")
        .expect("gap proposal");
    assert_eq!(proposal.current_value, "0.50");
    assert_eq!(proposal.proposed_value, "0.54");
    assert_eq!(proposal.evidence.sample_size, 2);

    let applied = db
        .apply_learning_proposal(&proposal.proposal_id)
        .await
        .expect("apply");
    assert_eq!(applied.action, VersionAction::Apply);
    assert_eq!(applied.prior_value.as_deref(), Some("0.50"));
    assert_eq!(applied.new_value.as_deref(), Some("0.54"));

    let parameters = db.list_learning_parameters().await.expect("parameters");
    let gap = parameters
        .iter()
        .find(|parameter| parameter.parameter == "gap_confidence_threshold")
        .expect("gap parameter");
    assert_eq!(gap.value, "0.54");
    assert_eq!(gap.origin, ProposalOrigin::Learned);

    let rolled = db
        .rollback_learning_proposal(&proposal.proposal_id)
        .await
        .expect("rollback");
    assert_eq!(rolled.action, VersionAction::Rollback);
    assert_eq!(rolled.new_value.as_deref(), Some("0.50"));

    let parameters = db.list_learning_parameters().await.expect("parameters");
    let gap = parameters
        .iter()
        .find(|parameter| parameter.parameter == "gap_confidence_threshold")
        .expect("gap parameter");
    assert_eq!(gap.value, "0.50");
    assert_eq!(gap.origin, ProposalOrigin::Manual);

    let versions = db
        .get_learning_versions(Some(&proposal.proposal_id))
        .await
        .expect("versions");
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].action, VersionAction::Apply);
    assert_eq!(versions[1].action, VersionAction::Rollback);
}

#[tokio::test]
async fn metrics_group_by_dimensions() {
    let (_file, db) = test_db().await;
    let mut accepted = decision(ReviewLabel::Accept, 1000, 2000);
    accepted.film = "film-a".to_string();
    accepted.language = "de".to_string();
    accepted.genre = Some("drama".to_string());
    accepted.model_versions.vad_engine = "energy".to_string();
    accepted.cue_source = "vad".to_string();

    let mut rejected = decision(ReviewLabel::Reject, 3000, 4000);
    rejected.film = "film-b".to_string();
    rejected.language = "en".to_string();
    rejected.genre = None;
    rejected.model_versions.vad_engine = "silero".to_string();
    rejected.cue_source = "subtitle".to_string();

    db.import_review_feedback(&batch(vec![accepted, rejected]), &ImportOptions::default())
        .await
        .expect("import");
    let metrics = db.review_outcome_metrics().await.expect("metrics");
    assert_eq!(metrics.len(), 2);

    let film_a = metrics
        .iter()
        .find(|metric| metric.film == "film-a")
        .expect("film-a");
    assert_eq!(film_a.decisions, 1);
    assert_eq!(film_a.accepted, 1);
    assert_eq!(film_a.rejected, 0);
    assert_eq!(film_a.genre.as_deref(), Some("drama"));

    let film_b = metrics
        .iter()
        .find(|metric| metric.film == "film-b")
        .expect("film-b");
    assert_eq!(film_b.rejected, 1);
    assert_eq!(film_b.genre, None);
    assert_eq!(film_b.vad_engine, "silero");
    assert_eq!(film_b.cue_source, "subtitle");
}

#[tokio::test]
async fn privacy_policy_blocks_uploads_and_redacts_paths() {
    let policy = PrivacyPolicy::default();
    assert!(policy
        .assert_upload_allowed(ArtifactKind::SourceMedia)
        .is_err());
    assert!(policy
        .assert_upload_allowed(ArtifactKind::VoiceSample)
        .is_err());
    assert!(policy
        .assert_upload_allowed(ArtifactKind::ReviewArtifact)
        .is_err());

    let (_file, db) = test_db().await;
    db.import_review_feedback(
        &batch(vec![decision(ReviewLabel::Accept, 1000, 2000)]),
        &ImportOptions::default(),
    )
    .await
    .expect("import");
    let export = db.export_review_feedback(&policy).await.expect("export");
    assert_eq!(export.decisions[0].tts_output.as_deref(), Some("seg-1.wav"));
    assert!(!serde_json::to_string(&export)
        .expect("serialize")
        .contains("/runs/run-1/tts"));
}

#[tokio::test]
async fn correction_events_are_immutable() {
    let (_file, db) = test_db().await;
    db.import_review_feedback(
        &batch(vec![decision(ReviewLabel::Reject, 1000, 2000)]),
        &ImportOptions::default(),
    )
    .await
    .expect("import");
    let err = db
        .conn
        .execute("UPDATE correction_events SET label = 'accept'", ())
        .await
        .expect_err("update must be blocked");
    assert!(err.to_string().contains("immutable"));
}
