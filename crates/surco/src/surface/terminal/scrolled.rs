//! A document in fewer rows than it takes: its lines from the one a person asked to see,
//! held within its last lines and moved so that the line a pointer stands on is in sight,
//! with a row that counts the lines below when they do not all show; and what was laid
//! out, handed back for the next key to move from. A desk's pane scrolls this way, and so
//! does every region of a screen a terminal shows in part.

use super::Terminal;
use crate::components::{Laid, Sight};
use crate::doc::Doc;
use crate::glyphs::Glyphs;
use crate::line::{Line, Tone};
use crate::text::counted;

impl Terminal {
    /// `doc` in at most `rows` rows, from the line `sight` asks for, each cut to this
    /// stream's cells, and what was laid out of it. When lines lie below the last one
    /// shown, the row that counts them takes the last row.
    pub fn scrolled(&self, doc: &Doc, sight: Sight, rows: usize) -> (Vec<Line>, Laid) {
        let glyphs = self.look.glyphs;
        let lines = self.lines(doc);
        let most = lines.len().saturating_sub(rows);
        let pointed = sight
            .follows_pointer
            .then(|| {
                lines
                    .iter()
                    .position(|line| line.text().trim_start().starts_with(glyphs.pointer()))
            })
            .flatten();
        let laid = Laid {
            top: first_shown(sight.top, pointed, most, rows),
            page: rows,
            most,
        };
        (shown(&lines, laid, self.look.width.cells(), glyphs), laid)
    }
}

/// The first line shown: the one asked for, `top`, held within the last first line,
/// `most`, and moved so that the line a pointer stands on, `pointed`, is in sight among a
/// page of `page` rows.
fn first_shown(top: usize, pointed: Option<usize>, most: usize, page: usize) -> usize {
    let top = top.min(most);
    let Some(row) = pointed else {
        return top;
    };
    // Past the last line every row of the body shows a line, so none is kept for a count.
    let in_sight = if top == most {
        page
    } else {
        page.saturating_sub(1)
    };
    if row < top || row >= top + in_sight {
        row.min(most)
    } else {
        top
    }
}

/// The rows `laid` shows of `lines`, each cut to `cells`. When lines lie below the last
/// one shown, the row that counts them takes its place.
fn shown(lines: &[Line], laid: Laid, cells: usize, glyphs: Glyphs) -> Vec<Line> {
    let Laid { top, page, most } = laid;
    if top == most {
        return lines[top..]
            .iter()
            .map(|line| line.cut(cells, glyphs))
            .collect();
    }
    if page == 0 {
        return Vec::new();
    }
    let end = top + page - 1;
    let more = format!(
        "{} {}",
        glyphs.ellipsis(),
        counted(lines.len() - end, "more line")
    );
    let mut body: Vec<Line> = lines[top..end]
        .iter()
        .map(|line| line.cut(cells, glyphs))
        .collect();
    body.push(Line::new().push(Tone::Muted, more).cut(cells, glyphs));
    body
}

#[cfg(test)]
mod tests;
