//! See [`super`]. What is done of this machine and this project, and the shapes of
//! the documents Yunta reads: where doctor, upkeep and the shapes stand on the trail,
//! what stands while doctor's checks and sessions run and while upkeep collects, what
//! opening a session per binding and collecting ask, and their keys.

use surco::components::Legend;

use crate::prompt::ESC;

/// Where what doctor checks of this machine stands on the trail.
pub const DOCTOR: &str = "doctor";

/// What stands while doctor checks this machine and this project.
pub const CHECKING_THE_MACHINE: &str = "checking this machine and its agent CLIs";

/// What stands while doctor opens a session on each agent binding.
pub const OPENING_SESSIONS: &str = "opening a session on each agent binding";

/// What is said of sessions stopped before every one of them ended.
pub const SESSIONS_STOPPED: &str = "stopped before every session ended";

/// What opening a session on each agent binding asks before it does.
pub const OPENING_SESSIONS_ASKS: &str = "open a session per binding? It spends a prompt each.";

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
/// or the glance again, a session per binding once the checks ended, upkeep, back to
/// the runs, or leaving; while the sessions run, stopping them, or leaving.
pub fn doctor_legend(full: bool, checks: DoctorChecks) -> Legend {
    if checks == DoctorChecks::OpeningSessions {
        return Legend::new()
            .key(ESC, "stops the sessions")
            .key("q", "leaves yunta");
    }
    let rows = match full {
        true => "folds it",
        false => "every check",
    };
    let legend = Legend::new().key("f", rows);
    let legend = match checks {
        DoctorChecks::Ended => legend.key("s", "a session per binding"),
        DoctorChecks::Running | DoctorChecks::OpeningSessions => legend,
    };
    legend
        .key("u", "what collecting would reclaim")
        .key(ESC, "back to the runs")
        .key("q", "leaves yunta")
}

/// Where collecting what runs left stands on the trail.
pub const UPKEEP: &str = "upkeep";

/// What stands while what collecting would reclaim is found.
pub const FINDING_WHAT_IT_RECLAIMS: &str = "finding what collecting would reclaim";

/// What stands while what runs left is collected.
pub const COLLECTING: &str = "collecting what runs left";

/// What is said of a collection stopped before it ended.
pub const COLLECTION_STOPPED: &str = "stopped before the collection ended";

/// What collecting asks before it does.
pub const COLLECTING_ASKS: &str =
    "collect what the dry run found? What it lists is removed for good.";

/// The keys of upkeep: collecting what a dry run found (`would`), back to doctor, or
/// leaving; while it collects (`collecting`), stopping it, or leaving.
pub fn upkeep_legend(would: bool, collecting: bool) -> Legend {
    if collecting {
        return Legend::new().key(ESC, "stops it").key("q", "leaves yunta");
    }
    let legend = match would {
        true => Legend::new().key("c", "collects it"),
        false => Legend::new(),
    };
    legend.key(ESC, "back to doctor").key("q", "leaves yunta")
}

/// Where the shapes of the documents Yunta reads stand on the trail.
pub const SCHEMA: &str = "schema";

/// What the list of the documents Yunta reads asks.
pub const WHICH_DOCUMENT: &str = "Which document's shape?";

/// The keys of a document's shape: back to the documents, or leaving.
pub fn shape_legend() -> Legend {
    Legend::new()
        .key(ESC, "back to the documents")
        .key("q", "leaves yunta")
}
