//! The cursor's rules over a frame the walk drew: a list held where the person is shows the
//! cursor if and only if the row it is narrowed on stands in sight, on that row where typing
//! lands, read off the frame, so a list whose row gave way for room shows none; anywhere else
//! the cursor shows wherever a line is typed, only where a prompt is held, and inside what is
//! laid out.

use ratatui::buffer::Buffer;
use ratatui::layout::Position;
use surco::components::{CardBody, RoundStanding, Screen};
use surco::doc::{Block, Doc};
use surco::look::Look;
use surco::width::cell_width;

use super::MARGIN;

/// Where the cursor `frame`, drawn of `screen` in `look`, stands at breaks the rules, the
/// person typing where `typing` says.
pub fn broken(
    screen: &Screen,
    look: &Look,
    frame: &Buffer,
    cursor: Option<Position>,
    typing: bool,
) -> Option<String> {
    let size = (frame.area.width, frame.area.height);
    match held_list(screen) {
        Some(filter) => list_cursor_broken(filter.as_deref(), look, frame, cursor),
        None => line_cursor_broken(screen, cursor, typing, size),
    }
}

/// The list held where the person is, by what was typed on the row it is narrowed on, where
/// a list is held: in what is open over the screen where something is, else in the region
/// beside the main one or the main one, the first prompt of each; a list picked from or a
/// round's question picked from a list. None where the prompt held is no list, or none is.
fn held_list(screen: &Screen) -> Option<Option<String>> {
    let docs: Vec<&Doc> = match &screen.overlay {
        Some(overlay) => vec![&overlay.doc],
        None => [screen.side.as_ref().map(|side| &side.region.doc)]
            .into_iter()
            .flatten()
            .chain([&screen.main.doc])
            .collect(),
    };
    let held = docs
        .into_iter()
        .find_map(|doc| doc.prompt_at().map(|at| &doc.blocks()[at]))?;
    match held {
        Block::ChoiceList(list) => Some(list.filter.clone()),
        Block::Card(card) => match &card.body {
            CardBody::Round(rows) => rows.iter().find_map(|row| match &row.stands {
                RoundStanding::Picking { filter, .. } => Some(filter.clone()),
                _ => None,
            }),
            _ => None,
        },
        _ => None,
    }
}

/// Where the cursor breaks the rule of a list held where the person is, `filter` typed on
/// the row it is narrowed on: the cursor stands on that row where typing lands, past the
/// list's own pointer, over `type to filter` while nothing is typed and after the words
/// otherwise, wherever `frame`, drawn in `look`, shows that row; and nowhere where it shows
/// none, the row having given way for room.
fn list_cursor_broken(
    filter: Option<&str>,
    look: &Look,
    frame: &Buffer,
    cursor: Option<Position>,
) -> Option<String> {
    let said = filter.unwrap_or("type to filter");
    let opening = format!("{} ", look.glyphs.pointer());
    let lands: Vec<Position> = (0..frame.area.height)
        .filter_map(|y| {
            let row: Vec<String> = (0..frame.area.width)
                .map(|x| frame[(x, y)].symbol().to_string())
                .collect();
            let text = row.concat();
            let at = text.find(&format!("{opening}{said}"))?;
            let rest = &text[at + opening.len() + said.len()..];
            // Nothing of the list after the words, but an edge of the box or the region
            // beside it: a choice that only begins with them is no narrowed row.
            let after = rest.trim_start();
            let alone = after.is_empty() || after.starts_with('│') || after.starts_with('|');
            let x =
                cell_width(&text[..at]) + cell_width(&opening) + cell_width(filter.unwrap_or(""));
            alone.then(|| Position::new(u16::try_from(x).expect("a cell"), y))
        })
        .collect();
    match (lands.as_slice(), cursor) {
        ([], None) => None,
        ([], Some(at)) => Some(format!(
            "the cursor shows at {},{} where the list's narrowed row has given way",
            at.x, at.y
        )),
        (_, None) => Some("the list's narrowed row stands and no cursor shows".to_string()),
        (lands, Some(at)) if lands.contains(&at) => None,
        (lands, Some(at)) => Some(format!(
            "the cursor stands at {},{}, not where typing lands on the list's narrowed row \
             ({lands:?})",
            at.x, at.y
        )),
    }
}

/// Where the cursor breaks the layout's rules where no list is held: hidden while a line is
/// typed, shown where no prompt is held, or standing outside what is laid out.
fn line_cursor_broken(
    screen: &Screen,
    cursor: Option<Position>,
    typing: bool,
    (width, height): (u16, u16),
) -> Option<String> {
    let held = [
        screen.overlay.as_ref().map(|overlay| &overlay.doc),
        screen.side.as_ref().map(|side| &side.region.doc),
        Some(&screen.main.doc),
    ]
    .into_iter()
    .flatten()
    .any(|doc| doc.prompt_at().is_some());
    match cursor {
        None if typing => Some("a field is open and no cursor shows".to_string()),
        Some(_) if !held => Some("the cursor shows where no prompt is held".to_string()),
        Some(Position { x, y })
            if x < MARGIN || x >= width - MARGIN || y <= 1 || y >= height - 1 =>
        {
            Some(format!(
                "the cursor stands outside what is laid out, at {x},{y}"
            ))
        }
        _ => None,
    }
}
