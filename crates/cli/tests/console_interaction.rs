//! `ConsoleInteraction` reads a person's answer on a blocking thread, so a
//! prompt awaiting input never freezes the run's single-threaded runtime.
//! The tasks that share that runtime — the per-node run-tools listener, a
//! `--follow` follower, the Ctrl-C handler — must keep running while a
//! prompt waits. Driven end to end: a real `yunta run` with a pty on stdin
//! (so the console surface engages instead of degrading to a pause)
//! reaches a gate prompt, and a SIGINT is still handled while it waits —
//! which happens only if the read is off the runtime thread.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use nix::pty::openpty;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use yunta_testkit::{git, init_repo, wait_until, write};

/// Waits until the console output `seen` contains `needle`, failing with
/// `context` and everything seen so far.
fn wait_for_output(seen: &Arc<Mutex<String>>, needle: &str, context: &str) {
    wait_until(
        || seen.lock().unwrap().contains(needle),
        || format!("{context}\nsaw so far:\n{}", seen.lock().unwrap()),
    );
}

#[test]
fn console_prompt_does_not_stall_the_run_tools_listener() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    // A bash lint that fails with its reroute budget already spent
    // escalates a gate — reached with no agent at all. On a real terminal
    // the console surface prompts for the decision and blocks on the
    // answer.
    write(
        &repo.join("wf.yaml"),
        "name: gated\nnodes:\n  - id: lint\n    kind: bash\n    run: \"false\"\n    \
         on_failure: { goto: fix, max_reroutes: 0 }\n  - id: fix\n    kind: bash\n    \
         run: \"true\"\n",
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "wf"]);

    // The child's stdio is a real terminal (the pty slave), so
    // `is_terminal()` is true and the console surface engages; we read what
    // it writes from the master end.
    let pty = openpty(None, None).unwrap();
    let stdin: Stdio = pty.slave.try_clone().unwrap().into();
    let stdout: Stdio = pty.slave.try_clone().unwrap().into();
    let stderr: Stdio = pty.slave.into();
    let mut child = Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(["run", "wf.yaml"])
        .current_dir(&repo)
        .env("YUNTA_HOME", &home)
        .stdin(stdin)
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .unwrap();

    // Drain the master in the background — the prompt and any notes the run
    // prints land here.
    let seen = Arc::new(Mutex::new(String::new()));
    let seen_reader = seen.clone();
    let reader = std::thread::spawn(move || {
        let mut master = std::fs::File::from(pty.master);
        let mut buf = [0u8; 1024];
        while let Ok(read) = master.read(&mut buf) {
            if read == 0 {
                break;
            }
            seen_reader
                .lock()
                .unwrap()
                .push_str(&String::from_utf8_lossy(&buf[..read]));
        }
    });

    wait_for_output(
        &seen,
        "choose an option id:",
        "the gate never prompted on the console",
    );

    // While the prompt waits for an answer, a SIGINT must still be handled:
    // the Ctrl-C task shares the run's runtime with the console read (and
    // the run-tools listener). A read that blocked the runtime would starve
    // every one of them, and this note would never print.
    kill(Pid::from_raw(child.id() as i32), Signal::SIGINT).unwrap();
    wait_for_output(
        &seen,
        "interrupt received",
        "SIGINT went unhandled while the console prompt waited — the read stalled the runtime",
    );

    let _ = child.kill();
    let _ = child.wait();
    let _ = reader.join();
}
