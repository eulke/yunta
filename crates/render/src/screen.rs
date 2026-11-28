//! What a screen of the application says around the pages it shows: the trail every
//! screen starts at, each stage's keys, and what a confirmation asks before an act.
//! The pages themselves are `page`'s.
//!
//! Each family of stages says its own in a module of its own: `field`, `catalog` (the
//! workflows and the packs), `run` (a run and what it leaves and costs), `setup`,
//! `start` (a run started, and the first one) and `machine` (doctor, upkeep and the
//! shapes of the documents Yunta reads). What every
//! screen shares stays here: the trail's start, the palette's, help's, leaving, the keys
//! that scroll a page, and every confirmation ([`Confirming`]), whichever stage asks it:
//! a new act a person confirms is a variant there, its question, sentence and keys worded
//! with the others, its keys' words in `prompt::PromptAct`.
//!
//! A key is said in a legend by a word or two (`s stats`) and in help by a sentence
//! (`s g  what it cost, its graph`). The keys a person presses anywhere they are not
//! typing, the palette and help, are [`anywhere`], drawn at the legend's far end apart
//! from the screen's own keys; help says the palette's among the keys every stage answers
//! to, with a new run, what needs the person, going down and leaving, wherever each acts
//! ([`StageKeys`]). Whether a page's legend names the keys that scroll it is one rule,
//! [`PageRead`], which [`page_legend`] owns.

mod catalog;
mod field;
mod machine;
mod run;
mod setup;
mod start;

pub use catalog::{
    auditing, cases_legend, catalog_legend, checking, enter_goes_back, fetched_legend, fetching,
    installing_pack, naming_legend, new_pack_asks, new_workflow_asks, pack_naming_legend,
    pack_reference_asks, pack_source_asks, packs_legend, scaffolding, shapes_legend, workflow_acts,
    writing_workflow, CASES, CASES_STOPPED, CHECK, INSTALL, NEW_PACK, NEW_WORKFLOW, PACKS,
    PACK_LET_GO, READING_THE_PACKS, READING_THE_WORKFLOWS, RUNNING_THE_CASES, WORKFLOWS,
    WORKFLOWS_YOU_CAN_RUN,
};
pub use field::{
    field_legend, field_place, help_over_the_field, not_closed, NOTHING_TO_RESUME,
    READING_THE_RUNS, WHAT_IT_COST,
};
pub(crate) use machine::THIS_MACHINE;
pub use machine::{
    doctor_legend, documents_legend, machine_keys_beyond, upkeep_legend, which_document_asks,
    DoctorChecks, CHECKING_THE_MACHINE, COLLECTING, COLLECTION_STOPPED, FINDING_WHAT_IT_RECLAIMS,
    MACHINE, OPENING_SESSIONS, SCHEMA, SESSIONS_STOPPED, UPKEEP,
};
pub use run::{
    answered_elsewhere, answers_recorded, help_over_a_run, help_over_an_unread_run, in_the_legend,
    opened_legend, place_of, recorded, round_answered_elsewhere, run_legend, unread_run_legend,
    Along, Leaving, PromptKeys, RunKeys, CHRONICLE, GRAPH, HISTORY, MAKE_ROOM, NOTHING_RECORDED,
    OPEN_THE_PULL_REQUEST, PAGE, READING_THE_CHRONICLE, READING_THE_GRAPH, READING_THE_HISTORY,
    READING_THE_RUNS_GRAPH, READING_THE_STATS, RECEIPT, STATS, VERIFICATION, VERIFYING,
    WRITING_THE_PAGE, WRITING_THE_RECEIPT,
};
pub use setup::{
    choosing_legend, setup_audit_keys_beyond, setup_audit_legend, setup_legend, setup_row_legend,
    typing_legend, written_legend, wrote_the_setup, Kept, FINDING_WHAT_IT_RUNS, SETUP,
    SETUP_LET_GO, WRITING_THE_SETUP,
};
pub use start::{first_request_legend, new_run_legend, NewRunRow};

use surco::components::{Binding, KeyList, Legend};
use surco::doc::Doc;
use surco::text::agreeing;
use yunta_core::refusal::CloseRefusal;
use yunta_core::step::{NextStep, Reach};

use crate::ending::CloseLeaves;
use crate::prompt::{PromptAct, ENTER, ESC};

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

/// An act a confirmation asks about before it is done, with what names what it acts on:
/// every confirmation the application asks, each worded once here, its question as the box's
/// title, what the act does under it, and its keys. Not [`crate::prompt::confirming_legend`],
/// the keys of a number chosen at a plan's desk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirming {
    /// Closing runs for good, each by its handle, and the runs named with them that it
    /// leaves open, each with why.
    Close {
        runs: Vec<String>,
        left: Vec<LeftOpen>,
    },
    /// Stopping a run, by its handle, with every session under it.
    Stop { run: String },
    /// Removing a pack from the project.
    RemovePack { pack: String },
    /// Installing a pack that ships executors it runs, each named.
    InstallPack {
        pack: String,
        executors: Vec<String>,
    },
    /// Setting a repository up with the pack a setup installs, by its name: the pack
    /// installed, then the setup written, which writes `writes`, each by its path in the
    /// repository. Escape writes the setup without the pack.
    SetUpWith { pack: String, writes: Vec<String> },
    /// Opening a session on each agent binding.
    OpenSessions,
    /// Collecting what a dry run found.
    Collect,
}

impl Confirming {
    /// What it asks, as its box's title: the act, and what it acts on.
    pub fn question(&self) -> String {
        match self {
            Confirming::Close { runs, .. } => format!("close {}?", named(runs)),
            Confirming::Stop { run } => format!("stop {run}?"),
            Confirming::RemovePack { pack } => format!("remove {pack}?"),
            Confirming::InstallPack { pack, .. } | Confirming::SetUpWith { pack, .. } => {
                format!("install {pack}?")
            }
            Confirming::OpenSessions => "open a session per binding?".to_string(),
            Confirming::Collect => "collect what the dry run found?".to_string(),
        }
    }

    /// What the act does, under its question: closing says what it leaves, in words that
    /// agree with how many runs it closes.
    pub fn sentence(&self) -> String {
        match self {
            Confirming::Close { runs, .. } => what_a_close_does(runs.len()),
            Confirming::Stop { .. } => "It stops now, with every session under it, and waits \
                                       where it stopped until you continue it or close it."
                .to_string(),
            Confirming::RemovePack { .. } => {
                "Its files and its lock entry go from this project.".to_string()
            }
            Confirming::InstallPack { executors, .. } => format!(
                "It ships executors it runs: {}.",
                surco::text::listed(executors.iter().map(String::as_str))
            ),
            Confirming::SetUpWith { pack, writes } => setup::setting_up_with(pack, writes),
            Confirming::OpenSessions => "It spends a prompt each.".to_string(),
            Confirming::Collect => "What it lists is removed for good.".to_string(),
        }
    }

    /// Its keys: `enter` does the act, and `esc` keeps what it would change, or goes back
    /// where it would change nothing a person holds; on a setup's pack, `esc` writes the setup
    /// without it. A close of several runs says both in the plural.
    pub fn legend(&self) -> Legend {
        let (act, not) = match self {
            Confirming::Close { runs, .. } if runs.len() > 1 => {
                (PromptAct::CloseThem, PromptAct::KeepThem)
            }
            Confirming::Close { .. } => (PromptAct::CloseIt, PromptAct::KeepIt),
            Confirming::Stop { .. } => (PromptAct::StopIt, PromptAct::KeepIt),
            Confirming::RemovePack { .. } => (PromptAct::RemoveIt, PromptAct::KeepIt),
            Confirming::InstallPack { .. } => (PromptAct::InstallIt, PromptAct::GoBack),
            Confirming::SetUpWith { .. } => (PromptAct::InstallIt, PromptAct::WriteWithoutIt),
            Confirming::OpenSessions => (PromptAct::OpenThem, PromptAct::GoBack),
            Confirming::Collect => (PromptAct::CollectIt, PromptAct::KeepIt),
        };
        Legend::new().key(ENTER, act.said()).key(ESC, not.said())
    }

    /// What the act leaves undone of what was asked of it, under its sentence, a line each:
    /// each run a close names and leaves open, with why. None for any other act.
    pub fn left_undone(&self) -> Vec<String> {
        match self {
            Confirming::Close { left, .. } => left.iter().map(LeftOpen::said).collect(),
            Confirming::Stop { .. }
            | Confirming::RemovePack { .. }
            | Confirming::InstallPack { .. }
            | Confirming::SetUpWith { .. }
            | Confirming::OpenSessions
            | Confirming::Collect => Vec::new(),
        }
    }
}

/// A run a close was asked to close and leaves open, by its handle, with why. Not a close
/// refused once confirmed ([`crate::refusal::Refused::Close`]), which its answer says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftOpen {
    pub run: String,
    pub why: WhyLeftOpen,
}

/// Why a close leaves a run open: what a close of it refuses; or, where the run offers no
/// close and nothing refuses one, what holds it as its row says it (a log that broke, a run
/// that cannot be read).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhyLeftOpen {
    Refused(CloseRefusal),
    Holding(String),
}

impl LeftOpen {
    /// The line a confirmation names it on: the run, that it is not closed, and why.
    pub fn said(&self) -> String {
        let why = match &self.why {
            WhyLeftOpen::Refused(refusal) => crate::refusal::close(refusal),
            WhyLeftOpen::Holding(holding) => holding.clone(),
        };
        format!("{} is not closed: {why}.", self.run)
    }
}

/// What closing `runs` runs will do and leave, in words that agree with how many: each closes
/// as cancelled, by the person, and stops waiting on one; then what a close leaves of it
/// ([`CloseLeaves`]).
fn what_a_close_does(runs: usize) -> String {
    format!(
        "{} as cancelled, by you, and {} waiting on a person. {}",
        agreeing(runs, "It closes", "They close"),
        agreeing(runs, "stops", "stop"),
        CloseLeaves::Ahead { runs }.said(),
    )
}

/// `names` as a screen's sentence names them: bare, since a box's title and a screen's row are
/// drawn as written, the last after `and`: the runs a question closes, the gates a new run
/// stops at.
pub(crate) fn named(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [only] => only.clone(),
        [before @ .., last] => format!("{} and {last}", before.join(", ")),
    }
}

/// The keys of a screen with nothing to do but leave.
pub fn leaving_legend() -> Legend {
    Legend::new().key("q", PromptAct::Quit.said())
}

/// The keys the signpost takes beyond its legend, which help says: Escape leaves too.
pub fn signpost_keys_beyond() -> Legend {
    Legend::new().key(ESC, PromptAct::Quit.said())
}

/// The keys of the signpost, outside a repository: the runs on this machine, which the field
/// lists at that reach, and leaving.
pub fn signpost_legend() -> Legend {
    let machine = NextStep::ListRuns {
        reach: Reach::Machine,
    };
    Legend::new()
        .key("e", crate::command::does(&machine))
        .key("q", PromptAct::Quit.said())
}

/// What the palette is titled, over the screen it stands on: every act from where the
/// person stands, and every act always there.
pub const PALETTE: &str = "what do you want to do?";

/// The keys a person presses anywhere they are not typing, at the legend's far end: the
/// palette, and help. Not a screen's legend, the keys of what is on it.
pub fn anywhere() -> Legend {
    Legend::new().key("/", "anything").key("?", "keys")
}

/// What help is titled, over the screen it stands on.
pub const HELP: &str = "keys";

/// What help says above the keys it lists: they are every key there is.
pub const NOTHING_MORE_TO_REMEMBER: &str = "You never have to remember more than this.";

/// What going back does, where a key says it in a word: closing what stands over a
/// screen, or leaving a page, a plan's desk among them, for what it was opened from.
pub const BACK: &str = "back";

/// What `n` does where a legend names it, and where a new run stands on the trail: a new
/// run.
pub const NEW_RUN: &str = "new run";

/// What leaving the application leaves: every run, going on without it. Help says it
/// beside Ctrl-C, and beside `q` where `q` leaves.
pub(crate) const LEAVE: &str = "leave: every run keeps going without this window";

/// What a key that goes to the field does, where a legend says it in a word: `q` on a run's
/// screen, and `esc` on the first request.
pub(crate) const FIELD: &str = "field";

/// What `j k` and the arrows do where a legend says it in a word: move the pointer along a list.
pub(crate) const MOVE: &str = "move";

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

/// Help's list of `keys`, a key a row, under the sentence that says they are all there
/// is.
pub(crate) fn listed(keys: Legend) -> Doc {
    Doc::new().with(KeyList {
        said: Some(NOTHING_MORE_TO_REMEMBER.to_string()),
        keys,
    })
}

/// Where `q` takes a person from a stage that does not leave with it, as help says it: down
/// to the nearest field under the stage, or, where none stands under it, back to the stage at
/// the bottom of what stands, where the application opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoingDown {
    ToTheField,
    ToWhereItOpened,
}

impl GoingDown {
    /// What `q` does, as help says it.
    pub fn said(self) -> &'static str {
        match self {
            GoingDown::ToTheField => "go to the field",
            GoingDown::ToWhereItOpened => "go back to where yunta opened",
        }
    }
}

/// The keys of a stage that is neither a run nor the field, as help lists them over it: those
/// its legend names and those it takes beyond them, beside the keys every stage answers to
/// wherever each acts there. Not its legend, which says its own keys in a word or two.
pub struct StageKeys<'a> {
    /// The keys its legend names.
    pub legend: &'a Legend,
    /// The keys it takes that its legend leaves out, each with what it does.
    pub beyond: Legend,
    /// Whether `n` starts a new run here: where a run can be asked for and the stage does
    /// not take `n` itself.
    pub new_run: bool,
    /// Whether `a` goes to what needs the person here.
    pub attends: bool,
    /// Whether it stands on a page that scrolls where it is taller than what shows of it.
    pub scrolls: bool,
    /// Where `q` takes the person, where it does not leave.
    pub down: Option<GoingDown>,
}

/// Help over a stage, as `keys` say it, a key a row under the sentence that says they are all
/// there is, each key as the one builder of help lists them.
pub fn help(keys: StageKeys<'_>) -> Doc {
    listed(keys.rows())
}

impl StageKeys<'_> {
    /// The keys help lists over the stage, a key a row: the palette, and a new run where `n`
    /// starts one; each key its legend names, then each it takes beyond them; the keys that
    /// scroll, where it stands on a page that scrolls; what needs the person, where `a` goes
    /// there; where `q` goes down to; and Ctrl-C leaving from anywhere. A key said once, where
    /// it is first named. The one builder of help: the field and a run hand it their own keys
    /// as its legend, each in help's words, in the order their help reads.
    pub(crate) fn rows(self) -> Legend {
        let mut said = Legend::new().key("/", ANYTHING);
        if self.new_run {
            said = said.key("n", STARTS_A_RUN);
        }
        let named = |said: &Legend, binding: &Binding| {
            let key = binding.keys.named();
            said.0.iter().any(|named| named.keys.named() == key)
        };
        for binding in self.legend.0.iter().chain(&self.beyond.0) {
            if !named(&said, binding) {
                said.0.push(binding.clone());
            }
        }
        let mut more = Legend::new();
        if self.scrolls {
            more = more.arrows("scroll").key("pgup pgdn", BY_THE_PAGE);
        }
        if self.attends {
            more = more.key("a", surco::words::TO_WHAT_NEEDS_YOU);
        }
        if let Some(down) = self.down {
            more = more.key("q", down.said());
        }
        for binding in more.key("ctrl-c", LEAVE).0 {
            if !named(&said, &binding) {
                said.0.push(binding);
            }
        }
        said
    }
}

/// The keys of help while it stands: closing it.
pub fn help_legend() -> Legend {
    Legend::new().key(ESC, BACK)
}

/// The keys of the palette while it stands: moving along its rows, doing the one
/// pointed at, and closing it.
pub fn palette_legend() -> Legend {
    Legend::new()
        .arrows(MOVE)
        .key(ENTER, PromptAct::DoIt.said())
        .key(ESC, BACK)
}

/// How a page is read, which decides whether its legend names the keys that scroll it: the
/// one rule the design's frames follow. A page that runs past the design's own screen by
/// what it holds, read by scrolling (a run's whole chronicle, a graph, the audit of the pack
/// a setup installs), names the arrows at every size; a page the design's screen shows whole
/// (a run's numbers, what it leaves, a workflow's check or past runs, the cases, what the
/// packs show, the machine, upkeep, a document's shape) names none at any size, though it
/// scrolls on a smaller terminal, where help says them. A legend is the same at every size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageRead {
    /// Read by scrolling: its legend leads with the arrows.
    Scrolled,
    /// Shown whole at the design's size: its legend leaves the keys that scroll to help.
    Whole,
}

/// The keys of a page opened over another, read as `read` ([`PageRead`]): the arrows where it
/// is read by scrolling, its `own` keys, back to what it was opened over, and a new run where
/// its legend names one (`new_run`), as a page opened from a run or the workflows does where
/// a run can be asked for. Every page's legend is made here.
pub fn page_legend(read: PageRead, own: Legend, new_run: bool) -> Legend {
    let legend = match read {
        PageRead::Scrolled => Legend::new().arrows("scroll"),
        PageRead::Whole => Legend::new(),
    };
    let legend = led_by(legend, own).key(ESC, BACK);
    match new_run {
        true => legend.key("n", NEW_RUN),
        false => legend,
    }
}

/// What a list of acts is said under, where a screen offers them beside what they act on:
/// the field's glance at a run, and the panel beside the workflows.
pub const WHAT_YOU_CAN_DO: &str = "what you can do";

/// What PageUp and PageDown do where a page taller than what shows of it scrolls, said after
/// the arrows, which scroll it too. Not a run's screen's, said where the arrows move along
/// something else.
pub const BY_THE_PAGE: &str = "by the page";

/// What PageUp and PageDown do on a run's screen, as help over it says them: its pane scrolls
/// a page at a time, while `j k` and the arrows move along its nodes. Not [`BY_THE_PAGE`], said
/// after arrows that scroll.
pub(crate) const SCROLLS_THE_PANE: &str = "scroll the pane a page at a time";

/// `rest` after the keys of `first`.
fn led_by(first: Legend, rest: Legend) -> Legend {
    let Legend(mut keys) = first;
    keys.extend(rest.0);
    Legend(keys)
}
