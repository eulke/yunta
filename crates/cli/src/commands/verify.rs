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
use crate::error::{CliError, Outcome};
use crate::render::blocks::{paint, Checklist, Found, Headline};
use crate::render::{Look, Mark};

pub async fn verify(run_id: &RunId) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let storage = ctx.storage()?;
    // The chain first: it refuses a run the log does not know, and it is
    // the one check that reads the rows as bytes rather than as events —
    // so it still answers about a log whose payloads no longer parse.
    let mut checks = Checklist::default();
    chain(run_id, &storage, &mut checks)?;
    objects(run_id, &ctx, &storage, &mut checks).await;
    let holds = checks.holds();
    let (mark, said) = match holds {
        true => (Mark::Done, "intact"),
        false => (Mark::Failed, "broken"),
    };
    let headline = Headline {
        subject: format!("run {}", run_id.handle()),
        mark,
        said: said.to_string(),
    };
    print!("{}", paint(&[&headline, &checks], &Look::stdout()));
    Ok(match holds {
        true => Outcome::Success,
        false => Outcome::Reported,
    })
}

/// Recomputes the run's event hash chain, a check on `checks`.
fn chain(run_id: &RunId, storage: &Storage, checks: &mut Checklist) -> Result<(), CliError> {
    match storage.verify_chain(run_id)? {
        ChainVerification::Intact { events } => checks.push(
            Found::Holds,
            "chain",
            format!(
                "intact — {} verified",
                yunta_core::text::counted(events, "event")
            ),
        ),
        // A broken chain is the `broken` reading of the run: the log can
        // no longer be trusted from this point on.
        ChainVerification::Broken { seq, detail } => checks.push(
            Found::Problem,
            "chain",
            format!("broken at seq {seq} — {detail}"),
        ),
    }
    Ok(())
}

/// Reads back every object the run's log names and puts what the store
/// answered on `checks` — the same verification a resume runs before it
/// wakes the run. Each artifact that is not the bytes the run accepted
/// is a check of its own.
async fn objects(run_id: &RunId, ctx: &Context, storage: &Storage, checks: &mut Checklist) {
    let Some(run_dir) = ctx.project.run_dir(run_id.as_str()) else {
        // The log outlives the directory: `yunta gc` removes a run's
        // directory before purging its events. The chain still answers
        // for the log; the bytes it names are simply no longer here.
        checks.push(
            Found::Caution,
            "objects",
            format!(
                "not checked — no run directory under {} (or the default state root), so the \
                 bytes its log names are not here to read",
                ctx.project.runs_root.display()
            ),
        );
        return;
    };
    // Which objects to read comes from the log itself, so a log that no
    // longer reads back as events leaves nothing to check against —
    // reported as the failing verdict it is.
    let events = match storage.events_for_run(run_id) {
        Ok(events) => events,
        Err(error) => {
            checks.push(
                Found::Problem,
                "objects",
                format!(
                    "not checked — the log does not read back as events, so nothing names \
                     them: {error}"
                ),
            );
            return;
        }
    };

    report(&ArtifactIntegrity::of(&run_dir, &events).await, checks);
}

/// What reading the objects back found, on `checks`.
fn report(integrity: &ArtifactIntegrity, checks: &mut Checklist) {
    match integrity.faults.is_empty() {
        true => checks.push(
            Found::Holds,
            "objects",
            format!(
                "intact — {} verified",
                yunta_core::text::counted(integrity.verified, "artifact")
            ),
        ),
        false => {
            checks.push(
                Found::Problem,
                "objects",
                format!(
                    "broken — {} of {} are not the bytes the run accepted",
                    integrity.faults.len(),
                    yunta_core::text::counted(
                        integrity.verified + integrity.faults.len(),
                        "artifact"
                    )
                ),
            );
            for fault in &integrity.faults {
                checks.push(
                    Found::Problem,
                    fault.artifact.to_string(),
                    fault.error.to_string(),
                );
            }
        }
    }
    if let Some(detail) = integrity.unverifiable_detail() {
        checks.push(Found::Caution, "objects", detail);
    }
}
