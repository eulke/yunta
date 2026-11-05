# Tasks: `run-guard`

Spec: `SPEC-run-guard.md`. Plan: `tasks/plan.md`. Status: **approved on 2026-11-04, done on 2026-11-05**.

**How these run:** one at a time, with `incremental-implementation` and `test-driven-development`. Each task names the test that fails first and why; it is seen failing for that cause before the code that makes it pass. Every task leaves the tree compiling and its tests green.

**Commands:** `cargo` here always means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui`, which builds with no incremental state and gives any `yunta` a home of its own. Before editing a file, `cargo xtask docs --about <file>`.

```
1 ─ 2 ─ 3 ─ [A] ─ 4 ─ 5 ─ 6 ─ [B] ─ 7 ─ 8 ─ [C] ─ 9 ─ 10 ─ 11 ─ [D] ─ 12 ─ 13 ─ 14 ─ [E] ─ 15 ─ 16 ─ 17 ─ 18 ─ [F] ─ 19
```

## Phase 0: reproduce and measure

### Task 1: Reproduce the double engine

**Description:** Show on a release build of the base commit that a second engine starts on a moving run, with both isolation modes and on a workflow's child. It replaces the earlier evidence, which was gathered ahead of its approval.

**Acceptance criteria:**
- [x] With both isolation modes, the second `yunta resume` does not refuse, `engine.json` changes pid and the node starts twice.
- [x] After `yunta cancel`, the first engine is alive and the log ends with `run_finished` after its `run_paused`.
- [x] A `yunta resume` on a child its parent drives makes the child's node start twice.

**Verification:**
- [x] Build succeeds: `cargo build --release --locked -p yunta`
- [x] Manual check: read `before.txt`, `before-none.txt` and `child-before.txt` in `.plans/yunta-tui/proof/run-guard/`. If the bug does not reproduce, the work stops.

**Dependencies:** None

**Files likely touched:** only `.plans/yunta-tui/proof/run-guard/`, outside git

**Estimated scope:** XS

### Task 2: Measure a real run

**Description:** A real fragua run of the base binary, in a small project of its own, to compare the engine's time afterwards and to test the 5 s the plan assumes before anything is built on it.

**Acceptance criteria:**
- [x] A detached run with `--mode quick` stops at `ship`, publishes nothing, and its `yunta stats` is saved.
- [x] `ship` answered with `abort` through `yunta resolve-gate` starts a detached resume, and what it does is recorded.
- [x] The time from `run_paused` to the engine's exit is measured. If it passes half of the 5 s, I bring the number to the user before going on. Measured at 0.008 s on a small real run with one session and a gate, and 0.018 s on an engine that only applied a decision. The measurer failed on the fragua run itself, so that value comes in Task 17.

**Verification:**
- [x] Manual check: read `stats-before.txt` and `leaving-before.txt` in `.plans/yunta-tui/proof/run-guard/`.

**Dependencies:** Task 1

**Files likely touched:** only `.plans/yunta-tui/proof/run-guard/`, outside git

**Estimated scope:** S

### Task 3: Record the suite on the base

**Description:** The whole suite on the untouched base, so that a later failure can be told apart from one that was already there.

**Acceptance criteria:**
- [x] The log is saved, with how many test binaries passed: 262 binaries, 4,318 tests.
- [x] No binary aborted, and whatever failed is listed by name: nothing failed.

**Verification:**
- [x] Tests pass: `cargo test --workspace --locked --no-fail-fast`, written to `suite-before.log`
- [x] Manual check: search that log for `test result: FAILED` and `process didn't exit successfully`.

**Dependencies:** Task 2, since a build beside a real run would skew its measure

**Files likely touched:** only `.plans/yunta-tui/proof/run-guard/`, outside git

**Estimated scope:** XS

### Checkpoint A: after Tasks 1-3
- [x] The bug reproduces, the 5 s has room, and the base's suite is known.
- [x] The user sees the result.

## Phase 1: tell whether a holder is alive

### Task 4: macOS reports when a process started

**Description:** `process_start` reads the start through `libproc` on macOS, so a reused pid stops reading as the engine that had it. Linux does not change. A process of another user stays `None` on macOS, as measured.

**Fails first:** `a_process_started_now_reports_a_start_between_just_before_and_now`, because on macOS the answer is `None`.

**Acceptance criteria:**
- [x] A process of this user has a real start, on macOS and on Linux.
- [x] A process of another user has none on macOS and has one on Linux. Run on macOS here; the Linux test compiles here and runs in CI.
- [x] A pid nothing runs under still has none.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-core --test process_start`
- [x] Tests pass: `cargo test -p yunta-engine --test worktree`, which reads the same start

**Dependencies:** Task 3

**Files likely touched:**
- `crates/core/src/process/process_start.rs`
- `crates/core/tests/process_start.rs`

**Estimated scope:** S

### Task 5: A pid another user reused reads as gone

**Description:** `holder_state` compares the process start also when the pid answers `Unknown`. Without this, that lock would stay present for good on Linux.

**Fails first:** `a_pid_another_user_reused_after_the_lock_reads_as_gone`, because the answer is `Unknown`.

**Acceptance criteria:**
- [x] `Unknown` with a start after the lock reads as `Dead`.
- [x] `Unknown` with an earlier start, or none, stays `Unknown`.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-engine --lib lock::`

**Dependencies:** Task 4

**Files likely touched:**
- `crates/engine/src/lock.rs`

**Estimated scope:** XS

### Task 6: Read a lock without taking it

**Description:** `lock::refusal_now` answers what a `Refuse` would answer, and touches nothing. A command that must refuse before it waits or starts a process needs it.

**Fails first:** `reading_a_lock_answers_as_taking_it_would_and_takes_nothing` does not compile: `refusal_now` does not exist.

**Acceptance criteria:**
- [x] No file or a dead holder gives `None`; a live or doubtful holder gives `Held`; an unreadable file gives `Unreadable`.
- [x] It neither removes nor writes the file.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-engine --lib lock::`

**Dependencies:** Task 5

**Files likely touched:**
- `crates/engine/src/lock.rs`
- `crates/engine/src/lock/tests.rs` (new: the tests moved here, since the file passed the repository's 500 lines)

**Estimated scope:** XS

### Checkpoint B: after Tasks 4-6
- [x] `cargo test -p yunta --test stalled_runs --test close_cmd` is green, since `cancel`, `close` and `status` read what changed.
- [x] The user sees the result.

## Phase 2: one run, one engine

### Task 7: Claim a run

**Description:** The `run_hold` module: `RunHold`, the proof; `hold_run`, the only way to make one; `hold_refused`, which reads without taking; and `contention_for`, which chooses from the log. Nothing calls it yet, so no behavior changes. `LockOwner`'s record name stops saying "isolation lock", since two locks use it.

**Fails first:** `crates/engine/tests/run_guard.rs` does not compile: `hold_run` does not exist.

**Acceptance criteria:**
- [x] Of many attempts at once, exactly one holds the run.
- [x] A dead holder or a reused pid is replaced; a live, doubtful or unreadable one never is.
- [x] Dropping the hold removes the file, and the run can be held again. A parked run's next engine waits, and an interrupt ends that wait.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-engine --test run_guard`

**Dependencies:** Task 6

**Files likely touched:**
- `crates/engine/src/run_hold.rs` (new)
- `crates/engine/src/run_dir.rs`
- `crates/engine/src/lib.rs`
- `crates/engine/src/lock.rs`
- `crates/engine/tests/run_guard.rs` (new)

**Estimated scope:** M

### Task 8: Driving a run from the CLI claims it

**Description:** The slice that fixes the bug. `yunta resume` claims the run after the wait for a reset and before asking for a checkout; attached `yunta run` claims it once created; `drive` lets go when `execute` returns, before it starts anything. The refusal still reads in the raw words of `WorktreeError::Lock`. This is the first test of the spec's assumption that no flow needs two engines.

**Fails first:** in the new `guarding` module, `a_second_resume_on_a_run_moving_in_a_pooled_checkout_is_refused` fails under its deadline with "the second resume did not refuse". The test never waits on the resume's exit with no limit, so a missing guard fails it instead of hanging it.

**Acceptance criteria:**
- [x] A second resume exits 1 with both isolation modes, `engine.json` keeps its pid, the node starts once, and the log gains no `run_resumed`.
- [x] Of four resumes at once, one drives and three exit 1.
- [x] An engine killed with `kill -9` leaves its lock, the next resume replaces it, and no lock is left at the end.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test run_flow --test mcp_flow --test stalled_runs --test close_cmd`, which also holds `--detach` and answering a gate

**Dependencies:** Task 7

**Files likely touched:**
- `crates/cli/src/commands/drive.rs`
- `crates/cli/src/commands/resume.rs`
- `crates/cli/src/commands/run/attached.rs`
- `crates/cli/tests/run_flow.rs`
- `crates/cli/tests/run_flow/guarding.rs` (new)

**Estimated scope:** M

### Checkpoint C: after Task 8
- [x] No flow that passed on the base needs two engines. If one does, the work stops here and the test is not changed.
- [x] The user sees the result.

### Task 9: A workflow's child claims its own run

**Description:** A parent claims each child it drives, since a person can resume a child by its id.

**Fails first:** `a_second_resume_on_a_child_its_parent_drives_is_refused` fails with "the second resume did not refuse".

**Acceptance criteria:**
- [x] A resume on a child its parent drives exits 1, and the child's node starts once.
- [x] The parent ends well, and the child's lock is not left behind.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test run_flow guarding`
- [x] Tests pass: `cargo test -p yunta-engine --test workflow_compose`

**Dependencies:** Task 8

**Files likely touched:**
- `crates/engine/src/run/workflow_exec/child.rs`
- `crates/cli/tests/run_flow/guarding.rs`

**Estimated scope:** S

### Task 10: `execute` owns the claim

**Description:** `Executing` carries the `RunHold` in place of `run_dir`, and the claim ends when `execute` returns. So a promotion's successor and `yunta test` have to claim their run too, and the explicit release in `drive` goes. No behavior a person sees changes.

**Fails first:** nothing new, since this changes no behavior. The tests of Tasks 8 and 9 hold it, along with the promotion and `yunta test` tests.

**Acceptance criteria:**
- [x] `Executing` has no `run_dir`, and nothing calls `execute` without a claim.
- [x] Promoting to a successor and `yunta test` work as on the base.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test run_flow`
- [x] Tests pass: `cargo test -p yunta --bin yunta promote`
- [x] Tests pass: `cargo test -p yunta-engine --test promotion`

**Dependencies:** Task 9

**Files likely touched:**
- `crates/cli/src/commands/registry.rs`
- `crates/cli/src/commands/registry/tests.rs` (new: the tests moved here in Task 11, since the file passed the repository's 500 lines)
- `crates/cli/src/commands/drive.rs`
- `crates/cli/src/commands/promote.rs`
- `crates/cli/src/commands/test/case.rs`
- `crates/cli/src/commands/doctor/session.rs` (found by the compiler: `yunta doctor --session` also runs a run)
- `crates/cli/src/commands/run/attached.rs` (asks the one `claim` function instead of spelling the call)

**Estimated scope:** L. It went past five files for those two reasons, each one line.

### Task 11: `RunEnv` demands the claim

**Description:** `RunEnv` borrows the hold in place of `run_dir`, so no run can be driven without one, nor under another run's. The test harness claims like anyone else, and the child's claim is passed on. No behavior changes: the compiler is the check.

**Fails first:** nothing new, since this changes no behavior. Every site that builds a `RunEnv` stops compiling until it passes a claim.

**Acceptance criteria:**
- [x] `RunEnv` has no `run_dir`, and cannot be built without a `RunHold`.
- [x] `RunHold`'s fields are private, and only `hold_run` makes one.

**Verification:**
- [x] Build succeeds: `cargo check --workspace --all-targets`
- [x] Tests pass: `cargo test -p yunta-engine --test run_guard --test workflow_compose --test promotion --test run_gates_limits`

**Dependencies:** Task 10

**Files likely touched:**
- `crates/engine/src/run/mod.rs`
- `crates/engine/src/run/invocation.rs`
- `crates/engine/src/run/workflow_exec/child.rs`
- `crates/testkit/src/bench/driving.rs`
- `crates/cli/src/commands/registry.rs`

**Estimated scope:** M

### Checkpoint D: after Tasks 9-11
- [x] `cargo test --workspace --locked --no-fail-fast` gives what `suite-before.log` gave, with no binary aborted.
- [x] The user sees the result.

## Phase 3: what whoever uses it sees

### Task 12: A person reads who drives the run

**Description:** A refused claim becomes the spec's sentences through one mapping, `CliError::refused_hold`, instead of the name of a lock file. The first case offers `yunta status` and `yunta cancel` as the commands that go on from here.

**Fails first:** the sentences added to the `guarding` tests, and `a_moving_run_is_said_with_who_drives_it`, which does not compile.

**Acceptance criteria:**
- [x] `run Q78GRR is already moving: pid 91930 drives it`, with both commands, exits 1.
- [x] Under `--json`, one document on stdout with the whole id in `said` and the commands in `next`.
- [x] A run asking at another terminal uses the sentence that exists, and what cannot be checked names the file.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test run_flow guarding`
- [x] Tests pass: `cargo test -p yunta --bin yunta error::words`

**Dependencies:** Task 11

**Files likely touched:**
- `crates/cli/src/error.rs`
- `crates/cli/src/error/hold.rs` (new: the mapping and its tests, since `error.rs` passed the repository's 500 lines)
- `crates/cli/src/error/words.rs`
- `crates/cli/src/error/words/tests.rs`
- `crates/cli/src/output.rs` (`advice` now takes the reader: a program's document names the run whole, which `commands/advice` asks of every command it spells)
- `crates/cli/src/commands/resume.rs`
- `crates/cli/tests/run_flow/guarding.rs`

**Estimated scope:** L. It went past five files for those two reasons.

### Task 13: What cannot be checked refuses before the reset wait

**Description:** A run parked on a limit can wait for hours, and a lock nobody can check does not clear by waiting. `refuse_if_driven` answers before that wait. A live holder of a parked run passes, because it is leaving.

**Fails first:** a new test writes a `run_paused` on a far `awaiting_reset` into the log with `yunta_storage::Storage`, as `host_suspension_cmd.rs` does, beside an unreadable lock. The resume waits for that date, so the test fails under its deadline.

**Acceptance criteria:**
- [x] That resume exits 1 at once, and names the file.
- [x] A run just parked, with its holder leaving, still waits for it and does not refuse.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test run_flow guarding`

**Dependencies:** Task 12

**Files likely touched:**
- `crates/cli/src/commands/resume.rs`
- `crates/cli/tests/run_flow/guarding.rs`

**Estimated scope:** S

### Task 14: An agent reads the same refusal

**Description:** `resume_run` over MCP refuses before it starts a process, because that process's error would reach only `detached.log`.

**Fails first:** `resume_run_on_a_moving_run_is_refused_and_starts_nothing`, because the tool answers that it resumed the run.

**Acceptance criteria:**
- [x] `resume_run` returns a tool error with the same sentence and the whole id.
- [x] No process starts: the log gains no `run_resumed`, and the node starts once.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test mcp_flow`

**Dependencies:** Task 13

**Files likely touched:**
- `crates/cli/src/commands/mcp/tools.rs`
- `crates/cli/tests/mcp_flow.rs`

**Estimated scope:** S

### Checkpoint E: after Tasks 12-14
- [x] `cargo xtask smells --check` passes.
- [x] Manual check: I read the refusal for real, at a terminal and with `--json`.
- [x] The user sees how it reads.

## Phase 4: documentation

### Task 15: Explain the refusal to whoever uses Yunta

**Description:** What the refusal means and what to do about it, including when a person deletes the file by hand.

**Acceptance criteria:**
- [x] `docs/troubleshooting.md` explains it, and its section on waiting for a limit's reset does not change.
- [x] The row of `yunta resume` in `README.md` says it.

**Verification:**
- [x] Tests pass: `cargo xtask docs --check`

**Dependencies:** Task 14

**Files likely touched:**
- `docs/troubleshooting.md`
- `README.md`

**Estimated scope:** S

### Task 16: Record the rule for whoever works on Yunta

**Description:** The new product rule, the race left open and the check decided against, each in the one home the repository gives it.

**Acceptance criteria:**
- [x] `.agents/architecture.md` holds "A run has one engine", citing its test.
- [x] `.agents/open.md` holds the race of the writes with no engine, and `.agents/choices.md` the absent sequence check.
- [x] The `//!` of `resume.rs` says what is true.

**Verification:**
- [x] Tests pass: `cargo xtask docs --check`
- [x] Manual check: `cargo xtask docs --about` over each file the module touched, keeping true what it prints.

**Dependencies:** Task 15

**Files likely touched:**
- `.agents/architecture.md`
- `.agents/open.md`
- `.agents/choices.md` (the absent sequence check, and the absent flag of the spec's fourth assumption, which is an absence too)
- `crates/cli/src/commands/resume.rs`

**Estimated scope:** M

## Phase 5: proof and close

### Task 17: Repeat the measurements on the new binary

**Description:** Tasks 1 and 2 again on a release build of the finished code, with the same scripts and the same request.

**Acceptance criteria:**
- [x] With both isolation modes and on a child, a second resume refuses, `engine_pid` does not change, and the node starts once. `yunta cancel` leaves the run really stopped.
- [x] `yunta stats` shows no added engine time against `stats-before.txt`, and `resolve-gate` with `abort` still hands the run on.
- [x] The time from `run_paused` to `engine.lock` going away is measured. If it passes half of the 5 s, the constant goes up.

**Verification:**
- [x] Build succeeds: `cargo build --release --locked -p yunta`
- [x] Manual check: compare `after.txt`, `after-none.txt`, `child-after.txt`, `stats-after.txt` and `leaving-after.txt` with their pairs.

**Dependencies:** Task 16

**Files likely touched:** only `.plans/yunta-tui/proof/run-guard/`, unless the constant changes in `crates/engine/src/run_hold.rs`

**Estimated scope:** S

### Task 18: Pass every step of the `ci` job

**Description:** The whole job on the final tree, one build at a time and with `CARGO_INCREMENTAL=0`.

**Acceptance criteria:**
- [x] Every step passes.
- [x] No binary aborted: an exit of 101 with "0 failed" is an abort.
- [x] The smells baseline does not rise.

**Verification:**
- [x] Tests pass: `sh .plans/yunta-tui/tools/gate.sh /Users/eulke/development/yunta-tui <log-dir>`

**Dependencies:** Task 17

**Files likely touched:** none, unless the job asks for a fix

**Estimated scope:** XS

### Checkpoint F: after Tasks 15-18
- [x] All of the spec's success criteria are met on macOS. Linux runs for the first time in the pull request's CI, which is the only trigger the job has.
- [x] The user reviews the whole diff and the before and after of Task 17. **No commit until the user approves.**

### Task 19: Commit the module

**Description:** One commit on `feature/yunta-tui`, with its code, tests, spec, map, plan and task list. No pull request: that comes at the end of the whole initiative.

**Acceptance criteria:**
- [x] `fix(engine): a run has one engine`, with paths staged by name and no document cited in the message.
- [x] `.plans/` stays out of it, and the main checkout is untouched.
- [x] `cargo clean` ran in the worktree.

**Verification:**
- [x] Manual check: `git show --stat HEAD` and `git status`.

**Dependencies:** Checkpoint F

**Files likely touched:** none new

**Estimated scope:** XS

## The spec's success criteria

| Criterion | Tasks |
|---|---|
| 1. A second resume refuses and touches nothing | 8, 12 |
| 2. One holder among several attempts | 7, 8 |
| 3. A dead engine never blocks, with the macOS exception said | 4, 5, 7, 8, 12 |
| 4. A child its parent drives is refused | 9 |
| 5. Driving a run does not compile without the proof | 10, 11 |
| 6. The hand-offs work as on the base | 8, 10, Checkpoint D, 17 |
| 7. `resume_run` refuses and starts nothing | 14 |
| 8. No added engine time on a real run | 2, 17 |
| 9. Every step of the `ci` job | 18 |

## Not in this list

The other modules of `CAPABILITY-MAP.md` have no spec, plan or tasks yet. Each goes through the same flow when this one closes.
