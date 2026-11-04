//! How the lexer reads shell, YAML, TOML, JSON and Markdown.

use super::super::{code_spans, Fences, Kind, Language};
use super::read;

/// In YAML an apostrophe inside a plain word is text and opens no quote, and
/// a quote opens only where a value starts.
#[test]
fn yaml_opens_a_quote_only_where_a_value_starts() {
    let pieces = read("title: don't stop\nnext: 'x'", Language::Yaml);
    let next = pieces
        .iter()
        .position(|piece| *piece == (Kind::Name, "next"))
        .expect("the second key is read");
    assert!(pieces[..next].iter().all(|(kind, _)| *kind != Kind::Text));
    assert_eq!(pieces.last(), Some(&(Kind::Text, "'x'")));
}

/// A YAML single quote writes a quote as two, so a doubled quote is its text,
/// and a hash inside it is no comment.
#[test]
fn a_yaml_single_quote_writes_a_quote_as_two() {
    assert_eq!(
        read("a: 'Don''t # x'\nb: 1", Language::Yaml),
        [
            (Kind::Name, "a"),
            (Kind::Sign, ":"),
            (Kind::Text, "'Don''t # x'"),
            (Kind::Name, "b"),
            (Kind::Sign, ":"),
            (Kind::Number, "1"),
        ]
    );
}

/// A YAML block scalar is text through the lines indented under it, blank
/// ones among them, so a hash in it is no comment.
#[test]
fn a_yaml_block_scalar_is_text_through_its_indented_lines() {
    assert_eq!(
        read(
            "run: |\n  echo hi # x\n  ## Task\n\nnext: 'a'\n",
            Language::Yaml
        ),
        [
            (Kind::Name, "run"),
            (Kind::Sign, ":"),
            (Kind::Text, "|\n  echo hi # x\n  ## Task"),
            (Kind::Name, "next"),
            (Kind::Sign, ":"),
            (Kind::Text, "'a'"),
        ]
    );
}

/// A block scalar ends at the first line indented less than its first line,
/// as a key beside its own is.
#[test]
fn a_yaml_block_scalar_ends_where_its_indentation_does() {
    assert_eq!(
        read("- key: >-\n    text\n  other: x\n", Language::Yaml),
        [
            (Kind::Sign, "-"),
            (Kind::Name, "key"),
            (Kind::Sign, ":"),
            (Kind::Text, ">-\n    text"),
            (Kind::Name, "other"),
            (Kind::Sign, ":"),
            (Kind::Name, "x"),
        ]
    );
}

/// A shell here-document is text through the line that holds its word, with
/// its names as holes, and the rest of its operator's line is code.
#[test]
fn a_shell_here_document_is_text_and_its_operator_s_line_is_code() {
    assert_eq!(
        read(
            "cat <<EOF | sort\nDon't $HOME\nEOF\necho 'a'\n",
            Language::Shell
        ),
        [
            (Kind::Name, "cat"),
            (Kind::Text, "<<EOF"),
            (Kind::Sign, "|"),
            (Kind::Name, "sort"),
            (Kind::Text, "\nDon't $"),
            (Kind::Name, "HOME"),
            (Kind::Text, "\nEOF"),
            (Kind::Name, "echo"),
            (Kind::Text, "'a'"),
        ]
    );
}

/// A quoted here-document word holds no code, and with `<<-` its closing
/// line may open with tabs.
#[test]
fn a_quoted_here_document_holds_no_code_and_a_dash_lets_its_close_indent() {
    assert_eq!(
        read("cat <<-'EOF'\n\t$HOME\n\tEOF\necho\n", Language::Shell),
        [
            (Kind::Name, "cat"),
            (Kind::Text, "<<-'EOF'\n\t$HOME\n\tEOF"),
            (Kind::Name, "echo"),
        ]
    );
}

/// In shell a quote a backslash escapes opens nothing, and `$'…'` escapes
/// its quotes with a backslash.
#[test]
fn a_shell_quote_a_backslash_escapes_opens_nothing() {
    assert_eq!(
        read("echo don\\'t # c\nx=$'a\\'b'\n", Language::Shell),
        [
            (Kind::Name, "echo"),
            (Kind::Name, "don"),
            (Kind::Sign, "\\"),
            (Kind::Sign, "'"),
            (Kind::Name, "t"),
            (Kind::Comment, "# c"),
            (Kind::Name, "x"),
            (Kind::Sign, "="),
            (Kind::Sign, "$"),
            (Kind::Text, "'a\\'b'"),
        ]
    );
}

/// In shell a hash inside a word is a sign that opens no comment, and a hash
/// that starts a word opens one.
#[test]
fn a_hash_opens_a_comment_only_where_a_word_starts() {
    assert_eq!(
        read("echo a#b # c", Language::Shell),
        [
            (Kind::Name, "echo"),
            (Kind::Name, "a"),
            (Kind::Sign, "#"),
            (Kind::Name, "b"),
            (Kind::Comment, "# c"),
        ]
    );
}

/// Markdown reads its HTML comments, its code spans and its fenced code
/// blocks as the parts a reader sees.
#[test]
fn markdown_reads_its_comments_code_and_fences() {
    assert_eq!(
        read("a `b` <!-- c -->\n```rust\nfn x\n```\n", Language::Markdown),
        [
            (Kind::Name, "a"),
            (Kind::Text, "`b`"),
            (Kind::Comment, "<!-- c -->"),
            (Kind::Text, "```rust\nfn x\n```"),
        ]
    );
}

/// A fence closes only at a line of its marks, so marks inside its code do
/// not close it.
#[test]
fn a_markdown_fence_closes_only_at_a_line_of_its_marks() {
    assert_eq!(
        read(
            "```rust\nlet s = \"```\";\n```\nafter\n",
            Language::Markdown
        ),
        [
            (Kind::Text, "```rust\nlet s = \"```\";\n```"),
            (Kind::Name, "after"),
        ]
    );
}

/// A tilde fence is a fence too, and a longer fence holds a shorter run of
/// its marks.
#[test]
fn a_tilde_fence_and_a_longer_fence_hold_shorter_runs() {
    assert_eq!(
        read("~~~\nx\n~~~\n````\n```\n````\n", Language::Markdown),
        [(Kind::Text, "~~~\nx\n~~~"), (Kind::Text, "````\n```\n````"),]
    );
}

/// A code span closes only at a run of exactly as many backticks, and a run
/// that nothing closes is plain backticks.
#[test]
fn a_code_span_closes_at_a_run_of_as_many_and_an_unclosed_run_is_text() {
    assert_eq!(
        read("`a``b` and ``` alone\n", Language::Markdown),
        [
            (Kind::Text, "`a``b`"),
            (Kind::Name, "and"),
            (Kind::Sign, "`"),
            (Kind::Sign, "`"),
            (Kind::Sign, "`"),
            (Kind::Name, "alone"),
        ]
    );
}

/// A code span runs on over a line break, but not past a blank line, which
/// ends its paragraph.
#[test]
fn a_code_span_runs_over_a_line_break_but_not_past_a_blank_line() {
    assert_eq!(
        read("`a\nb` `c\n\nd`", Language::Markdown),
        [
            (Kind::Text, "`a\nb`"),
            (Kind::Sign, "`"),
            (Kind::Name, "c"),
            (Kind::Name, "d"),
            (Kind::Sign, "`"),
        ]
    );
}

/// A backtick a backslash escapes opens no code span.
#[test]
fn an_escaped_backtick_opens_no_code_span() {
    assert_eq!(
        read("\\`a` b", Language::Markdown),
        [
            (Kind::Sign, "\\"),
            (Kind::Sign, "`"),
            (Kind::Name, "a"),
            (Kind::Sign, "`"),
            (Kind::Name, "b"),
        ]
    );
}

/// In shell arithmetic `<<` shifts bits, so it opens no here-document, and
/// the comment after it is still a comment.
#[test]
fn a_shift_in_shell_arithmetic_opens_no_here_document() {
    assert_eq!(
        read("x=$((1 << bits))\n# c\n", Language::Shell),
        [
            (Kind::Name, "x"),
            (Kind::Sign, "="),
            (Kind::Sign, "$"),
            (Kind::Sign, "("),
            (Kind::Sign, "("),
            (Kind::Number, "1"),
            (Kind::Sign, "<"),
            (Kind::Sign, "<"),
            (Kind::Name, "bits"),
            (Kind::Sign, ")"),
            (Kind::Sign, ")"),
            (Kind::Comment, "# c"),
        ]
    );
}

/// A fence reads its opening line, its info string, its code and its
/// closing line, an indented closing line among them; a fence closed on the
/// line after it holds no code, and marks a backtick follows on their line,
/// or that a word stands before, open none.
#[test]
fn a_fence_reads_its_opening_info_code_and_closing() {
    let source = "```rust title\nfn a() {}\n  ```\nafter";
    let fences = Fences::of(source);
    let fence = fences.at(0).expect("the fence opens");
    assert_eq!(&source[fence.opening.clone()], "```rust title");
    assert_eq!(&source[fence.info.clone()], "rust title");
    assert_eq!(&source[fence.code.clone()], "fn a() {}");
    assert_eq!(
        fence.closing.clone().map(|closing| &source[closing]),
        Some("```")
    );
    let empty = Fences::of("```\n```");
    assert!(empty.at(0).expect("the fence opens").code.is_empty());
    assert!(Fences::of("```a`b`\n```").at(0).is_none());
    assert!(Fences::of("x ```\n```").at(2).is_none());
}

/// The code spans of prose are the runs of backticks a run of as many
/// closes, and a run none closes is not one.
#[test]
fn prose_reads_its_code_spans() {
    let prose = "run `a` and ``b`c`` but ` lone";
    let spans: Vec<&str> = code_spans(prose)
        .into_iter()
        .map(|span| &prose[span])
        .collect();
    assert_eq!(spans, ["`a`", "``b`c``"]);
}

/// A quoted `((` is text, so the here-document after it still opens, and its
/// body is text.
#[test]
fn a_quoted_double_paren_opens_no_arithmetic_so_its_here_document_opens() {
    assert_eq!(
        read("echo \"((\" <<EOF\nbody\nEOF\n", Language::Shell),
        [
            (Kind::Name, "echo"),
            (Kind::Text, "\"((\""),
            (Kind::Text, "<<EOF\nbody\nEOF"),
        ]
    );
}

/// A quoted `))` closes no arithmetic, so the shift in the arithmetic after it
/// stays in arithmetic: no here-document opens, and the comment after it is
/// still a comment.
#[test]
fn a_quoted_close_before_arithmetic_leaves_its_shift_in_arithmetic() {
    assert_eq!(
        read("echo \"))\" $((1 << 2))\n# c\n", Language::Shell),
        [
            (Kind::Name, "echo"),
            (Kind::Text, "\"))\""),
            (Kind::Sign, "$"),
            (Kind::Sign, "("),
            (Kind::Sign, "("),
            (Kind::Number, "1"),
            (Kind::Sign, "<"),
            (Kind::Sign, "<"),
            (Kind::Number, "2"),
            (Kind::Sign, ")"),
            (Kind::Sign, ")"),
            (Kind::Comment, "# c"),
        ]
    );
}

/// A group closed inside arithmetic closes only that group, so the shift after
/// it stays in arithmetic: no here-document opens, and the comment after it is
/// still a comment.
#[test]
fn a_closed_group_in_arithmetic_leaves_its_shift_in_arithmetic() {
    assert_eq!(
        read("echo $(( (a * (b + c)) << 1 ))\n# c\n", Language::Shell),
        [
            (Kind::Name, "echo"),
            (Kind::Sign, "$"),
            (Kind::Sign, "("),
            (Kind::Sign, "("),
            (Kind::Sign, "("),
            (Kind::Name, "a"),
            (Kind::Sign, "*"),
            (Kind::Sign, "("),
            (Kind::Name, "b"),
            (Kind::Sign, "+"),
            (Kind::Name, "c"),
            (Kind::Sign, ")"),
            (Kind::Sign, ")"),
            (Kind::Sign, "<"),
            (Kind::Sign, "<"),
            (Kind::Number, "1"),
            (Kind::Sign, ")"),
            (Kind::Sign, ")"),
            (Kind::Comment, "# c"),
        ]
    );
}

/// In a double quote, `$((…))` is arithmetic too, so a shift in it opens no
/// here-document, and the comment after the quote is still a comment.
#[test]
fn a_shift_in_double_quoted_arithmetic_opens_no_here_document() {
    assert_eq!(
        read("echo \"$((1 << 2))\"\n# c\n", Language::Shell),
        [
            (Kind::Name, "echo"),
            (Kind::Text, "\"$("),
            (Kind::Sign, "("),
            (Kind::Number, "1"),
            (Kind::Sign, "<"),
            (Kind::Sign, "<"),
            (Kind::Number, "2"),
            (Kind::Sign, ")"),
            (Kind::Text, ")\""),
            (Kind::Comment, "# c"),
        ]
    );
}

/// A fence in a list stands three spaces into the item that holds it, whatever that comes to
/// on its line; and what ends the item ends the fence.
#[test]
fn a_fence_deep_in_a_list_is_read_by_the_item_that_holds_it() {
    let text = "- a\n  - b\n\n    ```sh\n    x\n    ```\nAfter.\n";
    let at = text.find("```").expect("the opening marks");
    let fences = Fences::of(text);
    let fence = fences.at(at).expect("a fence");
    assert_eq!(&text[fence.info.clone()], "sh");
    assert_eq!(&text[fence.code.clone()], "    x");
    assert_eq!(fence.indent, 4);
    assert!(fence.closing.is_some());
    assert_eq!(&text[fence.next..], "After.\n");

    let open = "- a\n\n  ```sh\n  x\nAfter.\n";
    let at = open.find("```").expect("the opening marks");
    let fences = Fences::of(open);
    let fence = fences.at(at).expect("a fence");
    assert_eq!(&open[fence.code.clone()], "  x");
    assert!(fence.closing.is_none());
    assert_eq!(&open[fence.next..], "After.\n");

    let flat = "text\n\n    ```sh\n    x\n    ```\n";
    let at = flat.find("```").expect("the marks");
    assert!(
        Fences::of(flat).at(at).is_none(),
        "four spaces make it code"
    );
}

/// A fence's marks stand three spaces into their line at most: four make
/// indented code, so such marks open no fence and close none.
#[test]
fn a_fence_s_marks_stand_three_spaces_in_at_most() {
    assert!(Fences::of("    ```\nx\n```").at(4).is_none());
    assert_eq!(
        read("```\ncode\n    ```\nafter", Language::Markdown),
        [(Kind::Text, "```\ncode\n    ```\nafter")]
    );
}

/// A code span stops before a line a fence opens, so the fence is read
/// whole and the backtick before it is text.
#[test]
fn a_code_span_stops_before_a_fence() {
    assert_eq!(
        read("Press `:\n```rust\nlet c = '`';\n```\n", Language::Markdown),
        [
            (Kind::Name, "Press"),
            (Kind::Sign, "`"),
            (Kind::Sign, ":"),
            (Kind::Text, "```rust\nlet c = '`';\n```"),
        ]
    );
}
