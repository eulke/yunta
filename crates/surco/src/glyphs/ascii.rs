//! What a text becomes on a full-screen terminal drawn in the ASCII glyphs: ASCII alone, a
//! character at a time, taken before anything in it is measured.
//!
//! [`Glyphs::Ascii`] is for an environment that names no UTF-8 locale, where a terminal draws
//! a character outside ASCII as several cells of noise. Surco's own marks are ASCII there, but
//! the words a screen is handed are not Surco's: what a session said, what a pack wrote, a
//! path. One of them outside ASCII pushes its row past the screen's edge, and a grid of cells
//! no longer holds. So each character of a text takes a form:
//!
//! - a character Surco draws one of its glyphs with takes what Surco draws in its place in
//!   ASCII, read from [`Glyphs`] itself (`·` `;`, `…` `...`, `›` `::`, `→` `->`, `—` `--`);
//!   the en dash is `-`, and a curved quote a straight one;
//! - a Latin letter with a mark takes its letter (`ñ` `n`, `Ç` `C`), a ligature its two
//!   (`ß` `ss`, `æ` `ae`, `œ` `oe`), and `ø` `o`: every letter of the Latin-1 Supplement and
//!   of Latin Extended-A, written out here, with no table of a dependency's;
//! - any other character is `?`, one a character whatever cells it took: a wide character
//!   and an emoji take one cell where they took two, a combining mark one where it took none.
//!
//! A text takes its form before a row is measured, so a row wraps and is cut by what it draws
//! and ends at its width. A page a command prints never takes it, nor do Markdown and HTML: a
//! log or a pipe is often read later where UTF-8 shows, and a page never loses a character of
//! what was recorded. Not [`Fold`](crate::components::Fold), a section a reader opens.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::LazyLock;

use super::{Glyphs, Plow, ALL_MARKS};
use crate::diagram::{Shape, Stroke};

/// What a character with no form of its own is drawn as: one cell, whatever it took.
const NO_FORM: char = '?';

/// `text` in ASCII alone, each character in its form ([the module's rule](self)); `text`
/// itself where it is ASCII already.
pub(crate) fn in_ascii(text: &str) -> Cow<'_, str> {
    if text.is_ascii() {
        return Cow::Borrowed(text);
    }
    let mut ascii = String::with_capacity(text.len());
    for character in text.chars() {
        match (character.is_ascii(), form(character)) {
            (true, _) => ascii.push(character),
            (false, Some(form)) => ascii.push_str(form),
            (false, None) => ascii.push(NO_FORM),
        }
    }
    Cow::Owned(ascii)
}

/// The form of `character`, outside ASCII, where it has one: Surco's own, then a dash's or a
/// quote's, then a letter's.
fn form(character: char) -> Option<&'static str> {
    GLYPH_FORMS
        .get(&character)
        .map(String::as_str)
        .or_else(|| punctuation(character))
        .or_else(|| letter(character))
}

/// Each character Surco draws a glyph with in Unicode, and what it draws in its place in ASCII.
static GLYPH_FORMS: LazyLock<HashMap<char, String>> =
    LazyLock::new(|| glyph_forms().into_iter().collect());

/// Each character outside ASCII that Surco draws a glyph with in Unicode, beside what it
/// draws in that glyph's place in ASCII, glyph by glyph. A glyph of one character takes the
/// whole of its ASCII form (`…` `...`); one of several gives each its own where its ASCII form
/// has as many (`╱╱` `//`), and none where that form says it in words (`↑↓` `up/down`). A
/// branch of a list of nodes is left out: its corners are a joint's, which draws them `+`.
fn glyph_forms() -> Vec<(char, String)> {
    let both = |glyph: &dyn Fn(Glyphs) -> String| (glyph(Glyphs::Unicode), glyph(Glyphs::Ascii));
    let mut drawn = vec![
        both(&|glyphs| glyphs.bar_empty().to_string()),
        both(&|glyphs| glyphs.ellipsis().to_string()),
        both(&|glyphs| glyphs.times().to_string()),
        both(&|glyphs| glyphs.sep().to_string()),
        both(&|glyphs| glyphs.arrow().to_string()),
        both(&|glyphs| glyphs.pointer().to_string()),
        both(&|glyphs| glyphs.picked().to_string()),
        both(&|glyphs| glyphs.within().to_string()),
        both(&|glyphs| glyphs.joins().to_string()),
        both(&|glyphs| glyphs.continued().to_string()),
        both(&|glyphs| glyphs.gutter().to_string()),
        both(&|glyphs| glyphs.decision_edge().to_string()),
        both(&|glyphs| glyphs.returns().to_string()),
        both(&|glyphs| glyphs.changed().iter().collect()),
        both(&|glyphs| glyphs.head(true).to_string()),
        both(&|glyphs| glyphs.head(false).to_string()),
        both(&|glyphs| glyphs.ramp().iter().collect()),
        both(&|glyphs| glyphs.cut().to_string()),
    ];
    drawn.extend(ALL_MARKS.map(|mark| both(&|glyphs| glyphs.mark(mark).to_string())));
    drawn.extend(
        [Plow::Plowed, Plow::Ahead, Plow::Skipped]
            .map(|plow| both(&|glyphs| glyphs.furrow(plow).to_string())),
    );
    drawn.extend(
        [Shape::Box, Shape::Round, Shape::Decision]
            .map(|shape| both(&|glyphs| glyphs.corners(shape).iter().collect())),
    );
    let ways = [false, true];
    let strokes = [Stroke::Arrow, Stroke::Open, Stroke::Dotted, Stroke::Thick];
    for up in ways {
        for down in ways {
            for left in ways {
                for right in ways {
                    for stroke in strokes {
                        let joint = |glyphs: Glyphs| glyphs.joint(up, down, left, right, stroke);
                        drawn.push(both(&|glyphs| joint(glyphs).to_string()));
                    }
                }
            }
        }
    }
    drawn
        .iter()
        .flat_map(|(unicode, ascii)| each_in_its_place(unicode, ascii))
        .filter(|(character, _)| !character.is_ascii())
        .collect()
}

/// Each character of a glyph drawn `unicode` beside its form in a glyph drawn `ascii`: the
/// whole of `ascii` for a glyph of one character, the character in its place where both have
/// as many, and none otherwise.
fn each_in_its_place(unicode: &str, ascii: &str) -> Vec<(char, String)> {
    let unicode: Vec<char> = unicode.chars().collect();
    let in_ascii: Vec<char> = ascii.chars().collect();
    match unicode.as_slice() {
        [one] => vec![(*one, ascii.to_string())],
        several if several.len() == in_ascii.len() => several
            .iter()
            .zip(in_ascii)
            .map(|(character, form)| (*character, form.to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

/// The dashes and quotes Surco draws no glyph with: the en dash as a hyphen, and a curved quote
/// as a straight one. The em dash is Surco's own ([`Glyphs::joins`]).
fn punctuation(character: char) -> Option<&'static str> {
    match character {
        '\u{2013}' => Some("-"),
        '\u{2018}' | '\u{2019}' => Some("'"),
        '\u{201C}' | '\u{201D}' => Some("\""),
        _ => None,
    }
}

/// The letters a Latin letter with a mark is written with: its base letter, a ligature's two,
/// a letter English does not write as the ones it is read as (`þ` `th`); every letter of the
/// Latin-1 Supplement and of Latin Extended-A, and none of their signs.
fn letter(character: char) -> Option<&'static str> {
    LETTERS
        .iter()
        .find(|(letters, _)| letters.contains(character))
        .map(|(_, form)| *form)
}

/// Each form a Latin letter with a mark takes, beside the letters that take it.
const LETTERS: &[(&str, &str)] = &[
    ("ÀÁÂÃÄÅĀĂĄ", "A"),
    ("àáâãäåāăą", "a"),
    ("Æ", "AE"),
    ("æ", "ae"),
    ("ÇĆĈĊČ", "C"),
    ("çćĉċč", "c"),
    ("ÐĎĐ", "D"),
    ("ðďđ", "d"),
    ("ÈÉÊËĒĔĖĘĚ", "E"),
    ("èéêëēĕėęě", "e"),
    ("ĜĞĠĢ", "G"),
    ("ĝğġģ", "g"),
    ("ĤĦ", "H"),
    ("ĥħ", "h"),
    ("ÌÍÎÏĨĪĬĮİ", "I"),
    ("ìíîïĩīĭįı", "i"),
    ("Ĳ", "IJ"),
    ("ĳ", "ij"),
    ("Ĵ", "J"),
    ("ĵ", "j"),
    ("Ķ", "K"),
    ("ķĸ", "k"),
    ("ĹĻĽĿŁ", "L"),
    ("ĺļľŀł", "l"),
    ("ÑŃŅŇŊ", "N"),
    ("ñńņňŉŋ", "n"),
    ("ÒÓÔÕÖØŌŎŐ", "O"),
    ("òóôõöøōŏő", "o"),
    ("Œ", "OE"),
    ("œ", "oe"),
    ("ŔŖŘ", "R"),
    ("ŕŗř", "r"),
    ("ŚŜŞŠ", "S"),
    ("śŝşšſ", "s"),
    ("ß", "ss"),
    ("ŢŤŦ", "T"),
    ("ţťŧ", "t"),
    ("Þ", "TH"),
    ("þ", "th"),
    ("ÙÚÛÜŨŪŬŮŰŲ", "U"),
    ("ùúûüũūŭůűų", "u"),
    ("Ŵ", "W"),
    ("ŵ", "w"),
    ("ÝŶŸ", "Y"),
    ("ýÿŷ", "y"),
    ("ŹŻŽ", "Z"),
    ("źżž", "z"),
];

/// A value whose every text has an ASCII form: the same value with each text in it as
/// [`in_ascii`] writes it, and every other part as it was. Not [`Glyphs::Ascii`], the set
/// Surco's own marks are drawn with.
pub(crate) trait InAscii {
    fn in_ascii(&self) -> Self;
}

impl InAscii for String {
    fn in_ascii(&self) -> Self {
        in_ascii(self).into_owned()
    }
}

/// A word Yunta names in its own source stays as written: it is written in ASCII, which no
/// fold can give a text that lives as long as the program, so a word outside ASCII fails every
/// test build that draws it in the ASCII glyphs, where a release build draws it as written.
impl InAscii for &'static str {
    fn in_ascii(&self) -> Self {
        debug_assert!(
            self.is_ascii(),
            "a word named in Yunta's source is written in ASCII: {self}"
        );
        self
    }
}

/// A key: its form where that is one character, and `?` where it is none or several.
impl InAscii for char {
    fn in_ascii(&self) -> Self {
        let form = in_ascii(self.encode_utf8(&mut [0; 4])).into_owned();
        let mut characters = form.chars();
        match (characters.next(), characters.next()) {
            (Some(one), None) => one,
            _ => NO_FORM,
        }
    }
}

/// A path as a screen shows it.
impl InAscii for PathBuf {
    fn in_ascii(&self) -> Self {
        PathBuf::from(in_ascii(&self.to_string_lossy()).into_owned())
    }
}

impl<T: InAscii> InAscii for Option<T> {
    fn in_ascii(&self) -> Self {
        self.as_ref().map(T::in_ascii)
    }
}

impl<T: InAscii> InAscii for Vec<T> {
    fn in_ascii(&self) -> Self {
        self.iter().map(T::in_ascii).collect()
    }
}

impl<T: InAscii> InAscii for Box<T> {
    fn in_ascii(&self) -> Self {
        Box::new(self.as_ref().in_ascii())
    }
}

impl<A: InAscii, B: InAscii> InAscii for (A, B) {
    fn in_ascii(&self) -> Self {
        (self.0.in_ascii(), self.1.in_ascii())
    }
}

impl<K: InAscii + Ord, V: InAscii> InAscii for BTreeMap<K, V> {
    fn in_ascii(&self) -> Self {
        self.iter()
            .map(|(key, value)| (key.in_ascii(), value.in_ascii()))
            .collect()
    }
}

#[cfg(test)]
mod tests;
