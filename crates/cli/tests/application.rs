//! The application at a terminal, opened by this build's `yunta`: each slice of what a
//! person does in it in a file of its own beside this one, driven at a pseudo-terminal.

#[path = "application/deciding.rs"]
mod deciding;
#[path = "application/machine.rs"]
mod machine;
#[path = "application/opening.rs"]
mod opening;
#[path = "application/repository.rs"]
mod repository;
#[path = "application/review.rs"]
mod review;
#[path = "application/runs_left.rs"]
mod runs_left;
#[path = "application/starting.rs"]
mod starting;
#[path = "application/watching.rs"]
mod watching;

use serde_json::Value;
use yunta_testkit::{
    full_run_id, run_id_from, runs_root, stderr, stdout, yunta_at, yunta_on_terminal, Checkout,
    Terminal,
};

/// What the field says of a repository with runs.
const THE_FIELD: &str = "runs of this repository";

/// Bare `yunta` at a terminal of the harness's size, in `checkout`'s repository, open on the
/// field.
fn opened(checkout: &Checkout) -> Terminal {
    opened_with(checkout, &[])
}

/// Bare `yunta` as [`opened`] opens it, with `environment` set over the harness's.
fn opened_with(checkout: &Checkout, environment: &[(&str, &str)]) -> Terminal {
    let terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &[], environment);
    terminal.wait_on_screen(THE_FIELD, "bare yunta never opened on the field");
    assert!(
        terminal.screen().alternate,
        "the application holds the other screen"
    );
    terminal
}

/// The palette opened over where `terminal` stands, and its row `named` taken: the whole
/// name typed, which no other row answers as well.
fn taken_from_the_palette(terminal: &mut Terminal, named: &str) {
    terminal.keys("/");
    terminal.wait_on_screen("what you can do here", "the palette never opened");
    terminal.keys(named);
    terminal.wait_on_screen(
        &format!("▸ {named}"),
        "the palette never pointed at the row",
    );
    terminal.keys("\r");
}

/// `terminal` given back as the binary found it, once it ended, and the code it exited
/// with.
fn given_back(terminal: &mut Terminal) -> Option<i32> {
    let drawn = terminal.ended();
    let screen = terminal.screen();
    assert!(!screen.alternate, "the main screen is back\n{drawn}");
    assert!(!screen.cursor_hidden, "the cursor is shown\n{drawn}");
    assert!(
        !screen.bracketed_paste,
        "pastes are no longer marked\n{drawn}"
    );
    assert!(!screen.mouse_captured, "the mouse was never taken\n{drawn}");
    assert!(
        terminal.line_discipline_is_back(),
        "raw mode is left\n{drawn}"
    );
    terminal.exit_code()
}

/// What the run's legend says once its nodes stand beside its page, the pointer on one:
/// what only the run's following brings, since a run not yet read has no nodes to point at.
const FOLLOWED: &str = "move along its nodes";

/// Bare `yunta` in `checkout`'s repository, its first run opened from the field and
/// followed.
fn following(checkout: &Checkout) -> Terminal {
    let mut terminal = opened(checkout);
    terminal.keys("\r");
    terminal.wait_on_screen(FOLLOWED, "the run opened from the field was never followed");
    terminal
}

/// How `yunta status` says the run `id` stands, as its document names it.
fn outcome(checkout: &Checkout, id: &str) -> String {
    let status = yunta_at!(checkout, &["status", id, "--json"]);
    let document: Value = serde_json::from_str(&stdout(&status)).expect("one document");
    document["outcome"].as_str().unwrap_or_default().to_string()
}

/// The run `checkout` starts with `arguments`, off a terminal, once it parked on what it
/// asks a person: its whole id.
fn parked(checkout: &Checkout, arguments: &[&str]) -> String {
    let parked = yunta_at!(checkout, arguments);
    assert_eq!(
        parked.status.code(),
        Some(3),
        "it parks: {}",
        stderr(&parked)
    );
    full_run_id(&checkout.home, &run_id_from(&parked))
}

/// The run `id`'s log, as it stands.
fn log(checkout: &Checkout, id: &str) -> String {
    std::fs::read_to_string(runs_root(&checkout.home).join(id).join("events.jsonl"))
        .unwrap_or_default()
}

/// What the run's page says at its head once the run finished: in sight however long its
/// chronicle grows below.
const FINISHED: &str = "(default) · finished";

/// The process the engine of the run `id` runs in, as the run's registry names it while the
/// engine lives.
fn engine_of(checkout: &Checkout, id: &str) -> u64 {
    let registry = runs_root(&checkout.home)
        .join(id)
        .join("scratch")
        .join("engine.json");
    let file: Value = serde_json::from_str(&std::fs::read_to_string(registry).unwrap()).unwrap();
    file["engine_pid"].as_u64().expect("the engine's process")
}
