use movie_radio_types::VisualGap;

/// Longest span narrated as one unit. A 70 s non-voice stretch described by a
/// single line leaves a radio play mute for a minute, so longer gaps are cut
/// into near-equal windows.
pub const MAX_NARRATION_WINDOW_MS: u64 = 15_000;

/// Splits every gap longer than `max_window_ms` into the fewest equal windows
/// that fit. Windows inherit confidence, reason, priority and tags; output is
/// ordered by start time and deterministic.
pub fn split_gap_windows(gaps: &[VisualGap], max_window_ms: u64) -> Vec<VisualGap> {
    let max_window_ms = max_window_ms.max(1);
    let mut out = Vec::new();
    for gap in gaps {
        let duration = gap.end_ms.saturating_sub(gap.start_ms);
        let parts = duration.div_ceil(max_window_ms).max(1);
        for i in 0..parts {
            out.push(VisualGap {
                start_ms: gap.start_ms + duration * i / parts,
                end_ms: gap.start_ms + duration * (i + 1) / parts,
                ..gap.clone()
            });
        }
    }
    out.sort_by_key(|g| g.start_ms);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gap(start_ms: u64, end_ms: u64) -> VisualGap {
        VisualGap {
            start_ms,
            end_ms,
            confidence: 0.9,
            reason: "r".into(),
            priority: 3,
            tags: vec!["ambience".into()],
        }
    }

    #[test]
    fn short_gap_is_untouched() {
        let out = split_gap_windows(&[gap(1_000, 9_000)], 15_000);
        assert_eq!(
            (out.len(), out[0].start_ms, out[0].end_ms),
            (1, 1_000, 9_000)
        );
    }

    #[test]
    fn long_gap_tiles_exactly_without_overlap() {
        let out = split_gap_windows(&[gap(10_000, 80_000)], 15_000);
        assert_eq!(out.len(), 5);
        assert_eq!(out[0].start_ms, 10_000);
        assert_eq!(out[4].end_ms, 80_000);
        for pair in out.windows(2) {
            assert_eq!(pair[0].end_ms, pair[1].start_ms);
        }
        assert!(out
            .iter()
            .all(|g| g.end_ms - g.start_ms <= 15_000 && g.tags == ["ambience"]));
    }

    #[test]
    fn inverted_gap_does_not_panic() {
        assert_eq!(split_gap_windows(&[gap(5_000, 1_000)], 15_000).len(), 1);
    }
}
