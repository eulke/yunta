//! How a line is painted on a stream, and whether a stream gets paint
//! at all.
//!
//! The plain ink writes the text and nothing else; the ANSI ink wraps
//! each span in the color its tone takes. Color only repeats what the
//! words already say, so the two inks write the same text: a captured
//! stream reads the same whoever captured it, and that is held by a test
//! rather than by care. Whether a stream gets color, and links with it,
//! is a policy read once from the command line and the environment by
//! whoever runs the process, then asked per stream.
//!
//! This is the one place in the crate that writes an SGR escape or a
//! terminal link.

use crate::glyphs::{Glyphs, Mark};
use crate::line::{Line, Tone};

/// When `--color` paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum ColorWhen {
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
pub struct ColorPolicy {
    pub when: ColorWhen,
    /// `NO_COLOR`, set and not empty.
    pub no_color: bool,
    /// `CLICOLOR_FORCE`, set to anything but `0` or empty.
    pub force: bool,
    /// `CLICOLOR=0`.
    pub off: bool,
    /// `TERM=dumb`.
    pub dumb: bool,
    /// Whether a path is a link: `YUNTA_HYPERLINKS` set to `1` or `0`
    /// says outright; otherwise a terminal that announces it opens links
    /// — iTerm, WezTerm, VS Code's, a VTE from 0.50 on, kitty, Windows
    /// Terminal — gets them.
    pub links: Links,
}

/// Whether a stream that gets color gets links too.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Links {
    /// `YUNTA_HYPERLINKS=1`: wherever there is color.
    Forced,
    /// `YUNTA_HYPERLINKS=0`.
    Refused,
    /// The terminal announced it opens links: on a terminal with color.
    Announced,
    /// Nothing said they open.
    #[default]
    Unknown,
}

impl ColorPolicy {
    /// The ink a stream gets, `terminal` saying whether it is one.
    ///
    /// The command line decides first; then a reader who asked for no
    /// color; then one who asked to force it; then one who turned it
    /// off; and then whether the stream is a terminal that draws it.
    pub fn ink(&self, terminal: bool) -> Ink {
        match self.color(terminal) {
            false => Ink::Plain,
            true => match (self.links, terminal) {
                (Links::Forced, _) | (Links::Announced, true) => Ink::Linked,
                _ => Ink::Ansi16,
            },
        }
    }

    /// Whether a stream gets color, `terminal` saying whether it is one.
    fn color(&self, terminal: bool) -> bool {
        match self.when {
            ColorWhen::Always => return true,
            ColorWhen::Never => return false,
            ColorWhen::Auto => {}
        }
        if self.no_color {
            return false;
        }
        if self.force {
            return true;
        }
        !(self.off || !terminal || self.dumb)
    }
}

/// How a stream's lines are painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    /// The text and nothing else.
    Plain,
    /// The text, each span in the color of its tone.
    Ansi16,
    /// As [`Ink::Ansi16`], and a path a link to the file it names — for
    /// a terminal that opens one.
    Linked,
}

impl Ink {
    /// `line`, painted.
    pub fn paint(self, line: &Line) -> String {
        line.spans()
            .iter()
            .map(|span| {
                let word = self.word(span.tone, &span.text);
                match (self, &span.link) {
                    // OSC 8: the text between the two sequences opens
                    // the target; a terminal without links shows the
                    // text alone.
                    (Ink::Linked, Some(target)) => {
                        format!("\x1b]8;;{target}\x1b\\{word}\x1b]8;;\x1b\\")
                    }
                    _ => word,
                }
            })
            .collect()
    }

    /// `text` in `tone`, painted.
    pub fn word(self, tone: Tone, text: &str) -> String {
        match (self, sgr(tone)) {
            (Ink::Ansi16 | Ink::Linked, Some(sgr)) => format!("\x1b[{sgr}m{text}\x1b[0m"),
            _ => text.to_string(),
        }
    }

    /// `mark`'s glyph, painted in the tone the mark takes.
    pub fn mark(self, glyphs: Glyphs, mark: Mark) -> String {
        self.word(Tone::of(mark), &glyphs.mark(mark).to_string())
    }
}

/// The SGR parameters `tone` is painted with: the sixteen ANSI
/// colors only, so every terminal draws them in its own palette.
fn sgr(tone: Tone) -> Option<&'static str> {
    match tone {
        Tone::Plain => None,
        Tone::Strong | Tone::Command => Some("1"),
        Tone::Muted => Some("2"),
        Tone::Done => Some("32"),
        Tone::Failed => Some("31"),
        Tone::Running => Some("36"),
        Tone::NeedsYou => Some("1;35"),
        Tone::Caution => Some("33"),
    }
}

/// `text` with every SGR escape and every link taken out: what a painted
/// line reads as once color and links are gone.
#[cfg(test)]
pub fn strip_sgr(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    // A link opens and closes with `ESC ] 8 ; ; <target> ESC \`; what it
    // wraps is the text a reader reads.
    let mut unlinked = String::with_capacity(text.len());
    while let Some(at) = rest.find("\x1b]8;;") {
        unlinked.push_str(&rest[..at]);
        let after = &rest[at..];
        rest = match after.find("\x1b\\") {
            Some(end) => &after[end + 2..],
            None => "",
        };
    }
    unlinked.push_str(rest);
    let mut rest = unlinked.as_str();
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
    fn links_are_drawn_where_the_terminal_announces_them_or_a_reader_forces_them() {
        let with = |links| ColorPolicy {
            links,
            ..ColorPolicy::default()
        };
        assert_eq!(with(Links::Announced).ink(true), Ink::Linked);
        assert_eq!(with(Links::Announced).ink(false), Ink::Plain);
        assert_eq!(with(Links::Unknown).ink(true), Ink::Ansi16);
        assert_eq!(with(Links::Refused).ink(true), Ink::Ansi16);
        let forced = ColorPolicy {
            when: ColorWhen::Always,
            links: Links::Forced,
            ..ColorPolicy::default()
        };
        assert_eq!(forced.ink(false), Ink::Linked, "a pipe asked for both");
        let no_color = ColorPolicy {
            no_color: true,
            links: Links::Forced,
            ..ColorPolicy::default()
        };
        assert_eq!(no_color.ink(true), Ink::Plain, "no color, no escapes");
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
}
