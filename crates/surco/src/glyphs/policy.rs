//! The policy that decides which of the two sets an environment allows, and the names the
//! two sets are read and written by.

use super::Glyphs;

/// The environment variable that decides the set outright: `unicode` or
/// `ascii`, case and surrounding space ignored.
pub const OVERRIDE_VAR: &str = "YUNTA_GLYPHS";

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
