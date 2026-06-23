use movie_radio_types::AnalysisConfig;

use super::{
    FILTER_MERGE_STRATEGY, MAX_FILTER_MIN_NON_VOICE_MS, MAX_RESIDUAL_BRIDGE_GAP_MS,
    NON_SPARSE_AMBIGUOUS_EXPAND_MAX_MS,
};

pub(crate) fn should_apply_verification_filter(cfg: &AnalysisConfig) -> bool {
    cfg.min_non_voice_ms <= MAX_FILTER_MIN_NON_VOICE_MS
        && cfg
            .merge_options
            .as_ref()
            .map(|opts| opts.merge_strategy == FILTER_MERGE_STRATEGY)
            .unwrap_or(false)
}

pub(crate) fn residual_bridge_gap_ms(cfg: &AnalysisConfig) -> u64 {
    let Some(options) = cfg.merge_options.as_ref() else {
        return MAX_RESIDUAL_BRIDGE_GAP_MS;
    };
    if options.merge_strategy == FILTER_MERGE_STRATEGY {
        MAX_RESIDUAL_BRIDGE_GAP_MS
    } else {
        options.min_gap_to_merge.max(options.min_silence_duration) as u64
    }
}

pub(crate) fn ambiguous_expand_max_ms(cfg: &AnalysisConfig) -> Option<u64> {
    let Some(options) = cfg.merge_options.as_ref() else {
        return Some(NON_SPARSE_AMBIGUOUS_EXPAND_MAX_MS);
    };
    if options.merge_strategy == FILTER_MERGE_STRATEGY {
        None
    } else {
        Some(NON_SPARSE_AMBIGUOUS_EXPAND_MAX_MS)
    }
}

pub(crate) fn should_apply_speech_evidence_filter(cfg: &AnalysisConfig) -> bool {
    cfg.merge_options
        .as_ref()
        .map(|opts| opts.merge_strategy == FILTER_MERGE_STRATEGY)
        .unwrap_or(false)
}
