use super::lines;
use crate::components::Code;
use crate::glyphs::Glyphs;
use crate::line::{Line, Syntax, Tone};

/// The spans of `line`, each as the text it says and the tone it is in, in
/// order.
fn spans(line: &Line) -> Vec<(&str, Tone)> {
    line.spans()
        .iter()
        .map(|span| (span.text.as_str(), span.tone))
        .collect()
}

/// A code's words are each drawn in the tone of what each is: a keyword and
/// a function's name in their own tones, and the signs and spaces around
/// them plain.
#[test]
fn a_code_s_words_take_the_tone_of_what_each_is() {
    let code = Code::whole("src/a.rs", None, "fn f() {}");
    let drawn = lines(&code, Glyphs::Unicode);
    assert_eq!(
        drawn.iter().map(spans).collect::<Vec<_>>(),
        [vec![
            ("fn", Tone::Code(Syntax::Keyword)),
            (" ", Tone::Plain),
            ("f", Tone::Code(Syntax::Function)),
            ("() {}", Tone::Plain),
        ]]
    );
}

/// A word a change marks is drawn in its change's tone, and the words
/// around it keep the tone of what each is.
#[test]
fn a_word_a_change_marks_keeps_its_change_s_tone_and_the_rest_its_own() {
    let code = Code::whole("src/a.rs", None, "fn f(a: u8) {}").against(Some("fn f() {}"));
    let drawn = lines(&code, Glyphs::Unicode);
    assert_eq!(drawn.len(), 1);
    let line = spans(&drawn[0]);
    let added: Vec<(&str, Tone)> = line
        .iter()
        .copied()
        .filter(|(_, tone)| *tone == Tone::Added)
        .collect();
    assert_eq!(added, [("{+a: u8+}", Tone::Added)]);
    assert!(
        line.contains(&("fn", Tone::Code(Syntax::Keyword))),
        "{line:?}"
    );
}

/// A string that runs over a line break is one literal, so each of its two
/// lines colors the part of it that stands there as text.
#[test]
fn a_string_over_two_lines_is_colored_on_both() {
    let code = Code::whole("src/a.rs", None, "let s = \"a\nb\";");
    let drawn = lines(&code, Glyphs::Unicode);
    let per_line: Vec<Vec<(&str, Tone)>> = drawn.iter().map(spans).collect();
    assert_eq!(per_line.len(), 2);
    assert_eq!(per_line[0].last(), Some(&("\"a", Tone::Code(Syntax::Text))));
    assert_eq!(
        per_line[1].first(),
        Some(&("b\"", Tone::Code(Syntax::Text)))
    );
}

/// A file whose extension names no language the lexer knows is drawn as its
/// text, word for word and all of it plain.
#[test]
fn a_file_of_no_language_is_drawn_plain() {
    let code = Code::whole("notes.txt", None, "fn f() {}");
    let drawn = lines(&code, Glyphs::Unicode);
    assert_eq!(
        drawn.iter().map(spans).collect::<Vec<_>>(),
        [vec![("fn f() {}", Tone::Plain)]]
    );
}
