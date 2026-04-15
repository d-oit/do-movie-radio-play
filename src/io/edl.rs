use std::collections::HashSet;

use crate::types::{SegmentKind, TimelineOutput};

pub fn export_edl(timeline: &TimelineOutput, verified: Option<&HashSet<(u64, u64)>>) -> String {
    let mut output = String::new();
    output.push_str("TITLE: Non-Voice Timeline\n");
    output.push_str("FCM: NON-DROP FRAME\n\n");

    let mut event_num = 1;
    for segment in &timeline.segments {
        if segment.kind != SegmentKind::NonVoice {
            continue;
        }

        let is_verified = verified
            .map(|v| v.contains(&(segment.start_ms, segment.end_ms)))
            .unwrap_or(false);

        if !is_verified {
            continue;
        }

        let start_tc = ms_to_timecode(segment.start_ms);
        let end_tc = ms_to_timecode(segment.end_ms);
        let duration = (segment.end_ms - segment.start_ms) as f64 / 1000.0;

        output.push_str(&format!(
            "{:03}  001      V     C        {} {} {} EVENT {}\n",
            event_num,
            start_tc,
            end_tc,
            format_duration(duration),
            if is_verified {
                "VERIFIED"
            } else {
                "UNVERIFIED"
            }
        ));
        output.push_str("* FROM CLIP NAME: Non-voice segment\n");
        output.push('\n');

        event_num += 1;
    }

    output
}

fn ms_to_timecode(ms: u64) -> String {
    let total_seconds = ms as f64 / 1000.0;
    let hours = (total_seconds / 3600.0).floor() as u32;
    let minutes = ((total_seconds % 3600.0) / 60.0).floor() as u32;
    let seconds = (total_seconds % 60.0).floor() as u32;
    let frames = ((total_seconds.fract()) * 30.0).floor() as u32;
    format!("{hours:02}:{minutes:02}:{seconds:02}:{frames:02}")
}

fn format_duration(seconds: f64) -> String {
    let hours = (seconds / 3600.0).floor() as u32;
    let minutes = ((seconds % 3600.0) / 60.0).floor() as u32;
    let secs = (seconds % 60.0).floor() as u32;
    let frames = ((seconds.fract()) * 30.0).floor() as u32;
    format!("{hours:02}:{minutes:02}:{secs:02}:{frames:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Segment;

    #[test]
    fn edl_export_basic() {
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
        let output = export_edl(&timeline, Some(&verified));
        assert!(output.contains("TITLE: Non-Voice Timeline"));
    }
}
