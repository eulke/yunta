//! Doctor in the application, opened from the field: what every check of a sandbox
//! found, as `yunta doctor` prints it at a glance and with every check on its own row,
//! as `--full` prints it; and a session per binding opened only once the person
//! confirms each spends a prompt, what it remarks shown as it comes, stopped with
//! Escape.

mod support;

use surco::surface::{Markdown, Surface};
use yunta_api::Remark;
use yunta_render::page::doctor::{Doctor, Finding};
use yunta_render::page::Page;
use yunta_testkit::Bench;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Outcome, Request, State, Ticket};

use support::{on_the_field, performed, stopped};

/// The screen's main region, as Markdown writes it.
fn main_region(state: &State) -> String {
    let look = Markdown.look();
    Markdown.draw(&state.screen(&look).main.doc)
}

/// `page` as Markdown writes it.
fn drawn(page: &impl Page) -> String {
    let look = Markdown.look();
    Markdown.draw(&page.document(&look))
}

/// Doctor opened from a field of one stopped run: the state, and the ticket its checks
/// are asked under.
fn opened() -> (State, Ticket) {
    let run = "01M7DOCTOR0000000000000000".parse().expect("a run's id");
    let mut state = on_the_field(vec![stopped(&run)]);
    let asked = state.update(Input::Stroke(Stroke::Insert('m')));
    let [Request::Perform(ticket, Operation::Doctor { sessions: false })] = asked.as_slice() else {
        panic!("doctor checks the machine as it opens: {asked:?}");
    };
    (state, *ticket)
}

#[tokio::test]
async fn the_findings_are_those_doctor_prints_at_a_glance_and_whole() {
    let bench = Bench::new();
    let (mut state, checks) = opened();
    let (_, outcome) = performed(&bench, Operation::Doctor { sessions: false }).await;
    let Outcome::Doctor(findings) = &outcome else {
        panic!("doctor's checks come to their findings: {outcome:?}");
    };
    let printed = |full| {
        drawn(&Doctor {
            session: false,
            findings,
            full,
        })
    };
    // A session per binding is offered once the checks ended, not while they run.
    let offers_sessions = |state: &State| {
        let legend = state.screen(&Markdown.look()).legend;
        legend
            .0
            .iter()
            .any(|binding| binding.does == "a session per binding")
    };
    assert!(!offers_sessions(&state));
    state.update(Input::Outcome(checks, outcome.clone()));
    assert!(offers_sessions(&state));
    assert_eq!(main_region(&state), printed(false));
    let trail = state.screen(&Markdown.look()).trail;
    assert_eq!(trail, ["yunta", "doctor"]);
    // Every check on its own row, as `--full` prints them, then the glance again.
    assert_ne!(printed(true), printed(false), "every row says more");
    state.update(Input::Stroke(Stroke::Insert('f')));
    assert_eq!(main_region(&state), printed(true));
    state.update(Input::Stroke(Stroke::Insert('f')));
    assert_eq!(main_region(&state), printed(false));
}

#[test]
fn a_session_per_binding_is_opened_once_confirmed_and_stopped_with_escape() {
    let (mut state, checks) = opened();
    let found = vec![Finding::Committer {
        who: "Ada <ada@example.com>".to_string(),
    }];
    state.update(Input::Outcome(checks, Outcome::Doctor(found)));
    // Asked, nothing is opened until the person confirms what it spends.
    let asked = state.update(Input::Stroke(Stroke::Insert('s')));
    assert!(asked.is_empty(), "{asked:?}");
    let overlay = state.screen(&Markdown.look()).overlay;
    let asks = overlay.map(|overlay| Markdown.draw(&overlay.doc));
    assert!(
        asks.as_deref()
            .is_some_and(|asks| asks.contains("a prompt each")),
        "{asks:?}"
    );
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let [Request::Perform(sessions, Operation::Doctor { sessions: true })] = asked.as_slice()
    else {
        panic!("confirmed, a session is opened per binding: {asked:?}");
    };
    let sessions = *sessions;
    // What the sessions remark stands as it comes.
    let said = "the session on executor is still opening".to_string();
    state.update(Input::Remark(
        sessions,
        Remark::Unstopped { said: said.clone() },
    ));
    assert!(
        main_region(&state).contains(&said),
        "{}",
        main_region(&state)
    );
    // Escape stops them, and the stage stays.
    let asked = state.update(Input::Stroke(Stroke::Decline));
    assert!(
        asked
            .iter()
            .any(|request| matches!(request, Request::LetGo(ticket) if *ticket == sessions)),
        "{asked:?}"
    );
    let shown = main_region(&state);
    assert!(
        shown.contains(yunta_render::screen::SESSIONS_STOPPED),
        "{shown}"
    );
    let trail = state.screen(&Markdown.look()).trail;
    assert_eq!(trail, ["yunta", "doctor"]);
}

#[test]
fn sessions_under_way_go_on_while_the_person_looks_elsewhere_and_comes_back() {
    let (mut state, checks) = opened();
    state.update(Input::Outcome(checks, Outcome::Doctor(Vec::new())));
    state.update(Input::Stroke(Stroke::Insert('s')));
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let [Request::Perform(sessions, Operation::Doctor { sessions: true })] = asked.as_slice()
    else {
        panic!("confirmed, a session is opened per binding: {asked:?}");
    };
    let sessions = *sessions;
    // Upkeep, opened from the palette over doctor, then back to doctor.
    let mut asked = Vec::new();
    for stroke in "/gc"
        .chars()
        .map(Stroke::Insert)
        .chain([Stroke::Enter, Stroke::Decline])
    {
        asked.extend(state.update(Input::Stroke(stroke)));
    }
    assert_eq!(state.screen(&Markdown.look()).trail, ["yunta", "doctor"]);
    let opened_upkeep = asked.iter().any(|request| {
        matches!(
            request,
            Request::Perform(_, Operation::Collect { dry_run: true })
        )
    });
    assert!(opened_upkeep, "upkeep opened over doctor: {asked:?}");
    let let_go = asked
        .iter()
        .any(|request| matches!(request, Request::LetGo(ticket) if *ticket == sessions));
    assert!(!let_go, "the sessions go on: {asked:?}");
    let checked_again = asked.iter().any(|request| {
        matches!(
            request,
            Request::Perform(_, Operation::Doctor { sessions: false })
        )
    });
    assert!(!checked_again, "nothing is checked over them: {asked:?}");
}
