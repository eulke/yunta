//! A workflow with several problems reports all of them in one read, in
//! the order the file has them, each where it is written.

use std::path::Path;

use yunta_core::diagnostic::RuleCode;
use yunta_core::workflow::read::{read, read_all};

/// What the report says, line by line, with where each problem is.
fn said(text: &str) -> Vec<(String, Option<usize>)> {
    let (_, report) = read_all(text, Path::new("w.yaml"));
    report
        .diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.to_string(), diagnostic.at.map(|at| at.line)))
        .collect()
}

#[test]
fn three_unknown_keys_and_a_broken_reference_are_reported_together() {
    let text = "name: w\nnodez_note: x\nnodes:\n  - id: lint\n    kind: bash\n    rn: \"true\"\n    run: \"true\"\n  - id: fix\n    kind: prompt\n    promt: p\n    prompt: q\n    depends_on: [lnt]\n";
    let found = said(text);
    assert_eq!(found.len(), 4, "{found:#?}");
    assert!(
        found[0].0.contains("unknown key `nodez_note`") && found[0].1 == Some(2),
        "{found:#?}"
    );
    assert!(
        found[1].0.contains("unknown key `rn`") && found[1].1 == Some(6),
        "{found:#?}"
    );
    assert!(
        found[2].0.contains("unknown key `promt`") && found[2].1 == Some(10),
        "{found:#?}"
    );
    assert!(
        found[3].0.contains("`depends_on` names `lnt`")
            && found[3].0.contains("did you mean `lint`?"),
        "{found:#?}"
    );
}

#[test]
fn a_slip_in_a_kind_is_named_and_the_node_judged_as_the_kind_meant() {
    let text = "name: w\nnodes:\n  - id: fix\n    kind: prompy\n    promt: p\n";
    let found = said(text);
    assert_eq!(
        found,
        [
            (
                "node `fix`: unknown kind `prompy`; did you mean `prompt`? at line 4 column 11"
                    .to_string(),
                Some(4)
            ),
            (
                "node `fix`: unknown key `promt` for a `prompt` node; did you mean `prompt`? at \
                 line 5 column 5"
                    .to_string(),
                Some(5)
            ),
        ]
    );
}

#[test]
fn a_slip_in_a_kind_is_refused_under_its_own_rule_where_the_kind_is_written() {
    let (_, report) = read_all(
        "name: w\nnodes:\n  - id: fix\n    kind: prompy\n    prompt: p\n",
        Path::new("w.yaml"),
    );
    let slip = &report.diagnostics[0];
    assert_eq!(slip.rule(), Some(RuleCode::UnknownKind), "{report}");
    assert_eq!(slip.at.map(|at| (at.line, at.col)), Some((4, 11)));
}

#[test]
fn a_kind_no_kind_is_a_slip_from_is_refused_with_every_kind_there_is() {
    let found = said("name: w\nnodes:\n  - id: fix\n    kind: wibble\n    prompt: p\n");
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0]
            .0
            .contains("unknown kind `wibble`; one of `prompt`, `bash`"),
        "{found:#?}"
    );
}

/// The edge an artifact read implies is not a second reference: the read
/// is refused once, on the `node:` it names.
#[test]
fn a_reference_is_judged_once_where_it_is_written() {
    let text = "name: w\nnodes:\n  - id: plan\n    kind: bash\n    run: \"true\"\n  - id: fix\n    kind: prompt\n    prompt: p\n    depends_on: [plan]\n    context:\n      - files: [README.md]\n      - artifact: { node: plann, name: notes }\n";
    assert_eq!(
        said(text),
        [(
            "node `fix`: `context: artifact` names `plann`, and no node carries that id; did \
             you mean `plan`? at line 12 column 27"
                .to_string(),
            Some(12)
        )]
    );
}

#[test]
fn a_reference_a_fan_out_node_writes_is_judged_once() {
    let text = "name: w\nnodes:\n  - { id: review, kind: prompt, runners: [a, b], prompt: p, on_failure: { goto: fixx, max_reroutes: 1 } }\n  - { id: fix, kind: bash, run: \"true\" }\n";
    let found = said(text);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0]
            .0
            .starts_with("node `review`: `on_failure.goto` names `fixx`"),
        "{found:#?}"
    );
    let named = "name: w\nnodes:\n  - { id: review, kind: prompt, runners: [a, b], prompt: p }\n  - { id: ship, kind: bash, run: \"true\", depends_on: [review@a] }\n";
    assert_eq!(said(named), [], "a sibling a fan-out generates is a node");
}

#[test]
fn a_typo_in_a_required_key_does_not_hide_the_other_errors() {
    // `prompt` is what a prompt node cannot be without: read as the key
    // the person meant, the node reads, and the reference after it is
    // judged rather than hidden behind a missing field.
    let text =
        "name: w\nnodes:\n  - id: fix\n    kind: prompt\n    promt: p\n    depends_on: [nowhere]\n";
    let found = said(text);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert!(found[0].0.contains("did you mean `prompt`?"), "{found:#?}");
    assert!(
        found[1].0.contains("`depends_on` names `nowhere`"),
        "{found:#?}"
    );
    assert!(
        found.iter().all(|(text, _)| !text.contains("missing")),
        "the typo is not reported a second time as a missing key: {found:#?}"
    );
}

#[test]
fn the_workflow_a_typo_hides_is_judged_but_never_handed_to_a_run() {
    let text = "name: w\nnodes:\n  - id: fix\n    kind: prompt\n    promt: p\n";
    let (workflow, report) = read_all(text, Path::new("w.yaml"));
    assert!(workflow.is_some(), "what is left reads, for check to judge");
    assert_eq!(report.diagnostics.len(), 1);
    assert!(read(text, Path::new("w.yaml")).is_err(), "a run refuses it");
}

#[test]
fn the_audit_knows_every_key_a_workflow_accepts_at_its_top() {
    let error = yunta_core::yaml::parse::<yunta_core::Workflow>("name: w\nnodes: []\nnope: 1\n")
        .unwrap_err()
        .to_string();
    let expected = error
        .split("expected one of ")
        .nth(1)
        .and_then(|rest| rest.split(" at line").next())
        .expect("the parser lists what it accepts");
    let audited: Vec<String> = yunta_core::workflow::WORKFLOW_KEYS
        .iter()
        .map(|key| format!("`{key}`"))
        .collect();
    assert_eq!(audited.join(", "), expected, "parser: {error}");
    let found = said("name: w\nnodes: []\nnope: 1\n");
    assert_eq!(
        found[0].0,
        "the document has an unknown key `nope` at line 3 column 1"
    );
}

#[test]
fn a_near_miss_is_never_the_node_that_names_it() {
    // `tests2` is one slip from `tests`, the node that names it — and a
    // node depending on itself is a cycle, not what anyone meant.
    let said = said(
        "name: wf\nnodes:\n  - id: lint\n    kind: bash\n    run: \"true\"\n  \
         - id: tests\n    kind: bash\n    depends_on: [lint, tests2]\n    run: \"true\"\n",
    );
    let (problem, _) = said
        .iter()
        .find(|(problem, _)| problem.contains("`tests2`"))
        .expect("the reference that reaches no node is reported");
    assert!(!problem.contains("did you mean `tests`"), "{problem}");
}
