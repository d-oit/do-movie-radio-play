use crate::types::frame::Frame;

pub fn classify_frames(frames: &[Frame], threshold: f32) -> Vec<bool> {
    frames.iter().map(|f| f.rms >= threshold).collect()
}
