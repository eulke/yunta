//! See [`super`]. What is done of this machine and this project, and the shapes of
//! the documents Yunta reads: where the machine, upkeep under it and the shapes stand on
//! the trail, what stands while doctor's checks and sessions run and while upkeep collects,
//! and their keys. What opening a session per binding and collecting ask before they do is
//! a confirmation's, worded with every other in [`super::Confirming`]. Upkeep's `u` and
//! leaving are keys out of the machine's legend: the palette offers upkeep.

use surco::components::Legend;
use surco::line::{Line, Tone};
use yunta_core::step::NextStep;

use super::{PageRead, BACK};
use crate::prompt::{PromptAct, ENTER, ESC};

/// Where what doctor checks of this machine stands on the trail: the machine, a page
/// reached from the field.
pub const MACHINE: &str = "machine";

/// What a screen names this machine by: where the field stands on the trail when it lists
/// every run on it, and what the machine's page says it checked, in its headline.
pub(crate) const THIS_MACHINE: &str = "this machine";

/// What stands while doctor checks this machine and this project.
pub const CHECKING_THE_MACHINE: &str = "checking this machine and its agent CLIs";

/// What stands while doctor opens a session on each agent binding.
pub const OPENING_SESSIONS: &str = "opening a session on each agent binding";

/// What is said of sessions stopped before every one of them ended.
pub const SESSIONS_STOPPED: &str = "stopped before every session ended";

/// What doctor's checks are doing, which the keys of its page answer to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorChecks {
    /// They ended: what they found stands.
    Ended,
    /// They run, with no session.
    Running,
    /// They run, with a session per binding.
    OpeningSessions,
}

/// The keys of doctor's findings: every check on its own row, as `--full` shows them,
/// or the glance again, a session per binding once the checks ended, and back; while the
/// sessions run, stopping them.
pub fn doctor_legend(full: bool, checks: DoctorChecks) -> Legend {
    if checks == DoctorChecks::OpeningSessions {
        return Legend::new().key(ESC, PromptAct::StopTheSessions.said());
    }
    let rows = match full {
        true => "folds it",
        false => "every check",
    };
    let own = Legend::new().key("f", rows);
    let own = match checks {
        DoctorChecks::Ended => own.key("s", "a session per binding"),
        DoctorChecks::Running | DoctorChecks::OpeningSessions => own,
    };
    super::page_legend(PageRead::Whole, own, false)
}

/// The keys the machine takes beyond its legend, which help says: `u` opens upkeep, what
/// collecting would reclaim, where no session per binding runs (`sessions`).
pub fn machine_keys_beyond(sessions: bool) -> Legend {
    let upkeep = NextStep::Gc { dry_run: true };
    match sessions {
        true => Legend::new(),
        false => Legend::new().key("u", crate::command::does(&upkeep)),
    }
}

/// Where collecting what runs left stands on the trail, under the machine, and what its
/// page's headline names.
pub const UPKEEP: &str = "upkeep";

/// What stands while what collecting would reclaim is found.
pub const FINDING_WHAT_IT_RECLAIMS: &str = "finding what collecting would reclaim";

/// What stands while what runs left is collected.
pub const COLLECTING: &str = "collecting what runs left";

/// What is said of a collection stopped before it ended.
pub const COLLECTION_STOPPED: &str = "stopped before the collection ended";

/// The keys of upkeep: collecting what a dry run found (`would`), and back; while it
/// collects (`collecting`), stopping it.
pub fn upkeep_legend(would: bool, collecting: bool) -> Legend {
    if collecting {
        return Legend::new().key(ESC, PromptAct::StopIt.said());
    }
    let own = match would {
        true => Legend::new().key("c", PromptAct::CollectIt.said()),
        false => Legend::new(),
    };
    super::page_legend(PageRead::Whole, own, false)
}

/// Where the shapes of the documents Yunta reads stand on the trail.
pub const SCHEMA: &str = "schema";

/// What the list of the documents Yunta reads asks, over it.
pub fn which_document_asks() -> Line {
    Line::new().push(Tone::Strong, "Which document's shape?")
}

/// The keys of the documents Yunta reads while one is picked, what is typed narrowing them:
/// the arrows along them, Enter its shape, and back.
pub fn documents_legend() -> Legend {
    Legend::new()
        .arrows(super::MOVE)
        .key(ENTER, "its shape")
        .key(ESC, BACK)
}
