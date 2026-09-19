use super::*;

#[test]
fn fallback_run_id_format_is_stable() {
    let id = fallback_run_id(std::path::Path::new("some-movie-name.mkv"));
    assert!(id.starts_with("run-"), "{id}");
    assert!(id.ends_with("some-mov"), "{id}");
}

#[test]
fn fallback_run_ids_differ_within_one_millisecond() {
    let path = std::path::Path::new("same-movie-name.mkv");
    assert_ne!(fallback_run_id(path), fallback_run_id(path));
}

#[tokio::test]
async fn fallback_run_ids_are_unique_within_one_second() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("runs.db");
    let mut ctx = PipelineContext::new(
        PathBuf::from("same-movie-name.mkv"),
        PathBuf::from("out.wav"),
    );
    ctx.learning_db_path = Some(db_path);
    record_execution_trace(&ctx).await.expect("first trace");
    record_execution_trace(&ctx).await.expect("second trace");

    let db = movie_radio_learning::database::LearningDb::new(
        &ctx.learning_db_path.clone().expect("db path"),
    )
    .await
    .expect("open db");
    assert_eq!(db.get_run_traces(10).await.expect("traces").len(), 2);
}
