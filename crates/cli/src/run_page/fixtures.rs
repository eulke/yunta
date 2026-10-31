//! Runs a page is drawn from in tests: the ways a run stands that a person
//! reads differently, each with the frame, the state and the decision its
//! page reads.

use std::path::Path;
use std::time::Duration;

use chrono::DateTime;
use surco::doc::Depth;
use surco::words::Standing;
use yunta_core::events::{
    Claim, CommandExit, Escalation, EventPayload, ExhaustedReroute, Failure, FailureMenu,
    GateOption, Menu, NodeEvent, NodeFinishedPayload, NodeOutcome, NodeStartedPayload, NodeState,
    NodeWait, PauseReason, TaskEvent, TaskRegisteredPayload, TaskStatus, TaskStatusChangedPayload,
    TokenUsage,
};
use yunta_core::reserved::ReservedOption;
use yunta_core::{CommitSha, ContentHash, Isolation, NodeId, NonEmpty, RunId, Seq, TaskId};
use yunta_engine::{
    Counter, NodeFrame, NodeStanding, RunFrame, RunPhase, RunState, TaskCounter, WaitingOn,
};
use yunta_testkit::{node_frame, run_frame};
use yunta_testkit_core::{fixed_now, Log};

use super::{Decided, RunPage, Scope};
use crate::commands::status::decision::Beside;
use crate::surface::Outline;

pub(crate) const RUN: RunId = RunId::from_static("01K3W48MFW7H0ZZA5PZ07E5PH4");

/// How long after its last event every page is read: long enough for a
/// wait to read as one.
const READ_AFTER: Duration = Duration::from_secs(32 * 60 + 10);

/// Where the run lives, as every fixture page names it.
pub(crate) fn outline() -> Outline<'static> {
    Outline {
        run_dir: Path::new("/home/me/.yunta/runs/01K3W48MFW7H0ZZA5PZ07E5PH4"),
        worktree: Path::new("/home/me/.yunta/worktrees/01K3W48MFW7H0ZZA5PZ07E5PH4"),
        base_branch: "main",
        isolation: Isolation::Worktree,
        cwd: Path::new("/home/me/project"),
        home: Some(Path::new("/home/me")),
    }
}

pub(crate) fn reached(id: &str, state: NodeState) -> NodeFrame {
    node_frame(&NodeId::from(id), NodeStanding::Reached(state))
}

pub(crate) fn finished(id: &str) -> NodeFrame {
    reached(
        id,
        NodeState::Finished {
            outcome: NodeOutcome::Summary("exit 0".to_string()),
            tokens: TokenUsage::default(),
        },
    )
}

/// A node the run has not reached.
pub(crate) fn ahead(id: &str) -> NodeFrame {
    NodeFrame {
        attempt: None,
        elapsed: None,
        last_event_age: None,
        ..node_frame(&NodeId::from(id), NodeStanding::ToGo)
    }
}

/// How `lint` fails, as a compiler does: what went wrong on stdout,
/// kept whole in the run's objects.
pub(crate) fn lint_failure() -> Failure {
    Failure::exited(CommandExit {
        origin: None,
        code: 101,
        tail: vec![
            "error[E0425]: cannot find value `x` in this scope".to_string(),
            " --> src/lib.rs:3:5".to_string(),
        ],
        output: Some(ContentHash::sha256(b"what lint printed")),
    })
}

pub(crate) fn failed_lint() -> NodeFrame {
    reached(
        "lint",
        NodeState::Failed {
            failure: lint_failure(),
            tokens: TokenUsage::default(),
        },
    )
}

/// A frame for `nodes` standing at `phase`, its word decided as the
/// engine decides it, every node counted where it stands.
pub(crate) fn frame(phase: RunPhase, nodes: Vec<NodeFrame>, blocking_findings: usize) -> RunFrame {
    let mut by: Vec<(Standing, usize)> = Vec::new();
    for node in &nodes {
        match by
            .iter_mut()
            .find(|(standing, _)| *standing == node.standing)
        {
            Some((_, count)) => *count += 1,
            None => by.push((node.standing, 1)),
        }
    }
    RunFrame {
        workflow: "fragua".into(),
        mode: "quick".into(),
        standing: yunta_engine::run_standing(
            &phase,
            &nodes,
            blocking_findings,
            &yunta_engine::Engine::this(DateTime::UNIX_EPOCH),
        ),
        phase,
        blocking_findings,
        flow: Counter {
            by,
            total: nodes.len(),
            ..Counter::default()
        },
        elapsed: Some(Duration::from_secs(7 * 60 + 41)),
        tokens: TokenUsage {
            input: 18_000,
            output: 2_200,
            cached: None,
        },
        nodes,
        ..run_frame(&RUN)
    }
}

/// The escalation a node parks on once its re-routes are spent: the failure
/// of `lint`, whose re-route to `fix-lint` allows no further attempt.
pub(crate) fn exhausted_escalation() -> Escalation {
    let menu = FailureMenu {
        attempt: 1,
        failure: lint_failure(),
        remedies: Vec::new(),
        stuck: None,
        reroute: Some(ExhaustedReroute {
            goto: NodeId::from("fix-lint"),
            max_reroutes: 0,
            next_mode: None,
        }),
    };
    Escalation::new(
        Claim::NodeFailed {
            node: NodeId::from("lint"),
        },
        Menu::Failure(Box::new(menu)),
        NonEmpty::from((
            GateOption::reserved(ReservedOption::Retry),
            vec![GateOption::reserved(ReservedOption::Abort)],
        )),
    )
}

/// A run parked on `lint`, whose re-routes are spent: the decision it
/// waits on, as the page receives it.
pub(crate) fn parked_on_lint() -> (RunPhase, Decided) {
    let phase = RunPhase::Waiting {
        on: WaitingOn::Node {
            node: NodeId::from("lint"),
            on: NodeWait::Gate { external_ref: None },
            reason: Some(PauseReason::Escalation(Box::new(exhausted_escalation()))),
        },
    };
    let decided = Decided {
        node: NodeId::from("lint"),
        escalation: exhausted_escalation().into_payload(),
        shown: Vec::new(),
        beside: Beside { evidence: true },
    };
    (phase, decided)
}

/// The log of a run that registered four tasks — one done, one blocked,
/// two not begun — its last event at `fixed_now`.
fn tasks_log() -> Vec<yunta_core::events::StoredEvent> {
    let ids = [
        "global-pack-catalog",
        "global-pack-storage",
        "global-pack-cli",
        "global-pack-docs",
    ];
    let mut log = Log::for_run(RUN.as_str());
    for id in ids {
        log = log.event(EventPayload::Tasks(TaskEvent::Registered(
            TaskRegisteredPayload {
                task_id: TaskId::from(id),
                criteria: Vec::new(),
                scope: Vec::new(),
                depends_on: Vec::new(),
            },
        )));
    }
    let done = TaskStatusChangedPayload::done(
        TaskId::from(ids[0]),
        Seq::from(1u64),
        CommitSha::from_static("deadbeef"),
    );
    let blocked =
        TaskStatusChangedPayload::to(TaskId::from(ids[1]), TaskStatus::Blocked, Seq::from(2u64));
    log.event(EventPayload::Tasks(TaskEvent::StatusChanged(done)))
        .event(EventPayload::Tasks(TaskEvent::StatusChanged(blocked)))
        .build()
}

/// One way a run stands, with everything its page reads.
pub(crate) struct Case {
    pub(crate) name: &'static str,
    run_id: RunId,
    pub(crate) frame: RunFrame,
    pub(crate) state: RunState,
    pub(crate) decision: Option<Decided>,
}

impl Case {
    /// The run `frame` describes, its log holding its first node's work.
    fn new(name: &'static str, frame: RunFrame) -> Self {
        let first = frame.nodes.first().map_or("plan", |node| node.id.as_str());
        let finished = NodeFinishedPayload::new(
            NodeOutcome::Summary("exit 0".to_string()),
            TokenUsage::default(),
        );
        let log = Log::for_run(RUN.as_str())
            .node(
                first,
                EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
            )
            .after(12)
            .node(first, EventPayload::Node(NodeEvent::Finished(finished)))
            .build();
        Case {
            name,
            run_id: RUN,
            state: yunta_engine::derive(&log),
            frame,
            decision: None,
        }
    }

    /// The page this run is drawn as, read `READ_AFTER` its last event.
    pub(crate) fn page(&self, scope: Scope, depth: Depth) -> RunPage<'_> {
        let at = self.state.clock().last_at().unwrap_or_else(fixed_now)
            + chrono::Duration::from_std(READ_AFTER).expect("a wait fits a chrono duration");
        RunPage {
            scope,
            depth,
            run_id: &self.run_id,
            engine: yunta_engine::Engine::this(at),
            frame: &self.frame,
            state: &self.state,
            paths: outline(),
            decision: self.decision.clone(),
        }
    }
}

/// The ways a run stands that a person reads differently: it needs them,
/// it failed, it finished, it finished with work nobody has accepted, it
/// waits for a limit to reset, and a person stopped it.
pub(crate) fn cases() -> Vec<Case> {
    let failed = RunPhase::Failed {
        failure: Some(lint_failure()),
    };
    vec![
        needs_you(),
        Case::new(
            "failed",
            frame(
                failed,
                vec![finished("plan"), failed_lint(), ahead("test")],
                0,
            ),
        ),
        Case::new(
            "finished",
            frame(
                RunPhase::Finished,
                vec![finished("lint"), finished("test")],
                0,
            ),
        ),
        Case::new(
            "reported",
            frame(
                RunPhase::Finished,
                vec![finished("lint"), finished("review")],
                1,
            ),
        ),
        parked_on_run(
            "waiting",
            PauseReason::AwaitingReset {
                node: NodeId::from("build"),
                at: fixed_now() + chrono::Duration::hours(2),
            },
        ),
        parked_on_run("stopped", PauseReason::Cancelled),
    ]
}

/// A run parked on `lint`'s exhausted re-route, one task of its four
/// blocked, with the decision it waits on.
fn needs_you() -> Case {
    let (parked, decided) = parked_on_lint();
    let nodes = vec![
        finished("plan"),
        finished("build"),
        failed_lint(),
        ahead("fix-lint"),
        ahead("ship"),
    ];
    let mut case = Case::new("needs-you", frame(parked, nodes, 0));
    case.frame.reroutes = 1;
    case.frame.tasks = Some(TaskCounter {
        done: 1,
        blocked: 1,
        to_go: 2,
        total: 4,
        ..TaskCounter::default()
    });
    case.state = yunta_engine::derive(&tasks_log());
    case.decision = Some(decided);
    case
}

/// A run its own pause holds, `reason`, past its first node.
fn parked_on_run(name: &'static str, reason: PauseReason) -> Case {
    let phase = RunPhase::Waiting {
        on: WaitingOn::Run { reason },
    };
    Case::new(
        name,
        frame(
            phase,
            vec![finished("plan"), ahead("build"), ahead("test")],
            0,
        ),
    )
}
