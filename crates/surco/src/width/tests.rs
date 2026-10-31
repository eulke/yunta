use super::*;

#[test]
fn a_path_wider_than_the_column_is_never_cut() {
    let path = "~/.yunta/worktrees/01K3W48MFW7H0ZZA5PZ07E5PH4/and/deeper";
    let lines = wrap_unbroken(&format!("off main, in {path}"), 30);
    assert_eq!(lines, vec!["off main, in", path]);
}

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
    // A room of three cells beside the cut mark holds one indicator
    // and not the pair, so the pair goes whole.
    let name = "xx\u{1f1e6}\u{1f1f7}yyyy";
    assert_eq!(truncate(name, 6, Glyphs::Ascii), "xx... ");
    assert_eq!(truncate(name, 7, Glyphs::Ascii), "xx\u{1f1e6}\u{1f1f7}...");
    // A third indicator opens the next flag; it is not swallowed by
    // the first.
    assert_eq!(
        truncate(
            "\u{1f1e6}\u{1f1f7}\u{1f1fa}\u{1f1fe}\u{1f1fa}\u{1f1fe}\u{1f1fa}\u{1f1fe}",
            6,
            Glyphs::Ascii
        ),
        "\u{1f1e6}\u{1f1f7}... "
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
    assert_eq!(truncate("verification", 6, Glyphs::Ascii), "ver...");
}

#[test]
fn a_node_column_is_as_wide_as_its_longest_id() {
    assert_eq!(id_column(["plan", "review@reviewer-alt", "lint"]), 19);
    assert_eq!(id_column([]), 0);
}

#[test]
fn two_fan_out_ids_cut_to_a_column_stay_distinct() {
    let one = middle_cut("review@claude-code", 12, Glyphs::Unicode);
    let other = middle_cut("review@codex-cli-x", 12, Glyphs::Unicode);
    assert_eq!(cell_width(&one), 12);
    assert_ne!(one, other, "{one} / {other}");
    assert!(one.starts_with("revie") && one.ends_with("e-code"), "{one}");
    assert_eq!(middle_cut("lint", 6, Glyphs::Ascii), "lint  ");
}

#[test]
fn packed_items_break_between_two_and_never_inside_one() {
    assert_eq!(
        packed(
            &[
                "did you mean `prompt`?",
                "`yunta schema workflow` lists every key"
            ],
            ';',
            40
        ),
        [
            "did you mean `prompt`? ;",
            "`yunta schema workflow` lists every key"
        ]
    );
    assert_eq!(packed(&["git", "checkouts"], ';', 40), ["git ; checkouts"]);
}

#[test]
fn the_separator_after_an_item_wrapped_ends_its_last_line_and_never_takes_one_alone() {
    assert_eq!(
        packed(&["a node names its runner with `runner:`", "x"], ';', 38),
        ["a node names its runner with", "`runner:` ;", "x"]
    );
    let filled = packed(&["aaaaaaaaa bbbbbbbbbb", "x"], ';', 20);
    assert_eq!(filled, ["aaaaaaaaa", "bbbbbbbbbb ;", "x"]);
    assert!(filled.iter().all(|line| line.trim() != ";"), "{filled:?}");
}

#[test]
fn an_item_wider_than_the_room_is_wrapped_on_lines_of_its_own() {
    assert_eq!(
        packed(
            &["git", "a node that declares questions asks them"],
            ';',
            20
        ),
        ["git ;", "a node that declares", "questions asks them"]
    );
}

/// Keys a prompt names, a comma after each but the last, broken between
/// two keys when the line has no room for the next.
#[test]
fn items_joined_by_a_separator_break_only_between_two_items() {
    assert_eq!(
        joined(
            &[
                "arrows move",
                "type to filter",
                "enter chooses",
                "esc parks the run"
            ],
            ",",
            60
        ),
        [
            "arrows move, type to filter, enter chooses,",
            "esc parks the run"
        ]
    );
}
