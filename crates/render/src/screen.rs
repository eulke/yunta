//! What a screen of the application says around the pages it shows: the trail every
//! screen starts at, each stage's keys, and what a confirmation asks before an act.
//! The pages themselves are `page`'s.

use surco::components::Legend;
use surco::line::{Line, Tone};
use yunta_core::{InputName, InputSpec, NodeId};

use crate::prompt::ESC;

/// Where every screen's trail starts: the application itself.
pub const YUNTA: &str = "yunta";

/// What the application says while it reads the runs of the repository it opened in.
pub const READING_THE_RUNS: &str = "reading this repository's runs";

/// What the application says of a repository Yunta was never set up in.
pub const NEVER_SET_UP_HERE: &str = "Yunta was never set up in this repository";

/// Where starting a run stands on the trail.
pub const START: &str = "start";

/// What starting a run asks first: the workflow that builds what was asked for.
pub const WHICH_WORKFLOW: &str = "Which workflow builds it?";

/// What starting a run asks of a workflow that declares modes.
pub const WHICH_MODE: &str = "Which mode does it start in?";

/// What starting a run asks last: the adapter its sessions are pinned to.
pub const WHICH_ADAPTER: &str = "Which adapter runs its sessions?";

/// The adapter a run is started on when none is pinned: the one its runners name.
pub const AS_THE_RUNNERS_SAY: &str = "as the runners in config.yaml say";

/// What stands while a run is admitted, created and started.
pub const STARTING: &str = "admitting and starting the run";

/// What the first run asks: what the person wants built.
pub fn first_run_asks() -> Line {
    Line::new().push(Tone::Strong, "What do you want to build?")
}

/// The keys of the first run.
pub fn first_run_legend() -> Legend {
    Legend::new()
        .key("enter", "picks the workflow for it")
        .key(ESC, "leaves yunta")
}

/// What a run being started was asked for, over the questions that start it.
pub fn asked_for(request: &str) -> String {
    format!("asked: {request}")
}

/// What starting a run asks of input `name`: what it is, whether it is required, and
/// what the workflow says of it.
pub fn input_asks(name: &InputName, spec: &InputSpec) -> Line {
    let optionality = match spec.is_required() {
        true => "required",
        false => "optional",
    };
    let description = spec
        .description()
        .map(|description| format!(": {description}"))
        .unwrap_or_default();
    Line::new()
        .push(Tone::Strong, name.to_string())
        .plain(format!(
            " ({}, {optionality}){description}",
            crate::inputs::type_label(spec)
        ))
}

/// Where a decision stands on a run's trail.
pub const DECISION: &str = "decision";

/// What a confirmation is titled, over the screen it stands on.
pub const CONFIRM: &str = "confirm";

/// The keys of a screen with nothing to do but leave.
pub fn leaving_legend() -> Legend {
    Legend::new().key("q", "leaves yunta")
}

/// The keys of the field of runs: moving through it, opening the run pointed at or the
/// one waiting on the person, picking runs, the acts on the run pointed at or the ones
/// picked, how far it reads (`machine` when every run on the machine is listed) and how
/// deep (`closed` when the closed runs are), and leaving.
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
        .key("q", "leaves yunta")
}

/// The keys of a run: moving along its nodes and showing the page of the one `pointed`
/// at, each act the run offers (`acts`, the key each is bound to and what it does, in
/// the words `command` says it with), following it or not (`follows` when news of it
/// comes), going back to the run's page from a node's (`picked`) or to the runs, and
/// leaving.
pub fn run_legend(
    pointed: Option<&NodeId>,
    acts: &[(char, &str)],
    follows: bool,
    picked: bool,
) -> Legend {
    let mut legend = Legend::new();
    if let Some(node) = pointed {
        legend = legend
            .key("j k", "move along its nodes")
            .key("enter", format!("the page of {node}"));
    }
    for (key, does) in acts {
        legend = legend.key(key.to_string(), *does);
    }
    let following = match follows {
        true => "stops following",
        false => "follows",
    };
    let back = match picked {
        true => "back to the run",
        false => "back to the runs",
    };
    legend
        .key("f", following)
        .key(ESC, back)
        .key("q", "leaves yunta")
}

/// The keys of a confirmation.
pub fn confirmation_legend() -> Legend {
    Legend::new().key("enter", "confirms").key(ESC, "goes back")
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
