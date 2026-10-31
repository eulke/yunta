//! A file's code as Markdown: where it is and what it is there for, its
//! lines fenced in the language of its file, marked where it modifies the
//! code that stands where it goes, and what is said of it under the fence.
//! Where a description leaves code lines out, the lines and the fence go,
//! and the title and what is said of the code stay.

use super::{code_span, plain_text, Leaves, Page, GLYPHS};
use crate::components::Code;

/// Code as `page` says it: fenced with its lines, or, where the page leaves
/// code lines out, only its title and what is said under it.
pub(super) fn drawn(code: &Code, page: Page) -> String {
    match page.leaves >= Leaves::CodeLines {
        true => without_lines(code),
        false => fenced(code),
    }
}

/// Code under where it is and what it is there for, fenced in the language
/// of its file; then, in italics, what it is to the code that stands where
/// it goes and how long it is, and where the rest is read.
fn fenced(code: &Code) -> String {
    let mut out = title(code);
    if !code.lines.is_empty() {
        let lines: Vec<String> = crate::surface::diff::lines(code, GLYPHS)
            .iter()
            .map(|line| line.text())
            .collect();
        let lines = lines.join("\n");
        let fence = fence(&lines);
        out.push_str(&format!("\n\n{fence}{}\n{lines}\n{fence}", code.language()));
    }
    out.push_str(&footer(code));
    if let Some(rest) = &code.rest {
        out.push_str(&format!(
            "\n\n{} more: {}",
            crate::text::counted(code.whole - code.lines.len(), "line"),
            code_span(rest)
        ));
    }
    out
}

/// Code with its lines left out: its title and what is said under it, which
/// still says how long the code is.
fn without_lines(code: &Code) -> String {
    format!("{}{}", title(code), footer(code))
}

/// Where the code is and what it is there for: its title in backticks, then
/// what it is for after a colon, when it is for anything.
fn title(code: &Code) -> String {
    let mut out = code_span(&crate::surface::code_title(code, GLYPHS).join(" "));
    if let Some(what) = &code.what {
        out.push_str(&format!(": {}", plain_text(what)));
    }
    out
}

/// What is said under the code's lines, in italics after a blank line: what
/// it is to the code that stands where it goes and how long it is. Empty
/// when nothing is said.
fn footer(code: &Code) -> String {
    let said = code.footer();
    match said.is_empty() {
        true => String::new(),
        false => format!("\n\n_{}_", said.join(&format!(" {} ", GLYPHS.sep()))),
    }
}

/// The fence around `text` as code: one backtick more than the longest
/// run that opens any of its lines, and three at least, so a fence the
/// text holds — a Markdown file's own — never closes the one around it.
pub(super) fn fence(text: &str) -> String {
    let longest = text
        .lines()
        .map(|line| line.trim_start().chars().take_while(|c| *c == '`').count())
        .max()
        .unwrap_or_default();
    "`".repeat((longest + 1).max(MIN_FENCE))
}

/// The fewest signs that open or close a fence: CommonMark opens and closes a
/// fence with three signs, so a shorter run is ordinary text.
pub(super) const MIN_FENCE: usize = 3;

#[cfg(test)]
mod tests {
    use crate::components::{Code, Section};
    use crate::doc::Doc;
    use crate::line::Line;
    use crate::surface::{Markdown, Surface};

    /// Code that holds a fence of its own is fenced longer, so what comes
    /// after it is never read as code.
    #[test]
    fn a_fence_the_code_holds_never_closes_the_one_around_it() {
        let code = Code::whole("notes/install.md", None, "```bash\nyunta run\n```\n");
        let after = Section {
            lead: None,
            title: Line::new().plain("proof"),
            blocks: Vec::new(),
        };
        let drawn = Markdown.draw(&Doc::new().with(code).with(after));
        assert!(drawn.contains("````markdown\n```bash"), "{drawn}");
        let open = drawn.matches("````").count();
        assert_eq!(open, 2, "{drawn}");
        assert!(drawn.trim_end().ends_with("\n\n## proof"), "{drawn}");
    }

    #[test]
    fn a_modified_code_is_fenced_with_its_marks_and_said_under_the_fence() {
        let code = Code::whole(
            "crates/engine/src/catalog.rs",
            Some("accept global packs root and search local before global".to_string()),
            "pub fn resolve_workflow(repo_root: &Path, global_packs_root: &Path, name: &str) -> Result<ResolvedWorkflow, CatalogError>;",
        )
        .within(["resolve_workflow"])
        .against(Some(
            "pub fn resolve_workflow(repo_root: &Path, name: &str) -> Result<ResolvedWorkflow, CatalogError>;",
        ));
        assert_eq!(
            Markdown.draw(&Doc::new().with(code)),
            "`crates/engine/src/catalog.rs › resolve_workflow`: accept global packs root and search local before global\n\
             \n\
             ```rust\n\
             pub fn resolve_workflow(repo_root: &Path, {+global_packs_root: &Path+}, name: &str) -> Result<ResolvedWorkflow, CatalogError>;\n\
             ```\n\
             \n\
             _modified · 1 line_\n"
        );
    }
}
