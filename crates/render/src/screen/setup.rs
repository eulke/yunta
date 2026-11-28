//! See [`super`]. Setting a repository up: where it stands on the trail, what stands
//! while it is found and written, what writing it touches, said before it is confirmed and
//! once it is written, and its keys, by the row pointed at: the row that writes, a row that
//! changes, a role's adapter being chosen, a line being typed in place. What it says of the
//! repository is `page::Setup`'s.
//!
//! What writing touches is said from the paths it writes, never as a sentence of its own:
//! everything stands under `.yunta/` but the paths named, so a sentence says `.gitignore`
//! only where the write names it.

use surco::components::Legend;

use super::named;
use crate::page::Setup;
use crate::prompt::{Escape, PromptAct, ENTER, ESC};

/// Where setting a repository up stands on the trail.
pub const SETUP: &str = "set up";

/// What `q` does on a setup: it leaves the application with nothing written, since nothing
/// is until the row that writes is taken.
const LEAVES_NOTHING_WRITTEN: &str = "leave, nothing written";

/// What `enter` does on a setup's row that writes, and on the shapes a new workflow is asked
/// for: what was chosen written.
pub(super) const WRITE_IT: &str = "write it";

/// What stands while setting a repository up finds what it runs.
pub const FINDING_WHAT_IT_RUNS: &str =
    "finding what this repository runs, and the adapters this machine answers for";

/// What stands while a repository's setup is written.
pub const WRITING_THE_SETUP: &str = "writing the setup";

/// What a setup's writing came to when what was found for it was let go first: it is
/// found again, and asked again.
pub const SETUP_LET_GO: &str =
    "what setting this repository up found was let go before it was written";

/// The directory a setup writes under, as a path in the repository starts with it: what a
/// setup writes beyond it is named.
const YUNTA_DIRECTORY: &str = ".yunta/";

/// What setting a repository up with `pack` does, as its confirmation says it before: the
/// configuration written and the pack installed, and nothing touched outside `.yunta/` but
/// the paths of `writes`, every path the setup writes by its path in the repository, that
/// stand there.
pub(super) fn setting_up_with(pack: &str, writes: &[String]) -> String {
    format!(
        "It writes {} and installs {pack}. Nothing outside {YUNTA_DIRECTORY} is touched{}.",
        Setup::CONFIG,
        but(writes)
    )
}

/// What writing a setup did, as the note once it is written says it: the configuration
/// written, and `installed`, the pack installed with it, where one was; and nothing touched
/// outside `.yunta/` but the paths of `wrote`, every path it wrote by its path in the
/// repository, that stand there: `.gitignore` only where it did not hold yunta's lines
/// already.
pub fn wrote_the_setup(installed: Option<&str>, wrote: &[String]) -> String {
    let installed = installed
        .map(|pack| format!(" and installed {pack}"))
        .unwrap_or_default();
    format!(
        "Wrote {}{installed}. Nothing outside {YUNTA_DIRECTORY} was touched{}.",
        Setup::CONFIG,
        but(wrote)
    )
}

/// ` but` and the paths of `paths` outside `.yunta/`, named; nothing where none is.
fn but(paths: &[String]) -> String {
    let outside: Vec<String> = paths
        .iter()
        .filter(|path| !path.starts_with(YUNTA_DIRECTORY))
        .cloned()
        .collect();
    match outside.is_empty() {
        true => String::new(),
        false => format!(" but {}", named(&outside)),
    }
}

/// The keys of a setup with its pointer on the row that writes: `j k` step along its rows,
/// `enter` writes it, and `q` leaves with nothing written, since nothing is until `enter`.
pub fn setup_legend() -> Legend {
    Legend::new()
        .key("j k", "step")
        .key(ENTER, WRITE_IT)
        .key("q", LEAVES_NOTHING_WRITTEN)
}

/// The keys of the audit of the pack a setup installs, read before it is: the arrows scroll
/// it, `enter` asks to install it, and Escape goes back to the setup's rows, nothing written.
pub fn setup_audit_legend() -> Legend {
    let own = Legend::new().key(ENTER, PromptAct::InstallIt.said());
    super::page_legend(super::PageRead::Scrolled, own, false)
}

/// The keys the audit of the pack a setup installs takes beyond its legend, which help says:
/// `q` leaves with nothing written, as on the setup's rows.
pub fn setup_audit_keys_beyond() -> Legend {
    Legend::new().key("q", LEAVES_NOTHING_WRITTEN)
}

/// The keys of a setup with its pointer on a row that changes: `j k` step along its rows,
/// `enter` opens the row to change it in place, and `q` leaves with nothing written.
pub fn setup_row_legend() -> Legend {
    Legend::new()
        .key("j k", "step")
        .key(ENTER, PromptAct::ChangeIt.said())
        .key("q", LEAVES_NOTHING_WRITTEN)
}

/// The keys of a role whose adapter is being chosen in place: the arrows choose among the
/// adapters this machine answers for, `enter` takes the one chosen and goes on to its model,
/// and Escape gives the row back as it stood, `kept`.
pub fn choosing_legend(kept: Kept) -> Legend {
    Legend::new()
        .left_right("adapter")
        .key(ENTER, PromptAct::TakeIt.said())
        .key(ESC, kept.escape().does())
}

/// The keys of a line typed in place, the `row`'s value (a name, a branch, a model): `enter`
/// takes what is typed, and Escape gives the row back as it stood, `kept`.
pub fn typing_legend(row: &str, kept: Kept) -> Legend {
    Legend::new()
        .typing(format!("the {row}"))
        .key(ENTER, PromptAct::TakeIt.said())
        .key(ESC, kept.escape().does())
}

/// How a setup's row being changed stood before it was opened, which Escape gives back: as
/// detected, or as a person changed it before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kept {
    Detected,
    Changed,
}

impl Kept {
    /// What Escape comes to on a row that stood so.
    fn escape(self) -> Escape {
        match self {
            Kept::Detected => Escape::KeepsDefault,
            Kept::Changed => Escape::KeepsItAsItWas,
        }
    }
}

/// The keys of a setup written: on to the first request, which asks what to build, or
/// quitting.
pub fn written_legend() -> Legend {
    Legend::new()
        .key(ENTER, "on to what to build")
        .key("q", PromptAct::Quit.said())
}
