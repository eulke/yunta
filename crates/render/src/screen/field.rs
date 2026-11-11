//! See [`super`]. The field's: what stands while the runs are read, the field's
//! keys, and what cancelling and closing runs ask before they do.

use surco::components::Legend;

/// What the application says while it reads the runs of the repository it opened in.
pub const READING_THE_RUNS: &str = "reading this repository's runs";

/// The keys of the field of runs: moving through it, opening the run pointed at or the
/// one waiting on the person, picking runs, the acts on the run pointed at or the ones
/// picked, how far it reads (`machine` when every run on the machine is listed) and how
/// deep (`closed` when the closed runs are), the workflows, and leaving.
pub fn field_legend(machine: bool, closed: bool) -> Legend {
    let reach = match machine {
        true => "this repository's runs",
        false => "every run on this machine",
    };
    let depth = match closed {
        true => "hides the closed",
        false => "the closed too",
    };
    Legend::new()
        .key("j k", "move")
        .key("enter", "opens the run")
        .key("n", "the run waiting on you")
        .key("space", "picks")
        .key("r", "resumes")
        .key("c", "cancels")
        .key("x", "closes")
        .key("a", reach)
        .key("d", depth)
        .key("w", "the workflows")
        .key("m", "checks this machine")
        .key("q", "leaves yunta")
}

/// What closing `runs`, by the handles a person reads, asks before it does.
pub fn closing(runs: &[&str]) -> String {
    format!(
        "close {} for good? A closed run is not resumed.",
        surco::text::listed(runs.iter().copied())
    )
}

/// What cancelling `run`, by its handle, asks before it does.
pub fn cancelling(run: &str) -> String {
    format!("cancel {run}? It stops, with everything under it.")
}
