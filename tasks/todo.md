# Tasks: `run-words`

Spec: `SPEC-run-words.md`. Plan: `tasks/plan.md`. Status: **approved on 2026-11-05, in implementation**.

**How these run:** one at a time, with `incremental-implementation` and `test-driven-development`. Each task names the test that fails first and why. That test is seen failing for that cause before the code that makes it pass. A test that may pass at once says so, and says what it guards. Every task leaves the tree compiling and its tests green. `cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`. Before a file is edited, `cargo xtask docs --about <files>` runs, and what it prints is kept true. No commit until the user approves at Checkpoint E.

## Phase 1: the words of a run, in the engine

### Task 1: `Said` and the store

**Description:** The frame and the in-memory store of what a run's sessions said, in a new engine module whose name is settled here. The name must not be confused with `run_tools::words`, which names what the engine says to an agent, and the module's `//!` says so. The store keeps the recent past under one `std::sync::Mutex` and hands each new frame to its readers through a `tokio::sync::broadcast`. A reader that falls behind is told how many frames it missed. A frame is one JSON line, with its version first.

**Fails first:** `a_reader_gets_the_recent_past_then_what_comes_and_nothing_twice`, in the module's own tests, because the module does not exist.

**Acceptance criteria:**
- [x] A reader that subscribes after N frames gets the last of them, at most 64 within 256 KiB and in order, then each new frame exactly once. Two readers get the same frames.
- [x] A text over 16 KiB is cut at a character boundary, so a multi-byte character at the cut stays whole, and its frame says `cut`. A shorter text is left as it is.
- [x] A reader whose queue overflows is told, on its next frame, how many frames it lost. Handing a frame over returns at once with no reader, and with a reader that never reads.
- [x] A frame survives its JSON line both ways. A line with a version the reader does not know reads as skipped, and does not end the reading.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-engine --lib <module>`
- [x] `cargo xtask smells --check` passes. Each numeric const has its `///` reason.

**Dependencies:** None

**Files likely touched:**
- `crates/engine/src/<module>/mod.rs` (new)
- `crates/engine/src/<module>/tests.rs` (new)
- `crates/engine/src/lib.rs`

**Estimated scope:** S

### Task 2: the socket and its reader

**Description:** Where a run's socket lives, binding it, serving it and taking it away, and the reader that turns a socket into frames. The path is `scratch/live.sock` when it fits the platform's limit, and the user's short directory under `/tmp` otherwise. Binding replaces a stale socket and nothing else, and sets mode 0600. Only the engine's own uid is admitted. Each reader gets a task that writes the recent past and then each frame. Dropping the handle cancels every task and removes the file. The uid is read in `yunta-core`. Each `std::fs` call carries its `// blocking:` reason.

**Fails first:** `a_reader_of_the_socket_gets_what_the_store_holds_then_what_comes`, in the module's tests, because nothing binds a socket. The choice of path has its own unit test, `the_scratch_path_is_kept_up_to_the_limit_and_the_short_one_past_it`, which fails because the choice does not exist.

**Acceptance criteria:**
- [x] The path is `scratch/live.sock` at or under the limit, 103 bytes on macOS and 107 on Linux, and `/tmp/yunta-<uid>/<run id>.sock` past it. A short directory that is a link, belongs to another uid, or is open to group or others gives no path.
- [x] Binding over a stale socket replaces it, and binding where a regular file stands refuses and leaves the file alone. The socket's mode is 0600. Dropping the handle removes the file and ends every reader's stream.
- [x] A reader that never reads keeps neither another reader from getting every frame nor a handover from returning at once. A peer whose uid is not the engine's is refused, which the admission function's test holds, since a test cannot be another user.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-engine --lib <module>`, `cargo test -p yunta-core --lib process`
- [x] `cargo xtask smells --check` passes, with `engine_fs_outside_runtime` no higher.

**Dependencies:** Task 1

**Files likely touched:**
- `crates/engine/src/<module>/socket.rs` (new)
- `crates/engine/src/<module>/listen.rs` (new)
- `crates/engine/Cargo.toml` (tokio's `net` feature, named)
- `crates/core/src/process/user.rs` (new), `crates/core/src/process/mod.rs`

**Estimated scope:** M

### Task 3: the tap: a note hands its words over, redacted

**Description:** Where a note's digest is recorded, the note's words go to the run's store and the observer. `SessionRequest` gains the function that builds the session's redactor: the declared secrets bound in its environment, the rail's token and the run tools' token. It goes in a file of its own beside `port/session.rs`, which is at 497 lines. `stream_session` builds the redactor before the request is moved, and `AgentEventCtx` carries it. `note()` records the digest as today, then hands the redacted text and the digest's seq through a new `SessionObserver::said`, whose default ignores it. `RunCtx` stamps the run and the time from the run's clock, hands the frame to its store and then to a new `RunObserver::said`, whose default also ignores it. The testkit's recording observer learns to keep what it is handed.

**Fails first:** `what_a_session_says_reaches_the_observer_redacted_while_the_log_keeps_only_its_digest`, in `crates/engine/tests/run_words.rs`, because no observer is handed any words.

**Acceptance criteria:**
- [x] A mock session says a declared secret. The observer gets one frame per thing said, in order, with `[redacted]` in its place, naming the node, the session, the task and the seq of the note's digest.
- [x] The session's redactor takes out each declared secret its environment binds, the rail's token and the run tools' token. A unit test on a request holding all three holds this, since a scripted session cannot say a token minted for it.
- [x] The run's log holds the same kinds and payloads as the same run with no observer, and its `agent_message` notes carry only a digest.
- [x] A dispatch with no observer, as in the task cycle's own tests, hands nothing over and behaves as today.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-engine --test run_words`, `--test observer`, `--test log_words`; `cargo test -p yunta-core`
- [x] `cargo xtask smells --check` passes; `task_cycle/stream.rs` stays under 500 lines.

**Dependencies:** Task 1

**Files likely touched:**
- `crates/core/src/port/session_secrets.rs` (new), `crates/core/src/port/mod.rs`
- `crates/engine/src/task_cycle/stream.rs`, `crates/engine/src/task_cycle/session.rs`
- `crates/engine/src/observer.rs`, `crates/engine/src/run/ctx.rs`, `crates/engine/src/run/ctx/observers.rs`
- `crates/testkit/src/observer.rs`, `crates/engine/tests/run_words.rs` (new)

**Estimated scope:** M (small edits across these files)

### Task 4: the socket opens with the run, and the registry names it

**Description:** `build_ctx` binds the run's socket right after `open_registry` and keeps the handle in `RunCtx`. `EngineProcessFile` gains `words`, which `ProcessRegistry::clear` empties. `Engine`, the reading taken from the registry, gains `words` while the engine reads alive, and `Engine::this` names none. The testkit gains a reader that waits until the registry names a socket and gathers what it serves. That is the reader every later test uses.

**Fails first:** `a_reader_of_a_runs_socket_gets_what_its_session_says`, in `crates/engine/tests/run_words.rs`, because the registry names no socket.

**Acceptance criteria:**
- [x] While a mock session holds, `engine.json` names the socket, and a reader that connects then gets what was already said followed by what comes. Under a test home on macOS the socket is in the short directory, and on Linux it is in `scratch/`.
- [x] After the run finishes, pauses and is cancelled, the socket file is gone. A registry that stays at a terminal for a kept group names no socket. A resume replaces the socket a killed engine left.
- [x] A registry without the field reads as no socket, and the field is kept through `Persisted` for a reader that does not know it. No file under the home holds what was said, the manifest that freezes a mock fixture aside.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-engine --test run_words`, `--lib process_registry`, `--test cancel`
- [x] `cargo xtask smells --check` passes; `process_registry.rs` stays under 500 lines.

**Dependencies:** Tasks 2 and 3

**Files likely touched:**
- `crates/engine/src/run/invocation.rs`, `crates/engine/src/run/ctx.rs`
- `crates/engine/src/process_registry.rs`, `crates/engine/src/process_registry/write.rs`
- `crates/testkit/src/said.rs` (new), `crates/testkit/src/lib.rs`
- `crates/engine/tests/run_words.rs`

**Estimated scope:** M

### Checkpoint A: after Tasks 1-4
- [x] `cargo test -p yunta-engine` and `cargo xtask smells --check` pass.
- [x] On a release build, mock runs that say a lot take turns with and without three readers on their socket. The time between nodes is the same within noise, and a frame reaches a reader within the plan's bound. Kept in `.plans/yunta-tui/proof/run-words/`.
- [x] The user sees a mock run's words read off its socket, redacted, and the measurement.

## Phase 2: every adapter

### Task 5: the mock can say reasoning

**Description:** The mock's fixture gains a `reasoning` step, which it renders as its dialect writes reasoning: a `thinking` block for `claude-code` and a `reasoning` item for `codex`. The dialect's own parser drops it. `mock/fixture.rs` is at 498 lines, so the outcomes move to a file of their own first, with no change in behavior. The step's `///` says what it stands for.

**Fails first:** `a_mock_sessions_reasoning_never_reaches_its_stream`, in `crates/adapters/tests/mock.rs`, because a fixture with `type: reasoning` is refused as an unknown step.

**Acceptance criteria:**
- [x] Under each dialect, a fixture that says something, reasons, then says something else streams the two things said, in order, and nothing of the reasoning.
- [x] Every existing mock test passes unchanged after the outcomes move.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-adapters --test mock`, `--lib`
- [x] `cargo xtask smells --check` passes.

**Dependencies:** None

**Files likely touched:**
- `crates/adapters/src/mock/fixture.rs`, `crates/adapters/src/mock/fixture/outcome.rs` (new)
- `crates/adapters/src/claude_code/wire.rs`, `crates/adapters/src/codex/wire.rs`
- `crates/adapters/tests/mock.rs`

**Estimated scope:** M

### Task 6: the contract: what a session says arrives whole and in order, and never its reasoning

**Description:** A contract case run on every subject. The contract's `Step` gains reasoning, which the contract's wire writes as each CLI does. The case lives in a file of its own, since `cases.rs` is at 492 lines.

**Fails first:** `what_a_session_says_arrives_whole_and_in_order_and_never_its_reasoning`, which does not compile because `Step::Reasoning` does not exist. Once it does, the case may pass at once on every subject, since both parsers already drop reasoning and Task 5 gave the mock its step. It then guards that every adapter keeps doing so.

**Acceptance criteria:**
- [x] On `claude-code`, `codex` and `mock`, a session says a multi-line text with Markdown, reasons, then says a second text. Its stream carries both texts whole, in that order, and nothing of the reasoning.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-adapters --test contract`
- [x] `cargo xtask smells --check` passes.

**Dependencies:** Task 5

**Files likely touched:**
- `crates/adapters/tests/contract/said.rs` (new), `crates/adapters/tests/contract/main.rs`
- `crates/adapters/tests/contract/subject.rs`, `crates/adapters/tests/contract/wire.rs`

**Estimated scope:** S

### Task 7: every adapter in whole runs

**Description:** `yunta run` with `claude-code` on its stub, `codex` on its stub, and `mock`. Each session says something, including a secret the test declares, and then holds until the test lets it go. The testkit's reader gets what each one said off the run's socket.

**Fails first:** `each_adapter_speaks_through_the_runs_socket`, in `crates/cli/tests/run_words/adapters.rs`. It may pass at once, since Task 4 already serves every adapter's notes. It guards that each real adapter's words reach the socket through a whole run, which the engine's tests reach only with the mock. If it fails, the cause is named before any code changes.

**Acceptance criteria:**
- [x] For each adapter, the reader gets the session's words whole, in order, naming its node and session.
- [x] The declared secret never reaches the reader: `[redacted]` stands in its place.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test run_words`
- [x] `cargo xtask smells --check` passes.

**Dependencies:** Tasks 4 and 6

**Files likely touched:**
- `crates/cli/tests/run_words.rs` (new), `crates/cli/tests/run_words/adapters.rs` (new)

**Estimated scope:** S

### Checkpoint B: after Tasks 5-7
- [x] `cargo test -p yunta-adapters`, `cargo test -p yunta --test run_words` and `cargo xtask smells --check` pass.

## Phase 3: the row

### Task 8: the row, in the view

**Description:** `view::node_rows` takes the last frame said by a node and draws one row after `tasks running:` and before the calls. The label is `says:`, or `T001 says:` for a task's session, in `Tone::Muted`, and the sentence is `surco::prose::first_sentence` of the text, in `Tone::Plain`, cut to fit. The view's tests are at 405 lines, so the row's tests go in a file of their own.

**Fails first:** `a_working_node_shows_the_first_sentence_of_what_its_session_said`, because `node_rows` takes no frame.

**Acceptance criteria:**
- [x] A working node with a frame shows the row in its place, with its task when it has one. A node with no frame shows no row, and every existing row is as it was.
- [x] A text that opens with a heading, spans several lines, or carries an escape or another control character draws one plain sentence, with no control character reaching the terminal.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --bin yunta surface::view`
- [x] `cargo xtask smells --check` passes.

**Dependencies:** Task 1

**Files likely touched:**
- `crates/cli/src/surface/view/mod.rs`, `crates/cli/src/surface/view/said_tests.rs` (new)
- `crates/cli/src/surface/region.rs`

**Estimated scope:** S

### Task 9: the attached region

**Description:** `Feed` implements `RunObserver::said` with `try_send(Beat::Said)`. The painter keeps the last frame per node of the run it draws: it ignores another run's frames, starts empty on a successor, and drops a node's entry once the node stops working. It hands the frames to `Region::show`. `Draw::Lines` never sees them.

**Fails first:** `attached_run_shows_what_its_session_says_under_its_node`, a pty test in `crates/cli/tests/run_words/row.rs`, because the region draws no row.

**Acceptance criteria:**
- [x] At a terminal, `yunta run` with a mock session that says something and holds shows the row under its node. When the session says something else, the row changes to it.
- [x] A rerouted node does not show its last attempt's sentence. A child run's frames do not show in its parent's region.
- [x] The goldens that change are only those of a region whose sessions say something, and their diff is read before it is kept.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test run_words`, `--bin yunta surface::painter`
- [x] `cargo xtask smells --check` passes; `painter.rs` stays under 500 lines.

**Dependencies:** Tasks 4 and 8

**Files likely touched:**
- `crates/cli/src/surface/feed.rs`, `crates/cli/src/surface/painter.rs`, `crates/cli/src/surface/painter/tests.rs`
- `crates/cli/tests/run_words/row.rs` (new)

**Estimated scope:** M

### Task 10: the follower listens

**Description:** The surface's observer, the door the engine of `yunta run` hands a frame through, hands one to the painter here too, so no new door is added (decided while implementing, in place of a `Surface::hear`). The follow loop compares the `words` of each registry reading with the socket it listens to. When they differ, it stops the old reader and starts `listen(path)`, in a file of its own beside `follow.rs`, which is at 391 lines. A failed connection is tried again at the next reading.

**Fails first:** `a_follower_shows_the_row_the_attached_run_shows`, a pty test in `crates/cli/tests/run_words/row.rs`, because the follower never connects.

**Acceptance criteria:**
- [x] `yunta status <run> --follow` at a terminal, started before and after the session speaks, shows the same row the attached run shows for the same session.
- [x] A follower of a run with no socket, such as a registry without the field, follows as today. A follower started on a resumed run listens to the socket its new engine opened, and one started before an engine binds its socket starts listening once the registry names it.
- [x] Every test in `status_follow` passes unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test run_words`, `--test status_follow`
- [x] `cargo xtask smells --check` passes.

**Dependencies:** Task 9

**Files likely touched:**
- `crates/cli/src/surface/mod.rs`
- `crates/cli/src/commands/status/follow.rs`, `crates/cli/src/commands/status/follow/listen.rs` (new)
- `crates/cli/tests/run_words/row.rs`

**Estimated scope:** M

### Task 11: nothing said off a terminal

**Description:** Black-box tests for every output that is not a terminal: a pipe, `TERM=dumb`, the `detached.log` of a `--detach` engine, and `--json` documents.

**Fails first:** `nothing_said_reaches_an_output_that_is_not_a_terminal`, in `crates/cli/tests/run_words/off_terminal.rs`. It may pass at once, since `Draw::Lines` never sees a frame. It guards the spec's rule that words go only to a person at a terminal.

**Acceptance criteria:**
- [x] A run whose session says a known sentence shows that sentence in none of: `yunta run` through a pipe, `TERM=dumb`, `status --follow` through a pipe, `detached.log`, `status --json` and the receipt.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test run_words`

**Dependencies:** Task 10

**Files likely touched:**
- `crates/cli/tests/run_words/off_terminal.rs` (new), `crates/cli/tests/run_words.rs`

**Estimated scope:** S

### Checkpoint C: after Tasks 8-11
- [x] `cargo xtask smells --check` passes.
- [x] The user sees the row at a terminal, attached and followed, with mock sessions.

## Phase 4: what it says

### Task 12: docs, agent docs, and the `//!` this module makes false

**Description:** `docs/guide.md` describes the row and where it never appears. `docs/compatibility.md` describes the socket for a program: `engine.json` names it, a frame is one versioned JSON line, a newer version is skipped, and a frame is never evidence. `README.md` changes only if its rows describe what a run shows. `.agents/architecture.md` gains the rule, cited as `[test engine/run_words]` with its test in that target's own file: what an agent says reaches other processes only through its run's socket, redacted, never on disk or in the log. The `//!` of `observer.rs`, `task_cycle/stream.rs`, `process_registry.rs`, `surface/feed.rs` and the region's view say what is true now. Each test that holds a rule other code can break ends with its `Reached by:` line.

**Fails first:** `cargo xtask docs --check`, once the architecture rule cites its test, until that test exists in the target's own file with its name.

**Acceptance criteria:**
- [x] `cargo xtask docs --check` passes, and `cargo xtask docs --about` on the module's diff prints nothing left false.
- [x] No document, code comment or test cites a document, the spec or the plan.

**Verification:**
- [x] `cargo xtask docs --check`, `cargo xtask smells --check`

**Dependencies:** Tasks 1-11

**Files likely touched:**
- `docs/guide.md`, `docs/compatibility.md`, `.agents/architecture.md`
- the `//!` of the files named above

**Estimated scope:** S

### Checkpoint D: after Task 12
- [x] `cargo test --workspace --locked --no-fail-fast` passes. Its count is explained against 265 binaries and 4,414 tests on macOS, and no binary aborted.

## Phase 5: proof and close

### Task 13: measured on release builds

**Description:** On release builds of the base and of the module, with mock sessions that say a lot, runs take turns with and without three readers on the socket. Each reader records when each frame arrives against the frame's `at`. Then `repro.sh` runs again on the module's build.

**Acceptance criteria:**
- [x] The median time between nodes with and without readers is the same within noise, over at least 15 runs each.
- [x] The median delay before a frame reaches a reader is under 10 ms, and every frame arrives within 100 ms.
- [x] `repro.sh` on the module's build shows the socket, the words a reader gets with the secret redacted, the log unchanged, and the row the attached region draws.

**Verification:**
- [x] Results in `.plans/yunta-tui/proof/run-words/`, outside git, with the scripts that produced them.

**Dependencies:** Checkpoint D

**Files likely touched:** none in git.

**Estimated scope:** S

### Task 14: the real run

**Description:** A fragua run started with `--detach`, with its `reviewer` runners on the real `codex` and the model its configuration names, read before the run starts. It is followed at a terminal up to `ship`, while a reader keeps what the socket serves; then `abort` and `close`. Nothing is published. If a Codex retry notice ends a session, it is kept as evidence for `session-ends` and the cause is named before anything is resumed. The configured `gpt-6.1-sol` was refused by Codex for a ChatGPT account (run `X3GNEP`, `model_not_found`, no retry notice), so the user chose `gpt-6-luna` for the run that counts (corrected while implementing).

**Acceptance criteria:**
- [x] The follower showed rows of both CLIs, and the reader kept frames of both, with no secret in them.
- [x] Its time between nodes, read with `yunta stats`, is in line with the 0.741 s that `log-follow` measured, and its tokens are reported.

**Verification:**
- [x] The run's id, its stats and the frames kept, in `.plans/yunta-tui/proof/run-words/`.

**Dependencies:** Task 13

**Files likely touched:** none in git.

**Estimated scope:** S

### Task 15: every step of the `ci` job

**Description:** Every step of the `ci` job in `.github/workflows/ci.yml`, one build at a time, with `CARGO_INCREMENTAL=0`, through `sh .plans/yunta-tui/tools/gate.sh /Users/eulke/development/yunta-tui .plans/yunta-tui/out/gate-run-words`.

**Acceptance criteria:**
- [x] Every step passes: fmt, clippy, build, schema, smells, docs, deny, doc, test, core-release, workflows, crates. The test step's summary shows no aborted binary.

**Verification:**
- [x] The gate's logs, in `.plans/yunta-tui/out/gate-run-words/`.

**Dependencies:** Task 14

**Files likely touched:** whatever a failing step points at.

**Estimated scope:** S

### Checkpoint E: after Tasks 13-15
- [x] The user reviews the whole diff, the before and after, and the real run. No commit until approved.

### Task 16: commit the module

**Description:** One commit for the module on `feature/yunta-tui`, staging paths by name, with a message that cites no document. Then the push, the pull request's list and test plan, and CI on Linux and macOS. Then `cargo clean`.

**Acceptance criteria:**
- [x] The commit holds the module's files and nothing else, including `SPEC-run-words.md`, `tasks/plan.md`, `tasks/todo.md` and the map's row for `session-ends`.
- [ ] CI on the pull request passes on Linux and macOS, with the new tests seen in the log.

**Verification:**
- [ ] Manual check: `git show --stat HEAD`, `git status` and `gh pr checks 30`.

**Dependencies:** Checkpoint E

**Files likely touched:** none beyond the module's.

**Estimated scope:** XS

## The spec's success criteria

| Criterion | Tasks |
|---|---|
| 1. Every adapter's words reach a reader whole and in order, never its reasoning | 3, 4, 5, 6, 7 |
| 2. A secret or a session token never reaches a reader | 3, 7 |
| 3. The log is unchanged, and no file holds the words | 3, 4 |
| 4. `engine.json` names the socket, where it lives, gone at every terminal | 2, 4 |
| 5. Late readers, other users, slow readers | 1, 2 |
| 6. The same row, attached and followed | 8, 9, 10 |
| 7. Nothing said off a terminal | 11 |
| 8. A run with no socket is followed as today | 10 |
| 9. Measured on release builds, and the real run | Checkpoint A, 13, 14 |
| 10. Every step of the `ci` job | 15, 16 |

## Not in this list

`session-ends` and the other modules of `CAPABILITY-MAP.md` have no spec, plan or tasks yet. Each goes through the same flow when this one closes.
