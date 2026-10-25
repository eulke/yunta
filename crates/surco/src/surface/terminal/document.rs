//! Prose, marked items, code, and what a message says whole.

use crate::components::{Code, Marked, Note, Paths, Prose, Said};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, indent, wrap, wrap_unbroken, INDENT};

impl Prose {
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        paragraph(&self.0, Tone::Plain, look)
    }
}

impl Note {
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        paragraph(&self.0, Tone::Muted, look)
    }
}

/// `text` in `tone`, wrapped one step in. A word is never cut: a path or a
/// command cut in two is two things nobody can copy.
fn paragraph(text: &str, tone: Tone, look: &Look) -> Vec<Line> {
    let room = look.width.cells().saturating_sub(cell_width(INDENT));
    wrap_unbroken(text, room)
        .into_iter()
        .map(|part| Line::new().plain(INDENT).push(tone, part))
        .collect()
}

impl Marked {
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let glyph = look.glyphs.mark(self.mark).to_string();
        let under = format!("{INDENT}{}", " ".repeat(cell_width(&glyph) + 1));
        let room = look.width.cells().saturating_sub(cell_width(&under));
        let mut lines = Vec::new();
        for item in &self.items {
            for (at, part) in wrap_unbroken(item, room).into_iter().enumerate() {
                lines.push(match at {
                    0 => Line::new()
                        .plain(INDENT)
                        .push(Tone::of(self.mark), glyph.as_str())
                        .plain(" ")
                        .plain(part),
                    _ => Line::new().plain(under.as_str()).plain(part),
                });
            }
        }
        lines
    }
}

impl Code {
    /// Where the code is, on as many lines as the line's room asks: broken
    /// before a joiner, never inside the file's path, each part after the
    /// first under the file.
    fn titled(&self, look: &Look) -> Vec<Line> {
        let room = look.width.cells();
        let under = format!("{INDENT}{INDENT}");
        let margin = |at: usize| match at {
            0 => INDENT,
            _ => under.as_str(),
        };
        let mut parts = self.title_parts(look.glyphs).into_iter();
        let mut rows: Vec<String> = parts.next().into_iter().collect();
        for part in parts {
            let at = rows.len() - 1;
            let row = &mut rows[at];
            match cell_width(margin(at)) + cell_width(row) + 1 + cell_width(&part) <= room {
                true => {
                    row.push(' ');
                    row.push_str(&part);
                }
                false => rows.push(part),
            }
        }
        rows.into_iter()
            .enumerate()
            .map(|(at, row)| Line::new().plain(margin(at)).push(Tone::Strong, row))
            .collect()
    }

    /// Where the code is and what it is there for, its lines on the gutter
    /// they hang from, and under them, muted, what is said of it: what it is
    /// to the code that stands where it goes and how long it is, then where
    /// the rest is read.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let under = format!("{INDENT}{INDENT}");
        let room = look.width.cells().saturating_sub(cell_width(&under));
        let mut lines = self.titled(look);
        if let Some(what) = &self.what {
            lines.extend(
                wrap(what, room)
                    .into_iter()
                    .map(|part| Line::new().plain(under.as_str()).plain(part)),
            );
        }
        lines.extend(
            self.quoted(room, look)
                .into_iter()
                .map(|line| line.under(&under)),
        );
        let footer = self.footer().join(&format!(" {} ", look.glyphs.sep()));
        let rest = self.rest.as_ref().map(|rest| {
            let hidden = crate::text::counted(self.whole - self.lines.len(), "line");
            format!("{hidden} more: {rest}")
        });
        let said = (!footer.is_empty())
            .then_some(footer)
            .into_iter()
            .chain(rest);
        lines.extend(said.map(|said| Line::new().plain(under.as_str()).push(Tone::Muted, said)));
        lines
    }

    /// The code's lines inside `room`, each on the gutter it hangs from:
    /// marked where the code modifies what stands where it goes, and
    /// continued under itself where it is wider than the room.
    fn quoted(&self, room: usize, look: &Look) -> Vec<Line> {
        let gutter = format!("{} ", look.glyphs.gutter());
        let room = room.saturating_sub(cell_width(&gutter));
        crate::surface::diff::lines(self, look.glyphs)
            .iter()
            .flat_map(|line| match line.text().is_empty() {
                true => vec![Line::new().push(Tone::Muted, gutter.trim_end())],
                false => super::markdown::continued(line, room, look.glyphs)
                    .into_iter()
                    .map(|piece| Line::new().push(Tone::Muted, gutter.as_str()).then(piece))
                    .collect(),
            })
            .collect()
    }
}

impl Said {
    pub(in crate::surface) fn lines(&self, _look: &Look) -> Vec<Line> {
        self.0
            .iter()
            .map(|said| Line::new().plain(INDENT).plain(said.as_str()))
            .collect()
    }
}

impl Paths {
    pub(in crate::surface) fn lines(&self, _look: &Look) -> Vec<Line> {
        std::iter::once(Line::new().plain(INDENT).plain(self.heading.as_str()))
            .chain(self.paths.iter().map(|note| {
                let line = Line::new()
                    .plain(indent(2))
                    .plain(note.path.display().to_string());
                match &note.reason {
                    Some(reason) => line.plain(format!(" ({reason})")),
                    None => line,
                }
            }))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::components::Code;
    use crate::glyphs::Glyphs;
    use crate::golden::visible;
    use crate::look::Look;
    use crate::paint::Ink;

    /// `code` drawn plain at `cells`, in ASCII, as the text of each line.
    fn drawn(code: &Code, cells: usize) -> Vec<String> {
        let look = Look {
            width: Look::plain().width.within(cells),
            ..Look::plain()
        };
        code.lines(&look).iter().map(|line| line.text()).collect()
    }

    /// A signature a plan writes with a parameter more than today's.
    fn widened() -> Code {
        Code::whole(
            "crates/engine/src/catalog.rs",
            Some("accept global packs root and search local before global".to_string()),
            "pub fn resolve_workflow(repo_root: &Path, global_packs_root: &Path, name: &str) -> Result<ResolvedWorkflow, CatalogError>;",
        )
        .within(["resolve_workflow"])
        .against(Some(
            "pub fn resolve_workflow(repo_root: &Path, name: &str) -> Result<ResolvedWorkflow, CatalogError>;",
        ))
    }

    /// A title too long for its line breaks before a joiner, never inside
    /// the file's path; how long the code is is said under it.
    #[test]
    fn a_code_title_too_long_for_its_line_breaks_at_its_joiner() {
        let code = Code::whole("crates/engine/src/frozen/provenance.rs", None, "fn a();")
            .within(["pack_provenance"]);
        assert_eq!(
            drawn(&code, 44),
            [
                "  crates/engine/src/frozen/provenance.rs",
                "    :: pack_provenance",
                "    | fn a();",
                "    1 line",
            ]
        );
    }

    #[test]
    fn a_modified_code_marks_its_words_and_says_so_under_it() {
        assert_eq!(
            drawn(&widened(), 60),
            [
                "  crates/engine/src/catalog.rs :: resolve_workflow",
                "    accept global packs root and search local before global",
                "    | pub fn resolve_workflow(repo_root: &Path,",
                "    |   \\ {+global_packs_root: &Path+}, name: &str) ->",
                "    |   \\ Result<ResolvedWorkflow, CatalogError>;",
                "    modified ; 1 line",
            ]
        );
    }

    #[test]
    fn a_new_code_says_so_under_it_and_marks_nothing() {
        let code = Code::whole("src/greet.rs", None, "pub fn bye(name: &str) -> String;")
            .within(["bye"])
            .against(None);
        assert_eq!(
            drawn(&code, 60),
            [
                "  src/greet.rs :: bye",
                "    | pub fn bye(name: &str) -> String;",
                "    new ; 1 line",
            ]
        );
    }

    /// A break that falls inside the words a change puts in leaves each
    /// side of them painted as what goes in.
    #[test]
    fn a_mark_cut_by_a_continuation_is_painted_on_both_sides() {
        let look = Look {
            glyphs: Glyphs::Ascii,
            ink: Ink::Ansi16,
            width: Look::plain().width.within(70),
        };
        let painted: Vec<String> = widened()
            .lines(&look)
            .iter()
            .map(|line| visible(&look.ink.paint(line)))
            .collect();
        assert_eq!(
            painted[2],
            "    ␛[2m| ␛[0mpub fn resolve_workflow(repo_root: &Path, ␛[32m{+global_packs_root:␛[0m"
        );
        assert_eq!(
            painted[3],
            "    ␛[2m| ␛[0m  \\ ␛[32m&Path+}␛[0m, name: &str) -> Result<ResolvedWorkflow,"
        );
    }
}
