//! The loop on a real terminal: the test runs its own binary again in a pseudo-terminal,
//! where the application runs outside any repository, and reads what it draws and how it
//! ends. And, apart from a terminal, a drawing that changes no cell writes nothing.

use std::sync::Arc;

use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::{Terminal as Drawing, TerminalOptions, Viewport};
use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::{Paint, SYNC_BEGIN, SYNC_END};
use tokio::signal::unix::{signal, SignalKind};
use yunta_testkit::Terminal;
use yunta_testkit_core::{fixed_now, FixedClock};
use yunta_testkit_ui::sandbox::workspace;
use yunta_ui::{Performer, State};

use super::{Application, Synchronized};

/// What the signpost, where the application opens outside a repository, says.
const SIGNPOST: &str = "not in a repository";

/// A word of the signpost, which a drawing writes whole: it moves the cursor over the
/// spaces between words rather than write them.
const SIGNPOST_WORD: &str = "repository";

/// The application run again in a pseudo-terminal, at `columns` by `rows`.
fn running(columns: u16, rows: u16) -> Terminal {
    let binary = std::env::current_exe().expect("the test's own binary");
    let root = tempfile::tempdir().expect("a directory for the run");
    let args = [
        "--exact",
        "application::tests::the_application_run_again",
        "--nocapture",
    ];
    Terminal::open_sized(
        &binary,
        root.path(),
        &root.path().join("home"),
        &args,
        &[],
        (columns, rows),
    )
}

/// How the application run again ended, as it says once the terminal is given back.
fn ended(terminal: &mut Terminal) -> String {
    let drawn = terminal.ended();
    let screen = terminal.screen();
    assert!(!screen.alternate, "the main screen is back\n{drawn}");
    assert!(!screen.cursor_hidden, "the cursor is shown\n{drawn}");
    assert!(
        terminal.line_discipline_is_back(),
        "raw mode is left\n{drawn}"
    );
    drawn
}

#[test]
fn it_opens_drawn_whole_in_one_update_and_q_leaves() {
    let mut terminal = running(80, 24);
    terminal.wait_on_screen(SIGNPOST, "the application never opened");
    let drawn = terminal.drawn();
    let said = drawn.find(SIGNPOST_WORD).expect("the signpost drawn");
    let (before, after) = drawn.split_at(said);
    assert!(
        before.rfind(SYNC_BEGIN) > before.rfind(SYNC_END),
        "the signpost is drawn inside an update begun\n{drawn}"
    );
    assert!(after.contains(SYNC_END), "and ended\n{drawn}");
    terminal.keys("q");
    assert!(ended(&mut terminal).contains("ended: Left"));
}

#[test]
fn ctrl_c_ends_it_interrupted() {
    let mut terminal = running(80, 24);
    terminal.wait_on_screen(SIGNPOST, "the application never opened");
    terminal.keys("\u{3}");
    assert!(ended(&mut terminal).contains("ended: Interrupted"));
}

#[test]
fn what_asks_it_to_end_ends_it() {
    let mut terminal = running(80, 24);
    terminal.wait_on_screen(SIGNPOST, "the application never opened");
    terminal.terminate();
    assert!(ended(&mut terminal).contains("ended: AskedToEnd(())"));
}

#[test]
fn a_resize_lays_the_screen_out_at_the_new_size() {
    let mut terminal = running(80, 24);
    terminal.wait_on_screen(SIGNPOST, "the application never opened");
    terminal.resized(100, 30);
    yunta_testkit::wait_until(
        || {
            let screen = terminal.screen();
            screen.rows.len() == 30 && screen.rows[29].contains("leaves")
        },
        || format!("never laid out at 100 by 30\n{:#?}", terminal.screen().rows),
    );
    terminal.keys("q");
    ended(&mut terminal);
}

/// Ctrl-Z gives the terminal back and holds it again, the screen drawn whole on the other
/// screen without asking the terminal anything, which a terminal that answers nothing
/// would hold the loop on; the application goes on until it is left. Under the harness
/// the process is in a group no shell controls, which the kernel stops on no `SIGTSTP`,
/// so it goes on at once.
#[test]
fn after_ctrl_z_it_is_held_and_drawn_again_and_goes_on() {
    let mut terminal = running(80, 24);
    terminal.wait_on_screen(SIGNPOST, "the application never opened");
    terminal.keys("\u{1a}");
    yunta_testkit::wait_until(
        || {
            let held = terminal.drawn_on_the_other_screen();
            held.len() == 2 && held[1].contains(SIGNPOST_WORD) && terminal.screen().alternate
        },
        || {
            format!(
                "the signpost was never drawn again on the other screen\n{}",
                terminal.drawn()
            )
        },
    );
    terminal.keys("q");
    let drawn = ended(&mut terminal);
    assert!(drawn.contains("ended: Left"), "{drawn}");
}

/// A drawing that changes no cell writes no cell: what it writes is the backend's own
/// resetting of the pen; one that changes cells is written whole between the marks of a
/// synchronized update.
#[test]
fn a_drawing_that_changes_nothing_writes_no_cell() {
    let drawing = Synchronized::default();
    let options = TerminalOptions {
        viewport: Viewport::Fixed(Rect::new(0, 0, 20, 2)),
    };
    let mut terminal = Drawing::with_options(CrosstermBackend::new(drawing.clone()), options)
        .expect("a terminal of fixed size");
    let mut written = Vec::new();
    let draw = |terminal: &mut Drawing<CrosstermBackend<Synchronized>>| {
        terminal
            .draw(|frame| frame.render_widget("the same words", frame.area()))
            .expect("a drawing");
    };
    draw(&mut terminal);
    drawing.written(&mut written).expect("written");
    let first = String::from_utf8_lossy(&written).to_string();
    assert!(
        first.starts_with(SYNC_BEGIN) && first.ends_with(SYNC_END),
        "{first:?}"
    );
    assert!(first.contains("same"), "{first:?}");
    written.clear();
    draw(&mut terminal);
    drawing.written(&mut written).expect("written");
    let again = String::from_utf8_lossy(&written).to_string();
    let cells: String = unescaped(&again);
    assert!(cells.is_empty(), "{again:?}");
}

/// `written` without its escape sequences: the characters it puts in cells.
fn unescaped(written: &str) -> String {
    let mut cells = String::new();
    let mut characters = written.chars();
    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            cells.push(character);
            continue;
        }
        // An escape runs to the first letter after its bracket and parameters.
        for inside in characters.by_ref() {
            if inside.is_ascii_alphabetic() {
                break;
            }
        }
    }
    cells
}

/// What the tests above run in a pseudo-terminal: the application outside any repository,
/// at a clock that stands still, ended by a terminate where the test sends one, and how it
/// ended said once the terminal is given back. Run with the rest, it runs nothing.
#[test]
fn the_application_run_again() {
    let arguments: Vec<String> = std::env::args().collect();
    let named = arguments
        .iter()
        .any(|argument| argument == "application::tests::the_application_run_again");
    if !(named && arguments.iter().any(|argument| argument == "--exact")) {
        return;
    }
    let root = tempfile::tempdir().expect("a directory outside any repository");
    let outside = root.path().join("outside");
    std::fs::create_dir_all(&outside).expect("the directory");
    let performer = Arc::new(Performer::new(workspace(&outside, root.path())));
    let look = Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(78),
        motion: Motion::Still,
    };
    let application = Application::new(performer, look, Arc::new(FixedClock), |_| Ok(()));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let ended = runtime.block_on(async {
        let mut terminate = signal(SignalKind::terminate()).expect("a terminate to listen for");
        let asked_to_end = async move {
            terminate.recv().await;
        };
        application
            .run(State::opened(fixed_now()), asked_to_end)
            .await
    });
    println!("ended: {:?}", ended.expect("a terminal held"));
}
