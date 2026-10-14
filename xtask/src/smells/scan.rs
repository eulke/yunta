//! Blanking what a scan reads: a string literal and a comment are blanked, so
//! only the structural characters of a line survive, and a brace inside a
//! literal never opens or closes a body. A deliberate lexer rather than a
//! parser: a literal or a block comment that runs over lines carries its state
//! onto the next line.

/// Every line of `source` with its string literals and comments blanked,
/// so only structural braces survive. A literal or a block comment that
/// runs over several lines keeps the scan inside it on the next line.
pub fn blanked_lines(source: &str) -> Vec<String> {
    let mut state = Lexing::Code;
    source
        .lines()
        .map(|line| blank_line(line, &mut state))
        .collect()
}

/// What the scan is inside when a line begins. A literal or a block comment
/// can run over lines, so the state outlives the line that opened it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lexing {
    /// Code: a `//` here ends the line, and a quote or `/*` opens a literal
    /// or a comment.
    Code,
    /// Inside a block comment, this many `/*` deep: Rust nests them.
    Block(usize),
    /// Inside a normal or byte string literal, where an escape skips a character.
    Str,
    /// Inside a raw string literal, closed by a quote and this many hashes.
    Raw(usize),
}

/// One line blanked from the state the scan is in at its start. The state
/// it ends in is written back, so the next line starts where this one stops.
fn blank_line(line: &str, state: &mut Lexing) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut at = 0;
    while at < chars.len() {
        let current = *state;
        at += match current {
            Lexing::Code => code(&chars, at, state, &mut out),
            Lexing::Block(depth) => block(&chars, at, depth, state, &mut out),
            Lexing::Str => string(&chars, at, state, &mut out),
            Lexing::Raw(hashes) => raw(&chars, at, hashes, state, &mut out),
        };
    }
    out
}

/// Code: a `//` ends the line and is dropped, a `/*` opens a comment, a quote
/// opens a string, and an `r` or `br` that opens a raw string takes its
/// hashes. Returns how many characters it read.
fn code(chars: &[char], at: usize, state: &mut Lexing, out: &mut String) -> usize {
    let next = chars.get(at + 1).copied();
    match chars[at] {
        '/' if next == Some('/') => chars.len() - at,
        '/' if next == Some('*') => {
            *state = Lexing::Block(1);
            spaces(out, 2)
        }
        '"' => {
            *state = Lexing::Str;
            spaces(out, 1)
        }
        '\'' => char_literal(chars, at, out),
        'r' | 'b' => match raw_opener(chars, at) {
            Some((hashes, opener)) => {
                *state = Lexing::Raw(hashes);
                spaces(out, opener)
            }
            None => plain(out, chars[at]),
        },
        other => plain(out, other),
    }
}

/// A char literal (`'x'`, `'\n'`, `'\u{..}'`) is blanked whole; any other
/// quote is a lifetime, which stays code.
fn char_literal(chars: &[char], at: usize, out: &mut String) -> usize {
    let is_char = chars.get(at + 1) == Some(&'\\') || chars.get(at + 2) == Some(&'\'');
    if !is_char {
        return plain(out, '\'');
    }
    let mut used = spaces(out, 1);
    loop {
        match chars.get(at + used).copied() {
            None => return used,
            Some('\'') => return used + spaces(out, 1),
            Some('\\') => used += spaces(out, 2_usize.min(chars.len() - (at + used))),
            Some(_) => used += spaces(out, 1),
        }
    }
}

/// Inside a string literal, an escape blanks the character it carries, and
/// the closing quote returns the scan to code.
fn string(chars: &[char], at: usize, state: &mut Lexing, out: &mut String) -> usize {
    match chars[at] {
        '\\' => spaces(out, 2_usize.min(chars.len() - at)),
        '"' => {
            *state = Lexing::Code;
            spaces(out, 1)
        }
        _ => spaces(out, 1),
    }
}

/// Inside a raw string literal nothing escapes; the literal closes at a quote
/// followed by as many hashes as its opener had.
fn raw(chars: &[char], at: usize, hashes: usize, state: &mut Lexing, out: &mut String) -> usize {
    let closes = chars[at] == '"' && (1..=hashes).all(|k| chars.get(at + k) == Some(&'#'));
    if closes {
        *state = Lexing::Code;
        spaces(out, 1 + hashes)
    } else {
        spaces(out, 1)
    }
}

/// Inside a block comment, a `/*` nests one level deeper and a `*/` closes one.
fn block(chars: &[char], at: usize, depth: usize, state: &mut Lexing, out: &mut String) -> usize {
    match (chars[at], chars.get(at + 1).copied()) {
        ('/', Some('*')) => {
            *state = Lexing::Block(depth + 1);
            spaces(out, 2)
        }
        ('*', Some('/')) => {
            *state = if depth > 1 {
                Lexing::Block(depth - 1)
            } else {
                Lexing::Code
            };
            spaces(out, 2)
        }
        _ => spaces(out, 1),
    }
}

/// The hashes and the length of the opener of a raw string that begins at
/// `at`: an `r` or `br` that starts a word, then hashes and a quote.
fn raw_opener(chars: &[char], at: usize) -> Option<(usize, usize)> {
    let prefix = match (chars[at], chars.get(at + 1).copied()) {
        ('r', _) => 1,
        ('b', Some('r')) => 2,
        _ => return None,
    };
    let joins_word = at > 0 && (chars[at - 1].is_alphanumeric() || chars[at - 1] == '_');
    if joins_word {
        return None;
    }
    let hashes = chars[at + prefix..]
        .iter()
        .take_while(|&&c| c == '#')
        .count();
    (chars.get(at + prefix + hashes) == Some(&'"')).then_some((hashes, prefix + hashes + 1))
}

/// One character, kept as it is.
fn plain(out: &mut String, c: char) -> usize {
    out.push(c);
    1
}

/// `count` blanks in place of the text they cover.
fn spaces(out: &mut String, count: usize) -> usize {
    out.extend(std::iter::repeat_n(' ', count));
    count
}

#[cfg(test)]
mod tests {
    use crate::smells::shape::functions_over;

    #[test]
    fn a_raw_string_holding_a_block_comment_opener_hides_no_function() {
        // The quote inside the literal does not close it: only a quote with
        // the opener's one hash does.
        let source = format!(
            "const GLOBS: &str = r#\"\nsrc/** \"quoted\"\n\"#;\nfn long() {{\n{}}}\n",
            "    let step = 1;\n".repeat(52)
        );
        assert_eq!(functions_over(&source, 50), 1);
    }

    #[test]
    fn a_multi_line_string_holding_a_block_comment_opener_hides_no_function() {
        let source = format!(
            "const TASKS: &str = \"\nsrc/**\n\";\nfn long() {{\n{}}}\n",
            "    let step = 1;\n".repeat(52)
        );
        assert_eq!(functions_over(&source, 50), 1);
    }

    #[test]
    fn a_char_literal_holding_a_quote_opens_no_string() {
        let source = format!(
            "fn quote() -> char {{\n    '\"'\n}}\nfn long() {{\n{}}}\n",
            "    let step = 1;\n".repeat(52)
        );
        assert_eq!(functions_over(&source, 50), 1);
    }

    #[test]
    fn a_byte_string_holding_a_glob_hides_no_function() {
        // The escaped quote is a character of the literal, not its end.
        let source = format!(
            "const GLOBS: &[u8] = b\"src/** \\\"quoted\";\nfn long() {{\n{}}}\n",
            "    let step = 1;\n".repeat(52)
        );
        assert_eq!(functions_over(&source, 50), 1);
    }

    #[test]
    fn a_byte_raw_string_holding_a_glob_hides_no_function() {
        // The quotes inside are text: only a quote with the opener's one hash
        // ends the literal.
        let source = format!(
            "const GLOBS: &[u8] = br#\"src/** \"quoted\"#;\nfn long() {{\n{}}}\n",
            "    let step = 1;\n".repeat(52)
        );
        assert_eq!(functions_over(&source, 50), 1);
    }

    #[test]
    fn a_one_line_raw_string_holding_a_block_comment_opener_hides_no_function() {
        // The quote that precedes `open` is text, so the literal ends at the hash.
        let source = format!(
            "const OPENER: &str = r#\"/* \"open\"#;\nfn long() {{\n{}}}\n",
            "    let step = 1;\n".repeat(52)
        );
        assert_eq!(functions_over(&source, 50), 1);
    }
}
