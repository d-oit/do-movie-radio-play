//! What the default (acoustic) narration path actually says for a real film.
//!
//! This exists to answer one question with evidence rather than by reading the
//! templates: for a film whose gaps carry no acoustic tags, what does a listener
//! hear? Run with:
//!
//! ```text
//! cargo test -p movie-radio-goap --test narration_baseline -- --ignored --nocapture
//! ```
//!
//! It is `#[ignore]`d because it needs a checked-in detector timeline rather than
//! a synthetic fixture, so it is a research tool, not a CI gate.

use std::path::Path;

use movie_radio_goap::gaps::GapIdentifier;
use movie_radio_goap::narrate::NarrationGenerator;
use movie_radio_types::TimelineOutput;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

#[test]
#[ignore = "research tool: needs a checked-in detector timeline, not a CI gate"]
fn acoustic_narration_for_a_real_film() {
    // Default is the raw detector timeline, which carries no tags. Production
    // re-derives tags from the window audio (`retag_windows`), so pass a tagged
    // timeline via NARRATION_TIMELINE to see what a listener actually gets.
    let timeline_path = std::env::var_os("NARRATION_TIMELINE").map_or_else(
        || repo_root().join("analysis/validation/detector/sprite_fright_2021_en.json"),
        std::path::PathBuf::from,
    );
    let raw = std::fs::read_to_string(&timeline_path).expect("read detector timeline");
    let timeline: TimelineOutput = serde_json::from_str(&raw).expect("parse timeline");

    let mut identifier = GapIdentifier::new();
    // The stored timeline's confidences sit below the 0.8 default.
    identifier.high_confidence_threshold = 0.4;
    let analysis = identifier
        .identify_gaps(&timeline, None)
        .expect("identify gaps");

    let mut generator = NarrationGenerator::new(0.5);
    generator.language = "en".to_string();
    let scripts = generator
        .generate(&timeline, &analysis.gaps)
        .expect("generate narration");

    println!(
        "\nSprite Fright: {} gaps -> {} narration lines\n",
        analysis.gaps.len(),
        scripts.len()
    );
    for script in &scripts {
        println!(
            "  {:>7.1}s-{:>7.1}s  {:?}",
            script.gap_start_ms as f64 / 1000.0,
            script.gap_end_ms as f64 / 1000.0,
            script.text
        );
    }

    let distinct: std::collections::BTreeSet<&str> =
        scripts.iter().map(|s| s.text.as_str()).collect();
    println!(
        "\n  distinct lines: {} of {}",
        distinct.len(),
        scripts.len()
    );
    for line in &distinct {
        println!("    - {line}");
    }
}
