#[cfg(test)]
mod wiring_tests {
    use crate::actions::{ApplyLearnings, VerifyQuality};
    use crate::test_support::{empty_timeline, suspicious_report};
    use crate::{Action, PipelineContext};
    use std::path::PathBuf;

    #[tokio::test]
    async fn verify_quality_requires_timeline() {
        let mut ctx = PipelineContext::new(PathBuf::from("movie.mkv"), PathBuf::from("out.wav"));
        let err = VerifyQuality.execute(&mut ctx).await.unwrap_err();
        assert!(err.to_string().contains("Timeline not extracted"), "{err}");
    }

    #[tokio::test]
    async fn verify_quality_bails_when_media_missing() {
        let mut ctx = PipelineContext::new(
            PathBuf::from("/nonexistent/movie.mkv"),
            PathBuf::from("out.wav"),
        );
        ctx.timeline = Some(empty_timeline());
        let err = VerifyQuality.execute(&mut ctx).await.unwrap_err();
        assert!(err.to_string().contains("cannot verify quality"), "{err}");
    }

    #[tokio::test]
    async fn apply_learnings_requires_verification_first() {
        let mut ctx = PipelineContext::new(PathBuf::from("movie.mkv"), PathBuf::from("out.wav"));
        let err = ApplyLearnings.execute(&mut ctx).await.unwrap_err();
        assert!(err.to_string().contains("no verification report"), "{err}");
    }

    #[tokio::test]
    async fn apply_learnings_records_and_persists_state() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state_path = dir.path().join("learning-state.json");
        let db_path = dir.path().join("learn.db");
        let mut ctx = PipelineContext::new(PathBuf::from("movie.mkv"), PathBuf::from("out.wav"));
        ctx.verification = Some(suspicious_report(3));
        ctx.learning_state_path = Some(state_path.clone());
        ctx.learning_db_path = Some(db_path.clone());

        ApplyLearnings
            .execute(&mut ctx)
            .await
            .expect("apply learnings");

        assert!(ctx.learning.is_some(), "thresholds must be exposed on ctx");
        let state = movie_radio_learning::adaptive_thresholds::load_learning_state(&state_path)
            .expect("learning state persisted");
        assert_eq!(state.total_verifications, 3);
        assert!(state.total_false_positives > 0);
        assert!(
            db_path.exists(),
            "learning db must be created when configured"
        );
    }

    #[tokio::test]
    async fn record_execution_trace_honors_no_learn() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("learn.db");
        let mut ctx = PipelineContext::new(PathBuf::from("movie.mkv"), PathBuf::from("out.wav"));
        ctx.learning_db_path = Some(db_path.clone());
        ctx.no_learn = true;

        crate::record_execution_trace(&ctx)
            .await
            .expect("record trace with no_learn");

        assert!(
            !db_path.exists(),
            "db should not be created when no_learn is true"
        );

        ctx.no_learn = false;
        crate::record_execution_trace(&ctx)
            .await
            .expect("record trace");

        assert!(
            db_path.exists(),
            "db should be created when no_learn is false"
        );

        let db = movie_radio_learning::database::LearningDb::new(&db_path)
            .await
            .unwrap();
        let traces = db.get_run_traces(10).await.unwrap();
        assert_eq!(traces.len(), 1);
    }
}
