//! Calling the person from the application: a run on the field that comes to need them
//! while the application stands gives the window a title naming it, then rings the
//! terminal's bell where the project asks for one, and none where it does not; leaving
//! gives the window its own title back. Each is read in the order the terminal was told.

use std::path::Path;

use surco::paint::{title_saved, BELL, SYNC_END, TITLE_RESTORED};
use yunta_testkit::{handle, run_ids, stderr, Checkout, Hold};

use super::{given_back, opened};

/// The word the run's first node waits for before it goes on to its gate.
const PARK: &str = "park";

/// A project that asks for no bell.
const SILENT: &str = "defaults:\n  isolation: none\n";

/// A project that asks for the bell when a run needs the person.
const RINGING: &str = "defaults:\n  isolation: none\nterminal:\n  bell: true\n";

/// The binary these tests run.
fn yunta() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_yunta"))
}

/// The workflow `called`: its first node holds until `hold` gives the word [`PARK`], or
/// until its marks are gone, then its gate asks the person.
fn held_then_gated(hold: &Hold) -> String {
    format!(
        "name: called\nnodes:\n  - id: held\n    kind: bash\n    run: \"until [ -f {park} ] || [ ! -e {marks} ]; do sleep 0.05; done\"\n  - {{ id: approve, kind: gate, assignee: lead, message: \"Go on?\", options: [go], depends_on: [held] }}\n",
        park = hold.word(PARK).display(),
        marks = hold.marks().display(),
    )
}

/// Bare `yunta` on the field of a repository set up with `config`, whose one run, started
/// detached and held by a word, parks on its gate once let go while the field stands;
/// then left once the window names the run. What it drew, and the title that names the run.
fn called(config: &str) -> (String, String) {
    let hold = Hold::new();
    let checkout = Checkout::new()
        .config(config)
        .workflow("wf", &held_then_gated(&hold))
        .committed();
    let detached = checkout.run(yunta(), &["run", "wf.yaml", "--detach"]);
    assert!(detached.status.success(), "{}", stderr(&detached));
    let id = run_ids(&checkout.home).remove(0);
    let title = title_saved(&format!("◆ {} needs you · called (default)", handle(&id)));
    let mut terminal = opened(&checkout);
    // The field read whole holds the run before it is let go: a run that already needs the
    // person as the field is first read calls nobody.
    terminal.wait_on_screen(handle(&id), "the field never held the run");
    let before = terminal.drawn();
    assert!(
        !before.contains(&title) && !before.contains(BELL),
        "nothing is told the window while the run moves\n{before}"
    );
    hold.give(PARK);
    terminal.wait_for(
        &title,
        "the window never named the run that needs the person",
    );
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
    (terminal.drawn(), title)
}

/// The window, given `title` once while the application stood, is given its own title back
/// once, as the application leaves: after its last drawing.
fn given_its_title_back(drawn: &str, title: &str) {
    assert_eq!(drawn.matches(title).count(), 1, "titled once\n{drawn}");
    assert_eq!(
        drawn.matches(TITLE_RESTORED).count(),
        1,
        "its own title given back once\n{drawn}"
    );
    let titled = drawn.find(title).expect("the window titled");
    let restored = drawn.find(TITLE_RESTORED).expect("its title given back");
    let last_drawn = drawn.rfind(SYNC_END).expect("a drawing");
    assert!(
        restored > titled && restored > last_drawn,
        "the window's own title given back as the application leaves\n{drawn}"
    );
}

#[test]
fn a_run_that_comes_to_need_the_person_names_the_window_then_rings_the_bell_asked_for() {
    let (drawn, title) = called(RINGING);
    let titled = drawn.find(&title).expect("the window titled");
    let rung = drawn.find(BELL).expect("the bell rung");
    assert!(
        rung > titled,
        "the bell rings once the window names the run\n{drawn}"
    );
    assert_eq!(drawn.matches(BELL).count(), 1, "rung once\n{drawn}");
    given_its_title_back(&drawn, &title);
}

#[test]
fn a_project_that_asks_for_no_bell_has_the_window_named_and_no_bell() {
    let (drawn, title) = called(SILENT);
    assert!(
        !drawn.contains(BELL),
        "no bell unless it is asked for\n{drawn}"
    );
    given_its_title_back(&drawn, &title);
}
