# Spec: `run-guard`

Module of `CAPABILITY-MAP.md`. Status: **approved on 2026-11-04, done on 2026-11-05**.
Base: `main` at `34790ab8`. Depends on: nothing.

## Objective

A run has at most one engine driving it.

### What happens at the base commit

Reproduced on macOS, with `isolation: worktree` and `isolation: none`, on a run whose only node is a bash loop:

- `yunta resume` on a moving run does not refuse. A second engine starts, replaces `scratch/engine.json`, runs the node again in the same checkout and appends to the same log. Both engines stay alive.
- `yunta cancel` then reaches only the newest engine and says `stopped`, while the first goes on. The log ends with `run_finished` after its `run_paused`.
- `yunta resume` on a workflow's child does the same while its parent drives it: the child's node starts twice.

Read in the code, not reproduced:

- `resume_run` over MCP takes the same path, and its child's error would reach only `detached.log`.
- On macOS `process_start` always answers `None`, so a reused pid reads as the engine that had it.

### Who this is for

- **A person at a terminal**, who runs `yunta resume` by hand, or twice by accident.
- **An agent over MCP**, which calls `resume_run`.
- **The application in `tui`**, which offers resuming on one key. That is why this comes first: it makes an existing problem easy to provoke.

### What each of them gets

1. As a person who resumes a run another engine drives, I am told who drives it and how to follow or stop it, and nothing about the run changes.
2. As a person who starts several resumes at once, exactly one drives and the others are refused.
3. As a person whose engine died, the next `yunta resume` takes over with nothing else to do, also when another process got that pid.
4. As an agent, `resume_run` gives me the same sentence with the whole id, and no process starts.
5. As a person who resumes a child its parent drives, I get what story 1 says.
6. As a person who uses `run --detach`, answers a gate, waits out a limit's reset or promotes a run, I see no change.

### What it says

| Case | The sentence |
|---|---|
| Another engine drives the run | `run Q78GRR is already moving: pid 91930 drives it` |
| That engine is asking at its terminal | `node tests is asking at its terminal (pid 91930); answer it there`, which exists at the base commit |
| The holder cannot be signalled | ``run Q78GRR is held by pid 91930, a process this user cannot signal (`<path>/scratch/engine.lock`): if nothing drives this run, delete that file by hand and resume it again`` |
| The lock names nobody readable | ``run Q78GRR has an engine lock with no readable owner (`<path>/scratch/engine.lock`): if nothing drives this run, delete that file by hand and resume it again`` |

- The first case offers `yunta status Q78GRR` ("follows it") and `yunta cancel Q78GRR` ("stops it") as the commands that go on from here, the way `RunNotFound` does.
- Every refusal exits 1. Under `--json` it is one `ErrorDocument` on stdout, with the whole id in `said` and those commands in `next`.
- Over MCP it is a tool error with the same sentence and the whole id.
- Commands are spelled in `commands/advice`.

### Decisions the plan starts from

- **A lock, not a check.** Two resumes at once would both pass a check. `crates/engine/src/lock.rs` has the protocol: an atomic `create_new`, an owner with a pid and a process start, and a dead owner replaced without a race.
- **The compiler demands it.** Driving a run requires a proof that only taking the lock can produce, so a new caller cannot forget it.
- **A refused resume takes nothing.** The lock is taken before a checkout is asked for.

### Assumptions

1. Whoever suffers this is a person at the CLI, an agent over MCP and, later, the application.
2. No legitimate flow needs two processes driving one run.
3. A workflow's child is protected the same way: resuming it while its parent drives it is refused.
4. There is no flag to skip the guard: a `--force` would allow two engines again.
5. A dead engine is replaced with nothing else to do. The one exception is a pid another user took on macOS: `proc_pidinfo` answers EPERM there (measured), `sysctl` would need `unsafe`, which the workspace forbids, and so the refusal names the file to delete. It takes an engine that dies without letting go, the pid counter wrapping around, and that pid landing on another user's process.
6. A run its engine just parked waits up to 5 s for that engine to leave instead of being refused, because `resolve-gate` can start the next engine right then. The 5 s is an estimate, and the proof with real runs measures it.
7. `resolve_gate`, `answer_questions`, `close` and the closing step of `cancel` stay out: they require a stopped run, SQLite serializes the rows, and the race with a resume is narrow. It is recorded as open work.
8. No new events, no new dependencies, and no time added to a run nobody competes for.

### Out of scope

- The writes of assumption 7.
- A sequence check when the log appends: the lock gives that guarantee.
- The wait for a limit's reset takes no lock, as `docs/troubleshooting.md` says. If two resumes wait for one reset, one drives and the other is refused when the time comes.

## Tech Stack

Rust at the version `rust-toolchain.toml` pins, tokio, and `yunta_engine::lock`. On macOS, `libproc`, already a dependency of `yunta-core`. No new dependency. The workspace sets `unsafe_code = "forbid"`.

## Commands

The work happens in the worktree `/Users/eulke/development/yunta-tui`, on the branch `feature/yunta-tui`.

```sh
# Before editing a file, and before finishing with no paths
cargo xtask docs --about <files>

# Build
cargo build --workspace --locked
cargo build --release --locked -p yunta

# Test
cargo test -p yunta-core --test process_start
cargo test -p yunta-engine --test run_guard
cargo test -p yunta --test run_flow guarding
cargo test -p yunta --test mcp_flow
cargo test --workspace --locked --no-fail-fast

# Lint and checks
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo xtask smells --check
cargo xtask docs --check
cargo xtask schema --check
```

Before a commit, every step of the `ci` job in `.github/workflows/ci.yml` runs, one build at a time, with `CARGO_INCREMENTAL=0`. After it, `cargo clean`.

## Project Structure

```
crates/core/src/process/            when a process started, on macOS too
crates/core/tests/process_start.rs  its tests
crates/engine/src/lock.rs           the lock protocol; reading a lock without taking it
crates/engine/src/lock/tests.rs     NEW: its tests, moved out when the file passed 500 lines
crates/engine/src/run_hold.rs       NEW: the claim that one process drives a run
crates/engine/src/run/              a run cannot be driven without the claim; a child takes its own
crates/engine/tests/run_guard.rs    NEW: the claim, at the engine's level
crates/testkit/src/bench/           the test harness claims a run like anyone else
crates/cli/src/commands/            resume, run, promote, test and mcp take the claim or refuse
crates/cli/src/error.rs, error/     the refusal and its sentences
crates/cli/src/output.rs            the commands of a refusal, named for whoever reads them
crates/cli/tests/run_flow/          NEW module `guarding.rs` of a test binary that exists
crates/cli/tests/mcp_flow.rs        the refusal an agent reads
docs/troubleshooting.md, README.md  what the refusal means, for whoever uses Yunta
.agents/                            the rule, the open race, the absent sequence check and the absent flag
SPEC-run-guard.md                   this file
tasks/plan.md, tasks/todo.md        this module's plan and task list
```

## Code Style

```rust
/// Proof that this process drives a run: `RunEnv` borrows one and only
/// [`hold_run`] makes one, so no run is driven without it. Released on drop; a
/// process that dies without dropping it is replaced by the next holder.
pub struct RunHold {
    run_dir: PathBuf,
    owner: LockOwner,
}

/// How long a parked run's next engine waits for the engine that parked it: that
/// engine leaves moments after it writes the pause, and a longer wait would only
/// hide one that did not.
const LEAVING: Duration = Duration::from_secs(5);
```

The repository's rules hold:

- A numeric `const` carries a `///` line saying why it has that value.
- A printed sentence carries no dash: a colon explains, a semicolon comes before the hint it ends with.
- Code, tests, commits and pull requests cite no document.
- English, in the present tense.

## Testing Strategy

`cargo test`, with integration tests under each crate's `tests/` and the helpers of `yunta_testkit`.

| Level | What it proves |
|---|---|
| CLI, black box | A second resume on a moving run exits 1, says the handle, the pid and both commands; `engine.json` keeps the same pid; the node starts once; the log gains no `run_resumed`; the run then ends well. With both isolation modes |
| CLI, black box | Four resumes at once: one drives, three exit 1, the log gains one `run_resumed` |
| CLI, black box | An engine killed with `kill -9` leaves its lock, and the next resume replaces it |
| CLI, black box | A resume on a child its parent drives is refused, and the child's node starts once |
| CLI, black box | An unreadable lock is named and left alone, and a run parked on a far reset refuses at once instead of waiting |
| MCP | `resume_run` on a moving run returns the error with the whole id, and starts nothing |
| Engine | Of many attempts at once, exactly one holds the run. A dead holder or a reused pid is replaced; a live, doubtful or unreadable one never. A parked run's next engine waits; a moving run is refused at once. An interrupt ends the wait |
| Unit | `holder_state` with a fake probe: a later start reads as dead, with `Alive` and with `Unknown`. Reading a lock takes nothing |
| Unit | The sentences of the refusal, written by hand |
| Core | A child started now has a start between just before the spawn and now, on both platforms. A pid nothing runs under has none. Pid 1 has none on macOS and has one on Linux |
| Regression | `run_flow`, `mcp_flow`, `stalled_runs`, `close_cmd`, `run_gates_limits`, `workflow_compose` and `promotion` stay green with no change |

Rules for these tests:

- Expected values are known apart from the code.
- A test that claims concurrency makes the two sides wait on each other, so it fails when the work does not overlap.
- A test whose subject can block waits under a deadline, so a missing guard fails it instead of hanging it: with no guard, a second resume drives the run until the node ends.

**Proof with real runs**, because this is on the run path:

1. Before the change, with a release build: the double engine reproduced, with what the log and `engine.json` show.
2. After it: the same sequence refuses.
3. A fragua run started with `--detach`, stopped before `pr`, its gates answered with `yunta resolve-gate`, comparing `yunta stats` before and after.
4. How long passes from `run_paused` to the lock going away, against the 5 s of assumption 6.

## Boundaries

**Always**
- Work in this worktree, and stage paths by name.
- Reproduce the bug before fixing it.
- Keep the goldens intact, except for the new sentences.

**Ask first**
- Add a dependency, change `deny.toml` or touch `.github/workflows/ci.yml`.
- Add or change events of the log. This spec needs none.
- Change what `docs/troubleshooting.md` says about waiting for a limit's reset.
- Change a test that passed at the base commit. If a flow turns out to need two engines, the work stops there.

**Never**
- Let a refused resume take a checkout or write to the log.
- Skip the guard with a flag or a mode.
- Add waiting time to a run nobody competes for.

## Success Criteria

1. A second `yunta resume` on a moving run exits 1, names the pid and both commands, and leaves the log and `engine.json` untouched.
2. With several attempts at once, one process holds the run.
3. A run whose engine died resumes with nothing else to do, also with a reused pid, on Linux and on macOS. The one exception is a pid another user took on macOS: the refusal says so and names the file to delete.
4. A resume on a child its parent drives is refused like any other.
5. `execute_run` does not compile without the proof that the lock was taken.
6. `run --detach`, answering a gate followed by a resume, waiting out a limit's reset and promoting to a successor work as at the base commit.
7. `resume_run` over MCP refuses with the same sentence and starts no process.
8. `yunta stats` shows no added engine time on a real run.
9. Every step of the `ci` job passes.

## Open Questions

None. Where this lives, how far the work goes and the assumptions above were answered on 2026-11-04.
