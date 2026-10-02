//! `yunta status` leads with what a person came for: the word the run is
//! called by, then what holds it or that nothing does — and quotes why a
//! node failed rather than leaving it to be dug out of the log.

use yunta_testkit::{run_id_from, stdout, yunta_at, Checkout};

/// A build that prints what a compiler prints and fails with it.
const FAILS: &str = "name: build\nnodes:\n  - id: compile\n    kind: bash\n    run: \"for i in 1 2 3 4 5 6 7 8; do echo note $i; done; echo 'error: cannot find value x'; exit 101\"\n";
const FINISHES: &str = "name: ok\nnodes:\n  - { id: only, kind: bash, run: \"true\" }\n";

fn page(workflow: &str) -> Vec<String> {
    let checkout = Checkout::new()
        .config("defaults:\n  isolation: none\n")
        .workflow("wf", workflow)
        .committed();
    let run_id = run_id_from(&yunta_at!(&checkout, &["run", "wf.yaml"]));
    let status = yunta_at!(&checkout, &["status", &run_id]);
    assert!(
        status.status.success(),
        "status exits 0 whatever the run says"
    );
    stdout(&status).lines().map(str::to_string).collect()
}

#[test]
fn status_says_whether_the_run_needs_you_in_its_first_two_lines() {
    let parked = page(FAILS);
    assert!(parked[0].ends_with("needs you"), "{parked:#?}");
    assert_eq!(
        parked[1], "  node `compile` failed: exit 101",
        "{parked:#?}"
    );

    let finished = page(FINISHES);
    assert!(finished[0].ends_with("finished"), "{finished:#?}");
    assert_eq!(finished[1], "  nothing needs you", "{finished:#?}");
}

#[test]
fn status_quotes_why_a_node_failed() {
    let parked = page(FAILS);
    let quoted: Vec<&String> = parked
        .iter()
        .skip_while(|line| line.trim() != "compile")
        .collect();
    assert!(
        quoted
            .iter()
            .any(|line| line.ends_with("error: cannot find value x")),
        "the last line the command printed is quoted: {parked:#?}"
    );
    assert!(
        quoted.iter().any(|line| line.contains("3 lines above"))
            && quoted.iter().any(|line| line.contains("whole output: ")),
        "and how much came before it, and where the rest is: {parked:#?}"
    );
}
