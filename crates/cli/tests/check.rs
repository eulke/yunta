//! `yunta check` against a file, from a world of its own: an empty
//! state root and an empty org layer, so what `check` refuses is what
//! the file says rather than what the machine running the suite carries.

use yunta_testkit::{stderr, stdout, yunta_at, Checkout};

#[test]
fn a_well_formed_workflow_checks_clean_with_no_runner_fields() {
    let project = Checkout::new().file(
        "workflow.yaml",
        r#"
name: fixture
nodes:
  - id: lint
    kind: bash
    run: "true"
"#,
    );

    let output = yunta_at!(project, &["check", "workflow.yaml"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("OK"));
}

#[test]
fn an_unresolved_runner_fails_check_and_names_the_rule() {
    let project = Checkout::new().file(
        "workflow.yaml",
        r#"
name: fixture
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "do the thing"
"#,
    );

    let output = yunta_at!(project, &["check", "workflow.yaml"]);
    assert!(!output.status.success());
    let refusal = stderr(&output);
    assert!(
        refusal
            .lines()
            .any(|l| l
                == "  node `plan` references runner `planner`, which `runners:` does not define"),
        "check names the unresolved runner and the field that must define it: {refusal}"
    );
}

/// `run` refuses what `check` refuses, saying it in the same words: a
/// workflow that one of them accepts and the other rejects would leave a
/// reader to work out which to believe.
#[test]
fn run_refuses_with_the_errors_check_lists() {
    let project = Checkout::new()
        .file(
            "workflow.yaml",
            r#"
name: fixture
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "do the thing"
"#,
        )
        .committed();

    let check = yunta_at!(&project, &["check", "workflow.yaml"]);
    let run = yunta_at!(&project, &["run", "workflow.yaml"]);
    assert!(!check.status.success() && !run.status.success());
    let problem = "  node `plan` references runner `planner`, which `runners:` does not define";
    for (command, said) in [("check", stderr(&check)), ("run", stderr(&run))] {
        assert!(
            said.lines().any(|line| line == problem),
            "`{command}` names the problem in the words the other uses: {said}"
        );
    }
}

#[test]
fn a_config_file_resolves_the_runner() {
    let project = Checkout::new().file(
        "workflow.yaml",
        r#"
name: fixture
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "do the thing"
"#,
    );
    let project = project.file(
        "config.yaml",
        "runners:\n  planner:\n    - { adapter: mock, model: mock-model }\n",
    );

    let output = yunta_at!(
        project,
        &["check", "workflow.yaml", "--config", "config.yaml",]
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
}

#[test]
fn a_missing_workflow_file_is_a_clean_error_not_a_panic() {
    let project = Checkout::new();
    let output = yunta_at!(project, &["check", "/nonexistent/workflow.yaml"]);
    assert!(!output.status.success());
    let refusal = stderr(&output);
    assert!(
        refusal.starts_with("error: failed to read workflow at /nonexistent/workflow.yaml"),
        "a missing workflow file is one clean read error, not a panic: {refusal}"
    );
}

#[test]
fn malformed_yaml_is_a_clean_error_not_a_panic() {
    let project = Checkout::new().file("workflow.yaml", "not: [valid, workflow");

    let output = yunta_at!(project, &["check", "workflow.yaml"]);
    assert!(!output.status.success());
    let refusal = stderr(&output);
    assert!(
        refusal.contains("workflow.yaml: 1 error") && refusal.contains("does not parse"),
        "malformed YAML is one clean parse error, named at the file, not a panic: {refusal}"
    );
}

#[test]
fn a_repo_re_allowing_an_org_denied_pattern_fails_check_citing_the_layer() {
    // The org layer denies `sudo *`; the repo layer tries to
    // re-allow it. `yunta check` (no --config: the project's real layers)
    // must refuse, naming both layers and the pattern.
    let project = Checkout::new()
        .with_org_config("permissions:\n  commands:\n    deny: [\"sudo *\"]\n")
        .config("permissions:\n  commands:\n    allow: [\"sudo *\"]\n")
        .workflow(
            "workflow",
            "name: fixture\nnodes:\n  - id: lint\n    kind: bash\n    run: \"true\"\n",
        );

    let output = yunta_at!(project, &["check", "workflow.yaml"]);

    assert!(!output.status.success(), "check must refuse the re-allow");
    let refusal = stderr(&output);
    assert!(
        refusal.lines().any(|l| l
            == "  layer `repo` re-allows command pattern `sudo *` denied by layer `org` — permissions only narrow"),
        "the refusal cites both layers and the re-allowed pattern: {refusal}"
    );
}

#[test]
fn a_workflow_command_denied_by_the_org_layer_fails_check() {
    let project = Checkout::new()
        .with_org_config("permissions:\n  commands:\n    deny: [\"sudo *\"]\n")
        .workflow(
            "workflow",
            "name: fixture\nnodes:\n  - id: escalate\n    kind: bash\n    run: \"sudo make install\"\n",
        );

    let output = yunta_at!(project, &["check", "workflow.yaml"]);

    assert!(!output.status.success());
    let refusal = stderr(&output);
    assert!(
        refusal.lines().any(|l| l
            == "  node `escalate`: command `sudo make install` matches denied pattern `sudo *` (permissions.commands.deny)"),
        "the refusal cites the node, its command and the rule it matched: {refusal}"
    );
}

/// The §4 block heads a file's problems with how many there are. It
/// counts, and it says `error` or `errors` accordingly — one home for
/// the block means every surface that lists a file's problems reads the
/// same way.
#[test]
fn the_error_block_counts_what_it_lists_and_pluralises_it() {
    // One node with a runner nothing defines: exactly one error.
    let project = Checkout::new().file(
        "one.yaml",
        "name: one\nnodes:\n  - id: plan\n    kind: prompt\n    runner: planner\n    prompt: hi\n",
    );
    let output = yunta_at!(project, &["check", "one.yaml"]);
    assert!(!output.status.success());
    let refusal = stderr(&output);
    assert!(
        refusal.lines().any(|l| l == "one.yaml: 1 error"),
        "a single problem reads `1 error`: {refusal}"
    );

    // A second node with the same defect: two errors, plural.
    let project = project.file(
        "two.yaml",
        "name: two\nnodes:\n  - id: plan\n    kind: prompt\n    runner: planner\n    prompt: hi\n  \
         - id: build\n    kind: prompt\n    runner: builder\n    prompt: hi\n",
    );
    let output = yunta_at!(project, &["check", "two.yaml"]);
    assert!(!output.status.success());
    let refusal = stderr(&output);
    assert!(
        refusal.lines().any(|l| l == "two.yaml: 2 errors"),
        "two problems read `2 errors`: {refusal}"
    );
    assert!(
        !refusal.contains("error(s)"),
        "the count is known, so the plural is not hedged: {refusal}"
    );
}

#[test]
fn check_warns_about_a_suite_nothing_compares() {
    let project = Checkout::new()
        .file(
            "workflow.yaml",
            r#"
name: fixture
nodes:
  - id: lint
    kind: bash
    run: "true"
  - id: child
    kind: workflow
    use: child
"#,
        )
        .file(
            ".yunta/workflows/child.yaml",
            "name: child\nnodes:\n  - { id: only, kind: bash, run: \"true\" }\n",
        )
        .file("config.yaml", "baseline:\n  suite: \"make test\"\n");

    let output = yunta_at!(
        project,
        &["check", "workflow.yaml", "--config", "config.yaml"]
    );
    assert!(output.status.success(), "a warning never refuses the run");
    let said = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        said.contains("config declares `baseline.suite` (`make test`)")
            && said.contains("nothing reads the measurement"),
        "check says the suite would be measured and never read: {said}"
    );
}

#[test]
fn check_points_at_the_line_and_column_of_an_unknown_runner() {
    let project = Checkout::new()
        .file(
            ".yunta/config.yaml",
            "runners:\n  implementer:\n    - { adapter: mock, model: m }\n",
        )
        .file(
            "workflow.yaml",
            "name: fixture\nnodes:\n  - id: fix\n    kind: prompt\n    prompt: p\n    runner: implementr\n",
        );

    let output = yunta_at!(project, &["check", "workflow.yaml"]);
    assert!(!output.status.success());
    let said = stderr(&output);
    let quoted: Vec<&str> = said
        .lines()
        .skip_while(|line| !line.contains("runner `implementr`"))
        .take(5)
        .collect();
    assert!(
        quoted[0].ends_with("— did you mean `implementer`?"),
        "the near miss is named: {said}"
    );
    assert_eq!(
        quoted[1..]
            .iter()
            .map(|line| line.trim_end())
            .collect::<Vec<_>>(),
        [
            "     --> workflow.yaml:6:13",
            "      |",
            "    6 |     runner: implementr",
            "      |             ^^^^^^^^^^",
        ],
        "{said}"
    );
}

#[test]
fn three_unknown_keys_and_a_bad_runner_are_reported_together() {
    let project = Checkout::new()
        .file(
            ".yunta/config.yaml",
            "runners:\n  implementer:\n    - { adapter: mock, model: m }\n",
        )
        .file(
            "workflow.yaml",
            "name: fixture\nnodes:\n  - id: lint\n    kind: bash\n    rn: \"true\"\n    run: \"true\"\n  - id: fix\n    kind: prompt\n    promt: p\n    runner: implementr\n    descripton: d\n",
        );

    let output = yunta_at!(project, &["check", "workflow.yaml"]);
    assert!(!output.status.success());
    let said = stderr(&output);
    let placed: Vec<&str> = said
        .lines()
        .filter_map(|line| line.trim().strip_prefix("--> workflow.yaml:"))
        .collect();
    assert_eq!(placed, ["5:5", "9:5", "10:13", "11:5"], "{said}");
    assert!(said.starts_with("workflow.yaml: 4 errors"), "{said}");
    assert!(
        !said.contains("implementr:\n"),
        "a typo of a declared runner is not offered as one to declare: {said}"
    );
}
