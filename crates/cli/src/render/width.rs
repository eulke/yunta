//! Measuring text for a column, and cutting it to fit one.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::glyphs::Glyphs;

/// The width a rendered line stays inside.
///
/// Eighty cells is the floor a terminal is taken to have, and the width
/// a line still has to survive once it leaves the terminal — pasted into
/// a review, an issue, a log. A surface sizes its columns against this
/// and cuts what does not fit, because a wrap costs more than the
/// characters it would have dropped: it lands mid-column, and the table
/// a reader was scanning down stops being one.
pub(crate) const LINE_WIDTH: usize = 80;

/// The cells a name gets in a column of names — a node id, a runner, a
/// mode. Twelve is what [`LINE_WIDTH`] has left for a name once the bar
/// and the numbers beside it have taken their share.
pub(crate) const LABEL_WIDTH: usize = 12;

/// How many display cells `text` occupies.
///
/// Cells, not characters: a CJK ideograph and most emoji take two, a
/// combining mark takes none, and a column measured in characters is
/// therefore a column that moves. East-Asian *ambiguous* characters —
/// which is what the geometric shapes and block elements this module
/// draws with are — count as one cell, the width a terminal outside an
/// East Asian locale gives them, and the width every glyph set here is
/// chosen against.
pub(crate) fn cell_width(text: &str) -> usize {
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
pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
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
pub(crate) fn truncate(text: &str, width: usize, glyphs: Glyphs) -> String {
    let total = cell_width(text);
    if total <= width {
        return pad(text.to_string(), width.saturating_sub(total));
    }
    if width == 0 {
        return String::new();
    }
    // The mark is one cell in either glyph set, so it always has room.
    let room = width.saturating_sub(1);
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
    out.push(glyphs.ellipsis());
    let cells = cell_width(&out);
    pad(out, width.saturating_sub(cells))
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
mod tests {
    use super::*;

    /// Ten cells is the column; each name below asks for it and, counted
    /// in characters instead of cells, takes a different number.
    const COLUMN: usize = 10;

    #[test]
    fn a_cjk_name_fills_its_column_and_no_more() {
        // Eight ideographs: eight characters, sixteen cells.
        let cut = truncate("実装レビュー担当", COLUMN, Glyphs::Unicode);
        assert_eq!(cell_width(&cut), COLUMN);
        assert_eq!(cut, "実装レビ… ");
    }

    #[test]
    fn a_name_carrying_an_emoji_fills_its_column_and_no_more() {
        // Ten characters, eleven cells: the rocket takes two.
        let cut = truncate("ship 🚀 now", COLUMN, Glyphs::Unicode);
        assert_eq!(cell_width(&cut), COLUMN);
        assert_eq!(cut, "ship 🚀 n…");
    }

    #[test]
    fn a_name_carrying_a_combining_mark_fills_its_column_and_no_more() {
        // Twelve characters, eleven cells: `e` + U+0301 is drawn in one.
        let cut = truncate("cafe\u{301}-review", COLUMN, Glyphs::Unicode);
        assert_eq!(cell_width(&cut), COLUMN);
        assert_eq!(cut, "cafe\u{301}-revi…");
    }

    #[test]
    fn a_cut_never_lands_inside_a_cluster() {
        // A zero-width-joined family is one cluster drawn in two cells.
        let name = "xx\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}yy";
        // Four cells leave the family no room: it goes in whole or not
        // at all, and cutting between its characters is not the answer.
        let cut = truncate(name, 4, Glyphs::Unicode);
        assert_eq!(cut, "xx… ");
        // Five cells fit it, and it arrives with both joiners intact.
        assert_eq!(
            truncate(name, 5, Glyphs::Unicode),
            "xx\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}…"
        );
    }

    #[test]
    fn a_cut_never_leaves_half_a_flag() {
        // Two regional indicators name a country together and a letter
        // in a box apart — and each is one cell, so nothing but the
        // pairing rule keeps the cut outside them.
        let name = "xx\u{1f1e6}\u{1f1f7}yy";
        assert_eq!(truncate(name, 4, Glyphs::Ascii), "xx~ ");
        assert_eq!(truncate(name, 5, Glyphs::Ascii), "xx\u{1f1e6}\u{1f1f7}~");
        // A third indicator opens the next flag; it is not swallowed by
        // the first.
        assert_eq!(
            truncate("\u{1f1e6}\u{1f1f7}\u{1f1fa}\u{1f1fe}", 3, Glyphs::Ascii),
            "\u{1f1e6}\u{1f1f7}~"
        );
    }

    #[test]
    fn a_cluster_is_measured_as_the_terminal_draws_it() {
        // U+2600 is one cell as a character; the variation selector after
        // it asks for the emoji, which is drawn in two.
        assert_eq!(cell_width("\u{2600}\u{fe0f}"), 2);
        assert_eq!(
            cell_width(&truncate("\u{2600}\u{fe0f}ab", 3, Glyphs::Ascii)),
            3
        );
    }

    #[test]
    fn a_value_that_fits_is_kept_whole_and_padded_to_the_column() {
        let padded = truncate("plan", COLUMN, Glyphs::Unicode);
        assert_eq!(padded, "plan      ");
        assert_eq!(cell_width(&padded), COLUMN);
    }

    #[test]
    fn a_wrapped_line_never_takes_more_cells_than_its_column() {
        // Wide enough that every word fits in it, which is the case a
        // break between words covers.
        const PARAGRAPH: usize = 40;
        let tradeoff = "Uses one extra correction attempt beyond the declared \
                        max_reroutes (0); escalates again if `fix` doesn't fix it";
        let lines = wrap(tradeoff, PARAGRAPH);
        assert!(lines.len() > 1, "the line was too long to keep whole");
        for line in &lines {
            assert!(
                cell_width(line) <= PARAGRAPH,
                "{line:?} overflows the column"
            );
        }
        assert_eq!(
            lines.join(" "),
            tradeoff,
            "wrapping breaks the line and keeps the words"
        );
    }

    #[test]
    fn a_word_wider_than_the_column_breaks_inside_itself() {
        // Nothing else can be done with it, and leaving it whole is
        // leaving the terminal to wrap a line this surface then counts
        // as one.
        let lines = wrap("supercalifragilistic", 6);
        assert_eq!(lines, vec!["superc", "alifra", "gilist", "ic"]);
    }

    #[test]
    fn wrapping_measures_in_cells_and_breaks_between_clusters() {
        // Eight ideographs, two cells each.
        let lines = wrap("実装レビュー担当", COLUMN);
        assert_eq!(lines, vec!["実装レビュ", "ー担当"]);
        for line in &lines {
            assert!(cell_width(line) <= COLUMN);
        }
    }

    #[test]
    fn text_with_nothing_in_it_wraps_to_one_empty_line() {
        assert_eq!(wrap("", COLUMN), vec![String::new()]);
    }

    #[test]
    fn a_cut_value_closes_with_the_mark_of_its_glyph_set() {
        assert_eq!(truncate("verification", 6, Glyphs::Unicode), "verif…");
        assert_eq!(truncate("verification", 6, Glyphs::Ascii), "verif~");
    }
}
