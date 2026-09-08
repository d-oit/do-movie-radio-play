pub mod pipeline;
pub mod quality;

pub use pipeline::{
    AssembleRadioPlay, DecodeMovie, ExtractTimeline, GenerateNarration, IdentifyVisualGaps,
    SynthesizeNarrator,
};
pub use quality::{ApplyLearnings, VerifyQuality};

use crate::Action;

pub fn get_all_actions() -> Vec<Box<dyn Action>> {
    vec![
        Box::new(DecodeMovie),
        Box::new(ExtractTimeline),
        Box::new(IdentifyVisualGaps),
        Box::new(GenerateNarration),
        Box::new(SynthesizeNarrator),
        Box::new(AssembleRadioPlay),
        Box::new(VerifyQuality),
        Box::new(ApplyLearnings),
    ]
}

#[cfg(test)]
mod tests {
    use super::pipeline::build_narration_segments;
    use super::*;
    use crate::narrate::NarrationScript;
    use crate::{PipelineContext, WorldState};
    use movie_radio_voice::Emotion;

    fn script(gap_start_ms: u64) -> NarrationScript {
        NarrationScript {
            gap_start_ms,
            gap_end_ms: gap_start_ms + 5_000,
            text: "Hallo Welt".to_string(),
            emotion: Emotion::Neutral,
            word_count: 2,
            duration_ms: 1_000,
        }
    }

    fn audio(len_samples: usize) -> Option<movie_radio_voice::AudioOutput> {
        Some(movie_radio_voice::AudioOutput {
            samples: vec![0.25; len_samples],
            sample_rate_hz: 16_000,
        })
    }

    #[test]
    fn test_segments_keep_script_alignment_across_failures() {
        let scripts = vec![script(1_000), script(2_000), script(3_000)];
        let narration = vec![audio(800), None, audio(1_600)];
        let assembler = crate::assemble::RadioPlayAssembler::new(16_000, 50, 0.3);

        let segments = build_narration_segments(&scripts, &narration, &assembler);

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].start_sample, 16_000);
        assert_eq!(segments[0].samples.len(), 800);
        assert_eq!(segments[1].start_sample, 48_000);
        assert_eq!(segments[1].samples.len(), 1_600);
    }

    #[tokio::test]
    async fn test_synthesize_narrator_bails_when_all_fail() {
        std::env::remove_var("MODAL_TTS_ENDPOINT");
        let mut ctx = PipelineContext::new(
            std::path::PathBuf::from("movie.mp4"),
            std::path::PathBuf::from("/tmp/opencode/out.wav"),
        );
        ctx.scripts = Some(vec![script(500), script(6_000)]);

        let result = SynthesizeNarrator.execute(&mut ctx).await;

        let err = result.expect_err("total synthesis failure must not pass silently");
        assert!(err.to_string().contains("all 2 narration syntheses failed"));
        assert_eq!(ctx.narration_audio.len(), 2);
        assert!(ctx.narration_audio.iter().all(Option::is_none));
    }

    #[tokio::test]
    async fn test_verify_quality_and_apply_learnings() {
        let mut ctx = PipelineContext::new(
            std::path::PathBuf::from("nonexistent_movie.mp4"),
            std::path::PathBuf::from("/tmp/test_out.wav"),
        );

        let vq = VerifyQuality;
        let al = ApplyLearnings;

        assert_eq!(vq.name(), "verify_quality");
        assert_eq!(al.name(), "apply_learnings");

        let start = WorldState::default();
        let after_vq = vq.apply(&start);
        assert!(after_vq.quality_verified);

        let after_al = al.apply(&after_vq);
        assert!(after_al.learnings_applied);

        vq.execute(&mut ctx).await.unwrap();
        assert_eq!(ctx.quality_score, 1.0);

        al.execute(&mut ctx).await.unwrap();
        assert!(ctx.learning_state.is_some());
    }
}
