//! The decision a person makes when the run stops and asks.
//!
//! One component for every escalation the engine raises, because they
//! are one object: a summary of what happened, the evidence the engine
//! attached to it where that evidence says more than the summary, and
//! options that each declare what choosing them trades off. A gate
//! whose re-routes ran out, a promotion, a scope expansion put to a
//! person, a budget or loop cap — they differ in what they say, never
//! in how they are answered.

use yunta_core::events::{GateWaitingPayload, HumanChoice};
use yunta_core::OptionId;
use yunta_engine::ShownDocument;

use super::field::ask_line;
use super::keys::{Stroke, Strokes};
use super::menu::{choose, Choice};
use super::{attributed, Answered, Console, NoAnswer, ANSWER};
use crate::commands::status::decision::{account, before_you_decide, Beside};
use crate::render::blocks::{Drawn, Headline};
use crate::render::{label, Mark};

/// What settles an option that asks for nothing: the decision as it
/// stands, or a note sent with it.
const SETTLE: &str = "enter records it, n adds a note";

/// Puts `escalation` to the person, with the documents it shows, and
/// returns what they decided. `run` is the handle a document names the
/// command that shows it whole by.
pub(crate) fn decide(
    console: &Console,
    escalation: &GateWaitingPayload,
    shown: &[ShownDocument],
    run: &str,
) -> Answered<HumanChoice> {
    present(console, escalation, shown, run)?;
    let option = choose(console, "choose", options(escalation))?;
    console.say(&format!("chose `{option}`"))?;
    let asks = escalation
        .options()
        .iter()
        .find(|offered| offered.id == option)
        .and_then(|offered| offered.asks.as_deref());
    let said = match asks {
        Some(asks) => Some(required(console, asks)?),
        None => noted(console)?,
    };
    let by = attributed(console)?;
    Ok(HumanChoice {
        option,
        by,
        free_text: said,
    })
}

/// The words an option asks for, asked until they are given: the option
/// sends them to whoever works next, and an empty answer would send
/// nothing.
fn required(console: &Console, asks: &str) -> Answered<String> {
    console.say(&format!("{asks} ({})", console.escape().said()))?;
    loop {
        let said = ask_line(console, ANSWER)?.value;
        if !said.trim().is_empty() {
            return Ok(said);
        }
        console.say("this option needs an answer")?;
    }
}

/// What a person adds to an option that asks for nothing: one key
/// records the decision as it stands, and `n` opens a line for a note
/// sent with it. Nothing asked for words, so the common answer costs a
/// key rather than a line left empty.
fn noted(console: &Console) -> Answered<Option<String>> {
    console.say(&format!("{SETTLE}, {}", console.escape().said()))?;
    let mut strokes = Strokes::default();
    loop {
        match strokes.read(console.read_key()?) {
            Stroke::Enter => return Ok(None),
            Stroke::Insert('n' | 'N') => {
                let note = ask_line(console, &format!("note {ANSWER}"))?.value;
                return Ok((!note.is_empty()).then_some(note));
            }
            Stroke::Decline => return Err(NoAnswer::Declined),
            Stroke::Interrupt => {
                console.interrupt();
                return Err(NoAnswer::Interrupted);
            }
            _ => {}
        }
    }
}

/// Draws what the decision is about: the documents it shows, then that
/// it needs the person, the claim, the record the engine attached to
/// audit it against, and what weighs on it.
///
/// The documents come first and the question last, beside the menu: on a
/// terminal the last lines printed are the ones on the screen when the
/// menu asks, and a plan a screen tall would push the question off it.
/// The record goes above the options because a menu offered without it
/// asks for a decision on a claim nobody checked, and what weighs on the
/// decision goes last, against the options it weighs on. It is drawn as it is
/// on every surface that shows the decision, so a person who reads the
/// same run's page later reads the same block.
fn present(
    console: &Console,
    escalation: &GateWaitingPayload,
    shown: &[ShownDocument],
    run: &str,
) -> std::io::Result<()> {
    let look = console.look();
    for document in shown {
        console.say("")?;
        let doc = crate::render::shown::document(document, run, crate::render::shown::Form::Review);
        for line in crate::render::draw(doc, &look).lines() {
            console.say(line)?;
        }
    }
    let mut lines = Headline {
        subject: "decision".to_string(),
        mark: Mark::NeedsYou,
        said: "needs you".to_string(),
    }
    .lines(&look);
    let nothing_beside = Beside {
        claim: false,
        evidence: false,
    };
    lines.extend(account(escalation, nothing_beside, 1, &look));
    lines.extend(before_you_decide(escalation, shown, &look));
    console.say("")?;
    for line in &lines {
        console.say(&look.ink.paint(line))?;
    }
    console.say("")
}

/// The menu: every option by its id, with what it does and what
/// choosing it costs read above the list.
fn options(escalation: &GateWaitingPayload) -> Vec<Choice<OptionId>> {
    escalation
        .options()
        .iter()
        .map(|option| Choice {
            label: label(option).map(str::to_string),
            detail: Some(option.tradeoff.clone()),
            ..Choice::named(option.id.as_str(), option.id.clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use yunta_core::events::Escalation;
    use yunta_core::events::{Fact, GateOption};
    use yunta_core::NonEmpty;

    fn escalation() -> GateWaitingPayload {
        Escalation::new(
            "T007 failed three times",
            vec![Fact::labelled("criteria_checked", "2/3 green")].into(),
            NonEmpty::from((
                GateOption {
                    id: "approve".into(),
                    label: "Add an in-memory session store".to_string(),
                    tradeoff: "Unblocks now; one more task in the document".to_string(),
                    asks: None,
                },
                Vec::new(),
            )),
        )
        .expect("the summary states no fact the evidence holds")
        .into_payload()
    }

    #[test]
    fn an_option_is_named_by_its_id_and_carries_what_it_does_and_costs() {
        let drawn = options(&escalation());
        let first = drawn.first().expect("the menu offers the option");
        assert_eq!(first.name, "approve");
        assert_eq!(
            first.label.as_deref(),
            Some("Add an in-memory session store")
        );
        assert_eq!(
            first.detail.as_deref(),
            Some("Unblocks now; one more task in the document"),
            "every option declares what it trades off, and the menu shows it"
        );
    }
}
