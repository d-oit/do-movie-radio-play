//! Per-language narration vocabulary (ADR-128: every clause describes
//! detected content). Unknown language codes fall back to German, the
//! original default.

pub(super) struct Phrases {
    pub tag_clauses: &'static [(&'static str, &'static [&'static str])],
    pub environment_change: &'static str,
    pub ambiguous_sfx: &'static str,
    pub dialogue_pause: &'static str,
    pub long_passage: &'static str,
    pub fallback: &'static str,
    pub banned_filler: &'static [&'static str],
}

const DE: Phrases = Phrases {
    tag_clauses: &[
        (
            "impact_heavy",
            &[
                "Ein kräftiger Aufprall ertönt.",
                "Ein dumpfer Schlag hallt nach.",
                "Etwas Schweres schlägt krachend auf.",
                "Ein lauter Knall durchbricht die Szene.",
            ],
        ),
        (
            "crowd_like",
            &[
                "Eine Menschenmenge murmelt im Hintergrund.",
                "Stimmengewirr füllt den Raum.",
                "Viele Menschen reden durcheinander.",
            ],
        ),
        (
            "nature_like",
            &[
                "Naturgeräusche sind zu hören.",
                "Wind und Blätter rauschen leise.",
                "Draußen regt sich die Natur.",
            ],
        ),
        (
            "tonal",
            &[
                "Melodische Klänge erfüllen den Raum.",
                "Eine Melodie schwingt durch die Szene.",
                "Sanfte Töne klingen nach.",
            ],
        ),
        (
            "music_like",
            &[
                "Melodische Klänge erfüllen den Raum.",
                "Eine Melodie schwingt durch die Szene.",
                "Sanfte Töne klingen nach.",
            ],
        ),
        (
            "machinery_like",
            &[
                "Ein gleichmäßiges Maschinengeräusch läuft mit.",
                "Ein Motor brummt im Hintergrund.",
                "Mechanisches Surren liegt in der Luft.",
            ],
        ),
        (
            "music_bed",
            &[
                "Musik untermalt die Szene.",
                "Leise Musik begleitet das Geschehen.",
                "Ein Musikteppich trägt die Szene.",
            ],
        ),
        (
            "ambience",
            &[
                "Eine ruhige Klangkulisse liegt darüber.",
                "Leise Umgebungsgeräusche füllen den Raum.",
                "Ein gedämpftes Rauschen begleitet die Szene.",
            ],
        ),
        (
            "speech_like",
            &[
                "Gedämpfte Stimmen sind zu hören.",
                "Im Hintergrund murmeln Stimmen.",
                "Leises Sprechen dringt herüber.",
            ],
        ),
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
        (
            "impact_heavy",
            &[
                "A heavy impact rings out.",
                "A dull blow echoes.",
                "Something heavy crashes down.",
                "A loud bang cuts through the scene.",
            ],
        ),
        (
            "crowd_like",
            &[
                "A crowd murmurs in the background.",
                "A babble of voices fills the room.",
                "Many people talk over one another.",
            ],
        ),
        (
            "nature_like",
            &[
                "Sounds of nature can be heard.",
                "Wind and leaves rustle softly.",
                "Outside, nature stirs.",
            ],
        ),
        (
            "tonal",
            &[
                "Melodic tones fill the room.",
                "A melody drifts through the scene.",
                "Soft tones linger.",
            ],
        ),
        (
            "music_like",
            &[
                "Melodic tones fill the room.",
                "A melody drifts through the scene.",
                "Soft tones linger.",
            ],
        ),
        (
            "machinery_like",
            &[
                "A steady hum of machinery runs underneath.",
                "An engine drones in the background.",
                "A mechanical whirr hangs in the air.",
            ],
        ),
        (
            "music_bed",
            &[
                "Music underscores the scene.",
                "Soft music accompanies the action.",
                "A bed of music carries the scene.",
            ],
        ),
        (
            "ambience",
            &[
                "A quiet soundscape lies over the scene.",
                "Faint ambient noise fills the space.",
                "A muffled hiss accompanies the scene.",
            ],
        ),
        (
            "speech_like",
            &[
                "Muffled voices can be heard.",
                "Voices murmur in the background.",
                "Quiet talking drifts over.",
            ],
        ),
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
