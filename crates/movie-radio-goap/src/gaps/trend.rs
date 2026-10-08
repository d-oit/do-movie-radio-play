use movie_radio_types::GapTrend;

/// Shortest span with a meaningful development; shorter gaps read as one sound.
const MIN_SPAN_MS: u64 = 3_000;
/// Level change between the first and last third that counts as a trend.
const MIN_CHANGE_DB: f32 = 4.0;
/// Below this RMS both ends are practically inaudible; a ratio would be noise.
const AUDIBLE_RMS: f32 = 0.003;

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Classifies how loudness develops across `[start_ms, end_ms)` by comparing
/// the first and last third. Deterministic and based only on the samples.
pub fn energy_trend(
    samples: &[f32],
    sample_rate: u32,
    start_ms: u64,
    end_ms: u64,
) -> Option<GapTrend> {
    if end_ms.saturating_sub(start_ms) < MIN_SPAN_MS || sample_rate == 0 {
        return None;
    }
    let at = |ms: u64| ((ms as u128 * u128::from(sample_rate) / 1000) as usize).min(samples.len());
    let (start, end) = (at(start_ms), at(end_ms));
    let third = end.saturating_sub(start) / 3;
    if third == 0 {
        return None;
    }
    let first = rms(&samples[start..start + third]);
    let last = rms(&samples[end - third..end]);
    if first.max(last) < AUDIBLE_RMS {
        return None;
    }
    let change_db = 20.0 * (last.max(1e-6) / first.max(1e-6)).log10();
    if change_db >= MIN_CHANGE_DB {
        Some(GapTrend::Rising)
    } else if change_db <= -MIN_CHANGE_DB {
        Some(GapTrend::Falling)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 1_000;

    fn ramp(from: f32, to: f32, secs: usize) -> Vec<f32> {
        let n = secs * SR as usize;
        (0..n)
            .map(|i| from + (to - from) * i as f32 / n as f32)
            .collect()
    }

    #[test]
    fn rising_falling_and_steady_are_distinguished() {
        assert_eq!(
            energy_trend(&ramp(0.02, 0.4, 9), SR, 0, 9_000),
            Some(GapTrend::Rising)
        );
        assert_eq!(
            energy_trend(&ramp(0.4, 0.02, 9), SR, 0, 9_000),
            Some(GapTrend::Falling)
        );
        assert_eq!(energy_trend(&vec![0.1; 9_000], SR, 0, 9_000), None);
    }

    #[test]
    fn short_inaudible_or_out_of_range_spans_have_no_trend() {
        assert_eq!(energy_trend(&ramp(0.01, 0.5, 2), SR, 0, 2_000), None);
        assert_eq!(energy_trend(&ramp(0.0001, 0.002, 9), SR, 0, 9_000), None);
        assert_eq!(energy_trend(&ramp(0.02, 0.4, 9), SR, 50_000, 60_000), None);
        assert_eq!(energy_trend(&[], 0, 0, 9_000), None);
    }
}
