//! Each target screen drawn on Ratatui's test backend equals its target frame, which a
//! script wrote from the layout's rules apart from the Rust: the frames are what the code
//! is held to, so this test never writes them.
//!
//! A run's screen is drawn at each of the four sizes its targets are written at, from the
//! state `yunta_testkit_ui::screens::run` opens on a target's cut, laid out before each key
//! where its keys scroll, and held whole to its target, landed beside the goldens. Each
//! target's cut is built and its screen drawn on every core at once, a target to a thread,
//! since a cut builds its runs whole (`on_every_core`); every target that differs is named,
//! at each size it differs at, none hiding another.
//!
//! The frames hold text alone; what a box covers is held in color, cell by cell, against the
//! box drawn over nothing.

mod covered;
#[path = "screens/on_every_core.rs"]
mod on_every_core;

use std::path::{Path, PathBuf};

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::Terminal;
use surco::components::{Screen, ScreenLaid};
use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::Paint;
use yunta_testkit_ui::screens;
use yunta_ui::prompt::Stroke;
use yunta_ui::State;

fn targets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/screens")
}

/// A terminal a target frame is drawn on: its name, its glyphs and its size.
struct Environment {
    name: &'static str,
    glyphs: Glyphs,
    columns: u16,
    rows: u16,
}

const UNICODE_120: Environment = Environment {
    name: "unicode-120",
    glyphs: Glyphs::Unicode,
    columns: 120,
    rows: 36,
};

const UNICODE_80: Environment = Environment {
    name: "unicode-80",
    glyphs: Glyphs::Unicode,
    columns: 80,
    rows: 24,
};

const ASCII_120: Environment = Environment {
    name: "ascii-120",
    glyphs: Glyphs::Ascii,
    columns: 120,
    rows: 36,
};

const ASCII_80: Environment = Environment {
    name: "ascii-80",
    glyphs: Glyphs::Ascii,
    columns: 80,
    rows: 24,
};

const UNICODE_60: Environment = Environment {
    name: "unicode-60",
    glyphs: Glyphs::Unicode,
    columns: 60,
    rows: 14,
};

const UNICODE_59: Environment = Environment {
    name: "unicode-59",
    glyphs: Glyphs::Unicode,
    columns: 59,
    rows: 14,
};

/// `state`'s screen drawn on a test backend of `environment`'s size, still: every row, its
/// trailing blanks cut, one a line.
fn drawn(state: &State, environment: &Environment) -> String {
    drawn_moving(state, environment, Motion::Still)
}

/// `state`'s screen drawn on a test backend of `environment`'s size in `motion`: every row,
/// its trailing blanks cut, one a line.
fn drawn_moving(state: &State, environment: &Environment, motion: Motion) -> String {
    drawn_laid(state, environment, motion).0
}

/// `state`'s screen drawn on a test backend of `environment`'s size in `motion`, and what
/// the drawing laid out of it, which the next key that scrolls moves from.
fn drawn_laid(state: &State, environment: &Environment, motion: Motion) -> (String, ScreenLaid) {
    let look = Look {
        glyphs: environment.glyphs,
        paint: Paint::Plain,
        width: Width::exact(usize::from(environment.columns.saturating_sub(2))),
        motion,
    };
    let screen = state.screen(&look);
    let backend = TestBackend::new(environment.columns, environment.rows);
    let mut terminal = Terminal::new(backend).expect("a test backend");
    let mut laid = ScreenLaid::default();
    terminal
        .draw(|frame| {
            laid = yunta_tui::drawn(frame, &screen, &look);
        })
        .expect("a drawing");
    let buffer = terminal.backend().buffer();
    let rows = (0..environment.rows)
        .map(|y| {
            let row: String = (0..environment.columns)
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            format!("{}\n", row.trim_end())
        })
        .collect();
    (rows, laid)
}

/// `drawn` against the target `name` in `environment`, both shown whole where they differ.
fn held_to_its_target(name: &str, environment: &Environment, drawn: &str) {
    if let Some(differs) = differs_from_its_target(name, environment, drawn) {
        panic!("{differs}");
    }
}

/// How `drawn` differs from the target `name` in `environment`, both shown whole; none where
/// it equals it.
fn differs_from_its_target(name: &str, environment: &Environment, drawn: &str) -> Option<String> {
    let path = targets().join(format!("{name}.{}.txt", environment.name));
    let target = std::fs::read_to_string(&path).expect("a target frame");
    (drawn != target).then(|| {
        format!(
            "{name} at {} differs from its target {}:\n--- drawn\n{drawn}--- target\n{target}",
            environment.name,
            path.display()
        )
    })
}

#[test]
fn every_screen_equals_its_target_frame() {
    yunta_testkit::on_a_deep_stack(|| async {
        let built = [("field", screens::field().await)];
        for (name, state) in &built {
            for environment in [&ASCII_80, &UNICODE_60] {
                held_to_its_target(name, environment, &drawn(state, environment));
            }
        }
        let (_, field) = &built[0];
        held_to_its_target("room-needed", &UNICODE_59, &drawn(field, &UNICODE_59));
    });
}

/// The targets of a run's screen landed beside the goldens, each held whole at the four
/// sizes: a session's node at work, its tasks accepted one event on, a check at work, a run
/// created with no node started, a loop at work and one event on, a loop with two tasks at
/// work at once, three reviewers posting findings, and the findings being answered; then a
/// node the person points at while the run does not lead, by how it stands: ended, never
/// run, left out by the run's mode, at work, at work again once a gate sent the run back to
/// it, and the node the run waits on the person at; then a run standing with nothing to
/// answer here, by why: a pause no option here answers, no room on the disk, a node left
/// half done, a person's stop, and no process driving it; then work standing still: a run
/// that goes on alone once a limit resets, an answer awaiting its resume, and a pause whose
/// ask is not read yet, and one whose ask could not be read; then a run waiting on its pull
/// request, whose forge's reviewers have not answered; then a run that finished, as its
/// receipt, the design's run and one that reports a finding that blocks it; then a run that
/// ended failed, at the node that failed, one that closed promoted as the person held its
/// screen on the node its end names, and one whose log broke, at the node it waited on; then
/// the run's whole chronicle, opened from its palette over a loop at work; then a run's
/// numbers and its graph, over the run waiting on the person at its last gate; then the
/// palette over a run at work, whole and narrowed by words typed, over a run's receipt, and
/// over the node whose round the person put aside, and help over a run at work; then the
/// last gate's card in place, under the change by file
/// and the findings and criteria in a line, and the note its answer leaves as the run goes on
/// to its pull request; a gate that shows no plan, its card under the document it shows and
/// its change; and a run parked on its child, shown in its place with the child's card; then
/// a card on what else it rests on: a scope asked for, under the write the fence refused, the
/// task's reason in it, a note being typed under its options, and the note its grant leaves
/// as the task goes on; a failure under the
/// end of what it printed, and the note its retry leaves as the node runs again; a task's
/// departure from its plan under what each says; and a cap under what is spent against it, of
/// tokens and of a loop's iterations; then a round of questions in place of the nodes and the
/// pane, under the request it is about, its first question in turn and then its second, one
/// whose answers the engine refused with the refusal under its question, and the round put
/// aside, the node it waits at standing as its own scene; then the plan's desk between the rule
/// and the attention row, opened on the plan's first task, an option a number named waiting to
/// be confirmed, the words an option asks for typed at its foot in place of the options, the
/// task turned over to its proof, what the plan decided for the person, and the change the
/// plan makes as a whole. The node the run waits on the person at is reached by `esc` on the
/// card in place, which puts it aside.
const LANDED: [&str; 60] = [
    "work",
    "work-moved",
    "work-command",
    "created",
    "board",
    "board-moved",
    "board-tasks",
    "findings",
    "findings-answered",
    "node-ended",
    "node-never-ran",
    "node-skipped",
    "node-working",
    "node-sent-back",
    "node-needs-you",
    "standstill",
    "no-room",
    "orphans",
    "stopped",
    "stalled",
    "wakes-alone",
    "still-answered",
    "still-unread",
    "card-unread",
    "pull-request",
    "receipt",
    "reported",
    "failed",
    "promoted",
    "broken",
    "chronicle",
    "stats",
    "graph",
    "palette",
    "palette-narrowed",
    "receipt-palette",
    "palette-aside",
    "help",
    "ship",
    "card-recorded",
    "card",
    "child",
    "scope",
    "card-words",
    "scope-recorded",
    "failure",
    "failure-recorded",
    "departure",
    "budget",
    "loop-cap",
    "round",
    "round-answered",
    "round-refused",
    "round-aside",
    "desk",
    "desk-confirm",
    "desk-words",
    "desk-proof",
    "desk-decided",
    "desk-change",
];

/// A run's screen equals its landed target, cell for cell, at each of the four sizes its
/// targets are written at: its pane and its legend with every row around them. The targets
/// are opened on every core at once, and every one that differs is named at each size.
#[test]
fn a_runs_screen_equals_its_landed_target_at_every_size() {
    let differing = on_every_core::differences(&LANDED, |target| async move {
        let screen = screens::run::opened(target).await;
        [&UNICODE_120, &UNICODE_80, &ASCII_120, &UNICODE_60]
            .into_iter()
            .filter_map(|environment| {
                let drawn = drawn_moving(screen.state(), environment, Motion::Moves);
                differs_from_its_target(target, environment, &drawn)
            })
            .collect()
    });
    assert!(
        differing.is_empty(),
        "the landed screens differ from their targets, {} times in all:\n\n{}",
        differing.len(),
        differing.join("\n\n")
    );
}

/// A run's screen short of rows, where the gap under what stands across gave way, says what
/// the run spent in figures alone, the bars given way even where they would fit; a screen
/// with rows for that gap keeps them. Each case is a target, an environment and its meter's
/// row as the target draws it.
#[test]
fn a_screen_short_of_rows_says_what_its_run_spent_in_figures_alone() {
    yunta_testkit::on_a_deep_stack(|| async {
        let cases = [
            ("created", &UNICODE_60, " 0 tokens · 0s at work"),
            ("stopped", &UNICODE_60, " 0 tokens · 45s at work"),
            ("created", &UNICODE_80, " 0 tokens ▁▁ · 0s at work"),
        ];
        for (target, environment, meter) in cases {
            let screen = screens::run::opened(target).await;
            let drawn = drawn_moving(screen.state(), environment, Motion::Moves);
            let row = usize::from(environment.rows) - 3;
            assert_eq!(
                drawn.lines().nth(row),
                Some(meter),
                "{target} at {}:\n{drawn}",
                environment.name
            );
        }
    });
}

/// A run's feed scrolled back by the arrows equals its landed target, cell for cell, at each
/// of the four sizes: its cut's keys pressed with the screen laid out at that size before
/// each, as a terminal draws it between two keys, so each key moves from what the last
/// drawing showed. Its window counts the rows above and under it, and its nodes and its keys
/// say how to follow the run again.
#[test]
fn a_feed_scrolled_back_equals_its_landed_target_at_every_size() {
    yunta_testkit::on_a_deep_stack(|| async {
        let target = "feed-back";
        for environment in [&UNICODE_120, &UNICODE_80, &ASCII_120, &UNICODE_60] {
            let screen = screens::run::opened_laid_out(target, |state| {
                drawn_laid(state, environment, Motion::Moves).1
            })
            .await;
            let drawn = drawn_moving(screen.state(), environment, Motion::Moves);
            held_to_its_target(target, environment, &drawn);
        }
    });
}

/// Where a target names the process that drives its run: the registry of its cut names the
/// process the test runs in, so the target holds this in place of a number.
const PID: &str = "{pid}";

/// The gist of the run asked elsewhere, its process `pid`, on a row of `cells` as the rule on
/// a cut cuts it, written apart from the code: whole where it fits; else what fits beside the
/// ellipsis, the space before the cut left out, and the cut falling before the process's
/// number where it would split that number or end on it.
fn asking_gist_cut(pid: &str, cells: usize) -> String {
    let gist = format!("node `ship` is asking at its terminal (pid {pid}); answer it there");
    if gist.chars().count() <= cells {
        return gist;
    }
    let number = gist.find(pid).expect("the gist names the process");
    let kept: String = gist.chars().take(cells - 1).collect();
    let end = match kept.len() {
        end if end > number && end <= number + pid.len() => number,
        end => end,
    };
    format!("{}…", kept[..end].trim_end())
}

/// A run whose process asks a question at its own terminal equals its landed target at each
/// of the four sizes, its pane titled by the gate and its assignee: the target's process read
/// as the one this test runs in, which the registry of its cut names. At the least terminal
/// its gist, which names that process, is cut on its row: the target's row is the rule's cut
/// for a number of five digits, and the row drawn is the rule's cut for the number in hand,
/// so a process of any number holds it.
#[test]
fn a_run_asked_elsewhere_equals_its_landed_target_at_every_size() {
    yunta_testkit::on_a_deep_stack(|| async {
        let target = "asked-elsewhere";
        let screen = screens::run::opened(target).await;
        let pid = std::process::id().to_string();
        let gist_cut_at = [
            (&UNICODE_120, None),
            (&UNICODE_80, None),
            (&ASCII_120, None),
            (&UNICODE_60, Some(5)),
        ];
        for (environment, gist) in gist_cut_at {
            let drawn = drawn_moving(screen.state(), environment, Motion::Moves);
            let path = targets().join(format!("{target}.{}.txt", environment.name));
            let landed = std::fs::read_to_string(&path).expect("a target frame");
            let mut rows: Vec<String> = landed.lines().map(|row| row.replace(PID, &pid)).collect();
            if let Some(gist) = gist {
                let cells = usize::from(environment.columns) - 2;
                let held = landed.lines().nth(gist - 1).expect("the gist's row");
                assert_eq!(held, format!(" {}", asking_gist_cut(PID, cells)));
                rows[gist - 1] = format!(" {}", asking_gist_cut(&pid, cells));
            }
            let expected: String = rows.iter().map(|row| format!("{row}\n")).collect();
            assert!(
                drawn == expected,
                "{target} at {} differs from {}, the process {pid}:\n--- drawn\n{drawn}--- \
                 target\n{expected}",
                environment.name,
                path.display()
            );
        }
    });
}

/// A box open over a screen covers what stood under it whole: every cell inside it holds the
/// box's own glyph, tones and ground, as the box draws them over nothing. Held at the
/// design's size over the plan's desk, whose code block paints an added word on a ground of
/// its own and the rest on the code's ground, and whose rows over it carry a tint and bold
/// words, under the palette.
#[test]
fn a_box_covers_what_stood_under_it_whole() {
    yunta_testkit::on_a_deep_stack(|| async {
        let environment = &UNICODE_120;
        let look = Look {
            glyphs: environment.glyphs,
            paint: covered::PAINTED,
            width: Width::exact(usize::from(environment.columns - 2)),
            motion: Motion::Still,
        };
        let mut desk = screens::run::opened("desk").await;
        desk.pressed(&[Stroke::Insert('/')]).await;
        let over = desk.state().screen(&look);
        let overlay = desk.state().screen(&look).overlay;
        let alone = covered::alone(overlay.expect("the palette open over the desk"));
        let over = cells_of(&over, environment, &look);
        let alone = cells_of(&alone, environment, &look);
        if let Some(kept) = covered::kept_under(&over, &alone, &look) {
            panic!("the palette over the desk: {kept}");
        }
    });
}

/// `screen` drawn in `look` on a test backend of `environment`'s size: its cells, as the
/// terminal holds them.
fn cells_of(screen: &Screen, environment: &Environment, look: &Look) -> Buffer {
    let backend = TestBackend::new(environment.columns, environment.rows);
    let mut terminal = Terminal::new(backend).expect("a test backend");
    terminal
        .draw(|frame| {
            yunta_tui::drawn(frame, screen, look);
        })
        .expect("a drawing");
    terminal.backend().buffer().clone()
}
