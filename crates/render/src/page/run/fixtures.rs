//! Runs a page is drawn from in tests: the ways a run stands that a person
//! reads differently, each with the frame, the state and the decision its
//! page reads.

use std::path::Path;
use std::time::Duration;

use crate::escalation::Decision;
use chrono::{DateTime, Utc};
use surco::doc::Depth;
use surco::words::Standing;
use yunta_core::events::{
    AccountLimit, Claim, CommandExit, DecisionOn, Escalation, EventPayload, ExhaustedReroute,
    Failure, FailureMenu, GateOption, GateWaitingPayload, LimitSource, Menu, NodeEvent,
    NodeFinishedPayload, NodeOutcome, NodeStartedPayload, NodeState, NodeWait, PauseReason,
    StoredEvent, TaskEvent, TaskRegisteredPayload, TaskStatus, TaskStatusChangedPayload,
    TokenUsage,
};
use yunta_core::reserved::ReservedOption;
use yunta_core::{
    AdapterId, CommitSha, ContentHash, Isolation, NodeId, NonEmpty, RunId, Seq, TaskId,
};
use yunta_replay::frame::{
    Counter, NodeFrame, NodeStanding, RunFrame, RunPhase, TaskCounter, WaitingOn,
};
use yunta_replay::RunState;
use yunta_testkit_core::{fixed_now, Log};

use super::{Places, Run, RunScope};

pub(crate) const RUN: RunId = RunId::from_static("01K3W48MFW7H0ZZA5PZ07E5PH4");

/// How long after its last event every page is read: long enough for a
/// wait to read as one.
const READ_AFTER: Duration = Duration::from_secs(32 * 60 + 10);

/// The closing of the run `frame` stands for, with the code the CLI exits with
/// for its word: 0 when it finished, 3 when it needs a person, 4 when it
/// finished with work nobody accepted, 5 when it waits, and 1 for any other
/// stop. The closing goldens hold what the CLI printed for each.
pub(crate) fn closing(frame: &RunFrame) -> RunScope {
    let exit_code = match frame.standing {
        Standing::Finished => 0,
        Standing::NeedsYou => 3,
        Standing::Reported => 4,
        Standing::Waiting => 5,
        _ => 1,
    };
    RunScope::Closing { exit_code }
}

/// Where the run lives, as every fixture page names it.
pub(crate) fn places() -> Places<'static> {
    Places {
        run_dir: Path::new("/home/me/.yunta/runs/01K3W48MFW7H0ZZA5PZ07E5PH4"),
        worktree: Path::new("/home/me/.yunta/worktrees/01K3W48MFW7H0ZZA5PZ07E5PH4"),
        branch: "yunta/run/01K3W48MFW7H0ZZA5PZ07E5PH4",
        base_branch: "main",
        isolation: Isolation::Worktree,
        cwd: Path::new("/home/me/project"),
        home: Some(Path::new("/home/me")),
    }
}

pub(crate) fn reached(id: &str, state: NodeState) -> NodeFrame {
    NodeFrame::sample(&NodeId::from(id), NodeStanding::Reached(state))
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
        ..NodeFrame::sample(&NodeId::from(id), NodeStanding::ToGo)
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

/// A node a session's usage limit stopped, which the run waits on: its log
/// says it failed, and it stands `waiting`, as the engine words a node a
/// reset holds, until the limit resets at `resets_at` and it runs again.
fn failed_on_limit(id: &str, resets_at: DateTime<Utc>) -> NodeFrame {
    NodeFrame {
        standing: Standing::Waiting,
        ..reached(
            id,
            NodeState::Failed {
                failure: Failure::Limit {
                    limit: AccountLimit {
                        source: LimitSource::Session {
                            adapter: AdapterId::from_static("claude-code"),
                        },
                        resets_at: Some(resets_at),
                        message: "You've hit your usage limit".to_string(),
                    },
                },
                tokens: TokenUsage::default(),
            },
        )
    }
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
        standing: yunta_replay::frame::run_standing(
            &phase,
            &nodes,
            blocking_findings,
            &yunta_replay::registry::Engine::this(yunta_core::Pid::current()),
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
        ..RunFrame::sample(&RUN)
    }
}

/// The escalation a node parks on once its reroutes are spent: the failure
/// of `lint`, whose reroute to `fix-lint` allows no further attempt.
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

/// A run parked on `lint`, whose reroutes are spent.
pub(crate) fn parked_on_lint() -> RunPhase {
    RunPhase::Waiting {
        on: WaitingOn::Node {
            node: NodeId::from("lint"),
            on: NodeWait::Gate { external_ref: None },
            reason: Some(PauseReason::Escalation(Box::new(exhausted_escalation()))),
        },
    }
}

/// The decision a run [`parked_on_lint`] waits on, as the log rebuilds it.
pub(crate) fn lint_decision() -> (DecisionOn, GateWaitingPayload) {
    (
        DecisionOn::Node(NodeId::from("lint")),
        exhausted_escalation().into_payload(),
    )
}

/// The log of a run that registered four tasks — one done, one blocked,
/// two not begun — its last event at `fixed_now`.
fn tasks_log() -> Vec<StoredEvent> {
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

/// The log of a run with one task, blocked, whose last attempt left work.
pub(crate) fn blocked_log() -> Vec<StoredEvent> {
    let blocked = TaskStatusChangedPayload::blocked(
        TaskId::from("T001"),
        Seq::from(1u64),
        Some(CommitSha::from_static("deadbeef00")),
        yunta_core::events::TaskCause::ScopeDecisionOwed,
    );
    Log::for_run(RUN.as_str())
        .event(EventPayload::Tasks(TaskEvent::Registered(
            TaskRegisteredPayload {
                task_id: TaskId::from("T001"),
                criteria: Vec::new(),
                scope: Vec::new(),
                depends_on: Vec::new(),
            },
        )))
        .node(
            "implement",
            EventPayload::Tasks(TaskEvent::StatusChanged(blocked)),
        )
        .build()
}

/// One way a run stands, with everything its page reads.
pub(crate) struct Case {
    pub(crate) name: &'static str,
    run_id: RunId,
    pub(crate) frame: RunFrame,
    pub(crate) state: RunState,
    pub(crate) decision: Option<(DecisionOn, GateWaitingPayload)>,
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
        Case::logged(name, frame, &log)
    }

    /// The run `frame` describes, its log `log`, read `READ_AFTER` that
    /// log's last event: the frame carries what that instant measures, as
    /// `RunFrame::of` measures it.
    fn logged(name: &'static str, frame: RunFrame, log: &[StoredEvent]) -> Self {
        let state = yunta_replay::RunState::replay(log);
        let read_at = state.clock().last_at().unwrap_or_else(fixed_now)
            + chrono::Duration::from_std(READ_AFTER).expect("a wait fits a chrono duration");
        let frame = RunFrame {
            since_last_event: Some(READ_AFTER),
            time: state.time_spent((!state.ended()).then_some(read_at)),
            ..frame
        };
        Case {
            name,
            run_id: RUN,
            state,
            frame,
            decision: None,
        }
    }

    /// The page this run is drawn as, read `READ_AFTER` its last event.
    /// The closing this case's run leaves.
    pub(crate) fn closing(&self) -> RunScope {
        closing(&self.frame)
    }

    pub(crate) fn page(&self, scope: RunScope, depth: Depth) -> Run<'_> {
        Run {
            scope,
            depth,
            run_id: &self.run_id,
            frame: &self.frame,
            state: &self.state,
            places: places(),
            decision: self.decision.as_ref().map(|(on, escalation)| Decision {
                on,
                escalation,
                shown: &[],
            }),
            plans: Vec::new(),
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
    let reset = fixed_now() + chrono::Duration::hours(2);
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
                at: reset,
            },
            vec![
                finished("plan"),
                failed_on_limit("build", reset),
                ahead("test"),
            ],
        ),
        parked_on_run(
            "stopped",
            PauseReason::Cancelled,
            vec![finished("plan"), ahead("build"), ahead("test")],
        ),
    ]
}

/// A run parked on `lint`'s exhausted reroute, one task of its four
/// blocked, with the decision it waits on.
fn needs_you() -> Case {
    let parked = parked_on_lint();
    let nodes = vec![
        finished("plan"),
        finished("build"),
        failed_lint(),
        ahead("fix-lint"),
        ahead("ship"),
    ];
    let mut case = Case::logged("needs-you", frame(parked, nodes, 0), &tasks_log());
    case.frame.reroutes = 1;
    case.frame.tasks = Some(TaskCounter {
        done: 1,
        blocked: 1,
        to_go: 2,
        total: 4,
        ..TaskCounter::default()
    });
    case.decision = Some(lint_decision());
    case
}

/// A run its own pause holds, `reason`, with `nodes` where they stand.
fn parked_on_run(name: &'static str, reason: PauseReason, nodes: Vec<NodeFrame>) -> Case {
    let phase = RunPhase::Waiting {
        on: WaitingOn::Run { reason },
    };
    Case::new(name, frame(phase, nodes, 0))
}
