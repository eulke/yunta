//! A file's code as a page draws it: a figure whose caption says where the
//! code is and what it is there for, its lines colored in the language of
//! their file, and what is said under them and about the lines not shown.

use super::text;
use super::GLYPHS;
use crate::components::Code;

/// Code as a figure: its caption, its lines as written and colored, what is
/// said under them, and where the lines not shown are read.
pub(super) fn code(code: &Code) -> String {
    format!(
        "<figure>{}{}{}{}</figure>",
        caption(code),
        lines(code),
        foot(code),
        more(code)
    )
}

/// The caption: where the code is, then what it is there for, when it is
/// for anything.
fn caption(code: &Code) -> String {
    let title = crate::surface::code_title(code, GLYPHS).join(" ");
    let what = match &code.what {
        Some(what) => format!(" <span>{}</span>", text::words(what)),
        None => String::new(),
    };
    format!("<figcaption>{}{what}</figcaption>", text::esc(&title))
}

/// The lines shown, each word in the tone of what it is in the code's
/// language, one line of the page each. Empty when no line is shown.
fn lines(code: &Code) -> String {
    if code.lines.is_empty() {
        return String::new();
    }
    let lines: Vec<String> = crate::surface::diff::lines(code, GLYPHS)
        .iter()
        .map(text::line)
        .collect();
    format!("<pre class=\"code\">{}</pre>", lines.join("\n"))
}

/// What is said under the lines: whether the code is changed against what
/// stands where it goes, and how many lines it has. Empty when nothing is
/// said.
fn foot(code: &Code) -> String {
    let said = code.footer();
    if said.is_empty() {
        return String::new();
    }
    let sep = format!(" {} ", GLYPHS.sep());
    format!(
        "<p class=\"foot\">{}</p>",
        text::esc(&said.join(sep.as_str()))
    )
}

/// Where the lines not shown are read, as a command to copy, with how many
/// there are. Empty when every line is shown.
fn more(code: &Code) -> String {
    let Some(rest) = &code.rest else {
        return String::new();
    };
    // The lines not shown are the file's lines past those shown; a count
    // never goes below none.
    let hidden = code.whole.saturating_sub(code.lines.len());
    format!(
        "<p class=\"foot\">{} more:</p>{}",
        text::esc(&crate::text::counted(hidden, "line")),
        text::command(rest)
    )
}
