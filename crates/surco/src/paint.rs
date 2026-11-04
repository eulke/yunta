//! How a line is painted on a stream, and whether a stream gets paint
//! at all.
//!
//! A stream is painted plain, or in one layer of color: the sixteen colors of
//! its terminal's theme, or 24-bit colors tuned to the ground its terminal
//! draws on. Color only repeats what the words already say, so a stream
//! writes the same text plain and in either layer, in Unicode and in ASCII: a
//! captured stream reads the same whoever captured it, a code span keeps its
//! backticks under color, and no surface chooses a word by its layer. That is
//! held by a test rather than by care. A link comes with color, in either
//! layer, and is not a layer of its own. Whether a stream gets color, in which
//! layer and with links, is a policy read once from the command line and the
//! environment by whoever runs the process, then asked per stream. The terminal
//! is never asked: a query needs a terminal that answers and a wait for an answer
//! that may never come, at every start, and tells nothing about a pipe.
//!
//! This is the one place in the crate that writes an escape: an SGR color,
//! a terminal link, the window's title, a synchronized update or a bell.

use crate::line::{Fill, Line, Span, Tone};
use crate::palette::{self, Rgb};

mod control;
pub use control::*;

#[cfg(feature = "clap")]
mod help;

#[cfg(feature = "clap")]
pub use help::clap_styles;

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
    /// (iTerm, WezTerm, VS Code's, a VTE from 0.50 on, kitty, Windows
    /// Terminal) gets them.
    pub links: Links,
    /// The layer [`LOOK_VAR`] names, when it names one.
    pub look: Option<LookChoice>,
    /// Whether the terminal draws 24-bit colors: `COLORTERM` is `truecolor`
    /// or `24bit`.
    pub truecolor: bool,
    /// The ground the terminal draws on, from `COLORFGBG`.
    pub ground: Ground,
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

/// The variable that names a stream's layer outright: `plain`, `16` or `24`.
pub const LOOK_VAR: &str = "YUNTA_LOOK";

/// The layer [`LOOK_VAR`] names outright.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookChoice {
    /// `plain`: no color, unless `--color always` asks for it.
    Plain,
    /// `16`: the sixteen colors of the terminal's theme.
    Ansi16,
    /// `24`: 24-bit colors, tuned to the ground.
    Truecolor,
}

/// Whether a terminal draws on a dark ground or a light one, which the
/// 24-bit colors are tuned to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Ground {
    /// A dark ground: what a terminal that says nothing is taken to draw on.
    #[default]
    Dark,
    /// A light ground.
    Light,
}

/// The colors a stream with color draws: the sixteen of its terminal's theme,
/// or 24-bit colors tuned to its ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// The sixteen colors of the terminal's theme.
    Ansi16,
    /// 24-bit colors, tuned to the ground the terminal draws on.
    Truecolor(Ground),
}

/// How a stream's lines are painted: plain, or in a layer of color where a
/// path may be a link to the file it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    /// The text and nothing else: no color and no links.
    Plain,
    /// Color in `layer`; a path is a link to its file where `links` is true.
    Color { layer: Layer, links: bool },
}

impl ColorPolicy {
    /// The paint a stream gets, `terminal` saying whether it is one.
    ///
    /// The command line decides first; then `YUNTA_LOOK=plain`; then a
    /// reader who asked for no color, one who asked to force it, and one who
    /// turned it off, and then whether the stream is a terminal that draws
    /// it. A stream with color draws in the layer `YUNTA_LOOK` names, or else
    /// in 24-bit colors when its terminal draws them, or else in the sixteen.
    pub fn paint(&self, terminal: bool) -> Paint {
        match self.color(terminal) {
            false => Paint::Plain,
            true => Paint::Color {
                layer: self.layer(),
                links: matches!(
                    (self.links, terminal),
                    (Links::Forced, _) | (Links::Announced, true)
                ),
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
        if self.look == Some(LookChoice::Plain) {
            return false;
        }
        if self.no_color {
            return false;
        }
        if self.force {
            return true;
        }
        !(self.off || !terminal || self.dumb)
    }

    /// The layer a stream with color draws in: the one `YUNTA_LOOK` names,
    /// else 24-bit colors when the terminal draws them, else the sixteen.
    fn layer(&self) -> Layer {
        match self.look {
            Some(LookChoice::Ansi16) => Layer::Ansi16,
            Some(LookChoice::Truecolor) => Layer::Truecolor(self.ground),
            Some(LookChoice::Plain) | None => match self.truecolor {
                true => Layer::Truecolor(self.ground),
                false => Layer::Ansi16,
            },
        }
    }
}

/// The layer `value` names for [`LOOK_VAR`]: `plain`, `16` or `24`, with
/// case and surrounding space ignored. Any other value names none.
pub fn look_of(value: Option<&str>) -> Option<LookChoice> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "plain" => Some(LookChoice::Plain),
        "16" => Some(LookChoice::Ansi16),
        "24" => Some(LookChoice::Truecolor),
        _ => None,
    }
}

/// The ground COLORFGBG names: its last field is the background's color
/// number, 0 to 6 or 8 dark and 7 or 9 to 15 light; anything else, or none,
/// reads as dark, since no terminal is asked.
pub fn ground_of(colorfgbg: Option<&str>) -> Ground {
    let background = colorfgbg
        .and_then(|value| value.rsplit(';').next())
        .and_then(|field| field.trim().parse::<u8>().ok());
    match background {
        Some(7 | 9..=15) => Ground::Light,
        _ => Ground::Dark,
    }
}

impl Paint {
    /// The sixteen colors of the terminal's theme, and no links.
    pub const ANSI16: Paint = Paint::Color {
        layer: Layer::Ansi16,
        links: false,
    };

    /// The sixteen colors of the terminal's theme, and a path a link to the
    /// file it names.
    pub const LINKED: Paint = Paint::Color {
        layer: Layer::Ansi16,
        links: true,
    };

    /// `line`, painted: each span in its tone, and as a link where the paint
    /// has links on. Only a layer with 24-bit color paints fills: each span on
    /// the fill it carries or else the line's, and a filled line erased to its
    /// edge, so the fill reaches the edge with no space written: a space would
    /// change what a reader copies and the width a cut measures.
    pub fn line(self, line: &Line) -> String {
        let fill = line.fill();
        let painted: String = line
            .spans()
            .iter()
            .map(|span| self.span(span, fill))
            .collect();
        match self.erase(fill) {
            Some(erase) => format!("{painted}{erase}"),
            None => painted,
        }
    }

    /// `text` in `tone`, painted.
    pub fn word(self, tone: Tone, text: &str) -> String {
        let text = harmless(text);
        match self {
            Paint::Plain => text,
            Paint::Color {
                layer: Layer::Ansi16,
                ..
            } => match sgr(tone) {
                Some(sgr) => format!("\x1b[{sgr}m{text}\x1b[0m"),
                None => text,
            },
            Paint::Color {
                layer: Layer::Truecolor(ground),
                ..
            } => truecolor(ground, tone, None, None, &text),
        }
    }

    /// `span` in this paint: a layer with 24-bit color paints it on the fill
    /// it carries or else `fill`, the line's; a link is written only where the
    /// paint has links on.
    fn span(self, span: &Span, fill: Option<Fill>) -> String {
        let word = match self {
            Paint::Color {
                layer: Layer::Truecolor(ground),
                ..
            } => truecolor(ground, span.tone, span.fill, fill, &harmless(&span.text)),
            Paint::Plain
            | Paint::Color {
                layer: Layer::Ansi16,
                ..
            } => self.word(span.tone, &span.text),
        };
        match (self, &span.link) {
            // OSC 8: the text between the two sequences opens the target; a
            // terminal without links shows the text alone.
            (Paint::Color { links: true, .. }, Some(target)) => {
                format!("\x1b]8;;{target}\x1b\\{word}\x1b]8;;\x1b\\")
            }
            _ => word,
        }
    }

    /// The erase that ends a filled line in a layer that draws fills: the
    /// background is set, then the rest of the line is erased with it. None
    /// when the line has no background to draw, which a pill's line never has.
    fn erase(self, fill: Option<Fill>) -> Option<String> {
        match self {
            Paint::Plain
            | Paint::Color {
                layer: Layer::Ansi16,
                ..
            } => None,
            Paint::Color {
                layer: Layer::Truecolor(ground),
                ..
            } => {
                let background = color_parameter(48, palette::fill(ground, fill?)?);
                Some(format!("\x1b[{background}m\x1b[K\x1b[0m"))
            }
        }
    }
}

/// `text` as a terminal may be told it: a control character turns to a space,
/// since any of them could move the cursor back, ring or open a sequence, and
/// what a page repeats (a file's name, what a session said) tells a terminal
/// nothing. A line break and a tab only say where the next word goes, so they
/// stay, and tab-indented code keeps its shape.
pub(crate) fn harmless(text: &str) -> String {
    text.replace(
        |sign: char| sign.is_control() && !matches!(sign, '\n' | '\t'),
        " ",
    )
}

/// A color of the sixteen, which every terminal draws in its own theme.
#[derive(Clone, Copy)]
enum Ansi {
    Red,
    Green,
    Yellow,
    Magenta,
    Cyan,
}

impl Ansi {
    /// The SGR parameter that sets this color for text.
    fn code(self) -> u8 {
        match self {
            Ansi::Red => 31,
            Ansi::Green => 32,
            Ansi::Yellow => 33,
            Ansi::Magenta => 35,
            Ansi::Cyan => 36,
        }
    }
}

/// How `tone` is painted on sixteen colors: bold, dim, and the ANSI color
/// it takes, so every terminal draws it in its own palette. The one table
/// both the SGR escapes and clap's styles are written from.
fn ansi(tone: Tone) -> (bool, bool, Option<Ansi>) {
    match tone {
        Tone::Plain => (false, false, None),
        Tone::Strong | Tone::Command => (true, false, None),
        Tone::Muted => (false, true, None),
        // In sixteen colors faint is dim, as muted is: the layer has no
        // step between them.
        Tone::Faint => (false, true, None),
        Tone::Done => (false, false, Some(Ansi::Green)),
        Tone::Failed => (false, false, Some(Ansi::Red)),
        Tone::Running => (false, false, Some(Ansi::Cyan)),
        Tone::NeedsYou => (true, false, Some(Ansi::Magenta)),
        Tone::Caution => (false, false, Some(Ansi::Yellow)),
        Tone::Added => (false, false, Some(Ansi::Green)),
        Tone::Removed => (false, false, Some(Ansi::Red)),
        // Code's colors are material of the 24-bit layer: in the sixteen
        // colors a word of code reads plain.
        Tone::Code(_) => (false, false, None),
    }
}

/// The SGR parameters `tone` is painted with, from [`ansi`].
fn sgr(tone: Tone) -> Option<String> {
    let (bold, dim, color) = ansi(tone);
    let parameters: Vec<String> = [bold.then(|| "1".to_string()), dim.then(|| "2".to_string())]
        .into_iter()
        .flatten()
        .chain(color.map(|color| color.code().to_string()))
        .collect();
    (!parameters.is_empty()).then(|| parameters.join(";"))
}

/// `text` in `tone` on `ground`, on the fill its span carries (`own`) or else
/// the line's (`line`): its SGR parameters and a reset after it, or the text
/// alone when nothing is drawn.
fn truecolor(
    ground: Ground,
    tone: Tone,
    own: Option<Fill>,
    line: Option<Fill>,
    text: &str,
) -> String {
    let parameters = truecolor_parameters(ground, tone, own, line);
    if parameters.is_empty() {
        return text.to_string();
    }
    format!("\x1b[{}m{text}\x1b[0m", parameters.join(";"))
}

/// The SGR parameters that draw `tone` on `ground`: bold where Ansi16 is
/// bold, the text's color, and the background of the fill that paints it.
/// A span's own fill wins over the line's. A pill is its tone's color with
/// pill ink on it, in bold; a pill in a tone the palette leaves to the
/// terminal has no color to paint, so it is its word on the line's fill.
fn truecolor_parameters(
    ground: Ground,
    tone: Tone,
    own: Option<Fill>,
    line: Option<Fill>,
) -> Vec<String> {
    let bold = ansi(tone).0;
    let text = palette::tone(ground, tone);
    let (bold, text, background) = match (own, text) {
        (Some(Fill::Pill), Some(color)) => (true, Some(palette::pill_ink(ground)), Some(color)),
        _ => (
            bold,
            text,
            own.and_then(|fill| palette::fill(ground, fill))
                .or_else(|| line.and_then(|fill| palette::fill(ground, fill))),
        ),
    };
    bold.then(|| "1".to_string())
        .into_iter()
        .chain(text.map(|color| color_parameter(38, color)))
        .chain(background.map(|color| color_parameter(48, color)))
        .collect()
}

/// The SGR parameter that sets `color` in 24 bits: the text's color when
/// `target` is 38, the background's when it is 48.
fn color_parameter(target: u8, Rgb(red, green, blue): Rgb) -> String {
    format!("{target};2;{red};{green};{blue}")
}

/// `text` with every SGR escape, every erase and every link taken out: what
/// a painted line reads as once color, fills and links are gone.
#[cfg(test)]
pub fn strip_sgr(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('\x1b') {
        out.push_str(&rest[..at]);
        rest = skip_escape(&rest[at..]);
    }
    out.push_str(rest);
    out
}

/// The text after the escape at the start of `escape`. An OSC, which is how a
/// link opens and closes, runs to its string terminator; a CSI, which is how
/// color and an erase are written, runs to its final byte. What an escape
/// wraps is left for the caller to keep.
#[cfg(test)]
fn skip_escape(escape: &str) -> &str {
    let mut bytes = escape.char_indices().skip(1);
    match bytes.next() {
        Some((_, ']')) => escape.find("\x1b\\").map_or("", |end| &escape[end + 2..]),
        Some((_, '[')) => bytes
            .find(|(_, c)| ('@'..='~').contains(c))
            .map_or("", |(end, _)| &escape[end + 1..]),
        _ => &escape[1..],
    }
}

#[cfg(test)]
mod tests;
