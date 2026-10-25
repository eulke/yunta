use crate::components::Fields;
use crate::doc::Doc;
use crate::line::Line;
use crate::look::Look;
use crate::surface::{Markdown, Surface, Terminal};

/// Rows of a plan's card: the paths and names it lists, read as written,
/// and words that set a flag apart as code.
fn card() -> Fields {
    Fields::new()
        .push_literal("implies", "pkg/__init__.py")
        .push_literal("", "src/__tests__/catalog.test.ts")
        .push_literal(
            "changes",
            "crates/cli/src/commands/pack_audit.rs, docs/[slug]/a*b*.md",
        )
        .push_if(
            "keeps",
            "Pack commands accept `--global` only with `--run-tests`",
        )
}

/// A path in Markdown reads as the path: no underscore of it turns what
/// it stands around bold, while words keep the code they set apart.
#[test]
fn a_path_in_markdown_reads_as_the_path_and_words_keep_their_code() {
    let drawn = Markdown.draw(&Doc::new().with(card()));
    assert_eq!(
        drawn.lines().collect::<Vec<_>>(),
        [
            r"- implies: pkg/\_\_init\_\_.py  ",
            r"  src/\_\_tests\_\_/catalog.test.ts",
            r"- changes: crates/cli/src/commands/pack_audit.rs, docs/\[slug\]/a\*b\*.md",
            "- keeps: Pack commands accept `--global` only with `--run-tests`",
        ]
    );
}

/// Every sign Markdown reads as markup inside a line is escaped; a run of
/// underscores between two letters opens nothing and is kept.
#[test]
fn text_read_as_written_escapes_each_sign_markdown_would_read() {
    let names = Fields::new().push_literal("names", r"a\b `c` ~d~ <e> &f; snake_case _g_ h__i");
    assert_eq!(
        Markdown.draw(&Doc::new().with(names)),
        concat!(
            r"- names: a\\b \`c\` \~d\~ \<e> \&f; snake_case \_g\_ h__i",
            "\n"
        )
    );
}

/// A terminal reads no sign as markup: it draws what a row says as it is.
#[test]
fn a_terminal_draws_text_read_as_written_as_it_is() {
    let drawn: Vec<String> = Terminal::on(Look::plain())
        .block(&card().into())
        .iter()
        .map(Line::text)
        .collect();
    assert_eq!(
        drawn,
        [
            "  implies      pkg/__init__.py",
            "               src/__tests__/catalog.test.ts",
            "  changes      crates/cli/src/commands/pack_audit.rs, docs/[slug]/a*b*.md",
            "  keeps        Pack commands accept `--global` only with `--run-tests`",
        ]
    );
}
