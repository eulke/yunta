//! Markdown an author wrote, read for the fences it leaves open: a block that
//! ends inside a fence it opened is closed where it ends, so it does not run on
//! into what follows it. Fences are read as CommonMark reads them, in list
//! items too.

use super::code::MIN_FENCE;

/// The most spaces a line may be indented past its container: four make it code.
const MAX_INDENT: usize = 3;

/// The fewest signs a thematic break has: a break is three or more of one
/// sign, so a shorter run is text.
const MIN_BREAK: usize = 3;

/// The most spaces after a list marker before its text: five or more make code.
const MAX_PAD: usize = 4;

/// The most digits an ordered list marker has: CommonMark allows nine, so a
/// longer number is text.
const MAX_DIGITS: usize = 9;

/// The run of signs a fence line starts with: a backtick or a tilde, how many
/// times it repeats, and how many spaces come before it.
#[derive(Clone, Copy)]
struct Run {
    sign: char,
    len: usize,
    indent: usize,
}

impl Run {
    /// The line that closes the fence this run opens: the run as it is written,
    /// indented as its opener is, so that a fence in a list item closes in it.
    fn closer(self) -> String {
        let run = self.sign.to_string().repeat(self.len);
        format!("{}{run}", " ".repeat(self.indent))
    }
}

/// The text as it is, followed, when it ends inside a fence it opened, by the
/// line that closes that fence, so that such a block does not run on into what
/// follows it. The caller trims the text first.
///
/// Fences are read as CommonMark reads them, in list items too: a line not
/// indented into an item ends it, and its fence, unless it continues a
/// paragraph lazily. The closing line takes the opening line's indentation.
/// Not read: fences on list marker lines, tabs as indentation, HTML blocks, and
/// block quote content beyond the paragraph a quoted line leaves open.
pub(super) fn closed(text: &str) -> String {
    let mut open = Open::default();
    for line in text.split('\n') {
        // A carriage return ends a CRLF line, so it is not part of the line.
        open.read(line.strip_suffix('\r').unwrap_or(line));
    }
    match open.fence {
        Some(run) => format!("{text}\n{}", run.closer()),
        None => text.to_string(),
    }
}

/// What the lines read so far leave open.
#[derive(Default)]
struct Open {
    /// The fence a line opened that no line has closed yet, if any.
    fence: Option<Run>,
    /// The content column of each open list item, outermost first.
    items: Vec<usize>,
    /// The open paragraph, if any: `Some(true)` when a block quote holds it.
    paragraph: Option<bool>,
    /// Whether the last line opened a list item with no text after its marker,
    /// so the next blank line ends that item.
    empty: bool,
}

/// What a line starts, read in the container its content begins in.
enum Start {
    Fence(Run),
    /// A break; true when it is a block quote with text, which opens a paragraph.
    Break(bool),
    /// A list item, with its content column and whether text follows its marker.
    Item(usize, bool),
    /// Text: indented code when no paragraph is open and the line is indented four spaces or more.
    Text,
}

impl Open {
    /// Reads the next line: inside a fence, code or the closing line.
    fn read(&mut self, line: &str) {
        if line.chars().all(|c| matches!(c, ' ' | '\t')) {
            // An item that starts with a blank line is empty, and ends there.
            if std::mem::take(&mut self.empty) {
                self.items.pop();
            }
            self.paragraph = None;
            return;
        }
        let body = line.trim_start_matches(' ');
        let indent = line.len() - body.len();
        if let Some(run) = self.fence {
            let column = self.items.last().copied().unwrap_or_default();
            if indent >= column {
                if closes(body, indent - column, run) {
                    self.fence = None;
                }
                return;
            }
            // Not indented into the item: the item ends, and the fence with it.
            self.fence = None;
        }
        self.block(line, indent);
    }

    /// Reads a line outside a fence: the items it is not indented into end,
    /// unless it continues a paragraph lazily.
    fn block(&mut self, line: &str, indent: usize) {
        let kept = self.items.partition_point(|&content| content <= indent);
        let popped = self.items.split_off(kept);
        let container = self.items.last().copied().unwrap_or_default();
        // A plain paragraph in the items the line is indented into restricts the
        // items that may start here; a quoted one, outside its quote, does not.
        let restricted = self.paragraph == Some(false) && popped.is_empty();
        let what = classify(line, container, restricted);
        self.empty = false;
        if matches!(what, Start::Text) && self.paragraph.is_some() {
            // Text after an open paragraph continues it, so no item ends.
            self.items.extend(popped);
            return;
        }
        self.paragraph = match what {
            Start::Fence(run) => {
                self.fence = Some(run);
                None
            }
            Start::Break(quoted) => quoted.then_some(true),
            Start::Item(content, text) => {
                self.items.push(content);
                self.empty = !text;
                text.then_some(false)
            }
            Start::Text => (indent - container <= MAX_INDENT).then_some(false),
        };
    }
}

/// What a line starts in the container whose content begins at `container`.
/// A line indented more than three spaces past it is text.
fn classify(line: &str, container: usize, restricted: bool) -> Start {
    let body = line.trim_start_matches(' ');
    let indent = line.len() - body.len();
    if indent - container > MAX_INDENT {
        return Start::Text;
    }
    if let Some(run) = opening(body, indent) {
        return Start::Fence(run);
    }
    if crate::code::heading_level(body).is_some() || thematic(body) {
        return Start::Break(false);
    }
    if let Some(rest) = body.strip_prefix('>') {
        return Start::Break(!rest.trim().is_empty());
    }
    if restricted && setext(body) {
        return Start::Break(false);
    }
    item(body, indent, restricted).unwrap_or(Start::Text)
}

/// Whether a body is three or more of one of `-`, `*` or `_`, spaces apart or not.
fn thematic(body: &str) -> bool {
    let signs: Vec<char> = body.chars().filter(|c| !matches!(*c, ' ' | '\t')).collect();
    let first = signs.first().copied().unwrap_or(' ');
    signs.len() >= MIN_BREAK
        && matches!(first, '-' | '*' | '_')
        && signs.iter().all(|c| *c == first)
}

/// Whether a body is a setext underline: a run of `=` or of `-`, then spaces and tabs.
fn setext(body: &str) -> bool {
    let run = body.trim_end_matches([' ', '\t']);
    !run.is_empty() && (run.bytes().all(|b| b == b'=') || run.bytes().all(|b| b == b'-'))
}

/// The list item a body starts: its content column, and whether text follows
/// its marker. A restricted line needs text, and a bullet or a numbered one.
fn item(body: &str, indent: usize, restricted: bool) -> Option<Start> {
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    let bullet = matches!(body.bytes().next(), Some(b'-' | b'+' | b'*'));
    let ordered = (1..=MAX_DIGITS).contains(&digits)
        && matches!(body.as_bytes().get(digits).copied(), Some(b'.' | b')'));
    if !bullet && !ordered {
        return None;
    }
    let marker = if bullet { 1 } else { digits + 1 };
    let after = body.get(marker..)?;
    if !after.is_empty() && !after.starts_with(' ') {
        return None;
    }
    let rest = after.trim_start_matches(' ');
    let text = !rest.is_empty();
    let first = ordered && body.get(..digits).and_then(|n| n.parse::<usize>().ok()) == Some(1);
    if restricted && (!text || (ordered && !first)) {
        return None;
    }
    // Five or more spaces before the text make it code: the item then starts
    // one space past its marker, as an item without text does.
    let pad = after.len() - rest.len();
    let gap = if text && pad <= MAX_PAD { pad } else { 1 };
    Some(Start::Item(indent + marker + gap, text))
}

/// The fence a body opens at `indent`: a run of three or more signs, where a
/// backtick run has no backtick after it.
fn opening(body: &str, indent: usize) -> Option<Run> {
    let (sign, len, rest) = fence_run(body)?;
    (sign != '`' || !rest.contains('`')).then_some(Run { sign, len, indent })
}

/// Whether a body, `rel` spaces past its container, closes the fence `opened`.
fn closes(body: &str, rel: usize, opened: Run) -> bool {
    rel <= MAX_INDENT
        && fence_run(body).is_some_and(|(sign, len, rest)| {
            sign == opened.sign && len >= opened.len && rest.chars().all(|c| c == ' ' || c == '\t')
        })
}

/// The sign and length of the run a body starts with, and what follows the run,
/// when the run is three or more backticks or tildes.
fn fence_run(body: &str) -> Option<(char, usize, &str)> {
    let sign = body.chars().next().filter(|c| matches!(*c, '`' | '~'))?;
    let rest = body.trim_start_matches(sign);
    // A sign is one byte, so the bytes a run takes are the signs it counts.
    let len = body.len() - rest.len();
    (len >= MIN_FENCE).then_some((sign, len, rest))
}

#[cfg(test)]
mod tests {
    use super::closed;

    /// Text that ends inside a backtick fence gets a line of three backticks.
    #[test]
    fn a_text_ending_inside_a_backtick_fence_gets_its_closing_line() {
        assert_eq!(
            closed("Intro.\n```rust\nlet a = 1;"),
            "Intro.\n```rust\nlet a = 1;\n```"
        );
    }

    /// Text whose fences all close comes back as it is.
    #[test]
    fn a_text_whose_fences_all_close_comes_back_unchanged() {
        let text = "```rust\nlet a = 1;\n```\n\nSee `a` here.\n\n~~~\nb\n~~~";
        assert_eq!(closed(text), text);
    }

    /// A fence of four backticks is closed by four, not by a line of three.
    #[test]
    fn a_fence_of_four_backticks_is_closed_only_by_four() {
        assert_eq!(
            closed("````\n```\nlet b = 2;"),
            "````\n```\nlet b = 2;\n````"
        );
        assert_eq!(closed("````\n```\n````"), "````\n```\n````");
    }

    /// A tilde fence is closed by tildes, and backticks inside it are code.
    #[test]
    fn a_tilde_fence_is_closed_by_tildes_and_not_by_backticks() {
        assert_eq!(closed("~~~\nb\n```\n~~~"), "~~~\nb\n```\n~~~");
        assert_eq!(closed("~~~\nb\n```"), "~~~\nb\n```\n~~~");
    }

    /// Backticks with another backtick after their run open no fence.
    #[test]
    fn backticks_with_another_backtick_after_their_run_open_no_fence() {
        assert_eq!(closed("```a``` is inline code."), "```a``` is inline code.");
    }

    /// A line indented by four spaces is code, not a fence.
    #[test]
    fn a_fence_indented_by_four_spaces_opens_no_fence() {
        assert_eq!(closed("    ```\nlet a = 1;"), "    ```\nlet a = 1;");
    }

    /// A fence in a list item is closed by a line at the item's indentation.
    #[test]
    fn a_fence_in_a_list_item_is_closed_at_its_indentation() {
        assert_eq!(
            closed("- a\n\n  ```bash\n  x"),
            "- a\n\n  ```bash\n  x\n  ```"
        );
        assert_eq!(
            closed("1. a\n\n   ```\n   x"),
            "1. a\n\n   ```\n   x\n   ```"
        );
    }

    /// A run followed by text is code, so it leaves its fence open.
    #[test]
    fn a_run_followed_by_text_leaves_its_fence_open() {
        assert_eq!(closed("```\ncode\n``` x"), "```\ncode\n``` x\n```");
    }

    /// A closing run indented by four spaces is code, not the fence's end.
    #[test]
    fn a_closing_run_indented_by_four_spaces_is_code() {
        assert_eq!(closed("```\ncode\n    ```"), "```\ncode\n    ```\n```");
    }

    /// A closing run may be followed by a tab, as CommonMark allows.
    #[test]
    fn a_closing_run_may_be_followed_by_a_tab() {
        assert_eq!(closed("```\nx\n```\t\nafter"), "```\nx\n```\t\nafter");
    }

    /// A fence in a list item ends with the item, so text after the item is not in it.
    #[test]
    fn a_fence_in_a_list_item_ends_with_the_item() {
        let text = "- a\n\n  ```\n  x\n\nDone";
        assert_eq!(closed(text), text);
        let nested = "- a\n  - b\n\n    ```\n    x\n  y";
        assert_eq!(closed(nested), nested);
    }

    /// A lazy paragraph line does not end the list item that holds its fence.
    #[test]
    fn a_lazy_paragraph_line_keeps_the_list_item_open() {
        assert_eq!(closed("- a\nb\n  ```\n  x"), "- a\nb\n  ```\n  x\n  ```");
    }

    /// A break ends the items it is not indented into, and a quote's paragraph lets an item start.
    #[test]
    fn a_break_not_indented_into_a_list_item_ends_it() {
        assert_eq!(closed("- a\n# h\n  ```\nx"), "- a\n# h\n  ```\nx\n  ```");
        assert_eq!(closed("- a\n---\n  ```\nx"), "- a\n---\n  ```\nx\n  ```");
        assert_eq!(closed("- a\n> q\n  ```\nx"), "- a\n> q\n  ```\nx\n  ```");
        assert_eq!(closed("> q\n2. d\n   ```"), "> q\n2. d\n   ```\n   ```");
    }

    /// A setext underline ends the paragraph of its item, so a line after it is not lazy.
    #[test]
    fn a_setext_underline_ends_the_paragraph_of_its_item() {
        assert_eq!(
            closed("- x\n  -\nlazy\n  ```\n  y\nDone"),
            "- x\n  -\nlazy\n  ```\n  y\nDone\n  ```"
        );
    }

    /// A numbered item past one is text, so the fence after it stays in the
    /// bullet item above and gets its closing line at the fence's own indent.
    /// An item with no text ends at the blank line after its marker, so an
    /// indented fence after that line is code. A fence four spaces past its
    /// item's text is code too.
    #[test]
    fn a_numbered_item_past_one_is_text_and_four_spaces_is_code() {
        assert_eq!(
            closed("- a\n  b\n  2. c\n\n     ```\n     x\n   y"),
            "- a\n  b\n  2. c\n\n     ```\n     x\n   y\n     ```"
        );
        let blank_start = "- \n\n     ```";
        assert_eq!(closed(blank_start), blank_start);
        let four = "- a\n\n      ```\n      x";
        assert_eq!(closed(four), four);
    }

    /// A carriage return that ends a line is not part of the line.
    #[test]
    fn a_carriage_return_ending_a_line_is_not_part_of_it() {
        assert_eq!(closed("```\r\n```\r\nx"), "```\r\n```\r\nx");
    }
}
