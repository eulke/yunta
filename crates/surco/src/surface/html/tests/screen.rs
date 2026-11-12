//! A screen's blocks as a page: each says every word it holds, and a key is a
//! button that carries it.

use crate::components::Legend;
use crate::doc::Doc;
use crate::surface::Html;

/// Each key of a legend is a button carrying the key as a program names it,
/// beside what it does, inert on a static page.
#[test]
fn a_legend_draws_each_key_as_a_button_with_its_key() {
    let doc = Doc::new().with(Legend::new().arrows("move").key("enter", "open"));

    let page = Html.body(&doc);

    assert!(
        page.contains(r#"<button type="button" class="key" data-key="up-down">↑↓</button> move"#),
        "{page}"
    );
    assert!(
        page.contains(r#"<button type="button" class="key" data-key="enter">enter</button> open"#),
        "{page}"
    );
}
