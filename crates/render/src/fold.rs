//! A test file as a review reads it: each test whole, and each function
//! that only sets one up folded to its signature and how long it is. The
//! test is what a reader judges; a helper is a name it calls, and its
//! body is one command away.

use std::path::Path;

/// `code`, the file at `path`, with each function that is not a test
/// folded to `signature { ... N lines }`. A file in a language this does
/// not read comes back whole.
pub(crate) fn folded(path: &str, code: &str) -> String {
    let extension = Path::new(path).extension().and_then(|ext| ext.to_str());
    match extension {
        Some("rs") => braced(code, Language::Rust),
        Some("go") => braced(code, Language::Go),
        Some("js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx") => braced(code, Language::Script),
        Some("py") => indented(code),
        _ => code.to_string(),
    }
}

/// A language whose blocks are braces, by how its source says what is
/// code and what is a string or a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Language {
    Rust,
    Go,
    Script,
}

/// What a function header is to a review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Header {
    /// A test, shown whole.
    Test,
    /// A function that sets a test up, folded.
    Helper,
}

/// What `line` opens in `language`, read with the attributes above it.
fn header(language: Language, line: &str, above: &[&str]) -> Option<Header> {
    let line = line.trim_start();
    match language {
        Language::Rust => rust_header(line, above),
        Language::Go => go_header(line),
        Language::Script => script_header(line),
    }
}

/// A Rust `fn`: a test when an attribute above it names one.
fn rust_header(line: &str, above: &[&str]) -> Option<Header> {
    if !strip_qualifiers(line).starts_with("fn ") {
        return None;
    }
    let tested = above
        .iter()
        .rev()
        .map(|line| line.trim())
        .take_while(|line| line.starts_with("#[") || line.starts_with("///"))
        .any(|attribute| {
            attribute.starts_with("#[")
                && !attribute.starts_with("#[cfg(")
                && attribute.contains("test")
        });
    Some(if tested { Header::Test } else { Header::Helper })
}

/// A Go `func`: a test when its name says the runner calls it.
fn go_header(line: &str) -> Option<Header> {
    let rest = line.strip_prefix("func ")?;
    let name = match rest.strip_prefix('(') {
        Some(receiver) => receiver.split_once(')')?.1.trim_start(),
        None => rest,
    };
    let tested = ["Test", "Benchmark", "Example", "Fuzz"]
        .iter()
        .any(|prefix| name.starts_with(prefix));
    Some(if tested { Header::Test } else { Header::Helper })
}

/// A script's test call, or a function it declares or binds to a name.
fn script_header(line: &str) -> Option<Header> {
    if [
        "test(",
        "it(",
        "test.each",
        "it.each",
        "test.only(",
        "it.only(",
    ]
    .iter()
    .any(|call| line.starts_with(call))
    {
        return Some(Header::Test);
    }
    let line = line
        .trim_start_matches("export ")
        .trim_start_matches("default ")
        .trim_start_matches("async ");
    let function = line.starts_with("function ") || line.starts_with("function*");
    let bound = ["const ", "let ", "var "]
        .iter()
        .any(|binding| line.starts_with(binding))
        && line.contains('=')
        && (line.contains("=>") || line.contains("function"));
    (function || bound).then_some(Header::Helper)
}

/// `line` without the qualifiers a Rust item may open with.
fn strip_qualifiers(line: &str) -> &str {
    let mut line = line;
    loop {
        let before = line;
        for qualifier in [
            "pub(crate) ",
            "pub(super) ",
            "pub ",
            "async ",
            "const ",
            "unsafe ",
        ] {
            line = line.strip_prefix(qualifier).unwrap_or(line);
        }
        if let Some(rest) = line.strip_prefix("extern \"C\" ") {
            line = rest;
        }
        if line == before {
            return line;
        }
    }
}

/// `code` with each helper's body folded, in a language of braces.
fn braced(code: &str, language: Language) -> String {
    let lines: Vec<&str> = code.lines().collect();
    let braces = braces(&lines, language);
    let mut out: Vec<String> = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let found = header(language, lines[at], &lines[..at])
            .and_then(|kind| Some((kind, body(&lines, &braces, at)?)));
        let Some((kind, (open, close))) = found else {
            out.push(lines[at].to_string());
            at += 1;
            continue;
        };
        // The lines between the braces: one is the whole function, and
        // folding it would hide what it says to save nothing.
        let hidden = (close.0 - open.0).saturating_sub(1);
        if kind == Header::Test || hidden < 2 {
            out.extend(lines[at..=close.0].iter().map(|line| line.to_string()));
        } else {
            out.extend(lines[at..open.0].iter().map(|line| line.to_string()));
            let head = &lines[open.0][..open.1];
            let tail = &lines[close.0][close.1 + 1..];
            out.push(format!(
                "{head}{{ ... {} }}{tail}",
                yunta_core::text::counted(hidden, "line")
            ));
        }
        at = close.0 + 1;
    }
    let mut folded = out.join("\n");
    if code.ends_with('\n') {
        folded.push('\n');
    }
    folded
}

/// A place in a file: its line, and the byte in it.
type At = (usize, usize);

/// Where the body of the function whose header is on line `from` opens
/// and closes; `None` for a declaration with no body — a signature that
/// ends in `;` before any brace opens.
fn body(lines: &[&str], braces: &[Vec<(usize, char)>], from: usize) -> Option<(At, At)> {
    let mut open = None;
    let mut depth = 0usize;
    for (line, found) in braces.iter().enumerate().skip(from) {
        for &(byte, brace) in found {
            match (brace, open) {
                ('{', None) => {
                    open = Some((line, byte));
                    depth = 1;
                }
                ('{', Some(_)) => depth += 1,
                ('}', Some(opened)) => {
                    depth -= 1;
                    if depth == 0 {
                        return Some((opened, (line, byte)));
                    }
                }
                _ => {}
            }
        }
        if open.is_none() && lines[line].trim_end().ends_with(';') {
            return None;
        }
    }
    None
}

/// Each line's braces that are code — not inside a string, a character,
/// or a comment — as (byte, brace) pairs.
fn braces(lines: &[&str], language: Language) -> Vec<Vec<(usize, char)>> {
    let mut state = Lexed::Code;
    lines
        .iter()
        .map(|line| {
            let (found, after) = scan(line, language, state);
            state = after;
            found
        })
        .collect()
}

/// Where the lexer stands at the end of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lexed {
    Code,
    /// Inside a block comment, nested this deep.
    Comment(usize),
    /// Inside a string closed by this quote.
    Quoted(char),
    /// Inside a Rust raw string closed by a quote and this many hashes.
    Raw(usize),
}

/// The code braces of `line`, and where the lexer stands after it.
fn scan(line: &str, language: Language, mut state: Lexed) -> (Vec<(usize, char)>, Lexed) {
    let bytes = line.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let (next, after) = match state {
            Lexed::Comment(depth) => commented(bytes, at, depth, language),
            Lexed::Quoted(quote) => quoted(bytes, at, quote, language),
            Lexed::Raw(hashes) => raw(bytes, at, hashes),
            Lexed::Code if bytes[at] == b'/' && bytes.get(at + 1) == Some(&b'/') => break,
            Lexed::Code => {
                if matches!(bytes[at], b'{' | b'}') {
                    found.push((at, bytes[at] as char));
                }
                code(bytes, at, language)
            }
        };
        state = next;
        at = after;
    }
    // A string a line leaves open closes with it, except the ones a
    // language lets run on.
    if let Lexed::Quoted(quote) = state {
        if quote != '`' && quote != '"' {
            state = Lexed::Code;
        }
    }
    (found, state)
}

/// One step through a block comment `depth` deep.
fn commented(bytes: &[u8], at: usize, depth: usize, language: Language) -> (Lexed, usize) {
    match (bytes[at], bytes.get(at + 1)) {
        (b'*', Some(b'/')) if depth == 1 => (Lexed::Code, at + 2),
        (b'*', Some(b'/')) => (Lexed::Comment(depth - 1), at + 2),
        (b'/', Some(b'*')) if language == Language::Rust => (Lexed::Comment(depth + 1), at + 2),
        _ => (Lexed::Comment(depth), at + 1),
    }
}

/// One step through a string closed by `quote`. A backslash escapes what
/// follows, except in Go's raw strings.
fn quoted(bytes: &[u8], at: usize, quote: char, language: Language) -> (Lexed, usize) {
    match bytes[at] {
        b'\\' if !(language == Language::Go && quote == '`') => (Lexed::Quoted(quote), at + 2),
        byte if byte == quote as u8 => (Lexed::Code, at + 1),
        _ => (Lexed::Quoted(quote), at + 1),
    }
}

/// One step through a Rust raw string closed by a quote and `hashes`.
fn raw(bytes: &[u8], at: usize, hashes: usize) -> (Lexed, usize) {
    let closes = bytes[at] == b'"'
        && bytes.len() > at + hashes
        && bytes[at + 1..=at + hashes].iter().all(|byte| *byte == b'#');
    match closes {
        true => (Lexed::Code, at + 1 + hashes),
        false => (Lexed::Raw(hashes), at + 1),
    }
}

/// One step through code: what opens a comment, a string or a character
/// literal, or one byte of code.
fn code(bytes: &[u8], at: usize, language: Language) -> (Lexed, usize) {
    match (bytes[at], bytes.get(at + 1).copied()) {
        (b'/', Some(b'*')) => (Lexed::Comment(1), at + 2),
        (b'"', _) => (Lexed::Quoted('"'), at + 1),
        (b'`', _) if language != Language::Rust => (Lexed::Quoted('`'), at + 1),
        (b'\'', _) if language == Language::Script => (Lexed::Quoted('\''), at + 1),
        // A character literal, never a lifetime: `'{'`, `'\n'`.
        (b'\'', Some(b'\\')) => (Lexed::Quoted('\''), at + 1),
        (b'\'', _) if bytes.get(at + 2) == Some(&b'\'') => (Lexed::Code, at + 3),
        (b'r', Some(b'"' | b'#')) if language == Language::Rust && raw_opens(bytes, at) => {
            let hashes = bytes[at + 1..]
                .iter()
                .take_while(|byte| **byte == b'#')
                .count();
            (Lexed::Raw(hashes), at + 2 + hashes)
        }
        _ => (Lexed::Code, at + 1),
    }
}

/// Whether the `r` at `at` opens a raw string rather than ending a name.
fn raw_opens(bytes: &[u8], at: usize) -> bool {
    let named = at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
    let hashes = bytes[at + 1..].iter().take_while(|b| **b == b'#').count();
    !named && bytes.get(at + 1 + hashes) == Some(&b'"')
}

/// `code` with each Python function that is not a test folded to its
/// signature.
fn indented(code: &str) -> String {
    let lines: Vec<&str> = code.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        let trimmed = line.trim_start();
        let def = trimmed
            .strip_prefix("async ")
            .unwrap_or(trimmed)
            .strip_prefix("def ");
        let Some(def) = def else {
            out.push(line.to_string());
            at += 1;
            continue;
        };
        let indent = line.len() - trimmed.len();
        let signed = (at..lines.len())
            .find(|end| lines[*end].trim_end().ends_with(':'))
            .unwrap_or(at);
        let mut end = signed;
        for (next, body) in lines.iter().enumerate().skip(signed + 1) {
            if body.trim().is_empty() {
                continue;
            }
            if body.len() - body.trim_start().len() <= indent {
                break;
            }
            end = next;
        }
        let hidden = end - signed;
        if def.starts_with("test") || hidden < 2 {
            out.extend(lines[at..=end].iter().map(|line| line.to_string()));
        } else {
            out.extend(lines[at..signed].iter().map(|line| line.to_string()));
            out.push(format!(
                "{} ... {}",
                lines[signed].trim_end(),
                yunta_core::text::counted(hidden, "line")
            ));
        }
        at = end + 1;
    }
    let mut folded = out.join("\n");
    if code.ends_with('\n') {
        folded.push('\n');
    }
    folded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rust_test_stays_whole_and_its_helpers_fold_to_their_signature() {
        let code = r##"use std::process::Command;

/// Runs the program with `args`.
fn run(args: &[&str]) -> String {
    let out = Command::new("greet").args(args).output().unwrap();
    let said = String::from_utf8(out.stdout).unwrap();
    said.replace("{", "").replace('}', "")
}

#[test]
fn the_greeting_names_the_person() {
    let said = run(&["ana"]);
    assert_eq!(said, "hello, ana\n");
}

#[tokio::test]
async fn a_raw_string_holds_its_braces() {
    let raw = r#"{ "a": "}" }"#;
    assert!(raw.contains('{'));
}
"##;
        assert_eq!(
            folded("tests/greet.rs", code),
            r##"use std::process::Command;

/// Runs the program with `args`.
fn run(args: &[&str]) -> String { ... 3 lines }

#[test]
fn the_greeting_names_the_person() {
    let said = run(&["ana"]);
    assert_eq!(said, "hello, ana\n");
}

#[tokio::test]
async fn a_raw_string_holds_its_braces() {
    let raw = r#"{ "a": "}" }"#;
    assert!(raw.contains('{'));
}
"##
        );
    }

    #[test]
    fn a_rust_helper_inside_a_test_module_folds_and_a_short_one_stays() {
        let code = "#[cfg(test)]\nmod tests {\n    fn one() -> u8 {\n        1\n    }\n\n    fn setup() -> Vec<u8> {\n        let mut v = Vec::new();\n        v.push(one());\n        v\n    }\n\n    #[test]\n    fn it_sets_up() {\n        assert_eq!(setup(), [1]);\n    }\n}\n";
        assert_eq!(
            folded("src/lib.rs", code),
            "#[cfg(test)]\nmod tests {\n    fn one() -> u8 {\n        1\n    }\n\n    fn setup() -> Vec<u8> { ... 3 lines }\n\n    #[test]\n    fn it_sets_up() {\n        assert_eq!(setup(), [1]);\n    }\n}\n"
        );
    }

    #[test]
    fn a_python_test_stays_whole_and_its_fixtures_fold() {
        let code = "import pytest\n\n@pytest.fixture\ndef greeter():\n    g = Greeter()\n    g.load('{')\n    return g\n\n\ndef test_greets(greeter):\n    assert greeter.hello('ana') == 'hello, ana'\n";
        assert_eq!(
            folded("tests/test_greet.py", code),
            "import pytest\n\n@pytest.fixture\ndef greeter(): ... 3 lines\n\n\ndef test_greets(greeter):\n    assert greeter.hello('ana') == 'hello, ana'\n"
        );
    }

    #[test]
    fn a_script_s_tests_stay_whole_and_its_functions_fold() {
        let code = "const render = (name) => {\n  const out = `{${name}}`;\n  return out.trim();\n  // }\n};\n\ndescribe('greet', () => {\n  it('names the person', () => {\n    expect(render('ana')).toBe('{ana}');\n  });\n});\n";
        assert_eq!(
            folded("greet.test.ts", code),
            "const render = (name) => { ... 3 lines };\n\ndescribe('greet', () => {\n  it('names the person', () => {\n    expect(render('ana')).toBe('{ana}');\n  });\n});\n"
        );
    }

    #[test]
    fn a_go_test_stays_whole_and_its_helpers_fold() {
        let code = "package greet\n\nfunc run(name string) string {\n\tout := `{` + name\n\tout += \"}\"\n\treturn out\n}\n\nfunc TestGreets(t *testing.T) {\n\tif run(\"ana\") != \"{ana}\" {\n\t\tt.Fatal(\"no\")\n\t}\n}\n";
        assert_eq!(
            folded("greet_test.go", code),
            "package greet\n\nfunc run(name string) string { ... 3 lines }\n\nfunc TestGreets(t *testing.T) {\n\tif run(\"ana\") != \"{ana}\" {\n\t\tt.Fatal(\"no\")\n\t}\n}\n"
        );
    }

    #[test]
    fn a_file_in_a_language_it_does_not_read_comes_back_whole() {
        let code = "test \"$(cat greeting.txt)\" = Hello\nhelper() {\n  a\n  b\n  c\n}\n";
        assert_eq!(folded("tests/greet.sh", code), code);
    }
}
