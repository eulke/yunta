//! A problem quoted from the text it is about: where, the line, and a
//! mark under the place — the way a compiler points at a line.

use yunta_core::diagnostic::Report;
use yunta_core::yaml::Location;

use crate::cell_width;

/// The lines that place a problem at `at` in `text`, read from `path`:
/// the place, the line as written, and carets under the part at fault.
/// Nothing when `text` has no such line.
pub fn quoted(path: &str, text: &str, at: Location) -> Vec<String> {
    let Some(source) = text.lines().nth(at.line.saturating_sub(1)) else {
        return Vec::new();
    };
    // A tab is drawn as however many cells a terminal decides, so the
    // line and the caret under it agree on four.
    let source = source.replace('\t', "    ");
    let before: String = source.chars().take(at.col.saturating_sub(1)).collect();
    let rest = source
        .chars()
        .count()
        .saturating_sub(before.chars().count());
    let carets = "^".repeat(at.len.clamp(1, rest.max(1)));
    let number = at.line.to_string();
    let pad = " ".repeat(number.len());
    vec![
        format!("{pad}--> {path}:{}:{}", at.line, at.col),
        format!("{pad} |"),
        format!("{number} | {source}"),
        format!("{pad} | {}{carets}", " ".repeat(cell_width(&before))),
    ]
}

/// Every problem under `heading`, counted, each followed by the line it
/// is about when it has a place in `text` — the block `yunta check` and
/// a refused workflow both print.
pub fn located(
    heading: &str,
    problems: &[(String, Option<Location>)],
    path: &str,
    text: Option<&str>,
) -> String {
    let mut out = format!(
        "{heading}: {}",
        yunta_core::text::counted(problems.len(), "error")
    );
    for (said, at) in problems {
        out.push_str(&format!("\n  {said}"));
        let quote = match (text, at) {
            (Some(text), Some(at)) => quoted(path, text, *at),
            _ => Vec::new(),
        };
        for line in quote {
            out.push_str(&format!("\n    {line}"));
        }
    }
    out
}

/// A document's report, each problem quoted from `text` when it was read
/// from text.
pub fn report(report: &Report, text: Option<&str>) -> String {
    let problems: Vec<(String, Option<Location>)> = report
        .diagnostics
        .iter()
        .map(|diagnostic| match (text, diagnostic.at) {
            (Some(_), Some(at)) => (diagnostic.said(), Some(at)),
            _ => (diagnostic.to_string(), None),
        })
        .collect();
    located(
        &report.document.path,
        &problems,
        &report.document.path,
        text,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_problem_is_quoted_with_a_caret_under_its_place() {
        let text = "nodes:\n  - id: fix\n    runner: implementr\n";
        let at = Location {
            line: 3,
            col: 13,
            len: 10,
        };
        assert_eq!(
            quoted("wf.yaml", text, at),
            [
                " --> wf.yaml:3:13",
                "  |",
                "3 |     runner: implementr",
                "  |             ^^^^^^^^^^",
            ]
        );
    }

    #[test]
    fn a_place_past_the_end_of_the_text_quotes_nothing() {
        let at = Location {
            line: 9,
            col: 1,
            len: 1,
        };
        assert!(quoted("wf.yaml", "a: 1\n", at).is_empty());
    }
}
