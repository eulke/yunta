//! Whether a stream gets color, and links with it: a policy read once
//! from the command line and the environment by whoever runs the
//! process, then asked per stream.

use super::ink::Ink;

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
