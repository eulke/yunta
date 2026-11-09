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
//! options are. A run that moves is followed with
//! `--follow` ([`follow`]): what `yunta run` draws, from another process.

pub(crate) mod follow;

use std::path::PathBuf;

use yunta_api::{OpenedRun, RunReading};
use yunta_core::shown::ShownDocument;
use yunta_core::{NodeId, RunId};
use yunta_render::escalation::Decision;
use yunta_render::page::{Node, Page, Places, Run, RunScope};
use yunta_replay::frame::{NodeFrame, RunFrame};

use crate::context::Context;
use crate::error::CliError;
use crate::output::note;
use crate::output::Outcome;
use crate::output::{emit, Answer, Mode};
use crate::shell::Shell;
use surco::doc::{Depth, Doc};
use surco::look::Look;
use yunta_render::document::RunDocument;

/// Prints where `run_id` stands, as deep as `depth` asks — or, with
/// `node`, that one node whole. A program reads the run's document under
/// `Mode::Json`, whatever `node` names.
pub async fn status(
    shell: &Shell,
    run_id: &RunId,
    output: Mode,
    (node, depth): (Option<&NodeId>, Depth),
    look: &Look,
) -> Result<Outcome, CliError> {
    let ctx = Context::of(shell)?;
    let reading = read(&ctx, run_id).await?;
    let documents = documents(&reading, output, node).await?;
    let body = page_body(&ctx, &reading, &documents, (output, node, depth)).await?;
    let answer = StatusAnswer {
        body,
        reading: &reading,
    };
    emit(output, &answer, look)
}

/// Prints the page `yunta status <run>` prints a person, and hands back the
/// word the run is called by on it: what a caller that exits with the run's
/// code reads.
pub(crate) async fn print_page(
    ctx: &Context,
    run_id: &RunId,
    look: &Look,
) -> Result<surco::words::Standing, CliError> {
    let reading = read(ctx, run_id).await?;
    let documents = documents(&reading, Mode::Human, None).await?;
    let page = (Mode::Human, None, Depth::Glance);
    let body = page_body(ctx, &reading, &documents, page).await?;
    let answer = StatusAnswer {
        body,
        reading: &reading,
    };
    emit(Mode::Human, &answer, look)?;
    Ok(reading.frame.standing)
}

/// The run `run_id`, read at the clock's instant. What this binary does not
/// know in a manifest a newer binary wrote is said first, because a reader
/// acting on a manifest whose newer half is invisible to them should know
/// that is what they are doing.
async fn read<'p>(ctx: &'p Context, run_id: &RunId) -> Result<RunReading<'p>, CliError> {
    let open = ctx.project.run(run_id).await?;
    note_unknown_keys(&open);
    Ok(open.reading().await?)
}

/// Says the keys of `open`'s manifest this binary does not read, when there
/// are any.
fn note_unknown_keys(open: &OpenedRun<'_>) {
    let manifest = &open.manifest;
    if !manifest.unknown.is_empty() {
        note(format!(
            "this run's manifest carries {} this binary does not know ({}): a newer yunta \
             wrote it, and what it recorded there is not read here",
            surco::text::counted(manifest.unknown.len(), "key"),
            manifest.unknown_keys().join(", ")
        ));
    }
}

/// The documents a person's page shows, read once: one node's page shows
/// what the node produced, and the run's page what the decision it waits on
/// shows. A program reads the run's document, which shows none.
async fn documents(
    reading: &RunReading<'_>,
    output: Mode,
    node: Option<&NodeId>,
) -> Result<Vec<ShownDocument>, CliError> {
    match (output, node) {
        (Mode::Human, Some(id)) => Ok(reading.produced(id).await?),
        (Mode::Human, None) => Ok(reading.shown().await?),
        _ => Ok(Vec::new()),
    }
}

/// What a person reads: the run's page, or one node of it.
enum Body<'a> {
    Run(Box<Run<'a>>),
    Node(Node<'a>),
}

/// The body a person reads when `output` is a person's, with the `documents`
/// it shows. Every other reader gets the run's page, which is what its
/// document is about.
async fn page_body<'a>(
    ctx: &'a Context,
    reading: &'a RunReading<'a>,
    documents: &'a [ShownDocument],
    (output, node, depth): (Mode, Option<&NodeId>, Depth),
) -> Result<Body<'a>, CliError> {
    let run_id = &reading.run.id;
    match (output, node) {
        (Mode::Human, Some(id)) => Ok(Body::Node(Node {
            run_id,
            node: find(run_id, &reading.frame, id)?,
            run_dir: &reading.run.dir,
            cwd: &ctx.cwd,
            home: ctx.env.home.as_deref(),
            documents,
            state: &reading.state,
        })),
        _ => Ok(Body::Run(Box::new(
            run_page_for(ctx, reading, documents, (output, depth)).await,
        ))),
    }
}

/// The node `id` names in `frame`. A name the run does not declare is
/// refused with what the run calls the node it meant, when it has one.
fn find<'a>(run_id: &RunId, frame: &'a RunFrame, id: &NodeId) -> Result<&'a NodeFrame, CliError> {
    frame
        .nodes
        .iter()
        .find(|node| node.id == *id)
        .ok_or_else(|| {
            let declared = frame.nodes.iter().map(|node| node.id.as_str());
            CliError::msg(format!(
                "run {} has no node `{id}`{}",
                run_id.handle(),
                surco::text::did_you_mean(id.as_str(), declared)
            ))
        })
}

/// The run's own page, as deep as `depth` asks. A person reading it also
/// gets the decision the run waits on, showing `documents`, and, at the depth
/// that draws the run's story, the change each finished node's plan makes.
async fn run_page_for<'a>(
    ctx: &'a Context,
    reading: &'a RunReading<'a>,
    documents: &'a [ShownDocument],
    (output, depth): (Mode, Depth),
) -> Run<'a> {
    let decision = match output {
        Mode::Human => reading.waits_on().map(|(on, escalation)| Decision {
            on,
            escalation,
            shown: documents,
        }),
        _ => None,
    };
    let plans = match output {
        Mode::Human if depth.reaches(Depth::Full) => reading.plans().await,
        _ => Vec::new(),
    };
    let manifest = &reading.run.manifest.doc;
    Run {
        scope: RunScope::Status,
        depth,
        run_id: &reading.run.id,
        frame: &reading.frame,
        state: &reading.state,
        places: Places {
            run_dir: &reading.run.dir,
            worktree: &reading.tree,
            branch: &reading.branch,
            base_branch: &manifest.base_branch,
            isolation: manifest.isolation,
            cwd: &ctx.cwd,
            home: ctx.env.home.as_deref(),
        },
        decision,
        plans,
    }
}

/// A run's page as a browser reads it, and the folder it goes in: as deep as
/// `--full`, and, where it waits on a decision, each document the decision is
/// about, whole, before it: a browser has the room a terminal does not.
pub(crate) async fn page_for_a_browser(
    ctx: &Context,
    run_id: &RunId,
) -> Result<(Doc, PathBuf), CliError> {
    let reading = read(ctx, run_id).await?;
    let documents = documents(&reading, Mode::Human, None).await?;
    let page = run_page_for(ctx, &reading, &documents, (Mode::Human, Depth::Full))
        .await
        .in_a_browser(reading.now);
    let doc = page.document(&surco::surface::Html.look());
    Ok((doc, reading.run.dir.clone()))
}

/// What `yunta status` answers: the run's document for a program, and the
/// page a person reads.
struct StatusAnswer<'a> {
    body: Body<'a>,
    /// The run the document is built from. The document is built when a
    /// program reads it, so a person's page never pays for it.
    reading: &'a RunReading<'a>,
}

impl serde::Serialize for StatusAnswer<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let reading = self.reading;
        let document = RunDocument::of(
            &reading.run.id,
            &reading.run.events,
            &reading.state,
            &reading.run.manifest.doc,
            &reading.frame,
            reading.decision.clone(),
        );
        serde::Serialize::serialize(&document, serializer)
    }
}

impl Page for Body<'_> {
    fn document(&self, look: &Look) -> Doc {
        match self {
            Body::Run(page) => page.document(look),
            Body::Node(page) => page.document(look),
        }
    }
}

impl Answer for StatusAnswer<'_> {
    fn page(&self) -> impl Page + '_ {
        &self.body
    }

    /// Whatever the run is doing, `yunta status` reports success: what it
    /// tells a reader is the run's state, not the command's.
    fn outcome(&self) -> Outcome {
        Outcome::Success
    }
}
