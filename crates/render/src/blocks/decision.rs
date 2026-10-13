//! A decision a run waits on: every option, what it costs, and the
//! command that chooses it. The words a departure from the plan is put to
//! a person with, its answers included, are written here too.

use yunta_core::events::{DepartureMenu, Fact};
use yunta_core::reserved::ReservedOption;
use yunta_core::text;

use super::Drawn;
use crate::ink::{Line, Tone};
use crate::{cell_width, wrap, Look, INDENT};

/// One option on a decision's menu.
pub struct DecisionOption {
    pub id: String,
    /// What the option does. A label that says no more than the id is not
    /// drawn, so a caller can pass the words an option is read with.
    pub label: Option<String>,
    /// What choosing it costs — what makes the choice a decision and not a
    /// guess. Every option the engine offers carries one.
    pub tradeoff: Option<String>,
    /// What the option needs said with it, when it needs anything.
    pub asks: Option<String>,
}

impl DecisionOption {
    /// The label a menu prints after the id: nothing when the label says no
    /// more than the id.
    pub(crate) fn said_label(&self) -> Option<&str> {
        self.label
            .as_deref()
            .filter(|label| *label != self.id.as_str())
    }
}

/// A run's decision, every option with how it is chosen: at a terminal
/// a reader copies a line rather than composing one from a menu.
pub struct Decision {
    pub chosen: Chosen,
    pub options: Vec<DecisionOption>,
}

/// Where a decision's options are chosen.
pub enum Chosen {
    /// At a terminal, by the command each option names, for the run
    /// called `handle`.
    Here { handle: String },
    /// On the forge holding the pull request: each option's label says
    /// what to do there, and no command chooses it.
    OnTheForge,
}

impl Decision {
    /// The command that chooses `option`, with the words it asks for
    /// left for the reader to write — when a command chooses it.
    pub fn command(&self, option: &DecisionOption) -> Option<String> {
        let Chosen::Here { handle } = &self.chosen else {
            return None;
        };
        let text = match option.asks {
            Some(_) => " --text \"<answer>\"",
            None => "",
        };
        Some(format!("yunta resolve-gate {handle} {}{text}", option.id))
    }
}

/// The label and the tradeoff of a departure's answer, written from the
/// departure's own facts: accepting it, and sending it back. `None` for an
/// answer a departure does not offer.
pub(crate) fn departure_words(
    menu: &DepartureMenu,
    id: ReservedOption,
) -> Option<(String, Option<String>)> {
    match id {
        ReservedOption::Accept => Some(("Accept the departure".to_string(), Some(accepting(menu)))),
        ReservedOption::SendBack => Some((
            "Send it back".to_string(),
            Some("Its session picks the work back up with what you say".to_string()),
        )),
        _ => None,
    }
}

/// What accepting a departure does, in the words its option offers: the task
/// stops being held to the criteria it waives, and the test a run's spec
/// gave it is written again by the node that writes it.
fn accepting(menu: &DepartureMenu) -> String {
    let waived = (!menu.waived.is_empty()).then(|| {
        format!(
            "stops being held to {}",
            text::listed(menu.waived.iter().map(String::as_str))
        )
    });
    let rewritten = menu.rewritten_by.as_ref().map(|writer| {
        format!(
            "its tests are written again by `{writer}` from this departure and what you \
             say, and it goes on held to the new ones"
        )
    });
    let changes: Vec<String> = waived.into_iter().chain(rewritten).collect();
    match changes.is_empty() {
        true => "The task closes on the work as it stands, if its criteria pass; the rest of \
                 the plan builds on what it did instead"
            .to_string(),
        false => format!(
            "The task {} and closes on the work as it stands, if its other criteria pass; the \
             rest of the plan builds on what it did instead",
            changes.join(", and ")
        ),
    }
}

/// What a departure's question records: each departure in its session's own
/// words, and that accepting it is not offered when no node writes the tests
/// it departs from again.
pub(crate) fn departure_facts(menu: &DepartureMenu) -> Vec<Fact> {
    let mut facts: Vec<Fact> = menu
        .deviations
        .iter()
        .flat_map(|deviation| {
            [
                Fact::labelled("departs from", deviation.from.to_string()),
                Fact::labelled("the plan says", deviation.planned.clone()),
                Fact::labelled("its work instead", deviation.instead.clone()),
                Fact::labelled("because", deviation.why.clone()),
            ]
        })
        .collect();
    if !menu.accept_offered {
        facts.push(Fact::labelled(
            "not offered: accepting",
            "the tests came with the run, and no node of it writes them again",
        ));
    }
    facts
}

impl Drawn for Decision {
    fn lines(&self, look: &Look) -> Vec<Line> {
        let under = format!("{INDENT}{INDENT}");
        let room = look.width.cells().saturating_sub(cell_width(&under));
        let mut lines = Vec::new();
        for option in &self.options {
            let mut head = Line::new()
                .plain(INDENT)
                .push(Tone::Strong, option.id.as_str());
            if let Some(label) = option.said_label() {
                head = head.plain(" — ").plain(label);
            }
            lines.push(head);
            let mut said: Vec<String> = option.tradeoff.iter().cloned().collect();
            said.extend(option.asks.iter().map(|asks| format!("asks: {asks}")));
            for part in said.iter().flat_map(|text| wrap(text, room)) {
                lines.push(Line::new().plain(under.as_str()).push(Tone::Muted, part));
            }
            if let Some(command) = self.command(option) {
                lines.push(
                    Line::new()
                        .plain(under.as_str())
                        .push(Tone::Strong, command),
                );
            }
        }
        lines
    }
}
