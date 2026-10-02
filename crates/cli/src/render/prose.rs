//! Prose a session wrote, cut to the one line a row has for it.
//!
//! What an agent says comes as Markdown, and a row in a table has room
//! for its first sentence and nothing that only means something once
//! rendered: a heading's `#`, emphasis, a list's bullet. The sentence is
//! what a reader scans a column of; the whole of it is a command away.

use super::{truncate, Glyphs};

/// The first sentence of `markdown`, its markup dropped and its
/// whitespace collapsed, cut to `room` cells.
pub(crate) fn first_sentence(markdown: &str, room: usize, glyphs: Glyphs) -> String {
    // A heading names what follows rather than saying it, so the
    // sentence is the first one after it.
    let text: Vec<String> = markdown
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .skip_while(|line| line.is_empty())
        .take_while(|line| !line.is_empty())
        .map(unmarked)
        .collect();
    let text = yunta_core::text::one_line(&text.join(" "));
    let sentence = sentence_end(&text)
        .and_then(|end| text.get(..end))
        .unwrap_or(&text);
    truncate(sentence, room, glyphs).trim_end().to_string()
}

/// `line` without the markup that opens it — a quote, a bullet — or
/// wraps its words.
fn unmarked(line: &str) -> String {
    let opened = line
        .trim_start_matches('#')
        .trim_start_matches('>')
        .trim_start();
    let opened = ["- ", "* ", "+ "]
        .iter()
        .find_map(|bullet| opened.strip_prefix(bullet))
        .unwrap_or(opened);
    opened.replace("**", "").replace("__", "")
}

/// Where the first sentence of `text` ends: just after a `.`, `!` or `?`
/// that a space follows, which is the end of a sentence and not a dot
/// inside a path or a version.
fn sentence_end(text: &str) -> Option<usize> {
    text.char_indices()
        .zip(text.chars().skip(1))
        .find(|((_, end), next)| matches!(end, '.' | '!' | '?') && *next == ' ')
        .map(|((at, end), _)| at + end.len_utf8())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_gets_the_first_sentence_without_markup() {
        let said = "## Summary\n\n**Fixed** the lint in `src/lib.rs`. Then ran the suite.\n\nMore.";
        assert_eq!(
            first_sentence(said, 80, Glyphs::Ascii),
            "Fixed the lint in `src/lib.rs`."
        );
    }

    #[test]
    fn a_dot_inside_a_path_does_not_end_the_sentence() {
        assert_eq!(
            first_sentence("- edited v1.2 of a.rs and stopped", 80, Glyphs::Ascii),
            "edited v1.2 of a.rs and stopped"
        );
    }

    #[test]
    fn a_long_sentence_is_cut_to_its_room() {
        let cut = first_sentence("a very long sentence that does not fit", 12, Glyphs::Ascii);
        assert_eq!(cut, "a very long~");
    }
}
