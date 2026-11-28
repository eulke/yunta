//! `yunta init` asking at a terminal: the project's name and base branch, and
//! each runner its workflows propose, on an adapter this machine answers for.

use super::*;

/// A `PATH` on which no adapter CLI answers, so `init -i` asks no runner and what
/// a test reads does not depend on the CLIs the machine running the suite has.
const NO_ADAPTER: &str = "/usr/bin:/bin";

/// A `PATH` on which the one adapter CLI is the scripted `claude`.
fn only_claude(root: &Path) -> String {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(yunta_testkit_core::stubs::claude_code(), bin.join("claude"))
        .unwrap();
    format!("{}:{NO_ADAPTER}", bin.display())
}

/// `init -i` in a fresh repository under `root`, with only `claude` answering,
/// its name and base branch kept: the terminal at the first runner it asks.
fn asked_for_a_runner(root: &Path) -> (Terminal, std::path::PathBuf) {
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    yunta_testkit::init_repo(&repo);
    let home = root.join("state");
    let path = only_claude(root);
    let mut terminal =
        yunta_on_terminal!(&repo, &home, &["init", "-i"], &[("PATH", path.as_str())]);
    terminal.wait_for("name (enter keeps", "init never asked for a project name");
    terminal.keys("\r");
    terminal.wait_for("base (enter keeps", "init never asked for a base branch");
    terminal.keys("\r");
    terminal.wait_for(
        "the adapter `implementer` runs on",
        "init never asked the runner's adapter",
    );
    // A key typed before the list is drawn is no answer to it.
    terminal.wait_for(
        &format!("{POINTED}claude-code"),
        "the adapters that answered were never listed",
    );
    (terminal, repo)
}

/// The runner proposed is put on the adapter picked, with the model typed, and
/// written declared.
#[test]
fn init_declares_a_runner_on_the_adapter_picked_with_the_model_typed() {
    let root = tempfile::tempdir().unwrap();
    let (mut terminal, repo) = asked_for_a_runner(root.path());
    terminal.keys("\r");
    terminal.wait_for(
        "model (enter with none leaves it to the adapter)",
        "init never asked the runner's model",
    );
    terminal.keys("opus\r");
    let drawn = terminal.ended();
    assert!(terminal.ran_to_the_end(), "{drawn}");
    let config = std::fs::read_to_string(repo.join(".yunta/config.yaml")).unwrap();
    assert!(
        config
            .contains("runners:\n  implementer:\n    - adapter: claude-code\n      model: opus\n"),
        "{config}"
    );
    assert!(
        drawn.contains("implementer (the default) on claude-code/opus"),
        "{drawn}"
    );
}

/// A model left empty declares the runner on the adapter picked alone, its model left
/// to that adapter's CLI: the configuration reads back with the runner a run resolves.
#[test]
fn an_empty_model_declares_the_runner_on_its_adapter_alone() {
    let root = tempfile::tempdir().unwrap();
    let (mut terminal, repo) = asked_for_a_runner(root.path());
    terminal.keys("\r");
    terminal.wait_for(
        "model (enter with none leaves it to the adapter)",
        "init never asked the runner's model",
    );
    terminal.keys("\r");
    let drawn = terminal.ended();
    assert!(terminal.ran_to_the_end(), "{drawn}");
    let config = std::fs::read_to_string(repo.join(".yunta/config.yaml")).unwrap();
    assert!(
        config.contains("runners:\n  implementer:\n    - adapter: claude-code\n"),
        "{config}"
    );
    let runners = yunta_core::ConfigLayer::read(&config)
        .expect("the configuration written reads")
        .runners
        .expect("the runner is declared");
    let implementer = &runners[&yunta_core::RunnerName::PROPOSED];
    assert_eq!(implementer.len(), 1, "{config}");
    assert_eq!(implementer[0].adapter.as_str(), "claude-code", "{config}");
    assert_eq!(implementer[0].model, None, "{config}");
    assert!(
        drawn.contains("implementer (the default) on claude-code"),
        "{drawn}"
    );
    assert!(!drawn.contains("is not declared"), "{drawn}");
}

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

    let mut terminal = yunta_on_terminal!(&repo, &home, &["init", "-i"], &[("PATH", NO_ADAPTER)]);
    terminal.wait_for("name (enter keeps", "init never asked for a project name");
    terminal.keys("orchard\r");
    terminal.wait_for("base (enter keeps", "init never asked for a base branch");
    terminal.keys("\x1b");

    let drawn = terminal.ended();
    assert!(terminal.ran_to_the_end(), "{drawn}");
    assert!(
        drawn.contains("esc keep the detected default"),
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

    let mut terminal = yunta_on_terminal!(&repo, &home, &["init", "-i"], &[("PATH", NO_ADAPTER)]);
    terminal.wait_for("name (enter keeps", "init never asked for a project name");
    terminal.keys("\r");
    terminal.wait_for("base (enter keeps", "init never asked for a base branch");
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

    let mut terminal = yunta_on_terminal!(&repo, &home, &["init", "-i"], &[("PATH", NO_ADAPTER)]);
    terminal.wait_for("name (enter keeps", "init never asked for a project name");
    terminal.keys("\x03");

    let drawn = terminal.ended();
    assert_eq!(terminal.exit_code(), Some(130), "{drawn}");
    assert!(
        !drawn.contains("base (enter keeps"),
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

/// A scripted adapter CLI: the name it is found by on a `PATH`, the adapter it answers
/// for, and the script.
type Cli = (&'static str, &'static str, std::path::PathBuf);

/// Claude Code's scripted CLI.
fn claude() -> Cli {
    (
        "claude",
        "claude-code",
        yunta_testkit_core::stubs::claude_code(),
    )
}

/// Codex's scripted CLI.
fn codex() -> Cli {
    ("codex", "codex", yunta_testkit_core::stubs::codex())
}

/// `clis` linked into `root`'s `bin`: the directory, and a `PATH` with it first.
fn answering(root: &Path, clis: &[Cli]) -> (std::path::PathBuf, String) {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for (name, _, script) in clis {
        std::os::unix::fs::symlink(script, bin.join(name)).unwrap();
    }
    let path = format!("{}:{NO_ADAPTER}", bin.display());
    (bin, path)
}

/// A fresh repository named `repo` under `root`.
fn fresh(root: &Path) -> std::path::PathBuf {
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    yunta_testkit::init_repo(&repo);
    repo
}

/// What `init -i` writes in a fresh repository under `root` with `clis` answering: its
/// name and base branch kept, its one runner put on the adapter of `clis[picked]`, with
/// `model` typed.
fn init_answered(root: &Path, clis: &[Cli], picked: usize, model: &str) -> String {
    let repo = fresh(root);
    let (_, path) = answering(root, clis);
    let home = root.join("state");
    let mut terminal =
        yunta_on_terminal!(&repo, &home, &["init", "-i"], &[("PATH", path.as_str())]);
    terminal.wait_for("name (enter keeps", "init never asked for a project name");
    terminal.keys("\r");
    terminal.wait_for("base (enter keeps", "init never asked for a base branch");
    terminal.keys("\r");
    terminal.wait_for(
        &format!("{POINTED}{}", clis[0].1),
        "the adapters that answered were never listed",
    );
    for _ in 0..picked {
        terminal.keys("\x1b[B");
    }
    let adapter = clis[picked].1;
    let pointed = format!("{POINTED}{adapter}");
    terminal.wait_for(&pointed, "the adapter was never pointed at");
    terminal.keys("\r");
    terminal.wait_for(
        "model (enter with none leaves it to the adapter)",
        "init never asked the runner's model",
    );
    terminal.keys(&format!("{model}\r"));
    let drawn = terminal.ended();
    assert!(terminal.ran_to_the_end(), "{drawn}");
    std::fs::read_to_string(repo.join(".yunta/config.yaml")).unwrap()
}

/// What the API's draft writes in a fresh repository under `root` with `clis` answering,
/// its one runner answered on the adapter of `clis[picked]` with `model`.
fn drafted(root: &Path, clis: &[Cli], picked: usize, model: &str) -> String {
    let repo = fresh(root);
    let (bin, _) = answering(root, clis);
    let state = root.join("state");
    let workspace = yunta_testkit_ui::sandbox::workspace_on(&repo, &state, vec![bin]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let detected = workspace
            .detect_project(false, &yunta_api::Stop::new())
            .await
            .expect("what setting it up finds");
        let answer = yunta_api::RunnerAnswer {
            runner: yunta_core::RunnerName::PROPOSED,
            adapter: yunta_core::AdapterId::from_static(clis[picked].1),
            model: Some(model.parse().expect("a model name")),
        };
        let setup = detected.draft.answered("", "", vec![answer]);
        detected.write(setup).await.expect("the setup written");
    });
    std::fs::read_to_string(repo.join(".yunta/config.yaml")).unwrap()
}

/// `init -i` writes, byte for byte, what the API's draft writes from the same answers:
/// on one adapter, the runner on it alone; on two, the one picked first with the model
/// typed, then the other with none; the default runner either way, as the repository
/// has no workflow.
#[test]
fn init_i_writes_what_the_draft_writes_from_the_same_answers_on_one_adapter_and_on_two() {
    let one = [claude()];
    let (asked, drafted_here) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let written = init_answered(asked.path(), &one, 0, "opus");
    assert_eq!(written, drafted(drafted_here.path(), &one, 0, "opus"));
    assert!(
        written.ends_with(
            "runners:\n  implementer:\n    - adapter: claude-code\n      model: opus\n\
             defaults:\n  runner: implementer\n"
        ),
        "{written}"
    );

    let two = [claude(), codex()];
    let (asked, drafted_here) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let written = init_answered(asked.path(), &two, 1, "gpt-5");
    assert_eq!(written, drafted(drafted_here.path(), &two, 1, "gpt-5"));
    assert!(
        written.ends_with(
            "runners:\n  implementer:\n    - adapter: codex\n      model: gpt-5\n    \
             - adapter: claude-code\ndefaults:\n  runner: implementer\n"
        ),
        "{written}"
    );
}
