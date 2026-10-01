//! A workflow with several problems reports all of them in one read, in
//! the order the file has them, each where it is written.

use std::path::Path;

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
        found[0].0.contains("unknown field `nodez_note`") && found[0].1 == Some(2),
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
    let found = said("name: w\nnodes: []\nnope: 1\n");
    let audited = &found[0].0;
    let expected = error
        .split("expected one of ")
        .nth(1)
        .and_then(|rest| rest.split(" at line").next())
        .expect("the parser lists what it accepts");
    assert!(audited.contains(expected), "{audited}\nparser: {error}");
}
