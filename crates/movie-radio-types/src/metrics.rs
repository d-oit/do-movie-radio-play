use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StageDurations {
    pub decode_ms: u64,
    pub resample_ms: u64,
    pub framing_ms: u64,
    pub frame_ms: u64,
    pub features_ms: u64,
    pub vad_ms: u64,
    pub smooth_ms: u64,
    pub speech_ms: u64,
    pub segmenter_ms: u64,
    pub merge_ms: u64,
    pub invert_ms: u64,
    pub total_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub input_file: String,
    pub total_ms: u64,
    pub decode_ms: u64,
    pub frame_count: usize,
    pub segment_count: usize,
    pub stage_ms: StageDurations,
    /// Peak resident set size of the process, when the platform reports it.
    ///
    /// Long-form work is bounded by memory, not only by time, so a benchmark
    /// that reports only durations cannot show whether a change kept peak
    /// memory flat as the input grows (#418).
    #[serde(default)]
    pub peak_rss_kb: Option<u64>,
}

/// Peak resident set size of this process in KiB.
///
/// Reads `VmHWM` from `/proc/self/status`, which is the kernel's own
/// high-water mark and therefore needs no sampling. Returns `None` on
/// platforms without procfs rather than guessing.
pub fn peak_rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    parse_peak_rss_kb(&status)
}

/// Extracts `VmHWM` from the text of a `/proc/self/status` file.
///
/// Split out so the parsing is testable on hosts whose procfs omits the
/// field, such as sandboxed runners.
pub fn parse_peak_rss_kb(status: &str) -> Option<u64> {
    status.lines().find_map(|line| {
        let rest = line.strip_prefix("VmHWM:")?;
        rest.split_whitespace().next()?.parse().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vmhwm_from_proc_status() {
        let status =
            "Name:\tproc\nVmPeak:\t  123456 kB\nVmHWM:\t  680396 kB\nVmRSS:\t  100000 kB\n";
        assert_eq!(parse_peak_rss_kb(status), Some(680_396));
    }

    #[test]
    fn missing_vmhwm_is_none() {
        let status = "Name:\tproc\nVmRSS:\t  4464 kB\n";
        assert_eq!(parse_peak_rss_kb(status), None);
    }

    #[test]
    fn malformed_vmhwm_is_none() {
        assert_eq!(parse_peak_rss_kb("VmHWM:\t not-a-number kB\n"), None);
        assert_eq!(parse_peak_rss_kb(""), None);
    }

    #[test]
    fn benchmark_result_serializes_peak_rss() {
        let result = BenchmarkResult {
            input_file: "film.mp4".to_string(),
            total_ms: 10,
            decode_ms: 4,
            frame_count: 2,
            segment_count: 1,
            stage_ms: StageDurations::default(),
            peak_rss_kb: Some(680_396),
        };
        let json = serde_json::to_string(&result).expect("serialize");
        assert!(json.contains("\"peak_rss_kb\":680396"), "{json}");

        // Older benchmark files predate the field and must still load.
        let legacy = r#"{"input_file":"film.mp4","total_ms":10,"decode_ms":4,
            "frame_count":2,"segment_count":1,"stage_ms":{"decode_ms":0,
            "resample_ms":0,"framing_ms":0,"frame_ms":0,"features_ms":0,
            "vad_ms":0,"smooth_ms":0,"speech_ms":0,"segmenter_ms":0,
            "merge_ms":0,"invert_ms":0,"total_ms":0}}"#;
        let decoded: BenchmarkResult = serde_json::from_str(legacy).expect("deserialize legacy");
        assert_eq!(decoded.peak_rss_kb, None);
    }
}
