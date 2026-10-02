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
//! This is the one place in the crate that writes an SGR escape or a
//! terminal link.

use super::state::Mark;

pub use super::color::{ColorPolicy, ColorWhen, Links};

/// What a span of text is to a reader, which decides its color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
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
    pub fn of(mark: Mark) -> Self {
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

/// A run of text in one tone, and — for a path a reader opens — what it
/// links to where a terminal opens links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub tone: Tone,
    pub link: Option<String>,
}

/// One line, as the spans it is built from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Line(Vec<Span>);

impl Line {
    pub fn new() -> Self {
        Line(Vec::new())
    }

    /// The line with `text` appended in `tone`.
    pub fn push(mut self, tone: Tone, text: impl Into<String>) -> Self {
        let text = text.into();
        if !text.is_empty() {
            self.0.push(Span {
                text,
                tone,
                link: None,
            });
        }
        self
    }

    /// The line with `shown` appended as a path, linking to the file at
    /// `target` on a terminal that opens links.
    pub fn path(mut self, shown: impl Into<String>, target: &std::path::Path) -> Self {
        let text = shown.into();
        if !text.is_empty() {
            self.0.push(Span {
                text,
                tone: Tone::Plain,
                link: Some(format!("file://{}", target.display())),
            });
        }
        self
    }

    /// The line with `text` appended in the terminal's own color.
    pub fn plain(self, text: impl Into<String>) -> Self {
        self.push(Tone::Plain, text)
    }

    /// The line moved right by `margin`, its spans and their tones kept.
    pub fn under(mut self, margin: &str) -> Self {
        if !margin.is_empty() {
            self.0.insert(
                0,
                Span {
                    text: margin.to_string(),
                    tone: Tone::Plain,
                    link: None,
                },
            );
        }
        self
    }

    /// The line inside `cells`, cut where it passes them and marked with
    /// the ellipsis `glyphs` draws, each span keeping its tone — so a row
    /// is cut on what a reader sees and painted after.
    pub fn cut(&self, cells: usize, glyphs: super::Glyphs) -> Line {
        let plain: String = self.0.iter().map(|span| span.text.as_str()).collect();
        if super::cell_width(&plain) <= cells {
            return self.clone();
        }
        let ellipsis = glyphs.ellipsis();
        let cut = super::truncate(&plain, cells, glyphs);
        let kept = cut.trim_end().strip_suffix(ellipsis).unwrap_or_default();
        let mut line = Line::new();
        let mut left = kept.len();
        for span in &self.0 {
            // The cut lands between characters of the whole line, so it
            // lands between characters of the span it falls in.
            let taken = span.text.len().min(left);
            left -= taken;
            if taken > 0 {
                line.0.push(Span {
                    text: span.text[..taken].to_string(),
                    ..span.clone()
                });
            }
            if left == 0 {
                return line.push(span.tone, ellipsis.to_string());
            }
        }
        line
    }

    pub fn spans(&self) -> &[Span] {
        &self.0
    }

    /// The line with `other`'s spans after its own.
    pub fn then(mut self, other: Line) -> Self {
        self.0.extend(other.0);
        self
    }

    /// What the line reads as, unpainted.
    pub fn text(&self) -> String {
        self.0.iter().map(|span| span.text.as_str()).collect()
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
        match (self, tone.sgr()) {
            (Ink::Ansi16 | Ink::Linked, Some(sgr)) => format!("\x1b[{sgr}m{text}\x1b[0m"),
            _ => text.to_string(),
        }
    }

    /// `mark`'s glyph, painted in the tone the mark takes.
    pub fn mark(self, glyphs: super::Glyphs, mark: Mark) -> String {
        self.word(Tone::of(mark), &glyphs.mark(mark).to_string())
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

    #[test]
    fn a_line_cut_to_fit_keeps_the_tone_of_every_span_it_keeps() {
        let line = Line::new()
            .push(Tone::NeedsYou, "needs you")
            .plain(": ")
            .push(Tone::Strong, "yunta resolve-gate T1Y0P5 <option>");
        let cut = line.cut(20, super::super::Glyphs::Ascii);
        assert_eq!(Ink::Plain.paint(&cut), "needs you: yunta re~");
        assert_eq!(cut.spans()[0], line.spans()[0]);
        assert_eq!(cut.spans().last().map(|span| span.tone), Some(Tone::Strong));
        assert_eq!(line.cut(80, super::super::Glyphs::Ascii), line);
    }

    #[test]
    fn a_line_moved_under_a_margin_keeps_its_tones() {
        let line = Line::new().push(Tone::Failed, "exit 1").plain(" — lint");
        let moved = line.clone().under("  ");
        assert_eq!(Ink::Plain.paint(&moved), "  exit 1 — lint");
        assert_eq!(&moved.spans()[1..], line.spans());
    }
}
