//! What a run leaves, from its stage in the application: its numbers and its graph, its
//! receipt, and its page for a browser, shown through the system's opener, or said with
//! where it is when nothing here can open it. The application runs on a `PATH` that holds
//! git and, where a page is shown, a stand-in for the opener that records what it was
//! handed: no test opens a browser.

use yunta_testkit::stubs::path_opening_no_browser;
use yunta_testkit::workflows::FINISHES;
use yunta_testkit::{stderr, yunta_at, Checkout, Terminal};

use super::{given_back, opened_with, FOLLOWED};

/// A repository with one run that finished, and that run's whole id.
fn with_a_finished_run() -> (Checkout, String) {
    let checkout = Checkout::new()
        .working_in_place()
        .workflow("done", FINISHES)
        .committed();
    let ran = yunta_at!(&checkout, &["run", "done.yaml", "--quiet"]);
    assert!(ran.status.success(), "{}", stderr(&ran));
    let id = yunta_testkit::run_ids(&checkout.home).remove(0);
    (checkout, id)
}

/// Bare `yunta` with `environment` set over the harness's, on the field, its closed run
/// listed, opened and followed.
fn its_closed_run_followed(checkout: &Checkout, environment: &[(&str, &str)]) -> Terminal {
    let mut terminal = opened_with(checkout, environment);
    terminal.keys("d");
    terminal.wait_on_screen("done (default)", "the closed run was never listed");
    terminal.keys("\r");
    terminal.wait_on_screen(FOLLOWED, "the run was never followed");
    terminal
}

#[test]
fn a_runs_numbers_graph_and_receipt_open_from_its_stage() {
    let (checkout, _) = with_a_finished_run();
    let mut terminal = its_closed_run_followed(&checkout, &[]);
    terminal.keys("s");
    terminal.wait_on_screen("› stats", "its numbers never opened");
    terminal.keys("\u{1b}");
    terminal.wait_on_screen(FOLLOWED, "esc never came back to the run");
    terminal.keys("g");
    terminal.wait_on_screen("› graph", "its graph never opened");
    terminal.keys("\u{1b}");
    terminal.wait_on_screen(FOLLOWED, "esc never came back to the run");
    terminal.keys("e");
    terminal.wait_on_screen("receipt of", "its receipt never opened");
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

/// The page shown through the opener is the page `yunta open` names.
#[test]
fn a_runs_page_is_shown_through_the_opener_as_open_names_it() {
    let (checkout, id) = with_a_finished_run();
    let marks = tempfile::tempdir().unwrap();
    let recorded = marks.path().join("opened.txt");
    let path = path_opening_no_browser(marks.path(), Some(&recorded));
    let mut terminal = its_closed_run_followed(&checkout, &[("PATH", &path)]);
    terminal.keys("o");
    terminal.wait_on_screen("page.html", "the page was never written");
    let read = || std::fs::read_to_string(&recorded).unwrap_or_default();
    yunta_testkit::wait_until(
        || !read().is_empty(),
        || "the opener was never handed the page".to_string(),
    );
    let shown = read();
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
    // Off a terminal `yunta open` says where the page is, which a person at one gets opened.
    let open = yunta_at!(&checkout, &["open", &id, "--json"]);
    assert!(open.status.success(), "{}", stderr(&open));
    let named: serde_json::Value = serde_json::from_str(&yunta_testkit::stdout(&open)).unwrap();
    assert_eq!(
        shown.trim_end(),
        named["page"].as_str().unwrap(),
        "the page `yunta open` names"
    );
}

/// Where nothing here can open a page, the screen says so, and where the page is.
#[test]
fn a_page_nothing_here_can_open_is_said_with_where_it_is() {
    let (checkout, _) = with_a_finished_run();
    let marks = tempfile::tempdir().unwrap();
    let path = path_opening_no_browser(marks.path(), None);
    let mut terminal = its_closed_run_followed(&checkout, &[("PATH", &path)]);
    terminal.keys("o");
    terminal.wait_on_screen(
        "nothing here could open it",
        "the page not shown was never said",
    );
    assert!(
        terminal.screen().holds("page.html"),
        "{:#?}",
        terminal.screen().rows
    );
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}
