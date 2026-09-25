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
            let text = reject_banned_filler(self.generate_text(&context, max_words));

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

        // The gap's own detected audio tags (from the `tag` pipeline stage)
        // are the primary grounding signal: they describe what is actually
        // audible during this exact span, which is what the narrator must
        // explain (German audio-description convention: answer "was ist
        // zu hören"). Persisted timelines only ever contain the non-voice
        // segments themselves, so the neighbour loop below cannot see this
        // segment's own tags.
        if let Some(seg) = timeline
            .segments
            .iter()
            .find(|seg| seg.start_ms == gap.start_ms && seg.end_ms == gap.end_ms)
        {
            context.self_tags = seg.tags.clone();
        }

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
        context.gap_reason.clone_from(&gap.reason);

        context
    }

    fn infer_emotion(&self, context: &GapContext) -> Emotion {
        let all_tags: Vec<&str> = context
            .self_tags
            .iter()
            .chain(context.before_tags.iter())
            .chain(context.after_tags.iter())
            .map(|s| s.as_str())
            .collect();

        if all_tags.iter().any(|t| *t == "impact_heavy") {
            return Emotion::Tense;
        }
        if all_tags.iter().any(|t| *t == "crowd_like") {
            return Emotion::Excited;
        }
        if all_tags.iter().any(|t| *t == "nature_like") {
            return Emotion::Joyful;
        }
        if all_tags.iter().any(|t| *t == "tonal" || *t == "music_like") {
            return Emotion::Mysterious;
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

    /// Composes a grounded, present-tense description of what is audible
    /// during the gap instead of a content-free filler word (see German
    /// ARD/MDR audio-description guidelines: answer "was ist zu hören",
    /// present tense, objective, timed to the pause). Fully derived from
    /// input data — no randomness — so identical input yields identical
    /// output. Chunks are whole clauses so budget-fitting never truncates
    /// mid-sentence; the description must stand on its own whether or not
    /// a sound effect ends up mixed in underneath it.
    fn generate_text(&self, context: &GapContext, max_words: usize) -> String {
        let chunks = Self::build_chunks(context);
        self.fit_chunks_to_budget(&chunks, max_words)
    }

    fn build_chunks(context: &GapContext) -> Vec<&'static str> {
        let mut chunks = Vec::new();
        if let Some(clause) = Self::content_clause(context) {
            chunks.push(clause);
        }
        if let Some(clause) = Self::reason_clause(context) {
            chunks.push(clause);
        }
        if context.gap_duration_ms > 8000 {
            chunks.push("Die Passage dauert einige Sekunden.");
        }
        if chunks.is_empty() {
            chunks.push("Die Handlung läuft ohne Dialog weiter.");
        }
        chunks
    }

    /// Primary clause: what is actually audible. Priority order matches
    /// `infer_emotion` (impact dominates, then crowd/nature/tonal/
    /// machinery/music/ambience/speech-like). The gap's own tags are
    /// matched first; neighbouring segments' tags are only a fallback
    /// when the gap itself carries no known tag, so a loud neighbour can
    /// never drown out what the gap actually contains.
    fn content_clause(context: &GapContext) -> Option<&'static str> {
        const TAG_CLAUSES: &[(&str, &str)] = &[
            ("impact_heavy", "Ein kräftiger Aufprall ertönt."),
            ("crowd_like", "Eine Menschenmenge murmelt im Hintergrund."),
            ("nature_like", "Naturgeräusche sind zu hören."),
            ("tonal", "Melodische Klänge erfüllen den Raum."),
            ("music_like", "Melodische Klänge erfüllen den Raum."),
            (
                "machinery_like",
                "Ein gleichmäßiges Maschinengeräusch läuft mit.",
            ),
            ("music_bed", "Musik untermalt die Szene."),
            ("ambience", "Eine ruhige Klangkulisse liegt darüber."),
            ("speech_like", "Gedämpfte Stimmen sind zu hören."),
        ];

        fn first_clause_for(tags: &[&str]) -> Option<&'static str> {
            TAG_CLAUSES
                .iter()
                .find(|(tag, _)| tags.iter().any(|t| t == tag))
                .map(|(_, clause)| *clause)
        }

        let own: Vec<&str> = context.self_tags.iter().map(String::as_str).collect();
        if let Some(clause) = first_clause_for(&own) {
            return Some(clause);
        }
        let neighbour: Vec<&str> = context
            .before_tags
            .iter()
            .chain(context.after_tags.iter())
            .map(String::as_str)
            .collect();
        first_clause_for(&neighbour)
    }

    /// Secondary clause from the structural gap reason, added only when it
    /// conveys information the content clause doesn't already cover.
    fn reason_clause(context: &GapContext) -> Option<&'static str> {
        let reason = context.gap_reason.as_str();
        if reason.contains("environment change") {
            Some("Der Klang wechselt merklich.")
        } else if reason.contains("Ambiguous SFX") {
            Some("Ein auffälliges Geräusch tritt hervor.")
        } else if reason.contains("dialogue blocks") {
            Some("Das Gespräch pausiert kurz.")
        } else {
            None
        }
    }

    /// Greedily includes whole clauses while staying within `max_words`.
    /// The first clause is always attempted in full; only if it alone
    /// exceeds the budget does it fall back to word-level truncation, so
    /// the narration is never empty and rarely cut mid-sentence.
    fn fit_chunks_to_budget(&self, chunks: &[&str], max_words: usize) -> String {
        let mut result = String::new();
        let mut word_count = 0usize;
        for (i, chunk) in chunks.iter().enumerate() {
            let chunk_words = chunk.split_whitespace().count();
            if i == 0 {
                if chunk_words > max_words && max_words > 0 {
                    return self.fit_to_budget(chunk, max_words);
                }
                result.push_str(chunk);
                word_count += chunk_words;
                continue;
            }
            if word_count + chunk_words > max_words {
                break;
            }
            result.push(' ');
            result.push_str(chunk);
            word_count += chunk_words;
        }
        result
    }

    fn fit_to_budget(&self, text: &str, max_words: usize) -> String {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.len() <= max_words {
            return text.to_string();
        }
        words[..max_words].join(" ")
    }
}

/// Phrases that describe nothing (see ADR-128, `plans/adr/0128-audio-
/// description-standards.md`). A German audio-description narrator must
/// name what is happening; these are the historical content-free filler
/// this repo regressed to once already. This runtime check makes that
/// regression structurally unrepeatable: even a future template, backend,
/// or hand-edit that reintroduces a bare filler phrase gets substituted
/// for the safe, still-grounded fallback clause instead of shipping.
const BANNED_FILLER_ONLY: &[&str] = &["Stille.", "Pause.", "Schnitt.", "Atmosphäre."];

/// Fallback used when generated text collapses to a banned filler phrase.
/// Deliberately vague (no tag data available at this call site) but still
/// content-bearing: it states that the scene continues, not that nothing
/// is happening.
const SAFE_FALLBACK: &str = "Die Handlung läuft ohne Dialog weiter.";

fn reject_banned_filler(text: String) -> String {
    if BANNED_FILLER_ONLY.contains(&text.trim()) {
        tracing::warn!(
            rejected = %text,
            "narration text collapsed to banned filler; see ADR-128 (plans/adr/0128-audio-description-standards.md)"
        );
        return SAFE_FALLBACK.to_string();
    }
    text
}

#[derive(Debug, Default)]
struct GapContext {
    self_tags: Vec<String>,
    before_tags: Vec<String>,
    after_tags: Vec<String>,
    before_kind: Option<SegmentKind>,
    after_kind: Option<SegmentKind>,
    gap_duration_ms: u64,
    gap_reason: String,
}

#[cfg(test)]
mod tests;
