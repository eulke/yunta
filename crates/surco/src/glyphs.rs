//! The characters a terminal surface draws with; `policy` decides which of the two sets an
//! environment allows, and `ascii` what any other text becomes on a full-screen terminal
//! drawn in ASCII.

use crate::diagram::{Shape, Stroke};

mod ascii;
mod policy;

pub(crate) use ascii::InAscii;
pub use policy::{parse_choice, GlyphEnv, OVERRIDE_VAR};

/// What a line is marked with, before its word: a glyph the eye finds
/// first, and the color a terminal that draws one paints it.
///
/// Each mark means one thing. `NeedsYou` is for a person and nothing
/// else — a reader who learns that mark learns where they are wanted —
/// so a reroute, a caution and a stop have marks of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Done,
    Failed,
    /// Moving on its own, or waiting on a clock.
    Running,
    /// A person owes an answer.
    NeedsYou,
    /// Worth a reader's attention, and nothing anybody has to answer: a
    /// run nobody drives, findings that still block, a red baseline.
    Caution,
    /// Control went somewhere else: a reroute, a promotion.
    Reroute,
    /// A person stopped it, and nothing is wrong with it.
    Stopped,
    /// Left out by the run's own choice: its mode.
    Skipped,
    /// Nothing stands here yet: a node that never ran, a check that does
    /// not apply, what a plan leaves out.
    Absent,
}

/// Every mark: what a test holds all of at once, and what the ASCII form of a text reads each
/// mark's two glyphs from.
pub(crate) const ALL_MARKS: [Mark; 9] = [
    Mark::Done,
    Mark::Failed,
    Mark::Running,
    Mark::NeedsYou,
    Mark::Caution,
    Mark::Reroute,
    Mark::Stopped,
    Mark::Skipped,
    Mark::Absent,
];

/// What a path under the reader's home directory starts with, in both
/// sets: the way a shell writes it.
pub const HOME: char = '~';

/// The mark an escape is written as where a reader reads it, so a golden or
/// a failure shows a change of color as a change of text.
pub const ESCAPE: char = '␛';

/// How far the plow has come along one node's stretch of a run's furrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plow {
    /// The node has stood somewhere: it ran, finished, failed or waits.
    Plowed,
    /// The node never ran: the stretch is still ahead.
    Ahead,
    /// The run leaves the node out.
    Skipped,
}

/// The set of decorative characters a surface draws with.
///
/// Nothing a reader needs rides on a glyph. Every line reads with the
/// glyphs stripped, because the word beside one already says what it
/// says — a `✓` sits next to `done`, never instead of it. That rule is
/// what makes honoring `NO_COLOR` free as well: color, like a glyph,
/// only repeats what the word carries, so dropping it costs a line
/// nothing.
///
/// Which set is safe is a property of the environment the process was
/// given, never of the terminal at the other end: a terminal cannot be
/// asked whether it draws box characters. There is no query for it, and
/// an answer would arrive as characters the user appears to have typed.
/// So [`Glyphs::select`] reads what the environment *names* and gives
/// the reader [`OVERRIDE_VAR`] to say otherwise — a capability probe is
/// not an option that was passed over, it is one that does not exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyphs {
    /// Block elements, geometric shapes and dingbats, each one display
    /// cell wide.
    Unicode,
    /// ASCII alone, for an environment that names no UTF-8 locale or a
    /// terminal that declares itself dumb. Every character is one cell,
    /// so a line keeps the width it has under [`Glyphs::Unicode`]. A screen
    /// a full-screen terminal draws in these glyphs holds ASCII alone: every
    /// text it is handed takes its ASCII form too
    /// ([`Screen::in_ascii`](crate::components::Screen::in_ascii)), where a
    /// page keeps every character it holds.
    Ascii,
}

impl Glyphs {
    /// The character an empty step of a bar is drawn with — always
    /// something visible, so a bar at zero is still a bar and not a gap
    /// the eye reads as a missing row.
    pub fn bar_empty(self) -> char {
        match self {
            Self::Unicode => '░',
            Self::Ascii => '_',
        }
    }

    /// The mark of what a line leaves out: the end of a value cut to fit its
    /// column, or a value a reader fills in. One cell in Unicode and three in
    /// ASCII, where a single dot is a mark of its own; the arithmetic that
    /// places a cut measures it.
    pub fn ellipsis(self) -> &'static str {
        match self {
            Self::Unicode => "…",
            Self::Ascii => "...",
        }
    }

    /// The sign a multiple is read with: `120×`.
    pub fn times(self) -> char {
        match self {
            Self::Unicode => '×',
            Self::Ascii => 'x',
        }
    }

    /// What stands between two facts on one line: `a · b`.
    pub fn sep(self) -> char {
        match self {
            Self::Unicode => '·',
            Self::Ascii => ';',
        }
    }

    /// What stands between two facts on one line with the space at each side of it, as a
    /// row joins them: ` · `. Every such separator is this one.
    pub fn spaced_sep(self) -> String {
        format!(" {} ", self.sep())
    }

    /// What points from one thing to the one after it.
    pub fn arrow(self) -> &'static str {
        match self {
            Self::Unicode => "→",
            Self::Ascii => "->",
        }
    }

    /// What points at the one option of a list a key would take.
    pub fn pointer(self) -> char {
        match self {
            Self::Unicode => '▸',
            Self::Ascii => '*',
        }
    }

    /// What marks a row a person picked for an act that takes several: a mark no standing
    /// takes, since it stands beside the run's own (`■` is a stopped run's).
    pub fn picked(self) -> char {
        match self {
            Self::Unicode => '◉',
            Self::Ascii => '+',
        }
    }

    /// What says the keys that move up and down a list: `↑↓ move`.
    pub fn up_down(self) -> &'static str {
        match self {
            Self::Unicode => "↑↓",
            Self::Ascii => "up/down",
        }
    }

    /// What says the keys that change which word of a row is chosen: `←→ mode`.
    pub fn left_right(self) -> &'static str {
        match self {
            Self::Unicode => "←→",
            Self::Ascii => "left/right",
        }
    }

    /// What stands between a file and what in it a reader is shown, and
    /// between a thing and what is inside it: `src/greet.rs › Greeter`.
    pub fn within(self) -> &'static str {
        match self {
            Self::Unicode => "›",
            Self::Ascii => "::",
        }
    }

    /// What joins one thing to another without pointing either way.
    pub fn joins(self) -> &'static str {
        match self {
            Self::Unicode => "—",
            Self::Ascii => "--",
        }
    }

    /// What the rest of a line cut to fit opens with, so it reads as the
    /// line above going on rather than a line of its own.
    pub fn continued(self) -> char {
        match self {
            Self::Unicode => '↪',
            Self::Ascii => '\\',
        }
    }

    /// What points at the file and the line a quoted line is from, the
    /// way a compiler points at them. The quote is a family of its own,
    /// as a diagram is: its signs mean what they mean beside the line.
    pub fn points_at(self) -> &'static str {
        match self {
            Self::Unicode | Self::Ascii => "-->",
        }
    }

    /// What marks the part of a quoted line at fault, under it.
    pub fn fault(self) -> char {
        match self {
            Self::Unicode | Self::Ascii => '^',
        }
    }

    /// What opens and closes the words a change puts into a line of code:
    /// `{+name: &str+}`. These are git's word-diff marks, a family of their
    /// own as a quote's are: they mean what they mean inside a line of code.
    pub fn added(self) -> [&'static str; 2] {
        match self {
            Self::Unicode | Self::Ascii => ["{+", "+}"],
        }
    }

    /// What opens and closes the words a change takes out of a line of
    /// code, where they stood: `[-name: &str-]`. git's word-diff marks too.
    pub fn removed(self) -> [&'static str; 2] {
        match self {
            Self::Unicode | Self::Ascii => ["[-", "-]"],
        }
    }

    /// The signs before the lines a change adds and the lines it takes out:
    /// `+412 −87`. A family of their own, as the word diff's marks are.
    pub fn changed(self) -> [char; 2] {
        match self {
            Self::Unicode => ['+', '−'],
            Self::Ascii => ['+', '-'],
        }
    }

    /// The edge quoted output hangs from, so it reads as something a
    /// command printed and not as something this one says.
    pub fn gutter(self) -> char {
        match self {
            Self::Unicode => '│',
            Self::Ascii => '|',
        }
    }

    /// The edge down the left of the one decision a run waits on, drawn in
    /// every layer: the tint behind the decision shows only in 24 bits. It
    /// shares its ASCII form with the gutter, which is an edge too.
    pub fn decision_edge(self) -> char {
        match self {
            Self::Unicode => '▎',
            Self::Ascii => '|',
        }
    }

    /// The joint the lines through a cell make, by the ways they run —
    /// a straight run drawn in its link's `stroke`, a corner or a
    /// junction where lines turn or meet.
    pub fn joint(self, up: bool, down: bool, left: bool, right: bool, stroke: Stroke) -> char {
        let upright = (up || down) && !left && !right;
        let level = (left || right) && !up && !down;
        match (self, upright, level, stroke) {
            (Self::Unicode, true, _, Stroke::Dotted) => '┆',
            (Self::Unicode, true, _, Stroke::Thick) => '┃',
            (Self::Unicode, true, _, _) => '│',
            (Self::Unicode, _, true, Stroke::Dotted) => '┄',
            (Self::Unicode, _, true, Stroke::Thick) => '━',
            (Self::Unicode, _, true, _) => '─',
            (Self::Ascii, true, _, Stroke::Dotted) => ':',
            (Self::Ascii, true, _, _) => '|',
            (Self::Ascii, _, true, Stroke::Dotted) => '.',
            (Self::Ascii, _, true, Stroke::Thick) => '=',
            (Self::Ascii, _, true, _) => '-',
            (Self::Ascii, false, false, _) => '+',
            (Self::Unicode, false, false, _) => match (up, down, left, right) {
                (false, true, false, true) => '┌',
                (false, true, true, false) => '┐',
                (true, false, false, true) => '└',
                (true, false, true, false) => '┘',
                (true, true, false, true) => '├',
                (true, true, true, false) => '┤',
                (false, true, true, true) => '┬',
                (true, false, true, true) => '┴',
                _ => '┼',
            },
        }
    }

    /// A box's corners — top left, top right, bottom left, bottom right
    /// — by what the box is.
    pub fn corners(self, shape: Shape) -> [char; 4] {
        match (self, shape) {
            (Self::Unicode, Shape::Box) => ['┌', '┐', '└', '┘'],
            (Self::Unicode, Shape::Round) => ['╭', '╮', '╰', '╯'],
            (Self::Unicode, Shape::Decision) => ['╱', '╲', '╲', '╱'],
            (Self::Ascii, Shape::Box) => ['+', '+', '+', '+'],
            (Self::Ascii, Shape::Round) => ['.', '.', '\'', '\''],
            (Self::Ascii, Shape::Decision) => ['/', '\\', '\\', '/'],
        }
    }

    /// The head of a link arriving down onto a box, or across into it.
    pub fn head(self, down: bool) -> char {
        match (self, down) {
            (Self::Unicode, true) => '▼',
            (Self::Unicode, false) => '▶',
            (Self::Ascii, true) => 'v',
            (Self::Ascii, false) => '>',
        }
    }

    /// What says a node sends its failure to another node, which runs it
    /// again: `lint ↺ fix-lint`. One cell in Unicode and two in ASCII; the
    /// arithmetic that lays out a row measures it.
    pub fn returns(self) -> &'static str {
        match self {
            Self::Unicode => "↺",
            Self::Ascii => "<>",
        }
    }

    /// What hangs a child of a group from the group above it in a list of
    /// nodes: every child but the last, and the `last`.
    pub fn branch(self, last: bool) -> &'static str {
        match (self, last) {
            (Self::Unicode, false) => "├─",
            (Self::Unicode, true) => "└─",
            (Self::Ascii, false) => "+-",
            (Self::Ascii, true) => "`-",
        }
    }

    /// A node's stretch of a run's furrow: plowed where the run reached the
    /// node, whatever it came to; ahead where the node never ran; skipped
    /// where the run leaves it out. In ASCII the last two are the marks of
    /// what never ran and of what a run leaves out.
    pub fn furrow(self, plow: Plow) -> char {
        match (self, plow) {
            (Self::Unicode, Plow::Plowed) => '━',
            (Self::Unicode, Plow::Ahead) => '┄',
            (Self::Unicode, Plow::Skipped) => '╌',
            (Self::Ascii, Plow::Plowed) => '=',
            (Self::Ascii, Plow::Ahead) => '.',
            (Self::Ascii, Plow::Skipped) => 'o',
        }
    }

    /// The eight steps a sparkline climbs, lightest first.
    pub fn ramp(self) -> &'static [char; 8] {
        match self {
            Self::Unicode => &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'],
            Self::Ascii => &['_', '.', ':', '-', '=', '+', '*', '#'],
        }
    }

    /// The steps a fine bar is drawn in, the smallest first and a whole cell
    /// last: a bar a value long is whole cells and the step of the cell it
    /// ends in. ASCII has no step finer than a cell, so a bar there is whole
    /// cells alone.
    pub fn fine_steps(self) -> &'static [char] {
        match self {
            Self::Unicode => &['▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'],
            Self::Ascii => &['#'],
        }
    }

    /// The cell that draws `lit`, dots two across and four down, each lit where `lit` has the
    /// bit `across + 2 * down`: a picture finer than a character, in Unicode's braille cells.
    /// ASCII has no cell finer than a character, so a picture of dots stands nowhere there.
    pub fn dots(self, lit: u8) -> Option<char> {
        /// Each dot's bit in a braille cell, in the order `lit` counts them: a braille cell
        /// counts down its left column's first three, then its right's, then the two at its foot.
        const BRAILLE_BITS: [u32; 8] = [0x01, 0x08, 0x02, 0x10, 0x04, 0x20, 0x40, 0x80];
        /// The braille cell with no dot lit, the first of the 256: each dot lit adds its bit.
        const NO_DOT: u32 = 0x2800;
        let lit_at = (0..8).filter(|at| lit & (1 << at) != 0);
        match self {
            Self::Unicode => {
                char::from_u32(lit_at.fold(NO_DOT, |cell, at| cell | BRAILLE_BITS[at]))
            }
            Self::Ascii => None,
        }
    }

    /// The name in block letters, its two rows, each as wide as the other: half a letter's
    /// height to a cell, so `▀` and `▄` draw a stroke's top and foot. ASCII has no block
    /// elements, so there the name stands in its own letters on the first row and the second
    /// is blank: what stands under it keeps its row in either set.
    pub fn wordmark(self) -> [&'static str; 2] {
        match self {
            Self::Unicode => ["█ █ █ █ █▄ █ ▀█▀ ▄▀█", " █  █▄█ █ ▀█  █  █▀█"],
            Self::Ascii => ["yunta", ""],
        }
    }

    /// The frames of a spin, in the order it turns.
    pub fn spin(self) -> &'static [&'static str] {
        match self {
            Self::Unicode => &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
            Self::Ascii => &["|", "/", "-", "\\"],
        }
    }

    /// What marks a bar cut short, drawn over its track: the bar would run
    /// past its scale, and a note says how far.
    pub fn cut(self) -> &'static str {
        match self {
            Self::Unicode => "╱╱",
            Self::Ascii => "//",
        }
    }

    /// The glyph a mark is drawn with. It repeats the word beside it for
    /// the eye; the word is what says it.
    pub fn mark(self, mark: Mark) -> char {
        match (self, mark) {
            (Self::Unicode, Mark::Done) => '✓',
            (Self::Unicode, Mark::Failed) => '✗',
            (Self::Unicode, Mark::Running) => '●',
            (Self::Unicode, Mark::NeedsYou) => '◆',
            (Self::Unicode, Mark::Caution) => '▲',
            (Self::Unicode, Mark::Reroute) => '↻',
            (Self::Unicode, Mark::Stopped) => '■',
            (Self::Unicode, Mark::Skipped) => '○',
            (Self::Unicode, Mark::Absent) => '◌',
            (Self::Ascii, Mark::Done) => '+',
            (Self::Ascii, Mark::Failed) => '!',
            (Self::Ascii, Mark::Running) => '>',
            (Self::Ascii, Mark::NeedsYou) => '?',
            (Self::Ascii, Mark::Caution) => '^',
            (Self::Ascii, Mark::Reroute) => '@',
            (Self::Ascii, Mark::Stopped) => '#',
            (Self::Ascii, Mark::Skipped) => 'o',
            (Self::Ascii, Mark::Absent) => '.',
        }
    }
}

#[cfg(test)]
mod tests;
