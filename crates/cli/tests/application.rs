//! The application at a terminal, opened by this build's `yunta`: each slice of what a
//! person does in it in a file of its own beside this one, driven at a pseudo-terminal.

#[path = "application/opening.rs"]
mod opening;
#[path = "application/watching.rs"]
mod watching;

use yunta_testkit::{yunta_on_terminal, Checkout, Terminal};

/// What the field says of a repository with runs.
const THE_FIELD: &str = "runs of this repository";

/// Bare `yunta` at a terminal of the harness's size, in `checkout`'s repository, open on the
/// field.
fn opened(checkout: &Checkout) -> Terminal {
    let terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &[]);
    terminal.wait_on_screen(THE_FIELD, "bare yunta never opened on the field");
    assert!(
        terminal.screen().alternate,
        "the application holds the other screen"
    );
    terminal
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
