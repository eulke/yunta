//! What a node's sessions said, as the state keeps it of the run in view: let go once the
//! node stops working, so a node working again keeps none of its last session's words, and
//! not heard again when the same news is handed again.

use yunta_api::{FollowNews, RunNews};
use yunta_core::events::{EventPayload, NodeEvent, NodeState, StoredEvent};
use yunta_core::port::Said;
use yunta_core::{NodeId, SessionId};
use yunta_testkit::Bench;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, State};

use super::support::on_the_field;
use super::{followed, read_whole, NO_SESSIONS};

/// A check that fails once and sends the run to its fix, then runs again: a node that
/// works, ends, and works again.
const WORKS_AGAIN: &str = "name: again\nnodes:\n  - id: check\n    kind: bash\n    \
                           run: \"test -f fixed.txt\"\n    \
                           on_failure: { goto: fix, max_reroutes: 1 }\n  \
                           - { id: fix, kind: bash, run: \"touch fixed.txt\" }\n";

/// Where among `events` `node` met what `kind` names, in order.
fn where_node(events: &[StoredEvent], node: &NodeId, kind: fn(&NodeEvent) -> bool) -> Vec<usize> {
    let met = |event: &StoredEvent| match event.payload() {
        Some(EventPayload::Node(payload)) => event.node_id.as_ref() == Some(node) && kind(payload),
        _ => false,
    };
    let at = events.iter().enumerate().filter(|(_, event)| met(event));
    at.map(|(at, _)| at).collect()
}

/// What the check of [`WORKS_AGAIN`] says while it works first.
const WORDS: &str = "Reading the check first";

/// A run whose check said [`WORDS`], ended and works again, opened from the field: the
/// state on its stage, the news up to the words said, and the news on to the check
/// working again, in the two batches a follower hands across its end.
struct WorkedAgain {
    state: State,
    saying: [FollowNews; 2],
    working_again: [FollowNews; 2],
}

/// [`WorkedAgain`], made of a bench's run of [`WORKS_AGAIN`].
async fn worked_again() -> WorkedAgain {
    let bench = Bench::with_run_id("01M7W0RKSAGA1N000000000000");
    bench.run(WORKS_AGAIN, NO_SESSIONS).await;
    let (whole, events) = followed(&bench).await;
    let check = NodeId::from("check");
    let started = where_node(&events, &check, |kind| {
        matches!(kind, NodeEvent::Started(_))
    });
    let failed = where_node(&events, &check, |kind| matches!(kind, NodeEvent::Failed(_)));
    let ([first, again, ..], [ended, ..]) = (started.as_slice(), failed.as_slice()) else {
        panic!("the check works, fails, and works again: {started:?} {failed:?}");
    };
    let said = Said {
        run_id: whole.run().clone(),
        node_id: check,
        session_id: SessionId::from_static("session-1"),
        task_id: None,
        seq: events[*first].seq,
        at: events[*first].timestamp,
        text: WORDS.to_string(),
        cut: false,
    };
    let of_the_run = |news| FollowNews::Run {
        run: whole.run().clone(),
        news,
    };
    let (mut state, _) = on_the_field(&[whole.run()]).await;
    state.update(Input::Stroke(Stroke::Enter));
    WorkedAgain {
        state,
        saying: [
            read_whole(&whole, &events[..=*first]),
            of_the_run(RunNews::Said { said, missed: 2 }),
        ],
        working_again: [
            of_the_run(RunNews::Events(events[first + 1..=*ended].to_vec())),
            of_the_run(RunNews::Events(events[ended + 1..=*again].to_vec())),
        ],
    }
}

/// What `state` keeps of what the check of the run in view said last, and whether the check
/// is at work, as the run in view's log has it.
fn check_heard(state: &State) -> (Option<String>, bool) {
    let in_view = state.followed_runs().in_view.as_deref();
    let in_view = in_view.expect("the run is in view");
    let check = NodeId::from("check");
    let said = in_view.heard.of(in_view.run.run(), &check);
    let record = in_view.run.state().nodes.get(&check);
    let works = record.and_then(|record| record.state.as_ref());
    let works = matches!(works, Some(NodeState::Running { .. }));
    (said.map(|said| said.text.clone()), works)
}

/// A node whose session said words, which ended and works again, keeps none of its last
/// session's words: what a node said is let go once it stops working.
#[tokio::test]
async fn a_node_working_again_keeps_none_of_its_last_sessions_words() {
    let WorkedAgain {
        mut state,
        saying,
        working_again,
    } = worked_again().await;

    for news in saying {
        state.update(Input::FollowNews(news));
    }
    assert_eq!(check_heard(&state), (Some(WORDS.to_string()), true));
    for news in working_again {
        state.update(Input::FollowNews(news));
    }
    assert_eq!(check_heard(&state), (None, true));
}

/// The news of a node that said words, ended and works again, handed twice, leaves the
/// state as handed once: the words let go are not heard again, nor their missed words
/// counted again.
#[tokio::test]
async fn words_let_go_and_handed_again_are_not_heard_again() {
    let WorkedAgain {
        state,
        saying,
        working_again,
    } = worked_again().await;
    let (mut once, mut twice) = (state.clone(), state);

    let all = saying.iter().chain(&working_again);
    for news in all.clone() {
        once.update(Input::FollowNews(news.clone()));
    }
    for news in all.clone().chain(all) {
        twice.update(Input::FollowNews(news.clone()));
    }
    assert_eq!(format!("{twice:?}"), format!("{once:?}"));
}
