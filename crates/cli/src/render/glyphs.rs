//! The characters a terminal surface draws with, and the policy that
//! decides which of the two sets this process may use.

use super::state::StateWord;

/// The environment variable that decides the set outright: `unicode` or
/// `ascii`, case and surrounding space ignored.
const OVERRIDE_VAR: &str = "YUNTA_GLYPHS";

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
pub(crate) enum Glyphs {
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
pub(crate) struct GlyphEnv {
    /// [`OVERRIDE_VAR`], as the environment carries it.
    pub explicit: Option<String>,
    /// The locale in force: `LC_ALL`, else `LC_CTYPE`, else `LANG` — the
    /// order POSIX resolves character-type behavior in.
    pub locale: Option<String>,
    /// `TERM`.
    pub term: Option<String>,
}

impl GlyphEnv {
    /// The three values this process was started with.
    pub(crate) fn from_process() -> Self {
        Self {
            explicit: std::env::var(OVERRIDE_VAR).ok(),
            locale: ["LC_ALL", "LC_CTYPE", "LANG"]
                .iter()
                .find_map(|name| std::env::var(name).ok().filter(|v| !v.is_empty())),
            term: std::env::var("TERM").ok(),
        }
    }
}

impl Glyphs {
    /// The set this process's environment allows, naming on stderr a
    /// value of [`OVERRIDE_VAR`] it does not recognize and carrying on
    /// with what the rest of the environment says.
    pub(crate) fn from_env() -> Self {
        let env = GlyphEnv::from_process();
        let chosen = Self::select(&env);
        if let Some(value) = &env.explicit {
            if parse_choice(value).is_none() {
                crate::error::warn(format!(
                    "{OVERRIDE_VAR}=`{value}` is neither `unicode` nor `ascii` — \
                     drawing with {chosen}, which is what this environment names"
                ));
            }
        }
        chosen
    }

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
    pub(crate) fn select(env: &GlyphEnv) -> Self {
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
    pub(crate) fn bar_filled(self) -> char {
        match self {
            Self::Unicode => '█',
            Self::Ascii => '#',
        }
    }

    /// The character an empty step of a bar is drawn with — always
    /// something visible, so a bar at zero is still a bar and not a gap
    /// the eye reads as a missing row.
    pub(crate) fn bar_empty(self) -> char {
        match self {
            Self::Unicode => '·',
            Self::Ascii => '.',
        }
    }

    /// The mark that closes a value cut to fit its column. One cell in
    /// either set, so the arithmetic that places the cut is the same.
    pub(crate) fn ellipsis(self) -> char {
        match self {
            Self::Unicode => '…',
            Self::Ascii => '~',
        }
    }

    /// The eight steps a sparkline climbs, lightest first.
    pub(crate) fn ramp(self) -> &'static [char; 8] {
        match self {
            Self::Unicode => &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'],
            Self::Ascii => &['_', '.', ':', '-', '=', '+', '*', '#'],
        }
    }

    /// The mark that sits beside a state's word. It repeats the word for
    /// the eye; the word is what says it.
    pub(crate) fn state(self, word: StateWord) -> char {
        match (self, word) {
            (Self::Unicode, StateWord::Done) => '✓',
            (Self::Unicode, StateWord::Fail) => '✗',
            (Self::Unicode, StateWord::Run) => '●',
            (Self::Unicode, StateWord::Wait) => '◆',
            (Self::Unicode, StateWord::Skip) => '○',
            (Self::Unicode, StateWord::Todo) => '·',
            (Self::Ascii, StateWord::Done) => '+',
            (Self::Ascii, StateWord::Fail) => 'x',
            (Self::Ascii, StateWord::Run) => '>',
            (Self::Ascii, StateWord::Wait) => '?',
            (Self::Ascii, StateWord::Skip) => '-',
            (Self::Ascii, StateWord::Todo) => '.',
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
fn parse_choice(value: &str) -> Option<Glyphs> {
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
            drawn.extend(crate::render::state::ALL_WORDS.map(|word| glyphs.state(word)));
            for ch in drawn {
                assert_eq!(
                    crate::render::cell_width(&ch.to_string()),
                    1,
                    "{ch:?} is not one cell wide under {glyphs}"
                );
            }
        }
    }
}
