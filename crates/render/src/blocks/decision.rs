//! The decisions a run's own facts are put to a person with: the
//! command each option of a terminal's decision names, and the words a
//! departure from the plan is put to a person with, its answers
//! included.

use surco::components::{Chosen, Decision, DecisionOption};
use surco::text;
use yunta_core::events::{DepartureMenu, Fact};
use yunta_core::reserved::ReservedOption;
use yunta_core::step::{Choice, NextStep};
use yunta_core::RunId;

/// The words over what the plan decided, where a session departs from it.
pub(crate) const THE_PLAN_DECIDED: &str = "the plan decided";

/// The words over what a session that departs from the plan says it does instead.
pub(crate) const SAYS_INSTEAD: &str = "the session says instead";

/// The words over why a session departs from the plan.
pub(crate) const BECAUSE: &str = "because";

/// A decision a terminal chooses for `run`: each option names the command
/// that chooses it.
pub fn here(run: &RunId, options: Vec<DecisionOption>) -> Decision {
    let commands = options
        .iter()
        .map(|option| {
            // An id that does not parse as an option names none: the line
            // then puts the decision to the person instead.
            let choice = match option.id.parse() {
                Ok(id) => Choice::Picked {
                    option: id,
                    words: option.asks.is_some(),
                },
                Err(_) => Choice::Asked,
            };
            let step = NextStep::ResolveGate {
                run: run.clone(),
                choice,
            };
            (option.id.clone(), crate::command::line(&step))
        })
        .collect();
    Decision {
        chosen: Chosen::Here { commands },
        options,
        otherwise: Vec::new(),
    }
}

/// What accepting the departures of a loop's task, or of each of `tasks`, does: its work
/// stands, and the plan the run judges keeps why it differs.
pub(crate) fn accepted(tasks: usize) -> String {
    let whose = match tasks {
        1 => "the work",
        _ => "each task's work",
    };
    format!("{whose} stands as it is, and the plan records why it differs")
}

/// What sending a departing task, or each of `tasks`, back does: its session picks the
/// work back up, held to the plan.
pub(crate) fn sent_back(tasks: usize) -> String {
    let whose = match tasks {
        1 => "the session",
        _ => "each session",
    };
    format!("{whose} goes back to what the plan says")
}

/// The label and the tradeoff of a departure's answer, written from the
/// departure's own facts: accepting it, and sending it back, each said by
/// what it does alone. `None` for an answer a departure does not offer.
pub(crate) fn departure_words(
    menu: &DepartureMenu,
    id: ReservedOption,
) -> Option<(Option<String>, Option<String>)> {
    match id {
        ReservedOption::Accept => Some((None, Some(accepting(menu)))),
        ReservedOption::SendBack => Some((None, Some(sent_back(1)))),
        _ => None,
    }
}

/// What accepting a departure does, in the words its option offers: the
/// work stands, and beside it the task stops being held to the criteria it
/// waives, and the test a run's spec gave it is written again by the node
/// that writes it.
fn accepting(menu: &DepartureMenu) -> String {
    let waived = (!menu.waived.is_empty()).then(|| {
        format!(
            "the task stops being held to {}",
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
        true => accepted(1),
        false => format!("{}; {}", accepted(1), changes.join(", and ")),
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
                Fact::labelled(THE_PLAN_DECIDED, deviation.planned.clone()),
                Fact::labelled(SAYS_INSTEAD, deviation.instead.clone()),
                Fact::labelled(BECAUSE, deviation.why.clone()),
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use surco::components::{Chosen, Decision, DecisionOption};
    use surco::doc::Doc;
    use surco::look::Look;
    use surco::surface::{Surface, Terminal};

    /// Each option draws with its words and what it asks, and no command
    /// under any of them: none is given for it to name.
    #[test]
    fn options_draw_their_words_and_name_no_command() {
        let decision = Decision {
            chosen: Chosen::Here {
                commands: BTreeMap::new(),
            },
            options: vec![
                DecisionOption {
                    id: "approve".to_string(),
                    label: Some("On to `implement`".to_string()),
                    tradeoff: Some("resolves this gate".to_string()),
                    asks: None,
                },
                DecisionOption {
                    id: "adjust".to_string(),
                    label: None,
                    tradeoff: Some("sends what you say back to `plan`".to_string()),
                    asks: Some("what should change?".to_string()),
                },
            ],
            otherwise: Vec::new(),
        };
        assert!(decision
            .options
            .iter()
            .all(|option| decision.command(option).is_none()));
        let drawn = Terminal::on(Look::plain()).draw(&Doc::new().with(decision));
        assert!(drawn.contains("approve: On to `implement`"), "{drawn}");
        assert!(drawn.contains("asks: what should change?"), "{drawn}");
        assert!(!drawn.contains("yunta"), "{drawn}");
        assert!(!drawn.contains("resolve-gate"), "{drawn}");
    }
}
