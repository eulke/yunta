//! Each target screen drawn on Ratatui's test backend equals its target frame, which a
//! script wrote from the layout's rules apart from the Rust: the frames are what the code
//! is held to, so this test never writes them.

use std::path::{Path, PathBuf};

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::Paint;
use yunta_testkit_ui::screens;
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

/// `state`'s screen drawn on a test backend of `environment`'s size: every row, its
/// trailing blanks cut, one a line.
fn drawn(state: &State, environment: &Environment) -> String {
    let look = Look {
        glyphs: environment.glyphs,
        paint: Paint::Plain,
        width: Width::exact(usize::from(environment.columns.saturating_sub(2))),
        motion: Motion::Still,
    };
    let screen = state.screen(&look);
    let backend = TestBackend::new(environment.columns, environment.rows);
    let mut terminal = Terminal::new(backend).expect("a test backend");
    terminal
        .draw(|frame| {
            yunta_tui::drawn(frame, &screen, &look);
        })
        .expect("a drawing");
    let buffer = terminal.backend().buffer();
    (0..environment.rows)
        .map(|y| {
            let row: String = (0..environment.columns)
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            format!("{}\n", row.trim_end())
        })
        .collect()
}

/// `drawn` against the target `name` in `environment`, both shown whole where they differ.
fn held_to_its_target(name: &str, environment: &Environment, drawn: &str) {
    let path = targets().join(format!("{name}.{}.txt", environment.name));
    let target = std::fs::read_to_string(&path).expect("a target frame");
    assert!(
        drawn == target,
        "{name} at {} differs from its target {}:\n--- drawn\n{drawn}--- target\n{target}",
        environment.name,
        path.display()
    );
}

#[test]
fn every_screen_equals_its_target_frame() {
    yunta_testkit::on_a_deep_stack(|| async {
        let built = [
            ("field", screens::field().await),
            ("run", screens::run().await),
            ("card", screens::card().await),
            ("desk", screens::desk().await),
            ("confirmation", screens::confirmation().await),
            ("palette", screens::palette().await),
            ("help", screens::help().await),
        ];
        for (name, state) in &built {
            for environment in [&ASCII_80, &UNICODE_60] {
                held_to_its_target(name, environment, &drawn(state, environment));
            }
        }
        let (_, field) = &built[0];
        held_to_its_target("room-needed", &UNICODE_59, &drawn(field, &UNICODE_59));
    });
}
