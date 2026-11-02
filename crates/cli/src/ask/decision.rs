//! The decision a person makes when the run stops and asks.
//!
//! One component for every escalation the engine raises, because they
//! are one object: a summary of what happened, the evidence the engine
//! attached to it where that evidence says more than the summary, and
//! options that each declare what choosing them trades off. A gate
//! whose re-routes ran out, a promotion, a scope expansion put to a
//! person, a budget or loop cap — they differ in what they say, never
//! in how they are answered.

use yunta_core::events::{ChangesCountedPayload, GateWaitingPayload, HumanChoice};
use yunta_core::{Env, OptionId, RunId};
use yunta_engine::ShownDocument;

use super::field::ask_line;
use super::keys::{Stroke, Strokes};
use super::menu::{choose, Choice};
use super::{attributed, Answered, Console, NoAnswer, ANSWER};
use crate::commands::advice;

/// What settles an option that asks for nothing: the decision as it
/// stands, or a note sent with it.
const SETTLE: &str = "enter records it, n adds a note";

/// Puts `escalation` to the person, with the documents it shows, and
/// returns what they decided. `shown` pairs the run the documents belong
/// to with the documents; a caller that holds no run shows none. `env`
/// is the ambient identity the decision is attributed to. `changed` is what
/// the run changed, which the page says.
pub(crate) fn decide(
    console: &Console,
    env: &Env,
    escalation: &GateWaitingPayload,
    shown: Option<(&RunId, &[ShownDocument])>,
    changed: Option<&ChangesCountedPayload>,
) -> Answered<HumanChoice> {
    present(console, escalation, shown, changed)?;
    let option = choose(console, "choose", options(escalation))?;
    console.say(&format!("chose `{option}`"))?;
    let asks = escalation
        .options()
        .iter()
        .find(|offered| offered.id == option)
        .and_then(yunta_render::offers::asks);
    let said = match asks {
        Some(asks) => Some(required(console, &asks)?),
        None => noted(console)?,
    };
    let by = attributed(console, env)?;
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

/// Draws the decision as one page, laid out by the render crate: the
/// documents it is about, then the question the engine asked, the record
/// it attached to audit that question, what weighs on it, and each option
/// with what it does and costs.
///
/// The decision comes last, beside the menu: on a terminal the last lines
/// printed are the ones on the screen when the menu asks, and a plan a
/// screen tall would push the question off it.
fn present(
    console: &Console,
    escalation: &GateWaitingPayload,
    shown: Option<(&RunId, &[ShownDocument])>,
    changed: Option<&ChangesCountedPayload>,
) -> std::io::Result<()> {
    let look = console.look();
    let documents = shown.map_or(&[][..], |(_, documents)| documents);
    let page = yunta_render::gate::page(
        yunta_render::gate::GatePage {
            shown,
            waits: None,
            note: None,
            changed: changed.and_then(|change| yunta_render::changes::of(change, look.glyphs)),
            form: yunta_render::shown::Form::Review,
            decision: yunta_render::gate::asked(escalation, documents),
        },
        &advice::spell,
    );
    for line in crate::render::draw(page, &look).lines() {
        console.say(line)?;
    }
    console.say("")
}

/// The menu's options, each named by its id alone. What an option does
/// and what choosing it costs is on the page above the prompt, so the
/// list does not say it again.
fn options(escalation: &GateWaitingPayload) -> Vec<Choice<OptionId>> {
    escalation
        .options()
        .iter()
        .map(|option| Choice::named(option.id.as_str(), option.id.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use surco::doc::Doc;
    use surco::look::Look;
    use yunta_core::events::{AuthorGate, Claim, Escalation, GateOption, Leads, Menu};
    use yunta_core::reserved::ReservedOption;
    use yunta_core::{NodeId, NonEmpty};

    /// A gate an author declared, offering `offered`.
    fn escalation_with(offered: NonEmpty<GateOption>) -> GateWaitingPayload {
        Escalation::new(
            Claim::NeedsDecision {
                node: NodeId::from_static("approve"),
            },
            Menu::Gate(AuthorGate {
                assignee: "lead".to_string(),
                node: None,
            }),
            offered,
        )
        .into_payload()
    }

    /// An option that takes the run on past the gate, to `ship`.
    fn leading() -> GateOption {
        GateOption::leading(
            OptionId::from_static("approve"),
            Leads::On {
                next: vec![NodeId::from_static("ship")],
                settles: false,
            },
        )
    }

    /// The rows name each option by its id alone, with no label: the page says what each does.
    #[test]
    fn options_are_named_by_their_id_alone() {
        let escalation = escalation_with(NonEmpty::from((
            leading(),
            vec![GateOption::reserved(ReservedOption::Abort)],
        )));
        let drawn = options(&escalation);
        let names: Vec<&str> = drawn.iter().map(|choice| choice.name.as_str()).collect();
        let values: Vec<&str> = drawn.iter().map(|choice| choice.value.as_str()).collect();
        assert_eq!(names, ["approve", "abort"]);
        assert_eq!(values, ["approve", "abort"]);
        assert!(
            drawn.iter().all(|choice| choice.label.is_none()),
            "what each option does and costs is on the page, not in the list"
        );
    }

    /// The lines the page above the prompt draws for `escalation`, each trimmed.
    fn page_lines(escalation: &GateWaitingPayload) -> Vec<String> {
        let section = yunta_render::gate::asked(escalation, &[]);
        crate::render::surface_lines(Doc::new().with(section), &Look::plain())
            .iter()
            .map(|line| line.text().trim().to_string())
            .collect()
    }

    /// The page above the prompt says what each option does and what choosing
    /// it costs: the words the list no longer carries.
    #[test]
    fn the_page_above_the_prompt_says_what_each_option_does_and_costs() {
        let escalation = escalation_with(NonEmpty::from((
            leading(),
            vec![GateOption::reserved(ReservedOption::Abort)],
        )));
        let lines = page_lines(&escalation);
        assert!(
            lines
                .iter()
                .any(|line| line.contains("approve: On to `ship`")),
            "{lines:#?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("resolves this gate; `ship` runs next")),
            "{lines:#?}"
        );
    }

    /// An option whose words are its id is drawn once, by its id.
    #[test]
    fn an_option_whose_label_is_its_id_is_drawn_once() {
        let escalation = escalation_with(NonEmpty::from((
            GateOption::reserved(ReservedOption::Approve),
            Vec::new(),
        )));
        let lines = page_lines(&escalation);
        let approve = "| approve";
        assert!(
            lines.iter().any(|line| line.as_str() == approve),
            "{lines:#?}"
        );
        assert!(
            !lines.iter().any(|line| line.contains("approve: approve")),
            "{lines:#?}"
        );
    }
}
