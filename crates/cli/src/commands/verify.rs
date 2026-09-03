//! `yunta verify <run_id>`: walks the run's event hash chain
//! recomputing every link from the bytes as persisted. Integrity and
//! order only — never authenticity, which is a separate layer.

use yunta_core::RunId;
use yunta_storage::ChainVerification;

use crate::context::Context;
use crate::error::{note, CliError, Outcome};

pub fn verify(run_id: &RunId) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let storage = ctx.storage()?;
    match storage.verify_chain(run_id)? {
        ChainVerification::Intact { events } => {
            println!("run {run_id}: chain intact — {events} event(s) verified");
            Ok(Outcome::Success)
        }
        ChainVerification::Broken { seq, detail } => {
            // A broken chain is the `broken` reading of the run: the
            // log can no longer be trusted from this point on.
            note(format!(
                "run {run_id}: chain BROKEN at seq {seq} — {detail}"
            ));
            Ok(Outcome::Reported)
        }
    }
}
