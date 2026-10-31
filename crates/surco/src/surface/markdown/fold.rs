//! A fold as Markdown: a section a reader opens, drawn closed under a summary
//! that HTML reads, or only as the line of its title where a description
//! leaves folds out or nothing the fold holds draws.

use super::{block, lead, line, Leaves, Page};
use crate::components::Section;
use crate::line::{Line, Tone};

/// A fold of `section`, `depth` sections deep, written as `page` says: closed
/// under its summary with the blocks it holds, or only as the line of its
/// title when a description leaves folds out or its blocks draw nothing
pub(super) fn fold(section: &Section, depth: usize, page: Page) -> String {
    if page.leaves == Leaves::Folds {
        return titled_line(section);
    }
    let inside = Page {
        folded: true,
        ..page
    };
    let blocks: Vec<String> = section
        .blocks
        .iter()
        .map(|inner| block(inner, depth + 1, inside))
        .filter(|drawn| !drawn.trim().is_empty())
        .collect();
    match blocks.is_empty() {
        true => titled_line(section),
        false => format!(
            "<details>\n\n<summary>{}</summary>\n\n{}\n\n</details>",
            summary(section),
            blocks.join("\n\n")
        ),
    }
}

/// A fold's title as a line of Markdown: its lead, then its strong words in
/// bold, its commands in code and the rest as written, with each sign HTML
/// would read as a tag escaped
fn titled_line(section: &Section) -> String {
    format!("{}{}", lead(section), line(&section.title))
}

/// A fold's summary: its lead, then its title as HTML reads it
fn summary(section: &Section) -> String {
    format!("{}{}", lead(section), html(&section.title).trim())
}

/// A title as HTML reads it: each span's text with the signs HTML reads as
/// markup escaped, a strong span in bold, a command span in code, and each
/// pair of backticks in the rest in code
fn html(title: &Line) -> String {
    title
        .spans()
        .iter()
        .map(|span| match span.tone {
            Tone::Strong => format!("<b>{}</b>", code_pairs(&escaped(&span.text))),
            Tone::Command => format!("<code>{}</code>", escaped(&span.text)),
            _ => code_pairs(&escaped(&span.text)),
        })
        .collect()
}

/// `text` with the signs HTML reads as markup escaped; the ampersand goes
/// first, so the escapes of the other two stay whole
fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// `text` with each pair of backticks in it set as code, and a backtick with
/// no pair left as written
fn code_pairs(text: &str) -> String {
    let mut parts = text.split('`');
    let mut out = parts.next().unwrap_or_default().to_string();
    while let Some(code) = parts.next() {
        match parts.next() {
            Some(after) => out.push_str(&format!("<code>{code}</code>{after}")),
            None => out.push_str(&format!("`{code}")),
        }
    }
    out
}
