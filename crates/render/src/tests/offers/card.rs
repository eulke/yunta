//! Each option as a decision's card says it beside its name: what it does, then what it
//! costs, in the words of the screen a person answers it on, and where an option goes on to
//! a node, what that node does as the run froze it.

use yunta_core::ahead::NodesAhead;
use yunta_core::events::{
    AuthorGate, BudgetCap, Claim, DepartsFrom, DeviationDeclaredPayload, Escalation, Failure,
    FailureMenu, GateOption, Leads, Menu, OwedScope, Precondition, Remedy, ScopeWait, TaskCause,
    TaskStop,
};
use yunta_core::expansion::Waiting;
use yunta_core::reserved::ReservedOption;
use yunta_core::{NonEmpty, RunId};

use super::{menu, words};

/// What a card says beside option `id` of `escalation`: what it does, then what it costs,
/// as one sentence, the name alone where the menu words neither.
fn card_says(escalation: &Escalation, id: &str) -> String {
    let (label, tradeoff) = words(escalation, id);
    let label = (label != id).then_some(label);
    label
        .into_iter()
        .chain(tradeoff)
        .collect::<Vec<_>>()
        .join(": ")
}

/// The menu node `implement`'s `failure` offers, with `remedies` and an abort.
fn failed(failure: Failure, remedies: Vec<Remedy>) -> Escalation {
    let options = remedies
        .iter()
        .map(|remedy| GateOption::reserved(remedy.option))
        .chain([GateOption::reserved(ReservedOption::Abort)])
        .collect();
    let failure = FailureMenu {
        attempt: 2,
        failure,
        remedies,
        stuck: None,
        reroute: None,
    };
    let node = "implement".into();
    menu(
        Claim::NodeFailed { node },
        Menu::Failure(Box::new(failure)),
        options,
    )
}

/// A remedy that waits for nothing.
fn offered(option: ReservedOption) -> Remedy {
    Remedy {
        option,
        precondition: None,
    }
}

/// A task's request to touch one path outside its scope, owed a person's answer.
fn scope_owed() -> Failure {
    Failure::ScopeOwed {
        owed: vec![OwedScope {
            task_id: "global-pack-install".into(),
            paths: vec!["crates/cli/src/commands/pack_audit.rs".into()],
            reason: None,
        }],
    }
}

/// A task whose session departed from the plan, owed a person's answer.
fn departure_owed() -> Failure {
    let departs = DeviationDeclaredPayload {
        task_id: "greet".into(),
        from: DepartsFrom::Shape("Greeting".to_string()),
        planned: "Hello, and good night".to_string(),
        instead: "Hello".to_string(),
        why: "the second half needs the clock".to_string(),
    };
    Failure::TasksStopped {
        stopped: vec![TaskStop {
            task_id: "greet".into(),
            cause: TaskCause::DeviationOwed {
                deviations: NonEmpty::from((departs, Vec::new())),
            },
        }],
    }
}

/// A task's request for scope offers its grant and its denial in the card's words, on the
/// menu a parked run rebuilds and on the one the engine asks at its terminal.
#[test]
fn a_scope_card_offers_grant_and_deny_in_the_card_s_words() {
    let owed = failed(
        scope_owed(),
        vec![
            offered(ReservedOption::Grant),
            offered(ReservedOption::Deny),
        ],
    );
    let asked = menu(
        Claim::ScopeAsked {
            task: "global-pack-install".into(),
            reason: None,
        },
        Menu::Scope(ScopeWait {
            because: Waiting::ModeAsk,
            paths: vec!["crates/cli/src/commands/pack_audit.rs".into()],
            proposed: None,
        }),
        vec![
            GateOption::reserved(ReservedOption::Grant),
            GateOption::reserved(ReservedOption::Deny),
        ],
    );
    for scope in [owed, asked] {
        let grant = "the task may also touch this path, and goes on";
        assert_eq!(card_says(&scope, "grant"), grant);
        let deny = "the task goes on within the scope it has";
        assert_eq!(card_says(&scope, "deny"), deny);
    }
}

/// A failed check's retry says what a fresh attempt starts from and what it asks again.
#[test]
fn a_failed_check_s_retry_reads_as_the_card_says_it() {
    let retry = Remedy {
        option: ReservedOption::Retry,
        precondition: Some(Precondition::TreeChanged),
    };
    let red = Failure::Message {
        outcome: "exit 101".into(),
    };
    assert_eq!(
        card_says(&failed(red, vec![retry]), "retry"),
        "Starts a fresh attempt from the run's tree as it stands: what failed is asked again \
         and what passed stays passed"
    );
}

/// A departure owed offers to continue from the blocked work, to accept it, and to send its
/// session back, each in the card's words.
#[test]
fn a_departure_card_offers_its_answers_in_the_card_s_words() {
    let remedies = [
        ReservedOption::ContinueWork,
        ReservedOption::Accept,
        ReservedOption::SendBack,
    ];
    let departure = failed(departure_owed(), remedies.map(offered).to_vec());
    let continues = "keeps the blocked task's last changes and judges them before any session \
                     opens";
    assert_eq!(card_says(&departure, "continue-work"), continues);
    let accepts = "the work stands as it is, and the plan records why it differs";
    assert_eq!(card_says(&departure, "accept"), accepts);
    let sends = "the session goes back to what the plan says";
    assert_eq!(card_says(&departure, "send-back"), sends);
}

/// A capped loop's retry and a spent budget's continue say what each does to its cap.
#[test]
fn a_cap_s_card_offers_to_go_past_it_in_the_card_s_words() {
    let capped = Failure::LoopCapped {
        node: "implement".into(),
        loop_cap: 2,
    };
    let looped = failed(capped, vec![offered(ReservedOption::Retry)]);
    let again = "its iteration count starts over; it stops at the cap again if tasks remain";
    assert_eq!(card_says(&looped, "retry"), again);
    let budget = menu(
        Claim::TokensSpent {
            run: RunId::from_static("01M7SMA11RVN00000000T0KENS"),
        },
        Menu::Budget(BudgetCap {
            cap: 100,
            spent: 200,
        }),
        vec![GateOption::reserved(ReservedOption::Continue)],
    );
    let lifts = "Lifts the cap for this invocation only; a later resume will ask again before \
                 spending more";
    assert_eq!(card_says(&budget, "continue"), lifts);
}

/// The ship's gate: its approve goes on to `next`, settling the findings it shows.
fn ship(next: &str) -> Escalation {
    let approve = GateOption::leading(
        ReservedOption::Approve.id(),
        Leads::On {
            next: vec![next.into()],
            settles: true,
        },
    );
    let gate = Menu::Gate(AuthorGate {
        assignee: "lead".into(),
        node: None,
    });
    let claim = Claim::Written("Create the PR?".into());
    menu(
        claim,
        gate,
        vec![approve, GateOption::reserved(ReservedOption::Abort)],
    )
}

/// What lies ahead at `pr`: a pull request opened on GitHub's `eulke/yunta`.
fn opening() -> NodesAhead {
    let config = "forge:\n  github: { repo: eulke/yunta, token_env: FORGE_TOKEN }\n";
    ahead_of(config)
}

/// What lies ahead at the nodes of a run that ships through `pr`, under `config`.
fn ahead_of(config: &str) -> NodesAhead {
    let workflow = "name: w\nnodes:\n  - { id: ship, kind: gate, assignee: lead, message: Ship?, \
                    options: [approve] }\n  - { id: pr, kind: pull_request, title: it, optional: \
                    true, depends_on: [ship] }\n";
    let workflow: yunta_core::Workflow =
        yunta_core::workflow::read::read(workflow, std::path::Path::new("w.yaml")).unwrap();
    let config: yunta_core::ConfigLayer = yunta_core::yaml::parse(config).unwrap();
    let left = yunta_core::left_out(&workflow, &config);
    NodesAhead::of(&workflow, &config, &left)
}

/// The ship's approve says it opens the pull request on the forge and the repository the
/// run opens it on, and an approve that goes on to a node of another kind names it alone.
#[test]
fn an_approve_says_what_the_node_it_goes_on_to_does() {
    let opens = "On to `pr`: opens the pull request on github eulke/yunta";
    assert_eq!(card_says_ahead(&ship("pr"), "approve", &opening()), opens);
    let resolves = "On to `implement`: resolves this gate, and settles every finding it shows";
    assert_eq!(
        card_says_ahead(&ship("implement"), "approve", &opening()),
        resolves
    );
}

/// With no forge declared the run leaves `pr` out, and the approve says that is where it
/// would have gone, and that it resolves the gate.
#[test]
fn an_approve_past_a_node_the_run_leaves_out_says_it_is_not_in_the_run() {
    let left = ahead_of("{}\n");
    let resolves = "Resolve the gate: `pr` is not in this run: the project declares no \
                    `forge`, and settles every finding it shows";
    assert_eq!(card_says_ahead(&ship("pr"), "approve", &left), resolves);
}

/// What a card says beside option `id`, read with what lies `ahead`.
fn card_says_ahead(escalation: &Escalation, id: &str, ahead: &NodesAhead) -> String {
    let payload = escalation.clone().into_payload();
    let option = payload
        .options()
        .iter()
        .find(|option| option.id.as_str() == id)
        .expect("on the menu");
    let (label, tradeoff) = crate::offers::option_words(&payload, option, ahead);
    let label = (label != id).then_some(label);
    label
        .into_iter()
        .chain(tradeoff)
        .collect::<Vec<_>>()
        .join(": ")
}
