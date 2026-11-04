//! What a person wrote and what a diagram says, as a page draws them.

use super::Html;
use crate::diagram::Diagram;
use crate::doc::{Block, Doc};

#[test]
fn what_was_written_by_hand_reads_as_a_page() {
    let body = Html.body(&Doc::new().with(Block::Markdown(
        "# A title\n\nA **strong** word and `code`.\n\n- one\n- two\n\n```rust\nfn main() {}\n```\n\n> quoted\n".to_string(),
    )));
    assert!(body.contains(">A title</h"), "{body}");
    assert!(body.contains("<b>strong</b>"), "{body}");
    assert!(
        body.contains(r#"<code class="command">code</code>"#),
        "its backticks come off: {body}"
    );
    assert!(body.contains("<li>one</li>"), "{body}");
    assert!(body.contains("<li>two</li>"), "{body}");
    assert!(body.contains(r#"<pre class="code">"#), "{body}");
    assert!(body.contains(r#"<span class="kw">fn</span>"#), "{body}");
    assert!(body.contains("<blockquote>"), "{body}");
    assert!(body.contains("quoted"), "{body}");
}

#[test]
fn a_diagram_in_what_was_written_is_drawn_too() {
    let body = Html.body(&Doc::new().with(Block::Markdown(
        "before\n\n```mermaid\nflowchart LR\n  A[plan] --> B[build]\n```\n\nafter\n".to_string(),
    )));
    assert!(body.contains("<svg"), "{body}");
    assert!(body.contains(">plan</text>"), "{body}");
    assert!(body.contains("before"), "{body}");
    assert!(body.contains("after"), "{body}");
}

#[test]
fn a_diagram_it_can_read_is_drawn_with_its_words_and_one_it_cannot_is_its_source() {
    let body = Html.body(&Doc::new().with(Diagram::of(
        "flowchart LR\n  A[plan] --> B[build]\n  B -->|fails| A\n",
    )));
    assert!(body.contains(r#"<svg class="flow""#), "{body}");
    assert!(body.contains(">plan</text>"), "{body}");
    assert!(body.contains(">build</text>"), "{body}");
    assert!(body.contains(">fails</text>"), "{body}");
    assert!(
        body.contains(r#"aria-label="plan leads to build, build leads to plan (fails)""#),
        "{body}"
    );

    let body = Html.body(&Doc::new().with(Diagram::of(
        "flowchart LR\n  A[plan] --> B[build]\n  C[idle]\n",
    )));
    assert!(
        body.contains(r#"aria-label="plan leads to build, idle""#),
        "a box no link touches is said too: {body}"
    );

    let source = "sequenceDiagram\n  A->>B: hello <there>\n";
    let body = Html.body(&Doc::new().with(Diagram::of(source)));
    assert!(!body.contains("<svg"), "{body}");
    assert!(body.contains(r#"<pre class="lines">"#), "{body}");
    assert!(body.contains("hello &lt;there&gt;"), "{body}");
}
