//! What a screen of the application says around the pages it shows: the trail every
//! screen starts at, each stage's keys, and what a confirmation asks before an act.
//! The pages themselves are `page`'s.
//!
//! Each family of stages says its own in a module of its own: `field`, `catalog` (the
//! workflows and the packs), `run` (a run and what it leaves and costs), `setup`,
//! `start` (a run started, and the first one) and `machine` (doctor, upkeep and the
//! shapes of the documents Yunta reads). What every
//! screen shares stays here: the trail's start, a confirmation's, the palette's,
//! help's, leaving, and the keys that scroll a page.

mod catalog;
mod field;
mod machine;
mod run;
mod setup;
mod start;

pub use catalog::{
    auditing, cases_legend, catalog_legend, checking, executing, fetched_legend, fetching,
    installing_pack, naming_legend, new_pack_asks, new_workflow_asks, over_the_workflows_legend,
    pack_naming_legend, pack_page_legend, pack_reference_asks, pack_source_asks, packs_legend,
    removing_pack, scaffolding, writing_workflow, CASES, CASES_STOPPED, INSTALL, NEW, PACKS,
    PACK_LET_GO, READING_THE_PACKS, READING_THE_WORKFLOWS, RUNNING_THE_CASES, WORKFLOWS,
};
pub use field::{cancelling, closing, field_legend, READING_THE_RUNS};
pub use machine::{
    doctor_legend, shape_legend, upkeep_legend, DoctorChecks, CHECKING_THE_MACHINE, COLLECTING,
    COLLECTING_ASKS, COLLECTION_STOPPED, DOCTOR, FINDING_WHAT_IT_RECLAIMS, OPENING_SESSIONS,
    OPENING_SESSIONS_ASKS, SCHEMA, SESSIONS_STOPPED, UPKEEP, WHICH_DOCUMENT,
};
pub use run::{
    opened_legend, over_the_run_legend, run_legend, stats_legend, DECISION, GRAPH, HISTORY, PAGE,
    READING_THE_GRAPH, READING_THE_HISTORY, READING_THE_STATS, RECEIPT, STATS, VERIFICATION,
    VERIFYING, WRITING_THE_PAGE, WRITING_THE_RECEIPT,
};
pub use setup::{
    setup_legend, written_legend, FINDING_WHAT_IT_RUNS, SETUP, SETUP_LET_GO, WRITING_THE_SETUP,
};
pub use start::{
    asked_for, first_run_asks, first_run_legend, input_asks, AS_THE_RUNNERS_SAY, START, STARTING,
    WHICH_ADAPTER, WHICH_MODE, WHICH_WORKFLOW,
};

use surco::components::{Cell, Column, Holds, Legend, Row, Table};
use surco::doc::Doc;
use surco::glyphs::Glyphs;

use crate::prompt::ESC;

/// Where every screen's trail starts: the application itself.
pub const YUNTA: &str = "yunta";

/// What a piece of work that broke inside Yunta comes to, with what it said as it
/// broke, where it said anything.
pub fn broke_inside(said: &str) -> String {
    match said.is_empty() {
        true => "this broke inside yunta".to_string(),
        false => format!("this broke inside yunta: {said}"),
    }
}

/// What a confirmation is titled, over the screen it stands on.
pub const CONFIRM: &str = "confirm";

/// What the command a confirmation names does: what is confirmed, at a shell.
pub const DOES_THE_SAME: &str = "does the same at a shell";

/// The keys of a screen with nothing to do but leave.
pub fn leaving_legend() -> Legend {
    Legend::new().key("q", "leaves yunta")
}

/// The keys of a confirmation.
pub fn confirmation_legend() -> Legend {
    Legend::new().key("enter", "confirms").key(ESC, "goes back")
}

/// What the palette is titled, over the screen it stands on: every act from where the
/// person stands, and every act always there.
pub const PALETTE: &str = "what you can do here";

/// `legend` with the keys every stage answers to after its own: the palette, and help.
pub fn over_every_stage(legend: Legend) -> Legend {
    legend
        .key("/", "what you can do here")
        .key("?", "the keys here")
}

/// What help is titled, over the screen it stands on.
pub const HELP: &str = "the keys here";

/// Every key `legend` names, each with what it does, then the keys every stage answers
/// to, Ctrl-C leaving from anywhere: a row each, the keys spelled in `glyphs`.
pub fn help(legend: &Legend, glyphs: Glyphs) -> Doc {
    let every = over_every_stage(Legend::new()).key("ctrl-c", "leaves yunta");
    let rows = legend
        .0
        .iter()
        .chain(&every.0)
        .map(|binding| Row {
            mark: None,
            cells: vec![
                Cell::plain(binding.keys.spelled(glyphs)),
                Cell::plain(binding.does.clone()),
            ],
        })
        .collect();
    let columns = vec![
        Column {
            title: "key",
            holds: Holds::Id,
        },
        Column {
            title: "what it does",
            holds: Holds::Prose,
        },
    ];
    Doc::new().with(Table { columns, rows })
}

/// The keys of help: scrolling it, and closing it.
pub fn help_legend() -> Legend {
    Legend::new().arrows("scroll").key(ESC, "closes it")
}

/// `legend` led by the keys that scroll a page taller than what shows of it: a line
/// with the arrows, a page with PageUp and PageDown.
pub fn scrolling(legend: Legend) -> Legend {
    led_by(
        Legend::new()
            .arrows("scroll")
            .key("pgup pgdn", "by the page"),
        legend,
    )
}

/// `legend` led by the keys that scroll a run's page beside its nodes, the arrows
/// moving along the nodes: a page with PageUp and PageDown.
pub fn scrolling_beside(legend: Legend) -> Legend {
    led_by(Legend::new().key("pgup pgdn", "scroll its page"), legend)
}

/// `rest` after the keys of `first`.
fn led_by(first: Legend, rest: Legend) -> Legend {
    let Legend(mut keys) = first;
    keys.extend(rest.0);
    Legend(keys)
}
