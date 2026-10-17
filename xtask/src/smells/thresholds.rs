//! Reading a number a source file fixes, and the reason written beside it.
//!
//! A threshold is a choice about behaviour — how long a wait may take,
//! how deep a queue is, how many samples an estimate needs — and the
//! number alone says neither why it has that value nor what it trades
//! away. The rustdoc of such a constant states that reason in prose, so
//! a reader finds it beside the number, and changing the number means
//! revising the reason rather than making a second choice alone.

/// The integer types a threshold is written in.
const INT_TYPES: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
];

/// How many integer constants in `source` fix their number here and state
/// no reason for it. A constant whose value names another constant takes
/// its number from there, so the reason belongs to that constant.
pub fn numeric_consts_without_reason(source: &str) -> usize {
    let lines: Vec<&str> = source.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| declares_numeric_const(line))
        .filter(|(at, _)| !states_a_reason(&lines[..*at]))
        .count()
}

/// Whether the rustdoc above a constant has a `///` line of three or more
/// prose words: letters only, the punctuation around them ignored. The
/// block is the run of comment and attribute lines directly over the
/// declaration; a blank line or code ends it.
fn states_a_reason(above: &[&str]) -> bool {
    above
        .iter()
        .rev()
        .take_while(|line| line.trim_start().starts_with(['/', '#']))
        .filter(|line| line.trim_start().starts_with("///"))
        .any(|line| {
            line.trim_start()
                .trim_start_matches('/')
                .split_whitespace()
                .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric()))
                .filter(|word| word.len() >= 2 && word.chars().all(char::is_alphabetic))
                .count()
                >= 3
        })
}

/// Whether `line` declares an integer constant whose number is fixed
/// here: `const NAME: usize = 3;`, with or without a visibility, and
/// with or without the digit separators and the type suffix a literal
/// may carry. Arithmetic over literals alone — `1024 * 1024` — fixes its
/// number here too, and is read as one. A value naming another constant
/// is not a number declared here: the reason belongs to that constant.
fn declares_numeric_const(line: &str) -> bool {
    let mut code = line.trim();
    if let Some(rest) = code.strip_prefix("pub") {
        // `pub`, `pub(crate)` and `pub(super)` all end at the next space.
        code = rest
            .split_once(' ')
            .map_or("", |(_, tail)| tail)
            .trim_start();
    }
    let Some(declaration) = code.strip_prefix("const ") else {
        return false;
    };
    let Some((_name, typed)) = declaration.split_once(':') else {
        return false;
    };
    let Some((type_name, value)) = typed.split_once('=') else {
        return false;
    };
    if !INT_TYPES.contains(&type_name.trim()) {
        return false;
    }
    value
        .trim()
        .strip_suffix(';')
        .is_some_and(|value| is_integer_literal(value.trim()))
}

/// Whether `value` fixes a number here: one integer literal, or
/// arithmetic over literals and nothing else.
fn is_integer_literal(value: &str) -> bool {
    let terms: Vec<&str> = value
        .split(['+', '-', '*', '/', '(', ')'])
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .collect();
    !terms.is_empty() && terms.iter().all(|term| is_one_literal(term))
}

/// Whether `term` is one integer literal: digits, the `_` a reader
/// groups them with, and an optional type suffix.
fn is_one_literal(term: &str) -> bool {
    let digits = INT_TYPES
        .iter()
        .find_map(|suffix| term.strip_suffix(suffix))
        .unwrap_or(term);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_numeric_const_counts_until_its_rustdoc_states_a_reason() {
        // A word with a digit in it is an identifier, not prose, so it
        // states no reason on its own.
        assert_eq!(
            numeric_consts_without_reason("/// ED25519 signs it.\nconst WAIT: u64 = 10;\n"),
            1
        );
        assert_eq!(
            numeric_consts_without_reason("/// (ID2)\nconst WAIT: u64 = 10;\n"),
            1
        );
        assert_eq!(
            numeric_consts_without_reason("/// How long a wait may take.\nconst WAIT: u64 = 10;\n"),
            0
        );
        assert_eq!(numeric_consts_without_reason("const WAIT: u64 = 10;\n"), 1);
        // A visibility, digit separators and a suffix are the same
        // declaration; a value naming another constant is not a number
        // declared here, and neither is a string.
        assert_eq!(
            numeric_consts_without_reason("pub(crate) const N: usize = 1_024usize;\n"),
            1
        );
        // Arithmetic over literals alone fixes its number here; one
        // term naming another constant takes it from there.
        assert_eq!(
            numeric_consts_without_reason("const N: usize = 1024 * 1024;\n"),
            1
        );
        assert_eq!(
            numeric_consts_without_reason("const N: usize = OTHER + 1;\n"),
            0
        );
        assert_eq!(
            numeric_consts_without_reason("const NAME: &str = \"3\";\n"),
            0
        );
    }
}
