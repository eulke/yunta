//! The palette over a stage, opened with `/`: what the stage offers first, answering a
//! parked run among it, then what every stage offers, each row what it does, the key the
//! stage takes it by where one does, and the command that does the same, spelled by render.
//! Typed words narrow it by what a row does; a row chosen does what its key does on the
//! stage, or what taking its step does; Escape closes it with nothing done. Help over a run,
//! and over the field, lists the keys its legend leaves out.

mod support;

use surco::components::ChoiceRow;
use surco::doc::Block;
use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::Paint;
use surco::surface::{Markdown, Surface, Terminal};
use yunta_core::step::{Choice, NextStep, Reach};
use yunta_core::RunId;
use yunta_render::command;
use yunta_testkit::Bench;
use yunta_testkit_ui::screens::run;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Request, State};

use support::{on_the_field, read_whole_and_asked};

/// A gate nobody answers: its run parks on it.
const PARKS: &str = "name: gated\nnodes:\n  - { id: approve, kind: gate, assignee: lead, \
                     message: \"Go on?\", options: [go, stop] }\n";

/// A mock fixture with no sessions: the workflow opens none.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// The steps every stage's palette offers after its own, in their order.
fn always() -> [NextStep; 5] {
    [
        NextStep::ListRuns {
            reach: Reach::Repository,
        },
        NextStep::List,
        NextStep::Doctor {
            sessions: false,
            full: false,
        },
        NextStep::Gc { dry_run: false },
        NextStep::Schemas,
    ]
}

/// The palette's rows as it stands, each the command that does the same.
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
        .map(|choices| {
            choices
                .rows
                .iter()
                .filter_map(|row| row.said.clone())
                .collect()
        })
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

/// The application on `bench`'s parked run, its first reading and what it asks handed as
/// what the state follows hands them.
async fn on_the_parked_run(bench: &Bench) -> (State, RunId) {
    let run = bench.run_id.clone();
    let read = read_whole_and_asked(bench).await;
    let (mut state, _) = on_the_field(&[&run]).await;
    state.update(Input::Stroke(Stroke::Enter));
    for news in read {
        state.update(Input::FollowNews(news));
    }
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
    // Typed words narrow it by what a row does: `check` leads with doctor, which opens over
    // the run.
    for typed in "check".chars() {
        state.update(Input::Stroke(Stroke::Insert(typed)));
    }
    let doctor = command::line(&always()[2]);
    assert_eq!(rows(&state).first(), Some(&doctor));
    let asked = state.update(Input::Stroke(Stroke::Enter));
    asked_for(&asked, |work| {
        matches!(work, Operation::Doctor { sessions: false })
    });
    let screen = state.screen(&Markdown.look());
    assert!(screen.overlay.is_none(), "chosen, the palette closes");
    assert_eq!(screen.trail, ["yunta", "machine"]);
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

#[tokio::test]
async fn where_words_are_typed_a_slash_is_typed() {
    // A repository set up with no runs opens on the first run's question.
    let (mut state, _) = on_the_field(&[]).await;
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
    let listed: Vec<String> = choices(&state).into_iter().map(|row| row.name).collect();
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

#[tokio::test]
async fn the_fields_close_row_names_the_run_x_closes() {
    let first: RunId = "01M7PALETTE000000000AAAAAA".parse().expect("a run's id");
    let second: RunId = "01M7PALETTE000000000BBBBBB".parse().expect("a run's id");
    let (mut state, _) = on_the_field(&[&first, &second]).await;
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

/// The field's `d` and `e` rows name the command that lists what each key shows from where
/// the field stands: at the machine's reach, `d` every run on this machine with the closed;
/// with the closed shown there, `d` every run on this machine and `e` this repository's runs
/// with the closed.
#[tokio::test]
async fn the_fields_d_and_e_rows_name_the_command_that_lists_the_same() {
    let run: RunId = "01M7PALETTE000000000DDDDDD".parse().expect("a run's id");
    let (mut state, _) = on_the_field(&[&run]).await;
    let row_of = |state: &State, key: &str| -> Option<String> {
        let rows = choices(state);
        let row = rows.iter().find(|row| row.key.as_deref() == Some(key))?;
        row.said.clone()
    };
    for stroke in [Stroke::Insert('e'), Stroke::Insert('/')] {
        state.update(Input::Stroke(stroke));
    }
    assert_eq!(
        row_of(&state, "d").as_deref(),
        Some("yunta list --runs --all --full"),
        "at the machine's reach"
    );
    for stroke in [Stroke::Decline, Stroke::Insert('d'), Stroke::Insert('/')] {
        state.update(Input::Stroke(stroke));
    }
    assert_eq!(
        row_of(&state, "d").as_deref(),
        Some("yunta list --runs --all"),
        "at the machine's reach, the closed shown"
    );
    assert_eq!(
        row_of(&state, "e").as_deref(),
        Some("yunta list --runs --full"),
        "at the machine's reach, the closed shown"
    );
}

/// The rows of the palette standing over `state` as its list holds them: each what it does,
/// the key the stage under it takes it by, and the command that does the same.
fn choices(state: &State) -> Vec<ChoiceRow> {
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
        .map(|choices| choices.rows.clone())
        .unwrap_or_default()
}

/// The run the screen of `state` shows.
fn shown_run(state: &State) -> RunId {
    state
        .followed()
        .run
        .expect("a run's screen follows its run")
}

#[tokio::test]
async fn a_runs_palette_row_says_what_it_does_its_key_and_its_command() {
    let screen = run::opened("palette").await;
    let stats = NextStep::Stats {
        run: shown_run(screen.state()),
    };
    let rows = choices(screen.state());
    let row = rows
        .iter()
        .find(|row| row.said.as_deref() == Some(command::line(&stats).as_str()))
        .unwrap_or_else(|| panic!("no row's command is what the run cost: {rows:#?}"));
    assert_eq!(row.name, command::does(&stats), "{row:?}");
    assert_eq!(row.key.as_deref(), Some("s"), "{row:?}");
}

/// The box a palette stands in as a terminal of 120 columns draws it in Unicode, a row a
/// line.
fn boxed(state: &State) -> Vec<String> {
    let look = Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(118),
        motion: Motion::Still,
    };
    let overlay = state.screen(&look).overlay.expect("the palette stands");
    let boxed = Terminal::on(look).boxed(&overlay, 84, 20);
    boxed.lines.iter().map(|line| line.text()).collect()
}

#[tokio::test]
async fn words_typed_narrow_the_rows_by_what_each_does_and_stand_where_typing_was_offered() {
    let mut screen = run::opened("palette").await;
    let before = boxed(screen.state());
    assert!(before[1].contains("type to filter"), "{before:#?}");
    let chronicle = command::line(&NextStep::Follow {
        run: shown_run(screen.state()),
    });
    assert!(chronicle.contains("st"), "{chronicle}");
    screen
        .pressed(&[Stroke::Insert('s'), Stroke::Insert('t')])
        .await;
    let after = boxed(screen.state());
    assert!(after[1].trim_matches('│').trim() == "▸ st", "{after:#?}");
    let rows = choices(screen.state());
    assert!(!rows.is_empty(), "{after:#?}");
    let says_the_chronicle = |row: &ChoiceRow| {
        std::iter::once(&row.name)
            .chain(&row.said)
            .any(|said| *said == chronicle)
    };
    assert!(
        !rows.iter().any(says_the_chronicle),
        "a row its command alone answers stays out: {rows:#?}"
    );
    for row in &rows {
        let mut wanted = "st".chars().peekable();
        for letter in row.name.chars() {
            if wanted.peek() == Some(&letter) {
                wanted.next();
            }
        }
        assert!(wanted.peek().is_none(), "{row:?} holds no `st`");
    }
}

/// The keys help standing over `state` lists, each with what it does.
fn helped(state: &State) -> Vec<(String, String)> {
    let overlay = state
        .screen(&Markdown.look())
        .overlay
        .filter(|overlay| overlay.title == yunta_render::screen::HELP)
        .expect("help stands");
    overlay
        .doc
        .blocks()
        .iter()
        .find_map(|block| match block {
            Block::KeyList(list) => Some(&list.keys.0),
            _ => None,
        })
        .expect("help lists its keys")
        .iter()
        .map(|binding| (binding.keys.named().to_string(), binding.does.clone()))
        .collect()
}

#[tokio::test]
async fn help_over_a_run_lists_each_key_its_legend_leaves_out_with_what_it_does() {
    let screen = run::opened("help").await;
    let run = shown_run(screen.state());
    let keys = helped(screen.state());
    for (key, step) in [
        ('c', NextStep::Cancel { run: run.clone() }),
        ('o', NextStep::Open { run }),
    ] {
        let listed = (key.to_string(), command::does(&step).to_string());
        assert!(keys.contains(&listed), "{listed:?} in {keys:#?}");
    }
}

/// Help over the field lists the keys its legend leaves out, each with what it does: stopping
/// a run, the workflows and the machine's checks among them.
#[tokio::test]
async fn help_over_the_field_lists_the_keys_its_legend_leaves_out() {
    let run: RunId = "01M7PALETTE000000000AAAAAA".parse().expect("a run's id");
    let (mut state, _) = on_the_field(&[&run]).await;
    state.update(Input::Stroke(Stroke::Insert('?')));
    let keys = helped(&state);
    let doctor = NextStep::Doctor {
        sessions: false,
        full: false,
    };
    for (key, step) in [
        ('c', NextStep::Cancel { run }),
        ('w', NextStep::List),
        ('m', doctor),
    ] {
        let listed = (key.to_string(), command::does(&step).to_string());
        assert!(keys.contains(&listed), "{listed:?} in {keys:#?}");
    }
}
