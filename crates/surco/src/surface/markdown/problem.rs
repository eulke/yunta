//! A problem and a refusal, as Markdown: the word in bold before what it
//! says, and what the terminal draws under it as the lines a viewer wraps.

use super::text::{code_span, plain_text};
use super::{code, GLYPHS};
use crate::components::{Problem, Refusal};

/// The problem's word in bold and what it says, then where it is and its
/// line with the part at fault marked under it, fenced as text; then what
/// helps fix it after an italic `help:`, the items a separator apart.
pub(super) fn problem(problem: &Problem) -> String {
    let mut out = format!(
        "**{}** {}",
        problem.severity.word(),
        plain_text(&problem.said)
    );
    if let Some(place) = &problem.at {
        out.push_str(&format!("  \nat {}", code_span(&place.named())));
        if let Some((source, before, len)) = place.fault() {
            let number = place.line;
            let pad = place.pad();
            let gutter = GLYPHS.gutter();
            let marks = GLYPHS.fault().to_string().repeat(len);
            let under = " ".repeat(crate::width::cell_width(&before));
            let body = format!("{number} {gutter} {source}\n{pad} {gutter} {under}{marks}");
            let fence = code::fence(&body);
            out.push_str(&format!("\n\n{fence}text\n{body}\n{fence}"));
        }
    }
    if !problem.help.is_empty() {
        let help: Vec<String> = problem.help.iter().map(|item| plain_text(item)).collect();
        let help = help.join(&format!(" {} ", GLYPHS.sep()));
        out.push_str(&format!("\n\n_{}_ {help}", crate::surface::terminal::HELP));
    }
    out
}

/// What a command would not do, in bold, and where it looked under it.
pub(super) fn refusal(refusal: &Refusal) -> String {
    let said = format!(
        "**{}** {}",
        crate::surface::terminal::REFUSED,
        refusal
            .said
            .split('\n')
            .map(plain_text)
            .collect::<Vec<_>>()
            .join("  \n")
    );
    match &refusal.detail {
        Some(detail) => format!("{said}  \n_{}_", plain_text(detail)),
        None => said,
    }
}
