//! What warns a reader, drawn as HTML: a list whose items carry a mark, a
//! caution with what it means and what fixes it, a problem pointed at in its
//! line, and what a command would not do.

use super::text::{esc, mark, words};
use super::GLYPHS;
use crate::components::{Concern, Fix, Marked, Place, Problem, Refusal, Severity};
use crate::glyphs::Mark;
use crate::surface::terminal::{FIX, HELP, REFUSED, SO};
use crate::width::cell_width;

/// A list whose items each carry the same mark, each item beside its mark.
/// Nothing for a list with no items.
pub(super) fn marked(marked: &Marked) -> String {
    if marked.items.is_empty() {
        return String::new();
    }
    let items: String = marked
        .items
        .iter()
        .map(|item| {
            format!(
                "<li>{} <span>{}</span></li>",
                mark(marked.mark),
                words(item)
            )
        })
        .collect();
    format!(r#"<ul class="marked">{items}</ul>"#)
}

/// A caution: its fact with the caution's mark, then what it means and how to
/// make it hold under it.
pub(super) fn concern(concern: &Concern) -> String {
    format!(
        r#"<div class="concern"><p class="said">{} {}</p>{}</div>"#,
        mark(Mark::Caution),
        words(&concern.fact),
        hung(Some(concern.so.as_str()), Some(&concern.fix)),
    )
}

/// What a finding means and how to make it hold, as a list of its two parts
/// each under its label: the meaning, and the fix with the lines a person
/// pastes to make it hold. A part is drawn only where it has one, and no list
/// is drawn where neither does.
pub(super) fn hung(so: Option<&str>, fix: Option<&Fix>) -> String {
    let so_part = so.map(|meaning| format!("<dt>{}</dt><dd>{}</dd>", esc(SO), words(meaning)));
    let fix_part = fix.map(|fix| {
        format!(
            "<dt>{}</dt><dd>{}{}</dd>",
            esc(FIX),
            words(&fix.said),
            paste(&fix.paste)
        )
    });
    match (so_part, fix_part) {
        (None, None) => String::new(),
        (so_part, fix_part) => format!(
            r#"<dl class="hung">{}{}</dl>"#,
            so_part.unwrap_or_default(),
            fix_part.unwrap_or_default()
        ),
    }
}

/// The lines a person pastes to make a fix hold, each as written, in a block
/// of their own; nothing when there are none to paste.
fn paste(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let body: Vec<String> = lines.iter().map(|line| esc(line)).collect();
    format!("<pre>{}</pre>", body.join("\n"))
}

/// A problem: how grave it is and what it says, where it is, its line with the
/// part at fault marked under it, and what helps fix it.
pub(super) fn problem(problem: &Problem) -> String {
    let class = match problem.severity {
        Severity::Error => "failed",
        Severity::Warning => "caution",
    };
    let place = match &problem.at {
        Some(place) => at(place),
        None => String::new(),
    };
    format!(
        r#"<div class="problem"><p><span class="{class}">{}</span> {}</p>{place}{}</div>"#,
        problem.severity.word(),
        words(&problem.said),
        help(&problem.help),
    )
}

/// Where a problem is, named as a reader types it into an editor, then the
/// line it is on with the part at fault marked under it, when the line is known.
fn at(place: &Place) -> String {
    let named = format!(r#"<p class="at"><code>{}</code></p>"#, esc(&place.named()));
    match place.fault() {
        Some(fault) => format!("{named}{}", fault_pre(place, fault)),
        None => named,
    }
}

/// The line a problem is on, numbered and set in a gutter, with the marks of
/// its part at fault on a line under it, the marks in the failed tone.
fn fault_pre(place: &Place, (source, before, len): (String, String, usize)) -> String {
    let gutter = GLYPHS.gutter();
    let pad = place.pad();
    let under = " ".repeat(cell_width(&before));
    let marks = GLYPHS.fault().to_string().repeat(len);
    format!(
        "<pre>{} {gutter} {}\n{pad} {gutter} {under}<span class=\"failed\">{marks}</span></pre>",
        place.line,
        esc(&source),
    )
}

/// What helps fix a problem, the most direct first, after its label, each item
/// a separator apart; nothing when nothing helps.
fn help(items: &[String]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let said: Vec<String> = items.iter().map(|item| words(item)).collect();
    format!(
        r#"<p class="help">{} {}</p>"#,
        esc(HELP),
        said.join(format!(" {} ", GLYPHS.sep()).as_str())
    )
}

/// What a command would not do, in words, with where it looked beneath it when
/// the refusal gives a detail.
pub(super) fn refusal(refusal: &Refusal) -> String {
    let detail = match &refusal.detail {
        Some(detail) => format!(r#"<p class="detail">{}</p>"#, words(detail)),
        None => String::new(),
    };
    format!(
        r#"<div class="refusal"><p><span class="failed">{}</span> {}</p>{detail}</div>"#,
        esc(REFUSED),
        words(&refusal.said),
    )
}
