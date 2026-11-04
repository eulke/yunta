//! An interrupt at each moment a run can be caught in.

use super::*;

/// The git a run spawns to make its worktree is a subprocess the
/// invocation owns: a Ctrl-C while it runs kills it with its whole tree,
/// leaves the mutation lock to nobody, and leaves no run behind.
#[test]
fn an_interrupt_while_the_worktree_is_being_prepared_kills_the_git_and_frees_the_lock() {
    let root = tempfile::tempdir().unwrap();
    let held = root.path().join("worktree-add.pid");
    let stubs = yunta_testkit::stubs::git_holding(root.path(), "worktree add", &held, None);

    let project = Checkout::under(root.path())
        .with_stubs(stubs)
        .workflow(
            "wf",
            "name: prepared\nnodes:\n  - { id: work, kind: bash, run: \"true\" }\n",
        )
        .config("defaults:\n  isolation: worktree\n")
        .committed();

    let yunta = spawn_yunta(&project, &["run", "wf.yaml"], root.path().join("run.log"));
    let git_pid = holding_git(&held, "the run never reached `git worktree add`");

    signal_process(pid_of(&yunta), Signal::SIGINT).expect("the run is alive to be interrupted");
    let output = yunta.wait_with_output().expect("the command returns");

    wait_until(
        || liveness(git_pid) == Liveness::Dead,
        || format!("the git of a cancelled run outlived it (pid {git_pid})"),
    );
    assert!(
        !project.repo.join(".git/yunta-worktree.lock").exists(),
        "the mutation lock is released with the mutation it guarded: {}",
        stderr(&output)
    );
    assert_eq!(
        std::fs::read_dir(yunta_testkit::runs_root(&project.home))
            .map(|entries| entries.flatten().count())
            .unwrap_or(0),
        0,
        "a run whose tree was never prepared is a run that was never born"
    );
}

/// The pid of the stub git now holding, once it has published it. The
/// publication is a rename, so the file existing is the whole pid and
/// the git being under way — no interval, no guess.
fn holding_git(held: &Path, never: &str) -> Pid {
    wait_until(|| held.exists(), || never.to_string());
    parse_pid(&std::fs::read_to_string(held).expect("the stub wrote its pid"))
}

/// Interrupting a resume stops it the way interrupting a run does: the
/// log's last word is a pause somebody asked for, not a failure.
#[test]
fn an_interrupt_during_a_resume_pauses_the_run_as_cancelled_by_user() {
    let root = tempfile::tempdir().unwrap();
    let project = Checkout::under(root.path())
        .working_in_place()
        .workflow("wf", HOLDS_OPEN)
        .committed();
    let started = root.path().join("started.txt");

    // First: a run stopped mid-node, so there is something to resume.
    let log = root.path().join("run.log");
    let first = spawn_yunta(&project, &["run", "wf.yaml"], log.clone());
    wait_until(
        || marker_written(&started),
        || "the node never started".into(),
    );
    signal_process(pid_of(&first), Signal::SIGINT).expect("the run is alive");
    first.wait_with_output().expect("the run returns");
    let run_id = only_run_id(&project.home);
    std::fs::remove_file(&started).expect("the marker is the resume's own signal");

    // Then: the resume re-runs the interrupted node, and is stopped in
    // the same place, by the same means.
    let resumed = spawn_yunta(
        &project,
        &["resume", &run_id],
        root.path().join("resume.log"),
    );
    wait_until(
        || marker_written(&started),
        || "the resume never re-ran the node".into(),
    );
    signal_process(pid_of(&resumed), Signal::SIGINT).expect("the resume is alive");
    let output = resumed.wait_with_output().expect("the resume returns");

    let status = project.run(Path::new(env!("CARGO_BIN_EXE_yunta")), &["status", &run_id]);
    assert!(
        stdout(&status).contains("interrupted by a person"),
        "the resume's own stop is on the log: {}\n{}",
        stdout(&status),
        stderr(&output)
    );
}

/// A first Ctrl-C ends a run that works in the person's own checkout, and
/// the stop releases the checkout's lock on the way out: nothing is left
/// running or held, and the stop asks git for nothing.
#[test]
fn a_first_interrupt_ends_an_in_place_run_and_releases_its_lock() {
    let root = tempfile::tempdir().unwrap();
    let git_log = root.path().join("git.log");
    let project = Checkout::under(root.path())
        .with_stubs(yunta_testkit::stubs::git_counting(root.path(), &git_log))
        .working_in_place()
        .workflow("wf", HOLDS_OPEN)
        .committed();
    let lock = project.repo.join(".git/yunta-none.lock");

    let mut yunta = spawn_yunta(&project, &["run", "wf.yaml"], root.path().join("run.log"));
    wait_until(
        || marker_written(&root.path().join("started.txt")),
        || "the node never started".into(),
    );
    assert!(lock.exists(), "the run holds its checkout while it runs");
    let asked_before_the_stop = std::fs::read_to_string(&git_log).unwrap_or_default();
    assert!(
        !asked_before_the_stop.is_empty(),
        "the run's own git calls reach the log, so the stop's silence is measured"
    );

    signal_process(pid_of(&yunta), Signal::SIGINT).expect("the run is alive");
    wait_until(
        || matches!(yunta.try_wait(), Ok(Some(_))),
        || "the first interrupt left the run alive".into(),
    );
    assert!(!lock.exists(), "the stop released the checkout");
    assert_eq!(
        std::fs::read_to_string(&git_log).unwrap_or_default(),
        asked_before_the_stop,
        "the stop asked git for nothing"
    );
}

/// Two Ctrl-C, two stages: the first stops the work, and what gives the
/// checkout back runs *because* it fired, so it answers to the second, which
/// is what lets a person who really means it out. The release asks git
/// only when `.git` does not name the common directory itself, so the
/// checkout here keeps its git directory apart.
#[test]
fn a_second_interrupt_aborts_a_release_the_first_left_running() {
    let root = tempfile::tempdir().unwrap();
    let held = root.path().join("release.pid");
    let armed = root.path().join("armed");
    // Armed only once the run is under way, so the git the release runs is
    // held and the gits the birth ran are not.
    let stubs = yunta_testkit::stubs::git_holding(
        root.path(),
        "rev-parse --git-common-dir",
        &held,
        Some(&armed),
    );
    let project = Checkout::under(root.path())
        .with_stubs(stubs)
        .working_in_place()
        .workflow("wf", HOLDS_OPEN)
        .committed();
    with_a_git_dir_of_its_own(&project, root.path());

    let yunta = spawn_yunta(&project, &["run", "wf.yaml"], root.path().join("run.log"));
    wait_until(
        || marker_written(&root.path().join("started.txt")),
        || "the node never started".into(),
    );
    std::fs::write(&armed, "now").expect("arm the stub");
    // The first stops the work; the release it triggers meets the held git.
    signal_process(pid_of(&yunta), Signal::SIGINT).expect("the run is alive");
    let git_pid = holding_git(&held, "the release never reached its git");
    // The second aborts it.
    signal_process(pid_of(&yunta), Signal::SIGINT).expect("the invocation is still alive");
    let output = yunta
        .wait_with_output()
        .expect("the second interrupt lets it out");
    wait_until(
        || liveness(git_pid) == Liveness::Dead,
        || {
            format!(
                "the release's git outlived the second interrupt: {}",
                stderr(&output)
            )
        },
    );
}

/// Moves the checkout's git directory beside it and leaves `.git` as a
/// pointer to it. That directory holds no `commondir` file, since it is
/// the common directory, so a release asks git which directory is common.
fn with_a_git_dir_of_its_own(project: &Checkout, root: &Path) {
    let apart = root.join("apart.git");
    std::fs::rename(project.repo.join(".git"), &apart).expect("move the git directory");
    let pointer = format!("gitdir: {}\n", apart.display());
    std::fs::write(project.repo.join(".git"), pointer).expect("point the checkout at it");
}

/// A detached run is a `yunta resume` in a process of its own, and it
/// carries this invocation's interruption like any other: `yunta cancel`
/// signals it and the run pauses as cancelled, with no escalation to
/// SIGKILL and no waiting out the timeout.
#[test]
fn yunta_cancel_on_a_detached_run_pauses_it_as_cancelled_by_user() {
    let root = tempfile::tempdir().unwrap();
    let project = Checkout::under(root.path())
        .working_in_place()
        .workflow("wf", HOLDS_OPEN)
        .committed();
    let bin = Path::new(env!("CARGO_BIN_EXE_yunta"));
    let (run_id, _) = detached_run_holding(&project, root.path());

    let cancelled = project.run(bin, &["cancel", &run_id]);
    assert!(
        stdout(&cancelled).contains("its engine stopped every session and recorded the pause"),
        "the detached engine answered its SIGINT: {}{}",
        stdout(&cancelled),
        stderr(&cancelled)
    );
    assert!(
        !stdout(&cancelled).contains("escalating to SIGKILL")
            && !stderr(&cancelled).contains("escalating to SIGKILL"),
        "a run that answers needs no escalation: {}{}",
        stdout(&cancelled),
        stderr(&cancelled)
    );

    let status = stdout(&project.run(bin, &["status", &run_id]));
    assert!(
        status.contains("interrupted by a person"),
        "the run's own log says who stopped it: {status}"
    );
}

/// `yunta cancel` is one long wait, so it answers to its own
/// interruption: a person who stops waiting gets their shell back, and
/// the SIGINT already delivered is still with the engine.
#[test]
fn an_interrupt_during_yunta_cancel_stops_the_wait() {
    let root = tempfile::tempdir().unwrap();
    let project = Checkout::under(root.path())
        .working_in_place()
        .workflow("wf", HOLDS_OPEN)
        .committed();
    let bin = Path::new(env!("CARGO_BIN_EXE_yunta"));
    let (run_id, engine) = detached_run_holding(&project, root.path());

    // Stopped, so the SIGINT `cancel` sends can never be answered and
    // the wait is a real one.
    signal_process(engine, Signal::SIGSTOP).expect("the detached engine is alive");

    let said = root.path().join("cancel.log");
    let cancelling = spawn_yunta(&project, &["cancel", &run_id], said.clone());
    // Its own first line is the signal that the wait has begun.
    let waiting = said.clone();
    wait_until(
        || {
            std::fs::read_to_string(&waiting)
                .unwrap_or_default()
                .contains("signalling the live engine")
        },
        || "`cancel` never reached the wait".into(),
    );

    signal_process(pid_of(&cancelling), Signal::SIGINT).expect("`cancel` is alive");
    cancelling.wait_with_output().expect("`cancel` returns");
    let printed = std::fs::read_to_string(&said).unwrap_or_default();
    assert!(
        printed.contains("interrupted while waiting"),
        "the wait ends with what the person can do next: {printed}"
    );
    assert!(
        !printed.contains("escalating to SIGKILL"),
        "a person who stopped waiting never asked for an escalation: {printed}"
    );

    // Let the engine have its SIGINT back and reap it, so nothing holds
    // the directory this test is about to delete.
    signal_process(engine, Signal::SIGCONT).expect("the engine is still there");
    let status = || stdout(&project.run(bin, &["status", &run_id]));
    wait_until(
        || status().contains("interrupted by a person"),
        || {
            format!(
                "the engine never acted on the SIGINT it was sent: {}",
                status()
            )
        },
    );
}
