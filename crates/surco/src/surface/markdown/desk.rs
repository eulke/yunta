//! A plan's desk, written as Markdown: its title with its edge, what it is
//! about and what was asked, each group of its rail, the whole of what is
//! being read under its title, then what is being asked with what answers it
//! and its keys. What a terminal lays out in columns and scrolls, a file says
//! whole and in order.

use super::{plain_text, screen, Markdown, GLYPHS};
use crate::components::{Desk, DeskFoot, FootRow, Pane, RailGroup, RailRow};
use crate::glyphs::Mark;
use crate::surface::Surface;

/// A desk as a Markdown file says it, each part a paragraph of its own.
pub(super) fn desk(desk: &Desk) -> String {
    let mut parts = vec![title(desk)];
    parts.extend(desk.gist.as_deref().map(plain_text));
    parts.extend(
        desk.asked
            .as_deref()
            .map(|asked| format!("you asked: {}", plain_text(asked))),
    );
    parts.extend(desk.rail.iter().map(group));
    parts.push(pane(&desk.pane));
    parts.extend(foot(&desk.foot));
    parts.join("\n\n")
}

/// The title's parts, the first in bold, with the desk's edge after them.
fn title(desk: &Desk) -> String {
    let sep = format!(" {} ", GLYPHS.sep());
    let parts: Vec<String> = desk
        .title
        .iter()
        .enumerate()
        .map(|(at, part)| match at {
            0 => format!("**{}**", plain_text(part)),
            _ => plain_text(part),
        })
        .chain(desk.edge.as_deref().map(plain_text))
        .collect();
    parts.join(&sep)
}

/// A group of the rail: its title in bold, then each of its rows as a list item.
fn group(group: &RailGroup) -> String {
    let rows = group.rows.iter().map(|row| format!("- {}", rail_row(row)));
    group
        .title
        .as_deref()
        .map(|title| format!("**{}**", plain_text(title)))
        .into_iter()
        .chain(rows)
        .collect::<Vec<_>>()
        .join("\n")
}

/// One row of the rail: a place to stand on, marked by the pointer where the
/// person stands; a count, with the key that shows it; what weighs on the
/// decision; two things that go together; or the order things go in.
fn rail_row(row: &RailRow) -> String {
    match row {
        RailRow::Stop {
            lead,
            name,
            pointed,
        } => {
            let pointer = match pointed {
                true => format!("{} ", GLYPHS.pointer()),
                false => String::new(),
            };
            let lead = match lead.is_empty() {
                true => String::new(),
                false => format!("{} ", plain_text(lead)),
            };
            format!("{pointer}{lead}{}", plain_text(name))
        }
        RailRow::Tally { what, count, key } => {
            let key = key.map(|key| format!(" `{key}`")).unwrap_or_default();
            format!("{} {count}{key}", plain_text(what))
        }
        RailRow::Weighs(words) => plain_text(words),
        RailRow::Pair { left, right } => format!(
            "{} {} {}",
            plain_text(left),
            GLYPHS.sep(),
            plain_text(right)
        ),
        RailRow::Order(diagram) => plain_text(&diagram.source),
    }
}

/// What is being read: its title in bold with where it stands and its edge, then
/// all of it, as a file writes a document.
fn pane(pane: &Pane) -> String {
    let sep = format!(" {} ", GLYPHS.sep());
    let head: Vec<String> = std::iter::once(format!("**{}**", plain_text(&pane.title)))
        .chain(pane.place.as_deref().map(plain_text))
        .chain(pane.edge.as_deref().map(plain_text))
        .collect();
    let body = Markdown.draw(&pane.doc);
    format!("{}\n\n{}", head.join(&sep), body.trim_end())
}

/// What is being asked: the question under the mark of what needs a person, what
/// answers it, and its keys.
fn foot(foot: &DeskFoot) -> Vec<String> {
    let question = format!(
        "{} **{}**",
        GLYPHS.mark(Mark::NeedsYou),
        plain_text(&foot.question)
    );
    let answers = match &foot.row {
        FootRow::Options { named, more } => {
            let sep = format!(" {} ", GLYPHS.sep());
            let mut options: Vec<String> = named
                .iter()
                .enumerate()
                .map(|(at, name)| format!("`{}` {}", at + 1, plain_text(name)))
                .collect();
            if *more > 0 {
                options.push(crate::text::counted(*more, "more option") + " on the card");
            }
            options.join(&sep)
        }
        FootRow::Confirming(option) => {
            let mut said = format!("**{}**", plain_text(&option.name));
            for part in [&option.said, &option.cost].into_iter().flatten() {
                said.push(' ');
                said.push_str(&plain_text(part));
            }
            said
        }
    };
    let keys = screen::legend(&foot.legend);
    [question, answers, keys]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect()
}
