//! What a run's closing gathers: the pull request it opened, the files that moved
//! most, and where the time went when a person weighed in it.

use std::time::Duration;

use surco::doc::Depth;
use surco::glyphs::Glyphs;
use surco::look::Look;
use yunta_core::events::{
    AskingOpenedPayload, ChangesCountedPayload, EngineFact, EventPayload, FileChange, GateEvent,
    NodeEvent, NodeFinishedPayload, NodeOutcome, NodeStartedPayload, RunEvent, TokenUsage,
};
use yunta_replay::TimeSpent;
use yunta_testkit_core::{fixed_now, Log};

use super::super::facts::time_row;
use super::super::RunScope;
use super::{closing_of, draw, page_of, RUN};

/// The row of `drawn` that opens on `start`, trimmed.
fn row(drawn: &str, start: &str) -> Option<String> {
    drawn
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with(start))
        .map(str::to_string)
}

/// The log of a run whose change git counted, naming `largest` of its nine files.
fn counted(largest: Vec<FileChange>) -> Vec<yunta_core::events::StoredEvent> {
    Log::for_run(RUN.as_str())
        .event(EventPayload::Run(RunEvent::ChangesCounted(
            ChangesCountedPayload {
                tree: "deadbeef".into(),
                files: 9,
                added: 412,
                removed: 87,
                largest,
            },
        )))
        .build()
}

fn two_files() -> Vec<FileChange> {
    vec![
        FileChange {
            path: "crates/cli/src/pack.rs".to_string(),
            added: 142,
            removed: 38,
        },
        FileChange {
            path: "docs/notes.md".to_string(),
            added: 38,
            removed: 0,
        },
    ]
}

#[test]
fn a_closing_lists_the_files_that_moved_most_and_what_the_rest_come_to() {
    let drawn = draw(&closing_of(counted(two_files())), &Look::plain());
    let pack = row(&drawn, "crates/cli/src/pack.rs").unwrap_or_default();
    assert!(pack.contains("+142 -38"), "{drawn}");
    let docs = row(&drawn, "docs/notes.md").unwrap_or_default();
    assert!(docs.contains("+38"), "{drawn}");
    assert_eq!(
        row(&drawn, "7 more files").as_deref(),
        Some("7 more files, +232 -49"),
        "{drawn}"
    );
    assert!(row(&drawn, "changed").is_some(), "the total stays: {drawn}");
}

/// The files are the harvest's: a glance at a run going on does not hold them,
/// and `--full` does.
#[test]
fn the_status_page_holds_the_files_only_as_deep_as_it_holds_the_change() {
    let mut page = page_of(counted(two_files()), RunScope::Status);
    assert!(!draw(&page, &Look::plain()).contains("pack.rs"));
    page.depth = Depth::Full;
    assert!(draw(&page, &Look::plain()).contains("pack.rs"));
}

/// A log older than the files says the total, and names none.
#[test]
fn a_count_with_no_files_says_its_total_and_no_more() {
    let drawn = draw(&closing_of(counted(Vec::new())), &Look::plain());
    assert!(row(&drawn, "changed").is_some(), "{drawn}");
    assert!(!drawn.contains("more files"), "{drawn}");
}

/// The log of a run whose `plan` node opened pull request 43.
fn opened() -> Vec<yunta_core::events::StoredEvent> {
    let outcome = NodeOutcome::Fact(EngineFact::PullRequestOpened {
        number: 43,
        url: "https://github.com/acme/web/pull/43".to_string(),
    });
    let finished = NodeFinishedPayload::new(outcome, TokenUsage::default());
    Log::for_run(RUN.as_str())
        .node(
            "plan",
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        )
        .node("plan", EventPayload::Node(NodeEvent::Finished(finished)))
        .build()
}

#[test]
fn a_closing_says_the_pull_request_the_run_opened() {
    let drawn = draw(&closing_of(opened()), &Look::plain());
    let said = row(&drawn, "pull request").unwrap_or_default();
    assert!(
        said.ends_with("#43: https://github.com/acme/web/pull/43"),
        "{drawn}"
    );
}

#[test]
fn time_is_said_only_where_a_person_weighed_in_it() {
    let spent = |working, people| TimeSpent {
        working: Duration::from_secs(working),
        people: Duration::from_secs(people),
        ..TimeSpent::default()
    };
    assert_eq!(
        time_row(&spent(4440, 23_220), Glyphs::Ascii).as_deref(),
        Some("1h14m at work ; 6h27m waiting on you")
    );
    assert_eq!(
        time_row(&spent(0, 60), Glyphs::Ascii).as_deref(),
        Some("1m00s waiting on you")
    );
    assert_eq!(time_row(&spent(4440, 0), Glyphs::Unicode), None);
    let moment = TimeSpent {
        people: Duration::from_millis(400),
        ..TimeSpent::default()
    };
    assert_eq!(time_row(&moment, Glyphs::Unicode), None);
}

/// A wait no person has answered yet weighs as well: the log says nothing of it
/// until they answer, so it runs to the instant the page is read at.
#[test]
fn a_wait_still_open_runs_to_the_instant_the_page_is_read_at() {
    let asked = GateEvent::AskingOpened(AskingOpenedPayload { task_id: None });
    let events = Log::for_run(RUN.as_str())
        .at(fixed_now() - chrono::Duration::seconds(90))
        .node("plan", EventPayload::Gates(asked))
        .build();
    let mut page = page_of(events, RunScope::Status);
    page.depth = Depth::Full;
    let drawn = draw(&page, &Look::plain());
    let time = row(&drawn, "time").unwrap_or_default();
    assert!(time.ends_with("1m30s waiting on you"), "{drawn}");
}
