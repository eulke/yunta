//! A document as Markdown: what a file the engine writes, or a pull
//! request, says. Nothing is laid out to a width — the viewer wraps — and
//! a mark is its Unicode glyph beside its word, since a viewer draws any
//! character and the word is what carries the meaning.

use super::Surface;
use crate::blocks::{
    Check, Checklist, Code, Decision, Evidence, FailureDetail, FailureSays, Fields, Headline,
    Marked, Next, NodeTable, Section, Whole,
};
use crate::doc::{Block, Doc};
use crate::ink::{Line, Tone};
use crate::Glyphs;

/// A Markdown file or a forge's page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Markdown;

/// The set a Markdown reader is drawn with: any viewer draws it.
const GLYPHS: Glyphs = Glyphs::Unicode;

impl Surface for Markdown {
    /// The document, its blocks separated by a blank line.
    type Output = String;

    fn draw(&self, doc: &Doc<'_>) -> String {
        let blocks: Vec<String> = doc
            .blocks()
            .iter()
            .enumerate()
            .map(|(at, drawn)| match (at, drawn) {
                // A document's first headline is its title.
                (0, Block::Headline(headline)) => format!("# {}", said(headline)),
                (_, Block::Title(title)) => format!("# {}", title.text().trim()),
                _ => block(drawn, 0),
            })
            .filter(|drawn| !drawn.trim().is_empty())
            .collect();
        match blocks.is_empty() {
            true => String::new(),
            false => format!("{}\n", blocks.join("\n\n")),
        }
    }
}

/// One block, `depth` sections deep.
fn block(block: &Block<'_>, depth: usize) -> String {
    match block {
        Block::Headline(headline) => self::headline(headline),
        Block::Fields(fields) => self::fields(fields),
        Block::NodeTable(table) => self::table(table),
        Block::Evidence(evidence) => self::evidence(evidence),
        Block::Failure(failure) => self::failure(failure),
        Block::Decision(decision) => self::decision(decision),
        Block::Checklist(list) => self::checklist(list),
        Block::Next(next) => self::next(next),
        Block::Heading(title) => format!("{} {title}", "#".repeat(3 + depth)),
        Block::Title(title) => format!("# {}", title.text().trim()),
        Block::Section(section) => self::section(section, depth),
        Block::Prose(prose) => prose.0.clone(),
        Block::Markdown(text) => text.trim().to_string(),
        Block::Marked(marked) => self::marked(marked),
        Block::Code(code) => self::code(code),
        Block::Lines(lines) => lines.iter().map(line).collect::<Vec<_>>().join("  \n"),
    }
}

fn headline(headline: &Headline) -> String {
    format!("## {}", said(headline))
}

/// What a headline says: its subject, and its mark beside its word.
fn said(headline: &Headline) -> String {
    format!(
        "{}: {} {}",
        headline.subject,
        GLYPHS.mark(headline.mark),
        headline.said
    )
}

fn fields(fields: &Fields) -> String {
    let mut items: Vec<String> = Vec::new();
    for (label, value, tone) in fields.rows() {
        let value = match tone {
            Tone::Command => format!("`{value}`"),
            _ => value.to_string(),
        };
        // A row with no label goes on saying what the row above it says.
        match (label.is_empty(), items.last_mut()) {
            (true, Some(item)) => item.push_str(&format!("  \n  {value}")),
            _ => items.push(format!("- {label}: {value}")),
        }
    }
    items.join("\n")
}

/// A section: its title as a heading a level under the document's, and
/// each of its blocks a level deeper.
fn section(section: &Section<'_>, depth: usize) -> String {
    let mark = section
        .mark
        .map(|mark| format!("{} ", GLYPHS.mark(mark)))
        .unwrap_or_default();
    let mut parts = vec![format!(
        "{} {mark}{}",
        "#".repeat(3 + depth),
        section.title.text().trim()
    )];
    parts.extend(
        section
            .blocks
            .iter()
            .map(|inner| block(inner, depth + 1))
            .filter(|drawn| !drawn.trim().is_empty()),
    );
    parts.join("\n\n")
}

fn marked(marked: &Marked) -> String {
    marked
        .items
        .iter()
        .map(|item| format!("- {} {item}", GLYPHS.mark(marked.mark)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Code under where it is and what it is there for, fenced in the
/// language of its file.
fn code(code: &Code) -> String {
    let mut out = format!("`{}`", code.at);
    if let Some(what) = &code.what {
        out.push_str(&format!(" — {what}"));
    }
    if !code.lines.is_empty() {
        out.push_str(&format!(
            "\n\n```{}\n{}\n```",
            code.language(),
            code.lines.join("\n")
        ));
    }
    if let Some(rest) = &code.rest {
        out.push_str(&format!(
            "\n\n{} more — {rest}",
            yunta_core::text::counted(code.whole - code.lines.len(), "line")
        ));
    }
    out
}

fn table(table: &NodeTable) -> String {
    let mut out = String::from("| | node | |\n|---|---|---|");
    for row in &table.rows {
        out.push_str(&format!(
            "\n| {} {} | `{}` | {} |",
            GLYPHS.mark(row.mark),
            row.word,
            row.id,
            cell(&row.note)
        ));
    }
    out
}

fn evidence(evidence: &Evidence) -> String {
    let mut out = format!("```\n{}\n```", evidence.quoted().join("\n"));
    let mut rest = Vec::new();
    let above = evidence.above();
    if above > 0 {
        rest.push(format!(
            "{} above",
            yunta_core::text::counted(above, "line")
        ));
    }
    if let Some(whole) = &evidence.whole {
        rest.push(match whole {
            Whole::File { shown, .. } => format!("whole output: `{shown}`"),
            Whole::Command(command) => format!("whole output: `{command}`"),
        });
    }
    if !rest.is_empty() {
        out.push_str(&format!(
            "\n\n{}",
            rest.join(&format!(" {} ", GLYPHS.sep()))
        ));
    }
    out
}

fn failure(failure: &FailureDetail<'_>) -> String {
    match failure.says() {
        FailureSays::Evidence(evidence) => self::evidence(&evidence),
        FailureSays::Text(lines) => lines.join("  \n"),
        FailureSays::Listed { heading, paths } => std::iter::once(heading)
            .chain(paths.iter().map(|path| format!("- `{}`", path.display())))
            .collect::<Vec<_>>()
            .join("\n"),
        FailureSays::Nothing => String::new(),
    }
}

fn decision(decision: &Decision) -> String {
    decision
        .options
        .iter()
        .map(|option| {
            let mut out = format!("- **{}**", option.id);
            if let Some(label) = &option.label {
                out.push_str(&format!(" — {label}"));
            }
            out.push_str(&format!("  \n  {}", option.tradeoff));
            if let Some(asks) = &option.asks {
                out.push_str(&format!("  \n  asks: {asks}"));
            }
            if let Some(command) = decision.command(option) {
                out.push_str(&format!("  \n  `{command}`"));
            }
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn checklist(list: &Checklist) -> String {
    list.checks()
        .map(
            |Check {
                 found,
                 subject,
                 said,
             }| { format!("- {} **{subject}** {said}", GLYPHS.mark(found.mark())) },
        )
        .collect::<Vec<_>>()
        .join("\n")
}

fn next(next: &Next) -> String {
    next.steps
        .iter()
        .map(|(command, gloss)| format!("- `{command}` — {gloss}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// One line of spans, its strong parts in bold.
fn line(line: &Line) -> String {
    line.spans()
        .iter()
        .map(|span| match span.tone {
            Tone::Strong if !span.text.trim().is_empty() => format!("**{}**", span.text.trim()),
            Tone::Command if !span.text.trim().is_empty() => format!("`{}`", span.text.trim()),
            _ => span.text.clone(),
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Text for one table cell: on one line, and with the pipe that would
/// end the cell escaped.
fn cell(text: &str) -> String {
    yunta_core::text::one_line(text).replace('|', "\\|")
}
