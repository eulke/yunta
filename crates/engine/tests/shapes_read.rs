//! What the tree a plan is registered against holds of the plan's shapes:
//! the engine reads it once, in the reading that derives each task's
//! reach, and states it on the log for every page that draws the plan.

use std::sync::Arc;

use yunta_adapters::{MockForge, MockForgeState};
use yunta_core::events::{
    ArtifactId, EventPayload, InTree, Outside, ScopeDerivedPayload, ScopeEvent, ShapeRead,
    ShapesReadPayload, StoredEvent, TaskEvent,
};
use yunta_core::shown::ShownContent;
use yunta_core::{ArtifactKind, CommitSha, ContentHash, NodeId};
use yunta_engine::{BirthArtifact, BirthOrigin, RunReport, RunTerminal};
use yunta_testkit::{git, git_output, Bench};

mod common;
use common::*;

/// The tree a plan is written against: `a.rs` declares `build_it`,
/// `caller.rs` and `used.rs` call it, and `fixture.rs` says its name only
/// in a comment and in a string.
const TREE: [(&str, &str); 4] = [
    ("a.rs", "pub fn build_it(depth: u32) {}"),
    ("caller.rs", "fn caller() { build_it(1); }"),
    ("used.rs", "fn used() { build_it(2); }"),
    (
        "fixture.rs",
        "// build_it\nconst PLAN: &str = r#\"- name: build_it\"#;",
    ),
];

/// A plan whose `task-h` widens `build_it` and adds `fresh`, both in
/// `a.rs`, and changes the caller in `used.rs`.
const PLAN: &str = r#"summary: "Widen build_it"
description: "build_it takes a width beside its depth."
shapes:
  - { name: build_it, owner: task-h, file: a.rs, code: "pub fn build_it(depth: u32, width: u32);" }
  - { name: fresh, owner: task-h, file: a.rs, code: "pub fn fresh();" }
tasks:
  - id: task-h
    title: "Widen build_it"
    description: "Gives build_it a width, and its caller in used.rs the width it passes."
    scope: [a.rs, used.rs]
    changes:
      - { at: "a.rs::build_it", what: "takes a width", code: "pub fn build_it(depth: u32, width: u32);" }
      - { at: used.rs, what: "passes a width", code: "fn used() { build_it(2, 1); }" }
    outcome: "build_it takes a width"
    criteria:
      - { cmd: "test -f built.txt", proves: "the width is built" }
"#;

/// A second task of [`PLAN`], which changes the caller in `caller.rs`.
const CALLER_CHANGED: &str = r#"  - id: task-c
    title: "Pass a width from caller"
    description: "Gives the caller in caller.rs the width it passes."
    scope: [caller.rs]
    changes:
      - { at: caller.rs, what: "passes a width", code: "fn caller() { build_it(1, 1); }" }
    outcome: "the caller passes a width"
    criteria:
      - { cmd: "test -f called.txt", proves: "the caller passes a width" }
"#;

/// A plan whose `task-h` gives `TOKEN`, a constant `a.rs` holds, its new
/// value.
const ROTATED: &str = r#"summary: "Rotate the token"
description: "TOKEN takes its new value."
shapes:
  - { name: TOKEN, owner: task-h, file: a.rs, code: "pub const TOKEN: &str = \"rotated\";" }
tasks:
  - id: task-h
    title: "Rotate the token"
    description: "Gives TOKEN its new value."
    scope: [a.rs]
    changes:
      - { at: "a.rs::TOKEN", what: "takes its new value", code: "pub const TOKEN: &str = \"rotated\";" }
    outcome: "TOKEN is rotated"
    criteria:
      - { cmd: "test -f rotated.txt", proves: "the token is rotated" }
"#;

/// A planner and nothing after it.
const PLANNED: &str = r#"
name: planned
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
"#;

/// A planner, and a gate that shows its plan and may send it back.
const REVIEWED: &str = r#"
name: reviewed
nodes:
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: approve-plan
    kind: gate
    assignee: lead
    message: "Plan registered. Approve?"
    options: [approve, adjust]
    on: { adjust: plan }
    shows: [{ node: plan, kind: tasks }]
"#;

/// A planner, and a gate on a forge that publishes its plan.
const PUBLISHED: &str = r#"
name: published
nodes:
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: approve-plan
    kind: gate
    assignee: lead
    message: "Is this plan the one to build?"
    depends_on: [plan]
    external:
      kind: pull_request
      artifacts: [tasks]
      branch: "{{run.branch}}"
"#;

/// Commits each of `files`, a path and its text, into the bench's
/// repository, and answers with the commit that holds them.
fn holding<'a>(bench: &Bench, files: impl IntoIterator<Item = (&'a str, &'a str)>) -> CommitSha {
    for (path, text) in files {
        std::fs::write(bench.worktree.join(path), text).unwrap();
    }
    git(&bench.worktree, &["add", "-A"]);
    git(&bench.worktree, &["commit", "-q", "-m", "tree"]);
    git_output(&bench.worktree, &["rev-parse", "HEAD"])
        .parse()
        .unwrap()
}

/// Every reading of a plan's shapes the run stated, oldest first, with the
/// node each was stated under.
fn readings(bench: &Bench) -> Vec<(Option<NodeId>, ShapesReadPayload)> {
    bench
        .events()
        .into_iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Tasks(TaskEvent::ShapesRead(read))) => {
                Some((event.node_id.clone(), read.clone()))
            }
            _ => None,
        })
        .collect()
}

/// The one reading the run stated.
fn the_reading(bench: &Bench) -> ShapesReadPayload {
    match &readings(bench)[..] {
        [(_, read)] => read.clone(),
        other => panic!("one reading of the plan's shapes: {other:#?}"),
    }
}

/// The first reach the log states, which in these plans is `task-h`'s: the
/// one task that owns a shape.
fn derivation(bench: &Bench) -> Option<ScopeDerivedPayload> {
    bench
        .events()
        .into_iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Scope(ScopeEvent::Derived(derived))) => Some(derived.clone()),
            _ => None,
        })
}

/// The hash of the last tasks document `producer` accepted.
fn accepted_plan(bench: &Bench, producer: Option<&str>) -> ContentHash {
    let tasks = ArtifactId::Interpreted {
        kind: ArtifactKind::Tasks,
    };
    bench
        .accepted()
        .into_iter()
        .rev()
        .find(|held| {
            held.artifact == tasks && held.producer.as_ref().map(NodeId::as_str) == producer
        })
        .expect("the plan was accepted")
        .content_hash
}

/// `build_it` as the tree holds it, with whoever else names it.
fn widened(outside: Outside) -> ShapeRead {
    ShapeRead {
        name: "build_it".to_string(),
        owner: "task-h".into(),
        file: "a.rs".to_string(),
        in_tree: InTree::There {
            code: Some("pub fn build_it(depth: u32);".to_string()),
            outside,
        },
    }
}

/// `fresh`, which no file of the tree names.
fn fresh() -> ShapeRead {
    ShapeRead {
        name: "fresh".to_string(),
        owner: "task-h".into(),
        file: "a.rs".to_string(),
        in_tree: InTree::New,
    }
}

#[tokio::test]
async fn a_plan_s_shapes_are_read_in_the_tree_it_is_registered_against() {
    let bench = Bench::new();
    let head = holding(&bench, TREE);

    bench.run(PLANNED, &plan_session(PLAN)).await;

    let caller = vec!["caller.rs".to_string()];
    assert_eq!(
        the_reading(&bench),
        ShapesReadPayload {
            plan: accepted_plan(&bench, Some("plan")),
            at: head,
            shapes: vec![widened(Outside::Files { paths: caller }), fresh()],
        }
    );
}

#[tokio::test]
async fn a_caller_a_task_changes_is_not_implied() {
    let bench = Bench::new();
    holding(&bench, TREE);

    bench
        .run(PLANNED, &plan_session(&format!("{PLAN}{CALLER_CHANGED}")))
        .await;

    let read = the_reading(&bench);
    let nobody = Outside::Files { paths: Vec::new() };
    assert_eq!(read.shape("build_it"), Some(&widened(nobody)));
}

/// A file with a NUL in its head is bytes, whatever its extension says:
/// the name it carries calls nothing.
#[tokio::test]
async fn a_binary_file_that_carries_a_name_neither_reaches_nor_is_implied() {
    let bench = Bench::new();
    let bytes = ("bin.rs", "fn bin() { build_it(3); }\0");
    holding(&bench, TREE.into_iter().chain([bytes]));

    bench.run(PLANNED, &plan_session(PLAN)).await;

    let caller = vec!["caller.rs".to_string()];
    let shapes = [widened(Outside::Files { paths: caller }), fresh()];
    assert_eq!(the_reading(&bench).shapes, shapes);
    let derived = derivation(&bench).expect("the caller is on the log");
    assert_eq!(derived.paths, [yunta_core::ScopeGlob::from("caller.rs")]);
}

/// A shape whose file is bytes is new there, whatever the bytes say.
#[tokio::test]
async fn a_shape_whose_file_is_binary_is_new() {
    let bench = Bench::new();
    let binary = ("a.rs", "pub fn build_it(depth: u32) {}\0");
    let tree = std::iter::once(binary).chain(TREE.into_iter().skip(1));
    holding(&bench, tree);

    bench.run(PLANNED, &plan_session(PLAN)).await;

    let read = the_reading(&bench);
    let new = ShapeRead {
        name: "build_it".to_string(),
        ..fresh()
    };
    assert_eq!(read.shapes, [new, fresh()]);
}

#[tokio::test]
async fn a_name_too_common_to_follow_is_counted() {
    let bench = Bench::new();
    let callers: Vec<String> = (1..=21).map(|n| format!("caller{n}.rs")).collect();
    let calling = callers
        .iter()
        .map(|caller| (caller.as_str(), "fn caller() { build_it(1); }"));
    holding(&bench, std::iter::once(TREE[0]).chain(calling));

    bench.run(PLANNED, &plan_session(PLAN)).await;

    let read = the_reading(&bench);
    let common = Outside::Common { files: 22 };
    assert_eq!(read.shape("build_it"), Some(&widened(common)));
    let derived = derivation(&bench).expect("the common name is on the log");
    assert_eq!(derived.common, ["build_it"]);
}

#[tokio::test]
async fn a_plan_read_again_at_the_same_commit_is_stated_once() {
    let bench = Bench::new();
    holding(&bench, TREE);
    let again = format!(
        "{}{}",
        plan_session(PLAN),
        tasks_session(PLAN, "planned again")
    );
    let interaction =
        SequencedInteraction::answering(vec![("adjust", Some("read it again")), ("approve", None)]);

    let RunReport { terminal, .. } = bench
        .run_with_interaction(REVIEWED, &again, &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let registered = bench
        .events()
        .iter()
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Tasks(TaskEvent::Registered(_)))
            )
        })
        .count();
    assert_eq!(registered, 2, "the plan is registered at each handover");
    assert_eq!(readings(&bench).len(), 1, "{:#?}", readings(&bench));
}

/// The log keeps a secret the tree holds out of what it states, and the
/// reading it holds is compared in that form: the plan read again at the
/// same commit is stated once, and so is its reach.
#[tokio::test]
async fn a_reading_a_secret_stands_in_is_stated_once() {
    const SECRET: &str = "hunter2-the-whole-token";
    let bench = Bench::new();
    let token = format!("pub const TOKEN: &str = \"{SECRET}\";");
    let user = format!("uses-{SECRET}.rs");
    let tree = [
        ("a.rs", token.as_str()),
        (&user, "fn uses() -> u8 { TOKEN }"),
    ];
    holding(&bench, tree);
    let again = plan_session(ROTATED) + &tasks_session(ROTATED, "again");
    let interaction =
        SequencedInteraction::answering(vec![("adjust", Some("read it again")), ("approve", None)]);
    let config = yunta_testkit::MOCK_CONFIG.to_string() + "\nsecrets: [YUNTA_TEST_TOKEN]\n";
    let secrets = [("YUNTA_TEST_TOKEN", SECRET)];

    bench
        .run_with_secrets_answering(REVIEWED, &again, &config, &secrets, &interaction)
        .await;

    let events = bench.events();
    let stated = |kind: &str| {
        let of_kind =
            |event: &&StoredEvent| event.payload().map(EventPayload::kind_name) == Some(kind);
        events.iter().filter(of_kind).count()
    };
    let counts = ["task_registered", "shapes_read", "scope_derived"].map(stated);
    assert_eq!(counts, [2, 1, 1], "{events:#?}");
    let redacted = yunta_core::REDACTED;
    let paths = vec![format!("uses-{redacted}.rs")];
    let held = InTree::There {
        code: Some(format!("pub const TOKEN: &str = \"{redacted}\";")),
        outside: Outside::Files { paths },
    };
    let read = the_reading(&bench);
    assert_eq!(read.shape("TOKEN").map(|shape| &shape.in_tree), Some(&held));
}

/// The plan a gate shows carries what the tree held of its shapes, as the
/// log states it for the bytes the gate shows.
#[tokio::test]
async fn a_gate_shows_its_plan_with_what_the_tree_held_of_its_shapes() {
    let bench = Bench::new();
    holding(&bench, TREE);
    let interaction = SequencedInteraction::choosing(&["approve"]);

    bench
        .run_with_interaction(REVIEWED, &plan_session(PLAN), &interaction)
        .await;

    let shown = interaction.shown();
    let [documents] = &shown[..] else {
        panic!("the gate asked once: {shown:#?}");
    };
    let [document] = &documents[..] else {
        panic!("the gate shows the plan: {documents:#?}");
    };
    let ShownContent::Tasks(review) = &document.content else {
        panic!("a plan is shown as its tasks: {document:#?}");
    };
    assert_eq!(review.shapes_read, Some(the_reading(&bench)));
}

#[tokio::test]
async fn a_document_a_run_is_born_with_is_read_under_no_node() {
    let document: yunta_core::TasksFile = yunta_core::shape::read(PLAN.as_bytes(), "plan").unwrap();
    let bench = Bench::new().born_holding(vec![BirthArtifact {
        artifact: ArtifactId::Interpreted {
            kind: ArtifactKind::Tasks,
        },
        origin: BirthOrigin::Input {
            input: "plan".into(),
        },
        bytes: yunta_core::shape::render(&document).unwrap().into_bytes(),
    }]);
    holding(&bench, TREE);

    bench
        .birth(&no_scope_expansion_workflow())
        .await
        .expect("a run is born holding the plan");

    let stated: Vec<(Option<NodeId>, ContentHash)> = readings(&bench)
        .into_iter()
        .map(|(node, read)| (node, read.plan))
        .collect();
    assert_eq!(stated, [(None, accepted_plan(&bench, None))]);
}

/// What one line of the git stub's log asked git, past the settings put
/// ahead of it: the subcommand, and a `cat-file`'s first argument.
fn subcommand(line: &str) -> String {
    let mut words = line.split_whitespace();
    let mut first = words.next();
    while first == Some("-c") {
        words.next();
        first = words.next();
    }
    match first {
        Some("cat-file") => format!("cat-file {}", words.next().unwrap_or_default()),
        other => other.unwrap_or_default().to_string(),
    }
}

#[tokio::test]
async fn registration_asks_git_once_for_names_and_once_for_files() {
    let stubs = tempfile::tempdir().unwrap();
    let log = stubs.path().join("git.log");
    let bench =
        Bench::new().with_subprocess_vars(yunta_testkit::stubs::git_counting(stubs.path(), &log));
    holding(&bench, TREE);

    bench.run(PLANNED, &plan_session(PLAN)).await;

    let logged = tokio::fs::read_to_string(&log).await.unwrap_or_default();
    let asked: Vec<String> = logged.lines().map(subcommand).collect();
    let times = |what: &str| asked.iter().filter(|said| *said == what).count();
    assert_eq!(
        [times("grep"), times("cat-file --batch"), times("show")],
        [1, 1, 0],
        "{asked:#?}"
    );
}

#[tokio::test]
async fn a_published_plan_says_what_its_shapes_imply() {
    let forge = MockForgeState::new();
    let bench = Bench::new().with_forge(Arc::new(MockForge::new(forge.clone())));
    holding(&bench, TREE);

    bench.run(PUBLISHED, &plan_session(PLAN)).await;

    let published = forge.pull_requests().remove(0);
    let path = format!(".yunta/gates/{}/approve-plan/tasks.md", bench.run_id);
    let drawn = String::from_utf8(published.files[path.as_str()].clone()).unwrap();
    for said in [
        "- implies: build_it is named in 1 file no task changes:  \n  caller.rs",
        "pub fn build_it(depth: u32, {+width: u32+});",
        "_modified · 1 line_",
        "_new · 1 line_",
    ] {
        assert!(
            drawn.contains(said),
            "the published plan says `{said}`:\n{drawn}"
        );
    }
}
