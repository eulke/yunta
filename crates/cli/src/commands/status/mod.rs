//! `yunta status <run_id>`: where a run stands, derived from the event
//! log alone, never from an estimate or an agent's own report. Two
//! levels — **flow** (the DAG's nodes, over the ones this run's mode
//! schedules) and **task** (tasks done/total) — presented as counters
//! with context, never a percentage: a percentage lies the moment a
//! reroute grows the denominator.
//!
//! A run parked on a person is answerable from here: the decision it
//! stopped on is rebuilt from its own log ([`decision`]) and printed
//! with the command that answers it, so nobody has to read the exported
//! JSONL to learn what the options are.

pub(crate) mod decision;
mod node;
mod page;
pub(crate) mod progress;

use std::path::Path;

use yunta_core::events::StoredEvent;
use yunta_core::Clock;
use yunta_core::{Manifest, RunId};
use yunta_engine::RunPhase;

use crate::commands::advice;
use crate::context::Context;
use crate::error::note;
use crate::error::{CliError, Outcome};
use crate::render::blocks::Drawn;
use crate::render::ink::Line;
use crate::render::INDENT;

/// Prints where `run_id` stands — or, with `node`, that one node whole.
pub async fn status(
    run_id: &RunId,
    json: bool,
    node: Option<&yunta_core::NodeId>,
) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let open = ctx.open_run(run_id).await?;
    let events = open.events;
    let run_dir = open.run_dir;
    let engine = yunta_engine::engine_liveness(&run_dir, &yunta_engine::lock::SystemProbe);
    // What this binary did not understand in a file a later one wrote:
    // said, because a reader acting on a manifest whose newer half is
    // invisible to them should know that is what they are doing.
    if !open.manifest.unknown.is_empty() {
        note(format!(
            "this run's manifest carries {} this binary does not know: {} — a newer yunta \
             wrote it, and what it recorded there is not read here",
            yunta_core::text::counted(open.manifest.unknown.len(), "key"),
            open.manifest.unknown_keys().join(", ")
        ));
    }
    let manifest = open.manifest.doc;

    let now = ctx.clock.now();
    if json {
        return crate::json::print_json(&crate::json::RunDocument::of(
            run_id, &events, &manifest, now, engine,
        ));
    }

    let frame = progress::frame(run_id, &manifest, &events, now);
    if let Some(node) = node {
        let page = node::NodePage {
            run_id,
            frame: &frame,
            run_dir: &run_dir,
            cwd: &ctx.cwd,
            home: ctx.env.home.as_deref(),
        };
        print!("{}", page.render(node, &crate::render::stdout_look())?);
        return Ok(Outcome::Success);
    }
    let state = yunta_engine::derive(&events);
    let page = page::Page {
        run_id,
        frame: &frame,
        state: &state,
        engine,
        run_dir: &run_dir,
        cwd: &ctx.cwd,
        home: ctx.env.home.as_deref(),
    };
    let look = crate::render::stdout_look();
    let paint = |lines: Vec<Line>| -> String {
        lines
            .iter()
            .map(|line| format!("{}\n", look.ink.paint(line)))
            .collect()
    };
    let mut out = paint(page.head());
    let mut nodes = page.nodes().lines(&look);
    nodes.extend(page.calls());
    for part in [
        paint(nodes),
        paint(page.evidence()),
        paint(page.fields().lines(&look)),
    ] {
        if !part.is_empty() {
            out.push('\n');
            out.push_str(&part);
        }
    }
    let tree = ctx.project.run_tree(&manifest, run_id, &ctx.cwd);
    let decided = decision_page((run_id, &manifest, &tree), &events, &frame.phase);
    if let Some(decided) = &decided {
        out.push('\n');
        out.push_str(&decided.text);
    }
    let next = paint(
        page.next(decided.as_ref().is_some_and(|d| d.menu))
            .lines(&look),
    );
    if !next.is_empty() {
        out.push('\n');
        out.push_str(&next);
    }
    print!("{out}");
    Ok(Outcome::Success)
}

/// The decision a parked run waits on, as the page prints it, and
/// whether the log rebuilds a menu for it.
struct Decided {
    text: String,
    menu: bool,
}

/// What a parked run is waiting on, printed after the facts because it
/// is what the reader acts on next: the decision the log reconstructs,
/// with the command that answers it, or — for a pause that reconstructs
/// none — the sentence the frame carries for it and the way back into the
/// run. `None` for a run that is not parked.
fn decision_page(
    (run_id, manifest, tree): (&RunId, &Manifest, &Path),
    events: &[StoredEvent],
    phase: &RunPhase,
) -> Option<Decided> {
    let waiting = advice::parked(phase)?;
    let state = yunta_engine::derive(events);
    Some(match yunta_engine::current_escalation(manifest, &state) {
        Some((node, escalation)) => {
            let look = crate::render::stdout_look();
            // The page's second line made the claim, and a failed
            // node's evidence is quoted above.
            let beside = decision::Beside {
                claim: true,
                evidence: matches!(
                    state.nodes.state(&node),
                    Some(yunta_engine::NodeState::Failed { .. })
                ),
            };
            let block: String = decision::lines(run_id, &node, &escalation, beside, &look)
                .iter()
                .map(|line| format!("{}\n", look.ink.paint(line)))
                .collect();
            Decided {
                // Where a person acts before answering, said once
                // above the decision and whole, so it can be copied
                // into another terminal.
                text: format!("{INDENT}{}\n{block}", decision::run_tree_line(tree)),
                menu: true,
            }
        }
        None => Decided {
            text: decision::without_menu(
                run_id,
                &advice::parked_in_full(waiting),
                crate::render::stdout_width().cells(),
            ),
            menu: false,
        },
    })
}
