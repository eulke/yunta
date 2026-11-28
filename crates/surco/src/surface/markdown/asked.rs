//! A line asked and a list picked from, written as Markdown: the question and the
//! words typed for it, whole, or its placeholder in emphasis while nothing is typed, and
//! its hint; what narrowed the list, the rows it leaves with the one pointed at, and its
//! keys. A form says each of its rows the same way, one after the other, then what they
//! come to; a survey says the blocks it holds.

use super::card::{listed, typed_line};
use super::{block, line, plain_text, rows, screen, Page, GLYPHS};
use crate::components::{ChoiceList, Form, FormValue, Survey, TypedAnswer, TypedRow};
use crate::glyphs::Mark;

/// The question, then the row its answer is typed on, whole, or the row's prompt and the
/// placeholder in emphasis where nothing is typed; then the hint, each a paragraph.
pub(super) fn typed_answer(asked: &TypedAnswer) -> String {
    let row = typed_row(&asked.typed, asked.placeholder.as_deref());
    std::iter::once(line(&asked.asked))
        .chain([row])
        .chain(asked.hint.as_deref().map(plain_text))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The row words are typed on, whole, or its prompt and `placeholder` in emphasis where
/// nothing is typed.
fn typed_row(typed: &TypedRow, placeholder: Option<&str>) -> String {
    match placeholder {
        Some(placeholder) if typed.is_empty() => {
            format!("{}_{}_", plain_text(&typed.prompt), plain_text(placeholder))
        }
        _ => typed_line(typed),
    }
}

/// A survey `depth` sections deep, written as `page` says: the blocks it says, one after
/// another, a blank line between two.
pub(super) fn survey(survey: &Survey, depth: usize, page: Page) -> String {
    let written: Vec<String> = (survey.blocks(GLYPHS).iter())
        .map(|said| block(said, depth, page))
        .collect();
    written.join("\n\n")
}

/// What the form asks, then its first row and its hint; then each other row, its label,
/// what is set on it, its description and what refused the act over it, under the mark of a
/// failure; then its foot's heading, each of its steps by its name and where it stands, as a
/// ribbon is written, and what it says: each a paragraph of its own.
pub(super) fn form(form: &Form) -> String {
    let mut parts: Vec<String> = form.asked.iter().map(line).collect();
    for (at, row) in form.rows.iter().enumerate() {
        parts.extend(row.label.iter().map(line));
        parts.push(match &row.value {
            FormValue::Typed { typed, placeholder } => typed_row(typed, placeholder.as_deref()),
            FormValue::Listed(choices) => listed(choices),
            FormValue::Words(choice) => screen::word_choice(choice),
        });
        if at == 0 {
            parts.extend(form.hint.as_deref().map(plain_text));
        }
        parts.extend(row.description.as_deref().map(plain_text));
        parts.extend(
            row.refusal
                .as_deref()
                .map(|refusal| format!("{} {}", GLYPHS.mark(Mark::Failed), plain_text(refusal))),
        );
    }
    if let Some(foot) = &form.foot {
        parts.push(line(&foot.heading));
        parts.push(screen::stretches(&foot.strokes));
        parts.push(rows(&foot.said));
    }
    parts.retain(|part| !part.trim().is_empty());
    parts.join("\n\n")
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
