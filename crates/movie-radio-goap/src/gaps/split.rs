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

/// Removes every subtitle cue (widened by `pad_ms`) from the gaps. Cues are
/// exact timed dialogue, so unlike detector boundaries they are a hard veto:
/// a gap overlapping a cue keeps only the stretches outside it. Pieces shorter
/// than `min_piece_ms` are dropped (nothing useful fits). Input gaps need not
/// be sorted; output is ordered by start time.
pub fn subtract_cues(
    gaps: &[VisualGap],
    cues: &[(u64, u64)],
    pad_ms: u64,
    min_piece_ms: u64,
) -> Vec<VisualGap> {
    let mut blocked: Vec<(u64, u64)> = cues
        .iter()
        .map(|&(s, e)| (s.saturating_sub(pad_ms), e.saturating_add(pad_ms)))
        .collect();
    blocked.sort_unstable();
    let mut out = Vec::new();
    for gap in gaps {
        let mut cursor = gap.start_ms;
        let mut pieces = Vec::new();
        for &(s, e) in &blocked {
            if e <= cursor || s >= gap.end_ms {
                continue;
            }
            if s > cursor {
                pieces.push((cursor, s));
            }
            cursor = cursor.max(e);
        }
        if cursor < gap.end_ms {
            pieces.push((cursor, gap.end_ms));
        }
        out.extend(
            pieces
                .into_iter()
                .filter(|(s, e)| e - s >= min_piece_ms)
                .map(|(start_ms, end_ms)| VisualGap {
                    start_ms,
                    end_ms,
                    ..gap.clone()
                }),
        );
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

    #[test]
    fn cue_in_the_middle_splits_the_gap() {
        let out = subtract_cues(&[gap(0, 20_000)], &[(8_000, 12_000)], 500, 1_000);
        let spans: Vec<_> = out.iter().map(|g| (g.start_ms, g.end_ms)).collect();
        assert_eq!(spans, vec![(0, 7_500), (12_500, 20_000)]);
    }

    #[test]
    fn overlapping_cues_and_short_remainders_are_dropped() {
        let cues = [(1_000, 5_000), (4_000, 9_500)];
        let out = subtract_cues(&[gap(0, 10_000)], &cues, 0, 1_000);
        // 0..1000 is exactly the minimum; 9500..10000 is too short.
        let spans: Vec<_> = out.iter().map(|g| (g.start_ms, g.end_ms)).collect();
        assert_eq!(spans, vec![(0, 1_000)]);
    }

    #[test]
    fn gap_fully_inside_a_cue_vanishes_and_clear_gap_is_untouched() {
        let out = subtract_cues(
            &[gap(2_000, 6_000), gap(30_000, 40_000)],
            &[(1_000, 7_000)],
            0,
            1_000,
        );
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].start_ms, out[0].end_ms), (30_000, 40_000));
    }
}
