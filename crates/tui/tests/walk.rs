//! The walk laid out: every state `yunta_testkit_ui`'s walk reaches, drawn with
//! [`yunta_tui::drawn`] on Ratatui's test backend at 80×24 in ASCII and at 60×14 in
//! Unicode, holds the reference's layout. No drawing panics; nothing is written in a margin
//! but the rule; the side region's edge stands whole on every row it spans; the legend is
//! on the last row, the box's where a box is open, with the attention right above it; the
//! cursor shows wherever a field is open, only where a prompt is held, and inside what is
//! laid out. A state that breaks one is named with the inputs that reach it, and its frame.
//!
//! Each distinct screen is drawn once on each terminal, as the walk keeps each state once:
//! a drawing is a function of the screen alone, and states the walk tells apart by the
//! work they wait on often show the same one. A screen is told apart by everything its
//! regions say, as Markdown draws them whole, where each is scrolled to, and whether a
//! field is open.
//!
//! The rules are stated here apart from the crate's `layout`, as the target frames state
//! them: what each row holds is Surco's, where it stands is checked.

use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Instant;

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Position;
use ratatui::Terminal as Drawing;
use surco::components::Screen;
use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::Paint;
use surco::surface::Terminal;
use surco::width::cell_width;
use yunta_testkit_ui::home::Home;
use yunta_testkit_ui::recorded::Recordings;
use yunta_testkit_ui::walk::{Checks, Reached, Walk};
use yunta_ui::{Input, Request, State};

/// The cells a margin takes at each side of the terminal, but on the rule's row.
const MARGIN: u16 = 1;

/// The cells the side region takes, beside its edge.
const SIDE: u16 = 22;

/// The fewest cells the main region keeps before the side region gives way.
const LEAST_MAIN: u16 = 40;

/// The rows every screen keeps whatever it holds: the trail, the rule and the legend.
const ALWAYS: u16 = 3;

/// The row the rule stands on, under the trail: the one row that takes every cell.
const RULE_ROW: u16 = 1;

/// The fewest rows a region keeps before what the screen is about gives way: the line a
/// pointer stands on, with one above it and one below.
const LEAST_REGION: u16 = 3;

/// What a terminal's layout holds over the walk: the terminals it draws every state on.
struct LayoutChecks {
    terminals: Vec<OnATerminal>,
}

/// A terminal the walk's states are drawn on: its name, its glyphs, what draws on it, and
/// the screens drawn on it, each by its fingerprint.
struct OnATerminal {
    name: &'static str,
    glyphs: Glyphs,
    drawing: Drawing<TestBackend>,
    drawn: HashSet<u64>,
}

impl OnATerminal {
    fn new(name: &'static str, glyphs: Glyphs, columns: u16, rows: u16) -> Self {
        OnATerminal {
            name,
            glyphs,
            drawing: Drawing::new(TestBackend::new(columns, rows)).expect("a test backend"),
            drawn: HashSet::new(),
        }
    }

    /// `state`'s screen, which Markdown draws as `markdown`, drawn on this terminal where
    /// it was not yet and held to the layout's rules; a state that breaks one is named with
    /// how the walk reached it.
    fn laid_out(&mut self, state: &State, markdown: &str, reached: &Reached<'_>) {
        let area = self.drawing.backend().buffer().area;
        let look = Look {
            glyphs: self.glyphs,
            paint: Paint::Plain,
            width: Width::exact(usize::from(area.width - 2 * MARGIN)),
            motion: Motion::Still,
        };
        let name = self.name;
        let named = |rule: &str, frame: String| -> ! {
            panic!(
                "{rule}, at {name}:\n{frame}\nreached by:\n{}",
                reached.inputs()
            )
        };
        let Ok(screen) = catch_unwind(AssertUnwindSafe(|| state.screen(&look))) else {
            named("its screen panicked", String::new())
        };
        if !self
            .drawn
            .insert(fingerprint(&screen, markdown, state.typing()))
        {
            return;
        }
        let drawing = &mut self.drawing;
        let drew = catch_unwind(AssertUnwindSafe(|| {
            drawing
                .draw(|frame| {
                    yunta_tui::drawn(frame, &screen, &look);
                })
                .map(|_| ())
        }));
        match drew {
            Ok(Ok(())) => {}
            Ok(Err(error)) => named(&format!("the drawing failed: {error}"), String::new()),
            Err(_) => named("the drawing panicked", String::new()),
        }
        let backend = self.drawing.backend();
        let cursor = backend.cursor_visible().then(|| backend.cursor_position());
        if let Some(rule) = broken(&screen, &look, backend.buffer(), cursor, state.typing()) {
            named(&rule, framed(backend.buffer()));
        }
    }
}

impl Checks for LayoutChecks {
    fn reached(&mut self, state: &State, markdown: &str, reached: &Reached<'_>) {
        for terminal in &mut self.terminals {
            terminal.laid_out(state, markdown, reached);
        }
    }

    fn handed(&mut self, _from: &State, _input: &Input, _asked: &[Request], _: &Reached<'_>) {}
}

/// On a deep stack: the recordings run the repository's cases, each driving a run whole
/// inside the test, which a debug build's futures make deeper than a test thread's stack.
#[test]
fn every_state_the_walk_reaches_lays_out_at_both_sizes() {
    yunta_testkit::on_a_deep_stack(|| async {
        let home = Home::built().await;
        let recordings = Recordings::made(&home).await;
        let started = Instant::now();
        let mut checks = LayoutChecks {
            terminals: vec![
                OnATerminal::new("ascii-80", Glyphs::Ascii, 80, 24),
                OnATerminal::new("unicode-60", Glyphs::Unicode, 60, 14),
            ],
        };
        let states = Walk::over(&recordings).walked(&mut checks);
        let drawn: Vec<String> = checks
            .terminals
            .iter()
            .map(|terminal| format!("{} at {}", terminal.drawn.len(), terminal.name))
            .collect();
        eprintln!(
            "the walk reached {states} states and drew {} screens, in {:?}",
            drawn.join(" and "),
            started.elapsed()
        );
    });
}

/// The first of the layout's rules `frame`, drawn of `screen` in `look` with the cursor at
/// `cursor` where it shows, breaks; none where they all hold. `typing` is whether a field
/// is open where the person is.
fn broken(
    screen: &Screen,
    look: &Look,
    frame: &Buffer,
    cursor: Option<Position>,
    typing: bool,
) -> Option<String> {
    let (width, height) = (frame.area.width, frame.area.height);
    let inner = at(look, width - 2 * MARGIN);
    for y in (0..height).filter(|y| *y != RULE_ROW) {
        if frame[(0, y)].symbol() != " " || frame[(width - 1, y)].symbol() != " " {
            return Some(format!("row {} writes in a margin", y + 1));
        }
    }
    let legend = screen
        .overlay
        .as_ref()
        .map_or(&screen.legend, |overlay| &overlay.legend);
    if !said_on(frame, height - 1, &inner.legend(legend).text()) {
        return Some("the legend is not on the last row".to_string());
    }
    let edge = Terminal::on(*look).edge().text();
    let edge_cells = u16::try_from(cell_width(&edge)).expect("an edge's cells");
    let side = screen
        .side
        .as_ref()
        .filter(|_| width - 2 * MARGIN >= SIDE + edge_cells + LEAST_MAIN);
    let titled =
        screen.main.title.is_some() || side.is_some_and(|side| side.region.title.is_some());
    let about = screen
        .about
        .as_ref()
        .map_or(0, |about| inner.lines(about).len());
    let (about_rows, attention_row) = rows_kept(height, about, screen.attention.is_some(), titled);
    if let Some(attention) = screen.attention.as_ref().filter(|_| attention_row) {
        if screen.overlay.is_none()
            && !said_on(frame, height - 2, &inner.attention(attention).text())
        {
            return Some("the attention is not right above the legend".to_string());
        }
    }
    if side.is_some() && screen.overlay.is_none() {
        let top = 2 + about_rows;
        let bottom = height - 2 - u16::from(attention_row);
        let edge_at = |y: u16| {
            (MARGIN + SIDE..MARGIN + SIDE + edge_cells)
                .map(|x| frame[(x, y)].symbol())
                .collect::<String>()
        };
        if let Some(y) = (top..=bottom).find(|y| edge_at(*y) != edge) {
            return Some(format!("row {} breaks the side region's edge", y + 1));
        }
    }
    cursor_broken(screen, cursor, typing, (width, height))
}

/// Where the cursor breaks the layout's rules: hidden while a field is open, shown where
/// no prompt is held, or standing outside what is laid out.
fn cursor_broken(
    screen: &Screen,
    cursor: Option<Position>,
    typing: bool,
    (width, height): (u16, u16),
) -> Option<String> {
    let held = [
        screen.overlay.as_ref().map(|overlay| &overlay.doc),
        screen.side.as_ref().map(|side| &side.region.doc),
        Some(&screen.main.doc),
    ]
    .into_iter()
    .flatten()
    .any(|doc| doc.prompt_at().is_some());
    match cursor {
        None if typing => Some("a field is open and no cursor shows".to_string()),
        Some(_) if !held => Some("the cursor shows where no prompt is held".to_string()),
        Some(Position { x, y })
            if x < MARGIN || x >= width - MARGIN || y <= 1 || y >= height - 1 =>
        {
            Some(format!(
                "the cursor stands outside what is laid out, at {x},{y}"
            ))
        }
        _ => None,
    }
}

/// The rows what the screen is about keeps, with the blank under it, and whether the
/// attention keeps its row, on a terminal `height` rows high: what it is about gives way
/// from its last row while the regions would get fewer than [`LEAST_REGION`], then the
/// attention.
fn rows_kept(height: u16, about: usize, attention: bool, titled: bool) -> (u16, bool) {
    let block = |about: u16| if about > 0 { about + 1 } else { 0 };
    let fixed = ALWAYS + u16::from(attention) + u16::from(titled);
    let room = height.saturating_sub(fixed + LEAST_REGION);
    let mut about = u16::try_from(about).unwrap_or(u16::MAX);
    while about > 0 && block(about) > room {
        about -= 1;
    }
    let regions = height.saturating_sub(fixed + block(about));
    (block(about), attention && regions >= LEAST_REGION)
}

/// Whether row `y` of `frame` says `said` from its margin, as far as either goes.
fn said_on(frame: &Buffer, y: u16, said: &str) -> bool {
    let row: String = (MARGIN..frame.area.width - MARGIN)
        .map(|x| frame[(x, y)].symbol())
        .collect();
    row.trim_end() == said.trim_end()
}

/// Surco's terminal in `look`, its lines exactly `cells` wide.
fn at(look: &Look, cells: u16) -> Terminal {
    Terminal::on(Look {
        width: Width::exact(usize::from(cells)),
        ..*look
    })
}

/// What tells `screen`, which Markdown draws as `markdown`, apart from another: what its
/// regions say, where each is scrolled to, and whether a field is open where the person
/// is, which is where the cursor shows.
fn fingerprint(screen: &Screen, markdown: &str, typing: bool) -> u64 {
    let sights = [
        Some(screen.main.sight),
        screen.side.as_ref().map(|side| side.region.sight),
        screen.overlay.as_ref().map(|overlay| overlay.sight),
    ]
    .map(|sight| sight.map(|sight| (sight.top, sight.follows_pointer)));
    let mut fingerprint = DefaultHasher::new();
    (markdown, sights, typing).hash(&mut fingerprint);
    fingerprint.finish()
}

/// `frame` as a person reads it, each row framed so a margin's blank shows.
fn framed(frame: &Buffer) -> String {
    (0..frame.area.height)
        .map(|y| {
            let row: String = (0..frame.area.width)
                .map(|x| frame[(x, y)].symbol())
                .collect();
            format!("|{row}|\n")
        })
        .collect()
}
