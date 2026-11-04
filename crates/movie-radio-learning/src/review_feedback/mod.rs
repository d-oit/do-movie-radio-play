//! Human-in-the-loop review feedback for narration and gap decisions.
//!
//! Review decisions are imported as immutable correction events, kept apart
//! from evaluation holdouts, and only become learned changes through an
//! explicit, versioned apply step.

mod hash;
mod holdout;
mod proposal;
mod proposal_store;
mod queries;
mod store;
mod types;

#[cfg(test)]
mod tests;

pub use hash::checksum_file;
pub use types::*;

use anyhow::Result;
use libsql::Connection;

pub(crate) async fn create_review_tables(conn: &Connection) -> Result<()> {
    store::create_review_tables(conn).await?;
    holdout::create_holdout_tables(conn).await?;
    proposal::create_proposal_tables(conn).await?;
    Ok(())
}
