//! What a run waits on, through `State` alone: a decision opened from a run parked on a
//! gate, the plan it is on read at its desk and an option picked there recorded; and a
//! round of questions, recorded only once its last question is answered.

mod support;

use std::path::PathBuf;

use surco::doc::Block;
use surco::look::Look;
use surco::surface::{Markdown, Surface};
use yunta_core::events::{ArtifactId, Shown};
use yunta_core::shown::{PlanReview, ShownContent, ShownDocument};
use yunta_core::{
    Answer, ArtifactKind, ContentHash, NodeId, OptionId, QuestionId, RunId, TasksFile,
};
use yunta_testkit::Bench;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, News, Operation, Outcome, Request, State};

use support::{on_the_field, performed, stopped};

/// A gate nobody answers: its run parks on it.
const PARKS: &str = "name: gated\nnodes:\n  - { id: approve, kind: gate, assignee: lead, \
                     message: \"Go on?\", options: [go, stop] }\n";

/// A mock fixture with no sessions: the workflow opens none.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// A node that asks: its session hands a round of questions over and ends.
const ASKS: &str = "name: asking\nnodes:\n  - id: ask\n    kind: prompt\n    runner: executor\n    \
                    prompt: \"Ask what has to be known.\"\n    artifacts:\n      produces: [questions]\n";

/// The session that hands one question over.
const HANDS_THEM_OVER: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_questions
        arguments:
          document:
            questions:
              - { id: summary, text: "What changed?", answer_type: text, required: true }
    outcome: { type: completed, summary: asked }
"#;

/// A plan of one task, which a desk reads.
const PLAN: &str = "summary: Greet a person by name.\ntasks:\n  - id: greet\n    \
                    title: Greet by name\n    scope: [src/greet.rs]\n    criteria:\n      \
                    - { cmd: cargo test, proves: the greeting names the person }\n";

/// The plan, as the document a decision shows.
fn plan_shown() -> ShownDocument {
    ShownDocument {
        shown: Shown {
            producer: Some(NodeId::from("plan")),
            artifact: ArtifactId::Interpreted {
                kind: ArtifactKind::Tasks,
            },
            content_hash: ContentHash::sha256(PLAN.as_bytes()),
        },
        path: PathBuf::from("tasks.yaml"),
        content: ShownContent::Tasks(Box::new(PlanReview {
            plan: yunta_core::yaml::parse::<TasksFile>(PLAN).unwrap(),
            departed: Vec::new(),
            spec: None,
            spec_node: None,
            asked: None,
            held: Vec::new(),
            tasks: Vec::new(),
            handed_over: None,
            answered: Vec::new(),
            shapes_read: None,
        })),
    }
}

/// The ticket `asked` asks `kind`'s work under.
fn asked_for(asked: &[Request], kind: fn(&Operation) -> bool) -> yunta_ui::Ticket {
    asked
        .iter()
        .find_map(|request| match request {
            Request::Perform(ticket, operation) if kind(operation) => Some(*ticket),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{asked:?}"))
}

/// The application on the decision `bench`'s parked run waits on, its card put to the
/// person with the plan beside it among what it shows: the state, and the run. The run
/// is read for a round first, as `resolve-gate` reads it, and waits on none.
async fn on_the_decision(bench: &Bench) -> (State, RunId) {
    let run = bench.run_id.clone();
    let following = Operation::Follow { run: run.clone() };
    let (found, _) = performed(bench, following).await;
    let Some(News::Following(read)) = found.into_iter().next() else {
        panic!("the following hands back the run as it reads first");
    };
    let mut state = on_the_field(vec![stopped(&run)]);
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let followed = asked_for(&asked, |work| matches!(work, Operation::Follow { .. }));
    state.update(Input::News(followed, News::Following(read)));
    let asked = state.update(Input::Stroke(Stroke::Insert('a')));
    let round = asked_for(&asked, |work| {
        matches!(work, Operation::ReadQuestions { .. })
    });
    let (_, none) = performed(bench, Operation::ReadQuestions { run: run.clone() }).await;
    let asked = state.update(Input::Outcome(round, none));
    let reading = asked_for(&asked, |work| {
        matches!(work, Operation::ReadDecision { .. })
    });
    let (_, read) = performed(bench, Operation::ReadDecision { run: run.clone() }).await;
    let Outcome::Decision(mut waiting) = read else {
        panic!("the run waits on its gate: {read:?}");
    };
    waiting.shown.push(plan_shown());
    state.update(Input::Outcome(reading, Outcome::Decision(waiting)));
    (state, run)
}

/// Answering a run parked on a gate opens its card; asking to read turns to the plan's
/// desk, where a number picked and confirmed records that option.
#[tokio::test]
async fn reading_turns_to_the_desk_and_a_number_picked_there_decides() {
    let bench = Bench::with_run_id("01M7DEC1DEDATDESK000000000");
    bench.run(PARKS, NO_SESSIONS).await;
    let (mut state, run) = on_the_decision(&bench).await;
    let card = state.screen(&Look::plain());
    assert!(
        matches!(card.main.doc.blocks(), [Block::Card(_)]),
        "the card stands first"
    );

    state.update(Input::Stroke(Stroke::Insert('r')));
    let desk = state.screen(&Look::plain());
    assert!(
        matches!(desk.main.doc.blocks(), [Block::Desk(_)]),
        "the desk takes the screen"
    );
    state.update(Input::Stroke(Stroke::Insert('1')));
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let [Request::Perform(
        _,
        Operation::Decide {
            run: decided,
            option,
            words,
        },
    )] = asked.as_slice()
    else {
        panic!("the option is recorded: {asked:?}");
    };
    assert_eq!(decided, &run);
    assert_eq!(option, &OptionId::from("go"));
    assert_eq!(words, &None);
}

/// The application on the round `bench`'s run is parked on, put to the person.
async fn on_the_round(bench: &Bench) -> State {
    let run = bench.run_id.clone();
    let following = Operation::Follow { run: run.clone() };
    let (found, _) = performed(bench, following).await;
    let Some(News::Following(read)) = found.into_iter().next() else {
        panic!("the following hands back the run as it reads first");
    };
    let mut state = on_the_field(vec![stopped(&run)]);
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let followed = asked_for(&asked, |work| matches!(work, Operation::Follow { .. }));
    state.update(Input::News(followed, News::Following(read)));
    let asked = state.update(Input::Stroke(Stroke::Insert('a')));
    let reading = asked_for(&asked, |work| {
        matches!(work, Operation::ReadQuestions { .. })
    });
    let (_, read) = performed(bench, Operation::ReadQuestions { run }).await;
    state.update(Input::Outcome(reading, read));
    state
}

/// Each character of `words` typed, one stroke each.
fn typed(state: &mut State, words: &str) -> Vec<Request> {
    words
        .chars()
        .flat_map(|character| state.update(Input::Stroke(Stroke::Insert(character))))
        .collect()
}

/// A round left before its last answer records nothing, and goes back to the run.
#[tokio::test]
async fn a_round_left_before_its_last_answer_records_nothing() {
    let bench = Bench::with_run_id("01M7R0VNDLEFT0000000000000");
    bench.run(ASKS, HANDS_THEM_OVER).await;
    let mut state = on_the_round(&bench).await;
    let round = Markdown.draw(&state.screen(&Look::plain()).main.doc);
    assert!(round.contains("What changed?"), "{round}");

    let mut asked = typed(&mut state, "half an answer");
    asked.extend(state.update(Input::Stroke(Stroke::Decline)));
    assert!(
        !asked
            .iter()
            .any(|request| matches!(request, Request::Perform(_, Operation::Answer { .. }))),
        "{asked:?}"
    );
    let trail = state.screen(&Look::plain()).trail;
    assert_eq!(trail.len(), 2, "back on the run: {trail:?}");
}

/// The round's last answer records every answer, to the node that asked.
#[tokio::test]
async fn the_rounds_last_answer_records_every_answer() {
    let bench = Bench::with_run_id("01M7R0VNDANSWERED000000000");
    bench.run(ASKS, HANDS_THEM_OVER).await;
    let mut state = on_the_round(&bench).await;

    let mut asked = typed(&mut state, "the greeting");
    asked.extend(state.update(Input::Stroke(Stroke::Enter)));
    let [Request::Perform(_, Operation::Answer { node, answers, .. })] = asked.as_slice() else {
        panic!("the answers are recorded: {asked:?}");
    };
    assert_eq!(node, &NodeId::from("ask"));
    let summary = Answer {
        id: QuestionId::from("summary"),
        value: "the greeting".to_string(),
    };
    assert_eq!(answers, &[summary]);
}
