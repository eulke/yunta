use super::*;

/// The tokens of `source` in `language` that are not space, each as its
/// kind and the text it covers.
fn read(source: &str, language: Language) -> Vec<(Kind, &str)> {
    tokens(source, Some(language))
        .into_iter()
        .filter(|token| token.kind != Kind::Space)
        .map(|token| (token.kind, &source[token.range]))
        .collect()
}

/// The words of `text` as Rust, each as it reads.
fn words_of(text: &str) -> Vec<&str> {
    words(text, Some(Language::Rust))
        .into_iter()
        .map(|word| &text[word])
        .collect()
}

/// Each sample is covered by its tokens: every byte is in exactly one token,
/// the tokens come in order, and none is empty.
#[test]
fn every_byte_is_in_one_token_in_order() {
    let samples = [
        (
            Language::Rust,
            "let s = r#\"a\"#; // c\nfn f<'a>() -> char { 'x' }",
        ),
        (Language::C, "int x = 1; /* a */ char c = '\\n';"),
        (Language::Java, "String s = \"\"\"\nhi\n\"\"\";"),
        (Language::Go, "s := `a` + \"b\" // c"),
        (Language::Kotlin, "val s = \"${a} $b\""),
        (Language::Scala, "val s = s\"$a\""),
        (Language::Swift, "let s = #\"\\#(a)\"#"),
        (Language::CSharp, "var s = $\"{a}\"; var t = @\"b\";"),
        (Language::Dart, "var s = '${a}' + r'$b';"),
        (Language::Script, "const s = `a${b}c`; // é"),
        (Language::Python, "s = f\"{a}\"  # c\nt = '''x'''"),
        (Language::Shell, "echo \"${a} $b\" 'c' # d"),
        (Language::Yaml, "a: 'b' # c\n- \"d\"\n"),
        (Language::Toml, "a = \"b\" # c\nd = '''e'''"),
        (Language::Json, "{\"a\": [1.5, true]}"),
        (
            Language::Markdown,
            "# A\n<!-- b -->\n`c` and\n```rust\nfn x\n```\n",
        ),
        (
            Language::Shell,
            "cat <<EOF | sort\n$a ${b}\nEOF\necho \\' $'c\\'d' 'e'",
        ),
        (Language::Yaml, "a: |\n  b # c\n\nd: 'e''f'\n- >-\n  g\n"),
        (Language::Markdown, "``` x\n`y`` z\n\n~~~\nw\n"),
        (Language::CSharp, "var s = @\"a \"\"b\"\"\";"),
    ];
    for (language, sample) in samples {
        let pieces = tokens(sample, Some(language));
        let mut at = 0;
        for piece in &pieces {
            assert_eq!(
                piece.range.start, at,
                "{language:?} reads its tokens in order"
            );
            assert!(!piece.range.is_empty(), "{language:?} has no empty token");
            at = piece.range.end;
        }
        assert_eq!(at, sample.len(), "{language:?} covers the whole sample");
    }
}

/// A Rust source reads its raw string, its comment, its lifetimes and its
/// character literal as a reader sees them.
#[test]
fn rust_reads_its_comments_literals_and_lifetimes() {
    let source = "let s = r#\"a \"b\"\"#; // c\nfn f<'a>(x: &'a u8) -> char { 'x' }";
    assert_eq!(
        read(source, Language::Rust),
        [
            (Kind::Name, "let"),
            (Kind::Name, "s"),
            (Kind::Sign, "="),
            (Kind::Text, "r#\"a \"b\"\"#"),
            (Kind::Sign, ";"),
            (Kind::Comment, "// c"),
            (Kind::Name, "fn"),
            (Kind::Name, "f"),
            (Kind::Sign, "<"),
            (Kind::Sign, "'"),
            (Kind::Name, "a"),
            (Kind::Sign, ">"),
            (Kind::Sign, "("),
            (Kind::Name, "x"),
            (Kind::Sign, ":"),
            (Kind::Sign, "&"),
            (Kind::Sign, "'"),
            (Kind::Name, "a"),
            (Kind::Name, "u8"),
            (Kind::Sign, ")"),
            (Kind::Sign, "-"),
            (Kind::Sign, ">"),
            (Kind::Name, "char"),
            (Kind::Sign, "{"),
            (Kind::Text, "'x'"),
            (Kind::Sign, "}"),
        ]
    );
}

/// A Rust literal takes its byte or C prefix into its text, and a char is
/// one text however many bytes the char takes.
#[test]
fn a_rust_literal_takes_its_prefix_and_a_char_is_one_text() {
    assert_eq!(
        read("b\"x\" b'y' c\"z\" 'é'", Language::Rust),
        [
            (Kind::Text, "b\"x\""),
            (Kind::Text, "b'y'"),
            (Kind::Text, "c\"z\""),
            (Kind::Text, "'é'"),
        ]
    );
}

/// A C# verbatim string writes a quote as two, so a doubled quote is its
/// text and only a single one closes it.
#[test]
fn a_csharp_verbatim_string_writes_a_quote_as_two() {
    assert_eq!(
        read("@\"a \"\"b\"\" c\" + d", Language::CSharp),
        [
            (Kind::Text, "@\"a \"\"b\"\" c\""),
            (Kind::Sign, "+"),
            (Kind::Name, "d"),
        ]
    );
}

/// A hole in a literal is code: a name between the text on each side of it,
/// in each language whose literals hold code.
#[test]
fn a_hole_is_code_between_two_pieces_of_text() {
    assert_eq!(
        read("`a${b}c`", Language::Script),
        [(Kind::Text, "`a${"), (Kind::Name, "b"), (Kind::Text, "}c`")]
    );
    assert_eq!(
        read("f\"{x}\"", Language::Python),
        [(Kind::Text, "f\"{"), (Kind::Name, "x"), (Kind::Text, "}\"")]
    );
    assert_eq!(
        read("\"$name\"", Language::Kotlin),
        [
            (Kind::Text, "\"$"),
            (Kind::Name, "name"),
            (Kind::Text, "\""),
        ]
    );
    assert_eq!(
        read("\"\\(x)\"", Language::Swift),
        [
            (Kind::Text, "\"\\("),
            (Kind::Name, "x"),
            (Kind::Text, ")\""),
        ]
    );
    assert_eq!(
        read("\"${a} $b $(c)\"", Language::Shell),
        [
            (Kind::Text, "\"${"),
            (Kind::Name, "a"),
            (Kind::Text, "} $"),
            (Kind::Name, "b"),
            (Kind::Text, " $("),
            (Kind::Name, "c"),
            (Kind::Text, ")\""),
        ]
    );
}

/// Holes nested far deeper than code nests them are still read, every byte
/// in one token, on a small stack: past the deepest hole the lexer reads as
/// code, a hole is read as the text around it.
#[test]
fn holes_nested_past_the_deepest_are_read_without_exhausting_the_stack() {
    /// Far past `DEEPEST`, the deepest a hole is read as code, so most of
    /// the holes are read as text.
    const DEPTH: usize = 10_000;
    /// Smaller than a thread's default stack: a reading that nests must stay
    /// shallow to fit it.
    const STACK: usize = 512 * 1024;
    let source = format!("`{}x{}`", "${`".repeat(DEPTH), "`}".repeat(DEPTH));
    let covered = std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(move || {
            let mut at = 0;
            for piece in tokens(&source, Some(Language::Script)) {
                assert_eq!(piece.range.start, at, "the tokens come in order");
                at = piece.range.end;
            }
            at == source.len()
        })
        .expect("the reading starts")
        .join()
        .expect("the reading ends");
    assert!(covered, "the tokens cover the source");
}

/// A quote its language does not let run on ends at its line, so the code
/// after the break is code again; a Rust string does run on past a break.
#[test]
fn a_quote_that_cannot_run_on_ends_at_its_line() {
    assert_eq!(
        read("x = 'a\ny = 1", Language::Script),
        [
            (Kind::Name, "x"),
            (Kind::Sign, "="),
            (Kind::Text, "'a"),
            (Kind::Name, "y"),
            (Kind::Sign, "="),
            (Kind::Number, "1"),
        ]
    );
    assert_eq!(
        read("let s = \"a\nb\";", Language::Rust),
        [
            (Kind::Name, "let"),
            (Kind::Name, "s"),
            (Kind::Sign, "="),
            (Kind::Text, "\"a\nb\""),
            (Kind::Sign, ";"),
        ]
    );
}

/// A number is one token with its letters, digits and underscores, and a dot
/// joins it only where a digit follows, so the dots of a range stand apart.
#[test]
fn a_number_is_one_token() {
    assert_eq!(
        read("1.0 0x1F 1_000u32 1..2", Language::Rust),
        [
            (Kind::Number, "1.0"),
            (Kind::Number, "0x1F"),
            (Kind::Number, "1_000u32"),
            (Kind::Number, "1"),
            (Kind::Sign, "."),
            (Kind::Sign, "."),
            (Kind::Number, "2"),
        ]
    );
}

/// Masked code keeps every byte where it stood: a comment turns to spaces,
/// a string does too where quoting hides its words, and a YAML value keeps
/// its quotes, since YAML quoting hides no word.
#[test]
fn masked_blanks_comments_and_text_where_quoting_hides_a_word() {
    let rust = "a /* b */ \"c\"\nd // é";
    assert_eq!(
        masked(rust, Some(Language::Rust)),
        format!("a{}\nd{}", " ".repeat(12), " ".repeat(6))
    );
    assert_eq!(masked(rust, Some(Language::Rust)).len(), rust.len());
    assert_eq!(
        masked("a: \"b\" # c\n", Some(Language::Yaml)),
        format!("a: \"b\" {}\n", " ".repeat(3))
    );
    assert_eq!(masked("a // b", None), "a // b");
}

/// Each extension names its language and the code fence a viewer colors it
/// with; a family is every extension of one language, and an extension the
/// lexer does not know is a family of its own.
#[test]
fn the_extension_table_names_each_language_and_its_fence() {
    assert_eq!(Language::of("a/b.tsx"), Some(Language::Script));
    assert_eq!(Language::of("Makefile"), None);
    assert_eq!(fence("a/b.tsx"), "tsx");
    assert_eq!(fence("x.h"), "c");
    assert_eq!(fence("x.hpp"), "cpp");
    assert_eq!(fence("x.zzz"), "");
    assert_eq!(
        Language::family("tsx"),
        ["js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts"]
    );
    assert_eq!(Language::family("h"), ["c", "h", "cc", "cpp", "hpp"]);
    assert_eq!(Language::family("yml"), ["yaml", "yml"]);
    assert_eq!(Language::family("zsh"), ["sh", "bash", "zsh"]);
    assert_eq!(Language::family("md"), ["md", "markdown"]);
    assert_eq!(Language::family("rs"), ["rs"]);
    assert_eq!(Language::family("zzz"), ["zzz"]);
}

/// A fence's info string names its language by a fence name or an
/// extension, its first word read; a word no language has names none.
#[test]
fn a_fence_names_its_language_by_its_first_word() {
    assert_eq!(Language::named("rust"), Some(Language::Rust));
    assert_eq!(Language::named("rs"), Some(Language::Rust));
    assert_eq!(Language::named("rust,ignore"), Some(Language::Rust));
    assert_eq!(Language::named("ts"), Some(Language::Script));
    assert_eq!(Language::named("typescript"), Some(Language::Script));
    assert_eq!(Language::named("sh title=\"a\""), Some(Language::Shell));
    assert_eq!(Language::named(""), None);
    assert_eq!(Language::named("text"), None);
}

/// A word is a name, a literal or a sign, and never a comment or a comma; a
/// language the lexer does not read has no strings or comments.
#[test]
fn a_word_is_a_name_a_literal_or_a_sign_and_never_a_comment_or_a_comma() {
    assert_eq!(
        words_of("pub fn f(a: u8, /* b, */ s: &str) -> char; // c, d\n'{'"),
        [
            "pub", "fn", "f", "(", "a", ":", "u8", "s", ":", "&", "str", ")", "-", ">", "char",
            ";", "'{'"
        ]
    );
    assert_eq!(
        words_of("let s = \"a, b\" + r#\"c \"d\"\"#;"),
        ["let", "s", "=", "\"a, b\"", "+", "r#\"c \"d\"\"#", ";"]
    );
    assert_eq!(
        words_of("fn g<'a>(x: &'a Ñame)"),
        ["fn", "g", "<", "'", "a", ">", "(", "x", ":", "&", "'", "a", "Ñame", ")"]
    );
    assert_eq!(
        words("a, \"b c\" // d", None).len(),
        8,
        "a language this does not read has no strings or comments"
    );
}

/// What stands between two words is read as a reader sees it: the whitespace
/// and the commas, with the comments taken out.
#[test]
fn what_stands_between_two_words_is_read_without_its_comments() {
    assert_eq!(
        uncommented(",\n    /// the first\n    ", Some(Language::Rust)),
        ",\n    \n    "
    );
    assert_eq!(uncommented(" /* a, b */ , ", Some(Language::Rust)), "  , ");
}

/// A script reads a regular expression where an operand starts, so a bracket
/// or a quote in it is its text; a `/` after an operand divides.
#[test]
fn a_script_reads_a_regular_expression_where_an_operand_starts() {
    assert_eq!(
        read("const open = /\\(['/]/g;\nx = a / b / c;", Language::Script),
        [
            (Kind::Name, "const"),
            (Kind::Name, "open"),
            (Kind::Sign, "="),
            (Kind::Text, "/\\(['/]/g"),
            (Kind::Sign, ";"),
            (Kind::Name, "x"),
            (Kind::Sign, "="),
            (Kind::Name, "a"),
            (Kind::Sign, "/"),
            (Kind::Name, "b"),
            (Kind::Sign, "/"),
            (Kind::Name, "c"),
            (Kind::Sign, ";"),
        ]
    );
}

/// The code before a regular expression is read past its comments, so a
/// comment that ends the line before leaves the code before it as the last
/// code, and the `/` after it opens the expression. An escaped slash on an
/// earlier line is text, so the semicolon that ends that line is the last code.
#[test]
fn a_regular_expression_reads_the_code_before_it_past_comments_and_escapes() {
    assert_eq!(
        read("f(a, // first\n  /\\(/g)", Language::Script),
        [
            (Kind::Name, "f"),
            (Kind::Sign, "("),
            (Kind::Name, "a"),
            (Kind::Sign, ","),
            (Kind::Comment, "// first"),
            (Kind::Text, "/\\(/g"),
            (Kind::Sign, ")"),
        ]
    );
    assert_eq!(
        read(
            "const u = /https?:\\/\\//;\n/re/.test(s);",
            Language::Script
        ),
        [
            (Kind::Name, "const"),
            (Kind::Name, "u"),
            (Kind::Sign, "="),
            (Kind::Text, "/https?:\\/\\//"),
            (Kind::Sign, ";"),
            (Kind::Text, "/re/"),
            (Kind::Sign, "."),
            (Kind::Name, "test"),
            (Kind::Sign, "("),
            (Kind::Name, "s"),
            (Kind::Sign, ")"),
            (Kind::Sign, ";"),
        ]
    );
}

/// A `/` after a tag's `<` closes the tag, and one after `++` or `--`
/// divides, so neither opens a regular expression.
#[test]
fn a_slash_after_a_tag_or_an_increment_divides() {
    assert_eq!(
        read("<p>x</p>; i++ / 2 + j / 3", Language::Script),
        [
            (Kind::Sign, "<"),
            (Kind::Name, "p"),
            (Kind::Sign, ">"),
            (Kind::Name, "x"),
            (Kind::Sign, "<"),
            (Kind::Sign, "/"),
            (Kind::Name, "p"),
            (Kind::Sign, ">"),
            (Kind::Sign, ";"),
            (Kind::Name, "i"),
            (Kind::Sign, "+"),
            (Kind::Sign, "+"),
            (Kind::Sign, "/"),
            (Kind::Number, "2"),
            (Kind::Sign, "+"),
            (Kind::Name, "j"),
            (Kind::Sign, "/"),
            (Kind::Number, "3"),
        ]
    );
}

/// A backslash before a line break leaves no closing slash on the regular
/// expression's line, so the `/` opens nothing and the code after the break
/// is code.
#[test]
fn a_backslash_before_a_line_break_opens_no_regular_expression() {
    assert_eq!(
        read("x = /a\\\nb/g;", Language::Script),
        [
            (Kind::Name, "x"),
            (Kind::Sign, "="),
            (Kind::Sign, "/"),
            (Kind::Name, "a"),
            (Kind::Sign, "\\"),
            (Kind::Name, "b"),
            (Kind::Sign, "/"),
            (Kind::Name, "g"),
            (Kind::Sign, ";"),
        ]
    );
}

/// A `/` before a `>` closes a self-closing tag when a `}` stands before it,
/// so a comment after the tag on its line is still a comment. After a `(` the
/// same two bytes open a regular expression, and a quote in it is its text.
#[test]
fn a_slash_before_a_greater_than_sign_closes_a_tag_only_after_a_brace() {
    assert_eq!(
        read("<Foo a={b} /> {/* c */}", Language::Script),
        [
            (Kind::Sign, "<"),
            (Kind::Name, "Foo"),
            (Kind::Name, "a"),
            (Kind::Sign, "="),
            (Kind::Sign, "{"),
            (Kind::Name, "b"),
            (Kind::Sign, "}"),
            (Kind::Sign, "/"),
            (Kind::Sign, ">"),
            (Kind::Sign, "{"),
            (Kind::Comment, "/* c */"),
            (Kind::Sign, "}"),
        ]
    );
    assert_eq!(
        read("s.replace(/>'/g, '')", Language::Script),
        [
            (Kind::Name, "s"),
            (Kind::Sign, "."),
            (Kind::Name, "replace"),
            (Kind::Sign, "("),
            (Kind::Text, "/>'/g"),
            (Kind::Sign, ","),
            (Kind::Text, "''"),
            (Kind::Sign, ")"),
        ]
    );
}

mod data;
