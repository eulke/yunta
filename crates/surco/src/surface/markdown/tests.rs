use crate::components::{
    Cell, Chosen, Code, Column, Concern, Decision, DecisionOption, Evidence, Fields, Fix, Fold,
    Gist, Headline, Holds, Marked, Prose, Row, Section, Table, Title,
};
use crate::doc::{Block, Doc};
use crate::glyphs::Mark;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::surface::{Description, Leaves, Markdown, Surface, Terminal};

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

/// A section titled `title`, holding `blocks`.
fn section(title: &str, blocks: Vec<Block>) -> Section {
    Section {
        lead: None,
        title: Line::new().plain(title),
        blocks,
    }
}

/// A task's card as a forge folds it: its id in bold, then what it does,
/// with code and the signs a summary must escape.
fn card_fold() -> Fold {
    Fold(Section {
        lead: None,
        title: Line::new()
            .push(Tone::Strong, "T001")
            .plain(": Say `<b>` & go"),
        blocks: vec![Prose("inside".to_string()).into()],
    })
}

/// A list whose every item carries the caution mark.
fn caution(items: &[&str]) -> Marked {
    Marked {
        mark: Mark::Caution,
        items: items.iter().map(|item| item.to_string()).collect(),
    }
}

/// An option `id`, read as `label`, which does `tradeoff`.
fn option(id: &str, label: &str, tradeoff: &str) -> DecisionOption {
    DecisionOption {
        id: id.to_string(),
        label: Some(label.to_string()),
        tradeoff: Some(tradeoff.to_string()),
        asks: None,
    }
}

/// A file's sections stand under its title, two levels down from it; a
/// forge's description starts on a line of its own, since the forge's
/// title heads the page, and its title is a heading of the second level.
#[test]
fn a_file_heads_its_sections_under_its_title_and_a_description_under_the_forges() {
    let doc = Doc::new()
        .with(Headline::new(Mark::NeedsYou, "decision", "needs you"))
        .with(Title::named(Line::new().plain("t")))
        .with(section("s", vec![section("i", Vec::new()).into()]));
    assert_eq!(
        Markdown.draw(&doc),
        "# ◆ decision · needs you\n\n# t\n\n## s\n\n### i\n"
    );
    assert_eq!(
        Description::default().draw(&doc),
        "◆ **decision** · needs you\n\n## t\n\n## s\n\n### i\n"
    );
}

/// A fold is drawn closed under its title, which the summary says as HTML:
/// strong words bold, code as code, and every sign HTML reads escaped.
#[test]
fn a_fold_is_drawn_closed_under_a_summary_html_reads() {
    assert_eq!(
        Markdown.draw(&Doc::new().with(card_fold())),
        "<details>\n\n<summary><b>T001</b>: Say <code>&lt;b&gt;</code> &amp; go</summary>\n\ninside\n\n</details>\n"
    );
}

/// A caution is an alert where it stands on its own; inside a fold, where
/// an alert cannot stand, it is a list of marked items.
#[test]
fn a_caution_is_an_alert_outside_a_fold_and_an_item_inside_one() {
    let doc = Doc::new()
        .with(caution(&["a", "b"]))
        .with(Fold(section("card", vec![caution(&["c"]).into()])));
    assert_eq!(
        Markdown.draw(&doc),
        "> [!CAUTION]\n> - a\n> - b\n\n<details>\n\n<summary>card</summary>\n\n- ▲ c\n\n</details>\n"
    );
}

/// A decision answered on a forge is a table of the answers, what a
/// reviewer does there for each and what happens, and then what is typed
/// elsewhere, under its own title.
#[test]
fn a_decision_on_a_forge_is_a_table_of_answers_and_what_is_typed_elsewhere() {
    let decision = Decision {
        chosen: Chosen::OnTheForge {
            forge: "GitHub".to_string(),
            elsewhere: "on the machine".to_string(),
        },
        options: vec![
            option("approve", "approve it", "it passes"),
            option("close", "close it", "nothing runs"),
        ],
        otherwise: vec![("yunta resume x".to_string(), "reads it")],
    };
    assert_eq!(
        Markdown.draw(&Doc::new().with(decision)),
        concat!(
            "| answer | on GitHub | what happens |\n",
            "|---|---|---|\n",
            "| **approve** | approve it | it passes |\n",
            "| **close** | close it | nothing runs |\n",
            "\n",
            "on the machine\n",
            "\n",
            "- `yunta resume x`: reads it\n",
        )
    );
}

/// A decision made here is a table of its options: what each does, what
/// happens and what it asks, and the command that chooses it; the other
/// ways out follow under "or".
#[test]
fn a_decision_made_here_is_a_table_of_options_with_the_command_of_each() {
    let mut adjust = option("adjust", "Back to `plan`", "sends it back");
    adjust.asks = Some("what should change?".to_string());
    let commands = [
        ("approve", "yunta resolve-gate x approve"),
        ("adjust", "yunta resolve-gate x adjust --text \"<answer>\""),
    ]
    .into_iter()
    .map(|(id, command)| (id.to_string(), command.to_string()))
    .collect();
    let decision = Decision {
        chosen: Chosen::Here { commands },
        options: vec![
            option("approve", "On to `build`", "resolves this gate"),
            adjust,
        ],
        otherwise: vec![("yunta close x".to_string(), "closes it for good")],
    };
    assert_eq!(
        Markdown.draw(&Doc::new().with(decision)),
        concat!(
            "| option | what it does | what happens | command |\n",
            "|---|---|---|---|\n",
            "| **approve** | On to `build` | resolves this gate | `yunta resolve-gate x approve` |\n",
            "| **adjust** | Back to `plan` | sends it back<br>asks: what should change? | `yunta resolve-gate x adjust --text \"<answer>\"` |\n",
            "\n",
            "or\n",
            "\n",
            "- `yunta close x`: closes it for good\n",
        )
    );
}

/// A description that leaves code lines out keeps what is said around each
/// block: where it is, what for, and what is said under it.
#[test]
fn a_description_that_leaves_code_lines_keeps_what_is_said_around_them() {
    let doc = Doc::new()
        .with(Code::whole("a.rs", Some("what".to_string()), "fn a() {}"))
        .with(card_fold());
    let lean = Description {
        leaves: Leaves::CodeLines,
    };
    assert_eq!(
        lean.draw(&doc),
        "`a.rs`: what\n\n_1 line_\n\n<details>\n\n<summary><b>T001</b>: Say <code>&lt;b&gt;</code> &amp; go</summary>\n\ninside\n\n</details>\n"
    );
}

/// A description that leaves folds out keeps each fold's title, as a line.
#[test]
fn a_description_that_leaves_folds_keeps_each_title_as_a_line() {
    let lean = Description {
        leaves: Leaves::Folds,
    };
    assert_eq!(
        lean.draw(&Doc::new().with(card_fold())),
        "**T001**: Say `<b>` & go\n"
    );
}

/// Plain text keeps a sign HTML would read as a tag: a summary that names
/// `Vec<T>` still says it, while code keeps its own text.
#[test]
fn plain_text_keeps_what_html_would_read_as_a_tag() {
    let doc = Doc::new()
        .with(Gist::new("Make Vec<T> generic"))
        .with(Prose("a `Vec<T>` stays code".to_string()))
        .with(section("T<x> for all", Vec::new()));
    assert_eq!(
        Markdown.draw(&doc),
        "Make Vec\\<T> generic.\n\na `Vec<T>` stays code\n\n## T\\<x> for all\n"
    );
}

/// A decision answered at the prompt under it is a table of its options,
/// what each does and what happens, with no command: the prompt chooses.
#[test]
fn a_decision_answered_at_the_prompt_is_a_table_without_commands() {
    let decision = Decision {
        chosen: Chosen::AtThePrompt,
        options: vec![option("approve", "On to `build`", "resolves this gate")],
        otherwise: Vec::new(),
    };
    assert_eq!(
        Markdown.draw(&Doc::new().with(decision)),
        concat!(
            "| option | what it does | what happens |\n",
            "|---|---|---|\n",
            "| **approve** | On to `build` | resolves this gate |\n",
        )
    );
}

/// Every word Markdown writes keeps a sign HTML would read as a tag, in an
/// alert, a list, a field, a table and a decision alike, while code keeps its
/// own text and the break a decision's cell is drawn with stays a break.
#[test]
fn words_keep_a_sign_html_would_read_as_a_tag_wherever_they_stand() {
    let table = Table {
        columns: vec![Column {
            title: "task",
            holds: Holds::Prose,
        }],
        rows: vec![Row {
            mark: None,
            cells: vec![Cell::plain("Parse Vec<u8>")],
        }],
    };
    let decision = Decision {
        chosen: Chosen::AtThePrompt,
        options: vec![DecisionOption {
            id: "approve".to_string(),
            label: Some("Keep Option<T>".to_string()),
            tradeoff: Some("returns Result<T>".to_string()),
            asks: Some("which <kind>?".to_string()),
        }],
        otherwise: Vec::new(),
    };
    let doc = Doc::new()
        .with(caution(&["risk: callers unwrap Option<String>"]))
        .with(Marked {
            mark: Mark::Absent,
            items: vec!["Vec<T> stays `Vec<T>`".to_string()],
        })
        .with(Fields::new().push_if("chose", "Return Option<String>"))
        .with(table)
        .with(decision);
    assert_eq!(
        Markdown.draw(&doc),
        concat!(
            "> [!CAUTION]\n",
            "> - risk: callers unwrap Option\\<String>\n",
            "\n",
            "- ◌ Vec\\<T> stays `Vec<T>`\n",
            "\n",
            "- chose: Return Option\\<String>\n",
            "\n",
            "| task |\n",
            "|---|\n",
            "| Parse Vec\\<u8> |\n",
            "\n",
            "| option | what it does | what happens |\n",
            "|---|---|---|\n",
            "| **approve** | Keep Option\\<T> | returns Result\\<T><br>asks: which \\<kind>? |\n",
        )
    );
}

/// An item of several lines stays whole inside what holds it: each line after
/// its first goes on inside the alert, or under the list item, and a blank line
/// within it keeps the quote.
#[test]
fn an_item_of_several_lines_stays_inside_its_alert_or_its_list_item() {
    let doc = Doc::new()
        .with(caution(&["risk: First.\n\nSecond."]))
        .with(Marked {
            mark: Mark::Absent,
            items: vec!["left out: one\ntwo".to_string()],
        });
    assert_eq!(
        Markdown.draw(&doc),
        concat!(
            "> [!CAUTION]\n",
            "> - risk: First.\n",
            ">\n",
            ">   Second.\n",
            "\n",
            "- ◌ left out: one\n",
            "  two\n",
        )
    );
}

/// Markdown a block holds cannot spill into what follows it: a fence it leaves
/// open is closed where the block ends, by the run of signs that opened it,
/// while a block whose fences close stays as written.
#[test]
fn a_fence_a_markdown_block_leaves_open_is_closed_where_it_ends() {
    let doc = Doc::new()
        .with(Block::Markdown(
            "Intro.\n\n````rust\nlet a = \"```\";".to_string(),
        ))
        .with(Block::Markdown("~~~\nb\n~~~".to_string()))
        .with(Prose("after".to_string()));
    assert_eq!(
        Markdown.draw(&doc),
        "Intro.\n\n````rust\nlet a = \"```\";\n````\n\n~~~\nb\n~~~\n\nafter\n"
    );
}

/// A value or a fact of several lines stays inside its list item: each later
/// line goes under the item's text, so a blank line in it ends nothing.
#[test]
fn a_value_or_a_fact_of_several_lines_stays_inside_its_list_item() {
    let doc = Doc::new()
        .with(Fields::new().push_if("because", "It reads.\n\nIt holds."))
        .with(Concern {
            fact: "callers unwrap\n\nnone check".to_string(),
            so: "a panic".to_string(),
            fix: Fix::said("return a Result"),
        });
    assert_eq!(
        Markdown.draw(&doc),
        "- because: It reads.\n\n  It holds.\n\n\
         - ▲ callers unwrap\n\n  none check\n  - so: a panic\n  - fix: return a Result\n"
    );
}

/// A fence around what a command printed or a fix pastes is longer than any
/// fence the text holds, so no line of the text closes it.
#[test]
fn a_fence_around_quoted_or_pasted_text_outlasts_every_fence_in_it() {
    let quoted = Evidence {
        tail: ["expected:", "```rust", "fn main() {}", "```"]
            .map(str::to_string)
            .to_vec(),
        whole: None,
    };
    let pasted = Concern {
        fact: "the hook is missing".to_string(),
        so: "nothing runs".to_string(),
        fix: Fix {
            said: "add it".to_string(),
            paste: ["```sh", "yunta run", "```"].map(str::to_string).to_vec(),
        },
    };
    assert_eq!(
        Markdown.draw(&Doc::new().with(quoted).with(pasted)),
        "````\nexpected:\n```rust\nfn main() {}\n```\n````\n\n\
         - ▲ the hook is missing\n  - so: nothing runs\n  - fix: add it\n\n\
         \x20   ````\n    ```sh\n    yunta run\n    ```\n    ````\n"
    );
}

/// A word never escapes or pairs with what is written after it: a backslash it
/// ends with is doubled, so it escapes no closing sign, and a backtick it
/// leaves unpaired is escaped, so no code span pairs across two spans.
#[test]
fn a_word_never_escapes_or_pairs_with_the_sign_written_after_it() {
    let line = Line::new()
        .push(Tone::Strong, "C:\\")
        .plain(" then ")
        .push(Tone::Strong, "a`")
        .plain(" x`<b>`y");
    assert_eq!(
        Markdown.draw(&Doc::new().with(Block::Lines(vec![line]))),
        "**C:\\\\** then **a\\`** x`<b>`y\n"
    );
}

/// A command is a code span as long as its text needs, so a backtick inside it
/// closes nothing, and one at its edge is set apart from the span's own.
#[test]
fn a_command_is_a_code_span_no_backtick_inside_it_closes() {
    let fields = Fields::new()
        .push_command("run", "echo `date` | wc")
        .push_command("then", "`ls`");
    assert_eq!(
        Markdown.draw(&Doc::new().with(fields)),
        "- run: ``echo `date` | wc``\n- then: `` `ls` ``\n"
    );
}
