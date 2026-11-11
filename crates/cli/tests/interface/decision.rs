//! A decision driven end to end: a run this build parked on a gate, put on the card
//! `resolve-gate` holds at a terminal, decided at it in the application, and handed on
//! to a detached resume this build carries to its end.

use super::*;

use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::paint::Paint;
use yunta_testkit::{
    full_run_id, run_id_from, runs_root, wait_until, yunta_at, yunta_on_terminal, Checkout,
};

/// A gate, then a node once it is passed.
const GATED: &str = "name: gated\nnodes:\n  - { id: approve, kind: gate, assignee: lead, \
                     message: \"Go on?\", options: [go] }\n  \
                     - { id: after, kind: bash, run: \"true\", depends_on: [approve] }\n";

/// A run parked on its gate, answered at its card with its first option, is recorded
/// and resumed detached, to its end.
#[tokio::test]
async fn a_parked_gate_decided_is_resumed_detached_to_its_end() {
    let checkout = Checkout::new()
        .working_in_place()
        .workflow("wf", GATED)
        .committed();
    let parked = yunta_at!(&checkout, &["run", "wf.yaml"]);
    let id = full_run_id(&checkout.home, &run_id_from(&parked));
    let performer = Arc::new(Performer::new(workspace(&checkout.repo, &checkout.home)));
    let (opened, asked) = State::opened(now());

    let answered = [Stroke::Enter, Stroke::Insert('a'), Stroke::Enter];
    let (state, _) = drive(opened, asked, &performer, &answered).await;
    let screen = Markdown.screen(&state.screen(&Markdown.look()));

    let log = || {
        std::fs::read_to_string(runs_root(&checkout.home).join(&id).join("events.jsonl"))
            .unwrap_or_default()
    };
    wait_until(
        || log().contains("\"run_finished\""),
        || format!("the run never finished:\n{}\non screen:\n{screen}", log()),
    );
    assert_eq!(log().matches("\"gate_resolved\"").count(), 1, "{}", log());
    let status = yunta_at!(&checkout, &["status", &id, "--json"]);
    let document: serde_json::Value =
        serde_json::from_str(&yunta_testkit::stdout(&status)).expect("one document");
    assert_eq!(document["outcome"], "finished", "{document:#}");
}

/// The card the application puts a parked gate on is the one `resolve-gate` holds at a
/// terminal of the same width: every line of it, its question, options, words and keys.
#[tokio::test]
async fn the_card_is_the_one_resolve_gate_holds_at_a_terminal() {
    let checkout = Checkout::new()
        .working_in_place()
        .workflow("wf", GATED)
        .committed();
    let parked = yunta_at!(&checkout, &["run", "wf.yaml"]);
    let id = full_run_id(&checkout.home, &run_id_from(&parked));
    let mut terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &["resolve-gate", &id]);
    terminal.wait_on_screen("▸ go", "resolve-gate never put the card");
    let held = terminal.screen();
    let performer = Arc::new(Performer::new(workspace(&checkout.repo, &checkout.home)));
    let (opened, asked) = State::opened(now());

    let (state, _) = drive(
        opened,
        asked,
        &performer,
        &[Stroke::Enter, Stroke::Insert('a')],
    )
    .await;

    let look = Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::row(usize::from(yunta_testkit::Terminal::COLUMNS), None),
        motion: surco::motion::Motion::Moves,
    };
    let screen = state.screen(&look);
    let [card] = screen.main.doc.blocks() else {
        panic!("the card alone in the main region");
    };
    let drawn = surco::surface::Terminal::on(look).block(card);
    for line in drawn.iter().map(|line| Paint::Plain.line(line)) {
        let line = line.trim_end();
        assert!(
            held.rows.iter().any(|row| row.trim_end() == line),
            "`{line}` is not on resolve-gate's card:\n{}",
            held.rows.join("\n")
        );
    }
    terminal.keys("\u{1b}");
    terminal.ended();
}
