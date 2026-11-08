//! `yunta run --detach`: what it leaves running, and what it says before it hands off.

use super::*;

// --- `yunta run --detach` ------------------------------------------------

/// A checkout under `isolation: none` takes one run at a time, and the
/// lock that says so names whoever is in the tree. `--detach` is the one
/// shape where the process that takes it is not the process that works:
/// it creates the run, hands it to a `yunta resume` of its own, and
/// leaves.
#[test]
fn a_detached_run_leaves_the_checkout_locked_by_the_child_that_is_in_it() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    // The node announces itself and then holds, so the assertions run
    // against a child that is provably still in the tree. Its markers
    // live beside the checkout rather than in it: `isolation: none`
    // refuses a dirty tree, and that refusal would answer the second
    // run before the lock ever did.
    write(
        &repo.join("wf.yaml"),
        "name: holds-open\nnodes:\n  - id: hold\n    kind: bash\n    \
         run: \"echo in > ../started.txt; until [ -f ../go.txt ] || [ ! -e ../started.txt ]; do sleep 0.05; done\"\n",
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let detached = yunta_in!(&repo, &home, &["run", "wf.yaml", "--detach"]);
    assert!(detached.status.success(), "{}", stderr(&detached));
    let run_id = run_id_from(&detached);
    wait_until(
        || marker_written(&root.path().join("started.txt")),
        || "the detached child never reached the node".into(),
    );

    // A second run has to be refused. Before the hand-off the lock named
    // the parent, which exits the moment it reports the id — so this
    // took the checkout over from a "dead" holder and ran a second time
    // in a tree the child was working in.
    let second = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    let said = unwrapped(&stderr(&second));
    // Released before anything is asserted: the node holds until this
    // file exists, and an assertion firing first would leave it holding
    // for as long as the suite runs.
    write(&root.path().join("go.txt"), "go");
    assert!(
        !second.status.success(),
        "a second run took a checkout the detached child is in: {said}"
    );
    assert!(
        said.contains("already has a run in progress"),
        "the refusal names what is in the way: {said}"
    );
    assert!(
        !said.contains("dead process"),
        "the holder is the child, alive, not a process that has exited: {said}"
    );

    // The child outlives this process by design, so the test waits for
    // it rather than leaving one behind: a detached run nobody reaps is
    // a detached run still holding the checkout of a directory the
    // harness is about to delete.
    let status = || stdout(&yunta_in!(&repo, &home, &["status", &run_id]));
    wait_until(
        || status().contains("finished"),
        || format!("the released child never finished: {}", status()),
    );
}

/// A fixture scripts the sessions of whoever runs them, and `--detach`
/// hands the run to another process, which resolves the adapters `runners:`
/// names and reads no fixture at all. The combination is
/// refused with what to do instead, never honoured as a run that quietly
/// stayed attached.
#[test]
fn detaching_a_run_against_a_mock_fixture_is_refused_before_anything_is_created() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    // The workflow passes `yunta check` and the fixture completes it, so
    // the refusal is the only thing standing between this invocation and
    // a finished run.
    write(
        &repo.join(".yunta/config.yaml"),
        "runners:\n  executor:\n    - { adapter: claude-code, model: claude-model }\n",
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: mocked
nodes:
  - id: implement
    kind: prompt
    runner: executor
    prompt: "Do the thing."
"#,
    );
    write(
        &repo.join("fixture.yaml"),
        "dialect: claude-code\nsessions:\n  - outcome: { type: completed, summary: done }\n",
    );

    for args in [
        vec![
            "run",
            "wf.yaml",
            "--detach",
            "--adapter",
            "mock",
            "--fixture",
            "fixture.yaml",
        ],
        // The same rule, reached with the fixture still missing: the
        // answer names the combination that cannot work rather than
        // sending the reader to add a flag that is refused next.
        vec!["run", "wf.yaml", "--detach", "--adapter", "mock"],
    ] {
        let refused = yunta_in!(&repo, &home, &args);
        assert!(
            !refused.status.success(),
            "`{args:?}` must be refused, got: {}",
            stdout(&refused)
        );
        let stderr = stderr(&refused);
        assert!(
            stderr.contains("--detach") && stderr.contains("hands the run to another process"),
            "the refusal names the flag and why a detached child cannot honour it: {stderr}"
        );
        assert!(
            !stderr.contains("`yunta resume`"),
            "no bare command stands in for the process it means: {stderr}"
        );
        assert!(
            !home.join("runs").exists(),
            "nothing is created for an invocation that is refused up front"
        );
    }
}

#[test]
fn yunta_run_detach_returns_immediately_and_the_workflow_finishes_in_a_detached_child() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: slow
nodes:
  - id: work
    kind: bash
    run: "until [ -f go.txt ]; do :; done; echo done > done.txt"
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml", "--detach"]);

    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );
    // --detach returned even though the node blocks until `go.txt` appears:
    // had it waited for the workflow it would still be hanging now. The run
    // is in progress, not finished, at the moment the invocation returns.
    let run_id = run_id_from(&run);
    let status = yunta_in!(&repo, &home, &["status", &run_id]);
    assert!(
        !stdout(&status).contains("finished"),
        "the workflow is still running in the detached child: {}",
        stdout(&status)
    );

    // Release the node and watch the detached child carry the run to
    // completion on its own.
    write(&repo.join("go.txt"), "go");
    let status = || stdout(&yunta_in!(&repo, &home, &["status", &run_id]));
    wait_until(
        || status().contains("finished"),
        || format!("the detached run never reached finished: {}", status()),
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("done.txt"))
            .unwrap()
            .trim(),
        "done"
    );
}

#[test]
fn yunta_run_detach_survives_a_sigterm_to_its_own_launchers_process_group() {
    // A shell's Ctrl-C delivers SIGINT (or a job-control kill delivers
    // whatever signal) to the *whole foreground process group* the
    // launched command sits in — never just that one pid. If the
    // detached workflow process shared the launcher's group, this would
    // kill it too, defeating the entire point of `--detach`. Simulated
    // here by putting the launcher in its own fresh group (exactly what
    // a shell's job control already does for a foreground command) and
    // signalling that whole group right after the launcher itself has
    // exited.
    use std::os::unix::process::CommandExt;

    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: slow
nodes:
  - id: work
    kind: bash
    run: "until [ -f go.txt ]; do :; done; echo done > done.txt"
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    hermetic(&mut command, &repo, &home);
    let launcher = command
        .args(["run", "wf.yaml", "--detach"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .unwrap();
    let launcher_pgid = pid_of(&launcher); // `process_group(0)`: pgid == its own pid
    let output = launcher.wait_with_output().unwrap();
    assert!(output.status.success());
    let run_id = run_id_from(&output);

    // The launcher itself has already exited — this signals whatever
    // else is still in its group, if anything is.
    signal_group(launcher_pgid, Signal::SIGTERM).expect("the launcher's group is ours to signal");

    // The node blocks until `go.txt` appears, so it is still running when
    // the signal lands; release it and confirm the detached run survived
    // the signal and finished on its own.
    write(&repo.join("go.txt"), "go");
    let status = || stdout(&yunta_in!(&repo, &home, &["status", &run_id]));
    wait_until(
        || status().contains("finished"),
        || {
            format!(
                "the detached run must survive a signal to its launcher's group, got: {}",
                status()
            )
        },
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("done.txt"))
            .unwrap()
            .trim(),
        "done"
    );
}

/// The prior distribution and the budget warning derived from it go to the
/// invocation that *creates* a run, and `yunta resume` picks a run up
/// instead of creating one. `--detach` creates the run and gives it straight
/// to such a child, so this is the only invocation either can reach a person
/// in, and a warning that asks whether to spend is worth nothing once the
/// child is already spending.
#[test]
fn run_detach_shows_the_distribution_and_the_budget_warning_before_handing_the_run_off() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    let config = |limits: &str| {
        format!(
            "defaults:\n  isolation: none\n{limits}runners:\n  executor:\n    \
             - {{ adapter: claude-code, model: some-model }}\nadapters:\n  claude-code:\n    \
             binary: {binary}\n",
            binary = claude_code_stub().display()
        )
    };
    write(&repo.join(".yunta/config.yaml"), &config(""));
    write(
        &repo.join("wf.yaml"),
        "name: budgeted\nnodes:\n  - id: implement\n    kind: prompt\n    runner: executor\n    \
         prompt: \"Do the thing.\"\n",
    );
    // 500 tokens a run, so the p90 this history earns is a number a cap
    // can sit under.
    write(
        &repo.join(".claude-stub-lines.jsonl"),
        &format!(
            "{}\n{}\n",
            r#"{"type":"system","subtype":"init","session_id":"sess-cli","model":"claude-sonnet-5"}"#,
            r#"{"type":"result","is_error":false,"result":"done","usage":{"input_tokens":400,"output_tokens":100}}"#,
        ),
    );
    // Isolation `none` runs in this very checkout and refuses a dirty one.
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "workflow and stub fixture"]);

    // The distribution stays silent below its own three-run floor, so
    // three runs are what earn this workflow one at all.
    for _ in 0..3 {
        let past = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
        assert!(past.status.success(), "stderr: {}", stderr(&past));
    }

    // A cap under what this workflow has historically spent, which is what
    // the warning is about. Each detached child then stops on that very
    // cap, exactly as the warning says it may.
    write(
        &repo.join(".yunta/config.yaml"),
        &config("limits:\n  max_tokens_per_run: 400\n"),
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "budget"]);

    // Every detached child is a process this test started: letting each
    // one reach its own stop keeps the next invocation off a moving log
    // and leaves nothing running once the temporary tree is gone.
    let settled = |run_id: &str| {
        let status = || stdout(&yunta_in!(&repo, &home, &["status", run_id]));
        wait_until(
            || {
                let text = status();
                ["finished", "failed", "needs you", "waiting", "stopped"]
                    .iter()
                    .any(|stop| text.contains(stop))
            },
            || format!("the detached run never reached a stop: {}", status()),
        );
    };

    // A detached run that parks on its cap stays parked *in this
    // checkout*, and keeps it: that is what `isolation: none` means and
    // what its lock is for. So each mode below gets a checkout of its
    // own, all sharing the one state root the history lives in.
    let another_checkout = |name: &str| {
        let other = root.path().join(name);
        std::fs::create_dir_all(&other).unwrap();
        init_repo(&other);
        write(
            &other.join(".yunta/config.yaml"),
            &config("limits:\n  max_tokens_per_run: 400\n"),
        );
        write(
            &other.join("wf.yaml"),
            "name: budgeted\nnodes:\n  - id: implement\n    kind: prompt\n    runner: executor\n    \
             prompt: \"Do the thing.\"\n",
        );
        write(
            &other.join(".claude-stub-lines.jsonl"),
            &std::fs::read_to_string(repo.join(".claude-stub-lines.jsonl")).unwrap(),
        );
        git(&other, &["add", "."]);
        git(&other, &["commit", "-q", "-m", "budget"]);
        other
    };

    let loud = yunta_in!(&repo, &home, &["run", "wf.yaml", "--detach"]);
    assert!(loud.status.success(), "stderr: {}", stderr(&loud));
    let text = stdout(&loud);
    let lines: Vec<&str> = text.lines().collect();
    // Three past runs, not four: the history is folded before this
    // invocation creates a run of its own, which is the only ordering in
    // which the warning beside it still precedes every token spent.
    let distribution = lines
        .iter()
        .position(|line| line.starts_with("3 past runs · "));
    let handoff = lines.iter().position(|line| line.contains("detached"));
    assert!(
        matches!((distribution, handoff), (Some(shown), Some(gone)) if shown < gone),
        "the distribution reaches the reader before the run is handed off: {text}"
    );
    assert!(
        stderr(&loud).contains("max_tokens_per_run") && stderr(&loud).contains("p90"),
        "the budget warning names the cap and what history says: {}",
        stderr(&loud)
    );
    // The CLI marks its own cautions, and marks each one once: the
    // sentence it prints states the fact and nothing about how it looks.
    let loud_stderr = stderr(&loud);
    let marked: Vec<&str> = loud_stderr
        .lines()
        .filter(|line| line.contains("max_tokens_per_run"))
        .collect();
    assert_eq!(marked.len(), 1, "the warning is printed once: {marked:?}");
    assert!(
        marked[0].starts_with("warning: ") && marked[0].matches("warning:").count() == 1,
        "one layer decides how a caution looks, so the prefix appears once: {}",
        marked[0]
    );
    settled(&run_id_from(&loud));

    let quiet_repo = another_checkout("quiet");
    let quiet = yunta_in!(
        &quiet_repo,
        &home,
        &["run", "wf.yaml", "--detach", "--quiet"]
    );
    assert!(quiet.status.success(), "stderr: {}", stderr(&quiet));
    let quiet_text = stdout(&quiet);
    let bare = quiet_text.strip_suffix('\n').expect("one line of output");
    assert!(
        bare.len() == 26
            && bare
                .bytes()
                .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase()),
        "--quiet prints the bare run id and nothing else: {quiet_text:?}"
    );
    assert!(
        stderr(&quiet).contains("max_tokens_per_run") && stderr(&quiet).contains("p90"),
        "the warning asks for a decision before anything is spent, so it survives: {}",
        stderr(&quiet)
    );
    settled(&run_id_from(&quiet));

    let json_repo = another_checkout("json");
    let json = yunta_in!(&json_repo, &home, &["run", "wf.yaml", "--detach", "--json"]);
    assert!(json.status.success(), "stderr: {}", stderr(&json));
    let document: serde_json::Value = serde_json::from_slice(&json.stdout)
        .unwrap_or_else(|e| panic!("`--json` prints one document and nothing else: {e}"));
    // The run's own word, from its own log: a run just handed off is
    // one its log calls `created`, or `running` once the child it was
    // handed to has started it. No surface reports `detached` — that is
    // something this invocation did, not a state the run is in.
    assert!(
        matches!(document["outcome"].as_str(), Some("created" | "running")),
        "{document:#}"
    );
    assert!(
        stderr(&json).contains("max_tokens_per_run") && stderr(&json).contains("p90"),
        "the warning reaches the person watching: {}",
        stderr(&json)
    );
    // And the reader who is watching nothing. A caller that asked for a
    // document reads stdout; a caution it never sees cannot ask it for
    // the decision the caution exists to ask for.
    assert!(
        document["budget_warning"]
            .as_str()
            .is_some_and(
                |warning| warning.contains("max_tokens_per_run") && warning.contains("p90")
            ),
        "the document carries the warning too: {document:#}"
    );
    assert!(
        !document["budget_warning"]
            .as_str()
            .unwrap_or_default()
            .starts_with("warning:"),
        "undecorated: how a caution looks is the terminal's word, not the document's: {document:#}"
    );
    let run_id = document["run_id"]
        .as_str()
        .expect("the document names the run");
    settled(run_id);
}
