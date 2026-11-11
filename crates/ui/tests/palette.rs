//! The palette over a stage, opened with `/`: what the stage offers first, answering a
//! parked run among it, then what every stage offers, each row the command that does
//! the same, spelled by render, with what it does. Typed words narrow it; a row chosen
//! does what its key does on the stage, or opens the stage that does it; Escape closes
//! it with nothing done.

mod support;

use surco::doc::Block;
use surco::surface::{Markdown, Surface};
use yunta_core::step::{Choice, NextStep};
use yunta_core::RunId;
use yunta_render::command;
use yunta_testkit::Bench;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, News, Operation, Request, State};

use support::{on_the_field, performed, stopped};

/// A gate nobody answers: its run parks on it.
const PARKS: &str = "name: gated\nnodes:\n  - { id: approve, kind: gate, assignee: lead, \
                     message: \"Go on?\", options: [go, stop] }\n";

/// A mock fixture with no sessions: the workflow opens none.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// The steps every stage's palette offers after its own, in their order.
fn always() -> [NextStep; 4] {
    [
        NextStep::List,
        NextStep::Doctor {
            sessions: false,
            full: false,
        },
        NextStep::Gc { dry_run: true },
        NextStep::Schemas,
    ]
}

/// The palette's rows as it stands, each the command it names.
fn rows(state: &State) -> Vec<String> {
    let overlay = state
        .screen(&Markdown.look())
        .overlay
        .expect("the palette stands over the stage");
    overlay
        .doc
        .blocks()
        .iter()
        .find_map(|block| match block {
            Block::ChoiceList(list) => list.choices.as_ref(),
            _ => None,
        })
        .map(|choices| choices.rows.iter().map(|row| row.name.clone()).collect())
        .unwrap_or_default()
}

/// The ticket the first piece of work `asked` asks of the kind `kind` stands under.
fn asked_for(asked: &[Request], kind: fn(&Operation) -> bool) -> yunta_ui::Ticket {
    asked
        .iter()
        .find_map(|request| match request {
            Request::Perform(ticket, operation) if kind(operation) => Some(*ticket),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{asked:?}"))
}

/// The application on `bench`'s parked run, followed as its following first reads it.
async fn on_the_parked_run(bench: &Bench) -> (State, RunId) {
    let run = bench.run_id.clone();
    let (found, _) = performed(bench, Operation::Follow { run: run.clone() }).await;
    let Some(News::Following(read)) = found.into_iter().next() else {
        panic!("the following hands back the run as it reads first");
    };
    let mut state = on_the_field(vec![stopped(&run)]);
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let followed = asked_for(&asked, |work| matches!(work, Operation::Follow { .. }));
    state.update(Input::News(followed, News::Following(read)));
    (state, run)
}

#[tokio::test]
async fn a_parked_runs_palette_offers_answering_first_and_every_stages_steps_last() {
    let bench = Bench::new();
    bench.run(PARKS, NO_SESSIONS).await;
    let (mut state, run) = on_the_parked_run(&bench).await;
    let asked = state.update(Input::Stroke(Stroke::Insert('/')));
    assert!(
        asked.is_empty(),
        "opening the palette asks nothing: {asked:?}"
    );
    let listed = rows(&state);
    let answering = NextStep::ResolveGate {
        run: run.clone(),
        choice: Choice::Asked,
    };
    assert_eq!(
        listed.first(),
        Some(&command::line(&answering)),
        "{listed:?}"
    );
    let last: Vec<String> = always().iter().map(command::line).collect();
    assert!(listed.ends_with(&last), "{listed:?}");
    let stats = command::line(&NextStep::Stats { run: run.clone() });
    assert!(listed.contains(&stats), "{listed:?}");
    // Typed words narrow it: `doc` leads with doctor, which opens over the run.
    for typed in "doc".chars() {
        state.update(Input::Stroke(Stroke::Insert(typed)));
    }
    let doctor = command::line(&always()[1]);
    assert_eq!(rows(&state).first(), Some(&doctor));
    let asked = state.update(Input::Stroke(Stroke::Enter));
    asked_for(&asked, |work| {
        matches!(work, Operation::Doctor { sessions: false })
    });
    let screen = state.screen(&Markdown.look());
    assert!(screen.overlay.is_none(), "chosen, the palette closes");
    assert_eq!(screen.trail, ["yunta", "doctor"]);
}

#[tokio::test]
async fn a_row_does_what_its_key_does_and_escape_does_nothing() {
    let bench = Bench::new();
    bench.run(PARKS, NO_SESSIONS).await;
    let (state, run) = on_the_parked_run(&bench).await;
    // Escape closes it, with nothing asked.
    let mut closed = state.clone();
    closed.update(Input::Stroke(Stroke::Insert('/')));
    let asked = closed.update(Input::Stroke(Stroke::Decline));
    assert!(asked.is_empty(), "{asked:?}");
    let screen = closed.screen(&Markdown.look());
    assert!(screen.overlay.is_none());
    // What it cost, chosen in the palette and pressed on the stage, are one act.
    let stats = command::line(&NextStep::Stats { run });
    let mut chosen = state.clone();
    chosen.update(Input::Stroke(Stroke::Insert('/')));
    let at = rows(&chosen)
        .iter()
        .position(|row| *row == stats)
        .expect("the palette offers what the run cost");
    for _ in 0..at {
        chosen.update(Input::Stroke(Stroke::Down));
    }
    let by_the_palette = chosen.update(Input::Stroke(Stroke::Enter));
    let mut pressed = state.clone();
    let by_the_key = pressed.update(Input::Stroke(Stroke::Insert('s')));
    let operations = |asked: &[Request]| -> Vec<Operation> {
        asked
            .iter()
            .filter_map(|request| match request {
                Request::Perform(_, operation) => Some(operation.clone()),
                _ => None,
            })
            .collect()
    };
    assert_eq!(operations(&by_the_palette), operations(&by_the_key));
    let draw = |state: &State| Markdown.screen(&state.screen(&Markdown.look()));
    assert_eq!(draw(&chosen), draw(&pressed));
}

#[test]
fn where_words_are_typed_a_slash_is_typed() {
    // A repository set up with no runs opens on the first run's question.
    let mut state = on_the_field(Vec::new());
    assert!(state.typing(), "the first run asks for words");
    state.update(Input::Stroke(Stroke::Insert('/')));
    let screen = state.screen(&Markdown.look());
    assert!(
        screen.overlay.is_none(),
        "no palette opens over a typed line"
    );
    let main = Markdown.draw(&screen.main.doc);
    assert!(main.contains('/'), "{main}");
}

#[tokio::test]
async fn a_palette_taller_than_its_box_keeps_the_row_pointed_at_in_sight() {
    let bench = Bench::new();
    bench.run(PARKS, NO_SESSIONS).await;
    let (mut state, _) = on_the_parked_run(&bench).await;
    state.update(Input::Stroke(Stroke::Insert('/')));
    let listed = rows(&state);
    let last = listed.len() - 1;
    for _ in 0..last {
        state.update(Input::Stroke(Stroke::Down));
    }
    let look = surco::look::Look::plain();
    let overlay = state.screen(&look).overlay.expect("the palette stands");
    let (shown, _) = surco::surface::Terminal::on(look).scrolled(&overlay.doc, overlay.sight, 4);
    let pointer = look.glyphs.pointer();
    let pointed = shown
        .iter()
        .map(|line| line.text())
        .find(|line| line.trim_start().starts_with(pointer));
    let pointed = pointed.unwrap_or_else(|| panic!("the row pointed at is in sight"));
    assert!(pointed.contains(&listed[last]), "{pointed}");
}

#[test]
fn the_fields_close_row_names_the_run_x_closes() {
    let first: RunId = "01M7PALETTE000000000AAAAAA".parse().expect("a run's id");
    let second: RunId = "01M7PALETTE000000000BBBBBB".parse().expect("a run's id");
    let mut state = on_the_field(vec![stopped(&first), stopped(&second)]);
    // The first picked, the pointer on the second: `x` closes the first.
    for stroke in [
        Stroke::Insert(' '),
        Stroke::Insert('j'),
        Stroke::Insert('/'),
    ] {
        state.update(Input::Stroke(stroke));
    }
    let closes = |run: &RunId| command::line(&NextStep::Close { run: run.clone() });
    let listed = rows(&state);
    assert!(listed.contains(&closes(&first)), "{listed:?}");
    assert!(!listed.contains(&closes(&second)), "{listed:?}");
    // Both picked: no one command closes them together, so no row says one does.
    for stroke in [Stroke::Decline, Stroke::Insert(' '), Stroke::Insert('/')] {
        state.update(Input::Stroke(stroke));
    }
    let listed = rows(&state);
    assert!(
        !listed.iter().any(|row| row.starts_with("yunta close")),
        "{listed:?}"
    );
}
