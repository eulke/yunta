//! What a page must hold, whatever it is dressed in: every expected text here is
//! written by hand from what the surface promises.

use std::collections::BTreeMap;

use super::Html;
use crate::components::{
    Chosen, Code, Decision, DecisionOption, Diffstat, Evidence, Fields, FileRow, Furrow, Gist,
    Headline, Marked, MoreFiles, Next, Prose, Section, Title, Whole,
};
use crate::diagram::Diagram;
use crate::doc::{Block, Doc};
use crate::glyphs::Mark;
use crate::line::{Line, Tone};
use crate::surface::Surface;
use crate::words::Standing;

/// What a page shows a reader, as the terminal's words read it: a tag that sets a
/// block apart is a space, and so is one that marks a change, where the terminal
/// has its marks; any other tag inside a line is nothing, so `<b>strong</b>.` reads
/// `strong.` on both, and two cells a page writes with nothing between read as one.
pub(crate) fn visible(page: &str) -> String {
    const INSIDE_A_LINE: [&str; 5] = ["b", "code", "span", "a", "i"];
    let mut shown = String::new();
    let mut rest = page;
    while let Some(open) = rest.find('<') {
        shown.push_str(&rest[..open]);
        let Some((tag, after)) = rest[open + 1..].split_once('>') else {
            rest = "";
            break;
        };
        let name = tag
            .trim_start_matches('/')
            .split([' ', '/'])
            .next()
            .unwrap_or_default();
        if !INSIDE_A_LINE.contains(&name) {
            shown.push(' ');
        }
        rest = after;
    }
    shown.push_str(rest);
    shown
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn section(title: &str, blocks: Vec<Block>) -> Section {
    Section {
        lead: None,
        title: Line::new().plain(title),
        blocks,
    }
}

fn decision() -> Decision {
    let mut commands = BTreeMap::new();
    commands.insert(
        "approve".to_string(),
        "yunta resolve-gate 7E5PH4 approve".to_string(),
    );
    commands.insert(
        "adjust".to_string(),
        "yunta resolve-gate 7E5PH4 adjust --text \"<answer>\"".to_string(),
    );
    Decision {
        chosen: Chosen::Here { commands },
        options: vec![
            DecisionOption {
                id: "approve".to_string(),
                label: Some("builds the plan".to_string()),
                tradeoff: Some("its tasks run, a step at a time".to_string()),
                asks: None,
            },
            DecisionOption {
                id: "adjust".to_string(),
                label: Some("Back to `plan`".to_string()),
                tradeoff: Some("the plan is written again".to_string()),
                asks: Some("what should change?".to_string()),
            },
        ],
        otherwise: vec![("yunta close 7E5PH4".to_string(), "closes it for good")],
    }
}

/// What stands between a tag's opening and its closing, first found after `after`.
fn inside<'a>(page: &'a str, open: &str, close: &str) -> &'a str {
    let start = page
        .find(open)
        .unwrap_or_else(|| panic!("no {open} in {page}"))
        + open.len();
    let end = page[start..]
        .find(close)
        .unwrap_or_else(|| panic!("no {close} in {page}"));
    &page[start..start + end]
}

#[test]
fn a_page_opens_with_its_mark_its_subject_its_word_and_its_handle() {
    let doc = Doc::new().with(
        Headline::new(Mark::NeedsYou, "fragua (quick)", "needs you")
            .fact("for 32m10s")
            .edge("7E5PH4"),
    );
    let body = Html.body(&doc);
    assert!(body.contains("<h1>fragua (quick)</h1>"), "{body}");
    assert!(
        body.contains(r#"<span class="pill needs">needs you</span>"#),
        "{body}"
    );
    assert!(
        body.contains(r#"<span class="fact">for 32m10s</span>"#),
        "{body}"
    );
    assert!(body.contains(r#"<span class="id">7E5PH4</span>"#), "{body}");
    assert!(
        body.contains(r#"<span class="mark needs">◆</span>"#),
        "{body}"
    );
    let page = Html.draw(&doc);
    assert!(
        page.contains("<title>fragua (quick) · needs you · 7E5PH4</title>"),
        "{page}"
    );
}

#[test]
fn only_a_documents_first_headline_is_its_title() {
    let doc = Doc::new()
        .with(Headline::new(Mark::Done, "first", "finished"))
        .with(Headline::new(Mark::Failed, "second", "failed"));
    let body = Html.body(&doc);
    assert!(body.contains("<h1>first</h1>"), "{body}");
    assert!(body.contains("<h2>second</h2>"), "{body}");
}

#[test]
fn the_furrow_has_one_stroke_a_node_and_says_them_to_a_reader_that_cannot_see_it() {
    let doc = Doc::new().with(Furrow(vec![
        Standing::Finished,
        Standing::Failed,
        Standing::NeverRan,
    ]));
    let body = Html.body(&doc);
    let furrow = inside(&body, r#"<div class="furrow""#, "</div>");
    assert!(
        furrow.contains(r#"aria-label="finished, failed, never ran""#),
        "{furrow}"
    );
    assert_eq!(furrow.matches("<i").count(), 3, "{furrow}");
    assert!(
        furrow.contains(r#"<i class="done"></i><i class="failed"></i><i></i>"#),
        "{furrow}"
    );
}

#[test]
fn nothing_to_draw_draws_nothing() {
    let body = Html.body(&Doc::new().with(Furrow(Vec::new())));
    assert!(!body.contains("furrow"), "{body}");
    let body = Html.body(&Doc::new().with(section("", vec![Gist::new("under no title").into()])));
    assert!(body.contains("under no title"), "{body}");
    assert!(
        !body.contains("<h2"),
        "a blank title has no heading: {body}"
    );
    assert!(!body.contains("id="), "and no place to link to: {body}");
    let body = Html.body(&Doc::new().with(Block::Markdown(" \n".to_string())));
    assert!(!body.contains("written"), "{body}");
    let body = Html.body(&Doc::new().with(Block::Lines(vec![Line::new()])));
    assert!(!body.contains("<pre"), "{body}");
    let nothing = Decision {
        options: Vec::new(),
        otherwise: Vec::new(),
        ..decision()
    };
    let body = Html.body(&Doc::new().with(nothing));
    assert!(!body.contains("<aside"), "{body}");
    assert!(body.contains("no-decide"), "{body}");
}

#[test]
fn a_change_lists_its_files_with_what_each_moved() {
    let body = Html.body(&Doc::new().with(Diffstat {
        files: vec![FileRow {
            path: "src/<lib>.rs".to_string(),
            added: 142,
            removed: 38,
        }],
        more: Some(MoreFiles {
            files: 5,
            added: 123,
            removed: 16,
        }),
    }));
    assert!(body.contains("<code>src/&lt;lib&gt;.rs</code>"), "{body}");
    assert!(
        body.contains(r#"<td class="figure"><span class="done">+142</span></td>"#),
        "{body}"
    );
    assert!(
        body.contains(r#"<td class="figure"><span class="failed">−38</span></td>"#),
        "{body}"
    );
    assert!(
        body.contains(r#"<th class="figure">removed</th>"#),
        "{body}"
    );
    assert!(body.contains("5 more files, +123 −16"), "{body}");
}

#[test]
fn a_document_a_page_holds_is_a_heading_under_it_with_a_place_in_the_list() {
    let mut held = Title::named(Line::new().plain("plan"));
    held.facts = vec!["2 tasks".to_string()];
    let body = Html.body(
        &Doc::new()
            .with(Headline::new(Mark::Done, "fragua (quick)", "finished"))
            .with(section("tasks", vec![Gist::new("one").into()]))
            .with(held),
    );
    assert_eq!(body.matches("<h1").count(), 1, "{body}");
    assert!(
        body.contains(r#"<h2 class="title" id="s-2">plan <span class="fact">2 tasks</span></h2>"#),
        "{body}"
    );
    assert!(body.contains(r##"<a href="#s-2">plan</a>"##), "{body}");
}

#[test]
fn a_titles_backticks_set_code_in_the_list_too() {
    let body = Html.body(
        &Doc::new()
            .with(section("node `lint`", vec![Gist::new("one").into()]))
            .with(section("the rest", vec![Gist::new("two").into()])),
    );
    assert!(
        body.contains(r##"<a href="#s-1">node <code>lint</code></a>"##),
        "{body}"
    );
    assert!(
        body.contains(r#"<h2 class="sec">node <code>lint</code></h2>"#),
        "{body}"
    );
    let body = Html.body(&Doc::new().with(Code::whole("a.go", None, "x := `raw`")));
    assert!(
        body.contains("`raw`"),
        "a backtick in code is the code's: {body}"
    );
}

#[test]
fn a_value_in_two_lines_says_two_things() {
    let body = Html.body(
        &Doc::new().with(Fields::new().push_if("instead of", "Refuse to run\nAsk for a name")),
    );
    assert!(body.contains("Refuse to run<br>Ask for a name"), "{body}");
}

#[test]
fn a_nested_section_is_a_level_under_the_one_that_holds_it() {
    let body = Html.body(&Doc::new().with(section(
        "steps",
        vec![section("step 1", vec![Gist::new("one").into()]).into()],
    )));
    assert!(body.contains(r#"<h2 class="sec">steps</h2>"#), "{body}");
    assert!(body.contains(r#"<h3 class="sec">step 1</h3>"#), "{body}");
}

#[test]
fn a_link_that_spans_a_layer_goes_around_the_box_in_it() {
    // `done` is two layers from `plan`, so that link has a lane beside `build`.
    let body = Html.body(&Doc::new().with(Diagram::of(
        "flowchart LR\n  A[plan] --> B[build]\n  B --> C[done]\n  A --> C\n",
    )));
    let paths: Vec<&str> = body.split("<path").skip(1).collect();
    assert_eq!(paths.len(), 3, "{body}");
    assert!(
        paths.iter().any(|path| path.contains(" L")),
        "one runs along its lane: {body}"
    );
}

#[test]
fn a_decision_stands_beside_the_page_and_each_option_copies_its_command() {
    let doc = Doc::new()
        .with(Gist::new("it waits"))
        .with(section("approve the plan?", vec![decision().into()]));
    let body = Html.body(&doc);
    let main = inside(&body, r#"<main class="doc">"#, "</main>");
    assert!(main.contains("it waits"), "{main}");
    assert!(!main.contains("builds the plan"), "{main}");
    let aside = inside(&body, r#"<aside class="decide""#, "</aside>");
    assert!(aside.contains("approve the plan?"), "{aside}");
    assert!(aside.contains("<b>approve</b>"), "{aside}");
    assert!(
        aside.contains(r#"<span class="does">builds the plan</span>"#),
        "{aside}"
    );
    assert!(
        aside.contains(r#"<span class="costs">its tasks run, a step at a time</span>"#),
        "{aside}"
    );
    assert!(
        aside.contains(r#"data-copy="yunta resolve-gate 7E5PH4 approve""#),
        "{aside}"
    );
    // The words it asks for, left for the reader to write, stay whole in the attribute.
    assert!(
        aside.contains(
            "data-copy=\"yunta resolve-gate 7E5PH4 adjust --text &quot;&lt;answer&gt;&quot;\""
        ),
        "{aside}"
    );
    assert!(aside.contains("what should change?"), "{aside}");
    assert!(
        aside.contains(r#"data-copy="yunta close 7E5PH4""#),
        "{aside}"
    );
    assert!(aside.contains("closes it for good"), "{aside}");
}

#[test]
fn a_page_with_no_decision_has_no_column_for_one() {
    let body = Html.body(&Doc::new().with(Gist::new("it finished")));
    assert!(!body.contains("<aside"), "{body}");
    assert!(body.contains(r#"class="page no-toc no-decide""#), "{body}");
}

#[test]
fn a_page_with_more_than_one_section_lists_them_and_every_link_lands() {
    let doc = Doc::new()
        .with(section(
            "steps",
            vec![section("step 1", vec![Gist::new("one").into()]).into()],
        ))
        .with(section("left out", vec![Gist::new("two").into()]));
    let body = Html.body(&doc);
    let toc = inside(&body, r#"<nav class="toc""#, "</nav>");
    for title in ["steps", "step 1", "left out"] {
        assert!(toc.contains(title), "{toc}");
    }
    assert!(
        toc.contains(r#"class="sub""#),
        "the nested section is a level in: {toc}"
    );
    let mut links = 0;
    for part in toc.split("href=\"#").skip(1) {
        let id = &part[..part.find('"').unwrap_or(0)];
        assert!(body.contains(&format!(r#"id="{id}""#)), "no {id} in {body}");
        links += 1;
    }
    assert_eq!(links, 3, "{toc}");

    let lone = Html.body(&Doc::new().with(section("only", vec![Gist::new("one").into()])));
    assert!(!lone.contains("<nav"), "one section needs no list: {lone}");
}

#[test]
fn a_word_a_log_holds_is_never_read_as_a_tag() {
    let hostile = "<script>alert(\"x\")</script> & more";
    let doc = Doc::new()
        .with(Headline::new(Mark::Failed, hostile, "failed"))
        .with(Prose(hostile.to_string()))
        .with(Evidence {
            tail: vec![hostile.to_string()],
            whole: Some(Whole::Command(hostile.to_string())),
        })
        .with(Fields::new().push_if("said", hostile))
        .with(Marked {
            mark: Mark::Caution,
            items: vec![hostile.to_string()],
        })
        .with(Code::whole("src/a.rs", Some(hostile.to_string()), hostile))
        .with(Block::Markdown(hostile.to_string()))
        .with(Next {
            steps: vec![(hostile.to_string(), "does it")],
        });
    let page = Html.draw(&doc);
    assert!(!page.contains("<script>alert"), "{page}");
    assert!(page.contains("&lt;script&gt;"), "{page}");
    assert!(!page.contains("data-copy=\"<script>"), "{page}");
    assert!(
        !page.contains("alert(\"x\")\""),
        "a quote that closes an attribute: {page}"
    );
}

#[test]
fn a_page_asks_nothing_of_the_network() {
    let page = Html.draw(&Doc::new().with(Gist::new("it finished")));
    assert!(!page.contains("http://"), "{page}");
    assert!(!page.contains("https://"), "{page}");
    assert!(!page.contains("<link"), "{page}");
    assert!(!page.contains(" src="), "{page}");
    assert!(
        page.contains("Content-Security-Policy\" content=\"default-src 'none';"),
        "and the browser holds it to that: {page}"
    );
}

#[test]
fn a_page_carries_both_grounds_from_the_palette() {
    let page = Html.draw(&Doc::new().with(Gist::new("it finished")));
    let dark_at = page
        .find("prefers-color-scheme:dark")
        .expect("a dark ground");
    // The colors of `done` and of a keyword on a light ground, then on a dark one.
    let light = &page[..dark_at];
    let dark = &page[dark_at..];
    assert!(light.contains("--done:#2d8649"), "{light}");
    assert!(light.contains("--kw:#3d5aa6"), "{light}");
    assert!(dark.contains("--done:#8fce9b"), "{dark}");
    assert!(dark.contains("--kw:#93a9d7"), "{dark}");
    // The page stands on the ground the palette is tuned on.
    assert!(light.contains("--ground:#fbfaf6"), "{light}");
    assert!(dark.contains("--ground:#15131a"), "{dark}");
}

#[test]
fn code_takes_the_class_of_what_each_word_is_and_says_where_it_is() {
    let code = Code::whole(
        "src/greet.rs",
        Some("says goodbye".to_string()),
        "pub fn bye(name: &str) -> String;",
    )
    .within(["bye"]);
    let body = Html.body(&Doc::new().with(code));
    let caption = inside(&body, "<figcaption>", "</figcaption>");
    assert!(caption.contains("src/greet.rs › bye"), "{caption}");
    assert!(caption.contains("says goodbye"), "{caption}");
    assert!(body.contains(r#"<pre class="code">"#), "{body}");
    assert!(body.contains(r#"<span class="kw">pub</span>"#), "{body}");
    assert!(body.contains(r#"<span class="fn">bye</span>"#), "{body}");
}

#[test]
fn a_change_reads_as_inserted_words_and_taken_out_words_without_its_marks() {
    let code =
        Code::whole("src/a.rs", None, "pub fn hello(name: &str);").against(Some("pub fn hello();"));
    let body = Html.body(&Doc::new().with(code));
    let added = inside(&body, "<ins>", "</ins>");
    assert!(added.contains("name"), "{body}");
    assert!(!body.contains("{+"), "{body}");
    assert!(!body.contains("+}"), "{body}");

    let code =
        Code::whole("src/a.rs", None, "pub fn hello();").against(Some("pub fn hello(name: &str);"));
    let body = Html.body(&Doc::new().with(code));
    let removed = inside(&body, "<del>", "</del>");
    assert!(removed.contains("name"), "{body}");
    assert!(!body.contains("[-"), "{body}");
}

#[test]
fn a_composed_line_keeps_its_columns_and_its_tones() {
    let lines: Vec<Line> = vec![
        Line::new().push(Tone::Done, "done").plain("  12s"),
        Line::new()
            .push(Tone::Strong, "strong")
            .push(Tone::Command, "yunta status"),
    ];
    let body = Html.body(&Doc::new().with(Block::Lines(lines)));
    let drawn = inside(&body, r#"<pre class="lines">"#, "</pre>");
    assert!(
        drawn.contains(r#"<span class="done">done</span>  12s"#),
        "{drawn}"
    );
    assert!(drawn.contains("<b>strong</b>"), "{drawn}");
    assert!(
        drawn.contains(r#"<code class="command">yunta status</code>"#),
        "{drawn}"
    );
    assert_eq!(drawn.matches('\n').count(), 1, "{drawn}");
}

#[test]
fn a_field_that_goes_on_stands_under_the_one_it_goes_on_from() {
    let fields = Fields::new()
        .push_if("tested by", "the greeting names the person")
        .push_command("tested by", "cargo test --test greet")
        .push_literal("changes", "src/greet.rs");
    let body = Html.body(&Doc::new().with(fields));
    assert_eq!(body.matches(">tested by</dt>").count(), 1, "{body}");
    assert!(body.contains("<dt></dt>"), "{body}");
    assert!(
        body.contains(r#"data-copy="cargo test --test greet""#),
        "{body}"
    );
    assert!(body.contains("<code>src/greet.rs</code>"), "{body}");
}

mod screen;
mod written;
