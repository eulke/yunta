//! A round of questions a run parked on, answered from another terminal: the round
//! is put to the person there as the run itself asks it, and the run goes on with
//! the answers on its log. Where the engine refused the answers the round was last given,
//! each refusal stands under the question it is about.

use yunta_core::events::{EventPayload, RunEvent};
use yunta_testkit::{run_id_from, yunta_in};
use yunta_testkit_ui::cut::Cut;

use super::*;

/// Whether the run started under `root` has closed on its log.
fn closed(root: &Path) -> bool {
    let Some(run_id) = run_dir(root).and_then(|dir| dir.file_name()?.to_str()?.parse().ok()) else {
        return false;
    };
    let Ok(storage) = yunta_storage::Storage::open(&home(root).join("yunta.db")) else {
        return false;
    };
    storage.events_for_run(&run_id).is_ok_and(|events| {
        events.iter().any(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Run(RunEvent::Finished(_)))
            )
        })
    })
}

/// A run of [`ASKING`] off a terminal parks on its round; `resolve-gate` at a terminal
/// asks the round, records the answer, and the run goes on to its end on its own.
#[test]
fn a_parked_round_is_answered_with_resolve_gate_and_the_run_goes_on() {
    let root = tempfile::tempdir().unwrap();
    let checkout = Checkout::under(root.path())
        .config(CONFIG)
        .workflow("wf", ASKING)
        .file(
            "fixture.yaml",
            &HANDED_THEM_OVER.replace(
                "QUESTIONS",
                &nested(
                    "questions:\n  - id: summary\n    text: \"What changed?\"\n    answer_type: \
                     text\n    required: true\n",
                ),
            ),
        )
        .committed();
    let (repo, home) = (checkout.repo, checkout.home);
    let parked = yunta_in!(
        &repo,
        &home,
        &[
            "run",
            "wf.yaml",
            "--adapter",
            "mock",
            "--fixture",
            "fixture.yaml"
        ]
    );
    assert_eq!(parked.status.code(), Some(3), "the run parks on its round");
    let run = run_id_from(&parked);

    let mut terminal = yunta_on_terminal!(&repo, &home, &["resolve-gate", &run]);
    terminal.wait_on_screen("◆ What changed?", "the round was never asked");
    terminal.keys("the greeting names the person\r");
    let drawn = terminal.ended();
    assert!(terminal.ran_to_the_end(), "{drawn}");

    wait_until(
        || closed(root.path()),
        || "the run never went on to its end".to_string(),
    );
    let recorded = answers(root.path()).unwrap_or_default();
    assert!(
        recorded.contains("the greeting names the person"),
        "the answer is on the run:\n{recorded}"
    );
}

/// A run whose answers the engine refused, its required question left without one, asked
/// again with `resolve-gate` at a terminal: the refusal stands under the question it is
/// about, in the engine's words, while the person types there.
#[tokio::test]
async fn a_refused_round_says_each_refusal_under_its_question_at_resolve_gate() {
    let cut = Cut::of_target("round-refused").await;
    let run = cut.runs()[0].to_string();
    let mut terminal = yunta_on_terminal!(&cut.repository(), cut.root(), &["resolve-gate", &run]);
    terminal.wait_on_screen(
        "◆ What is the release called?",
        "the refused round was never asked",
    );
    terminal.keys("Spring release");
    terminal.wait_on_screen("│ Spring release", "the words never reached the line");
    let refusal = "✗ this question is `required` and nothing answers it";
    assert!(terminal.screen().holds(refusal), "{:#?}", terminal.screen());
    terminal.keys("\x1b");
    terminal.ended();
}
