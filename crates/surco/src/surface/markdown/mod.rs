//! A document as Markdown: what a file the engine writes, or a pull
//! request, says. Nothing is laid out to a width — the viewer wraps — and
//! a mark is its Unicode glyph beside its word, since a viewer draws any
//! character and the word is what carries the meaning.

use crate::components::{
    Cell, Check, Checklist, Concern, Edge, Evidence, Fields, Holds, Lead, Marked, Next, Paths,
    Reads, Runs, Section, Table, Whole,
};
use crate::doc::{Block, Doc};
use crate::glyphs::{Glyphs, Mark};
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::motion::Motion;
use crate::paint::Paint;
use crate::surface::Surface;
use text::{cell, cell_code_span, code_span, literal, plain_text};

mod asked;
mod card;
mod code;
mod decision;
mod description;
mod desk;
mod diffstat;
mod fence;
pub(in crate::surface) mod fold;
mod headline;
mod problem;
mod screen;
mod text;

pub use description::{Description, Leaves};

/// How one page is written: as a file or as a forge's description, what it
/// leaves out, and whether the block at hand stands inside a fold.
#[derive(Debug, Clone, Copy)]
pub(super) struct Page {
    /// A forge's description, under the title the forge draws: its first
    /// headline is a line, and a document's title a heading of the second
    /// level.
    description: bool,
    /// What the page leaves out of its blocks.
    leaves: Leaves,
    /// Whether the block stands inside a fold, where an alert cannot.
    folded: bool,
}

/// A Markdown file or a forge's page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Markdown;

/// The set a Markdown reader is drawn with: any viewer draws it.
const GLYPHS: Glyphs = Glyphs::Unicode;

impl Markdown {
    /// The look a document is built with for a Markdown reader: its glyphs,
    /// no paint, no width, since a viewer wraps a file's words, and nothing
    /// that turns, since a file is read rather than watched.
    pub fn look(&self) -> Look {
        Look {
            glyphs: GLYPHS,
            paint: Paint::Plain,
            width: Width::of(None, None),
            motion: Motion::Still,
        }
    }
}

impl Surface for Markdown {
    /// The document, its blocks separated by a blank line.
    type Output = String;

    fn draw(&self, doc: &Doc) -> String {
        drawn(
            doc,
            Page {
                description: false,
                leaves: Leaves::Nothing,
                folded: false,
            },
        )
    }
}

/// `doc` written as `page` says, its blocks separated by a blank line.
fn drawn(doc: &Doc, page: Page) -> String {
    let blocks: Vec<String> = doc
        .blocks()
        .iter()
        .enumerate()
        .map(|(at, drawn)| match (at, drawn) {
            // A document's first headline is its title.
            (0, Block::Headline(headline)) => headline::first(headline, page),
            // The furrow is a picture of what the facts below say in words.
            (_, Block::Furrow(_)) => String::new(),
            _ => block(drawn, 0, page),
        })
        .filter(|drawn| !drawn.trim().is_empty())
        .collect();
    match blocks.is_empty() {
        true => String::new(),
        false => format!("{}\n", blocks.join("\n\n")),
    }
}

/// One block, `depth` sections deep, written as `page` says.
fn block(block: &Block, depth: usize, page: Page) -> String {
    match block {
        Block::Headline(headline) => headline::headline(headline),
        Block::Furrow(_) => String::new(),
        Block::Ribbon(ribbon) => screen::ribbon(ribbon),
        Block::Gist(gist) => plain_text(&gist.0),
        Block::Fields(fields) => self::fields(fields),
        Block::Table(table) => self::table(table),
        Block::Nodes(nodes) => self::table(&nodes.table(GLYPHS)),
        Block::Runs(runs) => self::runs(runs),
        Block::Evidence(evidence) => self::evidence(evidence),
        Block::Said(said) => said
            .0
            .iter()
            .map(|line| plain_text(line))
            .collect::<Vec<_>>()
            .join("  \n"),
        Block::Paths(paths) => self::paths(paths),
        Block::Diffstat(stat) => diffstat::diffstat(stat),
        Block::Decision(decision) => decision::decision(decision),
        Block::Card(drawn) => card::card(drawn),
        Block::Desk(drawn) => desk::desk(drawn),
        Block::TypedAnswer(drawn) => asked::typed_answer(drawn),
        Block::ChoiceList(drawn) => asked::choice_list(drawn),
        Block::Checklist(list) => self::checklist(list),
        Block::Problem(said) => problem::problem(said),
        Block::Refusal(refusal) => problem::refusal(refusal),
        Block::Next(next) => self::next(next),
        Block::Legend(legend) => screen::legend(legend),
        Block::Attention(attention) => screen::attention(attention),
        Block::Heading(title) => format!("{} {}", "#".repeat(2 + depth), plain_text(title)),
        Block::Title(title) => headline::title(title, page),
        Block::Section(section) => self::section(section, depth, page),
        Block::Fold(fold) => fold::fold(&fold.0, depth, page),
        Block::Prose(prose) => plain_text(&prose.0),
        Block::Note(note) => format!("_{}_", plain_text(&note.0)),
        Block::Markdown(text) => fence::closed(text.trim()),
        Block::Marked(marked) => self::marked(marked, page),
        Block::Concern(concern) => self::concern(concern),
        Block::Diagram(diagram) => {
            let fence = code::fence(&diagram.source);
            format!("{fence}mermaid\n{}\n{fence}", diagram.source)
        }
        Block::Code(shown) => code::drawn(shown, page),
        Block::Lines(lines) | Block::Rows(lines) => {
            lines.iter().map(line).collect::<Vec<_>>().join("  \n")
        }
    }
}

fn fields(fields: &Fields) -> String {
    let mut items: Vec<String> = Vec::new();
    let mut above = None;
    for field in fields.rows() {
        let value = match field.reads {
            Reads::Words => plain_text(&field.value),
            Reads::Literal => literal(&field.value),
            Reads::Command => code_span(&field.value),
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
            (true, Some(item)) => {
                item.push_str("  \n");
                item.push_str(&text::item("  ", "  ", &value));
            }
            _ => items.push(text::item(
                &format!("- {}: ", plain_text(field.label)),
                "  ",
                &value,
            )),
        }
    }
    items.join("\n")
}

/// What leads a section's title, with the space that sets it apart: its
/// mark, its place in a numbered list, or the pointer where it stands on it.
fn lead(section: &Section) -> String {
    match section.lead {
        Some(Lead::Mark(mark)) => format!("{} ", GLYPHS.mark(mark)),
        Some(Lead::Ordinal(at)) => format!("{at}  "),
        Some(Lead::Pointer { on: true }) => format!("{} ", GLYPHS.pointer()),
        Some(Lead::Pointer { on: false }) | None => String::new(),
    }
}

/// A section: its title as a heading a level under the document's, and
/// each of its blocks a level deeper.
fn section(section: &Section, depth: usize, page: Page) -> String {
    let mut parts = vec![format!(
        "{} {}{}",
        "#".repeat(2 + depth),
        lead(section),
        plain_text(section.title.text().trim())
    )];
    parts.extend(
        section
            .blocks
            .iter()
            .map(|inner| block(inner, depth + 1, page))
            .filter(|drawn| !drawn.trim().is_empty()),
    );
    parts.join("\n\n")
}

/// A list of items that each carry the same mark. A caution outside a fold
/// is an alert; anywhere else, each item is a list item with its mark beside
/// it, and each further line of an item kept under the item's first line.
fn marked(marked: &Marked, page: Page) -> String {
    match (marked.mark, page.folded) {
        (Mark::Caution, false) if !marked.items.is_empty() => alert(marked),
        _ => {
            let first = format!("- {} ", GLYPHS.mark(marked.mark));
            marked
                .items
                .iter()
                .map(|item| text::item(&first, "  ", &plain_text(item)))
                .collect::<Vec<_>>()
                .join("\n")
        }
    }
}

/// A caution as an alert: its tag on a line of its own, then each item as a
/// list item within the quote, each further line of it kept inside the quote
fn alert(marked: &Marked) -> String {
    std::iter::once("> [!CAUTION]".to_string())
        .chain(
            marked
                .items
                .iter()
                .map(|item| text::item("> - ", ">   ", &plain_text(item))),
        )
        .collect::<Vec<_>>()
        .join("\n")
}

/// A table with its header: an id as code, a row's mark in front of its
/// first cell.
fn table(table: &Table) -> String {
    let mut out = String::from("|");
    for column in &table.columns {
        match column.title {
            "" => out.push_str(" |"),
            title => out.push_str(&format!(" {} |", cell(title))),
        }
    }
    out.push_str(&format!("\n|{}", "---|".repeat(table.columns.len())));
    for row in &table.rows {
        out.push_str("\n|");
        for (at, (said, column)) in row.cells.iter().zip(&table.columns).enumerate() {
            let mut said = match column.holds {
                Holds::Id => id_cell(said),
                Holds::State | Holds::Words | Holds::Figure | Holds::Rest | Holds::Prose => {
                    cell(&said.text)
                }
            };
            if let (0, Some(mark)) = (at, row.mark) {
                said = format!("{} {said}", GLYPHS.mark(mark));
            }
            out.push_str(&format!(" {said} |"));
        }
    }
    out
}

/// A cell of an id column: its id as code, and whatever the cell says around it as words.
fn id_cell(said: &Cell) -> String {
    let Some(id) = &said.id else {
        return cell_code_span(&said.text);
    };
    [
        cell(&said.text[..id.start]),
        cell_code_span(&said.text[id.clone()]),
        cell(&said.text[id.end..]),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" ")
}

/// The pointer before the row a key acts on, as a file keeps it; nothing before any
/// other.
fn held(row: &crate::components::RunLine) -> String {
    match row.pointed {
        true => format!("{} ", GLYPHS.pointer()),
        false => String::new(),
    }
}

/// Each run as a table row: where it stands, and whether it is pointed at or picked;
/// its handle, what it runs, what holds it, and its edge.
fn runs(runs: &Runs) -> String {
    let mut out = String::from("| | run | workflow | where it stands | |\n|---|---|---|---|---|");
    for row in &runs.rows {
        let edge = match &row.edge {
            Edge::Age(age) => cell(age),
            Edge::Command(command) => cell_code_span(command),
        };
        out.push_str(&format!(
            "\n| {}{} {}{} | {} | {} ({}) | {} | {edge} |",
            held(row),
            GLYPHS.mark(row.standing.mark()),
            row.standing.word(),
            match row.marked {
                true => ", picked",
                false => "",
            },
            cell_code_span(&row.handle),
            cell(&row.workflow),
            cell(&row.mode),
            cell(&row.said),
        ));
    }
    out
}

fn evidence(evidence: &Evidence) -> String {
    let quoted = evidence.quoted().join("\n");
    let fence = code::fence(&quoted);
    let mut out = format!("{fence}\n{quoted}\n{fence}");
    let mut rest = Vec::new();
    let above = evidence.above();
    if above > 0 {
        rest.push(format!("{} above", crate::text::counted(above, "line")));
    }
    if let Some(whole) = &evidence.whole {
        rest.push(match whole {
            Whole::File { shown, .. } => format!("whole output: {}", code_span(shown)),
            Whole::Command(command) => format!("whole output: {}", code_span(command)),
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
    std::iter::once(plain_text(&paths.heading))
        .chain(paths.paths.iter().map(|note| {
            let path = code_span(&note.path.display().to_string());
            match &note.reason {
                Some(reason) => {
                    format!("- {path} ({})", plain_text(&crate::text::one_line(reason)))
                }
                None => format!("- {path}"),
            }
        }))
        .collect::<Vec<_>>()
        .join("\n")
}

fn checklist(list: &Checklist) -> String {
    let sep = format!(" {} ", GLYPHS.sep());
    let fold = list.fold().map(|fold| {
        let names: Vec<String> = fold.names.iter().map(|name| plain_text(name)).collect();
        let head = format!(
            "- {} **{}** ",
            GLYPHS.mark(crate::components::Found::Holds.mark()),
            plain_text(&crate::text::one_line(&fold.subject))
        );
        text::item(&head, "  ", &names.join(&sep))
    });
    fold.into_iter()
        .chain(
            list.unfolded()
                .map(|check| check_item(check, list.points_at(check))),
        )
        .collect::<Vec<_>>()
        .join("\n")
}

/// One check as a list item: the pointer where it is `pointed` at, its mark,
/// its subject and what it found, with what it means, how to make it hold and
/// what says more under it.
fn check_item(check: &Check, pointed: bool) -> String {
    let subject = plain_text(&crate::text::one_line(&check.subject));
    let pointer = match pointed {
        true => format!("{} ", GLYPHS.pointer()),
        false => String::new(),
    };
    let head = format!(
        "- {pointer}{} **{subject}** ",
        GLYPHS.mark(check.found.mark())
    );
    let mut row = text::item(&head, "  ", &plain_text(&check.said));
    row.push_str(&hung(check.so.as_deref(), check.fix.as_ref()));
    for detail in check.details.iter().flat_map(|detail| detail.lines()) {
        if !detail.trim().is_empty() {
            row.push_str(&format!("\n  - {}", plain_text(detail)));
        }
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

/// Each command beside what it does, one to an item: the command on one line,
/// and each further line of what it does kept under the item's first.
fn steps(steps: &[(String, &'static str)]) -> String {
    steps
        .iter()
        .map(|(command, gloss)| {
            let command = crate::text::one_line(command);
            text::item(
                &format!("- {}: ", code_span(&command)),
                "  ",
                &plain_text(gloss),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// One line of spans, its strong parts in bold and its command parts in code,
/// as written; every other part is words, written as `plain_text` writes them.
fn line(line: &Line) -> String {
    line.spans()
        .iter()
        .map(|span| match span.tone {
            Tone::Strong if !span.text.trim().is_empty() => {
                format!("**{}**", plain_text(span.text.trim()))
            }
            Tone::Command if !span.text.trim().is_empty() => code_span(span.text.trim()),
            _ => plain_text(&span.text),
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// A caution as a list item, with what it means and what fixes it under
/// it.
fn concern(concern: &Concern) -> String {
    let fact = text::item(
        &format!("- {} ", GLYPHS.mark(crate::glyphs::Mark::Caution)),
        "  ",
        &plain_text(&concern.fact),
    );
    format!("{fact}{}", hung(Some(&concern.so), Some(&concern.fix)))
}

/// What a finding means and how to make it hold, as the items under its
/// own, each beside its label, the lines to paste fenced under the fix.
fn hung(so: Option<&str>, fix: Option<&crate::components::Fix>) -> String {
    use crate::surface::terminal::{FIX, SO};
    let mut out = String::new();
    if let Some(so) = so {
        out.push('\n');
        out.push_str(&text::item(&format!("  - {SO}: "), "    ", &plain_text(so)));
    }
    if let Some(fix) = fix {
        out.push('\n');
        out.push_str(&text::item(
            &format!("  - {FIX}: "),
            "    ",
            &plain_text(&fix.said),
        ));
        if !fix.paste.is_empty() {
            let paste = fix.paste.join("\n");
            let fence = code::fence(&paste);
            out.push_str(&format!(
                "\n\n    {fence}\n    {}\n    {fence}",
                fix.paste.join("\n    ")
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests;
