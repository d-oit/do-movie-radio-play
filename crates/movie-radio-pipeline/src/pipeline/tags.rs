use anyhow::Result;
use std::path::Path;

use crate::pipeline::decode;
use crate::pipeline::features::FeatureExtractor;
use movie_radio_learning::profiles::TagThresholds;
use movie_radio_types::{SegmentKind, TimelineOutput};

#[derive(Debug, Clone)]
pub struct TagRules {
    pub ambience_max_rms: f32,
    pub impact_min_rms: f32,
    pub min_centroid_hz: f32,
}

impl Default for TagRules {
    fn default() -> Self {
        Self {
            ambience_max_rms: 0.012,
            impact_min_rms: 0.05,
            min_centroid_hz: 1800.0,
        }
    }
}

impl TagRules {
    pub fn from_thresholds(thresholds: &TagThresholds) -> Self {
        Self {
            ambience_max_rms: (0.012 + thresholds.ambience_max_rms_delta).max(0.002),
            impact_min_rms: (0.05 + thresholds.impact_min_rms_delta).max(0.01),
            min_centroid_hz: (1800.0 + thresholds.min_centroid_hz_delta).max(100.0),
        }
    }
}

pub fn add_tags(
    input_media: &Path,
    timeline: &mut TimelineOutput,
    rules: Option<&TagRules>,
) -> Result<()> {
    let default_rules = TagRules::default();
    let rules = rules.unwrap_or(&default_rules);
    let (samples, sr) = decode::decode_audio(input_media, timeline.analysis_sample_rate)?;
    let mut extractor = FeatureExtractor::new(1024);
    for seg in &mut timeline.segments {
        if seg.kind != SegmentKind::NonVoice {
            continue;
        }
        let start = (seg.start_ms * sr as u64 / 1000) as usize;
        let end = (seg.end_ms * sr as u64 / 1000) as usize;

        let start = start.min(samples.len());
        let end = end.clamp(start, samples.len());

        let clip = &samples[start..end];
        let f = extractor.extract(clip, sr);
        seg.tags = map_tags(f, rules);
    }
    Ok(())
}

fn map_tags(f: crate::pipeline::features::FeatureSet, rules: &TagRules) -> Vec<String> {
    let mut tags = Vec::new();
    if f.rms < rules.ambience_max_rms {
        tags.push("ambience".into());
    }
    if f.low_band_ratio > 0.25 && f.rms > 0.015 {
        tags.push("music_bed".into());
    }
    if f.spectral_flux > 0.01 && f.rms > rules.impact_min_rms {
        tags.push("impact_heavy".into());
    }
    if f.centroid_hz > rules.min_centroid_hz && f.zcr > 0.15 {
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
        let rules = TagRules::default();
        assert!(map_tags(f, &rules).contains(&"ambience".to_string()));
    }

    #[test]
    fn test_add_tags_indexing_safety() {
        use movie_radio_types::Segment;
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
        add_tags(&wav_path, &mut timeline, None).unwrap();
        assert!(!timeline.segments[0].tags.is_empty());

        // Test with start > end (should be clamped and empty clip)
        timeline.segments[0].start_ms = 3000;
        timeline.segments[0].end_ms = 2000;
        add_tags(&wav_path, &mut timeline, None).unwrap();
        assert!(timeline.segments[0].tags.contains(&"ambience".to_string()));

        // Test that custom TagRules changes tag outcomes
        // With default rules, rms=0.018 with low_band_ratio>0.25 and rms>0.015 matches music_bed
        let music_f = crate::pipeline::features::FeatureSet {
            rms: 0.018,
            zcr: 0.1,
            spectral_flux: 0.005,
            spectral_flatness: 0.3,
            spectral_entropy: 5.0,
            centroid_hz: 500.0,
            low_band_ratio: 0.3,
            high_band_ratio: 0.1,
        };
        let default_rules = TagRules::default();
        // Default rules: rms=0.018 > 0.012, so NOT ambience via rule; music_bed matches
        let tags_default = map_tags(music_f, &default_rules);
        assert!(
            tags_default.contains(&"music_bed".to_string()),
            "music_bed should match with default rules"
        );
        assert!(
            !tags_default.contains(&"ambience".to_string()),
            "ambience should not match via rule with rms=0.018"
        );

        // With raised ambience threshold, should match ambience directly
        let high_ambience = TagRules {
            ambience_max_rms: 0.020,
            ..TagRules::default()
        };
        let tags_high = map_tags(music_f, &high_ambience);
        assert!(
            tags_high.contains(&"ambience".to_string()),
            "raised ambience_max_rms should include rms=0.018 as ambience"
        );

        // Test that tightened centroid threshold excludes machinery_like tag
        let f_machinery = crate::pipeline::features::FeatureSet {
            rms: 0.02,
            zcr: 0.2,
            spectral_flux: 0.01,
            spectral_flatness: 0.3,
            spectral_entropy: 5.0,
            centroid_hz: 1900.0,
            low_band_ratio: 0.2,
            high_band_ratio: 0.1,
        };
        assert!(map_tags(f_machinery, &default_rules).contains(&"machinery_like".to_string()));
        let tight_centroid = TagRules {
            min_centroid_hz: 2000.0,
            ..TagRules::default()
        };
        assert!(!map_tags(f_machinery, &tight_centroid).contains(&"machinery_like".to_string()));
    }
}
