//! See [`super`]. A run's and what it leaves and costs: where its receipt, its
//! verification, its page, its stats and its graph stand on the trail, what stands
//! while each is read, and the keys of the run and of each page over it.

use surco::components::Legend;
use yunta_core::NodeId;

use crate::prompt::ESC;

/// Where a run's receipt stands on the trail.
pub const RECEIPT: &str = "receipt";

/// Where a run's verification stands on the trail.
pub const VERIFICATION: &str = "verification";

/// Where a run's page for a browser stands on the trail.
pub const PAGE: &str = "page";

/// What stands while a run's receipt is derived and written beside it.
pub const WRITING_THE_RECEIPT: &str = "writing the run's receipt";

/// What stands while a run's log and objects are verified.
pub const VERIFYING: &str = "verifying the run's log and what it names";

/// What stands while a run's page for a browser is written.
pub const WRITING_THE_PAGE: &str = "writing the run's page for a browser";

/// The keys of a page opened over a run, what it leaves or how it is drawn: back
/// to the run, or leaving.
pub fn over_the_run_legend() -> Legend {
    Legend::new()
        .key(ESC, "back to the run")
        .key("q", "leaves yunta")
}

/// The keys of a run's page written for a browser: opening it again, back to the
/// run, or leaving.
pub fn opened_legend() -> Legend {
    Legend::new()
        .key("enter", "opens it again")
        .key(ESC, "back to the run")
        .key("q", "leaves yunta")
}

/// Where what a run cost stands on the trail.
pub const STATS: &str = "stats";

/// Where what a workflow's past runs cost stands on the trail.
pub const HISTORY: &str = "history";

/// Where a workflow's graph stands on the trail.
pub const GRAPH: &str = "graph";

/// What stands while what a run cost is read.
pub const READING_THE_STATS: &str = "reading what the run cost";

/// What stands while a workflow's past runs are read.
pub const READING_THE_HISTORY: &str = "reading the workflow's past runs";

/// What stands while a workflow is read to be drawn.
pub const READING_THE_GRAPH: &str = "reading the workflow to draw it";

/// The keys of what a run cost: every row of it, as `--full` shows it, or the
/// glance again; back to the run, or leaving.
pub fn stats_legend(full: bool) -> Legend {
    let rows = match full {
        true => "folds it",
        false => "every row",
    };
    Legend::new()
        .key("f", rows)
        .key(ESC, "back to the run")
        .key("q", "leaves yunta")
}

/// Where a decision stands on a run's trail.
pub const DECISION: &str = "decision";

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
