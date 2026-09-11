//! The decision a person makes when the run stops and asks.
//!
//! One component for every escalation the engine raises, because they
//! are one object: a summary of what happened, the evidence the engine
//! attached to it, and options that each declare what choosing them
//! trades off. A gate whose re-routes ran out, a promotion, a scope
//! expansion put to a person, a budget or loop cap — they differ in
//! what they say, never in how they are answered.

use yunta_core::events::{GateWaitingPayload, HumanChoice};
use yunta_core::OptionId;

use super::field::ask_line;
use super::menu::{choose, Choice};
use super::{attributed, Answered, Console, ANSWER, PARKS};

/// Free text is offered on every decision, whatever was on the menu:
/// the menu is there to make the common answer quick, never to be the
/// only answer available.
const ASIDE: &str = "anything to add?";

/// Puts `escalation` to the person and returns what they decided.
pub(crate) fn decide(console: &Console, escalation: &GateWaitingPayload) -> Answered<HumanChoice> {
    present(console, escalation)?;
    let option = choose(console, "choose", options(escalation))?;
    console.say(&format!("chose `{option}`"))?;
    console.say(&format!(
        "{ASIDE} (enter records the decision as it stands, {PARKS})"
    ))?;
    let aside = ask_line(console, ANSWER)?.value;
    let by = attributed(console)?;
    Ok(HumanChoice {
        option,
        by,
        free_text: (!aside.is_empty()).then_some(aside),
    })
}

/// Draws what the decision is about.
///
/// The evidence goes above the options because it is what the summary
/// is audited against: the summary is an agent's account of what
/// happened and the evidence is the engine's own record of it, so a
/// menu offered without the evidence asks for a decision on a claim
/// nobody checked.
fn present(console: &Console, escalation: &GateWaitingPayload) -> std::io::Result<()> {
    console.say("")?;
    console.say("a decision is needed")?;
    console.block(&escalation.summary, "  ")?;
    if !escalation.evidence.trim().is_empty() {
        console.say("")?;
        console.say("evidence, attached by the engine from the run's own log")?;
        console.block(&escalation.evidence, "  ")?;
    }
    console.say("")
}

/// The menu, an option to a line with what it trades off underneath.
fn options(escalation: &GateWaitingPayload) -> Vec<Choice<OptionId>> {
    escalation
        .options
        .iter()
        .map(|option| Choice {
            head: format!("{} — {}", option.id, option.label),
            detail: Some(format!("tradeoff: {}", option.tradeoff)),
            value: option.id.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use yunta_core::events::GateOption;

    fn escalation() -> GateWaitingPayload {
        GateWaitingPayload {
            summary: "T007 failed three times".to_string(),
            evidence: "criteria_checked: 2/3 green".to_string(),
            options: vec![GateOption {
                id: "approve".into(),
                label: "Add an in-memory session store".to_string(),
                tradeoff: "Unblocks now; one more task on the ledger".to_string(),
            }],
            external_ref: None,
        }
    }

    #[test]
    fn an_option_carries_its_tradeoff_as_the_line_under_it() {
        let drawn = options(&escalation());
        let first = drawn.first().map(|choice| &choice.detail);
        assert_eq!(
            first,
            Some(&Some(
                "tradeoff: Unblocks now; one more task on the ledger".to_string()
            )),
            "every option declares what it trades off, and the menu shows it"
        );
    }
}
