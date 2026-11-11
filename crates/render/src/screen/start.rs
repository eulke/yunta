//! See [`super`]. A run started, and the first one: what each asks, what stands
//! while a run is admitted and started, and their keys.

use surco::components::Legend;
use surco::line::{Line, Tone};
use yunta_core::{InputName, InputSpec};

use crate::prompt::ESC;

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
