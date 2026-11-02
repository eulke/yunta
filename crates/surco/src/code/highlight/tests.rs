use super::*;
use crate::line::Syntax::*;

/// What `text`, code in `language`, colors, each stretch as the text it
/// covers and what it is.
fn colored(text: &str, language: Language) -> Vec<(&str, Syntax)> {
    highlighted(text, Some(language))
        .into_iter()
        .map(|(stretch, syntax)| (&text[stretch], syntax))
        .collect()
}

/// A Rust function reads as its attribute, keywords, types, calls, macro,
/// string, number and comment, each as a reader sees it.
#[test]
fn rust_reads_keywords_types_calls_macros_and_attributes() {
    assert_eq!(
        colored(
            "#[test]\nfn f(x: u8) -> Option<Path> { format!(\"a\", 1); g(x) } // c",
            Language::Rust,
        ),
        [
            ("#[test]", Attribute),
            ("fn", Keyword),
            ("f", Function),
            ("u8", Type),
            ("Option", Type),
            ("Path", Type),
            ("format!", Macro),
            ("\"a\"", Text),
            ("1", Number),
            ("g", Function),
            ("// c", Comment),
        ]
    );
}

/// A capital before a call is a type in Rust, and a function in Go, where a
/// capital names a function.
#[test]
fn a_capital_before_a_call_is_a_type_but_a_function_in_go() {
    assert_eq!(colored("Some(x)", Language::Rust), [("Some", Type)]);
    assert_eq!(
        colored("fmt.Println(x)", Language::Go),
        [("Println", Function)]
    );
}

/// A `!=` is a comparison in Rust, so its `!` opens no macro.
#[test]
fn a_rust_not_equal_is_no_macro() {
    assert!(colored("a!=b", Language::Rust).is_empty());
}

/// A Python decorator reads as an attribute, and its definition reads as a
/// function, with the words around it as keywords; `self` is a name Python
/// does not reserve.
#[test]
fn python_reads_its_decorators_and_definitions() {
    assert_eq!(
        colored(
            "@cached\ndef load(self):\n    return None",
            Language::Python
        ),
        [
            ("@cached", Attribute),
            ("def", Keyword),
            ("load", Function),
            ("return", Keyword),
            ("None", Keyword),
        ]
    );
}

/// Shell reads its keywords, its command words, its flags and its comment,
/// and a double-quoted string splits around the variable it holds.
#[test]
fn shell_reads_commands_flags_and_keywords() {
    assert_eq!(
        colored(
            "if git diff --name-only; then\n  echo \"$a\" # c\nfi",
            Language::Shell,
        ),
        [
            ("if", Keyword),
            ("git", Function),
            ("--name-only", Keyword),
            ("then", Keyword),
            ("echo", Function),
            ("\"$", Text),
            ("\"", Text),
            ("# c", Comment),
            ("fi", Keyword),
        ]
    );
}

/// YAML reads its keys, a comment, a quoted value and a number, each as a
/// reader sees it.
#[test]
fn yaml_reads_keys_values_and_comments() {
    assert_eq!(
        colored(
            "name: build # c\nsteps:\n  - run: \"true\"\n  - on_failure: 2",
            Language::Yaml,
        ),
        [
            ("name", Keyword),
            ("# c", Comment),
            ("steps", Keyword),
            ("run", Keyword),
            ("\"true\"", Text),
            ("on_failure", Keyword),
            ("2", Number),
        ]
    );
}

/// TOML reads a table's header as a type, its keys and its literal words as
/// keywords, and its strings as text.
#[test]
fn toml_reads_tables_keys_and_values() {
    assert_eq!(
        colored(
            "[package]\nname = \"a\"\nedition.workspace = true",
            Language::Toml,
        ),
        [
            ("[package]", Type),
            ("name", Keyword),
            ("\"a\"", Text),
            ("edition.workspace", Keyword),
            ("true", Keyword),
        ]
    );
}

/// JSON reads an object's keys apart from its values, and a literal word such
/// as null as a keyword.
#[test]
fn json_reads_keys_apart_from_values() {
    assert_eq!(
        colored("{\"a\": \"b\", \"c\": null}", Language::Json),
        [
            ("\"a\"", Keyword),
            ("\"b\"", Text),
            ("\"c\"", Keyword),
            ("null", Keyword),
        ]
    );
}

/// A Markdown heading, a code span and a fence's markers read as a reader
/// sees them, and a fence's code takes the tones of the language it names.
#[test]
fn markdown_reads_headings_code_and_fence_markers() {
    assert_eq!(
        colored(
            "# Title\n## Part\nsee `x`\n```rust\nfn a\n```",
            Language::Markdown,
        ),
        [
            ("# Title", Type),
            ("## Part", Keyword),
            ("`x`", Text),
            ("```rust", Comment),
            ("fn", Keyword),
            ("a", Function),
            ("```", Comment),
        ]
    );
}

/// Where a string runs on past a line, only the part of it inside the line's
/// range is colored, and the rest of the line stays plain.
#[test]
fn painted_colors_only_what_stands_inside_its_range() {
    let text = "let s = \"a\nb\";";
    let line = painted(text, 0..10, &highlighted(text, Some(Language::Rust)));
    let spans: Vec<(&str, Tone)> = line
        .spans()
        .iter()
        .map(|span| (span.text.as_str(), span.tone))
        .collect();
    assert_eq!(
        spans,
        [
            ("let", Tone::Code(Keyword)),
            (" s = ", Tone::Plain),
            ("\"a", Tone::Code(Text)),
        ]
    );
    assert_eq!(line.text(), "let s = \"a");
}

/// Without a language, no word of the text is code, so nothing is colored.
#[test]
fn with_no_language_nothing_is_colored() {
    assert!(highlighted("fn x", None).is_empty());
}

/// A fence that names no language leaves its code plain, with only its
/// markers colored.
#[test]
fn a_fence_that_names_no_language_reads_its_code_plain() {
    assert_eq!(
        colored("```\nfn a\n```", Language::Markdown),
        [("```", Comment), ("```", Comment)]
    );
}

/// A stack small enough that fences nested without a bound would exhaust it.
/// One mebibyte holds the DEEPEST levels even in a debug build, where frames
/// are largest.
const STACK: usize = 1024 * 1024;

/// An empty fence reads both its lines as markers, and a fence the text
/// leaves open reads its opening line as one.
#[test]
fn a_fence_s_lines_are_markers_whether_it_holds_code_or_not() {
    assert_eq!(
        colored("```rust\n```", Language::Markdown),
        [("```rust", Comment), ("```", Comment)]
    );
    assert_eq!(
        colored("```rust", Language::Markdown),
        [("```rust", Comment)]
    );
}

/// A fence is read as the lexer reads one: a backtick in its info string
/// makes its line no fence but text, with its own code span; a fence line
/// after it ends that paragraph and opens a fence of its own, and only that
/// fence's marks close it.
#[test]
fn a_fence_is_read_as_the_lexer_reads_one() {
    assert_eq!(
        colored("```a`b`\nx\n```", Language::Markdown),
        [("`b`", Text), ("```", Comment)]
    );
    assert_eq!(
        colored("~~~\nx\n```", Language::Markdown),
        [("~~~", Comment)]
    );
}

/// A comment before a `#` on its line leaves the `#` no heading.
#[test]
fn a_comment_before_a_heading_marker_makes_no_heading() {
    assert_eq!(
        colored("<!-- c --> # Title", Language::Markdown),
        [("<!-- c -->", Comment)]
    );
}

/// Fences nested past the deepest read their code plain, so no text can
/// exhaust the stack: an opening line is a marker at each depth through the
/// deepest, and none past it.
#[test]
fn fences_nested_past_the_deepest_read_their_code_plain() {
    let text = "```md\n".repeat(10_000);
    let markers = std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(move || highlighted(&text, Some(Language::Markdown)).len())
        .expect("the reading starts")
        .join()
        .expect("the reading ends");
    assert_eq!(markers, DEEPEST + 1);
}

/// An attribute left open is plain, and one closed after it is read whole.
#[test]
fn an_attribute_left_open_is_plain_and_the_next_is_read_whole() {
    assert_eq!(
        colored("#[a\n#[b]\nfn f() {}", Language::Rust),
        [("#[b]", Attribute), ("fn", Keyword), ("f", Function)]
    );
}

/// A name after a `.` names a member, whatever word it is: a method called
/// `set` reads as a function, and fields called `type` or `string` plain. A
/// keyword its language writes after a dot stays one, and a spread's `...`
/// or a range's `..` is no member's dot.
#[test]
fn a_member_s_name_is_the_member_s_whatever_word_it_is() {
    assert_eq!(
        colored("map.set(k, v); obj.type = obj.string;", Language::Script),
        [("set", Function)]
    );
    assert_eq!(
        colored("[...this.items]", Language::Script),
        [("this", Keyword)]
    );
    assert_eq!(
        colored("self.items.set(x); self.type", Language::Python),
        [("set", Function)]
    );
    assert_eq!(colored("x.await", Language::Rust), [("await", Keyword)]);
    assert_eq!(
        colored("0..self.len()", Language::Rust),
        [("0", Number), ("self", Keyword), ("len", Function)]
    );
    assert_eq!(
        colored("String.class", Language::Java),
        [("String", Type), ("class", Keyword)]
    );
}

/// A TOML line inside a value's array that opens with `[` is no table's
/// header.
#[test]
fn a_bracket_inside_a_toml_value_opens_no_table() {
    assert_eq!(
        colored("matrix = [\n  [1, 2],\n]", Language::Toml),
        [("matrix", Keyword), ("1", Number), ("2", Number)]
    );
}

/// A heading opens with one to six `#` that a space, a tab or the line's end
/// follows, so seven `#`, or a `#` a word follows, open none.
#[test]
fn a_heading_opens_with_one_to_six_marks_and_a_blank() {
    assert_eq!(
        colored("####### seven\n#tag\n##\tTab\n######", Language::Markdown),
        [("##\tTab", Keyword), ("######", Keyword)]
    );
}

/// A line of a code's text is painted with the stretches it holds, found
/// past the stretches of the lines before it.
#[test]
fn painted_finds_the_stretches_of_a_later_line() {
    let text = "fn a() {}\nfn b() {}";
    let colored = highlighted(text, Some(Language::Rust));
    let line = painted(text, 10..19, &colored);
    let spans: Vec<(&str, Tone)> = line
        .spans()
        .iter()
        .map(|span| (span.text.as_str(), span.tone))
        .collect();
    assert_eq!(
        spans,
        [
            ("fn", Tone::Code(Keyword)),
            (" ", Tone::Plain),
            ("b", Tone::Code(Function)),
            ("() {}", Tone::Plain),
        ]
    );
}
