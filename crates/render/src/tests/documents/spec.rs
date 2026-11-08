//! A spec's page, shown without its plan: each task's tests, cut where a
//! file runs long, with where the rest is.

use surco::look::Look;
use surco::surface::{Surface, Terminal};
use yunta_core::SpecFile;

use super::{run, SPEC};
use crate::page::{Form, Page, Spec};

/// A spec's page shows a long file to its cut, and where the rest is: the
/// page of the node that wrote it. With no node, the file is shown whole.
#[test]
fn a_spec_page_cuts_a_long_file_and_says_where_the_rest_is() {
    let mut spec: SpecFile = yunta_core::yaml::parse(SPEC).unwrap();
    spec.specs[0].files[0].content = (1..=80).map(|n| format!("// line {n}\n")).collect();
    let node: yunta_core::NodeId = "specify".into();
    let draw = |node: Option<&yunta_core::NodeId>| {
        let page = Spec {
            file: &spec,
            of: "",
            producer: node,
            run: &run(),
            form: Form::Review,
        };
        let doc = page.document(&Look::plain());
        Terminal::on(Look::plain()).draw(&doc)
    };
    let cut = draw(Some(&node));
    assert!(
        cut.contains("// line 60") && !cut.contains("// line 61"),
        "{cut}"
    );
    assert!(
        cut.contains("20 lines more: yunta status 7E5PH4 --node specify"),
        "{cut}"
    );
    assert!(draw(None).contains("// line 80"), "no node, the file whole");
}
