//! Window-wise tagging of the spooled source audio (#418).

use anyhow::Result;
use movie_radio_pipeline::pipeline::tags::add_tags_from_reader;
use movie_radio_verification::SampleSource;
use std::borrow::Cow;

use crate::gaps::energy_trend_from_source;

/// Re-derives each window's tags from its own audio so a long scene is
/// described by what is audible in that stretch, not by the whole segment.
pub(super) fn retag_windows(
    windows: &mut [movie_radio_types::VisualGap],
    source: &crate::OriginalAudio,
    sr: u32,
    file: &str,
) -> Result<()> {
    use movie_radio_types::{Segment, SegmentKind, TimelineOutput};
    let mut tl = TimelineOutput {
        file: file.to_string(),
        analysis_sample_rate: sr,
        frame_ms: 20,
        segments: windows
            .iter()
            .map(|w| Segment {
                start_ms: w.start_ms,
                end_ms: w.end_ms,
                kind: SegmentKind::NonVoice,
                confidence: w.confidence,
                tags: Vec::new(),
                prompt: None,
                sfx_trigger: None,
            })
            .collect(),
    };
    tag_from_source(source, sr, &mut tl)?;
    for (w, seg) in windows.iter_mut().zip(tl.segments) {
        if !seg.tags.is_empty() {
            w.tags = seg.tags;
        }
        w.trend = energy_trend_from_source(source, sr, w.start_ms, w.end_ms)?;
    }
    Ok(())
}

/// Tags non-voice segments from their own windows of the spooled audio.
pub(super) fn tag_from_source<S: SampleSource + ?Sized>(
    source: &S,
    sr: u32,
    timeline: &mut movie_radio_types::TimelineOutput,
) -> Result<()> {
    add_tags_from_reader(source.len(), sr, timeline, None, |start, end| {
        let mut clip = Vec::new();
        source.read_window(start, end, &mut clip)?;
        Ok(Cow::Owned(clip))
    })
}
