//! The characters a terminal surface draws with, and the policy that
//! decides which of the two sets an environment allows.

use super::diagram::{Shape, Stroke};
use super::state::Mark;

/// The environment variable that decides the set outright: `unicode` or
/// `ascii`, case and surrounding space ignored.
pub const OVERRIDE_VAR: &str = "YUNTA_GLYPHS";

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
    /// so a line keeps the width it has under [`Glyphs::Unicode`].
    Ascii,
}

/// The three values the glyph policy reads, lifted out of the process so
/// the policy itself is a function of its arguments and nothing else.
pub struct GlyphEnv {
    /// [`OVERRIDE_VAR`], as the environment carries it.
    pub explicit: Option<String>,
    /// The locale in force: `LC_ALL`, else `LC_CTYPE`, else `LANG` — the
    /// order POSIX resolves character-type behavior in.
    pub locale: Option<String>,
    /// `TERM`.
    pub term: Option<String>,
}

impl Glyphs {
    /// The set `env` allows.
    ///
    /// Three signals in order: an explicit [`OVERRIDE_VAR`] settles it;
    /// otherwise a locale that does not name UTF-8 means the encoding
    /// itself cannot carry the characters, so ASCII; otherwise `TERM=dumb`
    /// is a terminal that has said it draws nothing beyond text, so ASCII
    /// again. Anything else gets the Unicode set.
    ///
    /// A value of [`OVERRIDE_VAR`] that names neither set is not a
    /// choice, and the remaining two signals decide as if it were unset.
    pub fn select(env: &GlyphEnv) -> Self {
        if let Some(choice) = env.explicit.as_deref().and_then(parse_choice) {
            return choice;
        }
        if !names_utf8(env.locale.as_deref()) {
            return Self::Ascii;
        }
        if env.term.as_deref() == Some("dumb") {
            return Self::Ascii;
        }
        Self::Unicode
    }

    /// The character a full step of a bar is drawn with.
    pub fn bar_filled(self) -> char {
        match self {
            Self::Unicode => '█',
            Self::Ascii => '#',
        }
    }

    /// The character an empty step of a bar is drawn with — always
    /// something visible, so a bar at zero is still a bar and not a gap
    /// the eye reads as a missing row.
    pub fn bar_empty(self) -> char {
        match self {
            Self::Unicode => '·',
            Self::Ascii => '.',
        }
    }

    /// The mark that closes a value cut to fit its column. One cell in
    /// either set, so the arithmetic that places the cut is the same.
    pub fn ellipsis(self) -> char {
        match self {
            Self::Unicode => '…',
            Self::Ascii => '~',
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
            Self::Ascii => '|',
        }
    }

    /// What points from one thing to the one after it.
    pub fn arrow(self) -> &'static str {
        match self {
            Self::Unicode => "→",
            Self::Ascii => "->",
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
            Self::Ascii => '>',
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

    /// The eight steps a sparkline climbs, lightest first.
    pub fn ramp(self) -> &'static [char; 8] {
        match self {
            Self::Unicode => &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'],
            Self::Ascii => &['_', '.', ':', '-', '=', '+', '*', '#'],
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
            (Self::Unicode, Mark::Skipped) => '○',
            (Self::Unicode, Mark::Pending) => '·',
            (Self::Unicode, Mark::Reroute) => '↻',
            (Self::Unicode, Mark::Caution) => '▲',
            (Self::Ascii, Mark::Done) => '+',
            (Self::Ascii, Mark::Failed) => 'x',
            (Self::Ascii, Mark::Running) => '>',
            (Self::Ascii, Mark::NeedsYou) => '?',
            (Self::Ascii, Mark::Skipped) => '-',
            (Self::Ascii, Mark::Pending) => '.',
            (Self::Ascii, Mark::Reroute) => '~',
            (Self::Ascii, Mark::Caution) => '!',
        }
    }
}

impl std::fmt::Display for Glyphs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unicode => f.write_str("unicode"),
            Self::Ascii => f.write_str("ascii"),
        }
    }
}

/// The set `value` names, or `None` when it names neither. The one
/// reading of [`OVERRIDE_VAR`], shared by the policy and the report of a
/// value it could not use.
pub fn parse_choice(value: &str) -> Option<Glyphs> {
    match value.trim().to_ascii_lowercase().as_str() {
        "unicode" => Some(Glyphs::Unicode),
        "ascii" => Some(Glyphs::Ascii),
        _ => None,
    }
}

/// Whether `locale` names a UTF-8 encoding. The bytes written are UTF-8
/// either way; what the locale names is how the terminal at the other
/// end decodes them, and one that expects a single-byte encoding draws a
/// multi-byte glyph as the several characters it is made of. No locale
/// named at all is the POSIX default, `C`, which is such a one.
fn names_utf8(locale: Option<&str>) -> bool {
    let Some(locale) = locale else {
        return false;
    };
    let locale = locale.to_ascii_lowercase();
    locale.contains("utf-8") || locale.contains("utf8")
}

#[cfg(test)]
mod tests {

    #[test]
    fn every_glyph_a_diagram_is_drawn_with_is_ascii_in_the_ascii_set() {
        let ways = [false, true];
        let strokes = [Stroke::Arrow, Stroke::Open, Stroke::Dotted, Stroke::Thick];
        let mut drawn = Vec::new();
        for up in ways {
            for down in ways {
                for left in ways {
                    for right in ways {
                        for stroke in strokes {
                            drawn.push(Glyphs::Ascii.joint(up, down, left, right, stroke));
                        }
                    }
                }
            }
        }
        for shape in [Shape::Box, Shape::Round, Shape::Decision] {
            drawn.extend(Glyphs::Ascii.corners(shape));
        }
        drawn.extend([Glyphs::Ascii.head(true), Glyphs::Ascii.head(false)]);
        drawn.extend(Glyphs::Ascii.joins().chars());
        drawn.push(Glyphs::Ascii.continued());
        assert!(drawn.iter().all(char::is_ascii), "{drawn:?}");
    }

    use super::*;

    fn env(explicit: Option<&str>, locale: Option<&str>, term: Option<&str>) -> GlyphEnv {
        GlyphEnv {
            explicit: explicit.map(str::to_string),
            locale: locale.map(str::to_string),
            term: term.map(str::to_string),
        }
    }

    #[test]
    fn an_explicit_choice_wins_over_both_other_signals() {
        let ascii_environment = env(Some("unicode"), Some("C"), Some("dumb"));
        assert_eq!(Glyphs::select(&ascii_environment), Glyphs::Unicode);
        let unicode_environment = env(Some("ASCII "), Some("en_US.UTF-8"), Some("xterm"));
        assert_eq!(Glyphs::select(&unicode_environment), Glyphs::Ascii);
    }

    #[test]
    fn a_locale_that_names_no_utf8_gets_ascii() {
        assert_eq!(
            Glyphs::select(&env(None, Some("C"), Some("xterm"))),
            Glyphs::Ascii
        );
        assert_eq!(
            Glyphs::select(&env(None, None, Some("xterm"))),
            Glyphs::Ascii
        );
        assert_eq!(
            Glyphs::select(&env(None, Some("ja_JP.UTF-8"), Some("xterm"))),
            Glyphs::Unicode
        );
    }

    #[test]
    fn a_dumb_terminal_gets_ascii_even_under_a_utf8_locale() {
        assert_eq!(
            Glyphs::select(&env(None, Some("en_US.utf8"), Some("dumb"))),
            Glyphs::Ascii
        );
    }

    #[test]
    fn an_unrecognized_override_leaves_the_environment_to_decide() {
        assert_eq!(
            Glyphs::select(&env(Some("fancy"), Some("en_US.UTF-8"), None)),
            Glyphs::Unicode
        );
        assert_eq!(
            Glyphs::select(&env(Some("fancy"), Some("C"), None)),
            Glyphs::Ascii
        );
    }

    #[test]
    fn every_glyph_of_either_set_occupies_exactly_one_cell() {
        for glyphs in [Glyphs::Unicode, Glyphs::Ascii] {
            let mut drawn: Vec<char> =
                vec![glyphs.bar_filled(), glyphs.bar_empty(), glyphs.ellipsis()];
            drawn.extend(glyphs.ramp());
            drawn.extend(crate::state::ALL_MARKS.map(|mark| glyphs.mark(mark)));
            for ch in drawn {
                assert_eq!(
                    crate::cell_width(&ch.to_string()),
                    1,
                    "{ch:?} is not one cell wide under {glyphs}"
                );
            }
        }
    }
}
