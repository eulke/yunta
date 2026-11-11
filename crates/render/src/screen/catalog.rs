//! See [`super`]. The workflows' and the packs': where each stands on the trail,
//! what stands while each is read or worked on, their keys, what a new workflow
//! or pack asks for, and what installing or removing a pack asks before it does.

use surco::components::Legend;
use surco::line::{Line, Tone};

use crate::prompt::ESC;

/// Where the workflows a repository can run stand on the trail.
pub const WORKFLOWS: &str = "workflows";

/// What stands while the workflows a repository can run are read.
pub const READING_THE_WORKFLOWS: &str = "reading the workflows this repository can run";

/// What stands while the workflow `name` is checked.
pub fn checking(name: &str) -> String {
    format!("checking {name}")
}

/// The keys of the workflows: moving through them, running the one pointed at,
/// checking it, drawing it or reading its past runs, a new one, the cases, the
/// packs, the shapes of the documents Yunta reads, back to the runs, and leaving.
pub fn catalog_legend() -> Legend {
    Legend::new()
        .key("j k", "move")
        .key("enter", "runs it")
        .key("c", "checks it")
        .key("g", "draws it")
        .key("h", "its past runs")
        .key("n", "a new workflow")
        .key("t", "runs the cases")
        .key("p", "the packs")
        .key("s", "the documents' shapes")
        .key(ESC, "back to the runs")
        .key("q", "leaves yunta")
}

/// Where a new workflow stands on the trail.
pub const NEW: &str = "new";

/// What the catalog asks a new workflow's name with: the stem of its file.
pub fn new_workflow_asks() -> Line {
    Line::new().push(Tone::Strong, "What is the new workflow called?")
}

/// The keys of a new workflow's name: Enter answers, Escape goes back to the
/// workflows.
pub fn naming_legend() -> Legend {
    Legend::new()
        .key("enter", "answers")
        .key(ESC, "back to the workflows")
}

/// What stands while the new workflow `name` is written and checked.
pub fn writing_workflow(name: &str) -> String {
    format!("writing {name}")
}

/// Where the packs stand on the trail.
pub const PACKS: &str = "packs";

/// Where a pack being installed stands on the trail before it is known.
pub const INSTALL: &str = "install";

/// What stands while the packs installed are read.
pub const READING_THE_PACKS: &str = "reading the packs installed";

/// The keys of the packs: moving through them, auditing, updating or removing the
/// one pointed at, installing one, a new one, back to the workflows, and leaving.
pub fn packs_legend() -> Legend {
    Legend::new()
        .key("j k", "move")
        .key("a", "audits it")
        .key("u", "updates it")
        .key("x", "removes it")
        .key("i", "installs one")
        .key("n", "a new pack")
        .key(ESC, "back to the workflows")
        .key("q", "leaves yunta")
}

/// The keys of a page the packs show, an audit or a new pack: back to the packs,
/// or leaving.
pub fn pack_page_legend() -> Legend {
    Legend::new()
        .key(ESC, "back to the packs")
        .key("q", "leaves yunta")
}

/// The keys of a new pack's name: Enter answers, Escape goes back to the packs.
pub fn pack_naming_legend() -> Legend {
    Legend::new()
        .key("enter", "answers")
        .key(ESC, "back to the packs")
}

/// What the packs ask a new pack's name with: its publisher and its name.
pub fn new_pack_asks() -> Line {
    Line::new()
        .push(Tone::Strong, "What is the new pack called?")
        .push(Tone::Muted, " (publisher/name)")
}

/// What stands while `pack` is audited, its own cases run on it.
pub fn auditing(pack: &str) -> String {
    format!("auditing {pack} and running its cases")
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

/// What stands while `what`, a source or a pack, is fetched.
pub fn fetching(what: &str) -> String {
    format!("fetching {what}")
}

/// The keys of a pack fetched, its inventory read: installing it, or letting it go.
pub fn fetched_legend() -> Legend {
    Legend::new()
        .key("enter", "installs it")
        .key(ESC, "lets it go")
}

/// What stands while `pack` is installed from its copy.
pub fn installing_pack(pack: &str) -> String {
    format!("installing {pack}")
}

/// What installing `pack` asks before it does, where what it executes waits on a
/// person: each executor it ships, named.
pub fn executing(pack: &str, executors: &[String]) -> String {
    format!(
        "install {pack}? It ships executors it runs: {}.",
        surco::text::listed(executors.iter().map(String::as_str))
    )
}

/// What an install came to where the copy it was to vendor was let go first: the
/// pack is fetched again to install it.
pub const PACK_LET_GO: &str = "the pack fetched was let go before it was installed";

/// What removing `pack` asks before it does.
pub fn removing_pack(pack: &str) -> String {
    format!("remove {pack} from this project? Its files and its lock entry go.")
}

/// Where the cases stand on the trail.
pub const CASES: &str = "cases";

/// What stands under the cases that ended while the others run.
pub const RUNNING_THE_CASES: &str = "running the cases";

/// What is said of cases stopped before every one of them ran.
pub const CASES_STOPPED: &str = "stopped before every case ran";

/// The keys while the cases run: stopping them, or leaving.
pub fn cases_legend() -> Legend {
    Legend::new()
        .key(ESC, "stops them")
        .key("q", "leaves yunta")
}

/// The keys of a page shown over the workflows, a check, a new workflow, the
/// cases once they ended, a workflow's past runs or its graph: back to the
/// workflows, or leaving.
pub fn over_the_workflows_legend() -> Legend {
    Legend::new()
        .key(ESC, "back to the workflows")
        .key("q", "leaves yunta")
}
