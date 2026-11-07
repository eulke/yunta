# Spec: `session-ends`

Module of `CAPABILITY-MAP.md`. Status: **approved on 2026-11-06**.
Base: `feature/yunta-tui` at `7d2752fe`. Depends on: nothing. Needed by: nothing in the map; every run whose sessions stand on Codex or run commands.

## Objective

A session ends when its CLI says its turn ended, and when it ends, nothing its CLI started keeps running, whether that process still hangs from the CLI or its parent already left. Two bugs the agent audit of 2026-11-03 confirmed are still there at the base: a Codex `error` line ends its session even when Codex prints it for a retry it goes on from, and a command an agent started outlives the session that ran it on every way a session ends.

### What happens at the base commit

Reproduced with scripted CLIs and no tokens, on a release build of `7d2752fe` (`.plans/yunta-tui/proof/session-ends/repro.py` and `fake_cli.sh`, result in `before.txt`, outside git):

- **A retry fails the node.** A Codex stream of `thread.started`, `{"type":"error","message":"Reconnecting... 2/5 (unexpected status 503 Service Unavailable)"}`, an agent message and `turn.completed` ends the run at `needs you`, exit 3: `session codex refused the request: Reconnecting... 2/5 (unexpected status 503 Service Unavailable)`. `codex/parse.rs` maps every top-level `error` line to `AgentEvent::Failed`, and the stream guard ends the session at its first terminal event.
- **Codex prints no flag that tells a retry from a final error.** In its source on `main`, a retry is a `StreamError`, sent with `will_retry: true`, and a final error is an `Error`, sent with `will_retry: false`; `codex exec --json` prints both as `{"type":"error","message":...}` and drops the flag. Only a final error becomes the turn's error, and a turn with an error always ends with `turn.failed` carrying it. On the installed 0.157.0, the lowest the adapter accepts, a refused model prints `error` and then `turn.failed` with the same message (probed with no tokens spent). Claude Code's retry notice, `system/api_retry`, is already a line its parser drops.
- **A command in a group of its own outlives the session, every way the session ends.** The scripted CLI starts `sleep` in a process group of its own, as both real CLIs start each tool command (measured by the audit with both real CLIs on 2026-11-03). On `claude-code` and on `codex` alike, the `sleep` was still running, its parent now pid 1, after the session ended by itself, after `yunta cancel` stopped a run whose engine lived, and after `yunta cancel` stopped a run whose engine had been killed.
- **A command whose parent already left outlives it too.** A `sleep` started from a shell that exits at once, as a `cmd &` in a shell tool does, was a child of pid 1 before the session ended, and still ran after it.
- **Why.** `SubprocessSession` (`core/process/subprocess.rs`) signals the CLI's own group: its drop after a session's own end sends SIGKILL to it, and `kill` and `exit` close it until no member runs. `yunta cancel` and `yunta close` after a crash close the groups the registry names, which are the CLIs'. None of them reaches a group the CLI made, nor a process that left the CLI's tree. `AgentSession::kill` promises it "never leaves anything running", and `docs/compatibility.md` says each subprocess is "killed with its whole tree": neither holds.
- **What the audit measured.** A longer wait after SIGINT does not fix it: with 3 s the command survived, because Codex takes 0.2 to 3.2 s to clean up, and with 10 s it worked only on a timeout, cost time, and did nothing for a session that ends by itself. Walking the CLI's descendants by parent pid before the kill fixed every real path where the command still hung from the CLI, with no wait.
- **What a child inherits.** Every process a CLI starts inherits its environment unless it is cleared on purpose. Codex builds its commands' environment from `shell_environment_policy`: on `main` it inherits the whole environment by default, its default filter of names with `KEY`, `SECRET` or `TOKEN` is off unless configured, and a `set` entry is added after every filter but `include_only`. The adapter sets no `shell_environment_policy` today.

### Who this is for

- **A person whose run stands on Codex when the service blinks.** The node goes on as Codex does, instead of stopping for a person on a failure that never happened.
- **A person whose agent started a dev server, a watcher or a long test.** It stops with the session, so no port, CPU, file lock or write into the checkout is left behind for the next session or the next run.

### What each of them gets

- **An `error` line ends nothing.** A Codex turn ends on `turn.completed` or `turn.failed`, as Codex decides. The message of `turn.failed` is the failure, read and classified as today, so a refused model or a reached limit fails as it does at the base. A stream that closes with neither is a session that died, as the port's contract says for every adapter. A retry writes nothing to the log.
- **Nothing a CLI started outlives its session.** At every end of a session (its own end, a kill on a cancel, a timeout or a budget, the drop of a session nobody ends, and `yunta cancel` or `yunta close` of a run whose engine died):
  1. every process group under the CLI is closed while the CLI still lives, found by following parent links;
  2. the CLI's own group is closed, as today;
  3. every process of the user that carries the session's mark is closed, wherever its parent is.

  Each group and each process is closed as the CLI's group is, killed until nothing of it can execute. One that will not close fails the node as `session_not_stopped` and stays in the run's registry, as the CLI's does.
- **What stays out, and why.** A process that clears its environment and has also left the CLI's tree carries nothing that ties it to the session: a daemon made that way on purpose. Neither is a process the system starts at an agent's request, such as `open` or `launchctl` on macOS, `systemd-run` on Linux, or a container a daemon runs for `docker run`: its parent is the system, not the session. Holding those too takes containment from the operating system, which Linux has (cgroups, a child subreaper) and macOS does not offer without privileges.
- **What macOS hides.** macOS hides the environment of its own platform binaries from every other process: measured at Checkpoint A, `/bin/sleep`, `/bin/sh`, `/bin/zsh`, `/usr/bin/perl` and `/usr/bin/ruby` are hidden, while `/usr/bin/python3`, `node` and any binary built apart are readable, under a sandbox or not. So on macOS the sweep finds a process whose parent left only when it is not one of those binaries; a command still hanging from the CLI is closed by the walk whatever it runs. On Linux `/proc` shows every process of the user. The user chose to keep the sweep with this said in the docs (option a at Checkpoint A), over the walk alone. Corrected after the module's commit: GitHub's macOS runner showed a `/bin/sleep`'s environment, so macOS can hide it rather than always does. The Mac measured, with System Integrity Protection on, hides it. Where the host shows it, the sweep finds such a process, and the test compares the sweep with what `ps` shows.
- **The same on every adapter.** The adapter contract holds both fixes for every subject: a retry notice in each dialect ends nothing, and a command the CLI started, in a group of its own or left by its parent, does not outlive the session. `mock` runs no process of its own, so the second case holds for it with nothing to close.

### Decisions the plan starts from

1. **`turn.completed` and `turn.failed` are Codex's only terminal lines.** The `error` line becomes no event. Rejected: keeping the last `error` and reading it back as the failure when the stream closes with no end, which the audit's prototype did. It makes up a terminal event where the port's contract says the session died, and a CLI killed in the middle of a retry would read as one that refused the request.
2. **Each session's CLI carries a mark in its environment.** The value is unique to the session and names its run, so the sessions of one run and the runs of one user never share it, and `yunta cancel` and `yunta close` find what a dead engine's sessions left from the run's id alone, with nothing new in the registry. The variable's name holds none of `KEY`, `SECRET` or `TOKEN`, so no default filter drops it. On Codex the adapter also sets it through `shell_environment_policy.set`, which Codex applies after its own filters. A person's own `include_only` that leaves the name out still drops it, because Codex applies that after `set`; the docs say to name it there, and the walk still reaches those commands while they hang from the CLI (corrected with the plan). The mark is not a secret: it opens nothing.
3. **Two ways to find, one order.** The walk by parent links comes first, while the CLI lives, because it also reaches a process that cleared its environment while it still hangs from the CLI. The sweep by mark comes after the CLI's group is closed, and reaches what left the tree. The sweep repeats until it finds nothing, so a process that forked while it was being closed is closed too. A pid is read with its start time and signalled only if it still has that start time, as the registry already tells a live engine from a recycled pid.
4. **Both live in `core/process`**, beside the group closing they serve, so the session, its drop and `yunta cancel` reach them the same way. The walk lists children by parent pid with what the workspace already depends on (`libproc` on macOS, `procfs` on Linux). Neither ever signals the caller's own process or group, nor a pid or group id of 1 or less.
5. **Rejected:** a longer grace (measured above). A walk at each tool call, which misses a `cmd &` that leaves its parent within the call: Claude reports a call before running it, and Codex after it ends. Linux's child subreaper and cgroups, which macOS has no equivalent of, so the two platforms would behave apart. Starting the CLI in a session of its own, which takes its controlling terminal and still misses a command that calls `setsid`, as a detached Node child does. Reading the environment through `ps eww`, which prints another tool's text where a value may hold spaces.
6. **Parity goes through the contract.** The contract's steps gain a retry notice, written as each CLI writes it: Claude Code's `system/api_retry` line and Codex's top-level `error` line. The mock gains the same step and renders it as its dialect's line, so a mock fixture in the Codex dialect meets the same parser. The stubs gain knobs that start a command in a group of its own, and one whose parent leaves at once. The mock has no knob because it has no process.
7. **The cost is measured, not assumed.** The walk and the sweep run once per session end. With stub sessions on release builds, runs take turns on the base and on the module, 15 each, and their medians of time between nodes must match within noise. The time one end spends walking and sweeping is measured on its own too.
   - Measured on 2026-11-06: the time between nodes matched (179.7 against 181.1 ms). But a session's end is booked as its node's work, and there each finished session's end added about 11 ms; with a command hanging from the CLI, an end took 33 ms (45 at most). Two causes. A group close looked again only 10 ms after its SIGKILL. And the sweep read the environment of every process on the machine, 7 ms for about 570.
   - The user chose to remove that cost before the real runs. A close looks again 1 ms after its signal, then every 10 ms as before. A session's sweep reads the environment only of the user's processes that started no earlier than a minute before its CLI did. A process that started earlier cannot have inherited the CLI's environment, and the minute covers a clock stepped back in between. `yunta cancel` and `yunta close` know no start for a dead engine's sessions, so they still read every process.

### Assumptions

1. Both real CLIs at the versions installed here still start each tool command in a group of its own, and pass their own environment to it, mark included. Measured at Checkpoint A on the base's release build (`real-before.txt`, `real-env-before.txt` and `checkpoint-a.txt` beside `repro.py`):
   - Through each shell tool's background mode, the command ran in a group of its own, under the tool's shell, under the CLI. It outlived the session.
   - Each CLI passed its environment on: a probe set on `yunta`'s environment was in the environment of the CLI and of its `python3` command.
   - Asked for `cmd &`, Claude Code refused a long command outside its background mode, and Codex ended what its command left behind on its own.
   - A Claude Code session counted about 156k to 272k tokens, most of it cached input, and a Codex session about 39k.
2. A final Codex error is always followed by `turn.failed` at 0.157.0 and later, so this change moves no refused or limited session onto another cause or message. The repro's refused-model case and the adapter suite hold that.
3. No new event kind, no new field, and no change to what an append writes or to `engine.json`. A retry is not recorded anywhere a person reads. Showing it live is later work that the map does not hold.
4. A real run proves it after. A fragua run is started with `--detach`, its `reviewer` runners on Codex `gpt-6-luna` and the rest on Claude Code, followed at a terminal up to `ship`, then `abort` and `close`, about 2.3M tokens. Its time between nodes, read with `yunta stats`, is compared with the 0.614 s that `run-words` measured. The two small runs of assumption 1 run again after: each command is gone once its session ends.
5. `tasks/plan.md` and `tasks/todo.md` are replaced for this module. Those of `run-words` stay in its commit.
6. The sweep of stale sockets in `/tmp/yunta-<uid>/` that `run-words` left open is not part of this module: it is a socket file, not a process, and it stays as `run-words` left it.

### Out of scope

- Showing a retry, the command running now, its output or a session's plan.
- Codex App Server.
- What a bash node, a hook or an executor command starts: they are the engine's own processes, not an agent's.
- What stays out by the reasons above: a process that cleared its environment and left the tree, and one the system starts at an agent's request.
- A session's process outliving an engine that died with nobody running `yunta cancel` or `yunta close` for its run.
- Windows.

## Tech Stack

Rust at the version `rust-toolchain.toml` pins; `rustix`'s `getpgid`, `libproc`'s parent filter on macOS and `procfs` on Linux, all already dependencies of `yunta-core`; the existing group closing, process start times, registry, stubs and adapter contract. Reading another process's environment needs `/proc/<pid>/environ` on Linux, which `procfs` reads, and `sysctl(KERN_PROCARGS2)` on macOS, which the workspace cannot call without `unsafe` (forbidden in its own code). That takes a new dependency, the first Open Question.

## Commands

`cargo` means the wrapper that keeps this worktree's build target and state separate from other checkouts: `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.

```
# Before editing a file, and before finishing with no paths
cargo xtask docs --about <files>

# Build
cargo build -p yunta
cargo build --release --locked -p yunta

# Test
cargo test -p yunta-core --test <the process target>
cargo test -p yunta-adapters --test contract
cargo test -p yunta --test <the session's end target>
cargo test --workspace --locked --no-fail-fast

# Lint and checks
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo xtask smells --check
cargo xtask docs --check
cargo xtask schema --check
cargo deny check

# Reproduction, before and after
python3 .plans/yunta-tui/proof/session-ends/repro.py <yunta binary>
```

Before a commit, every step of the `ci` job in `.github/workflows/ci.yml` runs, one build at a time, with `CARGO_INCREMENTAL=0`. After the commit, `cargo clean`. `cargo xtask smells` never runs without `--check`.

## Project Structure

```
crates/adapters/src/codex/parse.rs              an `error` line is no event
crates/adapters/src/codex/config.rs             the mark, set through `shell_environment_policy.set`
crates/adapters/src/mock/                       a retry notice step, rendered in its dialect
crates/adapters/tests/contract/                 a retry notice ends nothing; no command outlives the session
crates/core/src/process/group.rs                the walk: the groups under a leader, closed before the leader's
crates/core/src/process/<the mark's module>     NEW: the mark, the sweep of what carries it, a process's environment read
crates/core/src/process/subprocess.rs           a session's CLI carries its mark; every end walks, closes, then sweeps
crates/core/src/port/session.rs                 the kill contract says what is true
crates/core/tests/                              the walk and the sweep, and a session's ends, with real child processes
crates/testkit-core/stubs/                      knobs that start a command in a group of its own, or one its parent leaves
crates/cli/src/commands/cancel.rs               after a crash, `yunta cancel` and `yunta close` walk each registered CLI and sweep the run's mark
crates/cli/tests/                               whole runs through the stubs: a retry goes on; no command left after an end or a cancel
docs/                                           what a person can rely on, and what stays out
```

## Code Style

The shape, for illustration. The plan decides the names.

```rust
/// Closes every process of this user that carries `mark` in its
/// environment, wherever its parent is: what a session's CLI started and
/// then left to pid 1. Repeats until a pass finds nothing, and signals a pid
/// only while it keeps the start time it was found with. Never the caller,
/// and never a pid of 1 or less.
pub async fn close_marked(mark: &SessionMark) -> Result<(), GroupError> {
```

The repository's own rules hold: English in the present tense, a `//!` for each module that says how it works, a `///` with a reason for each numeric const, files under 500 lines, functions under 50, `// blocking:` above each blocking call in the engine, and no document cited in code, tests, commits or the pull request.

## Testing Strategy

| Level | What it proves |
|---|---|
| Unit, core | The walk finds a grandchild in a group of its own under a leader, and nothing under a leader with no children. The sweep finds a process with the mark whose parent left, and none without it or with another session's mark. A pid whose start time changed is never signalled. Neither ever names the caller, its group, or a pid or group id of 1 or less |
| Core, process | A session whose CLI started a command in a group of its own, and one whose command's parent left, leave nothing running after `kill`, after `exit`, and after the session is dropped, on Linux and macOS |
| Adapter contract | For every subject, a retry notice ends nothing and the session ends on its turn's own end. A command the CLI started, in a group of its own or left by its parent, is gone once the session ends |
| Adapter, Codex | A refused model and a reached limit still fail with the cause and message they have at the base, through `turn.failed`. The CLI's arguments set the mark through `shell_environment_policy.set` |
| CLI, black box | Through the stubs: a Codex run whose stream carries a retry finishes; no command is left after the session's own end, after `yunta cancel` with the engine alive, and after `yunta cancel` or `yunta close` once the engine was killed |
| Measure | Stub runs on release builds, base and module, 15 each: the median time between nodes, and the time one end spends walking and sweeping. The real runs of assumptions 1 and 4 |

Rules for these tests:

- Expected values are known apart from the code.
- A test that waits for a process waits on what it writes or what the process table shows, never for a fixed time.
- A test that claims two things happen at once makes them wait on each other.
- A test that starts a process kills it at its end, whether it passed or not.

Proof on a release build, with `repro.py`, whose result at the base is kept beside it, and with the real runs of assumptions 1 and 4.

## Boundaries

**Always**
- Work in this worktree, and stage paths by name.
- Reproduce before changing, and measure after.
- Close the groups under a CLI while it lives, then its own, then what carries its mark.

**Ask first**
- A new dependency, the one the first Open Question names included, or any change to the log, to an event, to what an append writes, or to `engine.json`.
- Signalling any process neither the walk found under a session's CLI nor the sweep found carrying its mark.
- Spending tokens beyond the real runs of assumptions 1 and 4.

**Never**
- Signal the caller, its group, or a pid or group id of 1 or less.
- Signal a pid whose start time is not the one it was found with.
- A wait added to a session's end to give a CLI time to clean up.
- Touching `~/.yunta` or the main checkout's uncommitted files.
- Running `cargo xtask smells` without `--check`.

## Success Criteria

1. A Codex session whose stream carries an `error` line and then goes on ends as its later lines say: finished on `turn.completed`, failed with the message of `turn.failed` when that follows. `repro.py`'s retry case finishes with exit 0.
2. A refused model and a reached limit on Codex fail with the cause and message they have at the base.
3. Claude Code's `system/api_retry` and the mock's retry notice end nothing either, and the adapter contract holds it for every adapter.
4. A command a CLI started is gone once its session ends, whether it hung from the CLI in a group of its own or its parent had left: by itself, on a kill for a cancel, a timeout or a budget, when the session is dropped, and on `yunta cancel` or `yunta close` after its engine died. This holds on macOS and Linux, through both stubs, and in every case of `repro.py`, with one exception: on macOS, a process whose parent left and that runs one of the binaries macOS hides is not found, and the docs say so.
5. Nothing signals the caller, its group, a pid or group id of 1 or less, a process without the session's mark that the walk did not find, or a pid whose start time changed. A group or process that will not close fails the node as `session_not_stopped` and stays in the registry.
6. With each real CLI, a command the agent started in the background, both through the shell tool's background mode and as `cmd &`, is gone once the session ends; at the base it was not.
7. Measured on release builds: stub runs on the base and on the module have the same median time between nodes, and the same median working and wall time, within noise, over 15 runs each. The real fragua run's time between nodes is in line with 0.614 s.
8. The port's kill contract, `docs/compatibility.md` and `docs/adapters.md` say what is true, what stays out among them.
9. Every step of the `ci` job passes on macOS and Linux, `cargo deny` included.

## Open Questions

Answered on approval:

1. **The dependency that reads another process's environment on macOS.** The workspace forbids `unsafe` in its own code, and `KERN_PROCARGS2` needs it. The plan proposes one crate with its license, what it adds to the tree and to the binary, and whether `deny.toml` accepts it as it is, and the user approves it with the plan; `sysinfo`, under MIT, is the first candidate. Linux needs nothing new: `procfs` reads `/proc/<pid>/environ`.
2. **The tokens of the real runs** are approved: a few thousand tokens per CLI for assumption 1, before any code and again after, and about 2.3M tokens for the fragua run of assumption 4, on the user's subscriptions.
