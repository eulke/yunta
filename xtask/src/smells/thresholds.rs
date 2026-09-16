//! Reading a number a source file fixes, and the decision behind it.
//!
//! A threshold is a choice about behaviour — how long a wait may take,
//! how deep a queue is, how many samples an estimate needs — and the
//! number alone says neither who chose it nor what it trades away. The
//! rustdoc of such a constant cites the decision that fixes it, by its
//! `Dnnn`, so changing the number is revising that decision rather than
//! making a second choice alone.

/// The integer types a threshold is written in.
const INT_TYPES: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
];

/// How many integer constants in `source` fix their number here and
/// cite no decision. A constant whose value names another constant takes
/// its number from there, so the decision is the one that fixed it.
pub fn numeric_consts_without_decision(source: &str) -> usize {
    let lines: Vec<&str> = source.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| declares_numeric_const(line))
        .filter(|(at, _)| !cites_a_decision(&lines[..*at]))
        .count()
}

/// Whether the rustdoc block ending at `above`'s last line cites a
/// decision. The block is the run of `///` lines and attributes directly
/// over the declaration; a blank line or code ends it.
fn cites_a_decision(above: &[&str]) -> bool {
    above
        .iter()
        .rev()
        .take_while(|line| {
            let text = line.trim_start();
            text.starts_with("///") || text.starts_with("#[")
        })
        .any(|line| {
            line.as_bytes()
                .windows(2)
                .any(|pair| pair[0] == b'D' && pair[1].is_ascii_digit())
        })
}

/// Whether `line` declares an integer constant whose number is fixed
/// here: `const NAME: usize = 3;`, with or without a visibility, and
/// with or without the digit separators and the type suffix a literal
/// may carry. Arithmetic over literals alone — `1024 * 1024` — fixes its
/// number here too, and is read as one. A value naming another constant
/// is not a number declared here: the decision is the one that fixed
/// that constant.
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
    fn a_numeric_const_counts_until_its_rustdoc_cites_a_decision() {
        let cited = "/// How long a wait may take (D170).\nconst WAIT: u64 = 10;\n";
        assert_eq!(numeric_consts_without_decision(cited), 0);
        let bare = "/// How long a wait may take.\nconst WAIT: u64 = 10;\n";
        assert_eq!(numeric_consts_without_decision(bare), 1);
        // A visibility, digit separators and a suffix are the same
        // declaration; a value naming another constant is not a number
        // declared here, and neither is a string.
        assert_eq!(
            numeric_consts_without_decision("pub(crate) const N: usize = 1_024usize;\n"),
            1
        );
        // Arithmetic over literals alone fixes its number here; one
        // term naming another constant takes it from there.
        assert_eq!(
            numeric_consts_without_decision("const N: usize = 1024 * 1024;\n"),
            1
        );
        assert_eq!(
            numeric_consts_without_decision("const N: usize = OTHER + 1;\n"),
            0
        );
        assert_eq!(
            numeric_consts_without_decision("const NAME: &str = \"3\";\n"),
            0
        );
    }
}
