//! `yunta status <run_id>`: where a run stands, derived from the event
//! log alone, never from an estimate or an agent's own report. Two
//! levels — **flow** (the DAG's nodes, over the ones this run's mode
//! schedules) and **task** (tasks done/total) — presented as counters
//! with context, never a percentage: a percentage lies the moment a
//! reroute grows the denominator.
//!
//! A run parked on a person is answerable from here: the decision it
//! stopped on is rebuilt from its own log and printed, as
//! `yunta_render::escalation::Decision` draws it, with the command that
//! answers it, so nobody has to read the exported JSONL to learn what the
//! options are. The page is the API's, read whole; this command prints it,
//! or the run's document for a program. A run that moves is followed with
//! `--follow` ([`follow`]): what `yunta run` draws, from another process.

pub(crate) mod follow;

use surco::doc::Depth;
use surco::look::Look;
use yunta_api::Project;
use yunta_core::{NodeId, RunId};

use crate::error::CliError;
use crate::output::{print_json, show, Mode, Outcome};
use crate::shell::Shell;

/// Prints where `run_id` stands, as deep as `depth` asks — or, with
/// `node`, that one node whole. A program reads the run's document under
/// `Mode::Json`, whatever `node` names. Whatever the run is doing, `yunta
/// status` reports success: what it tells a reader is the run's state, not
/// the command's.
pub async fn status(
    shell: &Shell,
    run_id: &RunId,
    output: Mode,
    (node, depth): (Option<&NodeId>, Depth),
    look: &Look,
) -> Result<Outcome, CliError> {
    let project = shell.workspace().project()?;
    let open = project.run(run_id).await?;
    let remarks = |remark| crate::remarks::say(remark, output, look);
    match (output, node) {
        (Mode::Human, Some(node)) => show(look, &open.node_page(node, &remarks).await?),
        (Mode::Human, None) => show(look, &open.page(depth, &remarks).await?),
        (Mode::Json, _) => {
            print_json(&open.document(&remarks).await?)?;
        }
        (Mode::Quiet, _) => {
            open.document(&remarks).await?;
        }
    }
    Ok(Outcome::Success)
}

/// Prints the page `yunta status <run>` prints a person, and hands back the
/// word the run is called by on it: what a caller that exits with the run's
/// code reads.
pub(crate) async fn print_page(
    project: &Project,
    run_id: &RunId,
    look: &Look,
) -> Result<surco::words::Standing, CliError> {
    let remarks = |remark| crate::remarks::say(remark, Mode::Human, look);
    let page = project
        .run(run_id)
        .await?
        .page(Depth::Glance, &remarks)
        .await?;
    show(look, &page);
    Ok(page.frame.standing)
}
