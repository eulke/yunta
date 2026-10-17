//! What drawing depends on in this process: the color policy, the width
//! a reader asked for and the glyph set, each settled once by `main`
//! before anything is printed, and what each stream gets from them —
//! whether it is a terminal, and how wide it measures.

use std::io::IsTerminal;
use std::sync::OnceLock;

use dialoguer::console::Term;
use yunta_render::color::{ColorPolicy, ColorWhen};
use yunta_render::glyphs::{parse_choice, GlyphEnv, OVERRIDE_VAR};
use yunta_render::ink::Ink;
use yunta_render::{Glyphs, Look, Width};

static POLICY: OnceLock<ColorPolicy> = OnceLock::new();
static COLUMNS: OnceLock<Option<usize>> = OnceLock::new();
static GLYPHS: OnceLock<Glyphs> = OnceLock::new();

/// Fixes this process's color policy. `main` calls it once, before
/// anything is printed; a later call changes nothing.
///
/// The menu a prompt draws is drawn by a library that decides color for
/// itself, from the environment alone: it is told here what this policy
/// decided for stderr, so `--color never` and `NO_COLOR` reach the menu
/// as they reach every other line.
pub(crate) fn settle_color(policy: ColorPolicy) {
    dialoguer::console::set_colors_enabled_stderr(paints(&policy, std::io::stderr().is_terminal()));
    POLICY.get_or_init(|| policy);
}

/// Whether a stream that is a terminal or not, under `policy`, gets
/// paint at all.
fn paints(policy: &ColorPolicy, terminal: bool) -> bool {
    policy.ink(terminal) != Ink::Plain
}

/// Fixes the width this process's reader asked for — `COLUMNS`, when it
/// names one. `main` calls it once, before anything is printed; a later
/// call changes nothing.
pub(crate) fn settle_columns(columns: Option<usize>) {
    COLUMNS.get_or_init(|| columns);
}

/// Fixes the set this process draws with, from the environment it was
/// started with, naming on stderr a value of [`OVERRIDE_VAR`] it does not
/// recognize. `main` calls it once, before anything is printed; a later
/// call changes nothing.
pub(crate) fn settle_glyphs(env: &GlyphEnv) {
    let chosen = Glyphs::select(env);
    if let Some(value) = &env.explicit {
        if parse_choice(value).is_none() {
            crate::error::warn(format!(
                "{OVERRIDE_VAR}=`{value}` is neither `unicode` nor `ascii` — drawing with \
                 {chosen}, which is what this environment names"
            ));
        }
    }
    GLYPHS.get_or_init(|| chosen);
}

/// The set this process draws with: what `main` settled, or ASCII in a
/// process that never settled one — a unit test, which then reads the
/// same characters wherever it runs.
pub(crate) fn glyphs() -> Glyphs {
    GLYPHS.get().copied().unwrap_or(Glyphs::Ascii)
}

/// This process's color policy: what `main` settled, or the plain one a
/// process that never settled any — a unit test — gets.
fn policy() -> ColorPolicy {
    POLICY.get().cloned().unwrap_or(ColorPolicy {
        when: ColorWhen::Never,
        ..ColorPolicy::default()
    })
}

/// The width this process's reader asked for, if any; none in a process
/// that never settled one — a unit test.
fn columns() -> Option<usize> {
    COLUMNS.get().copied().flatten()
}

/// The ink stdout gets in this process.
pub(crate) fn stdout_ink() -> Ink {
    policy().ink(std::io::stdout().is_terminal())
}

/// The ink stderr gets in this process.
pub(crate) fn stderr_ink() -> Ink {
    policy().ink(std::io::stderr().is_terminal())
}

/// The width stdout gets in this process.
pub(crate) fn stdout_width() -> Width {
    Width::of(measured(&Term::stdout()), columns())
}

/// The width stderr gets in this process.
pub(crate) fn stderr_width() -> Width {
    Width::of(measured(&Term::stderr()), columns())
}

/// The width a row redrawn in place on a terminal `edge` cells wide
/// gets, in this process.
pub(crate) fn row_width(edge: usize) -> Width {
    Width::row(edge, columns())
}

/// The look stdout gets in this process.
pub(crate) fn stdout_look() -> Look {
    Look {
        glyphs: glyphs(),
        ink: stdout_ink(),
        width: stdout_width(),
    }
}

/// How wide `term` is, when it is a terminal at all.
fn measured(term: &Term) -> Option<usize> {
    term.size_checked().map(|(_, columns)| usize::from(columns))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_is_painted_exactly_when_the_stream_it_draws_on_is() {
        let never = ColorPolicy {
            when: ColorWhen::Never,
            ..ColorPolicy::default()
        };
        let no_color = ColorPolicy {
            no_color: true,
            ..ColorPolicy::default()
        };
        let always = ColorPolicy {
            when: ColorWhen::Always,
            ..ColorPolicy::default()
        };
        assert!(!paints(&never, true));
        assert!(!paints(&no_color, true));
        assert!(paints(&always, false));
        assert!(paints(&ColorPolicy::default(), true));
    }
}
