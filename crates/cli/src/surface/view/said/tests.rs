//! The row a working node gets for what its session said last.

use yunta_core::{NodeId, SessionId, TaskId};
use yunta_engine::said::Said;
use yunta_testkit::run_frame;

use crate::surface::view::{node_rows, NodeFrame, NodeStanding, RunId, Standing};
use surco::glyphs::Glyphs;
use surco::look::Look;
use surco::motion::Motion;
use surco::paint::Paint;

const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");

fn look() -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: surco::look::Width::row(120, None),
    }
}

/// What a session of `implement` said, the task's when it names one.
fn said(task: Option<&str>, text: &str) -> Said {
    Said {
        run_id: RUN,
        node_id: NodeId::from("implement"),
        session_id: SessionId::from_static("session-1"),
        task_id: task.map(TaskId::from),
        seq: yunta_core::Seq::from(7),
        at: chrono::DateTime::UNIX_EPOCH,
        text: text.to_string(),
        cut: false,
    }
}

/// `implement` at work on T001 in a session.
fn working() -> NodeFrame {
    let mut node = yunta_testkit::node_frame(&NodeId::from("implement"), NodeStanding::ToGo);
    node.standing = Standing::Running;
    node.running_tasks = vec![yunta_engine::RunningTask {
        id: "T001".into(),
        in_session: true,
    }];
    node
}

fn rows(node: &NodeFrame, said: Option<&Said>) -> Vec<String> {
    let frame = run_frame(&RUN);
    node_rows(&frame, node, said, &look(), Motion::Still)
        .iter()
        .map(|row| Paint::Plain.line(row))
        .collect()
}

#[test]
fn a_working_node_shows_the_first_sentence_of_what_its_session_said() {
    let said = said(
        Some("T001"),
        "I found the bug in the parser. Adding a test now.",
    );

    let rows = rows(&working(), Some(&said));

    let tasks = rows
        .iter()
        .position(|row| row.trim_start().starts_with("tasks running:"))
        .unwrap_or_else(|| panic!("{rows:?}"));
    assert_eq!(
        rows.get(tasks + 1).map(|row| row.trim()),
        Some("T001 says: I found the bug in the parser."),
        "{rows:?}"
    );
}

#[test]
fn a_session_that_works_no_task_says_without_one() {
    let said = said(None, "Reviewing the diff.");

    let rows = rows(&working(), Some(&said));

    assert!(
        rows.iter()
            .any(|row| row.trim() == "says: Reviewing the diff."),
        "{rows:?}"
    );
}

#[test]
fn a_node_whose_sessions_said_nothing_shows_no_row() {
    let rows = rows(&working(), None);

    assert!(!rows.iter().any(|row| row.contains("says:")), "{rows:?}");
}

/// A session writes Markdown and may write anything a terminal would act
/// on. The row is one plain sentence of it, and no control character it
/// wrote reaches the terminal.
#[test]
fn what_a_session_writes_reads_as_one_plain_sentence() {
    let said = said(
        Some("T001"),
        "## Plan\n\n**First** I read the \u{7}lexer\u{8} files.\nThen the rest.",
    );

    let rows = rows(&working(), Some(&said));

    let row = rows
        .iter()
        .find(|row| row.contains("says:"))
        .unwrap_or_else(|| panic!("{rows:?}"));
    assert!(!row.chars().any(char::is_control), "{row:?}");
    assert!(row.contains("First I read the"), "{row:?}");
    assert!(!row.contains("Then the rest"), "{row:?}");
    assert!(!row.contains("##"), "{row:?}");
}
