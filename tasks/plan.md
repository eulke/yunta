# Implementation Plan: `log-follow`

Spec: `SPEC-log-follow.md`, approved on 2026-11-05. Status: **approved on 2026-11-05**. Task list: `tasks/todo.md`.
Base: `feature/yunta-tui` at `06607c50`, on `main` at `34790ab8`, which had not moved when this was written.

## Overview

The store learns to read a run from a position and to say where every run stands in one read. On top of that, `yunta status <run> --follow` feeds the region `yunta run` draws from the log instead of from the engine, ends where `yunta run` ends, and leaves the same closing block and exit code. Everything that draws already takes events, so the new parts are a second feed, a follower that only reads, and the places that now offer `--follow`. No event kind, no field in the log, no change to what an append writes, and no dependency are added.

## Architecture Decisions

**1. `events_after` shares one row reader with `events_for_run`.**
- One function turns a row into a `StoredEvent`, with the handling `events_for_run` has today: an unknown kind kept verbatim, a corrupt seq, timestamp, node or payload as an error. `events_for_run` reads every row, `events_after` reads `seq > ?`, both ordered by `seq` on the `(run_id, seq)` key.
- Both exist on `Storage` and on `AsyncStorage`, as every read does.
- Why not a window or a limit: a follower always wants everything it has not seen, and a gap it chose to leave would be a reading of a log nobody wrote.

**2. Each run's head is read off the events' key, from one run to the next.**
- A `RunHead { run_id, seq }` per run with an event. One query walks the `(run_id, seq)` key: it seeks the next run id past the last one, then that run's last seq, so it reads two entries of the key a run whatever the run's length, and it is exact for a run an older binary wrote with no `run_heads` row.
- Measured in Task 2 on 1,000 runs of 200 events: 1 to 5 ms. The first reading this plan chose, `run_heads` with a fallback to the events of a run that lacks a row, took 28 to 39 ms, because the fallback scans every event even when no row is missing. The user chose the walk at Checkpoint A, so nothing is cached and nothing depends on `run_heads`. Its first reader is the follower of a promoted run (decision 9), and `control` reads it next.

**3. One region, two feeds.**
- `Surface` takes its seed from its caller, and gains a way to be handed events from the log. A handed event waits for room in the queue rather than being dropped, because the follower has no other copy to recover it from.
- `SurfaceEnv` says where the engine is read from: this process, as `yunta run` draws today, or the run's registry, as `status` reads it. The painter's frame uses it, so a follower's region says when the engine is asking at its terminal or is gone, from the same words `status` uses.
- `yunta run` passes this process and its own observer, so nothing it shows changes.

**4. The follower lives beside `status`, in `commands/status/follow.rs`.**
- It reads the log once, opens the surface with that seed, and then, at the pace of `yunta_engine::pace::Pace`, asks `events_after` its last seq, hands what came to the surface and applies it to its own `RunState`.
- The interval is a numeric const with its reason, chosen from the measurement of Task 11. A run that did not move costs one indexed read that returns nothing. The registry is read once a second, which is what a stall within 2 s needs.
- Each read is its own statement, so no read transaction is held between reads and the engine's log is never kept from its checkpoint.

**5. It ends where `yunta run` ends.**
- A phase other than created or running ends it: a pause, a finish, a failure, a stop, a promotion with no successor to follow, a broken log. Then the surface comes down and the closing block is printed the way `drive.rs` prints it, from the log, and its outcome is the exit code.
- A run asking at its terminal is running, so following goes on. A run whose engine reads dead while its log says it moves is a stall, which ends it with what `status` says of one.
- An engine with no registry yet is not a stall. That is what a run `--detach` has just handed over looks like, and `engine_liveness` already says so.

**6. A run that is not moving is not followed.**
- `--follow` on a run whose phase is neither created nor running prints the page `yunta status` prints and exits with `render::exit` of its standing, the mapping the document's `exit_code` already uses.

**7. Ctrl-C stops the follower and nothing else.**
- The follower watches the invocation's own stop token. When it trips, the surface comes down, a note says that following stopped, that the run goes on and which command follows it again, and the exit is `RunExit::Interrupted`.
- It never signals the engine, which is another process in another session.

**8. A follower only reads.**
- It never calls `hold_run`, never writes the registry, never appends, and opens the store the way `status` does. Two followers on one run leave the log, the lock and the registry as they are with none, and a test holds that.

**9. A promotion is followed onto its successor, and children come from the parent's log.**
- The predecessor's log does not name its successor; the successor's `run_created` names the run it was promoted from. So when the log closes the run as promoted, the follower reads each run's head, opens the runs it had not seen when it began, and goes on with the one promoted from this run, waiting up to a bound for it to appear. Handed that run's log, the painter moves onto it as it does for `yunta run`, and reads the successor's registry beside the predecessor's. A parent's children are drawn from the parent's log, as they are today.

**10. `--follow` on the command line.**
- A flag on `status` that conflicts with `--json`, `--full`, `--dig` and `--node`, so the parser refuses those pairs with exit 2. `yunta status` without it is untouched and still exits 0.

**11. The command that follows a run is spelled once.**
- `advice::follow(run)` writes `yunta status <run> --follow`. The line `run --detach` starts with, the next step of `resolve-gate`'s answer and the advice of the refusal of a moving run use it, by handle for a person and whole for a program, through `advice::named`. `NextStep` does not change: none of the three goes through it.

**12. What a person and a script rely on is written down.**
- `docs/compatibility.md` gains the exit codes of `status --follow`. `README.md`'s rows of `status` and `run`, and `docs/guide.md`, say how to follow a run. `.agents/architecture.md` gains the rule that a follower only reads, held by its test.

## Dependency Order

```
1 events_after ─────────────┐
2 heads (store only)        │
                            ▼
3 --follow, and the page of a run that is not moving
4 a surface fed from the log, the engine from the registry
                            │
                            ▼
5 the follow loop: seed, poll, end, closing block, exit code
                            │
            ┌───────────────┼────────────────┐
            ▼               ▼                ▼
6 Ctrl-C        7 stall, a run asking    8 promotion, children, followers that only read
            └───────────────┼────────────────┘
                            ▼
9 --follow where a run "follows it"      10 docs and agent docs
                            │
                            ▼
11 measurements   12 the real run   13 the ci job   14 commit
```

Task 2 has no reader in the binary and runs beside Tasks 3 to 5. Tasks 6 to 8 touch the follow loop in turn, so they go in that order.

## Phases

### Phase 1: the store
- Task 1: `events_after` on both stores, one row reader.
- Task 2: each run's head, with the fallback for older logs, and its cost measured.

**Checkpoint A:** the store's tests, the fallback's cost, and the decision on caching it, shown to the user.

### Phase 2: following a run that moves
- Task 3: `--follow`, its conflicts, and a run that is not moving.
- Task 4: a surface fed from the log, and the engine read from the registry.
- Task 5: the follow loop, at a terminal and off one.

**Checkpoint B:** a held run followed in a pty from start to closing block; the follower's block and exit code against those of an attached `yunta run` of the same log. Shown to the user.

### Phase 3: the edges
- Task 6: Ctrl-C.
- Task 7: an engine killed under the follower, and a run asking at its terminal.
- Task 8: a promotion, a parent, and two followers that change nothing.

**Checkpoint C:** `smells --check`, and the edges read at a terminal.

### Phase 4: what it says
- Task 9: `--follow` where a run "follows it", and the goldens it changes, read before they are kept.
- Task 10: docs, agent docs and the `//!` this module makes false.

**Checkpoint D:** the whole suite, its count explained against 264 binaries and 4,394 tests on macOS, no binary aborted.

### Phase 5: proof and close
- Task 11: measured on a release build, with mock sessions taking turns: latency to the follower and time added to the run. `repro.sh` again.
- Task 12: the real run: fragua `--detach`, followed in a pty to `ship`, `abort` and `close`.
- Task 13: every step of the `ci` job.

**Checkpoint E:** the user reviews the whole diff and the before and after. No commit until approved.
- Task 14: the commit, push, the pull request's list and test plan, and CI on Linux and macOS.

## Verification

- Each task names the test that fails first and why, and passes with `cargo test` on its targets through the wrapper.
- Goldens move only where a page offers `--follow`, and only in Task 9.
- `cargo xtask smells --check` at each checkpoint; the gate before the commit, with `CARGO_INCREMENTAL=0`.
- Measurements on release builds kept in `.plans/yunta-tui/proof/log-follow/`, outside git.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Reading each run's head scans every event of every run | Med | Measured in Task 2: the reading that did took 28 to 39 ms on 1,000 runs of 200 events, and the walk of the key that replaced it takes 1 to 5 ms |
| A run `--detach` handed over reads as stalled for an instant | High | Only `EngineLiveness::Dead` is a stall; an absent registry is not. A test starts following before the engine writes its registry |
| An engine that dies before it writes its registry leaves a follower waiting | Low | Ctrl-C ends it, and the page says what `status` says. Recorded in `.agents/open.md` if it shows up in a test |
| A dropped event would leave a hole the follower cannot recover | High | A handed event waits for room instead of being dropped; the follower is the only source, and its reads have no gaps by the log's own transaction |
| A follower's reads slow the engine | High | Each read is one statement with no transaction held; WAL keeps readers off the writer. Task 11 measures runs taking turns with and without a follower |
| The region of a follower and of `yunta run` drift apart | Med | One surface; Checkpoint B compares their closing blocks and exit codes on the same log |
| A file or a function passes the repository's limits, `status/mod.rs` at 361 lines and `painter.rs` at 477 | Med | The loop gets its own file; the painter's change is a field and a branch; smells checked at each checkpoint |
| `prior`, the history the closing block compares with, differs between a follower and `yunta run` | Low | The follower reads it the way `yunta run` does; the comparison test uses a workflow below the history floor and Task 12 reads the real one |
| A test that claims two processes overlap passes when they run in turn | Med | The pty tests wait on what the run writes, and the overlap tests make each side wait on the other |

## Parallelization

One person and one worktree: tasks run in order. Task 2 could run beside Tasks 3 to 5, and Task 10 beside Task 9, but nothing is gained by splitting them.

## Open Questions

None. The interval between reads is decided by the measurement of Task 11, within the bound the spec sets.
