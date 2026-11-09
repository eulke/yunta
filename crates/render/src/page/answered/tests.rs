use yunta_core::{OptionId, RunId};

use surco::look::Look;
use surco::surface::{Surface, Terminal};

use super::{Answered, Resolution};
use crate::page::Page;

#[test]
fn each_gate_answer_says_how_it_ended_and_what_to_type_next() {
    let run = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
    let said = |resolution: Resolution| {
        let page = Answered {
            run_id: &run,
            resolution: &resolution,
        };
        Terminal::on(Look::plain()).draw(&page.document(&Look::plain()))
    };
    let resolved = said(Resolution::Resolved {
        option: OptionId::from_static("retry"),
    });
    assert!(
        resolved.starts_with("+ run AAAAAA ; resolved `retry`\n"),
        "{resolved}"
    );
    assert!(resolved.contains("applies it"), "{resolved}");
    assert!(resolved.contains("yunta status AAAAAA"), "{resolved}");
    let declined = said(Resolution::Declined);
    assert!(
        declined.starts_with("? run AAAAAA ; needs you\n"),
        "{declined}"
    );
    assert!(
        declined.contains("nothing recorded: it still waits on the decision."),
        "{declined}"
    );
    assert!(declined.contains("yunta resolve-gate AAAAAA"), "{declined}");
    assert_eq!(said(Resolution::Interrupted), "");
}

/// Every recorded decision is answered alike: a resume in the background
/// applies it, in the words the page and the MCP sentence share.
#[test]
fn every_answer_says_a_resume_applies_it() {
    let run = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
    let abort = OptionId::from_static("abort");
    let resolution = Resolution::Resolved {
        option: abort.clone(),
    };
    let answered = Answered {
        run_id: &run,
        resolution: &resolution,
    };
    let page = Terminal::on(Look::plain()).draw(&answered.document(&Look::plain()));
    let said = page.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        said.contains("a resume in the background applies it"),
        "{page}"
    );
    assert!(!said.contains("stays open"), "{page}");
    assert!(!said.contains("drives the run on"), "{page}");
    assert_eq!(
        Answered::sentence(&run, &abort),
        "run 01AAAAAAAAAAAAAAAAAAAAAAAA: resolved `abort`; a resume in the background \
         applies it"
    );
    assert_eq!(
        Answered::sentence(&run, &OptionId::from_static("approve")),
        "run 01AAAAAAAAAAAAAAAAAAAAAAAA: resolved `approve`; a resume in the background \
         applies it"
    );
}
