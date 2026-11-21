//! The application at a terminal, opened by this build's `yunta`: each slice of what a
//! person does in it in a file of its own beside this one, driven at a pseudo-terminal.

#[path = "application/calling.rs"]
mod calling;
#[path = "application/deciding.rs"]
mod deciding;
#[path = "application/feed.rs"]
mod feed;
#[path = "application/field.rs"]
mod field;
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
use yunta_core::step::NextStep;
use yunta_render::screen::Leaving;
use yunta_testkit::{
    full_run_id, run_id_from, runs_root, stderr, stdout, wait_until, yunta_at, yunta_on_terminal,
    Checkout, Terminal,
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

/// The palette opened over where `terminal` stands, and the row of `step` taken: what it
/// does typed whole, in render's words, which no other row answers as well.
fn taken_from_the_palette(terminal: &mut Terminal, step: &NextStep) {
    let does = yunta_render::command::does(step);
    terminal.keys("/");
    terminal.wait_on_screen("what do you want to do?", "the palette never opened");
    terminal.keys(does);
    terminal.wait_on_screen(&format!("▸ {does}"), "the palette never pointed at the row");
    terminal.keys("\r");
}

/// `terminal`, on a run's screen, left with `q` for the field, which notes where the run is
/// left, as `said` says where the test knows it; then `q` there leaves the application.
fn left_through_the_field(terminal: &mut Terminal, said: Option<Leaving>) {
    terminal.keys("q");
    terminal.wait_on_screen(THE_FIELD, "`q` on the run never went to the field");
    if let Some(said) = said {
        terminal.wait_on_screen(said.said(), "the field never noted where the run is left");
    }
    terminal.keys("q");
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

/// What the run's legend says once its nodes stand beside its pane, the pointer on one:
/// what only the run's following brings, since a run not yet read has no nodes to point at.
const FOLLOWED: &str = "j k node";

/// What the run's legend says once the decision it waits on stands in place on its screen,
/// its card's keys in the legend: what only the run's following brings too, the decision
/// read.
const ITS_CARD_STANDS: &str = "j k choose";

/// What the run's legend says once the plan's desk stands in place on its screen, the desk's
/// keys in the legend: what only the run's following brings too, the decision read.
const ITS_DESK_STANDS: &str = "tab next place";

/// What the run's legend says once the round of questions it waits on stands in place on its
/// screen, the round's keys in the legend, its questions typed or picked from a list: what
/// only the run's following brings too, the round read. A run parked on a round may stand
/// as it at the first screen its following draws, its nodes never beside its pane.
const ITS_ROUND_STANDS: &str = "esc leave it for later";

/// Bare `yunta` in `checkout`'s repository, its first run opened from the field and
/// followed: its nodes stand beside its pane, or what it waits on stands in place, the
/// decision on its card or at the plan's desk, or the round of questions.
fn following(checkout: &Checkout) -> Terminal {
    let mut terminal = opened(checkout);
    terminal.keys("\r");
    wait_until(
        || {
            let screen = terminal.screen();
            [FOLLOWED, ITS_CARD_STANDS, ITS_DESK_STANDS, ITS_ROUND_STANDS]
                .iter()
                .any(|said| screen.holds(said))
        },
        || {
            format!(
                "the run opened from the field was never followed\non the screen:\n{}",
                terminal.screen().rows.join("\n")
            )
        },
    );
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

/// What the run's headline says across its screen once the run finished: in sight whatever
/// its pane shows, a page from its head or a feed at its end.
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
