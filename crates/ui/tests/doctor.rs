//! Doctor in the application, the machine, opened from the field: what every check of a
//! sandbox found, as `yunta doctor` prints it at a glance and with every check on its own
//! row, as `--full` prints it, its page as the screen tells it; and a session per binding
//! opened only once the person confirms each spends a prompt, what it remarks shown as it
//! comes, stopped with Escape, and what they find doctor's wherever it stands as it lands.

mod support;

use surco::components::Region;
use surco::surface::{Markdown, Surface};
use yunta_api::Remark;
use yunta_render::page::doctor::{Doctor, Finding};
use yunta_render::screen;
use yunta_testkit::Bench;
use yunta_testkit_ui::driving::{carried_out, pressed_while, CarriedOut, Inputs, Withheld};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Outcome, Request, Stage, State, Ticket};

use support::{on_the_field, performed, performer_in};

/// The screen's main region, as Markdown writes it.
fn main_region(state: &State) -> String {
    let look = Markdown.look();
    Markdown.draw(&state.screen(&look).main.doc)
}

/// `page` as the screen tells it, a page it stands on, as Markdown writes it.
fn drawn(page: &Doctor) -> String {
    Markdown.draw(&Region::of_a_page(page.on_screen()).doc)
}

/// Doctor opened from a field of one stopped run: the state, and the ticket its checks
/// are asked under.
async fn opened() -> (State, Ticket) {
    let run = "01M7DOCTOR0000000000000000".parse().expect("a run's id");
    let (mut state, _) = on_the_field(&[&run]).await;
    let asked = state.update(Input::Stroke(Stroke::Insert('m')));
    let [Request::Perform(ticket, Operation::Doctor { sessions: false })] = asked.as_slice() else {
        panic!("doctor checks the machine as it opens: {asked:?}");
    };
    (state, *ticket)
}

#[tokio::test]
async fn the_findings_are_those_doctor_prints_at_a_glance_and_whole() {
    let bench = Bench::new();
    let (mut state, checks) = opened().await;
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
    assert_eq!(trail, ["yunta", "machine"]);
    // Every check on its own row, as `--full` prints them, then the glance again.
    assert_ne!(printed(true), printed(false), "every row says more");
    state.update(Input::Stroke(Stroke::Insert('f')));
    assert_eq!(main_region(&state), printed(true));
    state.update(Input::Stroke(Stroke::Insert('f')));
    assert_eq!(main_region(&state), printed(false));
}

#[tokio::test]
async fn a_session_per_binding_is_opened_once_confirmed_and_stopped_with_escape() {
    let (mut state, checks) = opened().await;
    let found = vec![Finding::Committer {
        name: "Ada".to_string(),
        address: "ada@example.com".to_string(),
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
    assert_eq!(trail, ["yunta", "machine"]);
}

#[tokio::test]
async fn sessions_under_way_go_on_while_the_person_looks_elsewhere_and_comes_back() {
    let (mut state, checks) = opened().await;
    state.update(Input::Outcome(checks, Outcome::Doctor(Vec::new())));
    state.update(Input::Stroke(Stroke::Insert('s')));
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let [Request::Perform(sessions, Operation::Doctor { sessions: true })] = asked.as_slice()
    else {
        panic!("confirmed, a session is opened per binding: {asked:?}");
    };
    let sessions = *sessions;
    // Upkeep, opened from the palette over doctor by what its row does, then back to doctor.
    let mut asked = Vec::new();
    for stroke in "/orphan"
        .chars()
        .map(Stroke::Insert)
        .chain([Stroke::Enter, Stroke::Decline])
    {
        asked.extend(state.update(Input::Stroke(stroke)));
    }
    assert_eq!(state.screen(&Markdown.look()).trail, ["yunta", "machine"]);
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

/// The sessions are doctor's while a new run stands over it: what they find lands on doctor's
/// page, which shows it as the person comes back, under the checks doctor runs again then,
/// and the new run takes none of it for its own reading of the machine.
#[tokio::test]
async fn sessions_answered_while_a_new_run_stands_over_doctor_are_doctors() {
    let run = "01M7DOCTOR0000000000000003".parse().expect("a run's id");
    let (field, world) = on_the_field(&[&run]).await;
    let performer = performer_in(&world);
    let mut inputs = Inputs::new();
    let sessions = |work: &Operation| matches!(work, Operation::Doctor { sessions: true });
    let strokes = [
        Stroke::Insert('m'),
        Stroke::Insert('s'),
        Stroke::Enter,
        Stroke::Insert('n'),
    ];
    let pressed = pressed_while(
        &mut inputs,
        field,
        Vec::new(),
        &performer,
        sessions,
        &strokes,
    );
    let Withheld { driven, withheld } = pressed.await;
    assert!(
        matches!(driven.state.stage(), Some(Stage::Start(_))),
        "a new run stands over doctor while its sessions run"
    );
    let [(ticket, work)] = withheld.as_slice() else {
        panic!("a session per binding is opened once: {withheld:?}");
    };
    let CarriedOut { outcome, .. } = carried_out(&performer, *ticket, work.clone()).await;
    let Outcome::Doctor(findings) = &outcome else {
        panic!("the sessions come to their findings: {outcome:?}");
    };
    let page = drawn(&Doctor {
        session: true,
        findings,
        full: false,
    });
    let mut state = driven.state;
    let asked = state.update(Input::Outcome(*ticket, outcome.clone()));
    let checks = |work: &Operation| matches!(work, Operation::Doctor { sessions: false });
    let back = [Stroke::Decline];
    let back = pressed_while(&mut inputs, state, asked, &performer, checks, &back).await;
    let shown = main_region(&back.driven.state);
    assert!(!shown.contains(screen::OPENING_SESSIONS), "{shown}");
    assert!(shown.contains(&page), "{shown}");
    assert!(shown.contains(screen::CHECKING_THE_MACHINE), "{shown}");
}

/// What a round of sessions found stays when doctor comes back into view and checks again:
/// the plain checks refresh what they find, and the sessions' findings stand under them until
/// the person asks for sessions again or leaves doctor.
#[tokio::test]
async fn a_round_of_sessions_found_stays_when_doctor_checks_again() {
    let (mut state, checks) = opened().await;
    let committer = |name: &str| Finding::Committer {
        name: name.to_string(),
        address: "someone@example.com".to_string(),
    };
    state.update(Input::Outcome(
        checks,
        Outcome::Doctor(vec![committer("Ada")]),
    ));
    state.update(Input::Stroke(Stroke::Insert('s')));
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let [Request::Perform(sessions, Operation::Doctor { sessions: true })] = asked.as_slice()
    else {
        panic!("confirmed, a session is opened per binding: {asked:?}");
    };
    let found = vec![committer("Ada"), Finding::NoBinding];
    state.update(Input::Outcome(*sessions, Outcome::Doctor(found)));
    let mut asked = state.update(Input::Stroke(Stroke::Insert('u')));
    asked.extend(state.update(Input::Stroke(Stroke::Decline)));
    let again = asked.iter().find_map(|request| match request {
        Request::Perform(ticket, Operation::Doctor { sessions: false }) => Some(*ticket),
        _ => None,
    });
    let again = again.unwrap_or_else(|| panic!("doctor checks again as it comes back: {asked:?}"));
    state.update(Input::Outcome(
        again,
        Outcome::Doctor(vec![committer("Bea")]),
    ));
    let page = drawn(&Doctor {
        session: true,
        findings: &[committer("Bea"), Finding::NoBinding],
        full: false,
    });
    let shown = main_region(&state);
    assert!(shown.contains(&page), "{shown}\n--- the page\n{page}");
}
