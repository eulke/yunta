//! What a page's blocks draw, one to a line of the body, and the list of what
//! it holds, gathered as it draws them.

use super::text::{class_of, command, esc, line, mark, steps, title_words, words};
use super::{code, decision, diagram, markdown, notices, screen, tables, GLYPHS};
use crate::components::{Fields, Headline, Lead, Next, PathNote, Paths, Reads, Section, Title};
use crate::doc::Block;
use crate::glyphs::Mark;
use crate::line::{Line, Tone};
use crate::surface::terminal::NEXT;
use crate::words::Standing;

/// The level of the first section under a document's headline: the headline
/// is the first level, so a section a level under it is the second.
const SECTION_LEVEL: usize = 2;

/// The deepest heading level HTML has: a section nested deeper than this is
/// drawn at the sixth level, since no heading level goes past it.
const DEEPEST_LEVEL: usize = 6;

/// A place the page's list can link to, its title as the page draws it.
pub(super) struct Entry {
    pub(super) depth: usize,
    pub(super) id: String,
    pub(super) title: String,
}

/// What a page has drawn so far: what its list will hold, how many places it
/// has named, and whether its first headline has been drawn.
pub(super) struct Reader {
    pub(super) toc: Vec<Entry>,
    named: usize,
    headlined: bool,
}

impl Reader {
    /// A reader of a page that has drawn nothing yet.
    pub(super) fn new() -> Self {
        Reader {
            toc: Vec::new(),
            named: 0,
            headlined: false,
        }
    }

    /// A reader of a part of a page whose title stands elsewhere: a screen's,
    /// whose place says where the person is, so its first headline is a
    /// heading of the second level.
    pub(super) fn headed() -> Self {
        Reader {
            headlined: true,
            ..Reader::new()
        }
    }

    /// One that goes on naming places after `reader`, for a part of the page
    /// the list does not hold: its ids go on from where `reader` left off, and
    /// its first headline is not the page's title again.
    pub(super) fn after(reader: &Reader) -> Self {
        Reader {
            toc: Vec::new(),
            named: reader.named,
            headlined: reader.headlined,
        }
    }

    /// Gives the next place its id, and lists it at `depth` under `title`.
    fn place(&mut self, title: String, depth: usize) -> String {
        self.named += 1;
        let id = format!("s-{}", self.named);
        self.toc.push(Entry {
            depth,
            id: id.clone(),
            title,
        });
        id
    }

    /// Takes back the place last given, for a place that nothing drawn under
    /// it reached the page: the list and the ids then hold only what the page
    /// shows.
    fn withdraw(&mut self) {
        self.toc.pop();
        self.named -= 1;
    }
}

/// One block, `depth` sections deep, drawn as HTML: what it holds, through
/// the writers each kind of block has.
pub(super) fn block(reader: &mut Reader, block: &Block, depth: usize) -> String {
    match block {
        Block::Headline(head) => headline(reader, head),
        Block::Furrow(stands) => furrow(&stands.0),
        Block::Ribbon(stretches) => screen::ribbon(stretches),
        Block::Gist(gist) => paragraph("gist", &gist.0),
        Block::Fields(rows) => fields(rows),
        Block::Table(table) => tables::table(table),
        Block::Nodes(nodes) => tables::table(&nodes.table(GLYPHS)),
        Block::Runs(runs) => tables::runs(runs),
        Block::Evidence(evidence) => tables::evidence(evidence),
        Block::Said(said) => said_lines(&said.0),
        Block::Paths(listed) => paths(listed),
        Block::Diffstat(stat) => tables::diffstat(stat),
        Block::Decision(asked) => decision::decision(asked),
        Block::Checklist(list) => tables::checklist(list),
        Block::Problem(problem) => notices::problem(problem),
        Block::Refusal(refusal) => notices::refusal(refusal),
        Block::Next(next) => next_steps(next, depth),
        Block::Legend(legend) => screen::legend(legend),
        Block::Attention(attention) => screen::attention(attention),
        Block::Title(doc_title) => title(reader, doc_title, depth),
        Block::Heading(text) => heading(reader, text, depth),
        Block::Section(part) => section(reader, part, depth),
        Block::Fold(part) => fold(reader, &part.0, depth),
        Block::Prose(prose) => paragraph("prose", &prose.0),
        Block::Note(note) => paragraph("note", &note.0),
        Block::Markdown(text) => markdown::markdown(text),
        Block::Marked(marked) => notices::marked(marked),
        Block::Concern(concern) => notices::concern(concern),
        Block::Diagram(drawn) => diagram::diagram(drawn),
        Block::Code(shown) => code::code(shown),
        Block::Lines(rows) => lines(rows),
    }
}

/// The page's headline: its first is the title, drawn as `h1`, and each later
/// one a heading of the second level under it.
fn headline(reader: &mut Reader, head: &Headline) -> String {
    let tag = match reader.headlined {
        true => "h2",
        false => "h1",
    };
    reader.headlined = true;
    let facts: String = head
        .facts
        .iter()
        .map(|fact| format!(r#" <span class="fact">{}</span>"#, words(fact)))
        .collect();
    let edge = match &head.edge {
        Some(edge) => format!(r#" <span class="id">{}</span>"#, esc(edge)),
        None => String::new(),
    };
    format!(
        r#"<header class="head">{}<{tag}>{}</{tag}><span class="{}">{}</span>{facts}{edge}</header>"#,
        mark(head.mark),
        words(&head.subject),
        pill(Tone::of(head.mark)),
        words(&head.said),
    )
}

/// The class of a headline's pill: the class of its mark's tone, or the pill
/// alone when that tone has no class of its own.
fn pill(tone: Tone) -> String {
    match class_of(tone) {
        Some(class) => format!("pill {class}"),
        None => "pill".to_string(),
    }
}

/// Where each node of a run stands, one stroke to a node, in the class of its
/// word; the words the strokes stand for are said in the element's label, for a
/// reader who cannot see them. Nothing is drawn for no nodes.
pub(super) fn furrow(standings: &[Standing]) -> String {
    if standings.is_empty() {
        return String::new();
    }
    let strokes: String = standings.iter().map(|stand| stroke(stand.mark())).collect();
    let said: Vec<&str> = standings.iter().map(|stand| stand.word()).collect();
    format!(
        r#"<div class="furrow" role="img" aria-label="{}">{strokes}</div>"#,
        esc(&said.join(", "))
    )
}

/// One stroke of a furrow: a node's mark in its tone's class. A stroke of a
/// muted node is bare, since its class would paint it like what a reader skims.
pub(super) fn stroke(sign: Mark) -> String {
    let class = match Tone::of(sign) {
        Tone::Muted => None,
        tone => class_of(tone),
    };
    match class {
        Some(class) => format!(r#"<i class="{class}"></i>"#),
        None => "<i></i>".to_string(),
    }
}

/// A paragraph of words in the class that says what kind it is; nothing for
/// a paragraph with no words in it.
fn paragraph(class: &str, text: &str) -> String {
    match text.trim().is_empty() {
        true => String::new(),
        false => format!(r#"<p class="{class}">{}</p>"#, words(text)),
    }
}

/// Lines a message says whole, each kept on its own line by a line break.
fn said_lines(lines: &[String]) -> String {
    if lines.iter().all(|line| line.trim().is_empty()) {
        return String::new();
    }
    let said: Vec<String> = lines.iter().map(|line| words(line)).collect();
    format!(r#"<p class="said">{}</p>"#, said.join("<br>"))
}

/// Labelled facts as a list of terms and their descriptions. A row whose label
/// is empty, or the label of the row above it, goes on under that row, so its
/// term is left empty.
fn fields(fields: &Fields) -> String {
    let mut above: Option<&str> = None;
    let mut rows = String::new();
    for field in fields.rows() {
        let goes_on = field.label.is_empty() || above == Some(field.label);
        above = Some(field.label);
        let label = match goes_on {
            true => String::new(),
            false => esc(field.label),
        };
        let value = value(field.reads, field.mark, &field.value);
        rows.push_str(&format!("<dt>{label}</dt><dd>{value}</dd>"));
    }
    match rows.is_empty() {
        true => String::new(),
        false => format!(r#"<dl class="fields">{rows}</dl>"#),
    }
}

/// A field's value as its reader reads it: words, text as written, or a
/// command a reader copies; a value that opens with a mark has it before it.
fn value(reads: Reads, sign: Option<Mark>, text: &str) -> String {
    let said = match reads {
        Reads::Words => words(text),
        Reads::Literal => format!("<code>{}</code>", esc(text)),
        Reads::Command => command(text),
    };
    match sign {
        Some(sign) => format!("{} {said}", mark(sign)),
        None => said,
    }
}

/// A heading and each path it names, each path with its reason beside it
/// when it has one.
fn paths(listed: &Paths) -> String {
    let items: String = listed.paths.iter().map(path_item).collect();
    format!(
        r#"<section class="paths-of"><p class="said">{}</p><ul class="paths">{items}</ul></section>"#,
        words(&listed.heading)
    )
}

/// One path a message names, as a list item: the path as code, then its reason
/// on the line after it, in a span of its own, when it has one.
fn path_item(note: &PathNote) -> String {
    let path = format!("<code>{}</code>", esc(&note.path.display().to_string()));
    match &note.reason {
        Some(reason) => format!(
            "<li>{path} <span>({})</span></li>",
            words(&crate::text::one_line(reason))
        ),
        None => format!("<li>{path}</li>"),
    }
}

/// The commands that move on from what the page showed, under the title of
/// next steps. Nothing when there are no steps. It takes no place in the list:
/// it stands last, where a reader already is.
fn next_steps(next: &Next, depth: usize) -> String {
    if next.steps.is_empty() {
        return String::new();
    }
    let level = level(depth);
    format!(
        r#"<section class="next"><h{level} class="sec">{}</h{level}>{}</section>"#,
        esc(NEXT),
        steps(&next.steps)
    )
}

/// A document's title: its name, and the facts beside it. The first thing a page
/// says is the page's title; a document it holds after that is a heading under it,
/// with a place in the list.
fn title(reader: &mut Reader, doc_title: &Title, depth: usize) -> String {
    let facts: String = doc_title
        .facts
        .iter()
        .map(|fact| format!(r#" <span class="fact">{}</span>"#, words(fact)))
        .collect();
    let name = title_words(&doc_title.name);
    if reader.headlined {
        let id = reader.place(words(doc_title.name.text().trim()), depth);
        return format!(r#"<h2 class="title" id="{id}">{name}{facts}</h2>"#);
    }
    reader.headlined = true;
    format!(r#"<h1 class="title">{name}{facts}</h1>"#)
}

/// The title of what follows it, inside a document, as a heading of its level
/// under the page's, with a place in the list. Nothing for a blank title.
fn heading(reader: &mut Reader, text: &str, depth: usize) -> String {
    let title = text.trim();
    if title.is_empty() {
        return String::new();
    }
    let id = reader.place(words(title), depth);
    let level = level(depth);
    format!(
        r#"<h{level} class="sec" id="{id}">{}</h{level}>"#,
        words(title)
    )
}

/// The heading level of a section `depth` sections deep: one level under the
/// section that holds it, and no deeper than the deepest level HTML has.
fn level(depth: usize) -> usize {
    (SECTION_LEVEL + depth).min(DEEPEST_LEVEL)
}

/// What leads a section's title, with the space that sets it apart: its mark,
/// or its place in a numbered list.
fn lead_text(lead: Option<Lead>) -> String {
    match lead {
        Some(Lead::Mark(sign)) => format!("{} ", mark(sign)),
        Some(Lead::Ordinal(at)) => format!("{at}  "),
        None => String::new(),
    }
}

/// A section's title as the words of its heading: its lead, then its title's line.
fn heading_line(section: &Section) -> String {
    format!(
        "{}{}",
        lead_text(section.lead),
        title_words(&section.title).trim()
    )
}

/// A section: a titled one takes a place in the list and a heading a level
/// under the one that holds it, and its blocks a level deeper. A section with a
/// blank title has neither, and its blocks stay at its own depth.
fn section(reader: &mut Reader, part: &Section, depth: usize) -> String {
    let title = part.title.text();
    if title.trim().is_empty() {
        let inner = draw_all(reader, &part.blocks, depth);
        return match inner.is_empty() {
            true => String::new(),
            false => format!("<section>{inner}</section>"),
        };
    }
    let id = reader.place(words(title.trim()), depth);
    let level = level(depth);
    let inner = draw_all(reader, &part.blocks, depth + 1);
    format!(
        r#"<section id="{id}"><h{level} class="sec">{}</h{level}>{inner}</section>"#,
        heading_line(part)
    )
}

/// A fold: a section a reader opens, closed under its title, with its blocks
/// inside it. A fold whose blocks draw nothing is its title alone, and a fold
/// with a blank title reads as a section with a blank title.
fn fold(reader: &mut Reader, part: &Section, depth: usize) -> String {
    if part.title.text().trim().is_empty() {
        return section(reader, part, depth);
    }
    let id = reader.place(words(part.title.text().trim()), depth);
    let inner = draw_all(reader, &part.blocks, depth + 1);
    let summary = heading_line(part);
    if inner.is_empty() {
        reader.withdraw();
        return format!(r#"<p class="said">{summary}</p>"#);
    }
    format!(r#"<details id="{id}"><summary>{summary}</summary>{inner}</details>"#)
}

/// Lines a surface composed, each drawn in the tones of its spans, one to a
/// line of a preformatted block. Nothing for lines that say nothing: a blank line
/// is how a terminal sets two blocks apart, and a page has its own way.
fn lines(rows: &[Line]) -> String {
    if rows.iter().all(|row| row.text().trim().is_empty()) {
        return String::new();
    }
    let drawn: Vec<String> = rows.iter().map(line).collect();
    format!(r#"<pre class="lines">{}</pre>"#, drawn.join("\n"))
}

/// Each of `blocks`, `depth` deep, on a line of its own; a block that draws
/// nothing leaves no line.
pub(super) fn draw_all(reader: &mut Reader, blocks: &[Block], depth: usize) -> String {
    blocks
        .iter()
        .map(|inner| block(reader, inner, depth))
        .filter(|drawn| !drawn.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
