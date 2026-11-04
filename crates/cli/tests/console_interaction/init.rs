//! `yunta init` asking at a terminal.

use super::*;

#[test]
fn init_asks_through_the_console_and_honours_escape() {
    // `yunta init -i` asks on the one line every prompt in this binary
    // is answered on, so the gestures that answer a run answer this
    // too: Enter takes what was typed, and Escape — "not me, not now" —
    // takes the value `init` detected on its own.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    yunta_testkit::init_repo(&repo);
    let home = root.path().join("state");

    let mut terminal = yunta_on_terminal!(&repo, &home, &["init", "-i"]);
    terminal.wait_for("project name", "init never asked for a project name");
    terminal.keys("orchard\r");
    terminal.wait_for("base branch", "init never asked for a base branch");
    terminal.keys("\x1b");

    let drawn = terminal.ended();
    assert!(terminal.ran_to_the_end(), "{drawn}");
    assert!(
        drawn.contains("esc keeps the detected default"),
        "every prompt says what Escape does — and this one has no run to \
         park, so it says the other thing:\n{drawn}"
    );

    let config = std::fs::read_to_string(repo.join(".yunta/config.yaml")).unwrap();
    assert!(
        config.contains("name: orchard"),
        "the answer that was typed is what was written:\n{config}"
    );
    let kept = base_branch_in(&config);
    assert!(
        drawn.contains(&format!("> {kept}")),
        "the field never held it, so this is what was left being said with the value \
         it kept:\n{drawn}"
    );
}

/// The base branch a `.yunta/config.yaml` records.
fn base_branch_in(config: &str) -> &str {
    config
        .lines()
        .find_map(|line| line.trim().strip_prefix("base_branch: "))
        .expect("the configuration records a base branch")
}

#[test]
fn init_says_the_value_a_line_left_empty_keeps() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    yunta_testkit::init_repo(&repo);
    let home = root.path().join("state");

    let mut terminal = yunta_on_terminal!(&repo, &home, &["init", "-i"]);
    terminal.wait_for("project name", "init never asked for a project name");
    terminal.keys("\r");
    terminal.wait_for("base branch", "init never asked for a base branch");
    terminal.keys("\r");

    let drawn = terminal.ended();
    let config = std::fs::read_to_string(repo.join(".yunta/config.yaml")).unwrap();
    assert!(config.contains("name: repo"), "{config}");
    assert!(drawn.contains("> repo"), "the name it kept:\n{drawn}");
    let kept = base_branch_in(&config);
    assert!(
        drawn.contains(&format!("> {kept}")),
        "the branch it kept:\n{drawn}"
    );
}

#[test]
fn ctrl_c_at_init_stops_it_with_nothing_written_or_said() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    yunta_testkit::init_repo(&repo);
    let home = root.path().join("state");

    let mut terminal = yunta_on_terminal!(&repo, &home, &["init", "-i"]);
    terminal.wait_for("project name", "init never asked for a project name");
    terminal.keys("\x03");

    let drawn = terminal.ended();
    assert_eq!(terminal.exit_code(), Some(130), "{drawn}");
    assert!(
        !drawn.contains("base branch"),
        "nothing more was asked:\n{drawn}"
    );
    assert!(
        !drawn.contains("> repo"),
        "a prompt that was stopped says no value:\n{drawn}"
    );
    assert!(
        !repo.join(".yunta/config.yaml").exists(),
        "and nothing was written:\n{drawn}"
    );
    assert!(terminal.line_discipline_is_back(), "{drawn}");
    assert!(terminal.cursor_is_back(), "{drawn}");
}
