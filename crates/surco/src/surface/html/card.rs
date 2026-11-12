//! A decision put to a person, as a page draws it: its question as the page's
//! status, what stands under it, then its options as the cards a decision's
//! options are, the words being said for the one chosen, or a round's questions;
//! and its keys as the buttons a legend is. What a terminal fits to its rows, a
//! page draws whole.

use super::screen;
use super::text::{esc, mark, words};
use super::GLYPHS;
use crate::components::{Card, CardBody, ChoiceRow, Choices, RoundRow, RoundStanding, TypedRow};
use crate::glyphs::Mark;

/// A card as a page draws it, in one element.
pub(super) fn card(card: &Card) -> String {
    let mut parts = vec![head(card)];
    parts.extend(
        card.about
            .iter()
            .filter(|about| !about.is_empty())
            .map(|about| format!("<p>{}</p>", words(about))),
    );
    parts.extend(body(&card.body));
    parts.push(screen::legend(&card.legend));
    format!(r#"<div class="card">{}</div>"#, parts.join(""))
}

/// The question under the mark of what needs a person, in bold, with the card's
/// edge after it, as a page's status is drawn.
fn head(card: &Card) -> String {
    let edge = card
        .edge
        .as_deref()
        .map(|edge| format!(r#" <span class="muted">{}</span>"#, words(edge)))
        .unwrap_or_default();
    format!(
        r#"<p class="attention">{} <b>{}</b>{edge}</p>"#,
        mark(Mark::NeedsYou),
        words(&card.question)
    )
}

/// What the card holds under its question.
fn body(body: &CardBody) -> Vec<String> {
    match body {
        CardBody::Choosing { choices, then } => {
            let mut parts = vec![listed(choices)];
            if let Some(then) = then.as_deref().filter(|then| !then.is_empty()) {
                parts.push(format!(
                    r#"<p><span class="muted">then</span> {}</p>"#,
                    words(then)
                ));
            }
            parts
        }
        CardBody::Saying {
            chosen,
            asks,
            typed,
            refused,
        } => {
            let mut parts = vec![
                option(chosen, true),
                format!("<p><b>{}</b></p>", words(asks)),
                format!("<p>{}</p>", typed_line(typed)),
            ];
            if let Some(refused) = refused {
                parts.push(format!("<p>{} {}</p>", mark(Mark::Failed), words(refused)));
            }
            parts
        }
        CardBody::Round(rows) => vec![round(rows)],
    }
}

/// The options, each a card, the one pointed at marked by the pointer.
pub(super) fn listed(choices: &Choices) -> String {
    choices
        .rows
        .iter()
        .enumerate()
        .map(|(at, row)| option(row, at == choices.at))
        .collect()
}

/// One option as a decision's option is drawn: its name in bold, after the
/// pointer when it is pointed at, what it says and what it costs.
fn option(row: &ChoiceRow, pointed: bool) -> String {
    let pointer = match pointed {
        true => format!("{} ", GLYPHS.pointer()),
        false => String::new(),
    };
    let mut drawn = format!(r#"<div class="opt"><b>{pointer}{}</b>"#, esc(&row.name));
    if let Some(said) = &row.said {
        drawn.push_str(&format!(r#" <span class="does">{}</span>"#, words(said)));
    }
    if let Some(cost) = &row.cost {
        drawn.push_str(&format!(r#" <span class="costs">{}</span>"#, words(cost)));
    }
    drawn.push_str("</div>");
    drawn
}

/// The row words are typed on, whole, after what opens it.
pub(super) fn typed_line(typed: &TypedRow) -> String {
    format!(
        r#"<span class="muted">{}</span><code>{}{}</code>"#,
        esc(&typed.prompt),
        esc(&typed.before),
        esc(&typed.after)
    )
}

/// A round's questions as a list.
fn round(rows: &[RoundRow]) -> String {
    let items: String = rows.iter().map(question).collect();
    format!("<ul>{items}</ul>")
}

/// One question of a round: answered with its answer, asked with what is typed or
/// picked for it and what was refused, or to come.
fn question(row: &RoundRow) -> String {
    let asked = match &row.note {
        Some(note) => format!(
            r#"{} <span class="muted">{}</span>"#,
            words(&row.asked),
            words(note)
        ),
        None => words(&row.asked),
    };
    let inner = match &row.stands {
        RoundStanding::Answered(said) => {
            format!("{} {asked} <b>{}</b>", mark(Mark::Done), words(said))
        }
        RoundStanding::Ahead => format!("{} {asked}", mark(Mark::Absent)),
        RoundStanding::Typing { typed, refused } => format!(
            "{} <b>{asked}</b><br>{}{}",
            mark(Mark::NeedsYou),
            typed_line(typed),
            refusals(refused)
        ),
        RoundStanding::Picking {
            choices,
            filter,
            refused,
        } => {
            let filter = filter
                .as_deref()
                .map(|filter| {
                    format!(
                        r#"<br><span class="muted">filter</span> <b>{}</b>"#,
                        words(filter)
                    )
                })
                .unwrap_or_default();
            let listed = match choices {
                Some(choices) => listed(choices),
                None => r#"<p class="muted">nothing matches</p>"#.to_string(),
            };
            format!(
                "{} <b>{asked}</b>{filter}{listed}{}",
                mark(Mark::NeedsYou),
                refusals(refused)
            )
        }
    };
    format!("<li>{inner}</li>")
}

/// What the engine refused of an answer, each under the mark of a failure.
fn refusals(refused: &[String]) -> String {
    refused
        .iter()
        .map(|said| format!("<br>{} {}", mark(Mark::Failed), words(said)))
        .collect()
}
