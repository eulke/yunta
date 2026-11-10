//! A line asked and a list picked from, written as Markdown: the question and the
//! words typed for it, whole; what narrowed the list, the rows it leaves with the
//! one pointed at, and its keys.

use super::card::{listed, typed_line};
use super::{line, plain_text, screen};
use crate::components::{ChoiceList, TypedAnswer};

/// The question, then the row its answer is typed on, whole.
pub(super) fn typed_answer(asked: &TypedAnswer) -> String {
    format!("{}\n\n{}", line(&asked.asked), typed_line(&asked.typed))
}

/// What was typed to narrow the list, the rows it leaves or that it leaves none, and
/// its keys, each a paragraph of its own.
pub(super) fn choice_list(list: &ChoiceList) -> String {
    let mut parts: Vec<String> = list
        .filter
        .as_deref()
        .map(|filter| format!("filter **{}**", plain_text(filter)))
        .into_iter()
        .collect();
    parts.push(match &list.choices {
        Some(choices) => listed(choices),
        None => "nothing matches".to_string(),
    });
    let keys = screen::legend(&list.legend);
    if !keys.is_empty() {
        parts.push(keys);
    }
    parts.join("\n\n")
}
