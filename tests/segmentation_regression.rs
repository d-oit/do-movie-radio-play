use movie_nonvoice_timeline::pipeline::segmenter::{
    invert_to_non_voice, smooth_speech, speech_segments,
};

#[test]
fn smoothing_and_inversion_regression() {
    let raw = vec![false, true, false, false, false, false];
    let smoothed = smooth_speech(&raw, 20, 60);
    let frame_likelihoods = vec![0.2, 0.8, 0.3, 0.2, 0.2, 0.2];
    let speech = speech_segments(&smoothed, 20, 40, &frame_likelihoods);
    let nv = invert_to_non_voice(&speech, 200, 20, 20, &frame_likelihoods);
    assert!(!nv.is_empty());
}
