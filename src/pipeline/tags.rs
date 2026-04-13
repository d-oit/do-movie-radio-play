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
        let f = compute_features(clip, sr);
        seg.tags = map_tags(f);
    }
    Ok(())
}

fn map_tags(f: crate::pipeline::features::FeatureSet) -> Vec<String> {
    let mut tags = Vec::new();
    if f.rms < 0.012 {
        tags.push("ambience".into());
    }
    if f.low_band_ratio > 0.25 && f.rms > 0.015 {
        tags.push("music_bed".into());
    }
    if f.spectral_flux > 0.01 && f.rms > 0.05 {
        tags.push("impact_heavy".into());
    }
    if f.centroid_hz > 1800.0 && f.zcr > 0.15 {
        tags.push("machinery_like".into());
    }
    if f.low_band_ratio > 0.35 && f.zcr > 0.12 {
        tags.push("crowd_like".into());
    }
    if f.high_band_ratio < 0.1 && f.rms < 0.03 {
        tags.push("nature_like".into());
    }
    if tags.is_empty() {
        tags.push("ambience".into());
    }
    tags.sort();
    tags.dedup();
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_mapping_stable() {
        let f = crate::pipeline::features::FeatureSet {
            rms: 0.0,
            zcr: 0.0,
            spectral_flux: 0.0,
            centroid_hz: 0.0,
            low_band_ratio: 0.2,
            high_band_ratio: 0.0,
        };
        assert!(map_tags(f).contains(&"ambience".to_string()));
    }
}
