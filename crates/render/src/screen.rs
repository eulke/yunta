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
//!
//! A key is said in a legend by a word or two (`s stats`) and in help by a sentence
//! (`s g  what it cost, its graph`). The keys a person presses anywhere they are not
//! typing, the palette and help, are [`anywhere`], drawn at the legend's far end apart
//! from the screen's own keys; [`over_every_stage`] adds them after the keys of a stage
//! that draws no far end.

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
    answered_elsewhere, answers_recorded, help_over_a_run, in_the_legend, in_the_runs_frame_legend,
    opened_legend, over_the_run_legend, place_of, recorded, round_answered_elsewhere, run_legend,
    unread_run_legend, Along, Leaving, PromptKeys, RunKeys, CHRONICLE, GRAPH, HISTORY, MAKE_ROOM,
    NOTHING_RECORDED, OPEN_THE_PULL_REQUEST, PAGE, READING_THE_CHRONICLE, READING_THE_GRAPH,
    READING_THE_HISTORY, READING_THE_STATS, RECEIPT, STATS, VERIFICATION, VERIFYING,
    WRITING_THE_PAGE, WRITING_THE_RECEIPT,
};
pub use setup::{
    setup_legend, written_legend, FINDING_WHAT_IT_RUNS, SETUP, SETUP_LET_GO, WRITING_THE_SETUP,
};
pub use start::{
    asked_for, first_run_asks, first_run_legend, input_asks, AS_THE_RUNNERS_SAY, START, STARTING,
    WHICH_ADAPTER, WHICH_MODE, WHICH_WORKFLOW,
};

use surco::components::{KeyList, Legend};
use surco::doc::Doc;
use surco::words::Standing;

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

/// What a terminal too small for the application says, alone on it: how much room the
/// application needs, and how much the terminal has, each as columns by rows.
pub fn room_needed(least: (usize, usize), has: (usize, usize)) -> String {
    format!(
        "the application needs a terminal of {} by {} at least, and this one is {} by {}: \
         make it larger, or press q to leave",
        least.0, least.1, has.0, has.1
    )
}

/// What a confirmation is titled, over the screen it stands on.
pub const CONFIRM: &str = "confirm";

/// What the command a confirmation names does: what is confirmed, at a shell.
pub const DOES_THE_SAME: &str = "does the same at a shell";

/// The keys of a screen with nothing to do but leave.
pub fn leaving_legend() -> Legend {
    Legend::new().key("q", "leaves yunta")
}

/// What the palette is titled, over the screen it stands on: every act from where the
/// person stands, and every act always there.
pub const PALETTE: &str = "what do you want to do?";

/// The keys a person presses anywhere they are not typing, at the legend's far end: the
/// palette, and help. Not a screen's legend, the keys of what is on it.
pub fn anywhere() -> Legend {
    Legend::new().key("/", "anything").key("?", "keys")
}

/// `legend` with the keys every stage answers to after its own: the palette, and help.
/// For a stage that draws no far end to its legend, where [`anywhere`] stands.
pub fn over_every_stage(legend: Legend) -> Legend {
    legend
        .key("/", "what you can do here")
        .key("?", "the keys here")
}

/// What help is titled, over the screen it stands on.
pub const HELP: &str = "keys";

/// What help says above the keys it lists: they are every key there is.
pub const NOTHING_MORE_TO_REMEMBER: &str = "You never have to remember more than this.";

/// What going back does, where a key says it in a word: closing what stands over a
/// screen, or leaving a page, a plan's desk among them, for what it was opened from.
pub const BACK: &str = "back";

/// What `n` does where a legend names it: a new run.
pub(crate) const NEW_RUN: &str = "new run";

/// What leaving the application leaves: every run, going on without it. Help says it
/// beside Ctrl-C, and beside `q` where `q` leaves.
pub(crate) const LEAVE: &str = "leave: every run keeps going without this window";

/// What help says `/` does, on every screen.
pub(crate) const ANYTHING: &str = "anything: only what can be done here, with its command";

/// What help says `n` does, on every screen.
pub(crate) const STARTS_A_RUN: &str = "start a new run, wherever no prompt stands";

/// What help says `j k` do, on every screen: the legend says what they move there.
pub(crate) const MOVES: &str = "move; the footer always says what they move";

/// What help says `enter` does, on every screen.
pub(crate) const OPENS: &str = "open, answer, confirm";

/// What help says `esc` does, where it does something.
pub(crate) const ONE_STEP_BACK: &str = "one step back; what it leaves stays waiting";

/// What help says `a` does, on every screen: going to what needs the person, in the
/// word Surco says that state with.
pub(crate) fn to_what_needs_you() -> String {
    format!("go to what {}", Standing::NeedsYou.word())
}

/// Help's list of `keys`, a key a row, under the sentence that says they are all there
/// is.
pub(crate) fn listed(keys: Legend) -> Doc {
    Doc::new().with(KeyList {
        said: Some(NOTHING_MORE_TO_REMEMBER.to_string()),
        keys,
    })
}

/// Every key `legend` names, each with what it does, then the keys every stage answers
/// to, Ctrl-C leaving from anywhere: a key a row, as help lists them, under the
/// sentence that says they are all there is.
pub fn help(legend: &Legend) -> Doc {
    listed(over_every_stage(legend.clone()).key("ctrl-c", LEAVE))
}

/// The keys of help while it stands: closing it.
pub fn help_legend() -> Legend {
    Legend::new().key(ESC, BACK)
}

/// The keys of the palette while it stands: moving along its rows, doing the one
/// pointed at, and closing it.
pub fn palette_legend() -> Legend {
    Legend::new()
        .arrows("move")
        .key("enter", "do it")
        .key(ESC, BACK)
}

/// `legend` with `n` starting a new run, where a screen takes it.
pub fn with_a_new_run(legend: Legend) -> Legend {
    legend.key("n", NEW_RUN)
}

/// What PageUp and PageDown do where a page taller than what shows of it scrolls.
pub const BY_THE_PAGE: &str = "by the page";

/// `legend` led by the keys that scroll a page taller than what shows of it: a line
/// with the arrows, a page with PageUp and PageDown.
pub fn scrolling(legend: Legend) -> Legend {
    led_by(
        Legend::new().arrows("scroll").key("pgup pgdn", BY_THE_PAGE),
        legend,
    )
}

/// `rest` after the keys of `first`.
fn led_by(first: Legend, rest: Legend) -> Legend {
    let Legend(mut keys) = first;
    keys.extend(rest.0);
    Legend(keys)
}
