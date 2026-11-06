# Implementation Plan: `session-ends`

Spec: `SPEC-session-ends.md`, approved on 2026-11-06. Status: **approved on 2026-11-06**, with the dependency of decision 12. Task list: `tasks/todo.md`, written in the next phase.
Base: `feature/yunta-tui` at `7d2752fe`, on `main` at `34790ab8`, which had not moved when this was written.

## Overview

The Codex parser stops reading a top-level `error` line as the end of a turn, so only `turn.completed` and `turn.failed` end one, and the adapter contract gains a retry notice in every dialect. Each session's CLI is started with a mark in its environment that names its run and is unique to the session. Every end of a session then does three things in `core/process`: it walks the CLI's tree by parent links and closes every group under it, it closes the CLI's own group, and it sweeps the user's processes for the mark and closes what carries it. The engine ends a finished session through that same verified close instead of the unverified signal its drop sends today. `yunta cancel` and `yunta close` do the same for a run whose engine died, from the run's id alone. One dependency is added, on macOS only, to read another process's environment.

## Architecture Decisions

**1. Codex: an `error` line is no event.**
- `codex/parse.rs` maps `ThreadEvent::Error` to no event; `turn.failed` keeps mapping to `Failed` with its message, so `failure_cause` classifies a refused model, a reached limit and a lost service as at the base.
- The `//!` of `parse.rs` says why: the CLI prints the same line for a retry it goes on from and for the error a `turn.failed` then carries.
- A stream that closes with no terminal line stays a session that died (`DispatchOutcome::Crashed`), as the port's contract already says. Nothing reads the last `error` line back.

**2. The retry notice, in the contract and the mock.**
- The contract's `Step` gains `RetryNotice`. Its wire writes it as each CLI does. Claude Code prints `{"type":"system","subtype":"api_retry","attempt":2,"max_retries":10,"retry_delay_ms":500,"error_status":503,"error":"server_error"}`, the fields its documentation lists. Codex prints `{"type":"error","message":"Reconnecting... 2/5 (unexpected status 503 Service Unavailable)"}`.
- The mock's `MockStep` gains the same step, rendered by `claude_code/wire.rs` and `codex/wire.rs` as above, so a mock fixture in the Codex dialect goes through the parser that changes.
- The contract case runs on all three subjects: a session that prints a retry notice and then completes ends `Completed`. It fails at the base on `codex` and on the mock in the Codex dialect.

**3. The mark.**
- `SessionMark` lives in `core/process/mark.rs`: the run's id and a uuid v4 minted when the session is planned. Its value is `<run id>.<uuid>`, carried in `YUNTA_SESSION_MARK`.
- The variable's name holds none of `KEY`, `SECRET` or `TOKEN`. Neither the value nor the name is secret.
- `SessionMark::of_run(&RunId)` is what `yunta cancel` and `yunta close` match: every mark of that run.
- `SessionRequest` gains `mark: SessionMark`, built only in `run/session_plan.rs`, the one place a request is composed. `port/session.rs` is at 497 lines, so `SessionEnv` and `RunToolsEndpoint` move to a sibling module of `port` first, with no other change.
- The subprocess launch puts the mark in the CLI's environment itself, so no adapter can forget it. The mock ignores it, since it runs no process.

**4. Codex keeps the mark in its commands' environment.**
- `codex/config.rs` adds `-c shell_environment_policy.set.YUNTA_SESSION_MARK=<value>` through `ConfigOverride`, which already quotes every value.
- Codex adds `set` after every filter but `include_only`. A person's own policy with an `include_only` that leaves the name out still drops the mark. Then only the walk reaches those commands, and `docs/adapters.md` says to name the variable there.
- This corrects one sentence of the spec, which says a person's own policy cannot drop the mark; the spec is updated with this plan.

**5. The walk, in `core/process/tree.rs`.**
- `groups_under(group: Pid) -> Vec<Pid>` starts from every member of `group`, not only its leader, so a leader that exited still has its members walked.
- It follows children by parent pid to the bottom: `libproc`'s `ByParentProcess` filter on macOS, and `procfs` on Linux, both already dependencies. It collects every group it meets other than `group` itself.
- It never returns the caller's own group (`getpgrp()`), nor a group id of 1 or less.
- `close_groups_under(group)` closes each one with `force_kill_group`, killed until no member can execute, and names every group that would not close.
- It is synchronous on its read, so a drop can use it too. `group.rs` is at 469 lines, so the walk gets its own module.

**6. The sweep, in `core/process/mark.rs`.**
- `marked(mark_or_run) -> Vec<(Pid, SystemTime)>` lists this user's processes whose environment holds the mark, each with the start time `process_start` reads.
- On Linux it reads `/proc/<pid>/environ` through `procfs`. On macOS it reads `KERN_PROCARGS2` through the dependency of decision 12.
- `close_marked(…)` signals a pid with SIGKILL only while `process_start` still answers the start time it was found with, and repeats until a pass finds nothing marked, within a bound. It never signals the caller, nor a pid of 1 or less.
- A process that will not die within the bound is named in the error.
- A session's sweep reads the environment only of the user's processes that started no earlier than a minute before its CLI: `Marks::Session` carries the CLI's start. On macOS the candidates are listed as the walk lists the table, through `libproc`'s BSD info (0.3 ms), and `sysinfo` reads only their environments: a listing through `sysinfo` costs about 6 ms by itself, measured. A run's sweep from `yunta cancel` or `yunta close` has no start and reads every process. Added after Task 14 measured 7 ms a sweep for about 570 processes (the user's choice).
- macOS hides the environment of its own platform binaries (`/bin/sleep`, `sh`, `zsh`, `perl`, `ruby`) from every other process, measured at Checkpoint A. Such a process reads as carrying no mark, never as an error, so the sweep goes on with the rest. On macOS a process whose parent left is found only when it runs another binary, as `node`, `python3` or one built apart does; the docs say so (the user's choice at Checkpoint A, over the walk alone). GitHub's macOS runner shows those environments, so this holds only where macOS hides them, and the test compares the sweep with what `ps` shows (corrected after the module's commit).

**7. Every end of a `SubprocessSession`.**
- `kill` does three things, in this order: the walk and close of the groups under the CLI's group, the close of the CLI's own group as today, and the sweep of the session's mark.
- `exit` does the same three, for a stream that ended with no terminal line. There the CLI has usually exited already, so the walk finds nothing and the sweep is what reaches its commands.
- `interrupt` walks and closes the groups under the CLI's group before it sends SIGINT. The engine interrupts only to begin an end (cancel, timeout, budget), and a CLI that leaves at the SIGINT leaves its commands with no parent link to follow. On macOS, where the sweep cannot see `sleep` or a shell, such a command then outlived `yunta cancel` of a live engine (measured in Task 12). What that walk could not close is kept, and the `kill` that follows answers for it. Corrected while implementing.
- `force_kill_group` looks again 1 ms after its first signal, then every 10 ms as before, within the same bound of about a second. Before, it waited 10 ms after the signal, which every end of a group that dies at once paid. Added after Task 14 measured it (the user's choice).
- The drop does one pass of each synchronously, with SIGKILL and no wait: it is the last resort for a session nobody ended.
- The error of `kill` and `exit` names every group and pid that would not close.
- The port's kill contract says what is true: the whole tree, and what carries the session's mark, and nothing else.

**8. The engine ends a finished session through `kill`.**
- Today a session that reached its terminal line is released and dropped, and its drop sends SIGKILL to the CLI's group, with nothing checking that it closed.
- `read_and_release` now calls `session.kill()` once a terminal outcome is read, before `release`. The CLI said everything it had to say, so no interrupt and no grace come first; a cut-short session keeps `end_running`.
- A `kill` that fails becomes `DispatchError::NotStopped`, as a cut-short session's does.
- The registration keeps the CLI's group, and adds each group the error names, so the checkout stays held while anything of the session may still write it.

**9. `yunta cancel` and `yunta close` after the engine.**
- `kill_groups` (the escalation and the crash paths) and `kill_unstopped_groups` (the stopped path) first walk and close the groups under each registered group. Then they signal the registered groups as today, and sweep `SessionMark::of_run(run_id)` last.
- The registered groups are the run's own, so walking all of them is what "the run's whole tree" means. `may_signal`'s refusal of a group of 1 or less stays.

**10. Stubs and tests.**
- The two stubs gain `<NAME>_STUB_GROUP_CHILD`, which starts `sleep` in a group of its own, and `<NAME>_STUB_ORPHAN_CHILD`, which starts `python3` sleeping in a group of its own from a shell that exits at once. Both write the child's pid to a file the test waits on. The group child runs through `perl -e 'setpgrp(0, 0); exec …'`, and the orphan is `python3`, whose environment macOS lets another process read, so the orphan case holds on both platforms. Both are present on macOS and on the Linux runners.
- The contract's process cases gain:
  - a command in a group of its own is gone after `kill`, after an interrupt the CLI leaves at followed by `kill`, and after the stream ends by itself;
  - an orphaned one is gone after both.
- Corrected while implementing: each stub waits until each command it starts is in its own group and has written its pid before it prints, because a session that ends at its last line otherwise closes the stub's group while the command is still joining one of its own. A `<NAME>_STUB_SAID_FILE` says the stub's lines are all printed, so a test kills the engine only once the stub writes no more. An engine killed mid-write takes the stub with it by SIGPIPE.
- Core tests start real processes and check the process table, and every test kills what it started at its end.

**11. What a person and a program rely on is written down.**
- `docs/adapters.md`: the session's end, the mark, a person's `include_only`, and Codex's retry notices.
- `docs/compatibility.md`: Platforms, what the process layer guarantees and what stays out.
- `.agents/architecture.md` gains the rule "Nothing a session's CLI started outlives its session, linked to it or carrying its mark", held by the process tests and the contract.
- `.agents/choices.md`: the absence, with its reason: a process that cleared its environment and left the tree, and one the system starts at an agent's request.
- The `//!` this module makes false: `process/mod.rs`, `subprocess.rs`, `launch.rs` (its comment says a group signal reaches the CLI's tool subprocesses), `group.rs`, `cancel.rs`, `codex/parse.rs`.

**12. The dependency: `sysinfo`, on macOS only.** For the user to approve with this plan.
- `sysinfo = { version = "0.40.1", default-features = false, features = ["system"] }`, under `[target.'cfg(target_os = "macos")'.dependencies]` of `yunta-core`. Linux reads `/proc` through `procfs`, as the rest of the module does.
- **License:** `sysinfo` is MIT. On macOS it adds `objc2-core-foundation` and `objc2-io-kit`, both `Zlib OR Apache-2.0 OR MIT`. `libc`, `memchr` and `bitflags` are already in the workspace's lock.
- **deny.toml:** checked against the repository's own `deny.toml` in a scratch project: `bans` and `licenses` pass with no change, because MIT is allowed and each new crate offers it.
- **Weight:** a minimal release binary on macOS arm64 grows from 433,776 to 519,744 bytes (86 KB) with one sweep in it.
- **Time:** one sweep that reads the environment of this user's 367 processes took a median of 13.95 ms and at most 72.40 ms over 20 passes.
- **Toolchain:** its `rust-version` is 1.95, under the 1.98 that `rust-toolchain.toml` pins.
- **Rejected alternatives:**
  - writing the `sysctl` call with `unsafe`, which the workspace forbids;
  - parsing `ps eww`, another tool's text, where a value may hold spaces;
  - `libproc`, already a dependency, which reads no environment.

## Dependency Order

```
1 the real CLIs at the base: groups, parents, environment (no code)
                       │
        ┌──────────────┴───────────────────────────┐
        ▼                                          ▼
2 the retry notice in the contract and the mock   4 the walk
3 Codex: an `error` line is no event              5 the mark and the sweep, the dependency
                                                  6 the session carries its mark
                                                   │
                                                   ▼
                                     7 every end of a SubprocessSession
                                                   │
                                     ┌─────────────┴─────────────┐
                                     ▼                           ▼
                        8 the engine ends a finished    9 `yunta cancel` and `yunta close`
                          session through `kill`
                                     └─────────────┬─────────────┘
                                                   ▼
                                     10 docs, agent docs, the `//!`
                                                   │
                                                   ▼
                      11 measurements   12 the real runs   13 the ci job   14 commit
```

Tasks 2 and 3 touch other files than Tasks 4 to 6, so they could run beside them. The list keeps them in order so that one worktree stays green between tasks. Task 6 needs Task 5's `SessionMark`, and Task 7 needs Tasks 4 to 6.

## Phases

### Phase 1: the real CLIs, before any code
- Task 1: with each real CLI on the base's release build, a one-node run asks the agent to start a long command, once through the shell tool's background mode and once as `cmd &`, and to end its turn. While the session runs, a script reads the command's group, its parent and its environment: a probe variable set on `yunta`'s own environment shows whether a CLI passes its environment on. Once the session ends, the script reads whether the command still runs. A few thousand tokens per CLI.

**Checkpoint A:** the results shown to the user. If a CLI does not pass its environment on, or does not start its commands in groups of their own, the spec and this plan are updated and the user is told before any code.

### Phase 2: the Codex retry
- Task 2: the retry notice in the contract's steps and wire, and in the mock's fixture and both wire renderers. The contract case fails on `codex` and on the mock in the Codex dialect.
- Task 3: the Codex parser. The case passes. A refused model and a reached limit keep their cause and message, and a whole run through the Codex stub with a retry line finishes.

**Checkpoint B:** the adapter suite and the whole run, through the wrapper.

### Phase 3: the walk, the mark and the sweep
- Task 4: `tree.rs`, the walk and the close of the groups under a group, with real processes in the core tests.
- Task 5: `mark.rs`, the mark, a process's environment read on each platform, and the sweep. `sysinfo` enters here.
- Task 6: the session carries its mark: `SessionRequest.mark` from the session plan (after moving `SessionEnv` and `RunToolsEndpoint` out of `port/session.rs`), the launch's environment, and Codex's `shell_environment_policy.set`.

**Checkpoint C:** the core and adapter suites; `cargo deny check` with `sysinfo` in the tree.

### Phase 4: every end
- Task 7: every end of a `SubprocessSession`, the stubs' two knobs, and the contract's process cases.
- Task 8: the engine ends a finished session through `kill`, the registry keeps what would not close, and a whole run through each stub leaves nothing after a session's own end.
- Task 9: `yunta cancel` and `yunta close`: with the engine alive, after its crash, and on a stopped run.

**Checkpoint D:** `repro.py` on a debug build: every case closed. `smells --check`.

### Phase 5: what it says
- Task 10: docs, agent docs, and the `//!` this module makes false.

**Checkpoint E:** the whole workspace suite, its count explained against 267 binaries and 4,455 tests on macOS, no binary aborted.

### Phase 6: proof and close
- Task 11: measured on release builds. Stub runs on the base and on the module take turns, 15 each, and their median time between nodes is compared; the time one session end spends walking and sweeping is measured too. `repro.py` runs again.
- Task 12: the real runs. Task 1's script runs again on the module, and each command is gone once its session ends. Then a fragua run with `--detach` and `conform` on Codex `gpt-6-luna`, followed at a terminal up to `ship`, then `abort` and `close`, compared with 0.614 s.
- Task 13: every step of the `ci` job, `cargo deny` among them.

**Checkpoint F:** the user reviews the whole diff, the before and after, and the real runs. No commit until approved.
- Task 14: the commit, the push, the pull request's list and test plan, CI on Linux and macOS, `cargo clean`.

## Verification

- Each task names the test that fails first and why, and passes with `cargo test` on its targets through the wrapper.
- A test that starts a process waits on the process table, never for a fixed time, and kills what it started at its end, whether it passed or not.
- `cargo xtask smells --check` at each checkpoint. The gate runs before the commit, with `CARGO_INCREMENTAL=0`.
- Results on release builds are kept in `.plans/yunta-tui/proof/session-ends/`, outside git, beside the scripts that produced them and `before.txt`.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| A finished session's end now waits for its group to be seen empty, plus a sweep of about 14 ms | Med | Task 11 measures the median time between nodes against the base, 15 runs each, and one end's own time. If it shows, the plan comes back to the user with the measurement before anything changes on the run path |
| The sweep signals a process that is not the session's | High | Only a process whose environment holds the session's own value is signalled, and only while its start time is the one it was found with. The walk never returns the caller's group. Unit tests hold both with a process that carries another mark and one that carries none |
| A real CLI does not pass its environment to its commands, or clears the mark | High | Task 1 measures it with each real CLI before any code; for Codex the adapter also sets the mark through `shell_environment_policy.set`. A gap goes back to the spec and to the user |
| Killing a finished CLI at once cuts the write of what `resume` reads | Med | It is what the drop does today, a moment after the terminal line: the signal and its moment stay, and only the check that the group closed is new. The existing resume cases and a real run that resumes a session keep holding it |
| `perl` is missing on a runner | Low | It is on macOS and on GitHub's Ubuntu images; a stub that cannot start its child fails the test that asked for it, never silently |
| `sysinfo`'s process read on macOS changes with a new version | Low | The version is pinned by the lock, and the core tests read a real marked process on the macOS runner |

## Parallelization

None in practice: one worktree, and each phase's tests depend on the one before. Tasks 2 and 3 could run beside Tasks 4 to 6, which touch other files.

## Open Questions

None. The user approved the dependency of decision 12 with this plan, and the sentence of the spec that decision 4 corrects is updated.
