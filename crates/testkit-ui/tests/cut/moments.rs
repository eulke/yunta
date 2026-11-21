//! The moment each target's cut stands at, where the first targets showed it wrong: a run's
//! numbers and its graph where the run needs the person at its last gate; a card on a gate
//! that shows a document that is not a plan, with the menu the engine rebuilds for it; an
//! answer recorded, the log gone on past it to the next node's start; a round whose
//! question in turn is answered by typing, its keys typing the answer; and the screen of a
//! promoted run once the person leads it.

use yunta_api::{Awaited, FollowNews, Followed, RunNews, WaitingDecision};
use yunta_core::events::{EventPayload, GateEvent, GateResolvedPayload, Menu, StoredEvent};
use yunta_core::shown::ShownContent;
use yunta_core::{AnswerType, Question, QuestionsFile, RunId};
use yunta_replay::frame::{RunPhase, WaitingOn};
use yunta_testkit_ui::cut::{Cut, Stands, TargetCut, TARGET_CUTS};

use crate::{following, next, read_whole};

/// The target named `name`, by its row of `TARGET_CUTS`.
fn target(name: &str) -> &'static TargetCut {
    TARGET_CUTS
        .iter()
        .find(|target| target.name == name)
        .unwrap_or_else(|| panic!("no target is named `{name}`"))
}

/// The run `name`'s screen shows, as its cut holds it, and the cut.
async fn in_view(name: &str) -> (Cut, RunId) {
    let cut = Cut::of_target(name).await;
    let run = cut.runs()[0].clone();
    (cut, run)
}

/// What the run `run` of `cut` asks, as the follower hands it once for its pause.
async fn asked(cut: &Cut, run: &RunId) -> Awaited {
    let followed = Followed {
        field: None,
        run: Some(run.clone()),
    };
    let mut following = following(cut, followed).await;
    loop {
        match next(&mut following, "what the run asks").await {
            FollowNews::Run {
                news: RunNews::Awaited(Some((_, asked))),
                ..
            } => return asked,
            FollowNews::Run {
                news: RunNews::AwaitedUnread { why, .. },
                ..
            } => panic!("what the run asks could not be read: {why}"),
            _ => {}
        }
    }
}

/// The node a run's last gate is: the design's run is let through `ship` last.
const LAST_GATE: &str = "ship";

#[tokio::test]
async fn a_runs_numbers_and_its_graph_are_cut_where_it_needs_the_person_at_its_last_gate() {
    for name in ["stats", "graph"] {
        let (cut, run) = in_view(name).await;
        let (read, _) = read_whole(&cut, &run).await;
        let frame = read.frame(cut.now());
        let waits_at = match frame.phase.waiting_on() {
            Some(WaitingOn::Node { node, .. }) => Some(node.to_string()),
            _ => None,
        };
        assert_eq!(
            waits_at.as_deref(),
            Some(LAST_GATE),
            "`{name}`: {:?}",
            frame.phase
        );
    }
}

#[tokio::test]
async fn a_card_stands_on_a_gate_that_shows_a_document_that_is_not_a_plan_and_reads() {
    let (cut, run) = in_view("card").await;
    let Awaited::Decision(decision) = asked(&cut, &run).await else {
        panic!("`card` asks a decision, not a round");
    };
    let WaitingDecision {
        escalation, shown, ..
    } = &*decision;
    assert!(
        matches!(escalation.menu(), Some(Menu::Gate(_))),
        "the menu the engine rebuilds is the gate's: {:?}",
        escalation.menu()
    );
    assert!(!shown.is_empty(), "the gate shows no document");
    for document in shown {
        assert!(
            !matches!(document.content, ShownContent::Tasks(_)),
            "the gate shows a plan at {}",
            document.path.display()
        );
    }
}

/// The question in turn of `round`: the first a person has not answered yet, which is
/// the first, since nothing of a round is recorded until its last answer.
fn in_turn(round: &QuestionsFile) -> &Question {
    round.questions.first().expect("a round asks a question")
}

#[tokio::test]
async fn a_round_of_the_decisions_holds_a_question_answered_by_typing_and_its_keys_type_it() {
    let rounds = TARGET_CUTS
        .iter()
        .filter(|target| target.name.starts_with("round"))
        .filter(|target| matches!(target.stands, Stands::Run(_)));
    let mut typed = Vec::new();
    for each in rounds {
        let (cut, run) = in_view(each.name).await;
        let Awaited::Round(round) = asked(&cut, &run).await else {
            continue;
        };
        let question = in_turn(&round.questions);
        let types = each.keys.iter().any(|key| key.starts_with("type:"));
        if question.answer_type == AnswerType::Text && question.values.is_empty() && types {
            typed.push(each.name);
        }
    }
    assert!(
        !typed.is_empty(),
        "no round's question in turn is answered by typing, its answer typed by the keys"
    );
}

/// Each target of an answer recorded, the option `enter` records on its pause, and the node
/// the run goes on to start once it is recorded: the design's notes, `Recorded: approve. On
/// to `pr`.`, `Recorded: retry.`, `Recorded: grant.`
const RECORDED: [(&str, &str, &str); 3] = [
    ("card-recorded", "approve", "pr"),
    ("failure-recorded", "retry", "tests"),
    ("scope-recorded", "grant", "implement"),
];

#[test]
fn keys_go_on_only_where_one_run_is_in_view() {
    for each in TARGET_CUTS {
        assert_eq!(
            each.goes_on_to.is_some(),
            each.gone_on().is_some(),
            "`{}` goes on where no one run is in view",
            each.name
        );
    }
}

/// Whether `events` record `option` chosen.
fn chose(events: &[&StoredEvent], option: &str) -> bool {
    events.iter().any(|event| {
        matches!(event.payload(), Some(EventPayload::Gates(GateEvent::Resolved(resolved)))
            if matches!(resolved, GateResolvedPayload::Chosen(choice) if choice.option.as_str() == option))
    })
}

#[tokio::test]
async fn an_answer_recorded_is_cut_where_the_run_went_on_to_its_next_nodes_start() {
    for (name, option, next_node) in RECORDED {
        let recorded = target(name);
        assert_eq!(recorded.keys, ["enter"], "`{name}`");
        let (Stands::Run(paused), Some(Stands::Run(moved))) = (recorded.stands, recorded.gone_on())
        else {
            panic!("`{name}`'s keys leave its run's log where it stands");
        };
        assert_eq!(
            moved.built, paused.built,
            "`{name}` goes on with another run"
        );
        let cut = Cut::of(&Stands::Run(moved).runs()).await;
        let (read, events) = read_whole(&cut, &cut.runs()[0]).await;
        let after: Vec<&StoredEvent> = events
            .iter()
            .filter(|event| event.seq.get() > paused.seq)
            .collect();
        assert!(
            chose(&after, option),
            "`{name}`: no `{option}` recorded after the pause"
        );
        let last = after.last().expect("the log goes on past the pause");
        let node = last.node_id.as_ref().map(|node| node.as_str());
        assert_eq!(
            (last.body.kind_name(), node),
            ("node_started", Some(next_node)),
            "`{name}` is cut at seq {}",
            moved.seq
        );
        let frame = read.frame(cut.now());
        assert!(
            matches!(frame.phase, RunPhase::Running),
            "`{name}`: {:?}",
            frame.phase
        );
    }
}

/// The keys that move the node pointer along a run's nodes: the person takes it.
const ALONG_THE_NODES: [&str; 2] = ["j", "k"];

#[test]
fn the_promoted_runs_screen_is_cut_once_the_person_leads_it() {
    let promoted = target("promoted");
    assert!(
        promoted
            .keys
            .iter()
            .any(|key| ALONG_THE_NODES.contains(key)),
        "`promoted` presses {:?}: the run leads, and its successor takes the screen",
        promoted.keys
    );
}
