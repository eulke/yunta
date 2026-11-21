//! The words of every option a decision offers: what it does, which its
//! name may say alone, what choosing it costs, and the question it asks. An
//! option stores its id and where it leads, and its escalation's menu says
//! the rest, as a decision's card says it beside the name.

use surco::components::DecisionOption;
use surco::text;
use yunta_core::ahead::{Ahead, NodesAhead};
use yunta_core::events::{Asks, GateOption, GateWaitingPayload, Leads, Menu, ScopeWait};
use yunta_core::reserved::ReservedOption;
use yunta_core::NodeId;

use crate::blocks::{departure_words, remedy_words, reroute_words};

/// The label of an option that pauses the run with its decision open.
pub(crate) const PAUSE_HERE: &str = "Pause here";

/// What going on past the token cap does, wherever it is offered.
pub(crate) const LIFTS_THE_CAP: &str =
    "Lifts the cap for this invocation only; a later resume will ask again before spending more";

/// What going on past a gate does when no node waits on it.
const NOTHING_WAITS: &str = "nothing in the workflow waits on it";

/// What choosing an option that pauses the run does: nothing runs, and its
/// decision stays open.
pub const STAYS_OPEN: &str = "nothing runs; this decision stays open";

/// What a card says an option asks of a task's scope that a person grants: that the
/// task, or each of `tasks`, may touch the `paths` it asked for as well, and goes on.
pub(crate) fn may_also_touch(tasks: usize, paths: usize) -> String {
    match (tasks, paths) {
        (1, 1) => "the task may also touch this path, and goes on".to_string(),
        (1, _) => "the task may also touch these paths, and goes on".to_string(),
        _ => "each task may also touch what it asked for, and goes on".to_string(),
    }
}

/// What a card says denying a scope request does: the task, or each of `tasks`, goes on
/// inside the scope it declared.
pub(crate) fn goes_on_within(tasks: usize) -> String {
    match tasks {
        1 => "the task goes on within the scope it has".to_string(),
        _ => "each task goes on within the scope it has".to_string(),
    }
}

/// The label and the trade-off a person reads beside `option`: the words an
/// older log stored with it, else the words its data, its menu and what lies
/// `ahead` at the nodes it leads to give it. The label is the id when nothing
/// else says it.
pub fn option_words(
    payload: &GateWaitingPayload,
    option: &GateOption,
    ahead: &NodesAhead,
) -> (String, Option<String>) {
    let (label, tradeoff) = written(payload, option, ahead).unwrap_or_default();
    let label = option
        .logged
        .label()
        .map(str::to_string)
        .or(label)
        .unwrap_or_else(|| option.id.as_str().to_string());
    let tradeoff = option.logged.tradeoff().map(str::to_string).or(tradeoff);
    (label, tradeoff)
}

/// The question `option` asks, in words, when it asks one.
pub fn asks(option: &GateOption) -> Option<String> {
    option.asks().map(|asks| asked(&asks))
}

/// A question an option asks, as a person reads it.
pub fn asked(asks: &Asks) -> String {
    match asks {
        Asks::Change => "what should change?".to_string(),
        Asks::Instead => "what should it do instead?".to_string(),
        Asks::Logged(said) => said.as_str().to_string(),
    }
}

/// Every option `escalation` offers, as a decision draws it: its id, the
/// words it is read by, what choosing it costs and what it asks, each read with
/// what lies `ahead` at the nodes it leads to.
pub fn decision_options(
    escalation: &GateWaitingPayload,
    ahead: &NodesAhead,
) -> Vec<DecisionOption> {
    escalation
        .options()
        .iter()
        .map(|option| {
            let (label, tradeoff) = option_words(escalation, option, ahead);
            DecisionOption {
                id: option.id.to_string(),
                label: Some(label),
                tradeoff,
                asks: asks(option),
            }
        })
        .collect()
}

/// The words the option's data gives it, what it does and what it costs, either of which
/// its name may say alone: where an author's option leads, else what its menu offers, else
/// the abort every menu ends with.
fn written(
    payload: &GateWaitingPayload,
    option: &GateOption,
    ahead: &NodesAhead,
) -> Option<(Option<String>, Option<String>)> {
    if let Some(leads) = &option.leads {
        let (label, tradeoff) = leading(leads, ahead);
        return Some((Some(label), tradeoff));
    }
    let id = ReservedOption::of(&option.id)?;
    let words = match payload.menu() {
        Some(Menu::Failure(menu)) => match &menu.reroute {
            Some(reroute) => reroute_words(reroute, id),
            None => remedy_words(menu, id),
        },
        Some(Menu::Departure(menu)) => departure_words(menu, id),
        Some(Menu::Scope(wait)) => scoped(wait, id),
        Some(Menu::Budget(_)) => over_budget(id),
        Some(Menu::LoopCap(_)) => over_iterations(id),
        Some(Menu::Console(_)) => at_the_console(id),
        Some(Menu::Gate(_) | Menu::Published(_)) | None => None,
    };
    words.or_else(|| aborting(id))
}

/// Where an author's option leads, and what going on does there and settles: a node
/// `ahead` says opens the run's pull request says on which forge and repository; nodes the
/// run leaves out are said as not in it; any other node by its name alone.
pub(crate) fn leading(leads: &Leads, ahead: &NodesAhead) -> (String, Option<String>) {
    match leads {
        Leads::Back { node, resumes } => {
            let tradeoff = if *resumes {
                "sends what you say, and asks again once it completes".to_string()
            } else {
                "reroutes there, and asks again once it completes".to_string()
            };
            (format!("Back to `{node}`"), Some(tradeoff))
        }
        Leads::On { next, settles } => {
            let running: Vec<&NodeId> = next.iter().filter(|node| !left_out(node, ahead)).collect();
            let (label, tradeoff) = match (next.is_empty(), running.as_slice()) {
                (true, _) => ("Resolve the gate".to_string(), NOTHING_WAITS.to_string()),
                (false, []) => ("Resolve the gate".to_string(), not_in_the_run(next, ahead)),
                (false, [node]) => match ahead.at(node) {
                    Some(Ahead::OpensPullRequest { repo }) => {
                        let opens = format!("opens the pull request on github {repo}");
                        return (format!("On to `{node}`"), Some(opens));
                    }
                    _ => (format!("On to `{node}`"), "resolves this gate".to_string()),
                },
                (false, running) => {
                    let onward = text::listed(running.iter().map(|id| id.as_str()));
                    (format!("On to {onward}"), "resolves this gate".to_string())
                }
            };
            let tradeoff = if *settles {
                format!("{tradeoff}, and settles every finding it shows")
            } else {
                tradeoff
            };
            (label, Some(tradeoff))
        }
    }
}

/// Whether `ahead` says the run leaves `node` out.
pub(crate) fn left_out(node: &NodeId, ahead: &NodesAhead) -> bool {
    matches!(ahead.at(node), Some(Ahead::LeftOut(_)))
}

/// That every node of `next`, which `ahead` says the run leaves out, is not in it: one by
/// name with why, several by name.
fn not_in_the_run(next: &[NodeId], ahead: &NodesAhead) -> String {
    match next {
        [node] => match ahead.at(node) {
            Some(Ahead::LeftOut(because)) => format!("`{node}` is not in this run: {because}"),
            _ => format!("`{node}` is not in this run"),
        },
        _ => format!(
            "{} are not in this run",
            text::listed(next.iter().map(|id| id.as_str()))
        ),
    }
}

/// What going on past a gate does, for a reader whose option does not say where it
/// goes: on a forge, an approval's label is the forge's own.
pub(crate) fn resolving(next: &[NodeId]) -> String {
    if next.is_empty() {
        return format!("resolves this gate; {NOTHING_WAITS}");
    }
    let onward = text::listed(next.iter().map(|id| id.as_str()));
    let runs = text::agreeing(next.len(), "runs", "run");
    format!("resolves this gate; {onward} {runs} next")
}

/// The grant and the denial of one task's scope request, whose paths the card's question
/// names.
fn scoped(wait: &ScopeWait, id: ReservedOption) -> Option<(Option<String>, Option<String>)> {
    match id {
        ReservedOption::Grant => Some((None, Some(may_also_touch(1, wait.paths.len())))),
        ReservedOption::Deny => Some((None, Some(goes_on_within(1)))),
        _ => None,
    }
}

/// The continue of a run's token cap that is spent, which its name says. Its abort is the
/// one every menu offers.
fn over_budget(id: ReservedOption) -> Option<(Option<String>, Option<String>)> {
    match id {
        ReservedOption::Continue => Some((None, Some(LIFTS_THE_CAP.to_string()))),
        _ => None,
    }
}

/// The continue and the abort of a loop's iteration cap that is spent.
fn over_iterations(id: ReservedOption) -> Option<(Option<String>, Option<String>)> {
    match id {
        ReservedOption::Continue => Some((
            Some("Keep iterating".to_string()),
            Some(
                "Lifts the cap for this invocation only; a later resume will ask again".to_string(),
            ),
        )),
        ReservedOption::Abort => Some((
            Some("Fail the loop node".to_string()),
            Some(
                "The node fails naming the limit and the run pauses; a resume re-runs the loop \
                 and re-asks"
                    .to_string(),
            ),
        )),
        _ => None,
    }
}

/// The approve and the reject of a gate the forge could not be asked about.
fn at_the_console(id: ReservedOption) -> Option<(Option<String>, Option<String>)> {
    match id {
        ReservedOption::Approve => Some((
            Some("Approve".to_string()),
            Some("Marks the gate as passed; the run continues".to_string()),
        )),
        ReservedOption::Reject => Some((
            Some("Reject".to_string()),
            Some("Fails the node; its declared reroute (if any) takes over".to_string()),
        )),
        _ => None,
    }
}

/// The abort every menu offers, which pauses the run.
fn aborting(id: ReservedOption) -> Option<(Option<String>, Option<String>)> {
    (id == ReservedOption::Abort)
        .then(|| (Some(PAUSE_HERE.to_string()), Some(STAYS_OPEN.to_string())))
}
