//! What a review found, drawn on every surface and held to its goldens.

use surco::golden::{assert_golden, ENVIRONMENTS};
use surco::look::Look;
use surco::surface::{Markdown, Surface, Terminal};
use yunta_core::events::FindingSeverity;
use yunta_core::{FindingsFile, HeldEntry, HeldFindings};

use super::documents::goldens;
use crate::page::{Findings, Page};

const FINDINGS: &str = r#"
findings:
  - id: newline
    severity: major
    title: A name with a newline breaks the line
    location: src/greet.rs:4
    detail: |-
      `hello` prints the name as given, so `ana\nbob` greets on two lines.
  - id: unwrap
    severity: blocking
    title: An empty argument list panics
    location: src/main.rs:2
    detail: The first argument is read with `unwrap`.
"#;

#[test]
fn a_review_s_findings_match_their_golden() {
    let file: FindingsFile = yunta_core::yaml::parse(FINDINGS).unwrap();
    let held = HeldFindings {
        findings: file.findings.into_iter().map(HeldEntry::Written).collect(),
    };
    let doc = Findings::Held {
        held: &held,
        of: " of `review`",
    }
    .document(&Look::plain());
    for environment in &ENVIRONMENTS {
        assert_golden(
            &environment.golden(&goldens(), "findings"),
            &Terminal::on(Look::of(environment)).draw(&doc),
        );
    }
    assert_golden(&goldens().join("findings.md"), &Markdown.draw(&doc));
    assert_eq!(
        crate::page::findings::mark(FindingSeverity::Blocking),
        surco::glyphs::Mark::Failed
    );
}
