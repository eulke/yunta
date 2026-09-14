//! `yunta verify <run_id>`: the two mechanical guarantees a run makes
//! about its own evidence, checked and reported apart.
//!
//! The **event chain** covers the log: every link recomputed from the
//! bytes as persisted, so an altered payload or a deleted, inserted or
//! reordered event surfaces with the seq it begins at. Integrity and
//! order only — never authenticity, which is a separate layer.
//!
//! The **objects** cover the artifacts: every acceptance on that log
//! names bytes under `objects/`, and each one is read back and hashed
//! against its own name. The two are independent — a corrupt object
//! leaves the chain intact and an altered event leaves the objects
//! alone — so each answers on its own line, and either one failing is a
//! failing verdict.

use yunta_core::RunId;
use yunta_engine::ArtifactIntegrity;
use yunta_storage::{ChainVerification, Storage};

use crate::context::Context;
use crate::error::{note, CliError, Outcome};

pub async fn verify(run_id: &RunId) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let storage = ctx.storage()?;
    // The chain first, and reported before anything else is attempted:
    // it refuses a run the log does not know, and it is the one check
    // that reads the rows as bytes rather than as events — so it still
    // answers about a log whose payloads no longer parse.
    let chain = chain(run_id, &storage)?;
    let objects = objects(run_id, &ctx, &storage).await?;
    Ok(match (chain, objects) {
        (Outcome::Success, Outcome::Success) => Outcome::Success,
        _ => Outcome::Reported,
    })
}

/// Recomputes the run's event hash chain and reports it.
fn chain(run_id: &RunId, storage: &Storage) -> Result<Outcome, CliError> {
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

/// Reads back every object the run's log names and reports what the
/// store answered — the same verification a resume runs before it wakes
/// the run.
async fn objects(run_id: &RunId, ctx: &Context, storage: &Storage) -> Result<Outcome, CliError> {
    let Some(run_dir) = ctx.project.run_dir(run_id.as_str()) else {
        // The log outlives the directory: `yunta gc` removes a run's
        // directory before purging its events. The chain still answers
        // for the log; the bytes it names are simply no longer here.
        note(format!(
            "run {run_id}: objects not checked — no run directory under {} (or the default state \
             root), so the bytes its log names are not here to read",
            ctx.project.runs_root.display()
        ));
        return Ok(Outcome::Success);
    };
    // Which objects to read comes from the log itself, so a log that no
    // longer reads back as events leaves nothing to check against —
    // reported as the failing verdict it is.
    let events = match storage.events_for_run(run_id) {
        Ok(events) => events,
        Err(error) => {
            note(format!(
                "run {run_id}: objects not checked — the log does not read back as events, so \
                 nothing names them: {error}"
            ));
            return Ok(Outcome::Reported);
        }
    };

    let integrity = ArtifactIntegrity::of(&run_dir, &events).await;
    let verdict = if integrity.faults.is_empty() {
        println!(
            "run {run_id}: objects intact — {} artifact(s) verified",
            integrity.verified
        );
        Outcome::Success
    } else {
        note(format!(
            "run {run_id}: objects BROKEN — {} of {} artifact(s) are not the bytes the run \
             accepted",
            integrity.faults.len(),
            integrity.verified + integrity.faults.len()
        ));
        for fault in &integrity.faults {
            note(format!("  {}: {}", fault.artifact, fault.error));
        }
        Outcome::Reported
    };
    if let Some(detail) = integrity.unverifiable_detail() {
        note(format!("run {run_id}: {detail}"));
    }
    Ok(verdict)
}
