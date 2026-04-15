use std::collections::HashSet;

use crate::types::{SegmentKind, TimelineOutput};

pub fn export_vtt(timeline: &TimelineOutput, verified: Option<&HashSet<(u64, u64)>>) -> String {
    let mut output = String::new();
    output.push_str("WEBVTT\n\n");
    output.push_str("NOTE Non-Voice Timeline Export\n\n");

    for (i, segment) in timeline.segments.iter().enumerate() {
        if segment.kind != SegmentKind::NonVoice {
            continue;
        }

        let is_verified = verified
            .map(|v| v.contains(&(segment.start_ms, segment.end_ms)))
            .unwrap_or(false);

        if !is_verified {
            continue;
        }

        let start_ts = ms_to_vtt_timestamp(segment.start_ms);
        let end_ts = ms_to_vtt_timestamp(segment.end_ms);
        let duration_ms = segment.end_ms - segment.start_ms;

        output.push_str(&format!("{start_ts} --> {end_ts}\n"));
        output.push_str(&format!(
            "Non-voice segment #{} ({}ms, confidence: {:.2}) {}\n\n",
            i + 1,
            duration_ms,
            segment.confidence,
            if is_verified {
                "[VERIFIED]"
            } else {
                "[UNVERIFIED]"
            }
        ));
    }

    output
}

fn ms_to_vtt_timestamp(ms: u64) -> String {
    let total_seconds = ms as f64 / 1000.0;
    let hours = (total_seconds / 3600.0).floor() as u32;
    let minutes = ((total_seconds % 3600.0) / 60.0).floor() as u32;
    let seconds = (total_seconds % 60.0).floor() as u32;
    let millis = (total_seconds.fract() * 1000.0).round() as u32;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Segment;

    #[test]
    fn vtt_export_basic() {
        let timeline = TimelineOutput {
            file: "test.mp4".to_string(),
            analysis_sample_rate: 16000,
            frame_ms: 20,
            segments: vec![Segment {
                start_ms: 0,
                end_ms: 5000,
                kind: SegmentKind::NonVoice,
                confidence: 0.9,
                tags: vec![],
                prompt: None,
            }],
        };

        let verified: HashSet<(u64, u64)> = vec![(0, 5000)].into_iter().collect();
        let output = export_vtt(&timeline, Some(&verified));
        assert!(output.starts_with("WEBVTT"));
    }
}
