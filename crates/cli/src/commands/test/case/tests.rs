use super::*;

#[test]
fn a_case_declares_its_mode_and_reads_every_input_scalar_as_text() {
    let case: TestCase = yunta_core::yaml::parse(
        "workflow: build-feature\n\
         mode: quick\n\
         inputs: { idea: add dark mode, retries: 3, dry_run: true }\n\
         fixture: fixtures/quick.yaml\n\
         expect:\n  final_state: needs you\n",
    )
    .unwrap();
    assert_eq!(
        case.mode.as_ref().map(|value| value.as_str()),
        Some("quick")
    );
    assert_eq!(case.inputs["idea"], "add dark mode");
    assert_eq!(case.inputs["retries"], "3");
    assert_eq!(case.inputs["dry_run"], "true");
}

#[test]
fn a_case_names_the_directory_that_seeds_its_sandbox() {
    let case: TestCase = yunta_core::yaml::parse(
        "workflow: run-tasks\n\
         worktree: worktrees/greeting-crate\n\
         fixture: fixtures/run-tasks.yaml\n\
         expect:\n  final_state: finished\n",
    )
    .unwrap();
    assert_eq!(
        case.worktree.as_deref(),
        Some(Path::new("worktrees/greeting-crate"))
    );
}

#[test]
fn a_case_declares_the_option_each_gate_is_answered_with() {
    let case: TestCase = yunta_core::yaml::parse(
        "workflow: build-feature\n\
         fixture: fixtures/quick.yaml\n\
         decisions: { approve-plan: approve, ship: promote }\n\
         expect:\n  final_state: promoted\n",
    )
    .unwrap();
    assert_eq!(case.decisions["approve-plan"].as_str(), "approve");
    assert_eq!(case.decisions["ship"].as_str(), "promote");
    assert_eq!(case.expect.final_state, FinalState::Promoted);
}

#[test]
fn a_case_written_with_paused_is_refused_naming_needs_you() {
    let refused = yunta_core::yaml::parse::<TestCase>(
        "workflow: review\nfixture: fixtures/review.yaml\nexpect:\n  final_state: paused\n",
    )
    .expect_err("`paused` is a retired word");
    let said = yunta_core::describe(&refused);
    assert!(said.contains("`needs you`"), "{said}");
}

#[test]
fn a_case_that_answers_no_gate_parks_on_the_first_one() {
    let case: TestCase = yunta_core::yaml::parse(
        "workflow: review\nfixture: fixtures/review.yaml\nexpect:\n  final_state: needs you\n",
    )
    .unwrap();
    assert!(case.decisions.is_empty());
}

#[test]
fn a_case_without_mode_or_inputs_runs_the_whole_graph_on_input_defaults() {
    let case: TestCase = yunta_core::yaml::parse(
        "workflow: review\nfixture: fixtures/review.yaml\nexpect:\n  final_state: finished\n",
    )
    .unwrap();
    assert_eq!(case.mode, None);
    assert!(case.inputs.is_empty());
    assert_eq!(case.worktree, None);
}

#[test]
fn a_case_that_expects_a_run_waiting_on_a_limit_reset_parses_waiting() {
    let case: TestCase = yunta_core::yaml::parse(
        "workflow: review\nfixture: fixtures/review.yaml\nexpect:\n  final_state: waiting\n",
    )
    .unwrap();
    assert_eq!(case.expect.final_state, FinalState::Waiting);
}
