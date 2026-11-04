//! What a run's commits say, written by hand: a person reads these in `git log`.

use std::path::PathBuf;

use super::*;

fn run() -> RunId {
    RunId::from_static("01M6HGH9ZFBZPVTCDFKXH29WN3")
}

fn node(id: &str) -> NodeId {
    NodeId::from(id)
}

fn task(id: &'static str) -> TaskId {
    TaskId::from_static(id)
}

fn paths(paths: &[&str]) -> Vec<PathBuf> {
    paths.iter().map(PathBuf::from).collect()
}

/// The first line of what `message` records.
fn subject(message: &CommitMessage) -> String {
    message
        .text()
        .lines()
        .next()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn the_trailers_close_the_message_in_one_paragraph() {
    let message = CommitMessage::of_failed_node(&run(), &node("build"), 2, &["`a`".to_string()])
        .task(&task("T001"));
    assert_eq!(
        message.text(),
        "node build: what attempt 2 left when it failed\n\n\
         Holds what `a` left in the run's tree while this node worked.\n\n\
         Yunta-Run: 01M6HGH9ZFBZPVTCDFKXH29WN3\nYunta-Node: build\nYunta-Task: T001"
    );
}

#[test]
fn a_task_says_what_it_does() {
    let message = CommitMessage::of_task(&run(), &node("implement"), &task("T001"), "Write a");
    assert_eq!(
        message.text(),
        "task T001: Write a\n\n\
         Yunta-Run: 01M6HGH9ZFBZPVTCDFKXH29WN3\nYunta-Node: implement\nYunta-Task: T001"
    );
}

/// A title is one line whatever it holds: a line break would part the subject.
#[test]
fn a_subject_is_one_line() {
    let message = CommitMessage::of_task(
        &run(),
        &node("implement"),
        &task("T001"),
        "Write\n  a  file\n",
    );
    assert_eq!(subject(&message), "task T001: Write a file");
    assert!(!message.text().contains("  a"), "{}", message.text());
}

/// What does not fit a subject is said whole below it, and the subject stops at a word.
#[test]
fn a_long_subject_stops_at_a_word_and_is_said_whole_below() {
    let title = "Store packs and locks at a selected install scope, and read them back from there";
    let message = CommitMessage::of_task(&run(), &node("implement"), &task("T001"), title);
    let first = subject(&message);
    assert_eq!(
        first,
        "task T001: Store packs and locks at a selected install scope, and..."
    );
    assert!(first.chars().count() <= SUBJECT_MOST, "{first}");
    assert!(
        message
            .text()
            .contains(&format!("\n\ntask T001: {title}\n\n")),
        "{}",
        message.text()
    );
}

/// One word longer than a subject has no word to stop at.
#[test]
fn a_subject_with_no_word_to_stop_at_is_cut_where_it_passes() {
    let long = "x".repeat(120);
    let message = CommitMessage::of_task(&run(), &node("implement"), &task("T001"), &long);
    assert_eq!(subject(&message).chars().count(), SUBJECT_MOST);
    assert!(subject(&message).ends_with("..."));
}

/// A node says what it declares it does, and with nothing declared what it changed: what
/// the index holds, read after it was staged.
#[test]
fn a_node_says_what_it_declares_or_what_it_changed() {
    let said = |description: Option<&str>, changed: &[&str]| {
        subject(&CommitMessage::of_node(
            &run(),
            &node("fix-lint"),
            description,
            &paths(changed),
            &[],
        ))
    };
    assert_eq!(
        said(Some("Applies what lint asked for"), &["a.rs"]),
        "node fix-lint: Applies what lint asked for"
    );
    assert_eq!(
        said(None, &["crates/cli/src/pack.rs"]),
        "node fix-lint: changes crates/cli/src/pack.rs"
    );
    assert_eq!(
        said(None, &["a.rs", "b.rs"]),
        "node fix-lint: changes a.rs and b.rs"
    );
    assert_eq!(
        said(
            None,
            &[
                "crates/cli/src/a.rs",
                "crates/cli/src/b.rs",
                "crates/cli/src/c/d.rs"
            ]
        ),
        "node fix-lint: changes 3 files under crates/cli/src/"
    );
    assert_eq!(
        said(None, &["a.rs", "docs/b.md", "tests/c.rs"]),
        "node fix-lint: changes 3 files"
    );
    assert_eq!(said(Some("  "), &["a.rs"]), "node fix-lint: changes a.rs");
    assert_eq!(said(None, &[]), "node fix-lint");
}

/// Two paths too long to name stand as their count.
#[test]
fn paths_that_do_not_fit_a_subject_are_counted() {
    let message = CommitMessage::of_node(
        &run(),
        &node("write"),
        None,
        &paths(&[
            "crates/engine/src/run/loop_exec/integrate/member.rs",
            "crates/engine/src/run/loop_exec/integrate/other.rs",
        ]),
        &[],
    );
    assert_eq!(subject(&message), "node write: changes 2 files");
}

/// A commit that holds what other nodes left says whose.
#[test]
fn a_node_that_holds_what_others_left_says_so_below() {
    let message = CommitMessage::of_node(
        &run(),
        &node("second"),
        None,
        &paths(&["two.txt"]),
        &["`first`".to_string(), "`third`".to_string()],
    );
    assert_eq!(
        message.text(),
        "node second: changes two.txt\n\n\
         Holds what `first`, `third` left in the run's tree while this node worked.\n\n\
         Yunta-Run: 01M6HGH9ZFBZPVTCDFKXH29WN3\nYunta-Node: second"
    );
}

/// What an attempt left is said one way, whatever stopped it.
#[test]
fn what_an_attempt_left_is_said_one_way() {
    assert_eq!(
        subject(&CommitMessage::of_left_task(
            &run(),
            &node("implement"),
            &task("T001")
        )),
        "task T001: what its last attempt left"
    );
    assert_eq!(
        subject(&CommitMessage::of_failed_node(
            &run(),
            &node("build"),
            3,
            &[]
        )),
        "node build: what attempt 3 left when it failed"
    );
    let found = CommitMessage::of_found(&run(), &node("build"), Some(3));
    assert_eq!(
        found.text(),
        "found in the run's tree before node build started\n\n\
         Holds what attempt 3 left when it was interrupted.\n\n\
         Yunta-Run: 01M6HGH9ZFBZPVTCDFKXH29WN3\nYunta-Node: build"
    );
    assert_eq!(
        subject(&CommitMessage::of_found(&run(), &node("build"), None)),
        "found in the run's tree before node build started"
    );
}

#[test]
fn a_unit_s_first_tree_says_whose_it_is() {
    assert_eq!(
        subject(&CommitMessage::of_unit_start(&run(), &node("build"))),
        "the tree node build started from"
    );
}

/// A person reads a run by its handle: the whole id is in the trailer.
#[test]
fn the_distilled_knowledge_names_the_run_by_its_handle() {
    let message = CommitMessage::of_distilled(&run());
    assert_eq!(
        message.text(),
        format!(
            "docs(knowledge): distill from {}\n\nYunta-Run: 01M6HGH9ZFBZPVTCDFKXH29WN3",
            run().handle()
        )
    );
}

/// One commit a file, each saying which.
#[test]
fn what_a_gate_publishes_names_the_file_and_the_gate() {
    let message =
        CommitMessage::of_published(&run(), &node("approve-plan"), ".yunta/gates/plan.md");
    assert_eq!(
        message.text(),
        "publish .yunta/gates/plan.md for gate approve-plan\n\n\
         Yunta-Run: 01M6HGH9ZFBZPVTCDFKXH29WN3\nYunta-Node: approve-plan"
    );
}

/// A line of a title that reads as a trailer stays out of the last paragraph.
#[test]
fn a_title_that_reads_as_a_trailer_stays_in_the_subject() {
    let message = CommitMessage::of_task(
        &run(),
        &node("implement"),
        &task("T001"),
        "Yunta-Run: elsewhere",
    );
    assert_eq!(subject(&message), "task T001: Yunta-Run: elsewhere");
    assert_eq!(
        message.text().matches("\nYunta-Run: ").count(),
        1,
        "{}",
        message.text()
    );
}
