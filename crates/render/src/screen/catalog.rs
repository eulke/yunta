//! See [`super`]. The workflows' and the packs': where each stands on the trail,
//! what stands while each is read or worked on, their keys, and what a new workflow
//! or pack asks for. What installing or removing a pack asks before it does is a
//! confirmation's, worded with every other in [`super::Confirming`].

use surco::components::Legend;
use surco::line::{Line, Tone};

use super::{BACK, NEW_RUN};
use crate::prompt::{Escape, PromptAct, ENTER, ESC};

/// Where the workflows a repository can run stand on the trail.
pub const WORKFLOWS: &str = "workflows";

/// What the workflows a repository can run are listed under on their screen.
pub const WORKFLOWS_YOU_CAN_RUN: &str = "the workflows you can run";

/// Where a workflow's check stands on the trail, after the workflow's name.
pub const CHECK: &str = "check";

/// What stands while the workflows a repository can run are read.
pub const READING_THE_WORKFLOWS: &str = "reading the workflows this repository can run";

/// What the line that stands while a workflow is checked begins with, before its name.
pub(super) const CHECKING: &str = "checking";

/// What stands while the workflow `name` is checked.
pub fn checking(name: &str) -> String {
    format!("{CHECKING} {name}")
}

/// The keys of the workflows: moving through them, a new run of the one pointed at where one
/// `asks_for_a_run` here, a new workflow, the cases, the packs, the shapes of the documents
/// Yunta reads, and back. What the one pointed at offers beside it, its check, its graph and
/// its past runs, its panel says ([`workflow_acts`]).
pub fn catalog_legend(asks_for_a_run: bool) -> Legend {
    let legend = Legend::new().key("j k", super::MOVE);
    let legend = match asks_for_a_run {
        true => legend.key(ENTER, NEW_RUN),
        false => legend,
    };
    legend
        .key("n", NEW_WORKFLOW)
        .key("t", CASES)
        .key("p", PACKS)
        .key("s", "shapes")
        .key(ESC, BACK)
}

/// What can be done with the workflow pointed at, each act with its key, as its panel lists
/// them: a new run of it where it `reads` and one `asks_for_a_run` here, its check, its
/// graph, in the words a run's palette draws a run's graph with, and its past runs where it
/// reads.
pub fn workflow_acts(reads: bool, asks_for_a_run: bool) -> Legend {
    let legend = match reads && asks_for_a_run {
        true => Legend::new().key(ENTER, "a new run of it"),
        false => Legend::new(),
    };
    let legend = legend
        .key("c", "check it")
        .key("g", crate::command::DRAW_ITS_GRAPH);
    match reads {
        true => legend.key("h", "its past runs"),
        false => legend,
    }
}

/// Where a new workflow stands on the trail, and what `n` on the workflows asks for.
pub const NEW_WORKFLOW: &str = "new workflow";

/// What the catalog asks a new workflow's name with: the stem of its file.
pub fn new_workflow_asks() -> Line {
    Line::new().push(Tone::Strong, "What is the new workflow called?")
}

/// The keys of the shapes a new workflow starts as, while one is picked, what is typed
/// narrowing them: the arrows along them, Enter writing the workflow in the shape pointed at,
/// and Escape taking the first, the plainest.
pub fn shapes_legend() -> Legend {
    Legend::new()
        .arrows(super::MOVE)
        .key(ENTER, super::setup::WRITE_IT)
        .key(ESC, Escape::TakesTheFirst.does())
}

/// The keys a page standing in the workflows' place takes beyond its legend where Enter
/// goes back to them as Escape does, which help says: a workflow's check, the cases once
/// they ended, a workflow written, and what the packs show once their work ended.
pub fn enter_goes_back() -> Legend {
    Legend::new().key(ENTER, BACK)
}

/// The keys of a new workflow's name: it is typed, Enter goes on to the shape its skeleton
/// takes, and Escape goes back to the workflows.
pub fn naming_legend() -> Legend {
    Legend::new()
        .typing("the name")
        .key(ENTER, "choose its shape")
        .key(ESC, BACK)
}

/// What stands while the new workflow `name` is written and checked.
pub fn writing_workflow(name: &str) -> String {
    format!("writing {name}")
}

/// Where the packs stand on the trail.
pub const PACKS: &str = "packs";

/// Where a new pack stands on the trail, and what `n` on the packs asks for.
pub const NEW_PACK: &str = "new pack";

/// Where a pack being installed stands on the trail before it is known.
pub const INSTALL: &str = "install";

/// What stands while the packs installed are read.
pub const READING_THE_PACKS: &str = "reading the packs installed";

/// The keys of the packs: moving through them, auditing, updating or removing the
/// one pointed at, installing one, a new one, and back to the workflows.
pub fn packs_legend() -> Legend {
    Legend::new()
        .key("j k", super::MOVE)
        .key("a", "audit it")
        .key("u", "update it")
        .key("x", PromptAct::RemoveIt.said())
        .key("i", "install one")
        .key("n", NEW_PACK)
        .key(ESC, BACK)
}

/// The keys of a line the packs ask for, a new pack's name, where a pack is or the ref it
/// moves to: Enter answers, Escape goes back to the packs.
pub fn pack_naming_legend() -> Legend {
    Legend::new()
        .key(ENTER, PromptAct::Answer.said())
        .key(ESC, BACK)
}

/// What the packs ask a new pack's name with: its publisher and its name.
pub fn new_pack_asks() -> Line {
    Line::new()
        .push(Tone::Strong, "What is the new pack called?")
        .push(Tone::Muted, " (publisher/name)")
}

/// What the line that stands while a pack is audited begins with, before the pack.
pub(super) const AUDITING: &str = "auditing";

/// What stands while `pack` is audited, its own cases run on it.
pub fn auditing(pack: &str) -> String {
    format!("{AUDITING} {pack} and running its cases")
}

/// What stands while the new pack `pack` is written and checked.
pub fn scaffolding(pack: &str) -> String {
    format!("writing {pack} and running its case")
}

/// What the packs ask a pack to install with: where it is.
pub fn pack_source_asks() -> Line {
    Line::new()
        .push(Tone::Strong, "Where is the pack?")
        .push(Tone::Muted, " (a path or a URL, @ref to pin one)")
}

/// What updating `pack` asks: the ref to move it to.
pub fn pack_reference_asks(pack: &str) -> Line {
    Line::new().push(Tone::Strong, format!("Which ref of {pack}?"))
}

/// What the line that stands while a pack is fetched begins with, before what is fetched.
pub(super) const FETCHING: &str = "fetching";

/// What stands while `what`, a source or a pack, is fetched.
pub fn fetching(what: &str) -> String {
    format!("{FETCHING} {what}")
}

/// The keys of a pack fetched, its inventory read: installing it, or back to the packs,
/// letting it go.
pub fn fetched_legend() -> Legend {
    let own = Legend::new().key(ENTER, PromptAct::InstallIt.said());
    super::page_legend(super::PageRead::Whole, own, false)
}

/// What stands while `pack` is installed from its copy.
pub fn installing_pack(pack: &str) -> String {
    format!("installing {pack}")
}

/// What an install came to where the copy it was to vendor was let go first: the
/// pack is fetched again to install it.
pub const PACK_LET_GO: &str = "the pack fetched was let go before it was installed";

/// Where the cases stand on the trail.
pub const CASES: &str = "cases";

/// What stands under the cases that ended while the others run.
pub const RUNNING_THE_CASES: &str = "running the cases";

/// What is said of cases stopped before every one of them ran.
pub const CASES_STOPPED: &str = "stopped before every case ran";

/// The keys while the cases run: stopping them. What every page answers to, `q` going down
/// to the field and `n` a new run, each letting them go, help says.
pub fn cases_legend() -> Legend {
    Legend::new().key(ESC, PromptAct::StopThem.said())
}
