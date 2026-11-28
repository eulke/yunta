//! A round of one question in place, through `State` alone, on a run a bench parks on it:
//! brought back with `a` after it was put aside, it records nothing when put aside again
//! before its last answer, and every answer at its last, to the node that asked. Its answer
//! is noted in the order a following that reads the round again as the answer lands hands it,
//! the answer's outcome coming last. Once the log holds its answer and before anything says
//! nothing waits, the round is out of sight and speaks no more: no line is typed, the legend
//! is the run's, and a key acts on the run's screen.

use surco::glyphs::{Glyphs, Mark};
use surco::look::Look;
use surco::surface::{Markdown, Surface};
use yunta_api::{FollowNews, RunNews};
use yunta_core::{Answer, NodeId, QuestionId, RunId};
use yunta_testkit::Bench;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Outcome, Request, Scene, State};

use crate::support::{on_the_field, read_whole_and_asked};

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

/// The application on the round `bench`'s run is parked on, once the following read it: the
/// round standing in place on the run's screen, where `a` types, is put aside first, and `a`
/// then brings it back, the attention row naming the run's own decision.
async fn on_the_round(bench: &Bench) -> State {
    let run = bench.run_id.clone();
    let read = read_whole_and_asked(bench).await;
    let (mut state, _) = on_the_field(&[&run]).await;
    state.update(Input::Stroke(Stroke::Enter));
    for news in read {
        state.update(Input::FollowNews(news));
    }
    state.update(Input::Stroke(Stroke::Decline));
    let asked = state.update(Input::Stroke(Stroke::Insert('a')));
    assert!(
        asked.is_empty(),
        "the round stands in place again: {asked:?}"
    );
    state
}

/// Each character of `words` typed, one stroke each.
fn typed(state: &mut State, words: &str) -> Vec<Request> {
    words
        .chars()
        .flat_map(|character| state.update(Input::Stroke(Stroke::Insert(character))))
        .collect()
}

/// A round brought back with `a` and put aside again before its last answer records nothing,
/// the person on the node that asks.
#[tokio::test]
async fn a_round_brought_back_and_left_before_its_last_answer_records_nothing() {
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
    // On the node that asks, which the trail names.
    let trail = state.screen(&Look::plain()).trail;
    assert_eq!(trail.last().map(String::as_str), Some("ask"), "{trail:?}");
}

/// A round brought back with `a` records every answer at its last, to the node that asked.
#[tokio::test]
async fn a_round_brought_back_records_every_answer_at_its_last() {
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

/// An answer given on the round, which a following reads again as the answer lands: what
/// the round asks, read as of the head the following holds, then the events that record the
/// answer, then that nothing waits, and the answer's outcome last. The round goes with its
/// pause, and the note says what the answers recorded do.
#[tokio::test]
async fn an_answer_whose_round_is_read_again_as_it_lands_is_noted_with_its_outcome_last() {
    let bench = Bench::with_run_id("01M7R0VNDREADAGA1N00000000");
    bench.run(ASKS, HANDS_THEM_OVER).await;
    let mut state = on_the_round(&bench).await;
    let read_again: Vec<FollowNews> = read_whole_and_asked(&bench).await;
    let in_view = state.followed_runs().in_view.as_ref();
    let held = in_view.and_then(|in_view| in_view.run.state().head());

    let mut asked = typed(&mut state, "the greeting");
    asked.extend(state.update(Input::Stroke(Stroke::Enter)));
    let [Request::Perform(ticket, Operation::Answer { .. })] = asked.as_slice() else {
        panic!("the answers are asked to be recorded: {asked:?}");
    };
    let recorded = bench.answer_round("ask", &[("summary", "the greeting")]);
    recorded.await.expect("the answers are recorded");

    let run = bench.run_id.clone();
    let what_it_asks = read_again.into_iter().skip_while(|news| !asks(news));
    let past = bench
        .events()
        .into_iter()
        .filter(|event| Some(event.seq) > held);
    let events = RunNews::Events(past.collect());
    let in_order = what_it_asks
        .chain([FollowNews::Run {
            run: run.clone(),
            news: events,
        }])
        .chain([FollowNews::Run {
            run: run.clone(),
            news: RunNews::Awaited(None),
        }]);
    for news in in_order {
        state.update(Input::FollowNews(news));
    }
    state.update(Input::Outcome(*ticket, Outcome::Answered { run }));

    let drawn = state.screen(&Look::plain());
    assert!(
        !matches!(state.scene(), Some(Scene::Round)),
        "the round went"
    );
    let noted = drawn.noted.expect("the answers recorded are noted");
    assert_eq!(noted.mark, Mark::Done, "{}", noted.said);
    assert_eq!(
        noted.said,
        "Recorded: 1 answer. `ask` finishes with it, and the run goes on."
    );
}

/// The round's answer on the log, and nothing yet saying that nothing waits: the round is
/// out of sight, so the screen speaks as the run's does once nothing waits, no line typed and
/// its legend the run's, and `?` opens help over it, as on any run's screen.
#[tokio::test]
async fn a_round_out_of_sight_with_its_answer_on_the_log_types_nothing_and_its_keys_are_the_runs() {
    let bench = Bench::with_run_id("01M7R0VNDS1GHT000000000000");
    bench.run(ASKS, HANDS_THEM_OVER).await;
    let mut state = on_the_round(&bench).await;
    assert!(state.typing(), "a line is typed on the round in place");
    let run = answered_on_the_log(&bench, &mut state).await;
    assert!(
        !matches!(state.scene(), Some(Scene::Round)),
        "the round is out of sight: {:?}",
        state.scene()
    );
    let mut nothing_waits = state.clone();
    let news = FollowNews::Run {
        run,
        news: RunNews::Awaited(None),
    };
    nothing_waits.update(Input::FollowNews(news));
    assert!(!state.typing(), "no line is typed out of sight");
    assert!(
        !state.screen(&Look::plain()).typed,
        "no line is drawn typed"
    );
    assert_eq!(
        legend_and_far_keys(&state),
        legend_and_far_keys(&nothing_waits),
        "the legend is the run's"
    );

    let asked = state.update(Input::Stroke(Stroke::Insert('?')));
    assert!(asked.is_empty(), "{asked:?}");
    let overlay = state.screen(&Look::plain()).overlay;
    let title = overlay.map(|overlay| overlay.title);
    assert_eq!(title.as_deref(), Some(yunta_render::screen::HELP));
}

/// The round `state` holds in place answered on `bench`'s log, as another process records
/// it, and the events that record it handed as a following hands them, nothing after them:
/// the run they are of.
async fn answered_on_the_log(bench: &Bench, state: &mut State) -> RunId {
    let in_view = state.followed_runs().in_view.as_ref();
    let held = in_view.and_then(|in_view| in_view.run.state().head());
    let recorded = bench.answer_round("ask", &[("summary", "the greeting")]);
    recorded.await.expect("the answers are recorded");
    let past = bench
        .events()
        .into_iter()
        .filter(|event| Some(event.seq) > held);
    let run = bench.run_id.clone();
    let news = FollowNews::Run {
        run: run.clone(),
        news: RunNews::Events(past.collect()),
    };
    state.update(Input::FollowNews(news));
    run
}

/// What `state`'s screen says in its legend and in its far keys, as Unicode spells them.
fn legend_and_far_keys(state: &State) -> (String, String) {
    let drawn = state.screen(&Look::plain());
    (
        super::support::spelled(&drawn.legend, Glyphs::Unicode),
        super::support::spelled(&drawn.anywhere, Glyphs::Unicode),
    )
}

/// Whether `news` says what a run waits on a person for.
fn asks(news: &FollowNews) -> bool {
    matches!(
        news,
        FollowNews::Run {
            news: RunNews::Awaited(_) | RunNews::AwaitedUnread { .. },
            ..
        }
    )
}
