//! The field driven end to end: runs this build started, picked and closed after one
//! confirmation, and found among the closed runs by what is followed, with no reading of
//! the field asked.

use super::*;

use tokio::sync::mpsc;
use yunta_render::screen::Confirming;
use yunta_testkit::{full_run_id, handle, run_id_from, yunta_at, Checkout};
use yunta_ui::{Input, Operation};

/// A build that fails at once: its run stops on a person.
const FAILS: &str = "name: fails\nnodes:\n  - { id: build, kind: bash, run: \"exit 101\" }\n";

/// Two runs stopped on a person, picked and closed after one confirmation that names both,
/// leave the runs the field lists at a glance and stand among its closed runs in full, with
/// no reading of the field asked.
#[tokio::test]
async fn two_picked_runs_are_closed_and_found_among_the_closed_runs() {
    let checkout = Checkout::new()
        .working_in_place()
        .workflow("wf", FAILS)
        .committed();
    let runs: Vec<String> = (0..2)
        .map(|_| {
            let run = yunta_at!(&checkout, &["run", "wf.yaml"]);
            assert!(!run.status.success(), "the build fails and the run stops");
            full_run_id(&checkout.home, &run_id_from(&run))
        })
        .collect();
    let performer = Arc::new(Performer::new(
        workspace(&checkout.repo, &checkout.home),
        Channel::Tty,
    ));
    let look = Markdown.look();

    let (opened, asked) = State::opened(now());
    let (state, _) = drive(opened, asked, &performer, &[]).await;
    let before = Markdown.screen(&state.screen(&look));
    for run in &runs {
        assert!(before.contains(handle(run)), "{before}");
    }

    let state = picked_and_asked(state, &performer, &runs).await;
    let state = closed(state, &performer).await;
    let after = Markdown.screen(&state.screen(&look));
    for run in &runs {
        assert!(
            !after.contains(handle(run)),
            "a closed run leaves a glance: {after}"
        );
    }

    let (state, _) = drive(state, Vec::new(), &performer, &[Stroke::Insert('d')]).await;
    let full = Markdown.screen(&state.screen(&look));
    let closed = full
        .split("## closed\n")
        .nth(1)
        .unwrap_or_else(|| panic!("the field read in full lists its closed runs: {full}"));
    for run in &runs {
        assert!(closed.contains(handle(run)), "{full}");
    }
}

/// `state`, on the field of `runs`, with both picked and `x` pressed, driven by `performer`:
/// one confirmation stands, asking to close both, each by its handle.
async fn picked_and_asked(state: State, performer: &Arc<Performer>, runs: &[String]) -> State {
    let picked = [
        Stroke::Insert(' '),
        Stroke::Insert('j'),
        Stroke::Insert(' '),
        Stroke::Insert('x'),
    ];
    let (state, _) = drive(state, Vec::new(), performer, &picked).await;
    let asks = Confirming::Close {
        runs: runs.iter().map(|run| handle(run).to_string()).collect(),
        left: Vec::new(),
    };
    let overlay = state.screen(&Markdown.look()).overlay;
    let title = overlay.map(|overlay| overlay.title);
    assert_eq!(title, Some(asks.question()), "one confirmation names both");
    state
}

/// `state`, standing at the confirmation of a close, confirmed: the close carried out by
/// `performer`, its answer asking no reading of the field, then the state driven until it
/// stands caught up with the log.
async fn closed(mut state: State, performer: &Arc<Performer>) -> State {
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let [Request::Perform(closing, Operation::Close { .. })] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    let closing = *closing;
    let (inputs, mut received) = mpsc::unbounded_channel();
    for request in asked {
        performer.take(request, &inputs);
    }
    let answer = loop {
        match received
            .recv()
            .await
            .expect("the performer answers the close")
        {
            Input::Outcome(ticket, outcome) if ticket == closing => {
                break Input::Outcome(ticket, outcome);
            }
            _ => continue,
        }
    };
    let asked = state.update(answer);
    assert!(asked.is_empty(), "no reading of the field: {asked:?}");
    drive(state, asked, performer, &[]).await.0
}
