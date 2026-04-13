use anyhow::Result;
use std::path::Path;

use crate::pipeline::decode;
use crate::pipeline::features::compute_features;
use crate::types::{SegmentKind, TimelineOutput};

pub fn add_tags(input_media: &Path, timeline: &mut TimelineOutput) -> Result<()> {
    let (samples, sr) = decode::decode_audio(input_media)?;
    for seg in &mut timeline.segments {
        if seg.kind != SegmentKind::NonVoice {
            continue;
        }
        let start = (seg.start_ms * sr as u64 / 1000) as usize;
        let end = (seg.end_ms * sr as u64 / 1000) as usize;
        let clip = &samples[start.min(samples.len())..end.min(samples.len())];
        let f = compute_features(clip);
        seg.tags = map_tags(f.rms, f.zcr);
    }
    Ok(())
}

fn map_tags(rms: f32, zcr: f32) -> Vec<String> {
    let mut tags = Vec::new();
    if rms < 0.01 {
        tags.push("low_intensity".into());
        tags.push("ambience".into());
    } else if rms > 0.08 {
        tags.push("high_intensity".into());
        tags.push("impact_heavy".into());
    } else {
        tags.push("music_bed".into());
    }
    if zcr > 0.2 {
        tags.push("machinery_like".into());
    }
    if tags.is_empty() {
        tags.push("unclassified_non_voice".into());
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_mapping_stable() {
        assert!(map_tags(0.0, 0.0).contains(&"ambience".to_string()));
    }
}
