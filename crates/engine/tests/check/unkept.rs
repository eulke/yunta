//! What `check` refuses because a run would not do what it says: two gates that
//! share a pull request, two files that land on one path, and what a child of a
//! group declares that nothing reads.

use super::*;
use yunta_core::{ArtifactKind, ArtifactSpec};

/// An external gate on `branch`, publishing `artifacts`.
fn published(id: &str, branch: &str, artifacts: Vec<ArtifactSpec>) -> Node {
    let mut node = gate(id, &[]);
    let NodeKind::Gate { external, .. } = &mut node.kind else {
        panic!("`gate` makes a gate");
    };
    *external = Some(yunta_core::ExternalGate {
        kind: yunta_core::ForgeKind::PullRequest,
        artifacts,
        branch: branch.to_string(),
    });
    node
}

/// A forge opens one pull request per branch, and a run finds it by its own marker, so
/// two gates on one branch share it: an approval would resolve them both.
#[test]
fn two_external_gates_on_one_branch_are_refused() {
    let wf = workflow(vec![
        published("first", "{{run.branch}}", Vec::new()),
        published("second", "{{run.branch}}", Vec::new()),
    ]);
    assert_eq!(
        check(&wf, &config_with_forge()),
        vec![CheckError::ExternalGatesShareBranch {
            first: "first".into(),
            second: "second".into(),
            branch: "{{run.branch}}".to_string(),
        }]
    );
}

/// Two gates whose templates come to one branch in a run share it however they are
/// written, and a branch named after its gate is that gate's own.
#[test]
fn a_branch_is_compared_as_a_run_would_render_it() {
    let own = workflow(vec![
        published("first", "review/{{node.id}}", Vec::new()),
        published("second", "review/{{node.id}}", Vec::new()),
    ]);
    assert_eq!(check(&own, &config_with_forge()), Vec::new());

    let spelled = workflow(vec![
        published("first", "review/first", Vec::new()),
        published("second", "review/{{node.id}}", Vec::new()),
        published("third", "review/{{ node.id }}", Vec::new()),
        published("fourth", "review/first", Vec::new()),
    ]);
    assert_eq!(
        check(&spelled, &config_with_forge()),
        vec![CheckError::ExternalGatesShareBranch {
            first: "first".into(),
            second: "fourth".into(),
            branch: "review/first".to_string(),
        }]
    );
}

/// A document is published as the bytes the run holds and as a page drawn from them, in
/// one folder with whatever else the gate carries: one name twice overwrites a file.
#[test]
fn a_gate_that_publishes_two_files_under_one_name_is_refused() {
    let collisions = |second: &str| {
        let wf = workflow(vec![published(
            "approve",
            "review/approve",
            vec![
                ArtifactSpec::Interpreted(ArtifactKind::Tasks),
                ArtifactSpec::Opaque(second.to_string()),
            ],
        )]);
        check(&wf, &config_with_forge())
            .into_iter()
            .filter(|error| matches!(error, CheckError::GateFilesCollide { .. }))
            .collect::<Vec<_>>()
    };
    let collide = |second: &str, name: &str| CheckError::GateFilesCollide {
        node: "approve".into(),
        name: name.to_string(),
        first: "tasks".to_string(),
        second: second.to_string(),
    };
    assert_eq!(
        collisions("tasks.md"),
        vec![collide("tasks.md", "tasks.md")]
    );
    assert_eq!(
        collisions("tasks.yaml"),
        vec![collide("tasks.yaml", "tasks.yaml")]
    );
    assert_eq!(collisions("notes.md"), Vec::new());
}

/// A child starts with its group, and its failure is its group's: what a child declares
/// about either is read by nothing, so it is refused rather than silently ignored.
#[test]
fn what_a_child_of_a_group_declares_about_its_group_is_refused() {
    let mut returns = bash("right", "true", &[]);
    returns.on_failure = Some(yunta_core::OnFailure {
        goto: "before".into(),
        max_reroutes: 1,
    });
    let wf = workflow(vec![
        bash("before", "true", &[]),
        parallel(
            "group",
            JoinPolicy::All,
            vec![bash("left", "true", &["before"]), returns],
        ),
    ]);
    let refused = |node: &str, field| CheckError::ChildDeclaresWhatNothingReads {
        node: node.into(),
        group: "group".into(),
        field,
    };
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors.contains(&refused("left", "depends_on")),
        "{errors:?}"
    );
    assert!(
        errors.contains(&refused("right", "on_failure")),
        "{errors:?}"
    );
}

/// A group itself waits and returns as any node does.
#[test]
fn a_group_declares_what_its_children_may_not() {
    let mut group = parallel(
        "group",
        JoinPolicy::All,
        vec![bash("left", "true", &[]), bash("right", "true", &[])],
    );
    group.depends_on = vec!["before".into()];
    group.on_failure = Some(yunta_core::OnFailure {
        goto: "before".into(),
        max_reroutes: 1,
    });
    let wf = workflow(vec![bash("before", "true", &[]), group]);
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, CheckError::ChildDeclaresWhatNothingReads { .. })),
        "{errors:?}"
    );
}
