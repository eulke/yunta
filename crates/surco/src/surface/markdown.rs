//! A document as Markdown: what a file the engine writes, or a pull
//! request, says. Nothing is laid out to a width — the viewer wraps — and
//! a mark is its Unicode glyph beside its word, since a viewer draws any
//! character and the word is what carries the meaning.

use crate::components::{
    Check, Checklist, Code, Concern, Decision, Edge, Evidence, Fields, Headline, Holds, Marked,
    Next, Paths, Problem, Refusal, Runs, Section, Table, Whole,
};
use crate::doc::{Block, Doc};
use crate::glyphs::Glyphs;
use crate::line::{Line, Tone};
use crate::surface::Surface;

/// A Markdown file or a forge's page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Markdown;

/// The set a Markdown reader is drawn with: any viewer draws it.
const GLYPHS: Glyphs = Glyphs::Unicode;

impl Surface for Markdown {
    /// The document, its blocks separated by a blank line.
    type Output = String;

    fn draw(&self, doc: &Doc) -> String {
        let blocks: Vec<String> = doc
            .blocks()
            .iter()
            .enumerate()
            .map(|(at, drawn)| match (at, drawn) {
                // A document's first headline is its title.
                (0, Block::Headline(headline)) => format!("# {}", said(headline)),
                // The furrow is a picture of what the facts below say in words.
                (_, Block::Furrow(_)) => String::new(),
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
fn block(block: &Block, depth: usize) -> String {
    match block {
        Block::Headline(headline) => self::headline(headline),
        Block::Furrow(_) => String::new(),
        Block::Gist(gist) => gist.0.clone(),
        Block::Fields(fields) => self::fields(fields),
        Block::Table(table) => self::table(table),
        Block::Runs(runs) => self::runs(runs),
        Block::Evidence(evidence) => self::evidence(evidence),
        Block::Said(said) => said.0.join("  \n"),
        Block::Paths(paths) => self::paths(paths),
        Block::Decision(decision) => self::decision(decision),
        Block::Checklist(list) => self::checklist(list),
        Block::Problem(problem) => self::problem(problem),
        Block::Refusal(refusal) => self::refusal(refusal),
        Block::Next(next) => self::next(next),
        Block::Heading(title) => format!("{} {title}", "#".repeat(3 + depth)),
        Block::Title(title) => format!("# {}", title.text().trim()),
        Block::Section(section) => self::section(section, depth),
        Block::Prose(prose) => prose.0.clone(),
        Block::Note(note) => format!("_{}_", note.0),
        Block::Markdown(text) => text.trim().to_string(),
        Block::Marked(marked) => self::marked(marked),
        Block::Concern(concern) => self::concern(concern),
        Block::Diagram(diagram) => format!("```mermaid\n{}\n```", diagram.source),
        Block::Code(code) => self::code(code),
        Block::Lines(lines) => lines.iter().map(line).collect::<Vec<_>>().join("  \n"),
    }
}

fn headline(headline: &Headline) -> String {
    format!("## {}", said(headline))
}

/// What a headline says: its mark, its subject, its word and the facts
/// beside it, and what names it.
fn said(headline: &Headline) -> String {
    let sep = GLYPHS.sep();
    let mut said = format!(
        "{} {} {sep} {}",
        GLYPHS.mark(headline.mark),
        headline.subject,
        headline.said
    );
    for fact in headline.facts.iter().chain(&headline.edge) {
        said.push_str(&format!(" {sep} {fact}"));
    }
    said
}

fn fields(fields: &Fields) -> String {
    let mut items: Vec<String> = Vec::new();
    let mut above = None;
    for field in fields.rows() {
        let value = match field.tone {
            Tone::Command => format!("`{}`", field.value),
            _ => field.value.clone(),
        };
        let value = match field.mark {
            Some(mark) => format!("{} {value}", GLYPHS.mark(mark)),
            None => value,
        };
        // A row with no label, or the label of the row above it, goes on
        // saying what that row says.
        let goes_on = field.label.is_empty() || above == Some(field.label);
        above = Some(field.label);
        match (goes_on, items.last_mut()) {
            (true, Some(item)) => item.push_str(&format!("  \n  {value}")),
            _ => items.push(format!("- {}: {value}", field.label)),
        }
    }
    items.join("\n")
}

/// A section: its title as a heading a level under the document's, and
/// each of its blocks a level deeper.
fn section(section: &Section, depth: usize) -> String {
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
        out.push_str(&format!(": {what}"));
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
            "\n\n{} more: {rest}",
            crate::text::counted(code.whole - code.lines.len(), "line")
        ));
    }
    out
}

/// A table with its header: an id as code, a row's mark in front of its
/// first cell.
fn table(table: &Table) -> String {
    let mut out = String::from("|");
    for column in &table.columns {
        match column.title {
            "" => out.push_str(" |"),
            title => out.push_str(&format!(" {title} |")),
        }
    }
    out.push_str(&format!("\n|{}", "---|".repeat(table.columns.len())));
    for row in &table.rows {
        out.push_str("\n|");
        for (at, (said, column)) in row.cells.iter().zip(&table.columns).enumerate() {
            let mut said = match column.holds {
                Holds::Id => format!("`{}`", said.text),
                Holds::State | Holds::Words | Holds::Figure | Holds::Rest => cell(&said.text),
            };
            if let (0, Some(mark)) = (at, row.mark) {
                said = format!("{} {said}", GLYPHS.mark(mark));
            }
            out.push_str(&format!(" {said} |"));
        }
    }
    out
}

/// Each run as a table row: where it stands, its handle, what it runs, what
/// holds it, and its edge.
fn runs(runs: &Runs) -> String {
    let mut out = String::from("| | run | workflow | where it stands | |\n|---|---|---|---|---|");
    for row in &runs.rows {
        let edge = match &row.edge {
            Edge::Age(age) => age.clone(),
            Edge::Command(command) => format!("`{command}`"),
        };
        out.push_str(&format!(
            "\n| {} {} | `{}` | {} ({}) | {} | {edge} |",
            GLYPHS.mark(row.standing.mark()),
            row.standing.word(),
            row.handle,
            cell(&row.workflow),
            cell(&row.mode),
            cell(&row.said),
        ));
    }
    out
}

fn evidence(evidence: &Evidence) -> String {
    let mut out = format!("```\n{}\n```", evidence.quoted().join("\n"));
    let mut rest = Vec::new();
    let above = evidence.above();
    if above > 0 {
        rest.push(format!("{} above", crate::text::counted(above, "line")));
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

fn paths(paths: &Paths) -> String {
    std::iter::once(paths.heading.clone())
        .chain(paths.paths.iter().map(|note| match &note.reason {
            Some(reason) => format!("- `{}` ({reason})", note.path.display()),
            None => format!("- `{}`", note.path.display()),
        }))
        .collect::<Vec<_>>()
        .join("\n")
}

fn decision(decision: &Decision) -> String {
    let options = decision
        .options
        .iter()
        .map(|option| {
            let mut out = format!("- **{}**", option.id);
            if let Some(label) = option.said_label() {
                out.push_str(&format!(": {label}"));
            }
            if let Some(tradeoff) = &option.tradeoff {
                out.push_str(&format!("  \n  {tradeoff}"));
            }
            if let Some(asks) = &option.asks {
                out.push_str(&format!("  \n  asks: {asks}"));
            }
            if let Some(command) = decision.command(option) {
                out.push_str(&format!("  \n  `{command}`"));
            }
            out
        })
        .collect::<Vec<_>>()
        .join("\n");
    match decision.otherwise.is_empty() {
        true => options,
        false => format!(
            "{options}\n\n{}\n\n{}",
            crate::surface::terminal::OR,
            steps(&decision.otherwise)
        ),
    }
}

fn checklist(list: &Checklist) -> String {
    let sep = format!(" {} ", GLYPHS.sep());
    let fold = list.fold().map(|fold| {
        format!(
            "- {} **{}** {}",
            GLYPHS.mark(crate::components::Found::Holds.mark()),
            fold.subject,
            fold.names.join(&sep)
        )
    });
    fold.into_iter()
        .chain(list.unfolded().map(check_item))
        .collect::<Vec<_>>()
        .join("\n")
}

/// One check as a list item: its mark, its subject and what it found,
/// with what it means, how to make it hold and what says more under it.
fn check_item(check: &Check) -> String {
    let mut row = format!(
        "- {} **{}** {}",
        GLYPHS.mark(check.found.mark()),
        check.subject,
        check.said
    );
    row.push_str(&hung(check.so.as_deref(), check.fix.as_ref()));
    for detail in check.details.iter().flat_map(|detail| detail.lines()) {
        row.push_str(&format!("\n  - {detail}"));
    }
    row
}

fn next(next: &Next) -> String {
    if next.steps.is_empty() {
        return String::new();
    }
    format!(
        "**{}**\n\n{}",
        crate::surface::terminal::NEXT,
        steps(&next.steps)
    )
}

/// Each command beside what it does, one to an item.
fn steps(steps: &[(String, &'static str)]) -> String {
    steps
        .iter()
        .map(|(command, gloss)| format!("- `{command}`: {gloss}"))
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
/// A caution as a list item, with what it means and what fixes it under
/// it.
fn concern(concern: &Concern) -> String {
    format!(
        "- {} {}{}",
        GLYPHS.mark(crate::glyphs::Mark::Caution),
        concern.fact,
        hung(Some(&concern.so), Some(&concern.fix))
    )
}

/// What a finding means and how to make it hold, as the items under its
/// own, each beside its label, the lines to paste fenced under the fix.
fn hung(so: Option<&str>, fix: Option<&crate::components::Fix>) -> String {
    use crate::surface::terminal::{FIX, SO};
    let mut out = String::new();
    if let Some(so) = so {
        out.push_str(&format!("\n  - {SO}: {so}"));
    }
    if let Some(fix) = fix {
        out.push_str(&format!("\n  - {FIX}: {}", fix.said));
        if !fix.paste.is_empty() {
            out.push_str(&format!(
                "\n\n    ```\n    {}\n    ```",
                fix.paste.join("\n    ")
            ));
        }
    }
    out
}

/// The problem's word in bold and what it says, then where it is and its
/// line with the part at fault marked under it, fenced as text; then what
/// helps fix it after an italic `help:`, the items a separator apart.
fn problem(problem: &Problem) -> String {
    let mut out = format!("**{}** {}", problem.severity.word(), problem.said);
    if let Some(place) = &problem.at {
        out.push_str(&format!("  \nat `{}`", place.named()));
        if let Some((source, before, len)) = place.fault() {
            let number = place.line;
            let pad = place.pad();
            let gutter = GLYPHS.gutter();
            let marks = GLYPHS.fault().to_string().repeat(len);
            let under = " ".repeat(crate::width::cell_width(&before));
            out.push_str(&format!(
                "\n\n```text\n{number} {gutter} {source}\n{pad} {gutter} {under}{marks}\n```"
            ));
        }
    }
    if !problem.help.is_empty() {
        let help = problem.help.join(&format!(" {} ", GLYPHS.sep()));
        out.push_str(&format!("\n\n_{}_ {help}", crate::surface::terminal::HELP));
    }
    out
}

/// What a command would not do, in bold, and where it looked under it.
fn refusal(refusal: &Refusal) -> String {
    let said = format!(
        "**{}** {}",
        crate::surface::terminal::REFUSED,
        refusal.said.replace('\n', "  \n")
    );
    match &refusal.detail {
        Some(detail) => format!("{said}  \n_{detail}_"),
        None => said,
    }
}

fn cell(text: &str) -> String {
    crate::text::one_line(text).replace('|', "\\|")
}
