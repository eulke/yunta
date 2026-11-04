//! A shell script read the way a shell splits it into commands. What
//! separates two commands — a newline, `;`, `&`, `&&`, `||`, and between
//! the commands of a pipeline `|` — separates them only outside quotes,
//! escapes, command substitutions and a heredoc's body. A `\` at the end
//! of a line joins it to the next, a comment ends with its line, and `&`
//! in a redirection (`2>&1`, `&>`) separates nothing.
//!
//! It stays a reading: what only running the script tells — an alias, a
//! function, a command built at runtime — it does not guess.

use std::iter::Peekable;
use std::str::Chars;

/// The pipelines of `script`, in order: what a newline, `;`, `&`, `&&` or
/// `||` separates, each trimmed, none empty.
pub fn pipelines(script: &str) -> Vec<String> {
    Scanner::new(script, Split::Pipelines).run()
}

/// The simple commands of `script`, in order: its pipelines, and within
/// each what `|` separates.
pub fn simple_commands(script: &str) -> Vec<String> {
    Scanner::new(script, Split::Commands).run()
}

/// The words of one simple command, as a shell hands them to its program: split
/// at blanks outside quotes, the quotes taken off, and a `\` keeping the sign
/// after it: outside quotes any sign, and inside double quotes the few a shell
/// escapes there. What only running it tells, a variable or a substitution, stays
/// as written.
pub fn words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut quote = Quote::None;
    let mut signs = command.chars();
    while let Some(sign) = signs.next() {
        match (quote, sign) {
            (Quote::None, ' ' | '\t' | '\n') => words.extend(word.take()),
            (Quote::None, '\'') => {
                quote = Quote::Single;
                word.get_or_insert_default();
            }
            (Quote::None, '"') => {
                quote = Quote::Double;
                word.get_or_insert_default();
            }
            (Quote::Single, '\'') | (Quote::Double, '"') => quote = Quote::None,
            (Quote::None, '\\') => word.get_or_insert_default().extend(signs.next()),
            (Quote::Double, '\\') => {
                let held = word.get_or_insert_default();
                match signs.next() {
                    Some(sign @ ('$' | '`' | '"' | '\\' | '\n')) => held.push(sign),
                    other => {
                        held.push('\\');
                        held.extend(other);
                    }
                }
            }
            (_, sign) => word.get_or_insert_default().push(sign),
        }
    }
    words.extend(word);
    words
}

/// The files one simple command sends its output to, in order: what `>` and `>>`
/// name outside quotes, glued to the words around them or not. A `2>` sends what
/// the command says of itself, and a `>&2` names a stream and no file, so neither
/// is here. What only running it tells, as in [`words`], stays as written.
pub fn output_files(command: &str) -> Vec<String> {
    let mut files = Vec::new();
    let mut quote = Quote::None;
    let mut signs = command.char_indices().peekable();
    while let Some((at, sign)) = signs.next() {
        match (quote, sign) {
            (Quote::None, '\'') => quote = Quote::Single,
            (Quote::None, '"') => quote = Quote::Double,
            (Quote::Single, '\'') | (Quote::Double, '"') => quote = Quote::None,
            (Quote::None | Quote::Double, '\\') => {
                signs.next();
            }
            (Quote::None, '>') => {
                signs.next_if(|(_, next)| *next == '>');
                let to_a_stream = signs.next_if(|(_, next)| *next == '&').is_some();
                let after = signs.peek().map_or(command.len(), |(next, _)| *next);
                let stream = stream_before(command.get(..at).unwrap_or_default());
                if !to_a_stream && matches!(stream, None | Some("1")) {
                    let rest = command.get(after..).unwrap_or_default();
                    files.extend(words(rest).into_iter().next());
                }
            }
            _ => {}
        }
    }
    files
}

/// The stream a redirection names: digits right before it, a word of their own.
fn stream_before(before: &str) -> Option<&str> {
    let word = before.rsplit(char::is_whitespace).next().unwrap_or(before);
    (!word.is_empty() && word.chars().all(|sign| sign.is_ascii_digit())).then_some(word)
}

/// Whether `|` separates what the scanner hands back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Split {
    Pipelines,
    Commands,
}

/// The quote the scanner is inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single,
    Double,
}

struct Scanner<'a> {
    chars: Peekable<Chars<'a>>,
    split: Split,
    current: String,
    commands: Vec<String>,
    quote: Quote,
    /// How deep inside `$( … )` the scanner is.
    nested: usize,
    /// Whether it is inside a backquoted substitution.
    backtick: bool,
    /// The delimiters of the heredocs the current line opens, whose
    /// bodies follow it.
    heredocs: Vec<String>,
}

impl<'a> Scanner<'a> {
    fn new(script: &'a str, split: Split) -> Self {
        Scanner {
            chars: script.chars().peekable(),
            split,
            current: String::new(),
            commands: Vec::new(),
            quote: Quote::None,
            nested: 0,
            backtick: false,
            heredocs: Vec::new(),
        }
    }

    fn run(mut self) -> Vec<String> {
        while let Some(c) = self.chars.next() {
            if !self.quoted(c) {
                self.unquoted(c);
            }
        }
        self.finish();
        self.commands
    }

    /// Takes `c` inside a quote, answering whether it was in one.
    fn quoted(&mut self, c: char) -> bool {
        match self.quote {
            Quote::None => return false,
            Quote::Single if c == '\'' => self.quote = Quote::None,
            Quote::Double if c == '"' => self.quote = Quote::None,
            Quote::Double if c == '\\' => {
                self.current.push(c);
                if let Some(escaped) = self.chars.next() {
                    self.current.push(escaped);
                }
                return true;
            }
            Quote::Single | Quote::Double => {}
        }
        self.current.push(c);
        true
    }

    /// Takes `c` outside any quote.
    fn unquoted(&mut self, c: char) {
        let next = self.chars.peek().copied();
        match c {
            '\\' => match self.chars.next() {
                Some('\n') => {}
                Some(escaped) => self.current.extend([c, escaped]),
                None => self.current.push(c),
            },
            '\'' | '"' => {
                self.quote = if c == '\'' {
                    Quote::Single
                } else {
                    Quote::Double
                };
                self.current.push(c);
            }
            '`' => {
                self.backtick = !self.backtick;
                self.current.push(c);
            }
            '$' if next == Some('(') => {
                self.chars.next();
                self.nested += 1;
                self.current.push_str("$(");
            }
            '(' if self.nested > 0 => {
                self.nested += 1;
                self.current.push(c);
            }
            ')' if self.nested > 0 => {
                self.nested -= 1;
                self.current.push(c);
            }
            _ if self.nested > 0 || self.backtick => self.current.push(c),
            '#' if self
                .current
                .chars()
                .next_back()
                .is_none_or(char::is_whitespace) =>
            {
                while self.chars.next_if(|next| *next != '\n').is_some() {}
            }
            '<' if next == Some('<') => self.heredoc(),
            _ => self.operator(c, next),
        }
    }

    /// Takes `c`, followed by `next`, where it may separate commands.
    fn operator(&mut self, c: char, next: Option<char>) {
        match c {
            '\n' => {
                self.finish();
                self.skip_heredoc_bodies();
            }
            ';' => self.finish(),
            '&' if next == Some('&') => {
                self.chars.next();
                self.finish();
            }
            '&' if next == Some('>') || self.current.ends_with(['>', '<']) => self.current.push(c),
            '&' => self.finish(),
            '|' if next == Some('|') => {
                self.chars.next();
                self.finish();
            }
            '|' if self.split == Split::Commands => {
                self.chars.next_if_eq(&'&');
                self.finish();
            }
            _ => self.current.push(c),
        }
    }

    /// Reads a `<<` the scanner just met, the second `<` still ahead: a
    /// here-string, or a heredoc whose delimiter it notes.
    fn heredoc(&mut self) {
        self.chars.next();
        self.current.push_str("<<");
        if self.chars.next_if_eq(&'<').is_some() {
            self.current.push('<');
            return;
        }
        if self.chars.next_if_eq(&'-').is_some() {
            self.current.push('-');
        }
        while let Some(space) = self.chars.next_if(|c| *c == ' ' || *c == '\t') {
            self.current.push(space);
        }
        let mut word = String::new();
        while let Some(c) = self
            .chars
            .next_if(|c| !c.is_whitespace() && !matches!(c, ';' | '&' | '|' | '<' | '>'))
        {
            word.push(c);
        }
        self.current.push_str(&word);
        let delimiter = word.replace(['\'', '"', '\\'], "");
        if !delimiter.is_empty() {
            self.heredocs.push(delimiter);
        }
    }

    /// Passes over the bodies of the heredocs the line just ended opened.
    fn skip_heredoc_bodies(&mut self) {
        for delimiter in std::mem::take(&mut self.heredocs) {
            loop {
                let line: String =
                    std::iter::from_fn(|| self.chars.next_if(|c| *c != '\n')).collect();
                let ended = self.chars.next().is_none();
                if line.trim() == delimiter || ended {
                    break;
                }
            }
        }
    }

    /// Ends the command being read.
    fn finish(&mut self) {
        let command = self.current.trim();
        if !command.is_empty() {
            self.commands.push(command.to_string());
        }
        self.current.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_separator_inside_quotes_or_a_substitution_separates_nothing() {
        assert_eq!(
            pipelines(
                "echo 'ready; cargo test -p x' && grep -q \"a || b\" f; v=$(git log | wc -l)"
            ),
            vec![
                "echo 'ready; cargo test -p x'",
                "grep -q \"a || b\" f",
                "v=$(git log | wc -l)"
            ]
        );
        assert_eq!(
            simple_commands("printf 'tasks:\n  - id: T001\n    scope: [a]\n' > out | tee x"),
            vec![
                "printf 'tasks:\n  - id: T001\n    scope: [a]\n' > out",
                "tee x"
            ]
        );
    }

    #[test]
    fn a_redirection_is_part_of_its_command() {
        assert_eq!(
            simple_commands("cargo test 2>&1 | tail -5 &> log & make >&2"),
            vec!["cargo test 2>&1", "tail -5 &> log", "make >&2"]
        );
    }

    #[test]
    fn output_files_are_what_a_shell_writes_to() {
        let table: &[(&str, &[&str])] = &[
            ("cat a > b", &["b"]),
            ("cat a >> b", &["b"]),
            ("echo x>b", &["b"]),
            ("echo x >b", &["b"]),
            ("x2>b", &["b"]),
            ("cat a 1> out", &["out"]),
            ("cmd &> all", &["all"]),
            ("cmd &>> all", &["all"]),
            ("cat a > 'my file'", &["my file"]),
            ("cat > a > b", &["a", "b"]),
            ("cat a", &[]),
            ("echo '>' a", &[]),
            ("echo \">\" a", &[]),
            ("echo \\> a", &[]),
            ("echo x >&2", &[]),
            ("echo x 1>&2", &[]),
            ("cat a 2> err", &[]),
            ("cat a 2>err", &[]),
            ("cat a 2>&1", &[]),
            ("echo x >", &[]),
        ];
        for (command, expected) in table {
            assert_eq!(output_files(command), *expected, "{command}");
        }
    }

    #[test]
    fn words_are_what_a_shell_hands_its_program() {
        let table: &[(&str, &[&str])] = &[
            ("cargo test --quiet", &["cargo", "test", "--quiet"]),
            ("bash -lc 'cd src && ls'", &["bash", "-lc", "cd src && ls"]),
            ("zsh -c \"cat \\\"a b\\\"\"", &["zsh", "-c", "cat \"a b\""]),
            ("cat \"a\\nb\"", &["cat", "a\\nb"]),
            ("a\\ b  c", &["a b", "c"]),
            ("FOO='' x", &["FOO=", "x"]),
            ("''", &[""]),
            ("   ", &[]),
        ];
        for (command, expected) in table {
            assert_eq!(words(command), *expected, "{command}");
        }
    }

    #[test]
    fn lines_comments_and_heredocs_read_as_a_shell_reads_them() {
        let script =
            "git push \\\n  origin main # push; then\ncat <<'EOF' > f\nrm -rf /; boom\nEOF\nnext";
        assert_eq!(
            simple_commands(script),
            vec!["git push   origin main", "cat <<'EOF' > f", "next"]
        );
        assert_eq!(simple_commands("cat <<< 'a; b'"), vec!["cat <<< 'a; b'"]);
    }
}
