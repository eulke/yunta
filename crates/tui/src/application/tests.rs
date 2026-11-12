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
/// would hold the loop on; the application goes on until it is left. The test sends what
/// `fg` sends while it waits, which a shell's job control needs and a group no shell
/// controls does not.
#[test]
fn after_ctrl_z_it_is_held_and_drawn_again_and_goes_on() {
    let mut terminal = running(80, 24);
    terminal.wait_on_screen(SIGNPOST, "the application never opened");
    terminal.keys("\u{1a}");
    yunta_testkit::wait_until(
        || {
            terminal.go_on();
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

/// A drawing is written whole between the marks of one synchronized update, and one that
/// changes no cell writes nothing at all: a screen at rest costs the terminal nothing, not
/// even the pen's reset a drawing ends with.
#[test]
fn a_drawing_that_changes_no_cell_writes_nothing() {
    let root = tempfile::tempdir().expect("a directory for the run");
    let performer = Arc::new(Performer::new(workspace(root.path(), root.path())));
    let look = Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(78),
        motion: Motion::Still,
    };
    let application = Application::new(performer, look, Arc::new(FixedClock), recorded);
    let drawing = Synchronized::default();
    let options = TerminalOptions {
        viewport: Viewport::Fixed(Rect::new(0, 0, 80, 24)),
    };
    let mut terminal = Drawing::with_options(CrosstermBackend::new(drawing.clone()), options)
        .expect("a terminal of fixed size");
    let (inputs, _answered) = tokio::sync::mpsc::unbounded_channel();
    let mut running = super::Running {
        application: &application,
        inputs,
        drawing,
        laid: None,
        on_screen: None,
    };
    let (mut state, _) = State::opened(fixed_now());
    let mut first = Vec::new();
    let _: Option<super::Exit<()>> = running
        .draw(&mut terminal, &mut state, &mut first)
        .expect("a drawing");
    let first = String::from_utf8_lossy(&first).to_string();
    assert!(
        first.starts_with(SYNC_BEGIN) && first.ends_with(SYNC_END),
        "{first:?}"
    );
    assert!(first.contains("yunta"), "{first:?}");
    let mut again = Vec::new();
    let _: Option<super::Exit<()>> = running
        .draw(&mut terminal, &mut state, &mut again)
        .expect("a drawing");
    assert!(again.is_empty(), "{:?}", String::from_utf8_lossy(&again));
}

/// The pages a test's surface was handed to show, in the order it was handed them.
static SHOWN: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());

/// A surface's way to show a page, which records it.
fn recorded(page: &std::path::Path) -> std::io::Result<()> {
    SHOWN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(page.to_path_buf());
    Ok(())
}

/// What the state asks is carried out in the order it asks it: leaving, asked last after
/// what has to be done first, such as letting go of the work under way, comes last.
#[test]
fn what_the_state_asks_is_carried_out_in_order_leaving_last() {
    let root = tempfile::tempdir().expect("a directory for the run");
    let performer = Arc::new(Performer::new(workspace(root.path(), root.path())));
    let look = Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(78),
        motion: Motion::Still,
    };
    let application = Application::new(performer, look, Arc::new(FixedClock), recorded);
    let (inputs, _answered) = tokio::sync::mpsc::unbounded_channel();
    let running = super::Running {
        application: &application,
        inputs,
        drawing: Synchronized::default(),
        laid: None,
        on_screen: None,
    };
    let (mut state, _) = State::opened(fixed_now());
    let (first, second) = (
        root.path().join("first.html"),
        root.path().join("second.html"),
    );
    let asked = vec![
        yunta_ui::Request::ShowPage(first.clone()),
        yunta_ui::Request::ShowPage(second.clone()),
        yunta_ui::Request::Leave,
    ];
    let exit: Option<super::Exit<()>> = running.carried_out(&mut state, asked, false);
    assert_eq!(exit, Some(super::Exit::Left));
    let shown = SHOWN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    assert_eq!(
        shown,
        [first, second],
        "each page shown, in order, before leaving"
    );
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
