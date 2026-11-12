//! A plan's desk, as a page draws it: its title with its edge, what it is about
//! and what was asked, each group of its rail as a list, the whole of what is
//! being read under its title, then what is being asked as the page's status,
//! what answers it and its keys as the buttons a legend is. What a terminal
//! lays out in columns and scrolls, a page draws whole and in order.

use super::blocks::{draw_all, Reader};
use super::screen;
use super::text::{esc, mark, words};
use super::GLYPHS;
use crate::components::{Desk, DeskFoot, FootRow, Pane, RailGroup, RailRow};
use crate::glyphs::Mark;

/// A desk as a page draws it, in one element.
pub(super) fn desk(reader: &mut Reader, desk: &Desk) -> String {
    let mut parts = vec![title(desk)];
    parts.extend(
        desk.gist
            .as_deref()
            .map(|gist| format!("<p>{}</p>", words(gist))),
    );
    parts.extend(desk.asked.as_deref().map(|asked| {
        format!(
            r#"<p><span class="muted">you asked</span> {}</p>"#,
            words(asked)
        )
    }));
    parts.extend(desk.rail.iter().map(group));
    parts.push(pane(reader, &desk.pane));
    parts.push(foot(&desk.foot));
    format!(r#"<div class="desk">{}</div>"#, parts.join(""))
}

/// The title's parts, the first in bold, with the desk's edge after them.
fn title(desk: &Desk) -> String {
    let sep = format!(" {} ", GLYPHS.sep());
    let parts: Vec<String> = desk
        .title
        .iter()
        .enumerate()
        .map(|(at, part)| match at {
            0 => format!("<b>{}</b>", words(part)),
            _ => format!(r#"<span class="muted">{}</span>"#, words(part)),
        })
        .chain(
            desk.edge
                .as_deref()
                .map(|edge| format!(r#"<span class="muted">{}</span>"#, words(edge))),
        )
        .collect();
    format!("<p>{}</p>", parts.join(&esc(&sep)))
}

/// A group of the rail: its title in bold, then each of its rows as a list item.
fn group(group: &RailGroup) -> String {
    let title = group
        .title
        .as_deref()
        .map(|title| format!("<p><b>{}</b></p>", words(title)))
        .unwrap_or_default();
    let rows: String = group
        .rows
        .iter()
        .map(|row| format!("<li>{}</li>", rail_row(row)))
        .collect();
    format!("{title}<ul>{rows}</ul>")
}

/// One row of the rail, as a terminal's rail says it.
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
                false => format!(r#"<span class="muted">{}</span> "#, words(lead)),
            };
            format!("{pointer}{lead}{}", words(name))
        }
        RailRow::Tally { what, count, key } => {
            let key = key
                .map(|key| format!(r#" <span class="key">{}</span>"#, esc(&key.to_string())))
                .unwrap_or_default();
            format!("{} <b>{count}</b>{key}", words(what))
        }
        RailRow::Weighs(said) => words(said),
        RailRow::Pair { left, right } => format!(
            r#"{} <span class="muted">{}</span> {}"#,
            words(left),
            GLYPHS.sep(),
            words(right)
        ),
        RailRow::Order(diagram) => format!("<code>{}</code>", esc(&diagram.source)),
    }
}

/// What is being read: its title in bold with where it stands and its edge, then
/// all of it, as a page draws a document.
fn pane(reader: &mut Reader, pane: &Pane) -> String {
    let beside: String = pane
        .place
        .iter()
        .chain(&pane.edge)
        .map(|part| format!(r#" <span class="muted">{}</span>"#, words(part)))
        .collect();
    format!(
        "<p><b>{}</b>{beside}</p>{}",
        words(&pane.title),
        draw_all(reader, pane.doc.blocks(), 2)
    )
}

/// What is being asked, as the page's status, what answers it, and its keys.
fn foot(foot: &DeskFoot) -> String {
    let question = format!(
        r#"<p class="attention">{} <b>{}</b></p>"#,
        mark(Mark::NeedsYou),
        words(&foot.question)
    );
    let answers = match &foot.row {
        FootRow::Options { named, more } => {
            let mut options: Vec<String> = named
                .iter()
                .enumerate()
                .map(|(at, name)| format!("<b>{}</b> {}", at + 1, words(name)))
                .collect();
            if *more > 0 {
                options.push(esc(
                    &(crate::text::counted(*more, "more option") + " on the card")
                ));
            }
            format!("<p>{}</p>", options.join("&nbsp;&nbsp; "))
        }
        FootRow::Confirming(option) => {
            let mut said = format!("<b>{}</b>", esc(&option.name));
            for part in [&option.said, &option.cost].into_iter().flatten() {
                said.push_str(&format!(" {}", words(part)));
            }
            format!("<p>{said}</p>")
        }
    };
    format!("{question}{answers}{}", screen::legend(&foot.legend))
}
