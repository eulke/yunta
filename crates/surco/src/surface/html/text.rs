//! The words of a page, each escaped and dressed for a browser: a span of a
//! line, a word a person wrote, a command a reader copies, a mark, and the
//! steps a page ends on. A class names a tone; no color is written here.

use super::GLYPHS;
use crate::glyphs::Mark;
use crate::line::{Fill, Line, Span, Tone};
use crate::surface::markdown::fold::code_pairs;

/// `text` escaped for markup: the four signs a browser reads as markup written
/// as character references, every other sign as written.
pub(super) fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for sign in text.chars() {
        match sign {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(sign),
        }
    }
    out
}

/// The class a tone is dressed with on a page, or `None` for a tone that has
/// its own tag or none at all: plain, strong, a command, an insertion and a
/// removal.
pub(super) fn class_of(tone: Tone) -> Option<&'static str> {
    match tone {
        Tone::Plain | Tone::Strong | Tone::Command | Tone::Added | Tone::Removed => None,
        Tone::Muted => Some("muted"),
        Tone::Faint => Some("faint"),
        Tone::Done => Some("done"),
        Tone::Failed => Some("failed"),
        Tone::Running => Some("running"),
        Tone::NeedsYou => Some("needs"),
        Tone::Caution => Some("caution"),
        Tone::Code(syntax) => Some(code_class(syntax)),
    }
}

/// The class a word of code is dressed with, by what the word is.
fn code_class(syntax: crate::line::Syntax) -> &'static str {
    use crate::line::Syntax;
    match syntax {
        Syntax::Keyword => "kw",
        Syntax::Type => "ty",
        Syntax::Function => "fn",
        Syntax::Text => "st",
        Syntax::Number => "nu",
        Syntax::Comment => "cm",
        Syntax::Attribute => "at",
        Syntax::Macro => "ma",
    }
}

/// One line of spans as markup, each span dressed by its tone, its fill and
/// its link, one after another.
pub(super) fn line(line: &Line) -> String {
    line.spans().iter().map(span).collect()
}

/// A span as markup: its text in its tone, as a pill where it is one, then
/// wrapped in its link when it has one.
fn span(span: &Span) -> String {
    let inner = match span.fill {
        Some(Fill::Pill) => pill(span.tone, &span.text),
        _ => toned(span.tone, &span.text),
    };
    match &span.link {
        Some(link) => format!(r#"<a href="{}">{inner}</a>"#, esc(link)),
        None => inner,
    }
}

/// A title as markup: a line each of whose spans sets a pair of backticks as code,
/// as words a person wrote do. A line of a file is not one: a backtick there is
/// the code's.
pub(super) fn title_words(title: &Line) -> String {
    title
        .spans()
        .iter()
        .map(|part| code_pairs(&span(part)))
        .collect()
}

/// `text` as a pill: its tone's class beside the word `pill`.
fn pill(tone: Tone, text: &str) -> String {
    let class = match class_of(tone) {
        Some(class) => format!("pill {class}"),
        None => "pill".to_string(),
    };
    format!(r#"<span class="{class}">{}</span>"#, esc(text))
}

/// `text` in `tone` as markup. A span of only spaces is written as it is, so no
/// tag holds nothing to see; but a change of only spaces is still a change.
fn toned(tone: Tone, text: &str) -> String {
    match tone {
        Tone::Added => format!("<ins>{}</ins>", esc(without(text, GLYPHS.added()))),
        Tone::Removed => format!("<del>{}</del>", esc(without(text, GLYPHS.removed()))),
        _ if text.trim().is_empty() => esc(text),
        Tone::Plain => esc(text),
        Tone::Strong => format!("<b>{}</b>", esc(text)),
        Tone::Command => format!(r#"<code class="command">{}</code>"#, esc(unquoted(text))),
        _ => match class_of(tone) {
            Some(class) => format!(r#"<span class="{class}">{}</span>"#, esc(text)),
            None => esc(text),
        },
    }
}

/// `text` without the marks a change puts around its words: the mark that
/// opens it and the one that closes it, each taken off only where it is there.
fn without<'a>(text: &'a str, [open, close]: [&str; 2]) -> &'a str {
    text.strip_prefix(open)
        .and_then(|rest| rest.strip_suffix(close))
        .unwrap_or(text)
}

/// `text` without the backticks of a code span Markdown hands over with them:
/// a run of backticks that opens it, and the same run that closes it.
fn unquoted(text: &str) -> &str {
    let run = text.len() - text.trim_start_matches('`').len();
    if run == 0 {
        return text;
    }
    let fence = &text[..run];
    text.strip_prefix(fence)
        .and_then(|rest| rest.strip_suffix(fence))
        .unwrap_or(text)
}

/// Words a person wrote or a builder composed: escaped, each pair of backticks
/// set as code, and each line end written as a break, so a value in two lines
/// says two things.
pub(super) fn words(text: &str) -> String {
    code_pairs(&esc(text)).replace('\n', "<br>")
}

/// A command a reader copies: a button whose text is the command, and whose
/// word says that it is copied.
pub(super) fn command(command: &str) -> String {
    let said = esc(command);
    format!(
        r#"<button type="button" class="cmd" data-copy="{said}"><span class="line">{said}</span> <span class="act">copy</span></button>"#
    )
}

/// A mark: its glyph in the class of its tone. Every mark has a tone with a
/// class, so the class is never left empty.
pub(super) fn mark(mark: Mark) -> String {
    let class = class_of(Tone::of(mark)).unwrap_or_default();
    format!(r#"<span class="mark {class}">{}</span>"#, GLYPHS.mark(mark))
}

/// The steps a page ends on: each command a reader copies beside what it does,
/// each command on one line.
pub(super) fn steps(steps: &[(String, &'static str)]) -> String {
    let rows: String = steps
        .iter()
        .map(|(command_text, gloss)| {
            let command_text = crate::text::one_line(command_text);
            format!(
                r#"{} <span class="does">{}</span>"#,
                command(&command_text),
                words(gloss)
            )
        })
        .collect();
    format!(r#"<div class="steps">{rows}</div>"#)
}
