//! The words of every option a decision offers: the label a person picks it
//! by, what choosing it costs, and the question it asks. An option stores
//! its id and where it leads, and its escalation's menu says the rest.

use yunta_core::events::{Asks, GateOption, GateWaitingPayload, Leads, Menu, ScopeWait};
use yunta_core::reserved::ReservedOption;
use yunta_core::{listed_globs, text};

use crate::blocks::{departure_words, remedy_words, reroute_words};

/// The label and the trade-off a person reads beside `option`: the words an
/// older log stored with it, else the words its data and its menu give it.
/// The label is the id when nothing else says it.
pub fn option_words(payload: &GateWaitingPayload, option: &GateOption) -> (String, Option<String>) {
    let (label, tradeoff) = written(payload, option).unzip();
    let label = option
        .logged
        .label()
        .map(str::to_string)
        .or(label)
        .unwrap_or_else(|| option.id.as_str().to_string());
    let tradeoff = option
        .logged
        .tradeoff()
        .map(str::to_string)
        .or(tradeoff.flatten());
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

/// The words the option's data gives it: where an author's option leads,
/// else what its menu offers, else the abort every menu ends with.
fn written(payload: &GateWaitingPayload, option: &GateOption) -> Option<(String, Option<String>)> {
    if let Some(leads) = &option.leads {
        return Some(leading(leads));
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

/// Where an author's option leads, and what going on settles.
fn leading(leads: &Leads) -> (String, Option<String>) {
    match leads {
        Leads::Back { node, resumes } => {
            let tradeoff = if *resumes {
                format!("sends what you say back to `{node}`, and asks again once it completes")
            } else {
                format!("re-routes to `{node}` and asks again once it completes")
            };
            (format!("Back to `{node}`"), Some(tradeoff))
        }
        Leads::On { next, settles } => {
            let (label, tradeoff) = if next.is_empty() {
                (
                    "Resolve the gate".to_string(),
                    "resolves this gate; nothing in the workflow waits on it".to_string(),
                )
            } else {
                let onward = text::listed(next.iter().map(|id| id.as_str()));
                let runs = text::agreeing(next.len(), "runs", "run");
                (
                    format!("On to {onward}"),
                    format!("resolves this gate; {onward} {runs} next"),
                )
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

/// The grant and the denial of a scope request, which name the paths it asks for.
fn scoped(wait: &ScopeWait, id: ReservedOption) -> Option<(String, Option<String>)> {
    match id {
        ReservedOption::Grant => Some((
            format!("Grant access to {}", listed_globs(&wait.paths)),
            Some(
                "The task's final diff is evaluated against its scope plus these paths".to_string(),
            ),
        )),
        ReservedOption::Deny => Some((
            "Deny the request".to_string(),
            Some(
                "The denial becomes a finding; the task retries within its original scope"
                    .to_string(),
            ),
        )),
        _ => None,
    }
}

/// The continue and the abort of a run's token cap that is spent.
fn over_budget(id: ReservedOption) -> Option<(String, Option<String>)> {
    match id {
        ReservedOption::Continue => Some((
            "Continue past the cap".to_string(),
            Some(
                "Lifts the cap for this invocation only; a later resume will ask again before \
                 spending more"
                    .to_string(),
            ),
        )),
        ReservedOption::Abort => Some((
            "Pause the run".to_string(),
            Some("The run pauses with reason `budget`; a resume re-asks".to_string()),
        )),
        _ => None,
    }
}

/// The continue and the abort of a loop's iteration cap that is spent.
fn over_iterations(id: ReservedOption) -> Option<(String, Option<String>)> {
    match id {
        ReservedOption::Continue => Some((
            "Keep iterating".to_string(),
            Some(
                "Lifts the cap for this invocation only; a later resume will ask again".to_string(),
            ),
        )),
        ReservedOption::Abort => Some((
            "Fail the loop node".to_string(),
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
fn at_the_console(id: ReservedOption) -> Option<(String, Option<String>)> {
    match id {
        ReservedOption::Approve => Some((
            "Approve".to_string(),
            Some("Marks the gate as passed; the run continues".to_string()),
        )),
        ReservedOption::Reject => Some((
            "Reject".to_string(),
            Some("Fails the node; its declared re-route (if any) takes over".to_string()),
        )),
        _ => None,
    }
}

/// The abort every menu offers, which pauses the run.
fn aborting(id: ReservedOption) -> Option<(String, Option<String>)> {
    (id == ReservedOption::Abort).then(|| {
        (
            "Abort the run".to_string(),
            Some("Pauses here; nothing further executes".to_string()),
        )
    })
}
