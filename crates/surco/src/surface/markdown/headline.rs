//! A document's headlines and title as Markdown: the headline it opens with,
//! the headlines under it, and the title that names it.

use super::{plain_text, Page, GLYPHS};
use crate::components::{Headline, Title};

/// A headline under a document's first block, a heading of the second level
/// over what it says
pub(super) fn headline(headline: &Headline) -> String {
    format!("## {}", said(headline))
}

/// A document's first headline: in a file, a heading of the first level over
/// what it says; in a description, a line of its own with its subject in bold,
/// since the forge's title heads the page
pub(super) fn first(headline: &Headline, page: Page) -> String {
    match page.description {
        true => said_with(headline, &format!("**{}**", plain_text(&headline.subject))),
        false => format!("# {}", said(headline)),
    }
}

/// A document's title: a heading of the first level in a file, and one of the
/// second in a description, under the title the forge draws
pub(super) fn title(title: &Title, page: Page) -> String {
    let level = match page.description {
        true => 2,
        false => 1,
    };
    format!("{} {}", "#".repeat(level), plain_text(line(title).trim()))
}

/// A title on one line: its name's text, then its facts after a colon, a comma
/// apart, when it has any
fn line(title: &Title) -> String {
    match title.facts.is_empty() {
        true => title.name.text(),
        false => format!("{}: {}", title.name.text(), title.facts.join(", ")),
    }
}

/// What a headline says: its mark, its subject, its word and the facts beside
/// it, and what names it
fn said(headline: &Headline) -> String {
    said_with(headline, &plain_text(&headline.subject))
}

/// What a headline says, with its subject given already written. The caller
/// writes the subject through `plain_text`, since it may sit in bold; each
/// word after it goes through `plain_text` here, so a sign HTML would read as
/// a tag is read as written
fn said_with(headline: &Headline, subject: &str) -> String {
    let sep = GLYPHS.sep();
    let mut said = format!(
        "{} {subject} {sep} {}",
        GLYPHS.mark(headline.mark),
        plain_text(&headline.said)
    );
    for fact in headline.facts.iter().chain(&headline.edge) {
        said.push_str(&format!(" {sep} {}", plain_text(fact)));
    }
    said
}
