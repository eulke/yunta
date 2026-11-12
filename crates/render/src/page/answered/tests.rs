use yunta_core::{NodeId, OptionId, RunId};

use surco::look::Look;
use surco::surface::{Surface, Terminal};

use super::{GateAnswered, QuestionsAnswered, Resolution, RoundAnswer};
use crate::page::Page;

#[test]
fn each_gate_answer_says_how_it_ended_and_what_to_type_next() {
    let run = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
    let said = |resolution: Resolution| {
        let page = GateAnswered {
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
    let answered = GateAnswered {
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
        GateAnswered::sentence(&run, &abort),
        "run 01AAAAAAAAAAAAAAAAAAAAAAAA: resolved `abort`; a resume in the background \
         applies it"
    );
    assert_eq!(
        GateAnswered::sentence(&run, &OptionId::from_static("approve")),
        "run 01AAAAAAAAAAAAAAAAAAAAAAAA: resolved `approve`; a resume in the background \
         applies it"
    );
}

/// A round answered says which node it answered and that a resume carries the run
/// on; one left says nothing was recorded and how to be asked again; and off a
/// terminal, where they are asked.
#[test]
fn each_round_answer_says_how_it_ended_and_what_to_type_next() {
    let run = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
    let node = NodeId::from_static("ask");
    let said = |answer: RoundAnswer| {
        let page = QuestionsAnswered {
            run_id: &run,
            answer: &answer,
        };
        Terminal::on(Look::plain()).draw(&page.document(&Look::plain()))
    };
    let answered = said(RoundAnswer::Answered { node: node.clone() });
    assert!(
        answered.starts_with("+ run AAAAAA ; answered node `ask`\n"),
        "{answered}"
    );
    assert!(answered.contains("carries the run on"), "{answered}");
    let declined = said(RoundAnswer::Declined { node: node.clone() });
    assert!(
        declined.contains("the questions node `ask` asked"),
        "{declined}"
    );
    assert!(declined.contains("yunta resolve-gate AAAAAA"), "{declined}");
    assert!(said(RoundAnswer::Interrupted).is_empty());
    let off = QuestionsAnswered::off_terminal(&run, &node);
    assert!(
        off.contains("`yunta resolve-gate AAAAAA` asks them"),
        "{off}"
    );
}
