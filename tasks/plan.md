# Implementation Plan: `run-words`

Spec: `SPEC-run-words.md`, approved on 2026-11-05. Status: **approved on 2026-11-05**. Task list: `tasks/todo.md`.
Base: `feature/yunta-tui` at `3456baea`, on `main` at `34790ab8`, which had not moved when this was written.

## Overview

Where the engine records a note's digest, it also hands the note's words, redacted with the session's secrets, to the run's in-memory store of what was said. The store keeps the recent past and hands each new frame to every reader without waiting for any of them. One Unix socket per run serves the store to other processes of the same user; `engine.json` names where it is, and every terminal removes it. The observer hands the same frame to the attached region. The painter keeps the last thing said by each working node and draws it as one row at a terminal, whether the frame came from the observer or from the socket. No event, no field of the log, no file holding words and no dependency is added.

## Architecture Decisions

**1. One frame, `Said`, in a new engine module.**
- It names the run, the node, the session, the task when there is one, the seq of the note's digest, when it was said, the text, whether the text was cut, and how many frames the reader missed before it. A version field comes first.
- On the socket it is one JSON object per line, written by the engine and never read from the reader. A reader skips a line whose version it does not know and keeps reading, so an engine newer than its reader costs words, never the follow.
- It lives in the engine because the engine produces it and the CLI already reads the engine's types (`Observed`, `Engine`). `run_tools::words` already names what the engine says to an agent, so the module is named after what a session said, and its `//!` says what it is not.
- Not an event and not in `yunta-core`'s schemas: nothing persists it, and no person or workflow writes it.

**2. The store: the recent past under one lock, and a broadcast for what comes.**
- A `std::sync::Mutex` holds the last 64 frames within 256 KiB, and a `tokio::sync::broadcast` sender hands each new frame to the readers. Handing a frame over pushes it, drops the oldest past either bound, and sends it, all under the lock. A reader subscribes and copies the recent past under the same lock, so no frame reaches it twice or not at all.
- `broadcast::send` never waits. Each reader's queue is bounded by the channel's capacity. A reader that falls behind gets `Lagged(n)`, and its next frame carries `missed: n`. That is exactly the policy the spec asks for, from a crate the engine already uses.
- A text longer than 16 KiB is cut at a character boundary, and its frame says so. Each bound is a numeric const with its reason.
- Why not one channel per reader: it would rebuild the lag count by hand, and each send would walk the readers on the run's own task.

**3. Where the socket lives.**
- `scratch/live.sock` when its path fits the platform's limit: 103 bytes on macOS and 107 on Linux, the size of `sun_path` minus its closing NUL. One const per target, with that reason.
- Otherwise `/tmp/yunta-<uid>/<run id>.sock`, which is 46 bytes. The directory is created with mode 0700. If it already exists, it must be a directory, not a link, owned by this uid, with no group or other bits. If it is anything else the run has no socket, and the engine says why in its tracing.
- `/tmp` and not `$TMPDIR`: the path is the same whatever environment the engine was started from, and it is short on every host. A reader never computes the path: it reads it from `engine.json`.
- The uid comes from `yunta-core`, through `rustix::process::getuid`, which the workspace already compiles with its `process` feature and which needs no `unsafe`.
- This file work lives in the new module, and each `std::fs` call carries its `// blocking:` reason. The engine's filesystem smell exempts `process_registry.rs` itself, which is at 468 lines, and not the files beside it (corrected while writing the task list).

**4. The socket opens with the registry and closes with it.**
- `build_ctx` binds it right after `open_registry`, inside the runtime that `execute_run` runs on. A socket already at the path is removed first, only if it is a socket. The engine holds the run's lock, so nothing else can be using it.
- The socket is set to 0600 after binding. Each connection is admitted only if `peer_cred().uid()` is the engine's own uid. Between `bind` and `chmod` the umask's mode already leaves other users without write permission, and the uid check closes that window regardless.
- Each admitted reader gets its own task. The task writes the recent past, then each frame, serializing it there and never on the run's path. A reader that never reads blocks only its own task, until the broadcast drops frames for it.
- The handle lives in `RunCtx`, beside the registry. Dropping it cancels the accept loop and every reader task, and removes the file. `RunCtx` drops with the invocation: at a finish, a pause, a cancel or an error. A crash leaves the file behind, and the next engine for the run replaces it.
- A child run has its own `execute_run`, so it has its own registry and its own socket. A promotion's successor is another invocation, with another socket.

**5. `engine.json` names it, and the engine reading carries it.**
- `EngineProcessFile` gains `words: Option<PathBuf>`, with a serde default and skipped when empty. Its schema stays at version 1: the file does not deny unknown fields, so an older binary reading a newer file ignores the field. A test holds that.
- `ProcessRegistry::clear` empties the field: when the file stays at a terminal to name a group the engine could not stop, it no longer names a socket.
- `Engine`, the reading `status` and the follower take from the registry, gains `words`, named only while the engine reads alive. `Engine::this` names none, because the attached region never connects to its own socket.

**6. The tap is the note.**
- `stream_session` builds the session's redactor from its request before the request is moved: the declared secrets bound in its environment, plus the rail's token and the run tools' token. That is what the session's stderr tail is redacted with, plus the token that the adapter adds to the environment later. It is one function, on `SessionRequest`, in `yunta-core`.
- `note()` records the digest as it does today, takes the digest's seq, and hands the redacted text through a new `SessionObserver::said` method whose default ignores it. `RunCtx` implements it: it stamps the run and the time from the run's clock, hands the frame to the store and then to `RunObserver::said`.
- Before the session has opened there is no session to name, so nothing is handed over. The stream guard already fails any line that comes before the opening.
- Why not through `RunLog`: it is the one door to the log, and words never go through it. The observer's `//!` gains this second door: what was said reaches an observer through `said`, never through the log's mirror.

**7. The attached region gets the frame from the observer.**
- `RunObserver::said(&self, &Said)` has a default that ignores it, so the testkit's observers and the engine's tests do not change.
- `Feed` implements it with `try_send(Beat::Said)`. A full queue drops the frame, because words are not evidence and the next thing said replaces it.

**8. The row: the painter keeps the last thing said, and the view draws it.**
- The painter keeps the last frame per node of the run it draws. A frame of another run, such as a child's, is ignored. When the painter moves onto a successor it starts empty, and an entry is dropped when its node is no longer working, so a rerouted node never shows its last attempt's sentence.
- `Region::show` hands those frames to `view::node_rows`. The row comes after `tasks running:` and before the calls. Its label, `says:`, or `T001 says:` for a task's session, is `Tone::Muted`. The sentence is `surco::prose::first_sentence` of the text in `Tone::Plain`, cut by the region's `fit`. `surco::paint` already turns a control character into a space, and a test sends an escape in what was said.
- Only a `Draw::Live` region draws it. `Draw::Lines` never sees a frame, so nothing said goes off a terminal: not to a pipe, not under `TERM=dumb`, and not to the `detached.log` of a `--detach` engine. `--json` has no surface.
- A frame moves no spin and resets no quiet clock: those are read from the log, which receives the digest's own event.

**9. The follower listens to the socket the registry names.**
- The follow loop already reads the registry once a second. When the `words` path it reads differs from the one it listens to, it stops the old reader and starts `listen(path)`, a task that hands each frame to the surface through a new `Surface::hear`. That covers a follower that starts before the engine binds, a resume, and a successor.
- A refused or failed connection gives no words and is tried again at the next registry read. A run with no socket is followed as today.
- The reader is a function in the engine module of decision 1, so `control` and `tui` read the socket the same way.

**10. Every adapter, held by the contract.**
- A contract case: what a session says reaches the stream whole and in order, and its reasoning never does.
- The contract's `Step` gains reasoning, rendered as each CLI writes it: Claude's `thinking` block and Codex's `reasoning` item. The mock gains a `reasoning` fixture step, which it renders as its dialect's line and which its parser drops, so the mock expresses the fact through the same parser and the case runs on every subject. The step's `///` says what it stands for: the closed set of steps lives in the type, and no document lists it (corrected while writing the task list).
- In whole runs, each of `claude-code` through its stub, `codex` through its stub and `mock` says something that a test reader gets off the socket.

**11. What a person and a program rely on is written down.**
- `docs/guide.md` describes the row and where it never appears. `docs/compatibility.md` describes the socket for a program: where `engine.json` names it, that a frame is one versioned JSON line, that a frame is never evidence, and that a newer version is skipped. `README.md` changes only if its rows describe what a run shows.
- `.agents/architecture.md` gains the rule: what an agent says reaches other processes only through its run's socket, redacted, never on disk or in the log, held by the engine's test target.
- The `//!` this module makes false is rewritten: `observer.rs`, `task_cycle/stream.rs`, `process_registry.rs`, `surface/feed.rs`, and the region's view.

## Dependency Order

```
1 the frame and the store
        │
        ├──────────────────────────────┐
        ▼                              ▼
2 the socket and its reader      3 the tap: a note hands its words over
        └──────────────┬───────────────┘
                       ▼
4 the socket opens with the run, the registry names it
                       │
        ┌──────────────┴──────────────────────┐
        ▼                                     ▼
5 the mock says reasoning               8 the row, in the view
6 the contract                          9 the attached region
7 every adapter in whole runs          10 the follower listens
        └──────────────┬──────────────────────┘
                       ▼
11 nothing said off a terminal     12 docs and agent docs
                       │
                       ▼
13 measurements   14 the real run   15 the ci job   16 commit
```

Tasks 2 and 3 both need only Task 1. Tasks 5 to 7 touch other files than Tasks 8 to 10, so they could run beside them. The list keeps them in order so that one worktree stays green between tasks. Task 8 needs only the frame of Task 1.

The task list splits three of the tasks this plan first named, because each touched more than five files: the tap and the socket's opening are Tasks 3 and 4, the mock's step and the contract are Tasks 5 and 6, and the row is Tasks 8 and 9.

## Phases

### Phase 1: the words of a run, in the engine
- Task 1: `Said` and the store: the recent past, cutting a long text, a reader's queue that skips and counts.
- Task 2: the socket and its reader: where it lives, binding it and replacing a stale one, refusing another uid, serving each reader, removing it.
- Task 3: the tap: the session's redactor, and `said` from the note to the store and the observer.
- Task 4: the socket opens with the run: the registry names it, the engine reading carries it, every terminal takes it away.

**Checkpoint A:** a mock run's words read off its socket by a test reader, redacted, with the log unchanged; and a release build's mock runs taking turns with and without three readers, to show early that a reader adds no time. Shown to the user.

### Phase 2: every adapter
- Task 5: the mock's `reasoning` step, rendered as each dialect writes it.
- Task 6: the contract case, with the contract's wire writing reasoning as each CLI does. The stubs print whatever lines they are handed, so they do not change.
- Task 7: `claude-code` and `codex` through their stubs, and `mock`, in whole runs, a declared secret redacted. The tokens minted for a session are held by a unit test on its redactor, since a scripted session cannot say them.

**Checkpoint B:** the contract on every subject and the whole runs, through the wrapper.

### Phase 3: the row
- Task 8: the row in the view.
- Task 9: the attached region: `RunObserver::said`, `Beat::Said`, the painter's last frame per node. Goldens read before they are kept.
- Task 10: the follower: `Engine.words`, `Surface::hear`, listening and listening again, the same row as the attached run.
- Task 11: nothing said off a terminal: a pipe, `TERM=dumb`, `--detach`'s `detached.log`, `--json`.

**Checkpoint C:** `smells --check`, and the row read at a terminal, attached and followed, with mock sessions.

### Phase 4: what it says
- Task 12: docs, agent docs, and the `//!` this module makes false.

**Checkpoint D:** the whole suite, its count explained against 265 binaries and 4,414 tests on macOS, no binary aborted.

### Phase 5: proof and close
- Task 13: measured on release builds: how soon a frame reaches a reader, and the time readers add. `repro.sh` again.
- Task 14: the real run: fragua with `--detach` and its `reviewer` runners on the real `codex`, followed at a terminal up to `ship`, the words of both CLIs kept, then `abort` and `close`.
- Task 15: every step of the `ci` job.

**Checkpoint E:** the user reviews the whole diff and the before and after. No commit until approved.
- Task 16: the commit, the push, the pull request's list and test plan, and CI on Linux and macOS.

## Verification

- Each task names the test that fails first and why, and passes with `cargo test` on its targets through the wrapper.
- Goldens move only where a session in them says something, and only in Task 9.
- `cargo xtask smells --check` at each checkpoint. The gate runs before the commit, with `CARGO_INCREMENTAL=0`.
- Measurements on release builds are kept in `.plans/yunta-tui/proof/run-words/`, outside git. The bound for a frame reaching a reader: a median under 10 ms, and every frame under 100 ms, the bound a follower's events met in `log-follow`.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Handing a frame over slows the run | High | It costs one lock, one push and one send, and serialization happens in each reader's task. Checkpoint A measures it on a release build before anything draws, and Task 13 measures it again |
| The path does not fit, or `/tmp/yunta-<uid>` belongs to someone else, and a run gets no words | Med | `engine.json` names no socket and the engine's tracing says why. Tests on macOS go through the short directory and Linux CI through `scratch/`. A unit test holds the choice at the limit |
| A reader that never reads holds memory | Med | The broadcast's capacity bounds its queue and the kernel bounds its socket buffer. A test leaves a reader unread while the run goes on |
| A word carries a secret nobody declared | Med | The same exposure the CLI has in its own terminal, and only the same uid reads it. The compatibility doc says so |
| A crash leaves a socket file behind | Low | It holds nothing. The next engine for the run replaces it, and a reader treats the refused connection as no words |
| What a session says carries a terminal escape | Med | `surco::paint` turns every control character into a space. Task 8 sends one through the row |
| `task_cycle/stream.rs` is at 473 lines and passes 500 | Med | The note makes one call into the new module, and the redactor is one field of the context. A split is made if the count says so |
| The row of the attached run and of the follower drift apart | Med | One painter and one view whoever feeds them. Task 10 compares the two rows on the same run |
| A test that claims two processes overlap passes when they run in turn | Med | Sessions hold until the test lets them go (mock `hang`, the stubs' linger), and the test waits on what the run writes |
| A Codex retry notice ends the reviewer's session during the real run | Med | That is the bug `session-ends` fixes. If it happens it is kept as that module's evidence, and the run is resumed only once the cause is known |
| The real Codex model name is unknown | Low | Read from the user's Codex configuration in Task 14, before the run starts |

## Parallelization

One person and one worktree: tasks run in order. Tasks 5 to 7 could run beside Tasks 8 to 10, and Task 12 beside Task 11, but nothing is gained by splitting them.

## Open Questions

None. The plan settles the module's name, the readers' queue capacity and the reader's retry from what Tasks 1, 2 and 10 measure, within the bounds the spec sets.
