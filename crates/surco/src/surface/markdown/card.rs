//! A decision put to a person, written as Markdown: its question with its edge,
//! what stands under it, then its options with the one pointed at, the words
//! being said for the one chosen, or a round's questions; and its keys. What a
//! terminal fits to its rows, a file says whole.

use super::{plain_text, screen, GLYPHS};
use crate::components::{Card, CardBody, ChoiceRow, Choices, RoundRow, RoundStanding, TypedRow};
use crate::glyphs::Mark;

/// A card as a Markdown file says it, each part a paragraph of its own.
pub(super) fn card(card: &Card) -> String {
    let mut parts = vec![head(card)];
    parts.extend(
        card.about
            .iter()
            .filter(|about| !about.is_empty())
            .map(|about| plain_text(about)),
    );
    parts.extend(body(&card.body));
    let keys = screen::legend(&card.legend);
    if !keys.is_empty() {
        parts.push(keys);
    }
    parts.join("\n\n")
}

/// The question under the mark of what needs a person, in bold, with the card's
/// edge after it.
fn head(card: &Card) -> String {
    let question = format!(
        "{} **{}**",
        GLYPHS.mark(Mark::NeedsYou),
        plain_text(&card.question)
    );
    match &card.edge {
        Some(edge) => format!("{question} {} {}", GLYPHS.sep(), plain_text(edge)),
        None => question,
    }
}

/// What the card holds under its question, each part a paragraph.
fn body(body: &CardBody) -> Vec<String> {
    match body {
        CardBody::Choosing { choices, then } => {
            let mut parts = vec![listed(choices)];
            if let Some(then) = then.as_deref().filter(|then| !then.is_empty()) {
                parts.push(format!("then {}", plain_text(then)));
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
                format!("**{}**", plain_text(asks)),
                typed_line(typed),
            ];
            if let Some(refused) = refused {
                parts.push(format!(
                    "{} {}",
                    GLYPHS.mark(Mark::Failed),
                    plain_text(refused)
                ));
            }
            parts
        }
        CardBody::Round(rows) => vec![round(rows)],
    }
}

/// The options as a list, the one pointed at marked by the pointer.
pub(super) fn listed(choices: &Choices) -> String {
    choices
        .rows
        .iter()
        .enumerate()
        .map(|(at, row)| format!("- {}", option(row, at == choices.at)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// One option: the pointer when it is pointed at, its name in bold, what it says
/// and what it costs.
fn option(row: &ChoiceRow, pointed: bool) -> String {
    let mut said = match pointed {
        true => format!("{} **{}**", GLYPHS.pointer(), plain_text(&row.name)),
        false => format!("**{}**", plain_text(&row.name)),
    };
    for part in [&row.said, &row.cost].into_iter().flatten() {
        said.push(' ');
        said.push_str(&plain_text(part));
    }
    said
}

/// The row words are typed on, whole, with what opens it.
pub(super) fn typed_line(typed: &TypedRow) -> String {
    plain_text(&format!("{}{}{}", typed.prompt, typed.before, typed.after))
}

/// A round's questions as a list: each answered with its answer, each asked with
/// what is typed or picked for it and what was refused, and each to come.
fn round(rows: &[RoundRow]) -> String {
    rows.iter()
        .flat_map(question)
        .collect::<Vec<_>>()
        .join("\n")
}

/// One question of a round, as the lines of its list item.
fn question(row: &RoundRow) -> Vec<String> {
    let asked = match &row.note {
        Some(note) => format!("{} ({})", plain_text(&row.asked), plain_text(note)),
        None => plain_text(&row.asked),
    };
    match &row.stands {
        RoundStanding::Answered(said) => vec![format!(
            "- {} {asked}: **{}**",
            GLYPHS.mark(Mark::Done),
            plain_text(said)
        )],
        RoundStanding::Ahead => vec![format!("- {} {asked}", GLYPHS.mark(Mark::Absent))],
        RoundStanding::Typing { typed, refused } => {
            let mut lines = vec![
                format!("- {} **{asked}**", GLYPHS.mark(Mark::NeedsYou)),
                format!("  {}", typed_line(typed)),
            ];
            lines.extend(refusals(refused));
            lines
        }
        RoundStanding::Picking {
            choices,
            filter,
            refused,
        } => {
            let mut lines = vec![format!("- {} **{asked}**", GLYPHS.mark(Mark::NeedsYou))];
            if let Some(filter) = filter {
                lines.push(format!("  filter {}", plain_text(filter)));
            }
            match choices {
                Some(choices) => {
                    lines.extend(listed(choices).lines().map(|line| format!("  {line}")))
                }
                None => lines.push("  nothing matches".to_string()),
            }
            lines.extend(refusals(refused));
            lines
        }
    }
}

/// What the engine refused of an answer, each under the mark of a failure.
fn refusals(refused: &[String]) -> impl Iterator<Item = String> + '_ {
    refused
        .iter()
        .map(|said| format!("  {} {}", GLYPHS.mark(Mark::Failed), plain_text(said)))
}
