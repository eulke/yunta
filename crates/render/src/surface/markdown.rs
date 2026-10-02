//! A document as Markdown: what a file the engine writes, or a pull
//! request, says. Nothing is laid out to a width — the viewer wraps — and
//! a mark is its Unicode glyph beside its word, since a viewer draws any
//! character and the word is what carries the meaning.

use super::Surface;
use crate::blocks::{
    Check, Checklist, Decision, Evidence, FailureDetail, FailureSays, Fields, Headline, Next,
    NodeTable, Whole,
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
            .map(block)
            .filter(|drawn| !drawn.trim().is_empty())
            .collect();
        match blocks.is_empty() {
            true => String::new(),
            false => format!("{}\n", blocks.join("\n\n")),
        }
    }
}

fn block(block: &Block<'_>) -> String {
    match block {
        Block::Headline(headline) => self::headline(headline),
        Block::Fields(fields) => self::fields(fields),
        Block::NodeTable(table) => self::table(table),
        Block::Evidence(evidence) => self::evidence(evidence),
        Block::Failure(failure) => self::failure(failure),
        Block::Decision(decision) => self::decision(decision),
        Block::Checklist(list) => self::checklist(list),
        Block::Next(next) => self::next(next),
        Block::Lines(lines) => lines.iter().map(line).collect::<Vec<_>>().join("  \n"),
    }
}

fn headline(headline: &Headline) -> String {
    format!(
        "**{}**: {} {}",
        headline.subject,
        GLYPHS.mark(headline.mark),
        headline.said
    )
}

fn fields(fields: &Fields) -> String {
    fields
        .rows()
        .map(|(label, value)| format!("- {label}: {value}"))
        .collect::<Vec<_>>()
        .join("\n")
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
            out.push_str(&format!("  \n  `{}`", decision.command(option)));
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
            _ => span.text.clone(),
        })
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// Text for one table cell: on one line, and with the pipe that would
/// end the cell escaped.
fn cell(text: &str) -> String {
    yunta_core::text::one_line(text).replace('|', "\\|")
}
