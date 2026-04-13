use movie_nonvoice_timeline::pipeline::segmenter::{
    invert_to_non_voice, smooth_speech, speech_segments,
};

#[test]
fn smoothing_and_inversion_regression() {
    let raw = vec![false, true, false, false, false, false];
    let smoothed = smooth_speech(&raw, 20, 60);
    let speech = speech_segments(&smoothed, 20, 40);
    let nv = invert_to_non_voice(&speech, 200, 20);
    assert!(!nv.is_empty());
}
