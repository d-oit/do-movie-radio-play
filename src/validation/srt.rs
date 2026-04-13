use anyhow::{bail, Result};

use crate::types::{Segment, SegmentKind};

pub fn parse_srt_segments(input: &str) -> Result<Vec<Segment>> {
    let mut speech = Vec::new();
    for block in input.split("\n\n") {
        let mut lines = block.lines().filter(|l| !l.trim().is_empty());
        let Some(first) = lines.next() else { continue };
        let time_line = if first.contains("-->") {
            first
        } else {
            lines.next().unwrap_or_default()
        };
        if time_line.trim().is_empty() {
            continue;
        }
        let Some((start, end)) = parse_time_span(time_line) else {
            bail!("invalid srt timing line: {time_line}");
        };
        speech.push(Segment {
            start_ms: start,
            end_ms: end,
            kind: SegmentKind::Speech,
            confidence: 1.0,
            tags: vec![],
            prompt: None,
        });
    }
    Ok(speech)
}

fn parse_time_span(line: &str) -> Option<(u64, u64)> {
    let mut parts = line.split("-->");
    let start = parse_stamp(parts.next()?.trim())?;
    let end = parse_stamp(parts.next()?.trim())?;
    Some((start, end))
}

fn parse_stamp(stamp: &str) -> Option<u64> {
    let stamp = stamp.split_whitespace().next()?;
    let mut parts = stamp.split(':');
    let h: u64 = parts.next()?.parse().ok()?;
    let m: u64 = parts.next()?.parse().ok()?;
    let s_ms = parts.next()?;
    let mut sec_parts = s_ms.split(',');
    let s: u64 = sec_parts.next()?.parse().ok()?;
    let ms: u64 = sec_parts.next()?.parse().ok()?;
    Some((((h * 60 + m) * 60 + s) * 1000) + ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_srt() {
        let srt =
            "1\n00:00:00,000 --> 00:00:01,000\nHello\n\n2\n00:00:02,000 --> 00:00:03,000\nWorld\n";
        let segs = parse_srt_segments(srt).unwrap_or_default();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[1].start_ms, 2000);
    }
}
