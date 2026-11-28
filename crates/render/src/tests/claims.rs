//! The words of an escalation's claim: an engine claim is written from its
//! node, the assignee it names and the reroute it closes, and a written
//! claim is kept. Beside it, what a decision's card asks and names at its far
//! edge, and the gist a run parked on a person's answer says under its headline.

use surco::words::Standing;
use yunta_core::events::{
    Claim, DepartsFrom, DeviationDeclaredPayload, Escalation, ExhaustedReroute, Failure,
    FailureMenu, GateOption, GateWaitingPayload, Menu, NodeWait, OwedScope, PauseReason, RedCheck,
    TaskCause, TaskStop,
};
use yunta_core::reserved::ReservedOption;
use yunta_core::{NodeId, NonEmpty, QuestionId, RunId, TaskId};
use yunta_replay::frame::{RunFrame, RunPhase, RunTimes, WaitingOn};

use crate::claim::sentence;
use crate::escalation::question;
use crate::prompt::card::edge;
use crate::state::gist;

/// A failure menu whose reroutes to `fix-lint` are used up after `max_reroutes`.
fn exhausted(max_reroutes: u32) -> FailureMenu {
    FailureMenu {
        attempt: max_reroutes + 1,
        failure: Failure::Message {
            outcome: "exit 1".into(),
        },
        remedies: Vec::new(),
        stuck: None,
        reroute: Some(ExhaustedReroute {
            goto: NodeId::from("fix-lint"),
            max_reroutes,
            next_mode: None,
        }),
    }
}

#[test]
fn a_gate_that_needs_a_decision_says_so() {
    let claim = Claim::NeedsDecision {
        node: NodeId::from("approve"),
    };
    assert_eq!(sentence(&claim, None), "gate `approve` needs a decision");
}

#[test]
fn a_node_that_failed_says_it_failed() {
    let claim = Claim::NodeFailed {
        node: NodeId::from("lint"),
    };
    assert_eq!(sentence(&claim, None), "node `lint` failed");
}

#[test]
fn a_node_whose_re_routes_are_used_up_names_the_route_it_closes() {
    let claim = Claim::NodeFailed {
        node: NodeId::from("lint"),
    };
    assert_eq!(
        sentence(&claim, Some(&exhausted(1))),
        "node `lint` failed and its 1 reroute to `fix-lint` is exhausted"
    );
    assert_eq!(
        sentence(&claim, Some(&exhausted(2))),
        "node `lint` failed and its 2 reroutes to `fix-lint` are exhausted"
    );
    assert_eq!(
        sentence(&claim, Some(&exhausted(0))),
        "node `lint` failed and its 0 reroutes to `fix-lint` are exhausted"
    );
}

#[test]
fn a_written_claim_reads_as_it_was_written_whatever_the_menu_holds() {
    let claim = Claim::Written("Approve the plan?".to_string());
    assert_eq!(sentence(&claim, None), "Approve the plan?");
    assert_eq!(sentence(&claim, Some(&exhausted(1))), "Approve the plan?");
}

/// A question someone wrote over several lines reads on one line.
#[test]
fn a_question_written_over_lines_reads_as_one() {
    let claim = Claim::Written("Ship it?\n  Now.".to_string());
    assert_eq!(sentence(&claim, None), "Ship it? Now.");
}

#[test]
fn an_awaiting_review_claim_names_its_node_and_its_assignee() {
    let claim = Claim::AwaitsReview {
        node: NodeId::from("approve"),
        assignee: Some("lead".to_string()),
    };
    assert_eq!(
        sentence(&claim, None),
        "node `approve` is waiting on external review (assignee: lead)"
    );
}

#[test]
fn an_awaiting_review_claim_without_an_assignee_names_only_its_node() {
    let claim = Claim::AwaitsReview {
        node: NodeId::from("approve"),
        assignee: None,
    };
    assert_eq!(
        sentence(&claim, None),
        "node `approve` is waiting on external review"
    );
}

#[test]
fn a_gate_that_cannot_publish_for_want_of_a_forge_says_so_and_names_its_assignee() {
    let claim = Claim::ForgeUnreachable {
        node: NodeId::from("approve"),
        assignee: "lead".to_string(),
    };
    assert_eq!(
        sentence(&claim, None),
        "node `approve` wants an external gate (assignee: lead) but no forge is reachable from this machine (no credentials, or none configured): resolve it here instead"
    );
}

#[test]
fn a_gate_that_cannot_be_checked_again_names_the_reference_it_was_published_at() {
    let claim = Claim::RecheckUnreachable {
        node: NodeId::from("approve"),
        external_ref: "example/repo#1".to_string(),
    };
    assert_eq!(
        sentence(&claim, None),
        "node `approve`'s external gate (published at example/repo#1) needs re-checking, but no forge is reachable from this machine: resolve it here instead"
    );
}

/// A run that spent its token budget is named by its handle, as a person names a run.
#[test]
fn a_run_that_spent_its_token_budget_says_so() {
    let claim = Claim::TokensSpent {
        run: RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA"),
    };
    assert_eq!(
        sentence(&claim, None),
        "run `AAAAAA` exhausted its token budget"
    );
}

#[test]
fn a_loop_that_needs_another_iteration_says_its_cap_is_spent() {
    let claim = Claim::LoopCapSpent {
        node: NodeId::from("implement"),
    };
    assert_eq!(
        sentence(&claim, None),
        "loop `implement` needs another iteration and its cap is spent"
    );
}

#[test]
fn a_scope_request_says_the_session_reason_or_that_a_red_criterion_located_the_writes() {
    let asked = Claim::ScopeAsked {
        task: TaskId::from("task-h"),
        reason: Some("need b.txt".to_string()),
    };
    assert_eq!(
        sentence(&asked, None),
        "task `task-h` requests scope expansion: need b.txt"
    );
    let located = Claim::ScopeAsked {
        task: TaskId::from("task-h"),
        reason: None,
    };
    assert_eq!(
        sentence(&located, None),
        "task `task-h` requests scope expansion: a red criterion of the task locates the refused \
         writes"
    );
}

#[test]
fn a_task_whose_session_departs_from_the_plan_says_so() {
    let claim = Claim::Departs {
        task: TaskId::from("greet"),
    };
    assert_eq!(
        sentence(&claim, None),
        "task `greet`'s session departs from the plan"
    );
}

/// The question a plan's approval asks names the change its summary says: on
/// one line, with the marks it ends on taken off, so the question ends once.
#[test]
fn a_question_about_a_plan_names_its_change() {
    let asked = |summary: &str| {
        sentence(
            &Claim::ApprovePlan {
                summary: summary.to_string(),
            },
            None,
        )
    };
    assert_eq!(asked("Make it"), "Approve the plan: Make it?");
    assert_eq!(asked("Make it."), "Approve the plan: Make it?");
    assert_eq!(asked("Make it!"), "Approve the plan: Make it?");
    assert_eq!(asked("Ship v2?"), "Approve the plan: Ship v2?");
    assert_eq!(
        asked("line one\nline two"),
        "Approve the plan: line one line two?"
    );
}

/// The run that waits on a person in these tests.
const WAITING: RunId = RunId::from_static("01M7SMA11RVN00000000T0KENS");

/// A check that passed at the baseline and exits 101 now.
fn regression() -> Failure {
    Failure::Red {
        red: RedCheck::Regression {
            cmd: "cargo test --workspace".to_string(),
            exit_code: 101,
            tail: Vec::new(),
        },
    }
}

/// A task that asks to touch one path outside its scope, with why.
fn scope_owed() -> Failure {
    Failure::ScopeOwed {
        owed: vec![OwedScope {
            task_id: TaskId::from("global-pack-install"),
            paths: vec!["crates/cli/src/commands/pack_audit.rs".into()],
            reason: Some("the shared helpers change under it".to_string()),
        }],
    }
}

/// A task whose session departs from the plan, which a person accepts or not.
fn departure_owed() -> Failure {
    let departs = DeviationDeclaredPayload {
        task_id: TaskId::from("greet"),
        from: DepartsFrom::Shape("Greeting".to_string()),
        planned: "Hello, and good night".to_string(),
        instead: "Hello".to_string(),
        why: "the second half needs the clock".to_string(),
    };
    Failure::TasksStopped {
        stopped: vec![TaskStop {
            task_id: TaskId::from("greet"),
            cause: TaskCause::DeviationOwed {
                deviations: NonEmpty::from((departs, Vec::new())),
            },
        }],
    }
}

/// The decision `claim` puts to a person on `menu`, whose one option pauses the run.
fn asked(claim: Claim, menu: Menu) -> GateWaitingPayload {
    let abort = GateOption::reserved(ReservedOption::Abort);
    Escalation::new(claim, menu, NonEmpty::from((abort, Vec::new()))).into_payload()
}

/// The decision node `node`'s `failure` puts to a person.
fn failed(node: &str, failure: Failure) -> GateWaitingPayload {
    let menu = FailureMenu {
        attempt: 2,
        failure,
        remedies: Vec::new(),
        stuck: None,
        reroute: None,
    };
    let claim = Claim::NodeFailed {
        node: NodeId::from(node),
    };
    asked(claim, Menu::Failure(Box::new(menu)))
}

/// A failed check's card asks what now, naming the node that failed; one whose reroutes are
/// used up says so, as its claim does.
#[test]
fn a_failed_check_s_card_asks_what_now() {
    assert_eq!(
        question(&failed("tests", regression())),
        "`tests` failed. What now?"
    );
    let claim = Claim::NodeFailed {
        node: NodeId::from("lint"),
    };
    let rerouted = asked(claim, Menu::Failure(Box::new(exhausted(0))));
    assert_eq!(
        question(&rerouted),
        "node `lint` failed and its 0 reroutes to `fix-lint` are exhausted"
    );
}

/// A card on what a task's scope owes asks to touch the path the task asked for, and on a
/// departure asks whether to accept it.
#[test]
fn a_card_on_a_task_s_request_asks_it_in_the_task_s_place() {
    assert_eq!(
        question(&failed("implement", scope_owed())),
        "It asks to touch crates/cli/src/commands/pack_audit.rs"
    );
    assert_eq!(
        question(&failed("implement", departure_owed())),
        "The task departs from its plan. Accept it?"
    );
}

/// A card on a capped loop says the cap is spent, and one on a spent budget names the run by
/// its handle.
#[test]
fn a_card_on_a_cap_says_what_is_spent() {
    let capped = Failure::LoopCapped {
        node: NodeId::from("implement"),
        loop_cap: 2,
    };
    assert_eq!(
        question(&failed("implement", capped)),
        "loop `implement` needs another iteration and its cap is spent"
    );
    let budget = Menu::Budget(yunta_core::events::BudgetCap {
        cap: 100,
        spent: 200,
    });
    assert_eq!(
        question(&asked(Claim::TokensSpent { run: WAITING }, budget)),
        "run `T0KENS` exhausted its token budget"
    );
}

/// A gate's card asks the question its message ends on, and a message of one sentence
/// whole.
#[test]
fn a_gate_s_card_asks_the_question_its_message_ends_on() {
    let gate = |message: &str| {
        let menu = Menu::Gate(yunta_core::events::AuthorGate {
            assignee: "lead".to_string(),
            node: None,
        });
        question(&asked(Claim::Written(message.into()), menu))
    };
    let ship = "The work beside its plan, and every finding the run holds with its answer. \
                Create the PR?";
    assert_eq!(gate(ship), "Create the PR?");
    let notes = "The release notes, as they will be published. Publish them?";
    assert_eq!(gate(notes), "Publish them?");
    assert_eq!(gate("Approve the plan?"), "Approve the plan?");
}

/// A card's far edge names the node it is on, and the run's handle where it is on none.
#[test]
fn a_card_s_edge_names_its_node_or_else_its_run() {
    let node = NodeId::from("implement");
    assert_eq!(
        edge(Some(&node), Some(&WAITING)).as_deref(),
        Some("implement")
    );
    assert_eq!(edge(None, Some(&WAITING)).as_deref(), Some("T0KENS"));
    assert_eq!(edge(None, None), None);
}

/// A run that needs a person, parked on `on`.
fn parked(on: WaitingOn) -> RunFrame {
    RunFrame {
        standing: Standing::NeedsYou,
        phase: RunPhase::Waiting { on },
        ..RunFrame::sample(&WAITING)
    }
}

/// The gist of a run parked on node `node`'s `failure`.
fn failed_gist(node: &str, failure: Failure) -> Option<String> {
    let node = NodeId::from(node);
    let parked = parked(WaitingOn::Run {
        reason: PauseReason::NodeFailed { node, failure },
    });
    gist(&parked, &RunTimes::sample(&parked))
}

/// A run parked on a check's red verdict says the verdict, one parked on a task's request
/// for scope says that a task asks to touch a path outside it, and one parked on a task's
/// departure says that a task departs from what the plan decided.
#[test]
fn a_gist_says_what_a_failed_node_s_card_decides() {
    assert_eq!(
        failed_gist("implement", scope_owed()).as_deref(),
        Some("a task asks to touch a path outside its scope")
    );
    assert_eq!(
        failed_gist("tests", regression()).as_deref(),
        Some("regression: `cargo test --workspace` passed at baseline (exit 0) but now exits 101")
    );
    assert_eq!(
        failed_gist("implement", departure_owed()).as_deref(),
        Some("a task departs from what the plan decided")
    );
}

/// A run parked on a round says how many questions its node asks, and that nothing is
/// recorded until every one is answered.
#[test]
fn a_gist_says_a_round_records_nothing_until_it_is_answered() {
    let asked: Vec<QuestionId> = ["where", "how", "beside"].map(QuestionId::from).to_vec();
    let asked = NonEmpty::new(asked).expect("a round asks");
    let node = NodeId::from("grill");
    let on = WaitingOn::Node {
        node: node.clone(),
        on: NodeWait::Questions {
            asked: asked.clone(),
        },
        reason: Some(PauseReason::Questions {
            node,
            pending: asked,
        }),
    };
    assert_eq!(
        gist(&parked(on), &RunTimes::default()).as_deref(),
        Some("node `grill` asks 3 questions: nothing is recorded until every one is answered")
    );
}

/// A run parked on what a person answers says what holds it in one sentence wherever it
/// stands: its field's row, its glance and its page's gist, for a round, a red check, a task's
/// request for scope and a task's departure alike.
#[test]
fn a_parked_run_s_row_glance_and_gist_say_one_hold() {
    let asked: Vec<QuestionId> = ["where", "how", "beside"].map(QuestionId::from).to_vec();
    let asked = NonEmpty::new(asked).expect("a round asks");
    let grill = NodeId::from("grill");
    let round = WaitingOn::Node {
        node: grill.clone(),
        on: NodeWait::Questions {
            asked: asked.clone(),
        },
        reason: Some(PauseReason::Questions {
            node: grill,
            pending: asked,
        }),
    };
    let failed_on = |node: &str, failure: Failure| WaitingOn::Run {
        reason: PauseReason::NodeFailed {
            node: NodeId::from(node),
            failure,
        },
    };
    for on in [
        round,
        failed_on("tests", regression()),
        failed_on("implement", scope_owed()),
        failed_on("implement", departure_owed()),
    ] {
        let frame = parked(on);
        let times = RunTimes::sample(&frame);
        let said = gist(&frame, &times).expect("a parked run says what holds it");
        let row = crate::page::FieldRow {
            frame: std::sync::Arc::new(frame),
            times,
            menu: true,
        };
        assert_eq!(row.said(), said, "the field's row");
        let glance = crate::page::RunAtAGlance::Read {
            row: &row,
            base_branch: "main",
            isolation: yunta_core::Isolation::Worktree,
            acts: &[],
        };
        let doc = crate::page::Page::document(&glance, &surco::look::Look::plain());
        let glanced = doc.blocks().iter().find_map(|block| match block {
            surco::doc::Block::Gist(gist) => Some(gist.said.clone()),
            _ => None,
        });
        let gisted = surco::components::Gist::new(said).said;
        assert_eq!(glanced, Some(gisted), "the glance, as a gist says it");
    }
}
