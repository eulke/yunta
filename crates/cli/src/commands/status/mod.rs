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

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use yunta_core::caution::Caution;
use yunta_core::events::{Escalation, GateWaitingPayload, StoredEvent};
use yunta_core::{Clock, Manifest, NodeId, RunId};
use yunta_engine::{RunFrame, RunPhase, RunState, ShownDocument};

use crate::commands::{advice, stats};
use crate::context::Context;
use crate::error::note;
use crate::error::{CliError, Outcome};
use crate::json::{PreRunWarnings, RunDocument};
use crate::output::{emit, Answer, Mode};
use crate::render::doc::Doc;
use crate::render::ink::Line;
use crate::render::{Engine, Look, INDENT};
use crate::run_page::{cautions, Decided, RunPage, Scope};
use crate::surface::Outline;
use node::NodePage;

/// Prints where `run_id` stands — or, with `node`, that one node whole. A
/// program reads the run's document under `Mode::Json`, whatever `node`
/// names.
pub async fn status(
    run_id: &RunId,
    output: Mode,
    node: Option<&NodeId>,
    look: &Look,
) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let reading = Reading::of(&ctx, run_id).await?;
    let documents = match (output, node) {
        (Mode::Human, Some(id)) => {
            yunta_engine::produced_documents(&reading.run_dir, id, &reading.state).await?
        }
        _ => Vec::new(),
    };
    let body = page_body(&ctx, run_id, &reading, &documents, (output, node), look).await?;
    let answer = StatusAnswer {
        body,
        document: DocumentInputs::of(run_id, &reading),
    };
    emit(output, &answer, look)
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
    decision: Option<(NodeId, Escalation)>,
    tree: PathBuf,
    engine: Engine,
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
                "this run's manifest carries {} this binary does not know: {} — a newer yunta \
                 wrote it, and what it recorded there is not read here",
                yunta_core::text::counted(open.manifest.unknown.len(), "key"),
                open.manifest.unknown_keys().join(", ")
            ));
        }
        let manifest = open.manifest.doc;
        let now = ctx.clock.now();
        let state = yunta_engine::derive(&events);
        let decision = crate::json::decision_of(&manifest, &run_dir, &state).await?;
        let mut frame = progress::frame_of(run_id, &manifest, &state, now);
        frame.prior = stats::budget_prior(ctx, &manifest, &frame, &events).await;
        let bound = crate::project::bound_checkout(&state);
        let tree = ctx
            .project
            .run_tree(&manifest, bound.as_deref(), run_id, &ctx.cwd);
        Ok(Self {
            events,
            engine: Engine::of(&run_dir, now),
            run_dir,
            manifest,
            state,
            frame,
            decision,
            tree,
            now,
        })
    }
}

/// What a person reads: the run's page, or one node of it.
enum Body<'a> {
    Run(Box<RunPage<'a>>),
    Node(NodePage<'a>),
}

/// The body a person reads when `output` is a person's. Every other reader
/// gets the run's page, which is what its document is about.
async fn page_body<'a>(
    ctx: &'a Context,
    run_id: &'a RunId,
    reading: &'a Reading,
    documents: &'a [ShownDocument],
    (output, node): (Mode, Option<&NodeId>),
    look: &Look,
) -> Result<Body<'a>, CliError> {
    match (output, node) {
        (Mode::Human, Some(id)) => Ok(Body::Node(NodePage {
            run_id,
            node: node::find(run_id, &reading.frame, id)?,
            run_dir: &reading.run_dir,
            cwd: &ctx.cwd,
            home: ctx.env.home.as_deref(),
            documents,
            state: &reading.state,
        })),
        _ => Ok(Body::Run(Box::new(
            run_page_for(ctx, run_id, reading, output, look).await?,
        ))),
    }
}

/// The run's own page. A person reading it also gets the decision the run
/// waits on, laid out for their look.
async fn run_page_for<'a>(
    ctx: &'a Context,
    run_id: &'a RunId,
    reading: &'a Reading,
    output: Mode,
    look: &Look,
) -> Result<RunPage<'a>, CliError> {
    let decided = match output {
        Mode::Human => decided(run_id, reading, look).await?,
        _ => None,
    };
    Ok(RunPage {
        scope: Scope::Status,
        run_id,
        engine: reading.engine.clone(),
        frame: &reading.frame,
        state: &reading.state,
        paths: Outline {
            run_dir: &reading.run_dir,
            worktree: &reading.tree,
            base_branch: &reading.manifest.base_branch,
            isolation: reading.manifest.isolation,
            cwd: &ctx.cwd,
            home: ctx.env.home.as_deref(),
        },
        decision: decided,
    })
}

/// The decision a parked run waits on, read with what the page already says
/// beside it. `None` for a run that is not parked.
async fn decided(
    run_id: &RunId,
    reading: &Reading,
    look: &Look,
) -> Result<Option<Decided>, CliError> {
    let shown = match &reading.decision {
        Some((_, escalation)) => {
            crate::json::shown_of(&reading.run_dir, escalation, &reading.state).await?
        }
        None => Vec::new(),
    };
    Ok(decision_page(
        (run_id, reading.tree.as_path()),
        &reading.state,
        &reading.frame.phase,
        reading.decision.clone(),
        &shown,
        look,
    ))
}

/// The decision a parked run waits on, as the page draws it: the node's menu
/// under the tree the run works in, or — for a pause that reconstructs no
/// menu — what the run waits on. `None` for a run that is not parked.
fn decision_page(
    (run_id, tree): (&RunId, &Path),
    state: &RunState,
    phase: &RunPhase,
    decision: Option<(NodeId, Escalation)>,
    shown: &[ShownDocument],
    look: &Look,
) -> Option<Decided> {
    let waiting = advice::parked(phase)?;
    Some(match decision {
        Some((node, escalation)) => {
            // The page's second line made the claim, with the record
            // beside it when it said the run's own sentence, and a failed
            // node's evidence is quoted above.
            let beside = decision::Beside {
                claim: true,
                evidence: advice::parked_says_why(waiting)
                    || matches!(
                        state.nodes.state(&node),
                        Some(yunta_engine::NodeState::Failed { .. })
                    ),
            };
            let payload = escalation.into_payload();
            Decided {
                lines: menu_lines((run_id, tree), &node, &payload, shown, beside, look),
                escalation: Some(payload),
            }
        }
        None => Decided {
            lines: decision::without_menu(
                run_id,
                &advice::parked_in_full(waiting),
                advice::reset_pause(phase).is_some(),
                look.width.cells(),
            ),
            escalation: None,
        },
    })
}

/// The lines of a decision that has a menu: where the run works, said
/// whole and once above the decision, then the decision itself.
fn menu_lines(
    (run_id, tree): (&RunId, &Path),
    node: &NodeId,
    payload: &GateWaitingPayload,
    shown: &[ShownDocument],
    beside: decision::Beside,
    look: &Look,
) -> Vec<Line> {
    // Where a person acts to answer, said once above the decision
    // and whole, so it can be copied into another terminal.
    let mut lines = vec![Line::new()
        .plain(INDENT)
        .plain(decision::run_tree_line(tree))];
    lines.extend(decision::lines(
        (run_id, node),
        payload,
        shown,
        beside,
        look,
    ));
    lines
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
    now: DateTime<Utc>,
    engine: Engine,
    decision: Option<(NodeId, Escalation)>,
    cautions: Vec<Caution>,
}

impl<'a> DocumentInputs<'a> {
    fn of(run_id: &'a RunId, reading: &'a Reading) -> Self {
        Self {
            run_id,
            events: &reading.events,
            state: &reading.state,
            manifest: &reading.manifest,
            now: reading.now,
            engine: reading.engine.clone(),
            decision: reading.decision.clone(),
            cautions: cautions(&reading.frame),
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
            inputs.now,
            &inputs.engine,
            inputs.decision.clone(),
        )
        .warnings(PreRunWarnings {
            cautions: inputs.cautions.clone(),
        });
        serde::Serialize::serialize(&document, serializer)
    }
}

impl Answer for StatusAnswer<'_> {
    fn document<'a>(&'a self, look: &Look) -> Doc<'a> {
        match &self.body {
            Body::Run(page) => page.document(look),
            Body::Node(page) => page.document(look),
        }
    }

    /// Whatever the run is doing, `yunta status` reports success: what it
    /// tells a reader is the run's state, not the command's.
    fn outcome(&self) -> Outcome {
        Outcome::Success
    }
}
