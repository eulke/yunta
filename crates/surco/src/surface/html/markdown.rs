//! Markdown a document holds, as a page reads it: its paragraphs, headings,
//! lists, quotes, runs of lines that start with `|` and fenced code, each in
//! the block it is, with every diagram it holds drawn as one. Nothing is laid
//! out to a width: the browser wraps.
//!
//! How each line reads, its fences, its list items and its headings, is the
//! terminal's own reading of Markdown, so a page and a terminal agree on
//! where a fence starts and ends.

use super::text;
use crate::code::{heading_level, highlighted, painted, Fences, Language};
use crate::diagram::split::{parts, Part};
use crate::surface::terminal::markdown::prose::{list_item, marked_spans, spans};

/// Markdown as a page draws it: its parts in the order it says them, each
/// text read line by line and each diagram drawn. Nothing for what says nothing.
pub(super) fn markdown(text: &str) -> String {
    let blocks: Vec<String> = parts(text)
        .iter()
        .map(|part| match part {
            Part::Text(said) => blocks_of(said),
            Part::Diagram(diagram) => super::diagram::diagram(diagram),
        })
        .collect();
    let said = blocks.join("\n");
    match said.trim().is_empty() {
        true => String::new(),
        false => format!("<div class=\"written\">{said}</div>"),
    }
}

/// The blocks `source` reads as, one line at a time, in the order it says them.
fn blocks_of(source: &str) -> String {
    let mut page = Page {
        fences: Fences::of(source),
        ..Page::default()
    };
    let mut at = 0;
    for line in source.split_inclusive('\n') {
        page.read(source, at, line.trim_end_matches(['\n', '\r']));
        at += line.len();
    }
    page.done()
}

/// The blocks drawn so far, and the one block the next line may continue.
#[derive(Default)]
struct Page {
    blocks: Vec<String>,
    open: Open,
    /// The fences of the text, read once for the whole of it.
    fences: Fences,
}

/// The block that a line of Markdown may continue, until a line that is not
/// part of it closes it.
#[derive(Default)]
enum Open {
    #[default]
    Nothing,
    /// A paragraph's lines so far, each followed by a space.
    Paragraph(String),
    /// A run of items, all of one kind: ordered or not.
    List { ordered: bool, items: Vec<String> },
    /// A run of lines that start with `|`, each escaped as written.
    Lines(Vec<String>),
    /// A fenced block of code, drawn once the fence closes.
    Fence(Fenced),
}

/// A fenced block of code being read: the language its fence names, where
/// the marks of the line that closes it stand, where what follows it starts, how many
/// spaces each of its lines gives up, and its lines so far.
struct Fenced {
    language: Option<Language>,
    closing: Option<usize>,
    next: usize,
    indent: usize,
    lines: Vec<String>,
}

impl Page {
    /// Reads `line`, which starts at offset `at` of `source`: a line of the
    /// fenced block being read, the opening line of a fence, a blank line, or
    /// a line that is a heading, a list item, a quote, a run line or text.
    fn read(&mut self, source: &str, at: usize, line: &str) {
        if matches!(&self.open, Open::Fence(fenced) if at >= fenced.next) {
            self.close();
        }
        if matches!(self.open, Open::Fence(_)) {
            return self.code(at, line);
        }
        let trimmed = line.trim();
        let marks = at + line.len() - line.trim_start().len();
        let opened = self.fences.at(marks).map(|fence| Fenced {
            language: Language::named(&source[fence.info.clone()]),
            closing: fence.closing.as_ref().map(|closing| closing.start),
            next: fence.next,
            indent: fence.indent,
            lines: Vec::new(),
        });
        if let Some(fenced) = opened {
            self.close();
            self.open = Open::Fence(fenced);
            return;
        }
        if trimmed.is_empty() {
            self.close();
            return;
        }
        match list_item(trimmed) {
            Some(("", heading)) => self.heading(trimmed, heading),
            Some((mark, _)) => self.item(trimmed, mark),
            None => self.plain(line, trimmed),
        }
    }

    /// A line inside a fenced block, which starts at offset `at`: the line
    /// that holds the fence's closing marks closes the block, and any other
    /// line is kept as code.
    fn code(&mut self, at: usize, line: &str) {
        let Open::Fence(fenced) = &mut self.open else {
            return;
        };
        let closes = fenced
            .closing
            .is_some_and(|marks| (at..=at + line.len()).contains(&marks));
        if closes {
            self.close();
        } else {
            let lead = line.len() - line.trim_start_matches(' ').len();
            fenced
                .lines
                .push(line[lead.min(fenced.indent)..].to_string());
        }
    }

    /// A heading, from its marks: its words in the page's heading style, under
    /// the level its marks say.
    fn heading(&mut self, trimmed: &str, heading: &str) {
        self.close();
        // list_item reads a heading only where heading_level reads its level.
        let level = heading_level(trimmed).unwrap_or(1);
        let words = marked_spans(trimmed, trimmed.len() - heading.len());
        self.blocks
            .push(format!("<h{level}>{}</h{level}>", text::line(&words)));
    }

    /// A list item with its mark: it joins the open list of its kind, or
    /// closes whatever is open and starts a list of its own.
    fn item(&mut self, trimmed: &str, mark: &str) {
        let ordered = mark.starts_with(|ch: char| ch.is_ascii_digit());
        let item = format!(
            "<li>{}</li>",
            text::line(&marked_spans(trimmed, mark.len()))
        );
        if let Open::List {
            ordered: open,
            items,
        } = &mut self.open
        {
            if *open == ordered {
                items.push(item);
                return;
            }
        }
        self.close();
        self.open = Open::List {
            ordered,
            items: vec![item],
        };
    }

    /// A line that is none of the others: a quote when it starts with `> `, a
    /// run line when it starts with `|`, and otherwise a line of a paragraph.
    fn plain(&mut self, line: &str, trimmed: &str) {
        if let Some(quoted) = trimmed.strip_prefix("> ") {
            self.quote(quoted);
        } else if trimmed.starts_with('|') {
            self.run_line(line);
        } else {
            self.paragraph(trimmed);
        }
    }

    /// A quote's words, in a blockquote of their own.
    fn quote(&mut self, quoted: &str) {
        self.close();
        self.blocks.push(format!(
            "<blockquote>{}</blockquote>",
            text::line(&spans(quoted))
        ));
    }

    /// A line that starts with `|`, joining the run of such lines open above
    /// it, or starting one.
    fn run_line(&mut self, line: &str) {
        if let Open::Lines(lines) = &mut self.open {
            lines.push(text::esc(line));
            return;
        }
        self.close();
        self.open = Open::Lines(vec![text::esc(line)]);
    }

    /// A line of a paragraph, joining the paragraph open above it, or starting
    /// one.
    fn paragraph(&mut self, trimmed: &str) {
        if let Open::Paragraph(said) = &mut self.open {
            said.push_str(trimmed);
            said.push(' ');
            return;
        }
        self.close();
        self.open = Open::Paragraph(format!("{trimmed} "));
    }

    /// Draws the open block, if it holds anything, and leaves nothing open.
    fn close(&mut self) {
        let drawn = match std::mem::take(&mut self.open) {
            Open::Nothing => None,
            Open::Paragraph(said) => Some(format!("<p>{}</p>", text::line(&spans(said.trim())))),
            Open::List { ordered, items } => {
                let tag = if ordered { "ol" } else { "ul" };
                Some(format!("<{tag}>{}</{tag}>", items.concat()))
            }
            Open::Lines(lines) => Some(format!("<pre class=\"lines\">{}</pre>", lines.join("\n"))),
            Open::Fence(fenced) => code_block(fenced),
        };
        self.blocks.extend(drawn);
    }

    /// The blocks read so far, each one after the last, in the order read.
    fn done(mut self) -> String {
        self.close();
        self.blocks.join("\n")
    }
}

/// A fence's code, each line in the tones of what its words are in the
/// fence's language, its blank lines kept. None when the fence holds no line.
fn code_block(fenced: Fenced) -> Option<String> {
    if fenced.lines.is_empty() {
        return None;
    }
    let source = fenced.lines.join("\n");
    let colored = highlighted(&source, fenced.language);
    let mut start = 0;
    let lines: Vec<String> = fenced
        .lines
        .iter()
        .map(|line| {
            let range = start..start + line.len();
            start = range.end + 1;
            text::line(&painted(&source, range, &colored))
        })
        .collect();
    Some(format!("<pre class=\"code\">{}</pre>", lines.join("\n")))
}
