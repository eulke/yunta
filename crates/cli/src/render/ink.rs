//! What a line looks like once it is painted, and whether a stream gets
//! paint at all.
//!
//! A line is built from spans that each carry a tone — what the text is
//! to a reader — and painted once, by the ink its stream gets. The plain
//! ink writes the text and nothing else; the ANSI ink wraps each span in
//! the color its tone takes. Color only repeats what the words already
//! say, so the two inks write the same text: a captured stream reads the
//! same whoever captured it, and that is held by a test rather than by
//! care.
//!
//! This is the one place in the crate that writes an SGR escape.

use std::sync::OnceLock;

use super::state::Mark;

/// What a span of text is to a reader, which decides its color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    /// The body of a line: the terminal's own color.
    Plain,
    /// What a line is about, and a command to copy.
    Strong,
    /// What a reader skims past: a label, a duration, a path.
    Muted,
    Done,
    Failed,
    Running,
    /// Something waits on a person. The one tone that means a person.
    NeedsYou,
    /// Worth a reader's attention and nothing anybody has to answer.
    Caution,
}

impl Tone {
    /// The tone a mark is painted in.
    pub(crate) fn of(mark: Mark) -> Self {
        match mark {
            Mark::Done => Tone::Done,
            Mark::Failed => Tone::Failed,
            Mark::Running => Tone::Running,
            Mark::NeedsYou => Tone::NeedsYou,
            Mark::Caution | Mark::Reroute => Tone::Caution,
            Mark::Skipped | Mark::Pending => Tone::Muted,
        }
    }

    /// The SGR parameters this tone is painted with: the sixteen ANSI
    /// colors only, so every terminal draws them in its own palette.
    fn sgr(self) -> Option<&'static str> {
        match self {
            Tone::Plain => None,
            Tone::Strong => Some("1"),
            Tone::Muted => Some("2"),
            Tone::Done => Some("32"),
            Tone::Failed => Some("31"),
            Tone::Running => Some("36"),
            Tone::NeedsYou => Some("1;35"),
            Tone::Caution => Some("33"),
        }
    }
}

/// A run of text in one tone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Span {
    pub(crate) text: String,
    pub(crate) tone: Tone,
}

/// One line, as the spans it is built from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Line(Vec<Span>);

impl Line {
    pub(crate) fn new() -> Self {
        Line(Vec::new())
    }

    /// The line with `text` appended in `tone`.
    pub(crate) fn push(mut self, tone: Tone, text: impl Into<String>) -> Self {
        let text = text.into();
        if !text.is_empty() {
            self.0.push(Span { text, tone });
        }
        self
    }

    /// The line with `text` appended in the terminal's own color.
    pub(crate) fn plain(self, text: impl Into<String>) -> Self {
        self.push(Tone::Plain, text)
    }

    /// The line moved right by `margin`, its spans and their tones kept.
    pub(crate) fn under(mut self, margin: &str) -> Self {
        if !margin.is_empty() {
            self.0.insert(
                0,
                Span {
                    text: margin.to_string(),
                    tone: Tone::Plain,
                },
            );
        }
        self
    }

    pub(crate) fn spans(&self) -> &[Span] {
        &self.0
    }
}

/// How a stream's lines are painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ink {
    /// The text and nothing else.
    Plain,
    /// The text, each span in the color of its tone.
    Ansi16,
}

impl Ink {
    /// `line`, painted.
    pub(crate) fn paint(self, line: &Line) -> String {
        line.spans()
            .iter()
            .map(|span| self.word(span.tone, &span.text))
            .collect()
    }

    /// `text` in `tone`, painted.
    pub(crate) fn word(self, tone: Tone, text: &str) -> String {
        match (self, tone.sgr()) {
            (Ink::Ansi16, Some(sgr)) => format!("\x1b[{sgr}m{text}\x1b[0m"),
            _ => text.to_string(),
        }
    }

    /// `mark`'s glyph, painted in the tone the mark takes.
    pub(crate) fn mark(self, glyphs: super::Glyphs, mark: Mark) -> String {
        self.word(Tone::of(mark), &glyphs.mark(mark).to_string())
    }

    /// The ink stderr gets in this process.
    pub(crate) fn stderr() -> Self {
        use std::io::IsTerminal;
        policy().ink(std::io::stderr().is_terminal())
    }

    /// The ink stdout gets in this process.
    pub(crate) fn stdout() -> Self {
        use std::io::IsTerminal;
        policy().ink(std::io::stdout().is_terminal())
    }
}

/// `text` with every SGR escape taken out: what a painted line reads as
/// once color is gone.
#[cfg(test)]
pub(crate) fn strip_sgr(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("\x1b[") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        match after.find('m') {
            Some(end) => rest = &after[end + 1..],
            None => {
                rest = after;
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// When `--color` paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub(crate) enum ColorWhen {
    /// On a terminal that draws color, unless the environment says not to.
    #[default]
    Auto,
    /// Always, a pipe included.
    Always,
    /// Never.
    Never,
}

/// Everything that decides whether a stream gets color, read once from
/// the process and the command line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ColorPolicy {
    pub(crate) when: ColorWhen,
    /// `NO_COLOR`, set and not empty.
    pub(crate) no_color: bool,
    /// `CLICOLOR_FORCE`, set to anything but `0` or empty.
    pub(crate) force: bool,
    /// `CLICOLOR=0`.
    pub(crate) off: bool,
    /// `TERM=dumb`.
    pub(crate) dumb: bool,
}

impl ColorPolicy {
    /// The ink a stream gets, `terminal` saying whether it is one.
    ///
    /// The command line decides first; then a reader who asked for no
    /// color; then one who asked to force it; then one who turned it
    /// off; and then whether the stream is a terminal that draws it.
    pub(crate) fn ink(&self, terminal: bool) -> Ink {
        match self.when {
            ColorWhen::Always => return Ink::Ansi16,
            ColorWhen::Never => return Ink::Plain,
            ColorWhen::Auto => {}
        }
        if self.no_color {
            return Ink::Plain;
        }
        if self.force {
            return Ink::Ansi16;
        }
        if self.off || !terminal || self.dumb {
            return Ink::Plain;
        }
        Ink::Ansi16
    }
}

static POLICY: OnceLock<ColorPolicy> = OnceLock::new();

/// Fixes this process's color policy. `main` calls it once, before
/// anything is printed; a later call changes nothing.
pub(crate) fn settle(policy: ColorPolicy) {
    POLICY.get_or_init(|| policy);
}

/// This process's color policy: what `main` settled, or the plain one a
/// process that never settled any — a unit test — gets.
fn policy() -> ColorPolicy {
    POLICY.get().cloned().unwrap_or(ColorPolicy {
        when: ColorWhen::Never,
        ..ColorPolicy::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROLES: [Tone; 8] = [
        Tone::Plain,
        Tone::Strong,
        Tone::Muted,
        Tone::Done,
        Tone::Failed,
        Tone::Running,
        Tone::NeedsYou,
        Tone::Caution,
    ];

    /// Every tone around texts that carry what a line can: spaces, glyphs,
    /// backticks, a cell-wide character, an empty word.
    #[test]
    fn plain_text_equals_inked_text_with_escapes_stripped() {
        let texts = [
            "",
            "needs you",
            "✓ finished",
            "`yunta resume 7E5PH4`",
            "名前 · 3s",
        ];
        for (at, first) in ROLES.iter().enumerate() {
            for second in ROLES.iter().skip(at) {
                for text in texts {
                    let line = Line::new()
                        .push(*first, text)
                        .plain(" · ")
                        .push(*second, text);
                    assert_eq!(
                        strip_sgr(&Ink::Ansi16.paint(&line)),
                        Ink::Plain.paint(&line),
                        "{first:?} / {second:?} / {text:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn no_color_wins_over_auto_but_not_over_always() {
        let no_color = ColorPolicy {
            no_color: true,
            ..ColorPolicy::default()
        };
        assert_eq!(no_color.ink(true), Ink::Plain);
        let always = ColorPolicy {
            when: ColorWhen::Always,
            ..no_color
        };
        assert_eq!(always.ink(false), Ink::Ansi16);
    }

    #[test]
    fn clicolor_force_paints_a_pipe() {
        let forced = ColorPolicy {
            force: true,
            ..ColorPolicy::default()
        };
        assert_eq!(forced.ink(false), Ink::Ansi16);
    }

    #[test]
    fn color_is_decided_per_stream() {
        let auto = ColorPolicy::default();
        assert_eq!(auto.ink(true), Ink::Ansi16);
        assert_eq!(auto.ink(false), Ink::Plain);
        let dumb = ColorPolicy {
            dumb: true,
            ..ColorPolicy::default()
        };
        assert_eq!(dumb.ink(true), Ink::Plain);
        let off = ColorPolicy {
            off: true,
            ..ColorPolicy::default()
        };
        assert_eq!(off.ink(true), Ink::Plain);
    }

    #[test]
    fn a_person_and_a_caution_are_painted_apart() {
        assert_ne!(
            Ink::Ansi16.word(Tone::NeedsYou, "x"),
            Ink::Ansi16.word(Tone::Caution, "x")
        );
        assert_eq!(Tone::of(Mark::NeedsYou), Tone::NeedsYou);
        assert_eq!(Tone::of(Mark::Reroute), Tone::Caution);
    }

    #[test]
    fn a_line_moved_under_a_margin_keeps_its_tones() {
        let line = Line::new().push(Tone::Failed, "exit 1").plain(" — lint");
        let moved = line.clone().under("  ");
        assert_eq!(Ink::Plain.paint(&moved), "  exit 1 — lint");
        assert_eq!(&moved.spans()[1..], line.spans());
    }
}
