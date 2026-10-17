//! Markdown an author wrote, in the parts each surface draws its own way:
//! the text, and each `mermaid` block as a diagram.

use super::Diagram;
use crate::doc::Block;

/// One part of authored Markdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Part {
    Text(String),
    Diagram(Diagram),
}

/// The parts of `markdown`, in the order it says them: each stretch of
/// text, and each `mermaid` block with its source as written. A block
/// left open runs to the end.
pub(crate) fn parts(markdown: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut text: Vec<&str> = Vec::new();
    let mut diagram: Option<(usize, Vec<&str>)> = None;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        match &mut diagram {
            Some((indent, source)) => {
                if trimmed.starts_with("```") {
                    parts.push(Part::Diagram(Diagram::of(&source.join("\n"))));
                    diagram = None;
                } else {
                    let unindented = line
                        .get(*indent..)
                        .filter(|_| line[..*indent].trim().is_empty());
                    source.push(unindented.unwrap_or(line));
                }
            }
            None if trimmed.starts_with("```mermaid") => {
                flush(&mut parts, &mut text);
                diagram = Some((line.len() - trimmed.len(), Vec::new()));
            }
            None => text.push(line),
        }
    }
    if let Some((_, source)) = diagram {
        parts.push(Part::Diagram(Diagram::of(&source.join("\n"))));
    }
    flush(&mut parts, &mut text);
    parts
}

/// The text gathered so far as a part, when it says anything.
fn flush(parts: &mut Vec<Part>, text: &mut Vec<&str>) {
    let said = text.join("\n");
    if !said.trim().is_empty() {
        parts.push(Part::Text(said.trim().to_string()));
    }
    text.clear();
}

/// What a review shows of `markdown`: its first paragraph, then each
/// diagram — a picture of the change says what a paragraph takes a page
/// to.
pub(crate) fn reviewed(markdown: &str) -> Vec<Block<'static>> {
    let mut blocks = Vec::new();
    let mut said = false;
    for part in parts(markdown) {
        match part {
            Part::Text(text) if !said => {
                said = true;
                let first = text.split("\n\n").next().unwrap_or_default();
                blocks.push(Block::Markdown(first.to_string()));
            }
            Part::Text(_) => {}
            Part::Diagram(diagram) => {
                if !blocks.is_empty() {
                    blocks.push(Block::Lines(vec![crate::ink::Line::new()]));
                }
                blocks.push(Block::Diagram(diagram));
            }
        }
    }
    blocks
}
