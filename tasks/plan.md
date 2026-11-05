# Implementation Plan: `run-guard`

Spec: `SPEC-run-guard.md`, approved on 2026-11-04. Status: **approved on 2026-11-04**. Task list: `tasks/todo.md`.
Base: `main` at `34790ab8`, with no new commits on 2026-11-04.

## Overview

A run claims a lock file before anything drives it, and the type system makes that claim impossible to skip. `yunta resume` takes it before it asks for a checkout, so a refused resume touches nothing. A holder that is gone is replaced, which needs `process_start` to work on macOS. The CLI and MCP turn a refused claim into one sentence that says who drives the run. No event, no dependency and no wait on an uncontested run is added.

## Architecture Decisions

**1. The claim is one lock file per run, at `<run_dir>/scratch/engine.lock`, over the protocol in `crates/engine/src/lock.rs`.**
- That protocol already gives an atomic `create_new`, an owner with a pid and a process start, and a dead owner replaced with no race.
- Per run and not per checkout, because a run can change checkouts.
- Its path goes in `run_dir.rs`, the owner of the run directory's layout.
- Nothing clears it by accident: `ProcessRegistry::clear` deletes only credential files under `scratch/`, and nothing else lists that directory.

**2. A new engine module, `run_hold`, owns the claim.**
- `RunHold` is the proof. Its fields are private and only `hold_run` makes one.
- It is released on `Drop`. A process killed before that leaves the file, and the next holder replaces it.
- `hold_run` always answers `keep` with `false`, because `resume` already answers for the process groups a dead engine left running.
- `hold_run` creates `scratch/` when it is missing, because a resume on such a run directory works at the base commit.

**3. `RunEnv` borrows the hold, in place of its `run_dir` field.**
- A run's directory comes from its claim, so a claim on another directory cannot be passed.
- A borrow adds no field to `RunCtx`, and `invocation.rs` does not have to keep it alive.
- Rejected: `execute_run(env, hold)` as a separate argument. The child path goes through `execute_run_at_depth` with its own `RunEnv`, so a child would stay outside the guarantee.
- Rejected: an owned field. It would have to live in `RunCtx` to last the whole run.

**4. Who claims, and when.**

| Who | Where | When | If contended |
|---|---|---|---|
| `yunta resume` | `parked` in `commands/resume.rs` | After the wait for a limit's reset, before `still_or_again` | From the log, see decision 6 |
| `yunta run`, attached | `commands/run/attached.rs` | After the run is created, before `drive` | Refuse |
| A workflow's child | `run_member` in `run/workflow_exec/child.rs` | Each time its parent drives it | Refuse |
| A promotion's successor | `run_successor` in `commands/promote.rs` | Before it runs | Refuse |
| `yunta test` | `commands/test/case.rs` | Before each `execute` | Refuse |
| The test harness | `wake_through` in `testkit/src/bench/driving.rs` | Before each wake | Refuse |

`yunta run --detach` and `resolve-gate` claim nothing themselves: the process they start is a `yunta resume`.

**5. The claim ends when `execute` returns, owned by `Executing`.**
- That is before `finish`, so before promotion, `released` and `resumes_after_reset`.
- It has to be: `resumes_after_reset` starts a `yunta resume` on the same run, which would otherwise find its own parent holding it.
- Rejected: holding it through `settle`. See the window in Risks.

**6. How to contend comes from the log.**
- When the last event is `run_paused`, the next engine waits up to 5 s, looking every 15 ms, the same poll as `MUTATION_LOCK_POLL`. The engine that wrote the pause is on its way out.
- In any other case it refuses at once.

**7. A refusal that must come before a wait reads the lock without taking it.**
- `lock::refusal_now` answers what a `Refuse` would answer, and takes nothing.
- `resume` asks before it waits for a reset, and `resume_run` over MCP asks before it starts a process.
- A live holder on a parked run passes, because it is leaving. What cannot be checked refuses right there, because waiting does not clear it.

**8. Liveness is fixed at its source.**
- `process_start` on macOS reads `pbi_start_tvsec` and `pbi_start_tvusec` through `libproc`, already a dependency of `yunta-core` there.
- `holder_state` compares the start also when the pid answers `Unknown`. That settles the case on Linux, where `/proc` answers about any process.
- Rejected: `sysctl`, which needs `unsafe`, forbidden by the workspace. A `ps` subprocess, which would spawn a process inside a lock check. A new dependency, which the spec excludes.
- This also improves `cancel`, `close`, `gc` and `status`, which ask the same question.

**9. One mapping turns a refused claim into words.**
- `CliError::refused_hold` is the only place that reads a `LockError` for a person. Three new variants carry the cases of the spec's table.
- `said_to` names the run by handle or whole id, as the other refusals do, and `advice` offers the two commands.
- A claim refused below the CLI (a child, a successor) goes through the existing `From<LockError> for RunError`, whose sentence names the holder's pid.

**10. Tests go where they cost least.**
- The CLI tests are a new module of the `run_flow` binary, so they add no test binary to link.
- The engine tests are one new binary, `run_guard`, which the rule in `.agents/architecture.md` cites.

## Dependency Graph

```
process_start on macOS
  └─ holder_state with Unknown
       └─ refusal_now
            └─ run_hold: RunHold, hold_run, hold_refused, contention_for
                 └─ resume and run claim                  <- the bug is fixed here
                      └─ child and successor claim
                           └─ RunEnv demands the claim    <- the compiler holds it
                                └─ what a person reads
                                     ├─ refusing before the reset wait
                                     └─ what an agent reads
                                          └─ documentation
```

The reproduction comes before all of it, and the same proof, CI and the commit after it.

## Phases

`tasks/todo.md` breaks these down, with what closes each.

**Phase 0: reproduce and measure.** It repeats what ran ahead of its approval. The double engine with both isolation modes and on a child; a fragua run; the suite on the base. Two additions: `ship` is answered with `abort` through `yunta resolve-gate`, which starts a detached resume with nothing published, and the time from `run_paused` to the engine's exit is measured, which tests the 5 s of decision 6 before anything is built on it.

**Phase 1: tell whether a holder is alive.** `process_start`, `holder_state` and `refusal_now`. Nothing uses them for a run yet.

**Phase 2: one run, one engine.** The risky part comes first in this phase: `resume` and `run` claim the run, which fixes the bug and tests the spec's assumption that no flow needs two engines. Then children and successors. Last, `RunEnv` demands the claim, which is mechanical and changes no behavior.

**Phase 3: what whoever uses it sees.** The sentences for a person, refusing before the reset wait, then `resume_run` for an agent.

**Phase 4: documentation.** For whoever uses Yunta (`docs/troubleshooting.md`, `README.md`) and for whoever works on it (`.agents/`, the `//!` of what changed).

**Phase 5: proof and close.** The same measurements as Phase 0 on the new binary, every step of the `ci` job, the user's review of the whole diff, and one commit.

Every phase follows `incremental-implementation` and `test-driven-development`: one task at a time, in this session, with each test seen failing for the cause it states before the code that makes it pass.

## Verification Checkpoints

| After | What must hold | Who |
|---|---|---|
| Phase 0 | The bug reproduces. If it does not, the spec's diagnosis is wrong and the work stops | The user sees the result |
| Phase 1 | `stalled_runs`, `close_cmd` and the `worktree` tests stay green on macOS, since they read what changed | The user sees the result |
| Phase 2 | The whole suite, compared with the base's: 262 test binaries green with none aborted. A legitimate flow that needs two engines stops the work; the test is not changed | The user sees the result |
| Phase 3 | `cargo xtask smells --check`, and the refusal read for real, at a terminal and with `--json` | The user sees how it reads |
| Phase 5 | Every step of the `ci` job, and the user's approval of the diff before the commit | The user approves |

## Parallelization

None. Each step compiles on the one before it, they share files (`lock.rs`, `resume.rs`, `child.rs`), and the work is small enough for one session. Documentation could go in parallel, but it describes behavior the earlier phases settle.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| A legitimate flow needs two engines on one run | High | The Phase 2 checkpoint shows it, and the work stops there |
| The 5 s is too short: the resume `resolve-gate` starts is refused, its error reaches only `detached.log`, and the run stays parked with no word | High if it happens | Measured before building (Phase 0) and after (Phase 5). If the margin is thin, the constant goes up |
| macOS starts giving real process starts, and something passed only because it gave `None` | Medium | Linux already gives real starts and its CI is green. Phase 1 checkpoint, locally on macOS |
| A test drives an engine in process and leaves the lock taken | Medium | No test uses `abort` or `mem::forget` on a run (searched). The Phase 2 checkpoint |
| Between the claim ending and the process exiting, a new resume can start while the old process runs `released` | Low | The same window exists at the base commit. Holding the claim longer would break decision 5 |
| A holder that exited and was not collected reads as alive, and blocks until its parent collects it | Low | It takes a crash and a parent that does not collect. `spawn_detached_resume` collects its children |
| An unreadable lock blocks a run until a person acts | Low | A record is whole before its name exists, so only corruption gets there. The refusal names the file |
| An engine from an older binary is still running after an upgrade, and takes no lock | Low | Only during that upgrade. Accepted |
| A parent fails because someone resumed its child by id first | Low | Correct: an engine already drives that child. The sentence names its pid |

## Cost

Two real fragua runs with a small request, one in Phase 0 and one in Phase 5, as the repository asks of any change on the run path. They run in a small project of their own, because what is compared is the engine's own time and a heavy suite adds only noise and cost. Its forge names a repository that does not exist, so nothing can be published. The previous one took 2m40s of work and 2.69M tokens, 1.97M of them cached.

## Open Questions

None.
