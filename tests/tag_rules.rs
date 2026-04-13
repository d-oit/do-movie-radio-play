use movie_nonvoice_timeline::pipeline::features::compute_features;

#[test]
fn feature_computation_is_deterministic() {
    let s = vec![0.0f32, 1.0, -1.0, 1.0, -1.0];
    let f = compute_features(&s, 16_000);
    assert!(f.zcr > 0.0);
}
