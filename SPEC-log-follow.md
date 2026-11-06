# Spec: `log-follow`

Module of `CAPABILITY-MAP.md`. Status: **approved on 2026-11-05**.
Base: `feature/yunta-tui` at `06607c50`. Depends on: nothing. Needed by: `control`, and through it the "watch" slice of `tui`.

## Objective

A run can be followed from any process while it moves, reading its log as it grows: the log is read from a position, each run's last position is read in one go, and `yunta status <run> --follow` shows a person at another terminal what `yunta run` would have shown them.

### What happens at the base commit

Reproduced with bash nodes and no tokens (`.plans/yunta-tui/proof/log-follow/repro.sh`, results in `before.txt`, outside git):

- **No process can follow a run it did not start.** `yunta run wf.yaml --detach` says ``run 01M6...45C7WY: detached, driving forward independently (`yunta status 45C7WY` follows it)``, but `yunta status` reads the run once and exits 0. `yunta status 45C7WY --follow` is refused by the argument parser with exit 2. A person who detached a run, or who meets one another terminal drives, sees it only by asking again and again.
- **The view of a moving run exists only inside the process that drives it.** `yunta run` draws its region from the events the engine hands it in the same process (`RunObserver`), and reads the log only to close a hole. Nothing else receives them.
- **The log is only read whole.** `events_for_run` returns every event of a run, so a reader that already has most of them reads them all again. Nothing reads where a run stands without reading the run.
- **The log already keeps what following needs.** An append assigns `seq` and commits its row and the run's `run_heads` row in one transaction, so a reader in another process sees each run's events in order with no hole, and `run_heads` holds each run's last seq. The log is SQLite in WAL, so a reader never blocks the engine's writes.

### Who this is for

- **A person at another terminal**, who detached a run or meets one another terminal drives, and wants to watch it as if they had started it there.
- **A script**, which follows a run off a terminal and reads its outcome from the exit code, as it does from `yunta run`.
- **`control` and the application in `tui`**, which follow every run through the same reads. That is why this comes before them.

### What each of them gets

At a terminal, the region `yunta run` draws, from the log, and when the run stops the same block `yunta run` leaves:

```
$ yunta run fragua --detach
run 01M6PG912WJHJWHSE85WNP4SRF: detached, driving forward independently (`yunta status NP4SRF --follow` follows it)

$ yunta status NP4SRF --follow
  the region `yunta run` draws, moving as the log grows
  ...
  the block `yunta run` leaves when the run stops
$ echo $?
3
```

| Case | What it does |
|---|---|
| A run that moves | The region, from the log, until the run stops. Then the block an attached run leaves, and the exit code `yunta run` gives |
| Off a terminal, or `TERM=dumb` | The lines `yunta run` prints off a terminal, the first saying why, and the same block and exit code |
| A run that is not moving when it starts | What `yunta status` prints, and the run's exit code |
| Ctrl-C | Stops following and says the run goes on, with the command that follows it again. Exit 130. The run is not touched |
| The engine dies under it | Ends with what `yunta status` says of a stalled run. Exit 1 |
| The engine asks at its own terminal | The region says which node asks and at which terminal, and following goes on |
| A promotion | Follows the successor, as `yunta run` does |
| A parent | Its children under the nodes that bore them, as `yunta run` shows them |
| `--json`, `--full`, `--dig` or `--node` beside `--follow` | Refused by the argument parser, exit 2 |

Every place that offers `yunta status <run>` as what "follows it" offers `yunta status <run> --follow`: the line `run --detach` starts with, the same line attached `yunta run` starts with (approved at Checkpoint D, since the spec asks before any change to attached `yunta run`), the answer `resolve-gate` gives, and the refusal of a run another engine drives. In a document a program reads, that step names the run whole, as every step does.

For a program, the store gains two reads: the events of a run after a given seq, and each run's last seq.

### Decisions the plan starts from

- **It polls.** No daemon, no notification between processes and no new dependency: a follower reads what came after the last seq it holds, and a run that did not move costs one indexed read that returns nothing. The interval is the plan's to choose and measure.
- **A follower only reads.** It writes no event, takes no lock, touches no registry and never signals the engine, so any number of followers can watch one run and none changes it. It holds no read transaction between reads, so it never keeps the engine's log from being checkpointed.
- **One region, whoever feeds it.** The region and the closing block do not know whether their events came from the engine in the same process or from the log in another. What differs is only the feed.
- **It ends where `yunta run` ends.** The run's pause or its finish, read from its log, ends following with the same block and code. An engine gone from a run whose log says it moves is a stall, which `yunta run` never meets because it is that engine.
- **Exit codes follow the run, under `--follow` only.** `yunta status` without it still exits 0 whatever the run says.

### Assumptions

1. Attached `yunta run` does not change: its region keeps its feed from the engine, and its recovery read stays what it is.
2. MCP gets nothing new: an agent follows a run by asking `workflow_status` again, which already returns the whole document.
3. The head of each run enters here, as the map says. Its first reader in the binary is the follower of a promoted run, which looks for the successor among the runs it had not seen; `control` reads it next. A run written before `run_heads` existed has no row there, and its head is still read.
4. The words an agent says while it works are not shown: they reach other processes through `run-words`, never through the log.
5. A follower started on a run that has not moved yet, such as one `--detach` has just handed over, waits for it rather than ending.
6. A real run proves it: a fragua run started with `--detach` under no token cap, followed from a second terminal up to `ship`, about 3.1M tokens, then `abort` and `close`. Nothing is published. The base cannot follow, so the comparison is the run measured for `run-guard`, the same request with no cap: 3.07M tokens and 0.668 s between nodes.
7. `tasks/plan.md` and `tasks/todo.md` are replaced for this module. Those of `parked-answers` stay in its commit.
8. No time added to a run that is followed.

### Out of scope

- Following several runs at once, or `yunta list --runs` that keeps moving: the field of `tui` does that, through `control`.
- Answering from the follower. `yunta resolve-gate` answers from any process, and the follower then shows the run going on.
- Following a run on another machine.
- A stream of documents for a program under `--json`.
- The words an agent says, which belong to `run-words`.

## Tech Stack

Rust at the version `rust-toolchain.toml` pins, tokio, rusqlite on the existing log, and the existing region, closing block and replay. No new dependency.

## Commands

`cargo` means the wrapper that keeps this copy's target and state apart: `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.

```
# Before editing a file, and before finishing with no paths
cargo xtask docs --about <files>

# Build
cargo build -p yunta
cargo build --release --locked -p yunta

# Test
cargo test -p yunta-storage
cargo test -p yunta --test status_follow
cargo test --workspace --locked --no-fail-fast

# Output meant to change, then read the goldens' diff
YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine

# Lint and checks
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo xtask smells --check
cargo xtask docs --check
cargo xtask schema --check
```

Before a commit, every step of the `ci` job in `.github/workflows/ci.yml` runs, one build at a time, with `CARGO_INCREMENTAL=0`. After it, `cargo clean`. `cargo xtask smells` never runs without `--check`.

## Project Structure

```
crates/storage/src/store.rs, store/reading.rs, async_storage.rs   the events after a seq, and each run's head
crates/storage/tests/                           both reads, older logs included
crates/cli/src/cli/mod.rs, dispatch.rs          `--follow` on `status`, and what it conflicts with
crates/cli/src/commands/status/                 following: the page of a run that is not moving, the feed, the end
crates/cli/src/surface/                         the region fed from the log, the engine read from the registry, the row of a run asking
crates/cli/src/commands/drive/watch.rs          `yunta run` hands the surface its seed and reads the engine as itself
crates/cli/src/commands/advice.rs, mod.rs       `yunta status <run> --follow` where it "follows it"
crates/cli/src/commands/resolve_gate.rs, error.rs   the same in a gate's answer and in the refusal of a run another engine drives
crates/cli/tests/status_follow.rs, status_follow/   NEW: the black box, at a terminal and off one
docs/compatibility.md, README.md, docs/guide.md what a person and a script can rely on
.agents/                                        what the module teaches, in its one home
SPEC-log-follow.md                              this file
tasks/plan.md, tasks/todo.md                    this module's plan and task list
```

## Code Style

The shape, for illustration. The plan decides the names.

```rust
impl Storage {
    /// The events of `run_id` after `seq`, in log order: what a reader that
    /// already folded the run up to `seq` has not seen. Read the same way as
    /// [`Storage::events_for_run`], so an older kind comes back verbatim and a
    /// corrupt row fails the read.
    pub fn events_after(&self, run_id: &RunId, seq: Seq) -> Result<Vec<StoredEvent>> {
        // ...
    }
}
```

The repository's own rules hold: English in the present tense, a `//!` for each module that says how it works, a `///` with a reason for each numeric const, files under 500 lines, functions under 50, and no document cited in code, tests, commits or the pull request.

## Testing Strategy

| Level | What it proves |
|---|---|
| Unit, store | `events_after` returns exactly what `events_for_run` returns past the seq, in order, an unknown kind verbatim and a corrupt row as an error. The heads name every run once with its last seq, including a run with no `run_heads` row, and none for a purged run |
| CLI, pty | `status --follow` draws the region while a held run moves, and when it stops leaves the block and the exit code an attached `yunta run` of the same log leaves |
| CLI, black box | Off a terminal it prints the lines `yunta run` prints. A run that is not moving gives the page and the run's code. Each refused flag exits 2 |
| CLI, black box | Ctrl-C exits 130, says the run goes on, and the run still finishes. Two followers on one run both end, and the run's log, lock and registry are what they would be with none |
| CLI, black box | An engine killed under a follower ends it as a stall, exit 1. A run asking at its terminal is shown asking. A promotion is followed onto its successor |
| CLI, black box | `run --detach`, `resolve-gate` and the refusal of a driven run offer `--follow`, by handle for a person and whole in a document |
| Measure | With mock sessions, runs taking turns with and without a follower: the follower adds no time, and each event reaches it within the bound below |

Rules for these tests:

- Expected values are known apart from the code.
- A test that waits for a run waits on what it writes, never a fixed time.
- A test that claims two things happen at once makes them wait on each other.
- Goldens change only where a page offers `--follow`, and their diff is read before it is kept.

Proof on a release build, with `.plans/yunta-tui/proof/log-follow/repro.sh`, whose base result is kept beside it, and with the real run of assumption 6.

## Boundaries

**Always**
- Work in this worktree, and stage paths by name.
- Reproduce before changing, and measure after.
- Keep one region and one closing block, whoever feeds them.

**Ask first**
- A new dependency, a change to the log's schema or to what an append writes.
- Any change to attached `yunta run`.
- Spending tokens beyond the real run of assumption 6.

**Never**
- A follower that writes, locks or signals.
- Touching `~/.yunta` or the main checkout's uncommitted files.
- Running `cargo xtask smells` without `--check`.

## Success Criteria

1. `Storage::events_after(run, seq)` returns exactly the events `events_for_run` returns past `seq`, in order, with the same handling of an unknown kind and a corrupt row, on the sync and the async store.
2. One read returns each run's last seq, for every run with an event, including one written before `run_heads` existed, and no run that was purged.
3. At a terminal, `yunta status <run> --follow` draws the region `yunta run` draws while the run moves, and when it stops leaves the same closing block and exits with the code `yunta run` would give.
4. Off a terminal, it prints the lines `yunta run` prints off one, the first saying why, then the same block and code.
5. On a run that is not moving, it prints what `yunta status` prints and exits with the run's code.
6. Ctrl-C ends following with exit 130 and says the run goes on and how to follow it again; the run goes on to its end.
7. Following writes nothing: with two followers on one run, its log, its lock and its registry are what they are with none.
8. An engine killed with `kill -9` under a follower ends it as a stall within 2 s, exit 1.
9. A run asking at its own terminal is shown asking, and following goes on until it moves again. A promotion is followed onto its successor, and a parent shows its children as `yunta run` does.
10. Every place that offers `yunta status <run>` as what "follows it" offers `yunta status <run> --follow`, by handle for a person and whole in a document.
11. With mock sessions, taking turns: an event reaches the follower within 250 ms of its append, and the median `between_secs` of runs with a follower is no higher than the highest of runs without one.
12. A real fragua run started with `--detach` and followed from a second terminal moves on that terminal node by node, and the follower ends at `ship` with its decision and exit 3. Its `between_secs` stays under 1 s.
13. `docs/compatibility.md` says the exit codes of `status --follow`, and `README.md` and `docs/guide.md` say how to follow a run.
14. Every step of the `ci` job passes.

## Open Questions

None. Whether the head of each run had a reader before `control` was settled in Task 8: the follower of a promoted run reads it.
