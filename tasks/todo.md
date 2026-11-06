# Tasks: `session-ends`

Spec: `SPEC-session-ends.md`. Plan: `tasks/plan.md`. Status: **approved on 2026-11-06, in implementation**.

**How these run:** one at a time, with `incremental-implementation` and `test-driven-development`. Each task names the test that fails first and why. That test is seen failing for that cause before the code that makes it pass. A test that may pass at once says so, and says what it guards. Every task leaves the tree compiling and its tests green. A test that starts a process kills it at its end, whether it passed or not, and waits on the process table, never for a fixed time. `cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`. Before a file is edited, `cargo xtask docs --about <files>` runs, and what it prints is kept true. No commit until the user approves at Checkpoint F.

This list splits four of the plan's tasks, because each touched more than five files. The retry notice is Tasks 3 and 4. Moving the port's types out is Task 6. The session's mark is Tasks 8 and 9. The plan's Task 7 is Task 10 here.

## Phase 1: the real CLIs, before any code

### Task 1: the real CLIs at the base

**Description:** On the base's release build (`.plans/yunta-tui/proof/session-ends/yunta-base`), a one-node run on each real CLI asks the agent to start `sleep <n>` and end its turn at once: Claude Code on `claude-haiku-4-5`, Codex on `gpt-6-luna`. Each CLI is asked twice:
- once through its shell tool's own background mode: Claude's `run_in_background`, and a Codex command its tool returns from before it ends;
- once as `sleep <n> &` from the shell.

`yunta` runs with a probe variable in its environment, which the CLI inherits. While the session runs, a script reads the command's process group, its parent and whether its environment holds the probe. After the session ends, it reads whether the command still runs. The script kills every `sleep` it started.

**Fails first:** nothing; no code changes. What it guards is assumption 1 of the spec.

**Acceptance criteria:**
- [x] For each CLI and each way, the record says the command's group against the CLI's, its parent, whether the probe reached it, and whether it outlived the session. The probe was read again with a `python3` command (`real_env.py`), because macOS hides the environment of `/bin/zsh` and `/bin/sleep`.
- [x] Nothing the script started runs after it, and the tokens each run spent are recorded.

**Verification:**
- [x] Manual check: the script and its result (`real-before.txt`) in `.plans/yunta-tui/proof/session-ends/`, outside git.

**Dependencies:** None

**Files likely touched:** none in git.

**Estimated scope:** S

### Checkpoint A: after Task 1
- [x] The user sees the results. If a CLI does not pass its environment to its commands, or does not start them in groups of their own, the spec and the plan are updated and the user is told before Task 2. (Both do. macOS hiding its platform binaries' environment was found here; the user chose to keep the sweep, and the spec, plan and list say so.)

## Phase 2: the Codex retry

### Task 2: Codex, an `error` line is no event

**Description:** `codex/parse.rs` maps a top-level `error` line to no event, and its `//!` says why. `turn.failed` keeps mapping to `Failed` with its message, read by `failure_cause` as today. A whole run through the Codex stub, whose stream prints a retry notice and goes on, finishes.

**Fails first:** `an_error_line_is_no_event`, in the parser's own tests, because the parser maps the line to `Failed`. Then `a_codex_session_that_retries_finishes`, in a new CLI target `session_ends`, because the run ends at `needs you` with exit 3.

**Acceptance criteria:**
- [x] An `error` line, whatever its message, yields no event.
- [x] A refused model (`error` then `turn.failed` carrying `model_not_found`, as 0.157.0 prints it), a reached limit and a lost service each fail with the cause and message they have at the base.
- [x] Through the Codex stub, a stream of `thread.started`, a retry notice, an agent message and `turn.completed` finishes the run with exit 0.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-adapters --lib codex`, `cargo test -p yunta --test session_ends`

**Dependencies:** Checkpoint A

**Files likely touched:**
- `crates/adapters/src/codex/parse.rs`
- `crates/cli/tests/session_ends.rs` (new)

**Estimated scope:** S

### Task 3: the mock can print a retry notice

**Description:** `MockStep` gains `retry_notice`, the line a CLI prints when it retries a request it goes on with. `claude_code/wire.rs` renders it as Claude Code's `{"type":"system","subtype":"api_retry",...}` with the fields Claude Code's documentation lists, and `codex/wire.rs` as Codex's `{"type":"error","message":"Reconnecting... 2/5 (...)"}`. The step's `///` says what it stands for.

**Fails first:** `a_mock_session_that_retries_completes_in_either_dialect`, in the mock suite, because the fixture refuses the step.

**Acceptance criteria:**
- [x] A mock fixture with a `retry_notice` step and a `completed` outcome completes in the Claude Code dialect and in the Codex dialect.
- [x] The step takes `after_ms` as the other steps do.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-adapters --test mock`

**Dependencies:** Task 2

**Files likely touched:**
- `crates/adapters/src/mock/fixture.rs`
- `crates/adapters/src/claude_code/wire.rs`, `crates/adapters/src/codex/wire.rs`
- `crates/adapters/tests/mock/retry.rs` (new), `crates/adapters/tests/mock.rs`

**Estimated scope:** M

### Task 4: the contract, a retry notice ends nothing

**Description:** The contract's `Step` gains `RetryNotice`, and its wire writes it as each CLI does. A case runs on every subject: a session that prints a retry notice and then completes ends `Completed`, with its summary.

**Fails first:** `a_retry_notice_ends_nothing`, because the contract has no such step.

**Acceptance criteria:**
- [x] The case passes on `claude_code`, `codex` and `mock`.
- [x] The contract's wire writes the notice in each CLI's own shape.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-adapters --test contract`

**Dependencies:** Task 3

**Files likely touched:**
- `crates/adapters/tests/contract/subject.rs`, `crates/adapters/tests/contract/wire.rs`
- `crates/adapters/tests/contract/main.rs`, `crates/adapters/tests/contract/retry.rs` (new)

**Estimated scope:** S

### Checkpoint B: after Tasks 2-4
- [x] The adapter suite and `session_ends` pass through the wrapper. `cargo xtask smells --check` passes.

## Phase 3: the walk, the mark and the sweep

### Task 5: the walk

**Description:** A new module `core/process/tree.rs`:
- `groups_under(group)` starts from every member of `group`, follows children by parent pid to the bottom, and answers every group it meets other than `group`. It reads children with `libproc`'s `ByParentProcess` filter on macOS and `procfs` on Linux.
- It never answers the caller's own group, nor a group id of 1 or less.
- `close_groups_under(group)` closes each one with `force_kill_group`, and its error names every group that would not close.

**Fails first:** `the_walk_finds_a_grandchild_in_a_group_of_its_own`, in a new core target `process_tree`, because the module does not exist.

**Acceptance criteria:**
- [x] A grandchild in a group of its own under a leader is found, and is found too when the leader exited and another member remains.
- [x] A leader with no children answers nothing.
- [x] A child that joined the caller's group is never answered.
- [x] After `close_groups_under`, nothing found runs, and the leader's own group is untouched.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-core --test process_tree`, on macOS here and on Linux in CI
- [x] `cargo xtask smells --check` passes.

**Dependencies:** Checkpoint B

**Files likely touched:**
- `crates/core/src/process/tree.rs` (new), `crates/core/src/process/mod.rs`
- `crates/core/tests/process_tree.rs` (new)

**Estimated scope:** S

### Task 6: the port's environment types move out of `session.rs`

**Description:** `port/session.rs` is at 497 lines. `SessionEnv` and `RunToolsEndpoint` move, with their docs and impls, to a sibling module of `port`, re-exported where they are read today. Nothing else changes.

**Fails first:** nothing; a move. It may pass at once, and what guards it is every suite that names either type.

**Acceptance criteria:**
- [x] Every path that names either type compiles unchanged.
- [x] `port/session.rs` has room for the field Task 8 adds, and the `//!` of `port/mod.rs` names the new module.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-core`, `cargo build --workspace --all-targets`

**Dependencies:** Task 5

**Files likely touched:**
- `crates/core/src/port/session.rs`, `crates/core/src/port/mod.rs`
- `crates/core/src/port/<the new module>.rs` (new)

**Estimated scope:** S

### Task 7: the mark and the sweep

**Description:** A new module `core/process/mark.rs`:
- `SessionMark` is `<run id>.<uuid v4>`, carried in `YUNTA_SESSION_MARK`, minted by `SessionMark::new(&RunId)`. `SessionMark::of_run(&RunId)` matches every mark of that run.
- `marked(…)` lists this user's processes whose environment holds a mark, each with the start time `process_start` reads. On Linux it reads `/proc/<pid>/environ` through `procfs`; on macOS it reads through `sysinfo` 0.40.1, added under `[target.'cfg(target_os = "macos")'.dependencies]` with `default-features = false` and `features = ["system"]`.
- `close_marked(…)` signals a pid only while its start time is the one it was found with. It repeats until a pass finds nothing, within a bound whose `///` says why, and never signals the caller or a pid of 1 or less. Its error names what would not die.

**Fails first:** `the_sweep_finds_a_marked_process_whose_parent_left`, in a new core target `process_mark`, because the module does not exist.

**Acceptance criteria:**
- [x] A `python3` carrying the mark whose parent left is found and closed. One carrying another session's mark, or none, is left running.
- [x] A process whose environment the host hides, such as `/bin/sleep` on macOS, reads as unmarked and the sweep goes on with the rest.
- [x] `of_run` finds the marks of every session of that run and of no other run.
- [x] A pid whose start time changed since it was found is not signalled, and the caller never is: both are held by a unit test on the choice of what to signal.
- [x] `cargo deny check` passes with `sysinfo` in the tree. The lock adds `sysinfo`, `objc2-core-foundation` and `objc2-io-kit`, and also `sysinfo`'s Windows-only `ntapi` and `winapi` crates, which the lock lists for every platform and macOS and Linux never build (corrected while implementing: the tree read before planning was macOS's alone).

**Verification:**
- [x] Tests pass: `cargo test -p yunta-core --test process_mark`, `cargo test -p yunta-core --lib process::mark`
- [x] `cargo deny check`

**Dependencies:** Task 6

**Files likely touched:**
- `crates/core/src/process/mark.rs` (new), `crates/core/src/process/mark/tests.rs` (new), `crates/core/src/process/mod.rs`
- `Cargo.toml`, `crates/core/Cargo.toml`, `Cargo.lock`
- `crates/core/tests/process_mark.rs` (new)

**Estimated scope:** M

### Task 8: every session's request carries its mark

**Description:** `SessionRequest` gains `mark: SessionMark`. It is minted in `run/session_plan.rs`, the one place a request is composed, from the run's id. The test builder in `testkit-core` and the adapter suite's own requests mint one too.

**Fails first:** `each_session_carries_a_mark_of_its_run_and_its_own`, in the engine's session tests, because the request has no mark.

**Acceptance criteria:**
- [x] Two sessions of one run get marks that differ and that `of_run` of that run matches.
- [x] A session of another run gets a mark `of_run` of the first does not match.
- [x] `session_request_literal_outside_plan` does not rise.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-engine --test run_sessions`, `cargo test -p yunta-adapters`
- [x] `cargo xtask smells --check` passes.

**Dependencies:** Task 7

**Files likely touched:**
- `crates/core/src/port/session.rs`, `crates/engine/src/run/session_plan.rs`
- `crates/testkit-core/src/adapter.rs`
- `crates/adapters/tests/mock.rs`, `crates/adapters/tests/mock/rail.rs`

**Estimated scope:** M

### Task 9: a session's CLI starts with its mark

**Description:** `Launch` takes the request's mark, and the launch puts it in the CLI's environment itself, so no adapter can leave it out. `codex` also passes `-c shell_environment_policy.set.YUNTA_SESSION_MARK=<value>` through `ConfigOverride`. The mock runs no process and ignores the mark.

**Fails first:** `the_clis_environment_carries_the_sessions_mark`, a new process case of the contract, because no CLI gets the mark.

**Acceptance criteria:**
- [x] Through each stub, the CLI's environment holds `YUNTA_SESSION_MARK` with the request's mark.
- [x] Codex's arguments carry the `shell_environment_policy.set` override with the same value, quoted by `ConfigOverride`.
- [x] The launch comment that says a group signal reaches the CLI's tool subprocesses says what is true.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-adapters --test contract`, `cargo test -p yunta-adapters --lib codex`

**Dependencies:** Task 8

**Files likely touched:**
- `crates/core/src/process/subprocess.rs`, `crates/core/src/process/subprocess/launch.rs`
- `crates/adapters/src/claude_code/mod.rs`, `crates/adapters/src/codex/mod.rs`
- `crates/adapters/tests/contract/process.rs`, `crates/adapters/tests/contract/main.rs`

**Estimated scope:** M

### Checkpoint C: after Tasks 5-9
- [x] The core and adapter suites pass through the wrapper. `cargo deny check` and `cargo xtask smells --check` pass.

## Phase 4: every end

### Task 10: every end of a `SubprocessSession`

**Description:** `kill` and `exit` each do three things, in order:
1. `close_groups_under` the CLI's group;
2. close the CLI's own group, as today;
3. `close_marked` the session's mark.

Their error names every group and pid that would not close. The drop does one pass of each with SIGKILL and no wait. The two stubs gain `<NAME>_STUB_GROUP_CHILD`, which starts `sleep` in a group of its own under the stub through `perl -e 'setpgrp(0, 0); exec …'`, and `<NAME>_STUB_ORPHAN_CHILD`, which starts `python3` sleeping in a group of its own from a shell that exits at once, because macOS lets another process read a `python3`'s environment and hides `/bin/sleep`'s. Each writes its child's pid to the file the knob names. The port's kill contract says what is true.

**Fails first:** `a_command_in_a_group_of_its_own_is_gone_after_kill`, a new process case of the contract, because the command still runs.

**Acceptance criteria:**
- [x] Through each stub, a command in a group of its own, still hanging from the CLI, is gone after `kill`. (After `exit` the CLI has left, so such a command is reached by its mark alone, which `/bin/sleep` on macOS does not show; the orphan case below holds `exit`. Corrected while implementing.)
- [x] Through each stub, a command whose parent left is gone after `kill` and after `exit`.
- [x] A dropped session leaves neither behind, in the core suite.
- [x] The existing process cases still pass.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-adapters --test contract`, `cargo test -p yunta-core --test process_kill`

**Dependencies:** Checkpoint C

**Files likely touched:**
- `crates/core/src/process/subprocess.rs`, `crates/core/src/port/session.rs`
- `crates/testkit-core/stubs/claude_code_stub.sh`, `crates/testkit-core/stubs/codex_stub.sh`, `crates/testkit-core/src/stubs.rs`
- `crates/adapters/tests/contract/process.rs`, `crates/adapters/tests/contract/main.rs`

**Estimated scope:** M

### Task 11: the engine ends a finished session through `kill`

**Description:** `read_and_release` calls `session.kill()` once it has read a terminal outcome, before `release`. There is no interrupt and no grace, because the CLI already said everything. A cut-short session keeps `end_running`. A `kill` that fails becomes `DispatchError::NotStopped`. The registration keeps the CLI's group, and adds each group the error names.

**Fails first:**
- `a_finished_session_whose_kill_fails_ends_its_node_with_session_not_stopped`, in the engine's session tests, with a mock session whose kill is refused and whose outcome is `completed`, because the engine never kills a finished session today.
- `a_kept_registration_keeps_the_groups_a_refused_kill_named`, in the registration's unit tests, because a registration keeps only the CLI's own group today.
- `no_command_outlives_a_session_that_ends_by_itself`, in `session_ends`, through each stub, holds criterion 3 but is not red here. Measured: once the stub waits for its commands to be in their own groups before it prints, Task 10's drop pass closes both commands even without this task's kill. What the kill adds is a checked end: a refusal reported as `session_not_stopped`, with its groups kept for `yunta cancel` and `yunta close`.

**Acceptance criteria:**
- [x] A finished session whose kill is refused ends its node with `session_not_stopped`, keeping the tokens it spent.
- [x] Each group a refused kill names stays in the run's registry beside the CLI's, held by a unit test on the registration.
- [x] Through each stub, a session that ends by itself leaves no command, whether its command hung from it or was left by its parent.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-engine --test run_sessions`, `cargo test -p yunta --test session_ends`

**Dependencies:** Task 10

**Files likely touched:**
- `crates/engine/src/task_cycle/session.rs`
- `crates/engine/src/process_registry/write.rs`
- `crates/engine/tests/run_sessions/frozen.rs`, `crates/cli/tests/session_ends.rs`
- Found while implementing: `crates/engine/src/task_cycle/{dispatch,open}.rs` and `crates/engine/src/run/prompt_exec.rs` (the kill and `NotStopped.groups`). Also `crates/adapters/src/mock/{session.rs,fixture/outcome.rs}` with `crates/adapters/tests/mock/continuity.rs`: the mock refuses `kill` only while its script plays, so a finished session's refused end needs `kill: leave_behind`. Also both stubs: each now waits until its commands are in their own groups before it prints. And `crates/testkit-core/src/adapter.rs` with `adapter/tests.rs`: a pid never written to the fifo hung the test, because a runtime waits for its blocking tasks.

**Estimated scope:** M

### Task 12: `yunta cancel` and `yunta close` after the engine

**Description:** On the escalation path, the crash path and the stopped path, `cancel.rs` first walks and closes the groups under each registered group. Then it signals the registered groups as today, and sweeps `SessionMark::of_run(run_id)` last. `close` reaches the crash path through `settle_crash`. `may_signal`'s refusal stays.

**Fails first:** `cancel_after_its_engine_died_leaves_no_command`, in `session_ends`, through each stub, because only the registered groups are signalled.

**Found while implementing:** `cancel_with_its_engine_alive_leaves_no_command` failed too, for a cause outside `cancel.rs`. The live engine ends its session with `end_running`, an interrupt first, and the stub leaves at the SIGINT. Its `sleep` then hangs from pid 1, out of the walk's reach, and macOS hides it from the sweep. The new contract case `a_command_in_a_group_of_its_own_is_gone_after_interrupt_and_kill` failed on both adapters for that cause. Now `SubprocessSession::interrupt` closes the groups under the CLI first, and the `kill` after it answers for what that walk could not close (plan decision 7). The crash tests also failed on a race at first: an engine killed while the stub still wrote took the stub with it by SIGPIPE. They now wait for `<NAME>_STUB_SAID_FILE`, and were seen red again for the right cause, with the base `cancel.rs`.

**Acceptance criteria:**
- [x] Through each stub, no command is left after `yunta cancel` with the engine alive, after `yunta cancel` once the engine was killed, and after `yunta close` once the engine was killed.
- [x] A run whose registry names a group of 1 or less is still refused, with nothing signalled.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test session_ends`, `cargo test -p yunta --test run_flow`

**Dependencies:** Task 11

**Files likely touched:**
- `crates/cli/src/commands/cancel.rs`
- `crates/cli/tests/session_ends.rs`
- Found while implementing: `crates/core/src/process/subprocess.rs` and `subprocess/launch.rs`, `crates/core/src/port/session.rs` (the interrupt's contract), `crates/adapters/tests/contract/{process,main}.rs`, and both stubs (`<NAME>_STUB_SAID_FILE`).

**Estimated scope:** S

### Checkpoint D: after Tasks 10-12
- [x] `repro.py` on a debug build of the module closes every case it asks about. `cargo xtask smells --check` passes.

## Phase 5: what it says

### Task 13: docs, agent docs, and the `//!` this module makes false

**Description:**
- `docs/adapters.md` says how a session ends now, what the mark is, that a person's `include_only` must name `YUNTA_SESSION_MARK`, and that a Codex retry notice ends nothing.
- `docs/compatibility.md`'s Platforms says what the process layer guarantees and what stays out, macOS's hidden platform binaries among it.
- `.agents/architecture.md` gains the rule "Nothing a session's CLI started outlives its session, linked to it or carrying its mark", tagged with the process tests and the contract.
- `.agents/choices.md` records the absence and its reason: a process that cleared its environment and left the tree, and one the system starts at an agent's request.
- The `//!` of `process/mod.rs`, `subprocess.rs`, `group.rs`, `cancel.rs` and `codex/parse.rs` say what is true.
- Each test that holds a rule other code can break ends with its `Reached by:` line.

**Fails first:** `cargo xtask docs --check`, once the rule cites its tests, until the crate map of `yunta-core` names `tree` and `mark`.

**Found while implementing:** that red did not come. The crate map names `process`, and `process/mod.rs`'s `//!` already named `tree` and `mark` (Task 5). What failed instead was `.agents/architecture.md`'s budget of 6144 bytes, so the rule says the guarantee and its macOS limit, and the order of the closings lives in `subprocess.rs`'s `//!`.

**Acceptance criteria:**
- [x] `cargo xtask docs --check` passes, and `cargo xtask docs --about` on the module's diff prints nothing left false.
- [x] No document, code comment or test cites a document, the spec or the plan.

**Verification:**
- [x] `cargo xtask docs --check`, `cargo xtask smells --check`

**Dependencies:** Tasks 1-12

**Files likely touched:**
- `docs/adapters.md`, `docs/compatibility.md`, `.agents/architecture.md`, `.agents/choices.md`
- the `//!` of the files named above

**Estimated scope:** S

### Checkpoint E: after Task 13
- [x] `cargo test --workspace --locked --no-fail-fast` passes. Its count is explained against 267 binaries and 4,455 tests on macOS, and no binary aborted.

## Phase 6: proof and close

### Task 14: measured on release builds

**Description:**
- On release builds of the base and of the module, runs whose sessions stand on the Claude Code stub take turns, 15 each.
- Their median time between nodes is compared.
- The time one session end spends walking and sweeping is measured on its own, from the module's tracing or from a probe around `kill`.
- Then `repro.py` runs again on the module's build.

**Acceptance criteria:**
- [x] The median time between nodes on the base and on the module is the same within noise, over 15 runs each.
- [x] The time one end spends walking and sweeping is recorded, with its median and its maximum.
- [x] `repro.py` on the module's build shows the retry finishing, the refused model failing as at the base, and no command left in any case but the orphaned `/bin/sleep` on macOS, which the docs name.

**Verification:**
- [x] Results in `.plans/yunta-tui/proof/session-ends/`, outside git, with the scripts that produced them.

**Dependencies:** Checkpoint E

**Files likely touched:** none in git.

**Estimated scope:** S

**Measured on 2026-11-06** (`cost-after.txt`, `end-probe.txt`, `after.txt`):
- Between nodes the two match, 179.7 against 181.1 ms.
- Working time does not: +45 ms over four sessions, about 11 ms for each finished session's end.
- Through the probe, an end with a command hanging from the CLI takes 33.5 ms (45 at most): 13.5 for the walk's close, 12.6 for the CLI's group, 7.0 for the sweep. A close looks again only 10 ms after its signal, and the sweep reads all 574 processes.
- The user chose to remove that cost before the real runs: Tasks 14a and 14b, then this task's three measures again.

**Measured again after Tasks 14a and 14b** (`cost-after.txt`, `cost-base-vs-base.txt`, `end-probe.txt`, `after.txt`; the first measures are kept as `*-first.txt`):

| Measure | Base | Module |
|---|---|---|
| between nodes | 187.1 ms | 185.1 ms |
| working | 465.9 ms | 485.3 ms |
| wall | 781.3 ms | 800.6 ms |

- The base against itself, the same way, differs by 13 ms between nodes, 17 ms working and 23 ms wall, so the module's differences are within that noise.
- Through the probe, an end with a command hanging from the CLI takes 9.7 ms (10.2 at most): 4.2 for the walk's close, 3.8 for the CLI's group, 1.7 for the sweep.
- `repro.py` is unchanged: every case closes but the hidden `sleep`.

### Task 14a: a close looks again soon after its signal

**Description:** `force_kill_group` looks again 1 ms after its first SIGKILL, then every 10 ms, within the same bound of about a second.

**Fails first:** `a_group_that_dies_at_its_signal_closes_within_one_interval`, in `group.rs`'s unit tests, because the close looks again only one full interval after its signal.

**Found while implementing:**
- The timing test failed as stated, at 11.3 ms.
- Beside eight busy loops it then failed 2 times in 20 with one look at 1 ms, and once in 50 with looks at 1, 2, 4 and 8 ms: a saturated machine holds the test's own thread past 10 ms.
- So the schedule became a pure `waits()`, held by `a_kill_looks_again_soon_after_its_signal_then_backs_off`: looks at 1, 2, 4 and 8 ms, then every 10 ms, about a second in all. What it saves an end is measured by the probe when Task 14 runs again.
- The tests moved to `group/tests.rs`, since `group.rs` passed 500 lines.

**Acceptance criteria:**
- [x] A group whose one member dies at its SIGKILL is closed in less than one 10 ms interval.
- [x] The group tests, the core's process suites and the contract pass.

**Verification:**
- [x] `cargo test -p yunta-core`, `cargo test -p yunta-adapters --test contract`

**Dependencies:** Task 14

**Files likely touched:** `crates/core/src/process/group.rs`

**Estimated scope:** XS

### Task 14b: a session's sweep reads only what may carry its mark

**Description:**
- `Marks::Session` carries the CLI's start, which `SubprocessSession` takes before it spawns the CLI.
- A session's sweep finds no process that started more than a minute before that.
- On macOS the processes are listed first without their environment, and only the candidates' environments are read.
- `Marks::Run` has no start and reads every process.

**Fails first:** `a_process_older_than_its_session_is_not_read_for_its_mark`, in `process_mark`, because the sweep reads every process whatever its start.

**Found while implementing:**
- The test failed as stated: an hour-later session's sweep found the older process.
- With the filter on `sysinfo`'s own listing, the probe's sweep went only from 7.0 to 6.9 ms, so that listing was the cost: it reads far more of each process than its start.
- The candidates are now listed through `libproc`'s BSD info, as the walk lists the table, and `sysinfo` reads only their environments. The sweep went to 1.7 ms.

**Acceptance criteria:**
- [x] A marked process that started well before a session's CLI is not found by that session's sweep, and one that started after it is.
- [x] A run's sweep still finds every marked process of the run.
- [x] The core's process suites, the contract and `session_ends` pass.

**Verification:**
- [x] `cargo test -p yunta-core`, `cargo test -p yunta-adapters --test contract`, `cargo test -p yunta --test session_ends`

**Dependencies:** Task 14a

**Files likely touched:** `crates/core/src/process/mark.rs`, `mark/tests.rs`, `subprocess.rs`, `subprocess/launch.rs`, `crates/core/tests/process_mark.rs`

**Estimated scope:** S

### Task 15: the real runs

**Description:** Task 1's script runs again on the module's release build. Then a fragua run is started with `--detach`, with `conform` on Codex `gpt-6-luna` and the rest on Claude Code. It is followed at a terminal up to `ship`, then `abort` and `close`. Nothing is published.

**Acceptance criteria:**
- [x] For each CLI and each way of Task 1, the command is gone once the session ends.
- [x] The fragua run reaches `ship` with no command left after any of its sessions. Its time between nodes, read with `yunta stats`, is in line with the 0.614 s of `run-words`, and its tokens are reported.

**Verification:**
- [x] The runs' ids, stats and results, in `.plans/yunta-tui/proof/session-ends/`.

**Dependencies:** Task 14

**Files likely touched:** none in git.

**Estimated scope:** S

**Measured on 2026-11-06** (`real-after.txt`, `real-env-after.txt`, `real-fragua.txt`, `real_ends.py`):
- Task 1's scripts: at the base, the command outlived its session in 4 of 6 cases (background mode on both CLIs, and the environment probe on both). On the module it outlived none. Claude ran the `cmd &` this time, which it had refused at the base, and that command did not outlive its session either.
- Fragua run `01M6S9F9BGMYZBP6DEDVJ8BCX3`, `conform` on Codex `gpt-6-luna`, reached `ship` with every node finished.
- A sampler read the process table 524 times and kept 74 processes of its sessions: 15 by their mark, 59 under the engine, among them shells, `git`, `npm`, `node` and the MCP servers Claude Code starts. None ran while the run waited at `ship`.
- Between nodes: 0.584 s at `ship`, 0.593 s after `close`, against 0.614 s for `run-words`. Tokens: 3.64M, more than the 2.3M estimated, 2.02M of them in `implement`. `abort` and `close` exited 0.

### Task 16: every step of the `ci` job

**Description:** Every step of the `ci` job in `.github/workflows/ci.yml`, one build at a time, with `CARGO_INCREMENTAL=0`, through `sh .plans/yunta-tui/tools/gate.sh /Users/eulke/development/yunta-tui .plans/yunta-tui/out/gate-session-ends`.

**Acceptance criteria:**
- [x] Every step passes: fmt, clippy, build, schema, smells, docs, deny, doc, test, core-release, workflows, crates. The test step's summary shows no aborted binary.

**Verification:**
- [x] The gate's logs, in `.plans/yunta-tui/out/gate-session-ends/`.

**Dependencies:** Task 15

**Files likely touched:** whatever a failing step points at.

**Estimated scope:** S

### Checkpoint F: after Tasks 14-16
- [x] The user reviews the whole diff, the before and after, and the real runs. No commit until approved.

### Task 17: commit the module

**Description:** One commit for the module on `feature/yunta-tui`, staging paths by name, with a message that cites no document. Then the push, the pull request's list and test plan, and CI on Linux and macOS. Then `cargo clean`.

**Acceptance criteria:**
- [x] The commit holds the module's files and nothing else, including `SPEC-session-ends.md`, `tasks/plan.md` and `tasks/todo.md`.
- [ ] CI on the pull request passes on Linux and macOS, with the new tests seen in the log.

**Verification:**
- [ ] Manual check: `git show --stat HEAD`, `git status` and `gh pr checks 30`.

**Dependencies:** Checkpoint F

**Files likely touched:** none beyond the module's.

**Found while implementing:**
- On `b0cfd1cb`, CI passed on Linux and failed on macOS, in one test: `a_hidden_environment_reads_as_unmarked`. GitHub's macOS runner showed the marked `/bin/sleep`'s environment, which the test expected macOS to hide, as this Mac with System Integrity Protection on does.
- The test now compares the sweep with what `ps -E` shows of that process. `ps -E` reads environments through the host, as the sweep does: here it shows a `python3`'s mark and not a `sleep`'s.
- The docs, the `//!` and the rule say macOS can hide such an environment, not that it always does.

**Estimated scope:** XS

## The spec's success criteria

| Criterion | Tasks |
|---|---|
| 1. A Codex session that retries ends as its later lines say | 2, 4 |
| 2. A refused model and a reached limit fail as at the base | 2 |
| 3. Every adapter's retry notice ends nothing | 3, 4 |
| 4. No command outlives a session, every way it ends | 10, 11, 12, Checkpoint D |
| 5. Nothing signals what is not the session's | 5, 7 |
| 6. The real CLIs, before and after | 1, 15 |
| 7. Measured on release builds, and the real fragua run | 14, 15 |
| 8. The contract and the docs say what is true | 10, 13 |
| 9. Every step of the `ci` job | 16, 17 |

## Not in this list

The other modules of `CAPABILITY-MAP.md` have no spec, plan or tasks yet. Each goes through the same flow when this one closes.
