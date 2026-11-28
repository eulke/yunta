//! The walk, held whole: every state `yunta_testkit_ui`'s walk reaches, reached once and
//! held to the interface's rules and to a terminal's layout alike. The interface's are the
//! test kit's ([`InterfaceChecks`]): no work that stops, closes, removes or spends is asked
//! but on its confirmation, every screen draws on Markdown and HTML, a trail that names a run
//! names the one the state follows, every row of a palette and every help hold as every
//! input does, a remark asks nothing, and every screen, page and command the walk must meet
//! is met. The layout's are stated here.
//!
//! Every state is drawn with [`yunta_tui::drawn`] on Ratatui's test backend at the design's
//! own size, 120×36, in ASCII, where a side region stands beside the main one, and at the
//! least, 60×14, in Unicode, where it gives way, and holds the layout's rules. 80×24 is held
//! by each scene's goldens. No drawing panics; in ASCII every cell holds ASCII, the words a
//! session or a pack recorded among them, and each rule reads the screen's texts in their
//! ASCII form, as the terminal draws them; nothing is written in a margin
//! but the rule; the side region's edge stands whole on every row it spans; the legend is
//! on the last row, the box's where a box is open, with the note right above it where one
//! stands and the attention otherwise, and what the run spent on the row over that where
//! the screen says it, a box open or not; a box stands between the rule and the meter's
//! row, its corners the round ones, and every cell inside it the box's own, as it draws over
//! nothing, with no glyph, tone or ground of what stood under it; a list held where the
//! person is shows the cursor if and only if the row it is narrowed on stands in sight, on
//! that row where typing lands, read off the frame; anywhere else the cursor shows wherever
//! a line is typed, only where a prompt is held, and inside what is laid out. A state that
//! breaks one is named with the inputs that reach it, and its frame.
//!
//! Every state is drawn on 24 bits, where a pane paints grounds of its own, so a ground kept
//! from under a box shows; the other rules read the frame's glyphs alone.
//!
//! Each distinct screen is drawn once on each terminal, as the walk keeps each state once:
//! a drawing is a function of the screen alone, and states the walk tells apart by the
//! work they wait on often show the same one. A screen is told apart by everything its
//! regions say, as Markdown draws them whole, where each is scrolled to, and whether a
//! field is open.
//!
//! The layout's rules are stated here apart from the crate's `layout`, as the target frames
//! state them: what each row holds is Surco's, where it stands is checked.
//!
//! Where a person names a file for the walk's table ([`classes::TABLE`]), its totals and its
//! states by class are written there.

mod covered;
#[path = "walk/cursor.rs"]
mod cursor;

use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::RangeInclusive;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Instant;

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Position;
use ratatui::Terminal as Drawing;
use surco::components::{Legend, Overlay, Screen, Side, SideAt};
use surco::diagram::{Shape, Stroke};
use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::surface::Terminal;
use surco::width::cell_width;
use yunta_testkit_ui::home::Home;
use yunta_testkit_ui::recorded::Recordings;
use yunta_testkit_ui::walk::interface::InterfaceChecks;
use yunta_testkit_ui::walk::{classes, Checks, Reached, Walk};
use yunta_ui::{Input, Request, State};

/// The cells a margin takes at each side of the terminal, but on the rule's row.
const MARGIN: u16 = 1;

/// The fewest columns a side region stands on, at either edge, as the target frames draw
/// it: a screen 100 cells wide inside its margins. Under them it gives way.
const SIDE_FROM: u16 = 102;

/// The cells a side region at the left takes inside the left margin, as the target frames
/// draw a run's nodes.
const SIDE_AT_LEFT: u16 = 31;

/// The cells a side region at the right takes inside the right margin, as the target
/// frames draw what is read beside the field's runs.
const SIDE_AT_RIGHT: u16 = 39;

/// The rows every screen keeps whatever it holds: the trail, the rule, the row over the
/// legend and the legend.
const ALWAYS: u16 = 4;

/// The signs of a box's title its top edge is found by: as many as the narrowest box holds
/// whole of any title, so a title cut at the box's edge is still found.
const TITLE_SEEN: usize = 8;

/// The row the rule stands on, under the trail: the one row that takes every cell.
const RULE_ROW: u16 = 1;

/// The fewest rows the regions keep before what the screen is about gives way, as the
/// target frames draw a run's pane at 60×14: its title, the row that says what its node
/// works on, its feed's heading and the three rows a feed's window keeps at the least.
const LEAST_REGIONS: u16 = 6;

/// A terminal the walk's states are drawn on, which holds the layout's rules over the walk:
/// its name, its glyphs, what draws on it, what draws a box open over a screen over nothing
/// beside it, and the screens drawn on it, each by its fingerprint.
struct OnATerminal {
    name: &'static str,
    glyphs: Glyphs,
    drawing: Drawing<TestBackend>,
    alone: Drawing<TestBackend>,
    drawn: HashSet<u64>,
}

impl OnATerminal {
    fn new(name: &'static str, glyphs: Glyphs, columns: u16, rows: u16) -> Self {
        let backend = || Drawing::new(TestBackend::new(columns, rows)).expect("a test backend");
        OnATerminal {
            name,
            glyphs,
            drawing: backend(),
            alone: backend(),
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
            paint: covered::PAINTED,
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
        if let Some(failed) = drawn_on(&mut self.drawing, &screen, &look) {
            named(&failed, String::new());
        }
        // The rules read the screen as the terminal draws it: in ASCII, each text in its form.
        let screen = match look.glyphs {
            Glyphs::Ascii => screen.in_ascii(),
            Glyphs::Unicode => screen,
        };
        let backend = self.drawing.backend();
        let cursor = backend.cursor_visible().then(|| backend.cursor_position());
        if let Some(rule) = broken(&screen, &look, backend.buffer(), cursor, state.typing()) {
            named(&rule, framed(backend.buffer()));
        }
        let Some(overlay) = screen.overlay else {
            return;
        };
        let alone = covered::alone(overlay);
        if let Some(failed) = drawn_on(&mut self.alone, &alone, &look) {
            named(&format!("the box over nothing: {failed}"), String::new());
        }
        let over = self.drawing.backend().buffer();
        if let Some(rule) = covered::kept_under(over, self.alone.backend().buffer(), &look) {
            named(&rule, framed(over));
        }
    }
}

/// `screen` drawn in `look` by `drawing`; how the drawing failed, where it failed or
/// panicked.
fn drawn_on(drawing: &mut Drawing<TestBackend>, screen: &Screen, look: &Look) -> Option<String> {
    let drew = catch_unwind(AssertUnwindSafe(|| {
        drawing
            .draw(|frame| {
                yunta_tui::drawn(frame, screen, look);
            })
            .map(|_| ())
    }));
    match drew {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(format!("the drawing failed: {error}")),
        Err(_) => Some("the drawing panicked".to_string()),
    }
}

impl Checks for OnATerminal {
    fn reached(&mut self, state: &State, markdown: &str, reached: &Reached<'_>) {
        self.laid_out(state, markdown, reached);
    }

    fn handed(&mut self, _from: &State, _input: &Input, _asked: &[Request], _: &Reached<'_>) {}
}

/// On a deep stack: the recordings run the repository's cases, each driving a run whole
/// inside the test, which a debug build's futures make deeper than a test thread's stack.
/// The interface's rules and each terminal's layout hold the walk's states each on a thread
/// of its own.
#[test]
fn every_state_the_application_reaches_holds_and_lays_out_at_both_sizes() {
    yunta_testkit::on_a_deep_stack(|| async {
        let home = Home::built().await;
        let recordings = Recordings::made(&home).await;
        let started = Instant::now();
        let mut interface = InterfaceChecks::new(&recordings, home.runs());
        let mut terminals = [
            OnATerminal::new("ascii-120", Glyphs::Ascii, 120, 36),
            OnATerminal::new("unicode-60", Glyphs::Unicode, 60, 14),
        ];
        let [ascii_120, unicode_60] = &mut terminals;
        let mut checks: [&mut dyn Checks; 3] = [&mut interface, ascii_120, unicode_60];
        let walked = Walk::over(&recordings).walked(&mut checks);
        let states = walked.states();
        let drawn: Vec<String> = terminals
            .iter()
            .map(|terminal| format!("{} screens at {}", terminal.drawn.len(), terminal.name))
            .collect();
        eprintln!(
            "the walk reached {states} states, took {} palettes row by row, opened {} helps \
             and drew {}, in {:?}",
            interface.palettes_taken(),
            interface.helps_opened(),
            drawn.join(" and "),
            started.elapsed()
        );
        if let Some(table) = classes::asked() {
            let mut screens: Vec<usize> = terminals.iter().map(|on| on.drawn.len()).collect();
            screens.dedup();
            let screens: Vec<String> = screens.iter().map(usize::to_string).collect();
            let totals = format!(
                "states {states}  palettes {}  helps {}  screens {}",
                interface.palettes_taken(),
                interface.helps_opened(),
                screens.join("/")
            );
            std::fs::write(&table, format!("{totals}\n{}", walked.by_class()))
                .unwrap_or_else(|error| panic!("the walk's table at {table:?}: {error}"));
        }
        interface.met_all_it_must();
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
    if look.glyphs == Glyphs::Ascii {
        let mut cells = (0..height).flat_map(|y| (0..width).map(move |x| (x, y)));
        if let Some((x, y)) = cells.find(|at| !frame[*at].symbol().is_ascii()) {
            let symbol = frame[(x, y)].symbol();
            return Some(format!("row {} holds `{symbol}`, outside ASCII", y + 1));
        }
    }
    for y in (0..height).filter(|y| *y != RULE_ROW) {
        if frame[(0, y)].symbol() != " " || frame[(width - 1, y)].symbol() != " " {
            return Some(format!("row {} writes in a margin", y + 1));
        }
    }
    if !said_on(frame, height - 1, &keys_drawn(screen, &inner)) {
        return Some("the legend is not on the last row".to_string());
    }
    let side = screen.side.as_ref().filter(|_| width >= SIDE_FROM);
    let about = screen
        .about
        .as_ref()
        .map_or(0, |about| inner.across(about, usize::MAX).len());
    let over_the_legend = match (&screen.noted, &screen.attention) {
        (Some(noted), _) => (inner.noted(noted).text(), "the note"),
        (None, Some(attention)) => (inner.attention(attention).text(), "the attention"),
        (None, None) => (String::new(), "a blank row"),
    };
    let (said, which) = over_the_legend;
    if !said_on(frame, height - 2, &said) {
        return Some(format!("{which} is not right above the legend"));
    }
    let meter = u16::from(screen.meter.is_some());
    let across = across_kept(height - meter, about);
    // Short of rows, the blank row under what stands across gave way.
    let short_of_rows = about > 0 && usize::from(across) <= about;
    if let Some(rule) = meter_broken(screen, &inner, frame, short_of_rows) {
        return Some(rule);
    }
    let spans = 2 + across..=height - 3 - meter;
    side.filter(|_| screen.overlay.is_none())
        .and_then(|side| edge_broken(side, look, frame, spans))
        .or_else(|| {
            screen
                .overlay
                .as_ref()
                .and_then(|overlay| box_broken(overlay, look, frame))
        })
        .or_else(|| cursor::broken(screen, look, frame, cursor, typing))
}

/// Where what the run of `screen` spent breaks on `frame`, drawn at `inner`'s width, a box
/// standing over the screen or not: on the row over the one over the legend, in figures
/// alone where the screen stands `short_of_rows`, its bars kept where they fit anywhere else.
fn meter_broken(
    screen: &Screen,
    inner: &Terminal,
    frame: &Buffer,
    short_of_rows: bool,
) -> Option<String> {
    let spent = screen.meter.as_ref()?;
    let said = match short_of_rows {
        true => inner.meter_figures(spent),
        false => inner.meter(spent),
    };
    let row = frame.area.height - 3;
    (!said_on(frame, row, &said.text()))
        .then(|| "what the run spent is not over the row over the legend".to_string())
}

/// Where the box `overlay` is drawn in on `frame` breaks: its top edge, a round corner then
/// its title, stands under the rule; the uprights under that corner run down to the round
/// corner of its bottom edge, which stands over the meter's row.
fn box_broken(overlay: &Overlay, look: &Look, frame: &Buffer) -> Option<String> {
    let [top_left, _, bottom_left, _] = look.glyphs.corners(Shape::Round);
    let (width, height) = (frame.area.width, frame.area.height);
    let opening: Vec<String> = std::iter::once(top_left.to_string())
        .chain(std::iter::once(" ".to_string()))
        .chain(overlay.title.chars().take(TITLE_SEEN).map(String::from))
        .collect();
    let opens_at = |x: u16, y: u16| {
        opening
            .iter()
            .enumerate()
            .all(|(at, sign)| frame[(x + at as u16, y)].symbol() == sign)
    };
    let Some((x, top)) = (0..height)
        .flat_map(|y| (0..width.saturating_sub(opening.len() as u16)).map(move |x| (x, y)))
        .find(|(x, y)| opens_at(*x, *y))
    else {
        return Some("no box's top edge opens with its round corner and its title".to_string());
    };
    if top <= RULE_ROW {
        return Some(format!(
            "the box's top edge, row {}, is not under the rule",
            top + 1
        ));
    }
    let upright = look
        .glyphs
        .joint(true, true, false, false, Stroke::Open)
        .to_string();
    let bottom = (top + 1..height).find(|y| frame[(x, *y)].symbol() != upright);
    match bottom {
        Some(y) if frame[(x, y)].symbol() == bottom_left.to_string() && y < height - 3 => None,
        Some(y) => Some(format!(
            "the box's left side breaks on row {}, on the rows under the regions or with no \
             round corner",
            y + 1
        )),
        None => Some("the box has no bottom edge".to_string()),
    }
}

/// Where the edge of `side`, drawn in `look`, breaks on `frame`: the first of the rows it
/// `spans` that does not hold it whole. A side region whose words keep a measure stands that
/// wide, as a new run's panel does.
fn edge_broken(
    side: &Side,
    look: &Look,
    frame: &Buffer,
    spans: RangeInclusive<u16>,
) -> Option<String> {
    let edge = Terminal::on(*look).edge().text();
    let cells = u16::try_from(cell_width(&edge)).expect("an edge's cells");
    let measure = side
        .region
        .measure
        .map(|measure| u16::try_from(measure.cells).expect("a measure"));
    let from = match side.at {
        SideAt::Left => MARGIN + measure.unwrap_or(SIDE_AT_LEFT),
        SideAt::Right => frame.area.width - MARGIN - measure.unwrap_or(SIDE_AT_RIGHT) - cells,
    };
    let edge_at = |y: u16| {
        (from..from + cells)
            .map(|x| frame[(x, y)].symbol())
            .collect::<String>()
    };
    spans
        .into_iter()
        .find(|y| edge_at(*y) != edge)
        .map(|y| format!("row {} breaks the side region's edge", y + 1))
}

/// The rows what the screen is about keeps, with the blank row under it while that stands,
/// on a terminal `height` rows high less the meter's: the blank row gives way first, then
/// what it is about from its whole, a row at a time, while the regions would get fewer than
/// [`LEAST_REGIONS`].
fn across_kept(height: u16, about: usize) -> u16 {
    let room = height.saturating_sub(ALWAYS + LEAST_REGIONS);
    let about = u16::try_from(about).unwrap_or(u16::MAX);
    match about {
        0 => 0,
        about if about < room => about + 1,
        about => about.min(room),
    }
}

/// Whether row `y` of `frame` says `said` from its margin, as far as either goes.
fn said_on(frame: &Buffer, y: u16, said: &str) -> bool {
    let row: String = (MARGIN..frame.area.width - MARGIN)
        .map(|x| frame[(x, y)].symbol())
        .collect();
    row.trim_end() == said.trim_end()
}

/// The keys `screen`'s last row holds on `inner`: a box's own keys while one stands over it,
/// with no far keys; where a line is typed on it, its keys alone, folded as such a screen folds
/// them; its keys and the far keys otherwise.
fn keys_drawn(screen: &Screen, inner: &Terminal) -> String {
    match (&screen.overlay, screen.typed) {
        (Some(overlay), _) => inner.legend(&overlay.legend, &Legend::new()).text(),
        (None, true) => inner.typed_legend(&screen.legend).text(),
        (None, false) => inner.legend(&screen.legend, &screen.anywhere).text(),
    }
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
