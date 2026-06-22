use anyhow::Result;
use serde::{Deserialize, Serialize};

use movie_radio_types::{SegmentKind, TimelineOutput, VisualGap};
use movie_radio_voice::Emotion;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrationScript {
    pub gap_start_ms: u64,
    pub gap_end_ms: u64,
    pub text: String,
    pub emotion: Emotion,
    pub word_count: usize,
    pub duration_ms: u64,
}

pub struct NarrationGenerator {
    pub words_per_minute: f64,
    pub density: f64,
}

impl Default for NarrationGenerator {
    fn default() -> Self {
        Self {
            words_per_minute: 150.0,
            density: 0.5,
        }
    }
}

impl NarrationGenerator {
    pub fn new(density: f64) -> Self {
        Self {
            density: density.clamp(0.0, 1.0),
            ..Default::default()
        }
    }

    pub fn generate(
        &self,
        timeline: &TimelineOutput,
        gaps: &[VisualGap],
    ) -> Result<Vec<NarrationScript>> {
        let mut scripts = Vec::new();

        for gap in gaps {
            if gap.confidence < 0.4 {
                continue;
            }

            let gap_duration_ms = gap.end_ms.saturating_sub(gap.start_ms);
            if gap_duration_ms < 1000 {
                continue;
            }

            let max_words = self.max_words_for_duration(gap_duration_ms);
            if max_words == 0 {
                continue;
            }

            let context = self.extract_context(timeline, gap);
            let emotion = self.infer_emotion(&context);
            let text = self.generate_text(&context, max_words);

            if text.is_empty() {
                continue;
            }

            let word_count = text.split_whitespace().count();
            let duration_ms = self.estimate_duration_ms(word_count);

            scripts.push(NarrationScript {
                gap_start_ms: gap.start_ms,
                gap_end_ms: gap.end_ms,
                text,
                emotion,
                word_count,
                duration_ms,
            });
        }

        Ok(scripts)
    }

    fn max_words_for_duration(&self, duration_ms: u64) -> usize {
        let seconds = duration_ms as f64 / 1000.0;
        let raw_words = (seconds * self.words_per_minute / 60.0) * self.density;
        raw_words.floor() as usize
    }

    fn estimate_duration_ms(&self, word_count: usize) -> u64 {
        let seconds = word_count as f64 / self.words_per_minute * 60.0;
        (seconds * 1000.0) as u64
    }

    fn extract_context(&self, timeline: &TimelineOutput, gap: &VisualGap) -> GapContext {
        let mut context = GapContext::default();

        for seg in &timeline.segments {
            if seg.end_ms <= gap.start_ms {
                context.before_tags.extend(seg.tags.clone());
                context.before_kind = Some(seg.kind.clone());
            } else if seg.start_ms >= gap.end_ms {
                context.after_tags.extend(seg.tags.clone());
                context.after_kind = Some(seg.kind.clone());
                break;
            }
        }

        context.gap_duration_ms = gap.end_ms.saturating_sub(gap.start_ms);
        context.gap_reason = gap.reason.clone();

        context
    }

    fn infer_emotion(&self, context: &GapContext) -> Emotion {
        let all_tags: Vec<&str> = context
            .before_tags
            .iter()
            .chain(context.after_tags.iter())
            .map(|s| s.as_str())
            .collect();

        if all_tags.iter().any(|t| *t == "impact_heavy") {
            return Emotion::Tense;
        }
        if all_tags.iter().any(|t| *t == "machinery_like") {
            return Emotion::Neutral;
        }
        if all_tags.iter().any(|t| *t == "music_bed") {
            return Emotion::Mysterious;
        }
        if context.gap_duration_ms > 8000 {
            return Emotion::Mysterious;
        }

        Emotion::Neutral
    }

    fn generate_text(&self, context: &GapContext, max_words: usize) -> String {
        let templates = self.select_templates(context);

        if templates.is_empty() {
            return String::new();
        }

        let hash = self.context_hash(context);
        let idx = hash % templates.len();
        let template = templates[idx];

        self.fit_to_budget(template, max_words)
    }

    fn select_templates<'a>(&self, context: &GapContext) -> Vec<&'a str> {
        let mut templates = Vec::new();

        if context.gap_duration_ms > 5000 {
            templates.push("Stille.");
            templates.push("Pause.");
        }

        if context.before_tags.iter().any(|t| t == "ambience")
            || context.after_tags.iter().any(|t| t == "ambience")
        {
            templates.push("Atmosphäre.");
        }

        if context.gap_reason.contains("environment change") {
            templates.push("Schnitt.");
        }

        if context.before_tags.iter().any(|t| t == "impact_heavy")
            || context.after_tags.iter().any(|t| t == "impact_heavy")
        {
            templates.push("Ein Geräusch ertönt.");
        }

        if context.gap_duration_ms > 3000 {
            templates.push("Stille.");
            templates.push("Pause.");
        }

        if templates.is_empty() {
            templates.push("Stille.");
        }

        templates
    }

    fn context_hash(&self, context: &GapContext) -> usize {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        context.gap_duration_ms.hash(&mut hasher);
        context.gap_reason.hash(&mut hasher);
        for tag in &context.before_tags {
            tag.hash(&mut hasher);
        }
        for tag in &context.after_tags {
            tag.hash(&mut hasher);
        }
        hasher.finish() as usize
    }

    fn fit_to_budget(&self, text: &str, max_words: usize) -> String {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.len() <= max_words {
            return text.to_string();
        }
        words[..max_words].join(" ")
    }
}

#[derive(Debug, Default)]
struct GapContext {
    before_tags: Vec<String>,
    after_tags: Vec<String>,
    before_kind: Option<SegmentKind>,
    after_kind: Option<SegmentKind>,
    gap_duration_ms: u64,
    gap_reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use movie_radio_types::{Segment, SegmentKind, TimelineOutput, VisualGap};

    fn make_timeline(segments: Vec<Segment>) -> TimelineOutput {
        TimelineOutput {
            file: "test.wav".to_string(),
            analysis_sample_rate: 16000,
            frame_ms: 20,
            segments,
        }
    }

    #[test]
    fn test_max_words_for_duration() {
        let gen = NarrationGenerator::new(1.0);
        // 3 seconds at 150 wpm = 7.5 words
        assert_eq!(gen.max_words_for_duration(3000), 7);
        // 1 second at 150 wpm = 2.5 words
        assert_eq!(gen.max_words_for_duration(1000), 2);
    }

    #[test]
    fn test_fit_to_budget() {
        let gen = NarrationGenerator::default();
        assert_eq!(gen.fit_to_budget("Stille.", 5), "Stille.");
        assert_eq!(gen.fit_to_budget("Ein Geräusch ertönt.", 2), "Ein Geräusch");
    }

    #[test]
    fn test_generate_with_gap() {
        let gen = NarrationGenerator::new(0.5);
        let timeline = make_timeline(vec![
            Segment {
                start_ms: 0,
                end_ms: 1000,
                kind: SegmentKind::Speech,
                confidence: 1.0,
                tags: vec![],
                prompt: None,
            },
            Segment {
                start_ms: 1000,
                end_ms: 5000,
                kind: SegmentKind::NonVoice,
                confidence: 1.0,
                tags: vec!["ambience".to_string()],
                prompt: None,
            },
            Segment {
                start_ms: 5000,
                end_ms: 6000,
                kind: SegmentKind::Speech,
                confidence: 1.0,
                tags: vec![],
                prompt: None,
            },
        ]);

        let gaps = vec![VisualGap {
            start_ms: 1000,
            end_ms: 5000,
            confidence: 0.8,
            reason: "Extended ambience".to_string(),
            priority: 5,
        }];

        let scripts = gen.generate(&timeline, &gaps).unwrap();
        assert_eq!(scripts.len(), 1);
        assert!(!scripts[0].text.is_empty());
        assert!(scripts[0].word_count <= 7);
    }

    #[test]
    fn test_low_confidence_gap_skipped() {
        let gen = NarrationGenerator::default();
        let timeline = make_timeline(vec![]);
        let gaps = vec![VisualGap {
            start_ms: 0,
            end_ms: 5000,
            confidence: 0.2,
            reason: "weak".to_string(),
            priority: 1,
        }];

        let scripts = gen.generate(&timeline, &gaps).unwrap();
        assert!(scripts.is_empty());
    }
}
