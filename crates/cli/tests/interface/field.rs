//! The field driven end to end: runs this build started, picked and closed after one
//! confirmation, and the field read again.

use super::*;

use yunta_testkit::{full_run_id, handle, run_id_from, yunta_at, Checkout};

/// A build that fails at once: its run stops on a person.
const FAILS: &str = "name: fails\nnodes:\n  - { id: build, kind: bash, run: \"exit 101\" }\n";

/// Two runs stopped on a person, picked and closed after one confirmation, leave the
/// runs the field lists at a glance and stand among its closed runs when it is read in
/// full.
#[tokio::test]
async fn two_picked_runs_are_closed_and_the_field_read_again() {
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
    let performer = Arc::new(Performer::new(workspace(&checkout.repo, &checkout.home)));
    let look = Markdown.look();

    let (opened, asked) = State::opened(now());
    let (state, _) = drive(opened, asked, &performer, &[]).await;
    let before = Markdown.screen(&state.screen(&look));
    for run in &runs {
        assert!(before.contains(handle(run)), "{before}");
    }

    let picked_and_closed = [
        Stroke::Insert(' '),
        Stroke::Insert('j'),
        Stroke::Insert(' '),
        Stroke::Insert('x'),
        Stroke::Enter,
    ];
    let (state, _) = drive(state, Vec::new(), &performer, &picked_and_closed).await;
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
        .split("closed, newest first")
        .nth(1)
        .unwrap_or_else(|| panic!("the field read in full lists its closed runs: {full}"));
    for run in &runs {
        assert!(closed.contains(handle(run)), "{full}");
    }
}
