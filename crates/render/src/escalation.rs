//! The words a decision is met by, wherever a person meets it: on the
//! menu they answer at the prompt, on the page `yunta status` prints,
//! and in the trailer that closes a parked run out.
//!
//! An escalation is one object on all three, so an option is one line on
//! all three: a reader who picked `retry` off a menu and then reads the
//! same run's page has to see the same option, not two renderings of it.

use surco::text;
use yunta_core::events::{
    Fact, FailureMenu, GateWaitingPayload, Menu, ProposedCheck, ScopeWait, WhyWithheld, Withheld,
};
use yunta_core::expansion::{Precheck, Waiting};
use yunta_core::listed_globs;
use yunta_core::protected::Protection;
use yunta_core::shown::withheld_because;

use crate::blocks::{departure_facts, stuck_words};

/// The facts a person reads beside an escalation's claim: what an older log
/// attached, what its menu's data says, and why each option is withheld.
pub fn facts(escalation: &GateWaitingPayload) -> Vec<Fact> {
    let mut facts = shown_facts(escalation);
    facts.extend(withheld_facts(escalation));
    facts
}

/// The lines the engine's own record is shown on, one fact to a line.
/// Empty when nothing is attached.
///
/// The facts come in the order a person reads them: an older log's record
/// first, then what the escalation's menu says it is about, then why each
/// option the gate withholds is withheld. A surface with room for the claim
/// as well shows both, one beside the other, since neither repeats the other.
pub fn evidence(escalation: &GateWaitingPayload) -> Vec<String> {
    lines(facts(escalation))
}

/// The facts a page reads beside the claim when it also says what weighs on
/// the decision: [`evidence`] without why an option is withheld, which
/// "before you decide" says.
pub fn noted(escalation: &GateWaitingPayload) -> Vec<String> {
    lines(shown_facts(escalation))
}

/// The facts beside the claim, ahead of why options are withheld: an older
/// log's record, then the menu's data. `noted` and `evidence` both show these.
fn shown_facts(escalation: &GateWaitingPayload) -> Vec<Fact> {
    let mut facts = escalation.evidence().facts();
    facts.extend(menu_facts(escalation));
    facts
}

/// One line per fact, in order; a fact that reads as nothing gets no line.
fn lines(facts: Vec<Fact>) -> Vec<String> {
    facts
        .iter()
        .map(crate::evidence::line)
        .filter(|line| !line.is_empty())
        .collect()
}

/// The facts a menu's data gives beside its claim, in the words a person reads.
fn menu_facts(escalation: &GateWaitingPayload) -> Vec<Fact> {
    match escalation.menu() {
        Some(Menu::Gate(gate)) => vec![Fact::labelled("assignee", gate.assignee.clone())],
        Some(Menu::Published(gate)) => vec![Fact::labelled("published at", gate.url.clone())],
        Some(Menu::Budget(cap)) => vec![
            Fact::labelled("limits.max_tokens_per_run", cap.cap.to_string()),
            Fact::labelled(
                "total input+output tokens derived from the event log",
                cap.spent.to_string(),
            ),
        ],
        Some(Menu::LoopCap(cap)) => vec![
            Fact::labelled(
                "iterations already run this invocation",
                cap.ran.to_string(),
            ),
            Fact::labelled("limits.max_loop_iterations", cap.cap.to_string()),
            Fact::bare("the tasks document still has ready tasks"),
        ],
        Some(Menu::Scope(wait)) => scope_facts(wait),
        Some(Menu::Failure(menu)) => failure_facts(menu),
        Some(Menu::Departure(menu)) => departure_facts(menu),
        Some(Menu::Console(_)) | None => Vec::new(),
    }
}

/// A failure's own lines and, when it offers no remedy, the way out.
fn failure_facts(menu: &FailureMenu) -> Vec<Fact> {
    let mut facts = crate::failure::facts(&menu.failure);
    facts.extend(
        menu.stuck
            .as_ref()
            .map(|stuck| Fact::labelled("way out", stuck_words(stuck))),
    );
    facts
}

/// The facts a scope request is shown with: the paths it asks for when there
/// are any, why the loop waits, and what its pre-check found.
fn scope_facts(wait: &ScopeWait) -> Vec<Fact> {
    let mut facts = Vec::new();
    if !wait.paths.is_empty() {
        facts.push(Fact::labelled("paths", listed_globs(&wait.paths)));
    }
    facts.push(Fact::labelled("waiting", waiting(wait)));
    facts.push(Fact::labelled(
        "pre-check",
        pre_check(wait.proposed.as_ref()),
    ));
    facts
}

/// Why the loop waits on the scope request, in the words a person reads.
fn waiting(wait: &ScopeWait) -> String {
    match wait.because {
        Waiting::ModeAsk => "the loop's scope mode is `ask`, so a person grants scope".to_string(),
        Waiting::SharedWithBatch => "another task of the batch may write these paths".to_string(),
        Waiting::CapExhausted { granted, cap } => {
            format!("{granted} of {} used", text::counted(cap as usize, "grant"))
        }
        Waiting::Instructions => Protection::AgentInstructions.words().to_string(),
        Waiting::Person => "a node's own request, which a person answers".to_string(),
    }
}

/// What the criterion a session proposed found when the engine ran it.
fn pre_check(proposed: Option<&ProposedCheck>) -> String {
    match proposed {
        None => "no criterion proposed".to_string(),
        Some(ProposedCheck { cmd, precheck }) => match precheck {
            Precheck::Passes => format!("`{cmd}` passes"),
            Precheck::Red { exit } => format!("`{cmd}` exits {exit}"),
            Precheck::CouldNotRun => format!("`{cmd}` could not run"),
            Precheck::NotRun => format!("`{cmd}` was not run"),
        },
    }
}

/// Why each option is withheld, for the reasons that are data: an older log
/// wrote the reason it stored into the record it attached, so a stored
/// reason is read from that record and not written again.
fn withheld_facts(escalation: &GateWaitingPayload) -> Vec<Fact> {
    escalation
        .withheld()
        .iter()
        .filter(|withheld| matches!(withheld.because, WhyWithheld::Unprovable { .. }))
        .map(|withheld| {
            Fact::labelled(
                "withheld",
                format!("`{}`, because {}", withheld.option, because(withheld)),
            )
        })
        .collect()
}

/// Why `withheld` is not offered, in the words a person reads.
pub fn because(withheld: &Withheld) -> String {
    match &withheld.because {
        WhyWithheld::Unprovable { flaws } => withheld_because(flaws.as_slice()).unwrap_or_default(),
        WhyWithheld::Logged(said) => said.as_str().to_string(),
    }
}

/// The escalation on one line, its claim and then the facts behind it,
/// for a surface with room for exactly one: the reason a run is parked
/// with, and the row a listing gives a run.
///
/// A surface with room for two parts heads each separately; a line has
/// room for neither heading. Composing them here is what keeps the page
/// and the line from disagreeing about what an escalation says.
pub fn sentence(payload: &GateWaitingPayload) -> String {
    let claim = crate::claim::sentence(payload.summary(), payload.failure());
    text::aside(claim, &evidence(payload).join("; "))
}

#[cfg(test)]
mod tests;
