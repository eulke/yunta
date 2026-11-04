use super::*;
use crate::paint::Paint;

#[test]
fn a_line_wrapped_breaks_between_words_and_keeps_their_tones() {
    let line = Line::new()
        .push(Tone::Strong, "lint")
        .plain(": ")
        .push(Tone::Failed, "exit 101 after the suite ran");
    let wrapped = line.wrap(14);
    let said: Vec<String> = wrapped.iter().map(Line::text).collect();
    assert_eq!(said, ["lint: exit 101", "after the", "suite ran"]);
    assert_eq!(wrapped[0].spans()[0].tone, Tone::Strong);
    assert_eq!(wrapped[0].spans()[1].text, ":");
    assert_eq!(wrapped[2].spans()[0].tone, Tone::Failed);
    assert_eq!(line.wrap(80), vec![line.clone()]);
}

#[test]
fn a_line_cut_to_fit_keeps_the_tone_of_every_span_it_keeps() {
    let line = Line::new()
        .push(Tone::NeedsYou, "needs you")
        .plain(": ")
        .push(Tone::Strong, "yunta resolve-gate T1Y0P5 <option>");
    let cut = line.cut(20, Glyphs::Ascii);
    assert_eq!(Paint::Plain.line(&cut), "needs you: yunta...");
    assert_eq!(cut.spans()[0], line.spans()[0]);
    assert_eq!(cut.spans().last().map(|span| span.tone), Some(Tone::Strong));
    assert_eq!(line.cut(80, Glyphs::Ascii), line);
}

#[test]
fn a_line_sliced_keeps_the_tone_of_every_span_it_cuts() {
    let line = Line::new()
        .plain("fn a(")
        .push(Tone::Added, "{+b: u8+}")
        .plain(");");
    let tones = |line: &Line| -> Vec<(String, Tone)> {
        line.spans()
            .iter()
            .map(|span| (span.text.clone(), span.tone))
            .collect()
    };
    assert_eq!(
        tones(&line.slice(3..9)),
        [
            ("a(".to_string(), Tone::Plain),
            ("{+b:".to_string(), Tone::Added)
        ]
    );
    assert_eq!(
        tones(&line.slice(10..16)),
        [
            ("u8+}".to_string(), Tone::Added),
            (");".to_string(), Tone::Plain)
        ]
    );
    assert_eq!(tones(&line.slice(6..8)), [("+b".to_string(), Tone::Added)]);
    assert_eq!(line.slice(0..16), line, "the whole line is the line");
    assert_eq!(line.slice(5..5), Line::new(), "nothing cut is no span");
}

#[test]
fn a_line_moved_under_a_margin_keeps_its_tones() {
    let line = Line::new().push(Tone::Failed, "exit 1").plain(" — lint");
    let moved = line.clone().under("  ");
    assert_eq!(Paint::Plain.line(&moved), "  exit 1 — lint");
    assert_eq!(&moved.spans()[1..], line.spans());
}

/// Each span's text and fill, in order: what the fill tests compare.
fn fills(line: &Line) -> Vec<(&str, Option<Fill>)> {
    line.spans()
        .iter()
        .map(|span| (span.text.as_str(), span.fill))
        .collect()
}

/// A wrapped line keeps the fill of each span on both of its pieces.
#[test]
fn a_wrapped_line_keeps_the_fill_of_each_span_on_both_pieces() {
    let line = Line::new()
        .plain("see ")
        .push_filled(Tone::Plain, Fill::Code, "cargo ")
        .push_filled(Tone::Plain, Fill::Code, "test --all");
    let wrapped = line.wrap(12);
    assert_eq!(
        wrapped.iter().map(fills).collect::<Vec<_>>(),
        [
            vec![("see", None), (" ", None), ("cargo", Some(Fill::Code))],
            vec![
                ("test", Some(Fill::Code)),
                (" ", Some(Fill::Code)),
                ("--all", Some(Fill::Code)),
            ],
        ]
    );
}

/// A space put back between two words keeps the fill of the space it replaces.
#[test]
fn a_space_put_back_keeps_the_fill_of_the_space_it_replaces() {
    let line = Line::new()
        .push_filled(Tone::Plain, Fill::Code, "cargo ")
        .plain("test much longer words here");
    assert_eq!(fills(&line.wrap(12)[0])[1], (" ", Some(Fill::Code)));
}

/// Where whitespace from two spans meets, the space put back takes the fill of the first.
#[test]
fn a_space_put_back_takes_the_fill_of_the_first_whitespace_it_replaces() {
    let line = Line::new()
        .push_filled(Tone::Plain, Fill::Code, "cargo ")
        .plain(" test much longer words here");
    assert_eq!(
        fills(&line.wrap(12)[0]),
        [
            ("cargo", Some(Fill::Code)),
            (" ", Some(Fill::Code)),
            ("test", None),
        ]
    );
}

/// A line's fill reaches every piece wrapped, cut or sliced from it.
#[test]
fn a_line_filled_as_a_whole_keeps_its_fill_in_every_piece() {
    let line = Line::new()
        .filled(Fill::Tint)
        .push_filled(Tone::Strong, Fill::Pill, "lint: exit 101")
        .plain(" after the suite ran");
    let pieces = line.wrap(14);
    assert_eq!(pieces.len(), 3);
    assert!(pieces.iter().all(|piece| piece.fill() == Some(Fill::Tint)));
    assert_eq!(line.cut(12, Glyphs::Ascii).fill(), Some(Fill::Tint));
    assert_eq!(line.slice(2..9).fill(), Some(Fill::Tint));
}

/// A line moved under a margin keeps its fill and its spans.
#[test]
fn a_line_moved_under_a_margin_keeps_its_fill() {
    let line = Line::new()
        .filled(Fill::Tint)
        .push_filled(Tone::Strong, Fill::Pill, "lint: exit 101")
        .plain(" after the suite ran");
    let moved = line.clone().under("  ");
    assert_eq!(moved.fill(), Some(Fill::Tint));
    assert_eq!(&moved.spans()[1..], line.spans());
}

/// The text of a line does not change when its fills are added.
#[test]
fn text_reads_the_same_with_and_without_fills() {
    let plain = Line::new().push(Tone::Failed, "exit 1").plain(": lint");
    let filled = Line::new()
        .filled(Fill::Tint)
        .push_filled(Tone::Failed, Fill::Pill, "exit 1")
        .plain(": lint");
    assert_eq!(filled.text(), plain.text());
    assert_eq!(filled.text(), "exit 1: lint");
}

/// A cut or sliced line keeps the fill of each span it keeps.
#[test]
fn a_cut_or_sliced_line_keeps_the_fill_of_each_span_it_keeps() {
    let line = Line::new()
        .push(Tone::NeedsYou, "needs you")
        .plain(": ")
        .push_filled(Tone::Strong, Fill::Pill, "yunta resolve-gate T1Y0P5");
    let cut = line.cut(20, Glyphs::Ascii);
    assert_eq!(
        fills(&cut),
        [
            ("needs you", None),
            (": ", None),
            ("yunta", Some(Fill::Pill)),
            ("...", Some(Fill::Pill)),
        ]
    );
    let sliced = line.slice(8..14);
    assert_eq!(
        fills(&sliced),
        [("u", None), (": ", None), ("yun", Some(Fill::Pill))]
    );
}

/// A cut that ends a span marks the ellipsis in that span, not the next one.
#[test]
fn a_cut_that_ends_a_span_marks_it_in_that_span() {
    let line =
        Line::new()
            .push(Tone::Strong, "abcde")
            .push_filled(Tone::Plain, Fill::Code, "fghij");
    let cut = line.cut(8, Glyphs::Ascii);
    let spans: Vec<_> = cut
        .spans()
        .iter()
        .map(|span| (span.text.as_str(), span.tone, span.fill))
        .collect();
    assert_eq!(
        spans,
        [("abcde", Tone::Strong, None), ("...", Tone::Strong, None)]
    );
}

/// Joining lines keeps this line's fill and moves the other's onto its spans.
#[test]
fn a_joined_line_keeps_its_fill_and_moves_the_other_fill_onto_its_spans() {
    let joined = Line::new()
        .push_filled(Tone::Added, Fill::Added, "+b")
        .then(Line::new().filled(Fill::Tint).plain("x"));
    assert_eq!(
        fills(&joined),
        [("+b", Some(Fill::Added)), ("x", Some(Fill::Tint))]
    );
    assert_eq!(joined.fill(), None);
    let filled = Line::new()
        .filled(Fill::Code)
        .plain("a")
        .then(Line::new().plain("b"));
    assert_eq!(filled.fill(), Some(Fill::Code));
    assert_eq!(fills(&filled), [("a", None), ("b", None)]);
}

/// `fill_tone` fills only the unfilled spans of its tone; the rest stay as they were.
#[test]
fn fill_tone_fills_only_the_spans_in_its_tone_that_have_none() {
    let line = Line::new()
        .push(Tone::Added, "{+a+}")
        .push_filled(Tone::Added, Fill::Pill, "{+b+}")
        .plain(" c")
        .filled(Fill::Tint)
        .fill_tone(Tone::Added, Fill::Added);
    assert_eq!(
        fills(&line),
        [
            ("{+a+}", Some(Fill::Added)),
            ("{+b+}", Some(Fill::Pill)),
            (" c", None),
        ]
    );
    assert_eq!(line.fill(), Some(Fill::Tint));
}

/// A path links to its file with every byte an address cannot hold written as
/// `%` and two hex digits: a space, a `#` or a `%` would end the address
/// early, and a control character would end the link. The name shown stays
/// as it is.
#[test]
fn a_path_links_to_its_file_with_what_an_address_cannot_hold_escaped() {
    let target = std::path::Path::new("/w/ñ a b#c%d\u{7}.rs");
    let line = Line::new().path("ñ a b#c%d.rs", target);
    let span = &line.spans()[0];
    assert_eq!(span.text, "ñ a b#c%d.rs");
    assert_eq!(
        span.link.as_deref(),
        Some("file:///w/%C3%B1%20a%20b%23c%25d%07.rs")
    );
}
