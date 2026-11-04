//! CLI surface for the human-in-the-loop review feedback workflow.

use clap::Subcommand;
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum ReviewFeedbackCommands {
    /// Import human review feedback for narration and gap decisions.
    Import {
        input: PathBuf,
        #[arg(long, default_value = "analysis/thresholds/learning.db")]
        learning_db: PathBuf,
        /// Reviewer identity applied when a decision omits one.
        #[arg(long)]
        reviewer: Option<String>,
        /// Validate and report without writing.
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Show proposed learning changes derived from review feedback.
    Proposals {
        #[arg(long, default_value = "analysis/thresholds/learning.db")]
        learning_db: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Apply a proposed learning change (requires --confirm).
    Apply {
        #[arg(long)]
        proposal_id: String,
        #[arg(long)]
        confirm: bool,
        #[arg(long, default_value = "analysis/thresholds/learning.db")]
        learning_db: PathBuf,
    },
    /// Roll back an applied learning change (requires --confirm).
    Rollback {
        #[arg(long)]
        proposal_id: String,
        #[arg(long)]
        confirm: bool,
        #[arg(long, default_value = "analysis/thresholds/learning.db")]
        learning_db: PathBuf,
    },
    /// Show review outcome metrics by film, language, genre, VAD engine and cue source.
    Metrics {
        #[arg(long, default_value = "analysis/thresholds/learning.db")]
        learning_db: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Register an evaluation holdout fixture that learning must never write.
    RegisterHoldout {
        #[arg(long)]
        fixture_id: String,
        #[arg(long)]
        path: PathBuf,
        #[arg(long, default_value = "analysis/thresholds/learning.db")]
        learning_db: PathBuf,
    },
    /// Export review decisions and correction events as a feedback batch.
    Export {
        #[arg(long, default_value = "analysis/learnings/review-feedback.json")]
        output: PathBuf,
        #[arg(long, default_value = "analysis/thresholds/learning.db")]
        learning_db: PathBuf,
        /// Include artifact paths instead of redacted file names.
        #[arg(long)]
        include_artifact_paths: bool,
    },
}
