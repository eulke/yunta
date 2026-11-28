//! The field's keys beyond moving, marking and closing: `e` reaches the machine's field and
//! back, `a` opens the run the attention row names, `n` a new run, `esc` does nothing, and `r`
//! resumes where the run's acts hold a resume and notes, on a run that waits on a decision,
//! that `enter` reads it.

use surco::surface::Markdown;
use yunta_api::{FieldReach, Followed};
use yunta_core::RunId;
use yunta_render::screen::{NEW_RUN, NOTHING_TO_RESUME};
use yunta_testkit_ui::cut::{Built, Cut, Shown};
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::screens::field;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Operation, Request};

use super::support::{on_the_field, performer_in};
use super::{
    asked_and_older, asked_at_a_terminal_and_two_decisions, pointed_row, pressed, FIRST, SECOND,
};

/// `e` follows the machine's field in the field's place, with no reading of it asked, and
/// the pointer stays on the run it stood on once that field is read.
#[tokio::test]
async fn widening_the_reach_follows_the_machines_field_and_keeps_the_pointer() {
    let (mut state, world) = on_the_field(&[&FIRST, &SECOND]).await;
    pressed(&mut state, &[Stroke::Insert('j')]);
    let asked = pressed(&mut state, &[Stroke::Insert('e')]);
    let machine = Followed {
        field: Some(FieldReach::Machine),
        run: None,
    };
    assert_eq!(asked, [Request::Follow(machine)]);

    let (state, _) = drive(state, asked, &performer_in(&world), &[]).await;
    let screen = state.screen(&Markdown.look());
    let field = Markdown.screen(&state.screen(&Markdown.look()));
    assert_eq!(screen.trail, ["yunta", "this machine"], "{field}");
    assert!(pointed_row(&state).contains(SECOND.handle()), "{field}");
}

/// `esc` does nothing on the field: it asks nothing, and the field stands.
#[tokio::test]
async fn escape_does_nothing_on_the_field() {
    let (mut state, _) = on_the_field(&[&FIRST]).await;
    let asked = pressed(&mut state, &[Stroke::Decline]);
    assert!(asked.is_empty(), "{asked:?}");
    assert!(pointed_row(&state).contains(FIRST.handle()));
}

/// `a` opens the screen of the run the attention row names: the older of the two that wait on
/// a decision, never the run asking at its own terminal.
#[tokio::test]
async fn a_opens_the_run_the_attention_row_names() {
    let mut field = asked_at_a_terminal_and_two_decisions().await;
    let (_, older) = asked_and_older(&field);
    field.pressed(&[Stroke::Insert('a')]).await;
    let followed = field.state().followed().run;
    assert_eq!(followed.as_ref().map(RunId::handle), Some(older.as_str()));
}

/// `n` opens a new run over the field, as the palette's `start a new run` does, with nothing
/// typed: never the run that waited longest on the person, which `a` goes to.
#[tokio::test]
async fn n_opens_a_new_run_over_the_field() {
    let mut field = asked_at_a_terminal_and_two_decisions().await;
    field.pressed(&[Stroke::Insert('n')]).await;
    let screen = field.state().screen(&Markdown.look());
    let drawn = Markdown.screen(&field.state().screen(&Markdown.look()));
    assert_eq!(
        screen.trail.last().map(String::as_str),
        Some(NEW_RUN),
        "{drawn}"
    );
    assert_eq!(field.state().followed().run, None, "{drawn}");
}

/// `n` opens the new run at once, before anything is read for it: the new run asks the
/// workflows it can be of and what doctor finds as it opens, and the field asks nothing.
#[tokio::test]
async fn n_opens_the_new_run_at_once_which_reads_what_it_needs() {
    let (mut state, _world) = on_the_field(&[&FIRST]).await;
    let asked = pressed(&mut state, &[Stroke::Insert('n')]);
    let screen = state.screen(&Markdown.look());
    assert_eq!(screen.trail.last().map(String::as_str), Some(NEW_RUN));
    let performed: Vec<&Operation> = asked
        .iter()
        .filter_map(|request| match request {
            Request::Perform(_, operation) => Some(operation),
            _ => None,
        })
        .collect();
    let catalog_and_machine = [
        Operation::ReadCatalog,
        Operation::Doctor { sessions: false },
    ];
    assert_eq!(performed, catalog_and_machine.iter().collect::<Vec<_>>());
}

/// The field over the one run `shown`, the pointer on it.
async fn over_one(shown: Shown) -> field::FieldScreen {
    field::over(Cut::of(&[shown]).await).await
}

/// `r` on a run that waits on a decision, which no resume answers, asks no work and notes that
/// `enter` reads it.
#[tokio::test]
async fn r_on_a_run_waiting_on_a_decision_notes_it_and_asks_no_work() {
    let field = over_one(Shown::at(Built::NotLoggedIn, 7)).await;
    let mut state = field.state().clone();
    let asked = pressed(&mut state, &[Stroke::Insert('r')]);
    assert!(asked.is_empty(), "{asked:?}");
    let noted = state.screen(&Markdown.look()).noted.map(|noted| noted.said);
    assert_eq!(noted.as_deref(), Some(NOTHING_TO_RESUME));
}

/// `r` on a run whose acts hold its resume hands it back, and notes what the resume did.
#[tokio::test]
async fn r_on_a_run_it_resumes_notes_the_resume() {
    let mut field = over_one(Shown::at(Built::ConsoleGate, 5)).await;
    field.pressed(&[Stroke::Insert('r')]).await;
    let noted = field.state().screen(&Markdown.look()).noted;
    assert_eq!(
        noted.map(|noted| noted.said).as_deref(),
        Some("C0NS0E resumed, driving forward independently.")
    );
}
