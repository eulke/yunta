//! A test file as a review reads it: each test whole, and each function
//! that only sets one up folded to its signature and how long it is. The
//! test is what a reader judges; a helper is a name it calls, and its
//! body is one command away. Where a function starts and ends is the item
//! reader's; which function is a test is read here.

use super::items::{Item, Kind, Text};
use super::Language;
use crate::glyphs::Glyphs;

/// `code`, the file at `path`, with each function that is not a test
/// folded to its signature: `signature { ... N lines }` where braces hold
/// its body, and `signature: ... N lines` in Python. A function whose body
/// hides fewer than two lines stays whole, and a file in a language whose
/// items are not read comes back whole.
pub(crate) fn folded(path: &str, code: &str) -> String {
    let Some(text) = Language::of(path).and_then(|language| Text::of(code, language)) else {
        return code.to_string();
    };
    let file = File::of(code);
    let mut out = Vec::new();
    // The blocks of members the walk stands in, innermost last, each with
    // where its body ends: their lines are read as their members.
    let mut within: Vec<(Kind, usize)> = Vec::new();
    let mut line = 0;
    while line < file.lines.len() {
        let at = file.starts[line];
        while within.last().is_some_and(|&(_, end)| at >= end) {
            within.pop();
        }
        line = step(&text, &file, line, &mut within, &mut out);
    }
    let mut folded = out.join("\n");
    if code.ends_with('\n') {
        folded.push('\n');
    }
    folded
}

/// A file's lines, and the offset each starts at.
struct File<'a> {
    lines: Vec<&'a str>,
    starts: Vec<usize>,
}

impl<'a> File<'a> {
    /// Reads `code` into its lines, and records the offset each one starts at.
    fn of(code: &'a str) -> File<'a> {
        File {
            lines: code.lines().collect(),
            starts: std::iter::once(0)
                .chain(code.match_indices('\n').map(|(at, _)| at + 1))
                .collect(),
        }
    }

    /// The line that holds offset `at`.
    fn line_of(&self, at: usize) -> usize {
        let line = self.starts.partition_point(|&start| start <= at);
        line.saturating_sub(1)
            .min(self.lines.len().saturating_sub(1))
    }

    /// Lines `from` up to `to`, as the file writes them.
    fn copied(&self, from: usize, to: usize) -> impl Iterator<Item = String> + '_ {
        self.lines[from..to].iter().map(|line| line.to_string())
    }
}

/// Writes the statement at `line` onto `out`, a helper function folded,
/// and answers the line to read next. A test, and an item that holds no
/// members, is written whole; a block of members is written a line at a
/// time, its lines read as its members, and so is any other statement, so
/// the functions inside it are read too.
fn step(
    text: &Text,
    file: &File,
    line: usize,
    within: &mut Vec<(Kind, usize)>,
    out: &mut Vec<String>,
) -> usize {
    let holder = within.last().map(|&(kind, _)| kind);
    let (item, next) = text.item_at(file.starts[line], holder);
    // An item that starts on a later line is read when the walk reaches it.
    let item = item.filter(|item| file.line_of(item.start) == line);
    match item {
        Some(item) if item.kind == Kind::Fn && !tested(text, &item) => {
            fold(text, file, &item, line, out)
        }
        Some(item) if item.kind.holds_members() => {
            if let Some(body) = &item.bounds.body {
                within.push((item.kind, body.end));
            }
            out.push(file.lines[line].to_string());
            line + 1
        }
        Some(item) => {
            let last = file.line_of(item.bounds.end.saturating_sub(1));
            whole(file, line, last, out)
        }
        None if text.language() == Language::Script && calls_a_test(file.lines[line]) => {
            whole(file, line, file.line_of(next.saturating_sub(1)), out)
        }
        None => {
            out.push(file.lines[line].to_string());
            line + 1
        }
    }
}

/// Writes lines `line` through `last` onto `out` as the file writes them,
/// and answers the line after them.
fn whole(file: &File, line: usize, last: usize, out: &mut Vec<String>) -> usize {
    let last = last.max(line);
    out.extend(file.copied(line, last + 1));
    last + 1
}

/// Writes `item`, a function that is not a test, onto `out` from `line`,
/// folded to its signature where its body hides two lines or more, and
/// answers the line after it. Only a body braces hold folds to them, or a
/// Python body to its head; a script's arrow whose body is an expression
/// stays whole.
fn fold(text: &Text, file: &File, item: &Item, line: usize, out: &mut Vec<String>) -> usize {
    let last = file.line_of(item.bounds.end.saturating_sub(1)).max(line);
    let Some(body) = &item.bounds.body else {
        return whole(file, line, last, out);
    };
    let open = file.line_of(item.bounds.stop);
    let open_at = item.bounds.stop - file.starts[open];
    let python = text.language() == Language::Python;
    if !python && file.lines[open].as_bytes().get(open_at) != Some(&b'{') {
        return whole(file, line, last, out);
    }
    let close = file.line_of(body.end);
    let close_at = body.end - file.starts[close];
    // A body that no brace closes, or that another bracket closes, is not
    // one braces hold, so it stays whole.
    if !python && file.lines[close].as_bytes().get(close_at) != Some(&b'}') {
        return whole(file, line, last, out);
    }
    let hidden = match python {
        true => last - open,
        false => hidden_lines(file, (open, open_at), (close, close_at)),
    };
    // A body of one line is the whole function, and folding it would hide
    // what it says to save nothing.
    if hidden < 2 {
        return whole(file, line, last, out);
    }
    out.extend(file.copied(line, open));
    let lines = crate::text::counted(hidden, "line");
    let ellipsis = Glyphs::Ascii.ellipsis();
    match python {
        true => {
            let (head, rest) = file.lines[open].split_at(open_at + 1);
            out.push(format!("{head} {ellipsis} {lines}{}", rest.trim_end()));
            last + 1
        }
        false => {
            let head = &file.lines[open][..open_at];
            let tail = file.lines[close].get(close_at + 1..).unwrap_or_default();
            out.push(format!("{head}{{ {ellipsis} {lines} }}{tail}"));
            // The walk resumes past the `}`'s line: code the span holds after
            // it, such as an indented `;`, is read on its own, not dropped.
            close + 1
        }
    }
}

/// How many lines a body braces hold hides when folded: the lines between
/// the line of its `{`, at `open`, and the line of its `}`, at `close`, and
/// each of those two where code stands past the `{` or before the `}`.
fn hidden_lines(file: &File, open: (usize, usize), close: (usize, usize)) -> usize {
    let ((open, open_at), (close, close_at)) = (open, close);
    if close <= open {
        return 0;
    }
    let past_open = !file.lines[open][open_at + 1..].trim().is_empty();
    let before_close = !file.lines[close][..close_at].trim().is_empty();
    close - open - 1 + usize::from(past_open) + usize::from(before_close)
}

/// Whether `item`, a function, is a test: in Rust, one an attribute of it
/// names, as `#[test]` or `#[tokio::test]` do and `#[cfg(test)]` does not;
/// in Go, one whose own name the test runner calls, a method's after its
/// receiver's type, as testify runs `Suite.TestLogin`; in Python, one whose
/// name starts with `test`. A script's tests are calls, not functions.
fn tested(text: &Text, item: &Item) -> bool {
    match text.language() {
        Language::Rust => text
            .attributes(item)
            .split("#[")
            .skip(1)
            .any(|attribute| !attribute.starts_with("cfg(") && attribute.contains("test")),
        Language::Go => {
            let name = item.name.rsplit('.').next().unwrap_or_default();
            ["Test", "Benchmark", "Example", "Fuzz"]
                .iter()
                .any(|prefix| name.starts_with(prefix))
        }
        Language::Python => item.name.starts_with("test"),
        _ => false,
    }
}

/// Whether `line` of a script calls a test: `test(` or `it(`, alone or as
/// `.each` or `.only`.
fn calls_a_test(line: &str) -> bool {
    let line = line.trim_start();
    [
        "test(",
        "it(",
        "test.each",
        "it.each",
        "test.only(",
        "it.only(",
    ]
    .iter()
    .any(|call| line.starts_with(call))
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

    /// A script's helper inside a `describe` block folds, and its test
    /// stays whole.
    #[test]
    fn a_script_s_helper_inside_a_describe_block_folds() {
        let code = "describe('greet', () => {\n  const setup = () => {\n    const a = 1;\n    const b = 2;\n    return a + b;\n  };\n\n  it('adds', () => {\n    expect(setup()).toBe(3);\n  });\n});\n";
        assert_eq!(
            folded("greet.test.js", code),
            "describe('greet', () => {\n  const setup = () => { ... 3 lines };\n\n  it('adds', () => {\n    expect(setup()).toBe(3);\n  });\n});\n"
        );
    }

    /// A Python helper method folds inside its class, and a test method
    /// stays whole.
    #[test]
    fn a_python_helper_method_folds_inside_its_class() {
        let code = "class TestStore:\n    def make(self):\n        s = Store()\n        s.add(1)\n        return s\n\n    def test_adds(self):\n        assert self.make().count == 1\n";
        assert_eq!(
            folded("tests/test_store.py", code),
            "class TestStore:\n    def make(self): ... 3 lines\n\n    def test_adds(self):\n        assert self.make().count == 1\n"
        );
    }

    /// A script class's helper method folds inside its class, read as a
    /// member of it.
    #[test]
    fn a_script_class_s_helper_method_folds() {
        let code = "export class Fixture {\n  build(): Store {\n    const s = new Store();\n    s.add(1);\n    return s;\n  }\n}\n";
        assert_eq!(
            folded("fixture.test.ts", code),
            "export class Fixture {\n  build(): Store { ... 3 lines }\n}\n"
        );
    }

    /// A Go suite's method whose own name says it is a test stays whole.
    #[test]
    fn a_go_suite_s_test_method_stays_whole() {
        let code = "package suite\n\nfunc (s *Suite) TestLogin() {\n\ta := 1\n\tb := 2\n\ts.Equal(a, b)\n}\n";
        assert_eq!(folded("suite_test.go", code), code);
    }

    /// A Python helper's fold stands right after its colon, before the
    /// comment its head carries.
    #[test]
    fn a_python_fold_stands_before_the_comment_on_its_head() {
        let code = "def helper():  # sets up\n    a = 1\n    b = 2\n    return a + b\n";
        assert_eq!(
            folded("tests/test_a.py", code),
            "def helper(): ... 3 lines  # sets up\n"
        );
    }

    /// Code on the brace lines is hidden and counted, beside the lines
    /// between them.
    #[test]
    fn code_on_the_brace_lines_counts_among_the_hidden_lines() {
        let code = "func helper() { a := 1\n\tb := 2\n\tc := 3\n\treturn a + b + c }\n";
        assert_eq!(folded("a_test.go", code), "func helper() { ... 4 lines }\n");
    }

    /// A body no brace closes, as a file cut short leaves one, stays whole.
    #[test]
    fn a_body_no_brace_closes_stays_whole() {
        let code = "fn helper() -> u8 {\n    let a = 1;\n    let b = 2;\n    a + b\n";
        assert_eq!(folded("tests/a.rs", code), code);
    }

    /// What a statement holds past its folded body's `}`, on lines of its
    /// own, is written after the fold, not dropped.
    #[test]
    fn code_past_a_folded_body_s_brace_is_kept() {
        let code = "function helper() {\n  one();\n  two();\n  three();\n}\n  ;\n";
        assert_eq!(
            folded("a.test.ts", code),
            "function helper() { ... 3 lines }\n  ;\n"
        );
    }
}
