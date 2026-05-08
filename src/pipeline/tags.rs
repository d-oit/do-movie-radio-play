use anyhow::Result;
use std::path::Path;

use crate::pipeline::decode;
use crate::pipeline::features::FeatureExtractor;
use crate::types::{SegmentKind, TimelineOutput};

pub fn add_tags(input_media: &Path, timeline: &mut TimelineOutput) -> Result<()> {
    let (samples, sr) = decode::decode_audio(input_media, timeline.analysis_sample_rate)?;
    let mut extractor = FeatureExtractor::new(1024);
    for seg in &mut timeline.segments {
        if seg.kind != SegmentKind::NonVoice {
            continue;
        }
        let start = (seg.start_ms * sr as u64 / 1000) as usize;
        let end = (seg.end_ms * sr as u64 / 1000) as usize;
        let clip = &samples[start.min(samples.len())..end.min(samples.len())];
        let f = extractor.extract(clip, sr);
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
    if f.spectral_flatness > 0.35 && f.rms > 0.02 {
        tags.push("tonal".into());
    }
    if f.spectral_entropy > 6.0 && f.low_band_ratio > 0.3 {
        tags.push("music_like".into());
    }
    if f.spectral_entropy < 4.0 && f.spectral_flatness < 0.3 {
        tags.push("speech_like".into());
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
            spectral_flatness: 0.2,
            spectral_entropy: 5.0,
            centroid_hz: 0.0,
            low_band_ratio: 0.2,
            high_band_ratio: 0.0,
        };
        assert!(map_tags(f).contains(&"ambience".to_string()));
    }

    #[test]
    fn test_add_tags_indexing_safety() {
        use crate::types::Segment;
        let mut timeline = TimelineOutput {
            file: "test".into(),
            analysis_sample_rate: 16000,
            frame_ms: 20,
            segments: vec![Segment {
                start_ms: 0,
                end_ms: 2000, // 2 seconds
                kind: SegmentKind::NonVoice,
                confidence: 1.0,
                tags: vec![],
                prompt: None,
            }],
        };

        let temp_dir = tempfile::tempdir().unwrap();
        let wav_path = temp_dir.path().join("short.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&wav_path, spec).unwrap();
        // Only 1 second of audio
        for _ in 0..16000 {
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();

        // Should not panic even though segment (2s) > audio (1s)
        add_tags(&wav_path, &mut timeline).unwrap();
        assert!(!timeline.segments[0].tags.is_empty());
    }
}
