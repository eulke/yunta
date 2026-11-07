//! What a refused engine says: who drives the run when that can be told, and which
//! file a person looks at when it cannot.

use super::*;

/// A program copies what it reads from a refusal, so the sentence and the commands
/// name the run whole, on the one document stdout carries.
#[test]
fn a_program_reads_the_refusal_as_one_document_with_the_next_commands() {
    let run = Holding::started_in(IN_PLACE);
    let (id, pid) = (&run.run_id, run.engine_pid());

    let refused = run.second_resume(&["--json"]);
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));
    assert!(stderr(&refused).is_empty(), "{}", stderr(&refused));
    let document: Value = serde_json::from_slice(&refused.stdout).expect("one document on stdout");
    assert_eq!(
        document["error"]["said"],
        format!("run {id} is already moving: pid {pid} drives it"),
        "{document:#}"
    );
    let next: Vec<&str> = document["error"]["next"]
        .as_array()
        .expect("the commands that go on from here")
        .iter()
        .filter_map(|step| step["command"].as_str())
        .collect();
    assert_eq!(
        next,
        [
            format!("yunta status {id} --follow"),
            format!("yunta cancel {id}")
        ],
        "{document:#}"
    );

    run.go();
    run.lock_goes_away();
}

/// The way on from an engine asking at its terminal is answering it there: nothing
/// else moves that run, and a resume that said it was moving would hide why it waits.
#[test]
fn a_run_asking_at_its_terminal_is_said_to_be_answered_there() {
    let run = Holding::started_in(IN_PLACE);
    let pid = run.engine_pid();
    run.asks_at_its_terminal();

    let refused = run.second_resume(&[]);
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));
    let said = unwrapped(&stderr(&refused));
    let words = format!("node `hold` is asking at its terminal (pid {pid}); answer it there");
    assert!(said.contains(&words), "no `{words}` in: {said}");

    run.go();
    run.lock_goes_away();
}

/// A lock that names nobody says nothing about who drives the run, so it is never
/// taken: a person reads which file it is, and decides.
#[test]
fn a_lock_nobody_can_read_is_named_and_left_for_a_person() {
    let run = Holding::started_in(IN_PLACE);
    run.stopped();
    std::fs::write(run.lock(), "not a lock").unwrap();

    let refused = run.second_resume(&[]);
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));
    let said = unwrapped(&stderr(&refused));
    let handle = run.handle();
    for words in [
        format!("run {handle} has an engine lock with no readable owner"),
        "scratch/engine.lock`".to_string(),
        "if nothing drives this run, delete that file by hand and resume it again".to_string(),
    ] {
        assert!(said.contains(&words), "no `{words}` in: {said}");
    }
    assert_eq!(std::fs::read_to_string(run.lock()).unwrap(), "not a lock");
    assert_eq!(run.starts(), 1, "the node did not start again");
}

/// A run parked on a limit can wait for hours, and a lock nobody can check does not
/// clear by waiting: a resume that found out after the wait would have said nothing
/// until a date no test reaches.
#[test]
fn a_lock_nobody_can_check_refuses_before_a_limits_reset_is_waited_for() {
    let run = Holding::started_in(IN_PLACE);
    run.stopped();
    run.parked_on_a_far_reset();
    std::fs::write(run.lock(), "not a lock").unwrap();

    let refused = run.second_resume(&[]);
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));
    let said = unwrapped(&stderr(&refused));
    assert!(said.contains("no readable owner"), "{said}");
    assert!(said.contains("scratch/engine.lock`"), "{said}");
    assert_eq!(run.starts(), 1, "the node did not start again");
}

/// An answer has nothing to answer on a run another engine drives, and the sentence
/// that says who drives it is the one that says what to do.
#[test]
fn an_answer_on_a_moving_run_says_who_drives_it() {
    let run = Holding::started_in(IN_PLACE);
    let pid = run.engine_pid();

    let refused = yunta_at!(&run.checkout, &["resolve-gate", &run.run_id, "retry"]);
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));
    let said = unwrapped(&stderr(&refused));
    let words = format!(
        "run {} is already moving: pid {pid} drives it",
        run.handle()
    );
    assert!(said.contains(&words), "no `{words}` in: {said}");

    run.go();
    run.lock_goes_away();
}
