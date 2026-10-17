//! A task may write every file that names a shape it owns: the engine
//! reads which files those are in the run's tree when the plan is
//! registered, so the owner of a signature reaches its callers without
//! anyone listing them or granting them.

use yunta_core::events::{EventPayload, ScopeDerivedPayload, ScopeEvent, TaskStatus};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{git, Bench};

mod common;
use common::*;

/// A plan whose one task owns `build_it`, declared in `a.rs`, and may
/// write only that file; its session writes `a.rs` and `writes`.
fn owning(writes: &str) -> (String, String) {
    let mut fixture = plan_session(
        "shapes:\n  - name: build_it\n    owner: task-h\n    file: a.rs\n    code: \"pub fn build_it(depth: u32)\"\n\
         tasks:\n  - id: task-h\n    title: \"h\"\n    scope: [\"a.rs\"]\n    criteria:\n      - cmd: \"test -f a.rs\"\n",
    );
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-h\"\n    effects:\n      - {{ path: a.rs, content: \"pub fn build_it(depth: u32) {{}}\" }}\n{writes}    outcome: {{ type: completed, summary: did-h }}\n"
    ));
    (no_scope_expansion_workflow(), fixture)
}

/// Commits `files` into the bench's repository, each calling `build_it`.
fn naming(bench: &Bench, files: &[String]) {
    holding(bench, files, "fn caller() { build_it(); }\n");
}

/// Commits `files` into the bench's repository, each holding `source`.
fn holding(bench: &Bench, files: &[String], source: &str) {
    for file in files {
        std::fs::write(bench.worktree.join(file), source).unwrap();
    }
    git(&bench.worktree, &["add", "-A"]);
    git(&bench.worktree, &["commit", "-q", "-m", "callers"]);
}

fn derivation(bench: &Bench) -> Option<ScopeDerivedPayload> {
    bench
        .events()
        .into_iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Scope(ScopeEvent::Derived(derived))) => Some(derived.clone()),
            _ => None,
        })
}

#[tokio::test]
async fn owner_may_write_a_caller_outside_its_declared_scope() {
    let bench = Bench::new();
    naming(&bench, &["caller.rs".to_string()]);
    let (workflow, fixture) =
        owning("      - { path: caller.rs, content: \"fn caller() { build_it(1); }\" }\n");

    let RunReport { terminal, state } = bench.run(&workflow, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("task-h"), Some(TaskStatus::Done));
    let derived = derivation(&bench).expect("the owner's reach is on the log");
    assert_eq!(derived.paths, [yunta_core::ScopeGlob::from("caller.rs")]);
    assert_eq!(derived.shapes, ["build_it"]);
}

#[tokio::test]
async fn a_common_name_derives_nothing() {
    let bench = Bench::new();
    let files: Vec<String> = (1..=21).map(|n| format!("caller{n}.rs")).collect();
    naming(&bench, &files);
    let (workflow, fixture) = owning("");

    bench.run(&workflow, &fixture).await;

    let derived = derivation(&bench).expect("the common name is on the log");
    assert!(derived.paths.is_empty(), "{:?}", derived.paths);
    assert_eq!(derived.common, ["build_it"]);
}

#[tokio::test]
async fn a_name_only_a_string_or_a_comment_says_derives_nothing() {
    let bench = Bench::new();
    let fixture = "// build_it\nconst PLAN: &str = r#\"- name: build_it\"#;\n";
    holding(&bench, &["fixture.rs".to_string()], fixture);
    let (workflow, fixture) = owning("");

    bench.run(&workflow, &fixture).await;

    assert_eq!(derivation(&bench), None);
}
