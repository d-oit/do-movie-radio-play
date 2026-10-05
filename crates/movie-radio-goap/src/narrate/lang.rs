//! Per-language narration vocabulary (ADR-128: every clause describes
//! detected content). Unknown language codes fall back to German, the
//! original default.

pub(super) struct Phrases {
    pub tag_clauses: &'static [(&'static str, &'static str)],
    pub environment_change: &'static str,
    pub ambiguous_sfx: &'static str,
    pub dialogue_pause: &'static str,
    pub long_passage: &'static str,
    pub fallback: &'static str,
    pub banned_filler: &'static [&'static str],
}

const DE: Phrases = Phrases {
    tag_clauses: &[
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
    ],
    environment_change: "Der Klang wechselt merklich.",
    ambiguous_sfx: "Ein auffälliges Geräusch tritt hervor.",
    dialogue_pause: "Das Gespräch pausiert kurz.",
    long_passage: "Die Passage dauert einige Sekunden.",
    fallback: "Die Handlung läuft ohne Dialog weiter.",
    banned_filler: &["Stille.", "Pause.", "Schnitt.", "Atmosphäre."],
};

const EN: Phrases = Phrases {
    tag_clauses: &[
        ("impact_heavy", "A heavy impact rings out."),
        ("crowd_like", "A crowd murmurs in the background."),
        ("nature_like", "Sounds of nature can be heard."),
        ("tonal", "Melodic tones fill the room."),
        ("music_like", "Melodic tones fill the room."),
        (
            "machinery_like",
            "A steady hum of machinery runs underneath.",
        ),
        ("music_bed", "Music underscores the scene."),
        ("ambience", "A quiet soundscape lies over the scene."),
        ("speech_like", "Muffled voices can be heard."),
    ],
    environment_change: "The sound shifts noticeably.",
    ambiguous_sfx: "A distinct noise stands out.",
    dialogue_pause: "The conversation pauses briefly.",
    long_passage: "The passage lasts several seconds.",
    fallback: "The action continues without dialogue.",
    banned_filler: &["Silence.", "Pause.", "Cut.", "Atmosphere."],
};

pub(super) fn phrases(language: &str) -> &'static Phrases {
    let code = language.trim().to_ascii_lowercase();
    if code == "en" || code.starts_with("en-") || code.starts_with("en_") {
        &EN
    } else {
        &DE
    }
}
