//! What a review found, written as Markdown: a finding an item, its answer an item under it.

use super::text::item;
use super::{plain_text, GLYPHS};
use crate::components::FindingList;

/// A finding an item: the pointer where it stands on one, its mark, its severity's word and its
/// title, bold where the pointer stands, then the answer of an answered one an item under it.
pub(super) fn findings(list: &FindingList) -> String {
    list.rows
        .iter()
        .enumerate()
        .map(|(at, row)| {
            let title = plain_text(&crate::text::one_line(&row.title));
            let (pointer, title) = match list.pointed == Some(at) {
                true => (format!("{} ", GLYPHS.pointer()), format!("**{title}**")),
                false => (String::new(), title),
            };
            let finding = format!(
                "- {pointer}{} {}: {title}",
                GLYPHS.mark(row.mark()),
                plain_text(&row.severity)
            );
            match &row.answer {
                Some(answer) => format!("{finding}\n{}", item("  - ", "    ", &plain_text(answer))),
                None => finding,
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
