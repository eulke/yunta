//! The binary answering about itself: its version, and what it says
//! when it is given nothing to do.

use yunta_testkit::{stderr, stdout, yunta_at, yunta_in, Checkout};

#[test]
fn the_binary_reports_its_version() {
    let output = yunta_at!(Checkout::new(), &["--version"]);

    assert!(output.status.success());
    assert!(stdout(&output).starts_with("yunta "));
}

#[test]
fn the_binary_without_a_subcommand_shows_the_runs_of_the_repository_it_is_in() {
    let output = yunta_at!(Checkout::new(), &[]);

    assert!(output.status.success(), "{}", stderr(&output));
    let said = stdout(&output);
    assert!(
        said.lines()
            .next()
            .is_some_and(|line| line.contains("runs of this repository")),
        "{said}"
    );
    assert!(
        said.contains("yunta list"),
        "an empty field says what to type: {said}"
    );
}

#[test]
fn the_binary_without_a_subcommand_outside_a_repository_says_where_it_works() {
    let away = tempfile::tempdir().unwrap();
    let output = yunta_in!(away.path(), &away.path().join("home"), &[]);

    assert!(output.status.success(), "{}", stderr(&output));
    let said = stdout(&output);
    assert!(said.contains("not in a repository"), "{said}");
    assert!(said.contains("yunta --help"), "{said}");
}
