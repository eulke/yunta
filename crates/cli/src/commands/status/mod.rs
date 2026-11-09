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

pub(crate) mod decision_json;
pub(crate) mod follow;

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use yunta_core::events::{DecisionOn, Escalation, NodeState, StoredEvent};
use yunta_core::shown::ShownDocument;
use yunta_core::{Clock, Manifest, NodeId, RunId};
use yunta_render::escalation::Decision;
use yunta_render::page::{Node, Page, Places, Run, RunScope};
use yunta_replay::frame::{NodeFrame, NodeStanding, RunFrame};
use yunta_replay::RunState;

use crate::commands::stats;
use crate::context::Context;
use crate::error::CliError;
use crate::json::RunDocument;
use crate::output::note;
use crate::output::Outcome;
use crate::output::{emit, Answer, Mode};
use surco::doc::{Depth, Doc};
use surco::look::Look;

/// Prints where `run_id` stands, as deep as `depth` asks — or, with
/// `node`, that one node whole. A program reads the run's document under
/// `Mode::Json`, whatever `node` names.
pub async fn status(
    run_id: &RunId,
    output: Mode,
    (node, depth): (Option<&NodeId>, Depth),
    look: &Look,
) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let reading = Reading::of(&ctx, run_id).await?;
    let documents = reading.documents(output, node).await?;
    let body = page_body(&ctx, run_id, &reading, &documents, (output, node, depth)).await?;
    let answer = StatusAnswer {
        body,
        document: DocumentInputs::of(run_id, &reading),
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
    let reading = Reading::of(ctx, run_id).await?;
    let documents = reading.documents(Mode::Human, None).await?;
    let page = (Mode::Human, None, Depth::Glance);
    let body = page_body(ctx, run_id, &reading, &documents, page).await?;
    let answer = StatusAnswer {
        body,
        document: DocumentInputs::of(run_id, &reading),
    };
    emit(Mode::Human, &answer, look)?;
    Ok(reading.frame.standing)
}

/// What `yunta status` reads about one run, gathered once: its log, the
/// state and frame that log derives, the decision it stopped on, and the
/// tree it works in.
struct Reading {
    events: Vec<StoredEvent>,
    run_dir: PathBuf,
    manifest: Manifest,
    state: RunState,
    frame: RunFrame,
    decision: Option<(DecisionOn, Escalation)>,
    tree: PathBuf,
    /// The run's own branch, which its tree works on.
    branch: String,
    now: DateTime<Utc>,
}

impl Reading {
    /// The run as its own log describes it at the clock's instant.
    async fn of(ctx: &Context, run_id: &RunId) -> Result<Self, CliError> {
        let open = ctx.open_run(run_id).await?;
        let events = open.events;
        let run_dir = open.run_dir;
        // What this binary did not understand in a file written by a newer binary:
        // said, because a reader acting on a manifest whose newer half is
        // invisible to them should know that is what they are doing.
        if !open.manifest.unknown.is_empty() {
            note(format!(
                "this run's manifest carries {} this binary does not know ({}): a newer yunta \
                 wrote it, and what it recorded there is not read here",
                surco::text::counted(open.manifest.unknown.len(), "key"),
                open.manifest.unknown_keys().join(", ")
            ));
        }
        let manifest = open.manifest.doc;
        let now = ctx.clock.now();
        let state = yunta_replay::RunState::replay(&events);
        let decision = crate::json::decision_of(&manifest, &run_dir, &state).await?;
        let engine = yunta_engine::read_engine(&run_dir, &yunta_engine::lock::SystemProbe);
        let mut frame = RunFrame::of(run_id, &manifest.workflow, &state, None, &engine, now);
        frame.prior = stats::budget_prior(ctx, &manifest, &frame, &events).await;
        let bound = crate::project::bound_checkout(&state);
        let tree = ctx
            .project
            .run_tree(&manifest, bound.as_deref(), run_id, &ctx.cwd);
        Ok(Self {
            events,
            run_dir,
            manifest,
            state,
            frame,
            decision,
            tree,
            branch: yunta_engine::run_branch(run_id),
            now,
        })
    }

    /// The decision the run waits on. `None` for a run that is not parked,
    /// and for a pause that rebuilds no menu: the page's ask says what that
    /// one waits on.
    fn waits_on(&self) -> Option<(&DecisionOn, &Escalation)> {
        self.frame.phase.waiting_on()?;
        self.decision
            .as_ref()
            .map(|(on, escalation)| (on, escalation))
    }

    /// The documents a person's page shows, read once: one node's page shows
    /// what the node produced, and the run's page what the decision it waits
    /// on shows. A program reads the run's document, which shows none.
    async fn documents(
        &self,
        output: Mode,
        node: Option<&NodeId>,
    ) -> Result<Vec<ShownDocument>, CliError> {
        match (output, node, self.waits_on()) {
            (Mode::Human, Some(id), _) => Ok(yunta_engine::produced_documents(
                &self.run_dir,
                id,
                &self.state,
                self.manifest.request(),
            )
            .await?),
            (Mode::Human, None, Some((_, escalation))) => {
                crate::json::shown_of(&self.manifest, &self.run_dir, escalation, &self.state).await
            }
            _ => Ok(Vec::new()),
        }
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
    run_id: &'a RunId,
    reading: &'a Reading,
    documents: &'a [ShownDocument],
    (output, node, depth): (Mode, Option<&NodeId>, Depth),
) -> Result<Body<'a>, CliError> {
    match (output, node) {
        (Mode::Human, Some(id)) => Ok(Body::Node(Node {
            run_id,
            node: find(run_id, &reading.frame, id)?,
            run_dir: &reading.run_dir,
            cwd: &ctx.cwd,
            home: ctx.env.home.as_deref(),
            documents,
            state: &reading.state,
        })),
        _ => Ok(Body::Run(Box::new(
            run_page_for(ctx, run_id, reading, documents, (output, depth)).await?,
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
/// gets the decision the run waits on, showing `documents`.
async fn run_page_for<'a>(
    ctx: &'a Context,
    run_id: &'a RunId,
    reading: &'a Reading,
    documents: &'a [ShownDocument],
    (output, depth): (Mode, Depth),
) -> Result<Run<'a>, CliError> {
    let decision = match output {
        Mode::Human => reading.waits_on().map(|(on, escalation)| Decision {
            on,
            escalation,
            shown: documents,
        }),
        _ => None,
    };
    let plans = match output {
        Mode::Human => plans(reading, depth).await,
        _ => Vec::new(),
    };
    Ok(Run {
        scope: RunScope::Status,
        depth,
        run_id,
        frame: &reading.frame,
        state: &reading.state,
        places: Places {
            run_dir: &reading.run_dir,
            worktree: &reading.tree,
            branch: &reading.branch,
            base_branch: &reading.manifest.base_branch,
            isolation: reading.manifest.isolation,
            cwd: &ctx.cwd,
            home: ctx.env.home.as_deref(),
        },
        decision,
        plans,
    })
}

/// A run's page as a browser reads it, and the folder it goes in: as deep as
/// `--full`, and, where it waits on a decision, each document the decision is
/// about, whole, before it: a browser has the room a terminal does not.
pub(crate) async fn page_for_a_browser(
    ctx: &Context,
    run_id: &RunId,
) -> Result<(Doc, PathBuf), CliError> {
    let reading = Reading::of(ctx, run_id).await?;
    let documents = reading.documents(Mode::Human, None).await?;
    let page = run_page_for(
        ctx,
        run_id,
        &reading,
        &documents,
        (Mode::Human, Depth::Full),
    )
    .await?
    .in_a_browser(reading.now);
    let doc = page.document(&surco::surface::Html.look());
    Ok((doc, reading.run_dir.clone()))
}

/// The change each finished node's accepted plan says it makes, for the story
/// a page at `depth` draws: none when it draws no story. A node still at work
/// keeps what its row says of that work, and a node whose documents cannot be
/// read keeps what it said: the page draws either way.
async fn plans(reading: &Reading, depth: Depth) -> Vec<(NodeId, String)> {
    if !depth.reaches(Depth::Full) {
        return Vec::new();
    }
    let finished = reading.frame.nodes.iter().filter(|node| {
        matches!(
            node.state,
            NodeStanding::Reached(NodeState::Finished { .. })
        )
    });
    let mut summaries = Vec::new();
    for node in finished {
        let Ok(documents) = yunta_engine::produced_documents(
            &reading.run_dir,
            &node.id,
            &reading.state,
            reading.manifest.request(),
        )
        .await
        else {
            continue;
        };
        if let Some(change) = yunta_core::shown::change_shown(&documents) {
            summaries.push((node.id.clone(), surco::text::one_line(change)));
        }
    }
    summaries
}

/// What `yunta status` answers: the run's document for a program, and the
/// page a person reads.
struct StatusAnswer<'a> {
    body: Body<'a>,
    document: DocumentInputs<'a>,
}

/// What the run's document is built from. The document is built when a
/// program reads it, so a person's page never pays for it.
struct DocumentInputs<'a> {
    run_id: &'a RunId,
    events: &'a [StoredEvent],
    state: &'a RunState,
    manifest: &'a Manifest,
    frame: &'a RunFrame,
    decision: Option<(DecisionOn, Escalation)>,
}

impl<'a> DocumentInputs<'a> {
    fn of(run_id: &'a RunId, reading: &'a Reading) -> Self {
        Self {
            run_id,
            events: &reading.events,
            state: &reading.state,
            manifest: &reading.manifest,
            frame: &reading.frame,
            decision: reading.decision.clone(),
        }
    }
}

impl serde::Serialize for StatusAnswer<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let inputs = &self.document;
        let document = RunDocument::of(
            inputs.run_id,
            inputs.events,
            inputs.state,
            inputs.manifest,
            inputs.frame,
            inputs.decision.clone(),
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
