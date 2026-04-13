use crate::types::frame::Frame;

pub fn build_frames(samples: &[f32], sample_rate: u32, frame_ms: u32) -> Vec<Frame> {
    let frame_len = (sample_rate as usize * frame_ms as usize) / 1000;
    if frame_len == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (i, chunk) in samples.chunks(frame_len).enumerate() {
        if chunk.is_empty() {
            continue;
        }
        let sum_sq = chunk.iter().map(|v| v * v).sum::<f32>();
        let rms = (sum_sq / chunk.len() as f32).sqrt();
        let _ = i;
        out.push(Frame { rms });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_count_is_stable() {
        let s = vec![0.0f32; 3200];
        let frames = build_frames(&s, 16000, 20);
        assert_eq!(frames.len(), 10);
    }
}
