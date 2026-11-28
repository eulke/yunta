//! A line asked and a list picked from, as a page draws them: the question and the
//! words typed for it, whole, or its placeholder faint while nothing is typed, and its
//! hint muted; what narrowed the list, its rows as a decision's options are drawn with
//! the one pointed at, and its keys as a legend's buttons. A form draws each of its rows the
//! same way, one after the other, then what they come to; a survey draws the blocks it holds.

use super::blocks::{draw_all, furrow, lines, Reader};
use super::card::{listed, typed_line};
use super::text::{esc, line, mark, words};
use super::{screen, GLYPHS};
use crate::components::{ChoiceList, Form, FormValue, TypedAnswer, TypedRow};
use crate::doc::{Block, Prompt};
use crate::glyphs::Mark;
use crate::words::Standing;

/// `block`, a line asked, a list, a form or a survey, `depth` sections deep, as a page draws
/// it: a survey as the blocks it holds; anything else, nothing.
pub(super) fn asked(reader: &mut Reader, block: &Block, depth: usize) -> String {
    match block.prompt() {
        Some(Prompt::TypedAnswer(asked)) => typed_answer(asked),
        Some(Prompt::ChoiceList(list)) => choice_list(list),
        Some(Prompt::Form(asked)) => form(asked),
        Some(Prompt::Survey(survey)) => draw_all(reader, &survey.blocks(GLYPHS), depth),
        Some(Prompt::Card(_) | Prompt::Desk(_)) | None => String::new(),
    }
}

/// The question, then the row its answer is typed on, whole, or the row's prompt and the
/// placeholder, faint, where nothing is typed; then the hint, muted.
pub(super) fn typed_answer(asked: &TypedAnswer) -> String {
    let row = typed_row(&asked.typed, asked.placeholder.as_deref());
    let hint = muted(asked.hint.as_deref());
    format!("<p>{}</p><p>{row}</p>{hint}", line(&asked.asked))
}

/// The row words are typed on, whole, or its prompt and `placeholder`, faint, where nothing
/// is typed.
fn typed_row(typed: &TypedRow, placeholder: Option<&str>) -> String {
    match placeholder {
        Some(placeholder) if typed.is_empty() => format!(
            r#"<span class="muted">{}</span><span class="faint">{}</span>"#,
            esc(&typed.prompt),
            words(placeholder)
        ),
        _ => typed_line(typed),
    }
}

/// `text` as a muted paragraph; nothing for none.
fn muted(text: Option<&str>) -> String {
    text.map(|text| format!(r#"<p class="muted">{}</p>"#, words(text)))
        .unwrap_or_default()
}

/// What the form asks, then its first row and its hint; then each other row, its label, what
/// is set on it, its description, muted, and what refused the act over it, under the mark of a
/// failure; then its foot: its heading, its strokes as a furrow, and what it says.
pub(super) fn form(form: &Form) -> String {
    let mut drawn: String = form
        .asked
        .iter()
        .map(|asked| format!("<p>{}</p>", line(asked)))
        .collect();
    for (at, row) in form.rows.iter().enumerate() {
        if let Some(label) = &row.label {
            drawn.push_str(&format!("<p>{}</p>", line(label)));
        }
        drawn.push_str(&match &row.value {
            FormValue::Typed { typed, placeholder } => {
                format!("<p>{}</p>", typed_row(typed, placeholder.as_deref()))
            }
            FormValue::Listed(choices) => format!(r#"<div class="list">{}</div>"#, listed(choices)),
            FormValue::Words(choice) => screen::word_choice(choice),
        });
        if at == 0 {
            drawn.push_str(&muted(form.hint.as_deref()));
        }
        drawn.push_str(&muted(row.description.as_deref()));
        if let Some(refusal) = &row.refusal {
            drawn.push_str(&format!("<p>{} {}</p>", mark(Mark::Failed), words(refusal)));
        }
    }
    if let Some(foot) = &form.foot {
        drawn.push_str(&format!("<p>{}</p>", line(&foot.heading)));
        let standings: Vec<Standing> = foot.strokes.iter().map(|step| step.standing).collect();
        drawn.push_str(&furrow(&standings));
        drawn.push_str(&lines(&foot.said));
    }
    format!(r#"<div class="form">{drawn}</div>"#)
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
