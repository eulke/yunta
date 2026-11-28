use super::*;

#[test]
fn a_character_surco_draws_a_glyph_with_takes_its_ascii_form() {
    assert_eq!(in_ascii("Not logged in · Please"), "Not logged in ; Please");
    assert_eq!(in_ascii("a … b"), "a ... b");
    assert_eq!(in_ascii("yunta › Q78GRR"), "yunta :: Q78GRR");
    assert_eq!(in_ascii("plan → build"), "plan -> build");
    assert_eq!(in_ascii("built — its behavior"), "built -- its behavior");
    assert_eq!(in_ascii("✓ done ◆ asks"), "+ done ? asks");
}

/// Every glyph Surco draws a character with in Unicode gives that character one form, the one
/// it draws in ASCII: a glyph read twice, as a joint's corner and a box's, reads the same.
#[test]
fn each_glyph_character_has_one_form_and_it_is_ascii() {
    let mut seen: HashMap<char, String> = HashMap::new();
    for (character, form) in glyph_forms() {
        assert!(form.is_ascii(), "{character} takes {form}");
        if let Some(earlier) = seen.insert(character, form.clone()) {
            assert_eq!(earlier, form, "{character} takes two forms");
        }
    }
    assert_eq!(seen.get(&Glyphs::Unicode.sep()), Some(&";".to_string()));
    assert_eq!(seen.get(&'—'), Some(&Glyphs::Ascii.joins().to_string()));
}

#[test]
fn a_dash_and_a_curved_quote_take_their_plain_forms() {
    assert_eq!(in_ascii("pages 3–5"), "pages 3-5");
    assert_eq!(in_ascii("it’s ‘said’"), "it's 'said'");
    assert_eq!(in_ascii("“quoted”"), "\"quoted\"");
}

#[test]
fn a_latin_letter_with_a_mark_takes_its_letter() {
    assert_eq!(in_ascii("diseño — más"), "diseno -- mas");
    assert_eq!(in_ascii("ñ á ü ç Ñ"), "n a u c N");
    assert_eq!(in_ascii("Straße"), "Strasse");
    assert_eq!(in_ascii("æble Œuvre"), "aeble OEuvre");
    assert_eq!(in_ascii("søster Łódź"), "soster Lodz");
}

/// Every letter of the Latin-1 Supplement and of Latin Extended-A takes letters of its own case
/// alone, held against Unicode's own reading of each letter, apart from the table.
#[test]
fn every_latin_letter_of_the_two_blocks_takes_letters_of_its_case() {
    let letters = ('\u{C0}'..='\u{17F}').filter(|character| character.is_alphabetic());
    let mut read = 0;
    for character in letters {
        let form = letter(character).unwrap_or_else(|| panic!("{character} has no letters"));
        assert!(
            !form.is_empty() && form.chars().all(|c| c.is_ascii_alphabetic()),
            "{character} takes {form}"
        );
        if character.is_uppercase() {
            assert!(
                form.chars().all(|c| c.is_ascii_uppercase()),
                "{character} takes {form}"
            );
        }
        if character.is_lowercase() {
            assert!(
                form.chars().all(|c| c.is_ascii_lowercase()),
                "{character} takes {form}"
            );
        }
        read += 1;
    }
    // The two blocks hold 192 characters, the multiplication and the division signs among them.
    assert_eq!(read, 190);
    assert_eq!(in_ascii("÷"), "?");
}

/// A character with no form is `?`, one a character whatever cells it took: a wide character
/// and an emoji one cell where they took two, a combining mark one where it took none.
#[test]
fn a_character_with_no_form_is_one_question_mark() {
    assert_eq!(in_ascii("漢字"), "??");
    assert_eq!(in_ascii("ship 🚀"), "ship ?");
    assert_eq!(in_ascii("cafe\u{301}"), "cafe?");
    assert_eq!(in_ascii("\u{2800}"), "?");
}

#[test]
fn ascii_stays_as_it_is_and_is_not_copied() {
    let text = "plain words; `code` -- 100%";
    assert!(matches!(in_ascii(text), Cow::Borrowed(borrowed) if borrowed == text));
}

#[test]
fn a_key_takes_its_form_where_that_is_one_character() {
    assert_eq!('a'.in_ascii(), 'a');
    assert_eq!('ñ'.in_ascii(), 'n');
    assert_eq!('…'.in_ascii(), '?');
    assert_eq!('漢'.in_ascii(), '?');
}
