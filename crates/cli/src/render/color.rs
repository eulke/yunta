//! Whether a stream gets color, and links with it: decided once for the
//! process from the command line and the environment, then asked per
//! stream.

use std::sync::OnceLock;

use super::ink::Ink;

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
    /// Whether a path is a link: `YUNTA_HYPERLINKS` set to `1` or `0`
    /// says outright; otherwise a terminal that announces it opens links
    /// — iTerm, WezTerm, VS Code's, a VTE from 0.50 on, kitty, Windows
    /// Terminal — gets them.
    pub(crate) links: Links,
}

/// Whether a stream that gets color gets links too.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Links {
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
    pub(crate) fn ink(&self, terminal: bool) -> Ink {
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

static POLICY: OnceLock<ColorPolicy> = OnceLock::new();

/// Fixes this process's color policy. `main` calls it once, before
/// anything is printed; a later call changes nothing.
pub(crate) fn settle(policy: ColorPolicy) {
    POLICY.get_or_init(|| policy);
}

/// This process's color policy: what `main` settled, or the plain one a
/// process that never settled any — a unit test — gets.
pub(super) fn policy() -> ColorPolicy {
    POLICY.get().cloned().unwrap_or(ColorPolicy {
        when: ColorWhen::Never,
        ..ColorPolicy::default()
    })
}
