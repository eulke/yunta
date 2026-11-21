//! See [`super`]. A run's and what it leaves and costs: where its receipt, its
//! verification, its page, its stats, its graph and its whole chronicle stand on the
//! trail, a page about a run or a workflow taking its handle or its name and its word as one
//! place, what stands
//! while each is read, the keys of each of its scenes and of each page over it, help
//! over it, and the note an act on it leaves.
//!
//! A scene's legend says only the keys it takes, in the design's order: what `j k`
//! move, following the run where the person took the lead, the scene's own acts, its
//! numbers and its graph, a new run where it ended, then `q`. A decision's prompt
//! standing in the run's screen says its own keys in their place.

use surco::components::Legend;
use yunta_core::ahead::NodesAhead;
use yunta_core::events::{
    Failure, GateOption, GateResolvedPayload, GateWaitingPayload, Leads, Menu,
    QuestionsAnsweredPayload,
};
use yunta_core::reserved::ReservedOption;
use yunta_core::{NodeId, OptionId};
use yunta_replay::frame::Effect;

use super::{
    listed, to_what_needs_you, ANYTHING, BACK, LEAVE, MOVES, NEW_RUN, ONE_STEP_BACK, OPENS,
    STARTS_A_RUN,
};
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

/// The keys of a page opened over a run, what it leaves or how it is drawn: back to
/// the run.
pub fn over_the_run_legend() -> Legend {
    Legend::new().key(ESC, BACK)
}

/// The keys of a run's page written for a browser: opening it again, or back to the
/// run.
pub fn opened_legend() -> Legend {
    Legend::new().key("enter", "open it again").key(ESC, BACK)
}

/// Where what a run cost stands on the trail.
pub const STATS: &str = "stats";

/// Where what a workflow's past runs cost stands on the trail.
pub const HISTORY: &str = "history";

/// Where a workflow's graph stands on the trail.
pub const GRAPH: &str = "graph";

/// Where a run's whole chronicle stands on the trail.
pub const CHRONICLE: &str = "chronicle";

/// The place on the trail of a page about `subject`, a run's handle or a workflow's name,
/// where the page stands as `page`: the subject and the page's word, one place, nothing
/// between.
pub fn place_of(subject: &str, page: &str) -> String {
    format!("{subject} {page}")
}

/// The keys of a page over a run standing in the run's frame, its numbers, its graph or its
/// whole chronicle: the arrows where it `scrolls` by them, as the graph and the chronicle
/// do, back to the run, and a new run.
pub fn in_the_runs_frame_legend(scrolls: bool) -> Legend {
    let legend = match scrolls {
        true => Legend::new().arrows("scroll"),
        false => Legend::new(),
    };
    legend.key(ESC, BACK).key("n", NEW_RUN)
}

/// What stands while what a run cost is read.
pub const READING_THE_STATS: &str = "reading what the run cost";

/// What stands while a workflow's past runs are read.
pub const READING_THE_HISTORY: &str = "reading the workflow's past runs";

/// What stands while a workflow is read to be drawn.
pub const READING_THE_GRAPH: &str = "reading the workflow to draw it";

/// What stands while a run's whole chronicle is read.
pub const READING_THE_CHRONICLE: &str = "reading the run's chronicle";

/// What a scene of a run moves `j k` along: its nodes, or the findings the run's
/// reviewers posted, where `tab` hands `j k` to the nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Along {
    Nodes,
    Findings,
}

/// Where a run is left when the person leaves its screen: going on, since an engine
/// drives it and none lives in the application; waiting for someone to answer or take
/// it up; or ended. Not the run's standing: only whether anything moves it now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leaving {
    KeepsGoing,
    Waits,
    Ended,
}

impl Leaving {
    /// The note that leaving says, where the person lands.
    pub fn said(self) -> &'static str {
        match self {
            Leaving::KeepsGoing => "The run keeps going: no engine lives in this window.",
            Leaving::Waits => {
                "The run stays waiting: it moves again once it is answered or resumed."
            }
            Leaving::Ended => "The run has ended: leaving it changes nothing.",
        }
    }
}

/// What `q` does on a run's screen, in a word: it goes to the field.
const FIELD: &str = "field";

/// What a scene of a run offers its legend: what `j k` move along, its own acts (each
/// key with what it does, in the legend's words: [`in_the_legend`]), whether `n` starts
/// a new run from it, and where `q`, going to the field, leaves the run.
#[derive(Debug, Clone, Copy)]
pub struct RunKeys<'a> {
    pub along: Along,
    pub acts: &'a [(char, &'a str)],
    pub new_run: bool,
    pub leaving: Leaving,
}

/// The keys of a run's scene, in the order the design reads them: what `j k` move,
/// `f` giving the lead back to the run while the person holds it (`leads` false), the
/// scene's acts, its numbers, its graph on a scene of nodes the run leads, a new run
/// where the scene offers one, then `q`. No key the scene does not take.
pub fn run_legend(keys: &RunKeys, leads: bool) -> Legend {
    let legend = match keys.along {
        Along::Nodes => Legend::new().key("j k", "node"),
        Along::Findings => Legend::new().key("j k", "finding").key("tab", "nodes"),
    };
    let mut legend = match leads {
        true => legend,
        false => legend.key("f", "follow it"),
    };
    for (key, does) in keys.acts {
        legend = legend.key(key.to_string(), *does);
    }
    let legend = legend.key("s", "stats");
    let legend = match leads && keys.along == Along::Nodes {
        true => legend.key("g", "graph"),
        false => legend,
    };
    let legend = match keys.new_run {
        true => legend.key("n", NEW_RUN),
        false => legend,
    };
    let quit = match keys.leaving {
        Leaving::KeepsGoing => "field, it keeps going",
        Leaving::Waits | Leaving::Ended => FIELD,
    };
    legend.key("q", quit)
}

/// The keys of a run's screen before the run is read: going to the field, with nothing
/// said yet of whether the run keeps going.
pub fn unread_run_legend() -> Legend {
    Legend::new().key("q", FIELD)
}

/// What an act on a run says in a scene's legend: closing it and handing it back in a
/// word or two, going on where it stopped as the field's glance says it to a person, any
/// other act as every page says it.
pub fn in_the_legend(effect: Effect) -> &'static str {
    match effect {
        Effect::Closes => "close",
        Effect::HandsBackOnceSettled => "hand it back",
        Effect::ContinuesWhereStopped => "continue it where it stopped",
        Effect::ContinuesFromLog
        | Effect::WaitsForReset
        | Effect::RetriesOnceTreeChanged
        | Effect::ReadsPlan
        | Effect::ShowsWhatItWaitsOn
        | Effect::Stops
        | Effect::Certifies
        | Effect::Verifies => crate::command::effect(effect),
    }
}

/// What `u` says on a run standing still for room: collecting what it can.
pub const MAKE_ROOM: &str = "make room";

/// What `o` says on a run waiting on its pull request: the request, on its forge.
pub const OPEN_THE_PULL_REQUEST: &str = "open the pull request";

/// The keys of a run's acts help over a run says in rows of its own: a new run, and what it
/// cost with its graph.
const HELPS_ROWS_SAY: [char; 3] = ['n', 's', 'g'];

/// The keys of a prompt standing in place on a run's screen, as help over the run reads them:
/// every key it takes, in its words, those its legend names and those the legend leaves to
/// help, and whether a line is typed on it, where every printable key types. Not a `Legend` alone, which says the keys a screen names and not
/// what a line typed there swallows.
#[derive(Clone, Copy)]
pub struct PromptKeys<'l> {
    pub legend: &'l Legend,
    pub typed: bool,
}

/// The keys a line typed in place leaves acting for the screen it stands on: Ctrl-C, which
/// leaves from anywhere. Every other key types or is the line's own.
const A_TYPED_LINE_LEAVES: [&str; 1] = ["ctrl-c"];

impl PromptKeys<'_> {
    /// `rows`, the keys help over the run says, under this prompt: its keys first, each in its
    /// words; then each row of `rows` whose keys it names none of, and, while a line is typed
    /// on it, whose keys the line leaves acting.
    fn over(&self, rows: Legend) -> Legend {
        let named: Vec<&str> = self
            .legend
            .0
            .iter()
            .flat_map(|binding| binding.keys.named().split_whitespace())
            .collect();
        let stands = |keys: &str| {
            let mut keys = keys.split_whitespace();
            match self.typed {
                true => keys.all(|key| A_TYPED_LINE_LEAVES.contains(&key)),
                false => keys.all(|key| !named.contains(&key)),
            }
        };
        let standing = rows
            .0
            .into_iter()
            .filter(|binding| stands(binding.keys.named()));
        Legend(self.legend.0.iter().cloned().chain(standing).collect())
    }
}

/// Help over a run: the keys every screen answers to and the run's own, `tab` where `j k`
/// move `along` the findings, then each of the scene's `acts` those rows do not say (each
/// key with what it does), the ones its legend leaves out among them, then `q`, saying
/// whether the run keeps going as `leaving` says, and Ctrl-C. Under a prompt standing in
/// place, its keys first, in its words, the run's rows for those keys gone, and every row a
/// line typed there swallows (`PromptKeys`).
pub fn help_over_a_run(
    along: Along,
    acts: &[(char, &str)],
    prompt: Option<PromptKeys<'_>>,
    leaving: Leaving,
) -> surco::doc::Doc {
    let keys = Legend::new()
        .key("/", ANYTHING)
        .key("n", STARTS_A_RUN)
        .key("j k", MOVES);
    let keys = match along {
        Along::Nodes => keys,
        Along::Findings => keys.key("tab", "hand j k to the nodes and back"),
    };
    let mut keys = keys
        .key("pgup pgdn", "scroll the pane a page at a time")
        .key("enter", OPENS)
        .key(ESC, ONE_STEP_BACK)
        .key("a", to_what_needs_you())
        .key("f", "follow what the run is doing")
        .key("s g", "what it cost, its graph");
    for (key, does) in acts.iter().filter(|(key, _)| !HELPS_ROWS_SAY.contains(key)) {
        keys = keys.key(key.to_string(), *does);
    }
    let field = match leaving {
        Leaving::KeepsGoing => "go to the field; it keeps going",
        Leaving::Waits | Leaving::Ended => "go to the field",
    };
    let keys = keys.key("q", field).key("ctrl-c", LEAVE);
    listed(match prompt {
        Some(prompt) => prompt.over(keys),
        None => keys,
    })
}

/// What a round put aside leaves said: nothing of it was recorded.
pub const NOTHING_RECORDED: &str = "Nothing recorded. The run stays waiting on these questions.";

/// The note a round's answers recorded leave: how many `answers` were recorded, then what the
/// run does now: the node `asked` that asked them finishes with them, or with what each
/// question assumes where none was given, and the run goes on.
pub fn answers_recorded(asked: &NodeId, answers: usize) -> String {
    match answers {
        0 => format!(
            "Recorded: no answer. `{asked}` finishes with what each question assumes, and the \
             run goes on."
        ),
        answers => format!(
            "Recorded: {}. `{asked}` finishes with {}, and the run goes on.",
            surco::text::counted(answers, "answer"),
            surco::text::agreeing(answers, "it", "them")
        ),
    }
}

/// The note an answer recorded leaves: the option `chosen` on `escalation`, then what
/// the run does now, by the kind of option it is and by what lies `ahead` where it leads.
/// An option that sends the run back to a node with the `words` it was said with says
/// where the words went, and what they were.
pub fn recorded(
    escalation: &GateWaitingPayload,
    chosen: &OptionId,
    words: Option<&str>,
    ahead: &NodesAhead,
) -> String {
    let option = escalation
        .options()
        .iter()
        .find(|option| option.id == *chosen);
    let back = option.and_then(|option| match &option.leads {
        Some(Leads::Back { node, .. }) => Some(node),
        _ => None,
    });
    if let (Some(node), Some(words)) = (back, words) {
        return format!("Sent back to `{node}` with your words: \"{words}\".");
    }
    match option.and_then(|option| follows(option, escalation.menu(), ahead)) {
        Some(then) => format!("Recorded: {chosen}. {then}"),
        None => format!("Recorded: {chosen}."),
    }
}

/// What follows `option` once recorded: where an author's option takes the run, past the
/// nodes `ahead` says the run leaves out, else what the engine does on the option it
/// reserves, as `menu` asked it.
fn follows(option: &GateOption, menu: Option<&Menu>, ahead: &NodesAhead) -> Option<String> {
    if let Some(leads) = &option.leads {
        return Some(goes(leads, ahead));
    }
    let reserved = ReservedOption::of(&option.id)?;
    let loop_cap = matches!(menu, Some(Menu::LoopCap(_)));
    let then = match reserved {
        ReservedOption::Grant if owes_scope(menu) => "The session goes on with the wider scope.",
        ReservedOption::Grant if matches!(menu, Some(Menu::Failure(_))) => {
            "The node runs again with the wider scope."
        }
        ReservedOption::Grant => "The session goes on with the wider scope.",
        ReservedOption::Deny => "The task goes on within its scope; the denial becomes a finding.",
        ReservedOption::Retry => "A fresh attempt starts from the run's tree.",
        ReservedOption::ContinueWork => {
            "The loop runs again, each blocked task going on from the work it left."
        }
        ReservedOption::Abort if loop_cap => "The loop node fails at its cap.",
        ReservedOption::Abort => "The run pauses here, and asks again when it is resumed.",
        ReservedOption::Promote => "The run closes, and a new run goes on in the next mode.",
        ReservedOption::Approve => "The gate is approved, and the run goes on.",
        ReservedOption::Reject => "The gate fails, rejected.",
        ReservedOption::Continue if loop_cap => {
            "The loop keeps iterating past its cap until the run next stops."
        }
        ReservedOption::Continue => "The run goes past its token cap until it next stops.",
        ReservedOption::Accept => "The task closes on its work as it stands.",
        ReservedOption::SendBack => "The session resumes with what you said.",
    };
    Some(then.to_string())
}

/// Whether `menu` is a loop's failure owing a person its tasks' requests for scope: a grant
/// there picks each task's work back up in the session it worked in last, where a failure of
/// a node's own runs the node again.
fn owes_scope(menu: Option<&Menu>) -> bool {
    let Some(Menu::Failure(menu)) = menu else {
        return false;
    };
    matches!(menu.failure, Failure::ScopeOwed { .. })
}

/// Where an author's option `leads` the run once recorded, past the nodes `ahead` says the
/// run leaves out.
fn goes(leads: &Leads, ahead: &NodesAhead) -> String {
    match leads {
        Leads::On { next, .. } if next.is_empty() => {
            "The gate finishes; no node waits on it.".to_string()
        }
        Leads::On { next, .. } => {
            let running: Vec<&str> = next
                .iter()
                .filter(|node| !crate::offers::left_out(node, ahead))
                .map(|node| node.as_str())
                .collect();
            match running.is_empty() {
                true => format!(
                    "The gate finishes; {} {} not in this run.",
                    surco::text::listed(next.iter().map(|node| node.as_str())),
                    surco::text::agreeing(next.len(), "is", "are")
                ),
                false => format!("On to {}.", surco::text::listed(running)),
            }
        }
        Leads::Back { node, .. } => {
            format!("Back to `{node}`; the gate asks again once it has run.")
        }
    }
}

/// The note a decision answered elsewhere leaves: who answered it and how, as the log
/// says it.
pub fn answered_elsewhere(resolved: &GateResolvedPayload) -> String {
    format!(
        "Answered elsewhere: {}.",
        crate::chronicle::resolution(resolved)
    )
}

/// The note a round of questions answered elsewhere leaves: who answered it and through what,
/// in the words the chronicle says it in.
pub fn round_answered_elsewhere(answered: &QuestionsAnsweredPayload) -> String {
    let who = crate::chronicle::who_answered(answered.channel, answered.responder.as_ref());
    format!("Answered elsewhere: {who}.")
}
