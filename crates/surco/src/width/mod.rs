//! Measuring text for a column, and cutting it to fit one.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::glyphs::Glyphs;

/// The width a rendered line stays inside.
///
/// Eighty cells is the floor a terminal is taken to have, and the width
/// a line still has to survive once it leaves the terminal — pasted
/// into a review, an issue, a log. A surface sizes its columns against
/// this and cuts what does not fit, because a wrap costs more than the
/// characters it would have dropped: it lands mid-column, and the table
/// a reader was scanning down stops being one.
///
/// Decided once, here: how wide a line may be is not a property of what
/// is being said, and a second surface deciding it again is a second
/// answer. That includes a test, which reads what a surface drew and
/// has to check it against the same number the surface used.
pub const LINE_WIDTH: usize = 80;

/// The cells a name gets in a column of names — a node id, a runner, a
/// mode. Twelve is what [`LINE_WIDTH`] has left for a name once the bar
/// and the numbers beside it have taken their share.
pub const LABEL_WIDTH: usize = 12;

/// One step of indent: how far a block's body sits from the line that
/// introduced it.
///
/// One value for every surface, because the blocks nest — the trailer a
/// stopped run leaves behind hangs a decision inside a block that is
/// already one step in — and two steps of different widths turn a
/// nesting into a ragged left edge. Two spaces, so a block four levels
/// deep still leaves [`LINE_WIDTH`] most of its room for words.
pub const INDENT: &str = "  ";

/// The gap between a command a reader types and what it does, in every
/// column of commands.
pub const GAP: &str = "   ";

/// One step of [`INDENT`] in display cells, for a caller that lays a
/// column out by arithmetic rather than by prefixing a string — the
/// same step, so a list whose rows are *computed* still lines up with
/// the blocks that are *indented*.
pub const INDENT_WIDTH: usize = INDENT.len();

/// How deep a thing that belongs to a node sits under it: a child run
/// under the node that bore it, a `parallel` group's children under the
/// group. One step — a surface that indented one of them
/// further would say they belong to different things.
pub const CHILD_DEPTH: usize = 1;

/// `depth` steps of [`INDENT`], for a caller that places a line at a
/// known depth rather than under the line above it.
pub fn indent(depth: usize) -> String {
    INDENT.repeat(depth)
}

/// How many display cells `text` occupies.
///
/// Cells, not characters: a CJK ideograph and most emoji take two, a
/// combining mark takes none, and a column measured in characters is
/// therefore a column that moves. East-Asian *ambiguous* characters —
/// which is what the geometric shapes and block elements this module
/// draws with are — count as one cell, the width a terminal outside an
/// East Asian locale gives them, and the width every glyph set here is
/// chosen against.
pub fn cell_width(text: &str) -> usize {
    text.width()
}

/// `text` as lines that each occupy `width` display cells or fewer.
///
/// For a caller that owns a column and places what goes in it — a block
/// under an indent, an option read above a list — so the break falls
/// where the caller chose rather than wherever the terminal ran out of
/// row. The measure is display cells and not characters, because that
/// is what a terminal lays a line out in: a CJK label takes two cells
/// per character and a combining mark takes none.
///
/// Breaks fall between words where there is one to break at, and
/// between clusters where a single word is wider than the column — the
/// one case with nowhere else to go. Runs of whitespace collapse into
/// the breaks, so the result is the text, not its layout. Text with no
/// words in it is one empty line.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        for piece in pieces(word, width) {
            let spaced = usize::from(!line.is_empty());
            if cell_width(&line) + spaced + cell_width(&piece) > width && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(&piece);
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// `items` on lines of `room` cells, `sep` a space before it after each
/// but the last: [`joined`] with a separator that stands apart.
pub fn packed(items: &[&str], sep: char, room: usize) -> Vec<String> {
    joined(items, &format!(" {sep}"), room)
}

/// `items` on lines of `room` cells, each but the last followed by `sep`,
/// a line broken only between two items: one never starts on a line and
/// ends on the next. An item wider than the room is wrapped on lines of its
/// own, never cut inside a word, its separator at the end of its last line:
/// a separator never takes a line alone.
pub fn joined(items: &[&str], sep: &str, room: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for (at, item) in items.iter().enumerate() {
        let sep = (at + 1 < items.len()).then_some(sep);
        let piece = format!("{item}{}", sep.unwrap_or_default());
        if cell_width(&piece) > room {
            lines.extend((!line.is_empty()).then(|| std::mem::take(&mut line)));
            lines.extend(wrapped_with(item, sep, room));
            continue;
        }
        let joined = format!("{line} {piece}");
        if line.is_empty() {
            line = piece;
        } else if cell_width(&joined) <= room {
            line = joined;
        } else {
            lines.push(std::mem::replace(&mut line, piece));
        }
    }
    lines.extend((!line.is_empty()).then_some(line));
    lines
}

/// `item` wrapped on lines of `room` cells, never cut inside a word, with
/// `sep` ending its last line when one follows it.
fn wrapped_with(item: &str, sep: Option<&str>, room: usize) -> Vec<String> {
    let Some(sep) = sep else {
        return wrap_unbroken(item, room);
    };
    let mut lines = wrap_unbroken(item, room.saturating_sub(cell_width(sep)));
    if let Some(last) = lines.last_mut() {
        last.push_str(sep);
    }
    lines
}

/// `text` wrapped like [`wrap`], except that a word wider than the column
/// is never cut: it takes a line of its own, whole, and runs past the
/// edge. For a value a reader copies — a path, a branch, a command —
/// where a cut leaves two halves nobody can open.
pub fn wrap_unbroken(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let spaced = usize::from(!line.is_empty());
        if cell_width(&line) + spaced + cell_width(word) > width && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// `command` in lines that each fit `width`, cut only at a space no quote
/// holds, every line but the last ending in ` \` and each after the first
/// set in by two — so the lines a reader copies together are still the
/// one command a shell reads. A word wider than the line stays whole.
pub fn command_lines(command: &str, width: usize) -> Vec<String> {
    const GOES_ON: &str = " \\";
    const UNDER: &str = "  ";
    if cell_width(command) <= width {
        return vec![command.to_string()];
    }
    let words = shell_words(command);
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for (at, word) in words.iter().enumerate() {
        // The last line needs no room for the mark that it goes on.
        let rest = words[at..].join(" ");
        let room = match cell_width(&line) + 1 + cell_width(&rest) <= width {
            true => width,
            false => width.saturating_sub(cell_width(GOES_ON)),
        };
        if !line.trim().is_empty() && cell_width(&line) + 1 + cell_width(word) > room {
            lines.push(format!("{line}{GOES_ON}"));
            line = UNDER.to_string();
        } else if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    lines.push(line);
    lines
}

/// The words of `command` as a shell splits them: at spaces outside
/// quotes, a quoted stretch kept whole with its quotes.
fn shell_words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in command.chars() {
        match (quote, ch) {
            _ if escaped => escaped = false,
            (Some('\''), '\'') | (Some('"'), '"') => quote = None,
            (None, '\'' | '"') => quote = Some(ch),
            (None | Some('"'), '\\') => escaped = true,
            (None, ' ' | '\t') => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
                continue;
            }
            _ => {}
        }
        word.push(ch);
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// `line` in pieces that each occupy `width` display cells or fewer, cut
/// between clusters and nowhere else — for text whose spacing is its
/// meaning, like a line of code, which [`wrap`] would re-flow.
pub fn cut(line: &str, width: usize) -> Vec<String> {
    pieces(line, width.max(1))
}

/// `word` in the fewest pieces that each fit `width`: itself when it
/// already does, and cuts between clusters when it does not.
fn pieces(word: &str, width: usize) -> Vec<String> {
    if cell_width(word) <= width {
        return vec![word.to_string()];
    }
    let mut out = Vec::new();
    let mut piece = String::new();
    for cluster in clusters(word) {
        if cell_width(&piece) + cell_width(cluster) > width && !piece.is_empty() {
            out.push(std::mem::take(&mut piece));
        }
        piece.push_str(cluster);
    }
    if !piece.is_empty() {
        out.push(piece);
    }
    out
}

/// `text` in exactly `width` display cells: padded with spaces when it
/// is narrower, cut and closed with `glyphs`' truncation mark when it is
/// wider.
///
/// The caller gets `width` cells for any input, which is what lets the
/// columns of a row line up. The cut lands between clusters, never
/// inside one: half a cluster is not a narrower character, it is a
/// different one, and a terminal draws the leftover mark on whatever
/// follows or as a replacement box — either way wider than the cell it
/// was given.
pub fn truncate(text: &str, width: usize, glyphs: Glyphs) -> String {
    let total = cell_width(text);
    if total <= width {
        return pad(text.to_string(), width.saturating_sub(total));
    }
    let mark = glyphs.ellipsis();
    let mark_cells = cell_width(mark);
    if width <= mark_cells {
        return mark.chars().take(width).collect();
    }
    // The mark takes its own cells — one in Unicode, three in ASCII — so a
    // cut keeps what is left beside it.
    let room = width - mark_cells;
    let mut out = String::new();
    let mut used = 0;
    for cluster in clusters(text) {
        let cells = cell_width(cluster);
        if used + cells > room {
            break;
        }
        out.push_str(cluster);
        used += cells;
    }
    // A cut closes on the last word it kept, never on the space after it.
    out.truncate(out.trim_end().len());
    out.push_str(mark);
    let cells = cell_width(&out);
    pad(out, width.saturating_sub(cells))
}

/// The cells a column of `ids` takes: the widest of them, so that no id
/// in it is cut. An id is what a reader types into the next command, and
/// a column narrower than one turns two ids that share a prefix — a
/// fan-out's `review@claude-code` and `review@codex` — into one.
pub fn id_column<'a>(ids: impl IntoIterator<Item = &'a str>) -> usize {
    ids.into_iter().map(cell_width).max().unwrap_or(0)
}

/// `id` in exactly `width` cells: whole and padded when it fits, and cut
/// in the middle otherwise — its head, the cut mark, its tail — since the
/// two ends are where ids that share a prefix or a suffix differ.
pub fn middle_cut(id: &str, width: usize, glyphs: Glyphs) -> String {
    let total = cell_width(id);
    if total <= width {
        return pad(id.to_string(), width - total);
    }
    let mark = glyphs.ellipsis();
    let mark_cells = cell_width(mark);
    if width <= mark_cells {
        return mark.chars().take(width).collect();
    }
    let room = width - mark_cells;
    let pieces = clusters(id);
    let mut head = String::new();
    let mut head_cells = 0;
    for piece in &pieces {
        let cells = cell_width(piece);
        if head_cells + cells > room / 2 {
            break;
        }
        head.push_str(piece);
        head_cells += cells;
    }
    let mut tail: Vec<&str> = Vec::new();
    let mut tail_cells = 0;
    for piece in pieces.iter().rev() {
        let cells = cell_width(piece);
        if head_cells + tail_cells + cells > room {
            break;
        }
        tail.push(piece);
        tail_cells += cells;
    }
    tail.reverse();
    let cut = format!("{head}{mark}{}", tail.concat());
    let cells = cell_width(&cut);
    pad(cut, width.saturating_sub(cells))
}

/// `text` split where a cut may land.
///
/// A piece is a character that takes at least one cell together with
/// everything that hangs off it: the combining marks and variation
/// selectors that follow it, which take none of their own, whatever a
/// zero-width joiner binds to it, and — the one sequence a width cannot
/// reveal, because both halves are a cell wide on their own — the
/// regional indicator that follows another to make a flag. That is the
/// unit a terminal draws.
///
/// Each piece is measured whole, by [`cell_width`], because a sequence is
/// not always as wide as its characters add up to: a joined family and a
/// dingbat wearing an emoji variation selector are each drawn in the
/// cells of one glyph.
fn clusters(text: &str) -> Vec<&str> {
    const ZERO_WIDTH_JOINER: char = '\u{200d}';
    /// The letters a pair of which names a country, and which mean
    /// nothing alone: cutting between two of them turns a flag into a
    /// lettered box, which is the cut this function exists to prevent.
    const REGIONAL_INDICATORS: std::ops::RangeInclusive<char> = '\u{1f1e6}'..='\u{1f1ff}';

    let mut out = Vec::new();
    let mut start = 0;
    let mut joined = false;
    let mut half_a_flag = false;
    for (at, ch) in text.char_indices() {
        if at > start && ch.width().unwrap_or(0) > 0 && !joined && !half_a_flag {
            out.push(text.get(start..at).unwrap_or_default());
            start = at;
        }
        joined = ch == ZERO_WIDTH_JOINER;
        // A third indicator opens the next flag rather than extending
        // this one, which is how a terminal pairs them too.
        half_a_flag = REGIONAL_INDICATORS.contains(&ch) && !half_a_flag;
    }
    if start < text.len() {
        out.push(text.get(start..).unwrap_or_default());
    }
    out
}

fn pad(mut text: String, cells: usize) -> String {
    text.extend(std::iter::repeat_n(' ', cells));
    text
}

#[cfg(test)]
mod tests;
