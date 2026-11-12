//! A line asked and a list picked from, as a page draws them: the question and the
//! words typed for it, whole; what narrowed the list, its rows as a decision's
//! options are drawn with the one pointed at, and its keys as a legend's buttons.

use super::card::{listed, typed_line};
use super::screen;
use super::text::{line, words};
use crate::components::{ChoiceList, TypedAnswer};

/// The question, then the row its answer is typed on, whole.
pub(super) fn typed_answer(asked: &TypedAnswer) -> String {
    format!(
        "<p>{}</p><p>{}</p>",
        line(&asked.asked),
        typed_line(&asked.typed)
    )
}

/// What was typed to narrow the list, the rows it leaves or that it leaves none, and
/// its keys.
pub(super) fn choice_list(list: &ChoiceList) -> String {
    let filter = list
        .filter
        .as_deref()
        .map(|filter| {
            format!(
                r#"<p><span class="muted">filter</span> <b>{}</b></p>"#,
                words(filter)
            )
        })
        .unwrap_or_default();
    let rows = match &list.choices {
        Some(choices) => listed(choices),
        None => r#"<p class="muted">nothing matches</p>"#.to_string(),
    };
    format!(
        r#"<div class="list">{filter}{rows}{}</div>"#,
        screen::legend(&list.legend)
    )
}
