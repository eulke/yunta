//! See [`super`]. The field's: where it stands on the trail, what stands while the runs are
//! read, its keys in its legend and every key help says of it, and what `r` notes on a run no
//! resume moves. What closing and stopping runs ask before they do is a confirmation's,
//! worded with every other in [`super::Confirming`].
//!
//! The legend names the five keys the design gives the field where they serve, a new run and
//! quitting alone where no run stands in sight to move to, open or close; the others it takes
//! (marking, resuming, stopping, what a run cost, what needs the person, the closed, the
//! machine, the workflows, the machine's checks) stand in help, each in the words the palette
//! says its act with where the palette says one, and help's own where it does not or where the
//! design words it apart: `r` is `resume it` there, where the palette, the glance and a run's
//! legend say `continue it from its log`.

use std::path::Path;

use surco::components::Legend;
use surco::doc::Doc;
use yunta_core::step::NextStep;
use yunta_replay::frame::Effect;

use super::{LEAVE, MOVE, MOVES, NEW_RUN, OPENS, THIS_MACHINE};
use crate::command;
use crate::prompt::{PromptAct, ENTER};
use crate::refusal::Refused;

/// What the application says while it reads the runs of the repository it opened in.
pub const READING_THE_RUNS: &str = "reading this repository's runs";

/// What `s` does where help over the field and the glance at the run pointed at say it: the
/// run's numbers. Not the palette's row of the same act, which says where its time went too.
pub const WHAT_IT_COST: &str = "what it cost";

/// What `r` notes on the field where the run pointed at waits on a decision: a resume answers
/// none, and `enter` opens the run where it is read.
pub const NOTHING_TO_RESUME: &str =
    "Nothing to resume: this one waits on a decision. Enter reads it.";

/// What the field notes of a close some of whose runs were `refused`, in the order it asked
/// them: a close of one, its refusal whole; of several, in one sentence, how many were not
/// closed and why the first was not. None where none was refused.
pub fn not_closed(refused: &[Refused]) -> Option<String> {
    match refused {
        [] => None,
        [only] => Some(only.to_string()),
        [first, ..] => Some(format!(
            "{} were not closed, the first because {first}",
            surco::text::counted(refused.len(), "run")
        )),
    }
}

/// Where the field stands on the trail: the `repository` it lists, its path under the home
/// mark where it is under `home` and whole otherwise, as a place a person recognises from any
/// directory; or the machine, where it lists every run on it (no repository).
pub fn field_place(repository: Option<&Path>, home: Option<&Path>) -> String {
    match repository {
        Some(repository) => surco::paths::from_home(repository, home),
        None => THIS_MACHINE.to_string(),
    }
}

/// The keys of the field of runs, as its legend names them, those that serve where it stands:
/// moving through it and opening the run pointed at where it `acts_on_a_run`, a new run where
/// one `asks_for_a_run` here, closing where it acts on a run, and quitting. Help says every
/// other key it takes ([`help_over_the_field`]).
pub fn field_legend(asks_for_a_run: bool, acts_on_a_run: bool) -> Legend {
    let legend = match acts_on_a_run {
        true => Legend::new().key("j k", MOVE).key(ENTER, "open"),
        false => Legend::new(),
    };
    let legend = match asks_for_a_run {
        true => legend.key("n", NEW_RUN),
        false => legend,
    };
    let legend = match acts_on_a_run {
        true => legend.key("x", super::in_the_legend(Effect::Closes)),
        false => legend,
    };
    legend.key("q", PromptAct::Quit.said())
}

/// Help over the field: the keys every screen answers to, a new run among them where one
/// `asks_for_a_run` here, then the field's own, those its legend leaves out among them, each
/// act in the palette's words where the palette offers it; then leaving. `d` and `e` say what
/// they do from where the field stands: `d` hiding the closed where it shows them (`closed`);
/// `e` what the step it takes lists (`farther`), and no row where it takes none, outside a
/// repository, where the field stands at the machine's reach alone.
pub fn help_over_the_field(closed: bool, asks_for_a_run: bool, farther: Option<&NextStep>) -> Doc {
    let depth = match closed {
        true => "hides the closed",
        false => "the closed too",
    };
    let doctor = NextStep::Doctor {
        sessions: false,
        full: false,
    };
    let keys = Legend::new()
        .key("j k", MOVES)
        .key(ENTER, OPENS)
        .key("space", "mark it")
        .key("x", "close the marked, or the pointed, once confirmed")
        .key("r", "resume it")
        .key("c", command::effect(Effect::Stops))
        .key("s", WHAT_IT_COST)
        .key("a", command::does(&NextStep::Needs))
        .key("d", depth);
    let keys = match farther {
        Some(step) => keys.key("e", command::does(step)),
        None => keys,
    };
    let keys = keys
        .key("w", command::does(&NextStep::List))
        .key("m", command::does(&doctor))
        .key("q", LEAVE);
    super::help(super::StageKeys {
        legend: &keys,
        beyond: Legend::new(),
        new_run: asks_for_a_run,
        attends: false,
        scrolls: false,
        down: None,
    })
}
