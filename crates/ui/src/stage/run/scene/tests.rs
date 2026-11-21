//! See [`super`]. The scene each state a run can stand in makes, framed by hand: the node
//! the person points at in every `Standing`, a run that moves by each word its frame reads,
//! each pause no card answers here, a decision or a round not read yet, each end, and the
//! node a run ended on, pointed at by the person.

use std::collections::BTreeMap;

use surco::words::Standing;
use yunta_api::FollowedRun;
use yunta_core::diagnostic::{DocumentKind, DocumentRef, Report};
use yunta_core::events::{
    AuthorGate, Claim, Escalation, Failure, GateOption, Leads, Menu, PauseReason,
};
use yunta_core::{
    CommitSha, ConfigLayer, ContentHash, Isolation, Manifest, NodeId, NonEmpty, OptionId,
    QuestionId, RunId, Workflow,
};
use yunta_render::watched::Heard;
use yunta_replay::frame::{NodeFrame, NodeStanding, RunFrame, RunPhase, WaitingOn};
use yunta_replay::registry::Engine;
use yunta_replay::RunState;

use super::{scene, Grounds, Plan, Scene, Standstill};
use crate::RunInView;

/// The run every test frames.
const RUN: RunId = RunId::from_static("01M7SCENES0000000000SCENES");

/// Its workflow: a command, then a loop over the tasks it leaves.
const WORKFLOW: &str = "name: scenes\nnodes:\n  - { id: build, kind: bash, run: \"true\" }\n  \
                        - { id: tasks, kind: loop, until: all_tasks_complete, \
                        prompt: \"Do each task.\", depends_on: [build] }\n";

/// The command's id.
const COMMAND: &str = "build";

/// The loop's id.
const LOOP: &str = "tasks";

/// The run, frozen with the workflow above.
fn manifest() -> Manifest {
    Manifest {
        schema_version: 1,
        yunta_version: "0.0.0".to_string(),
        workflow: Workflow::parse(WORKFLOW).expect("a command and a loop parse"),
        config: ConfigLayer::default(),
        inputs: BTreeMap::new(),
        prompts: BTreeMap::new(),
        base_branch: "main".to_string(),
        base_commit: CommitSha::from_static("0123456789abcdef0123456789abcdef01234567"),
        isolation: Isolation::None,
        max_parallel_nodes: 1,
        workflow_hash: ContentHash::sha256(b"workflow"),
        config_hash: ContentHash::sha256(b"config"),
        paths: None,
        pack: None,
        project: None,
        adapter: None,
        frozen: None,
        provenance_gaps: Vec::new(),
        loaded_links: Vec::new(),
    }
}

/// The run in view, read whole with no event and nothing read of what it waits on; the
/// engine rebuilds a decision for it where `menu` says.
fn in_view(menu: bool) -> RunInView {
    RunInView {
        run: FollowedRun {
            run: RUN,
            manifest: manifest(),
            state: RunState::replay(&[]),
            engine: Engine::Unrecorded,
            menu,
        },
        moments: Vec::new(),
        by_node: BTreeMap::new(),
        handed_over: BTreeMap::new(),
        heard: Heard::default(),
        missed: 0,
        awaited: None,
        remarks: Vec::new(),
        successor: None,
        no_successor: false,
    }
}

/// The run framed in `phase`, its frame's word for it `standing`.
fn framed(phase: RunPhase, standing: Standing) -> RunFrame {
    RunFrame {
        phase,
        standing,
        ..RunFrame::sample(&RUN)
    }
}

/// The run parked on `pause`.
fn parked(pause: PauseReason) -> RunFrame {
    let on = WaitingOn::Run { reason: pause };
    framed(RunPhase::Waiting { on }, Standing::NeedsYou)
}

/// The node `id`, standing as `standing`.
fn node(id: &'static str, standing: Standing) -> NodeFrame {
    NodeFrame {
        standing,
        ..NodeFrame::sample(&NodeId::from_static(id), NodeStanding::ToGo)
    }
}

/// The scene `scene` stands as, by name, with what a card rests on and why a run stands
/// still.
fn named(scene: Scene<'_>) -> &'static str {
    match scene {
        Scene::Work => "work",
        Scene::Board => "board",
        Scene::Findings => "findings",
        Scene::Round => "round",
        Scene::Desk => "desk",
        Scene::Card(_, Grounds::Shown) => "a card on what the gate shows",
        Scene::Card(_, Grounds::Failure) => "a card on a failure",
        Scene::Card(_, Grounds::Scope) => "a card on a scope",
        Scene::Card(_, Grounds::Departure) => "a card on a departure",
        Scene::Card(_, Grounds::Cap) => "a card on a cap",
        Scene::PullRequest => "pull request",
        Scene::Standstill(Standstill::Unanswered) => "a standstill no option answers here",
        Scene::Standstill(Standstill::NoRoom) => "a standstill for room",
        Scene::Standstill(Standstill::HalfDone) => "a standstill on what is left half done",
        Scene::Standstill(Standstill::Stopped) => "a standstill where it stopped",
        Scene::Standstill(Standstill::NoProcess) => "a standstill with no process",
        Scene::AskedElsewhere => "asked elsewhere",
        Scene::Receipt => "receipt",
        Scene::Ended => "ended",
        Scene::Node => "the node's own",
    }
}

/// The scene the loop pointed at, at work, makes on `frame` while the run leads.
fn led(run: &RunInView, frame: &RunFrame) -> &'static str {
    named(scene(
        run,
        frame,
        &node(LOOP, Standing::Running),
        true,
        Plan::Behind,
    ))
}

#[test]
fn a_node_the_person_points_at_stands_by_its_standing() {
    let run = in_view(false);
    let frame = framed(RunPhase::Running, Standing::Running);
    for standing in Standing::ALL {
        let own = |id| {
            named(scene(
                &run,
                &frame,
                &node(id, standing),
                false,
                Plan::Behind,
            ))
        };
        let expected = match standing {
            Standing::Running | Standing::Waiting => ("work", "board"),
            Standing::Created
            | Standing::NeedsYou
            | Standing::Stalled
            | Standing::Finished
            | Standing::Reported
            | Standing::Failed
            | Standing::Rerouted
            | Standing::Stopped
            | Standing::Promoted
            | Standing::Broken
            | Standing::Skipped
            | Standing::NeverRan => ("the node's own", "the node's own"),
        };
        assert_eq!((own(COMMAND), own(LOOP)), expected, "{standing:?}");
    }
}

#[test]
fn a_run_that_moves_stands_by_the_word_its_frame_reads() {
    let run = in_view(false);
    let at_work = |id| {
        let frame = framed(RunPhase::Running, Standing::Running);
        named(scene(
            &run,
            &frame,
            &node(id, Standing::Running),
            true,
            Plan::Behind,
        ))
    };
    assert_eq!((at_work(COMMAND), at_work(LOOP)), ("work", "board"));
    let stalled = framed(RunPhase::Running, Standing::Stalled);
    assert_eq!(led(&run, &stalled), "a standstill with no process");
    let asked = framed(RunPhase::Running, Standing::NeedsYou);
    assert_eq!(led(&run, &asked), "asked elsewhere");
    let created = framed(RunPhase::Created, Standing::Created);
    assert_eq!(led(&run, &created), "work");
}

/// Each pause no card answers here that stands still, at `node` where it names one, with
/// the standstill it stands as, by name.
fn standing_still(node: &NodeId) -> Vec<(PauseReason, &'static str)> {
    let stopped = "a standstill where it stopped";
    let half_done = "a standstill on what is left half done";
    vec![
        (PauseReason::Cancelled, stopped),
        (PauseReason::CancelledAfterCrash, stopped),
        (
            PauseReason::NoRoom {
                available: 1,
                need: 2,
                keep_free: 3,
            },
            "a standstill for room",
        ),
        (
            PauseReason::UncertainOrphans {
                nodes: vec![node.clone()],
            },
            half_done,
        ),
        (
            PauseReason::Blocked {
                node: node.clone(),
                on: Vec::new(),
            },
            half_done,
        ),
        (
            PauseReason::GateAborted {
                node: node.clone(),
                free_text: None,
            },
            stopped,
        ),
        (
            PauseReason::Recorded {
                sentence: "waiting".into(),
            },
            stopped,
        ),
        (
            PauseReason::ChildPaused {
                node: node.clone(),
                run: RUN,
                child: Box::new(PauseReason::Cancelled),
            },
            "a standstill no option answers here",
        ),
    ]
}

#[test]
fn a_pause_no_card_answers_here_stands_still_by_what_moves_it_again() {
    let run = in_view(false);
    for (pause, still) in standing_still(&NodeId::from_static(LOOP)) {
        assert_eq!(led(&run, &parked(pause.clone())), still, "{pause:?}");
    }
}

#[test]
fn a_pause_a_forge_holds_stands_as_its_pull_request() {
    let run = in_view(false);
    let node = NodeId::from_static(LOOP);
    let url = "https://mock.forge/pr/1".to_string();
    let forge = [
        PauseReason::ExternalGate { url: url.clone() },
        PauseReason::PullRequestClosed { node, url },
    ];
    for pause in forge {
        assert_eq!(
            led(&run, &parked(pause.clone())),
            "pull request",
            "{pause:?}"
        );
    }
}

#[test]
fn work_held_for_a_reset_or_waiting_on_what_is_not_read_yet_stands_as_the_node_at_work() {
    let node = NodeId::from_static(LOOP);
    let reset = PauseReason::AwaitingReset {
        node: node.clone(),
        at: chrono::DateTime::UNIX_EPOCH,
    };
    assert_eq!(led(&in_view(false), &parked(reset)), "board");
    let round = [
        PauseReason::Questions {
            node: node.clone(),
            pending: NonEmpty::from((QuestionId::from_static("q-1"), Vec::new())),
        },
        PauseReason::AnswersRefused {
            node,
            report: Report::new(
                DocumentRef::new(DocumentKind::Workflow, "answers.yaml"),
                Vec::new(),
            ),
        },
    ];
    for pause in round {
        assert_eq!(
            led(&in_view(false), &parked(pause.clone())),
            "board",
            "{pause:?}"
        );
    }
}

#[test]
fn a_decision_not_read_yet_stands_as_the_node_at_work_and_one_never_rebuilt_stands_still() {
    let gate = Escalation::new(
        Claim::Written("the gate waits".to_string()),
        Menu::Gate(AuthorGate {
            assignee: "lead".into(),
            node: None,
        }),
        NonEmpty::from((
            GateOption::leading(
                OptionId::from_static("approve"),
                Leads::On {
                    next: Vec::new(),
                    settles: false,
                },
            ),
            Vec::new(),
        )),
    );
    let failed = PauseReason::NodeFailed {
        node: NodeId::from_static(LOOP),
        failure: Failure::Message {
            outcome: "it broke".into(),
        },
    };
    let decisions = [
        PauseReason::Escalation(Box::new(gate)),
        failed,
        PauseReason::BudgetExhausted { spent: 5, cap: 4 },
        PauseReason::LoopOverrun {
            node: NodeId::from_static(LOOP),
            cap: 2,
        },
    ];
    for pause in decisions {
        assert_eq!(
            led(&in_view(true), &parked(pause.clone())),
            "board",
            "{pause:?}"
        );
        let never = led(&in_view(false), &parked(pause.clone()));
        assert_eq!(never, "a standstill no option answers here", "{pause:?}");
    }
}

#[test]
fn a_run_that_ended_stands_as_its_receipt_or_its_end() {
    let run = in_view(false);
    let finished = framed(RunPhase::Finished, Standing::Finished);
    assert_eq!(led(&run, &finished), "receipt");
    let ends = [
        RunPhase::Failed { failure: None },
        RunPhase::Cancelled,
        RunPhase::Promoted { to: None },
        RunPhase::Broken {
            diagnostic: "seq 3 is missing its node".to_string(),
        },
    ];
    for phase in ends {
        let ended = framed(phase.clone(), Standing::Failed);
        assert_eq!(led(&run, &ended), "ended", "{phase:?}");
    }
}

#[test]
fn the_node_a_run_ended_on_shows_its_end_whoever_points_at_it() {
    let run = in_view(false);
    // A broken run whose log stops making sense while its command waits on the person: its
    // pointer stands on that node at its end.
    let broken = RunFrame {
        nodes: vec![
            node(COMMAND, Standing::NeedsYou),
            node(LOOP, Standing::NeverRan),
        ],
        ..framed(
            RunPhase::Broken {
                diagnostic: "seq 3 is missing its node".to_string(),
            },
            Standing::Broken,
        )
    };
    let pointed = |id, standing| {
        named(scene(
            &run,
            &broken,
            &node(id, standing),
            false,
            Plan::Behind,
        ))
    };
    assert_eq!(pointed(COMMAND, Standing::NeedsYou), "ended");
    assert_eq!(pointed(LOOP, Standing::NeverRan), "the node's own");
    // A finished run's receipt is the run's: a node pointed at stands as its own.
    let finished = RunFrame {
        nodes: vec![node(COMMAND, Standing::Finished)],
        ..framed(RunPhase::Finished, Standing::Finished)
    };
    let own = named(scene(
        &run,
        &finished,
        &node(COMMAND, Standing::Finished),
        false,
        Plan::Behind,
    ));
    assert_eq!(own, "the node's own");
}
