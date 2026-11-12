//! What stands over a stage: every act that stops, closes, removes or spends asks to be
//! confirmed first, saying what it does and the command that does the same, and Escape
//! leaves it undone; help lists the keys of where the person is and those every stage
//! answers to, and Escape closes it.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use surco::surface::{Markdown, Surface};
use yunta_core::step::{Arg, NextStep};
use yunta_core::{PackRef, RunId};
use yunta_render::command;
use yunta_render::page::Collected;
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::{pack_with_an_executor, with_a_pack_installed, workspace};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Outcome, Performer, Request, State};

use support::{on_the_field, stopped};

/// The run the field lists.
fn the_run() -> RunId {
    "01M7DIALOGS000000000000000".parse().expect("a run's id")
}

/// What the confirmation standing over `state` says, as Markdown writes it.
fn confirmation(state: &State) -> String {
    let overlay = state.screen(&Markdown.look()).overlay;
    let overlay = overlay
        .filter(|overlay| overlay.title == yunta_render::screen::CONFIRM)
        .unwrap_or_else(|| panic!("a confirmation stands"));
    Markdown.draw(&overlay.doc)
}

/// `state`, with a confirmation standing, says the command `same` that does the same,
/// and Escape leaves it with nothing asked.
fn confirmed_first(mut state: State, same: &NextStep) {
    let says = confirmation(&state);
    assert!(says.contains(&command::line(same)), "{says}");
    let asked = state.update(Input::Stroke(Stroke::Decline));
    assert!(asked.is_empty(), "Escape asks nothing: {asked:?}");
    assert!(state.screen(&Markdown.look()).overlay.is_none());
}

/// `key` pressed on `state` asks nothing yet, and stands a confirmation that names `same`.
fn asks_first(mut state: State, key: char, same: &NextStep) {
    let asked = state.update(Input::Stroke(Stroke::Insert(key)));
    assert!(
        asked.is_empty(),
        "{key} asks nothing before it is confirmed: {asked:?}"
    );
    confirmed_first(state, same);
}

#[test]
fn cancelling_and_closing_a_run_ask_first() {
    let run = the_run();
    let field = on_the_field(vec![stopped(&run)]);
    asks_first(field.clone(), 'c', &NextStep::Cancel { run: run.clone() });
    asks_first(field, 'x', &NextStep::Close { run });
}

#[test]
fn doctors_sessions_and_collecting_ask_first() {
    let mut state = on_the_field(vec![stopped(&the_run())]);
    let asked = state.update(Input::Stroke(Stroke::Insert('m')));
    let [Request::Perform(checks, _)] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    state.update(Input::Outcome(*checks, Outcome::Doctor(Vec::new())));
    let sessions = NextStep::Doctor {
        sessions: true,
        full: false,
    };
    asks_first(state.clone(), 's', &sessions);
    let asked = state.update(Input::Stroke(Stroke::Insert('u')));
    let [Request::Perform(dry, _)] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    let would = Outcome::Collected(Box::new(Collected::new(true)));
    state.update(Input::Outcome(*dry, would));
    asks_first(state, 'c', &NextStep::Gc { dry_run: false });
}

/// A repository set up for Yunta under `root`, no pack installed.
fn set_up(root: &Path) -> PathBuf {
    let repo = root.join("project");
    std::fs::create_dir_all(repo.join(".yunta")).expect("its .yunta");
    yunta_testkit::init_repo(&repo);
    let config = "defaults:\n  isolation: none\n";
    std::fs::write(repo.join(".yunta/config.yaml"), config).expect("its config");
    repo
}

#[tokio::test]
async fn removing_a_pack_and_running_its_executors_ask_first() {
    let root = tempfile::tempdir().expect("a temporary root");
    let repo = with_a_pack_installed(root.path()).await;
    let performer = Arc::new(Performer::new(workspace(&repo, root.path())));
    let field = on_the_field(vec![stopped(&the_run())]);
    let to_the_packs = [Stroke::Insert('w'), Stroke::Insert('p')];
    let (packs, _) = drive(field.clone(), Vec::new(), &performer, &to_the_packs).await;
    let demo: PackRef = "acme/demo".parse().expect("a pack's reference");
    asks_first(packs, 'x', &NextStep::PackRemove { pack: demo });
    // A pack that ships an executor, fetched and its inventory read, asks first too.
    let (upstream, _) = pack_with_an_executor(root.path());
    let fresh = set_up(&root.path().join("fresh"));
    let performer = Arc::new(Performer::new(workspace(&fresh, root.path())));
    let source = upstream.to_str().expect("a path that reads").to_string();
    let mut strokes = vec![
        Stroke::Insert('w'),
        Stroke::Insert('p'),
        Stroke::Insert('i'),
    ];
    strokes.extend(source.chars().map(Stroke::Insert));
    strokes.extend([Stroke::Enter, Stroke::Enter]);
    let (installing, _) = drive(field, Vec::new(), &performer, &strokes).await;
    let added = NextStep::PackAdd {
        source: Arg::Given(source),
    };
    confirmed_first(installing, &added);
}

#[test]
fn help_lists_the_keys_where_the_person_is_and_escape_closes_it() {
    let mut state = on_the_field(vec![stopped(&the_run())]);
    let keys = state.screen(&Markdown.look()).legend;
    let asked = state.update(Input::Stroke(Stroke::Insert('?')));
    assert!(asked.is_empty(), "{asked:?}");
    let overlay = state.screen(&Markdown.look()).overlay;
    let overlay = overlay
        .filter(|overlay| overlay.title == yunta_render::screen::HELP)
        .expect("help stands");
    let listed = Markdown.draw(&overlay.doc);
    for binding in &keys.0 {
        assert!(
            listed.contains(&binding.does),
            "{} in {listed}",
            binding.does
        );
    }
    assert!(listed.contains("leaves yunta"), "{listed}");
    let asked = state.update(Input::Stroke(Stroke::Decline));
    assert!(asked.is_empty(), "{asked:?}");
    assert!(state.screen(&Markdown.look()).overlay.is_none());
}

/// No work is asked by any of these keys before it is confirmed.
#[test]
fn a_stroke_never_asks_what_stops_closes_removes_or_spends() {
    let mut state = on_the_field(vec![stopped(&the_run())]);
    for key in ['c', 'x'] {
        let asked = state.update(Input::Stroke(Stroke::Insert(key)));
        let unconfirmed = asked.iter().any(|request| {
            matches!(request, Request::Perform(_, operation) if operation.needs_confirming())
        });
        assert!(!unconfirmed, "{asked:?}");
        state.update(Input::Stroke(Stroke::Decline));
    }
    let confirmed = state.update(Input::Stroke(Stroke::Insert('c')));
    assert!(confirmed.is_empty());
    let asked = state.update(Input::Stroke(Stroke::Enter));
    assert!(
        asked.iter().any(|request| matches!(
            request,
            Request::Perform(_, operation @ Operation::Cancel { .. }) if operation.needs_confirming()
        )),
        "{asked:?}"
    );
}

#[test]
fn help_taller_than_its_box_scrolls() {
    let mut state = on_the_field(vec![stopped(&the_run())]);
    state.update(Input::Stroke(Stroke::Insert('?')));
    let look = surco::look::Look::plain();
    let laid_out = |state: &State| {
        let overlay = state.screen(&look).overlay.expect("help stands");
        let terminal = surco::surface::Terminal::on(look);
        let (_, laid) = terminal.scrolled(&overlay.doc, overlay.sight, 4);
        surco::components::ScreenLaid {
            overlay: Some(laid),
            ..Default::default()
        }
    };
    let laid = laid_out(&state);
    assert!(laid.overlay.is_some_and(|laid| laid.scrolls()), "{laid:?}");
    state.update(Input::Laid(laid));
    state.update(Input::Stroke(Stroke::PageDown));
    let overlay = state.screen(&look).overlay.expect("help stands");
    assert_eq!(
        overlay.sight.top, 3,
        "a page down, less the row that counts the rest"
    );
}
