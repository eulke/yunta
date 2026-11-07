# Spec: `run-words`

Module of `CAPABILITY-MAP.md`. Status: **approved on 2026-11-05**.
Base: `feature/yunta-tui` at `3456baea`. Depends on: nothing. Needed by: `tui`, whose screen shows what an agent says once the socket exists.

## Objective

What an agent says while it works reaches whoever watches the run, in the process that drives it or in another one, through one Unix socket per run. The words are redacted before they leave the engine, never written to disk and never written to the log. The engine takes them where every adapter already hands them over, so `claude-code`, `codex` and `mock` all gain this at once. At a terminal, `yunta run` and `yunta status --follow` show under each working node the first sentence of the last thing one of its sessions said.

### What happens at the base commit

Reproduced with a mock session and no tokens, on a release build of `3456baea` (`.plans/yunta-tui/proof/run-words/repro.sh`, result in `before.txt`, outside git):

- **A session's words never leave the engine.** Each adapter hands the engine the whole of what a session says as `AgentEvent::Note`. `claude-code` reads it from an assistant message's `text` blocks, `codex` from an `agent_message` item, and `mock` from a `note` step. Both real parsers drop reasoning. The engine keeps only the size and sha256 (`task_cycle/stream.rs`, `note()`). The log row is `{"kind":"agent_message","message_type":"note","note":{"bytes":55,"sha256":"30b036dca9cd"}}`, and no row holds the words or the secret they contained.
- **No process can hear them.** While the session is open, the run's scratch holds `engine.json engine.lock index sessions staging` and no socket. Nothing that was said appears in `status --follow` off a terminal, in what the run prints, or in any file under the home. The manifest is set aside, because it freezes the mock's own script. The region that an attached `yunta run` draws on a pseudo terminal never shows the words either.
- **The region already has the parts it needs.** A working node's rows show what the node is, its running tasks, and its last calls (`cli/src/surface/view/mod.rs`). `surco::prose::first_sentence` already extracts the one sentence a row says of a session's prose. `surco::paint` already turns every control character a session writes into a space.
- **Where a socket can live is bounded.** macOS binds a socket path of at most 103 bytes, and Linux of at most 107. `~/.yunta/runs/<id>/scratch/live.sock` is 69 bytes. The same path under a test home in `$TMPDIR` is 115, and under the repro's root it is 120.
- **The engine already has the redactor.** `RunLog::append` redacts with the run's declared secrets. A session's stderr tail is redacted with those secrets plus the tokens minted for that session.
- **The volume is small.** The last real fragua run, 3.11M tokens, said 15 things. A real run of 154 s measured on 2026-11-03 said 15 messages, about 4 KB.

### Who this is for

- **A person at the terminal that drives a run**, who sees in its own words what each node's agent is doing, not only which tools it called.
- **A person following the run from another terminal** with `yunta status --follow`, who sees the same.
- **`tui`, and later the desktop**, which read the same socket and show the words when it exists.

### What each of them gets

At a terminal, one row under each working node whose sessions said something. The row sits after `tasks running:` and before the calls (approved). For illustration; the plan settles its Surco words, and the goldens hold them:

```
⠹ implement running for 4m12s · executor claude-code/claude-sonnet-4-6
    tasks running: T001, T002
    T001 says: I found the bug in the parser, adding a test.
    read parse.rs · run cargo test ×3
```

| Case | What it does |
|---|---|
| A working node whose session said something | One row: the first sentence of the last thing one of its sessions said, cut to fit the row. It starts with `T001 says:` when the session belongs to a task, and `says:` otherwise |
| A node that has said nothing, or has stopped working | No row |
| Attached `yunta run` at a terminal | The row, fed by the engine in the same process. It never connects to its own socket |
| `yunta status --follow` at a terminal | The same row, read from the run's socket |
| Off a terminal, `TERM=dumb`, `--detach`, `--json` | Never prints anything an agent said. A pipe ends up in CI logs, and `--detach` writes to `detached.log` |
| A follower that connects late | What the socket still holds, then whatever is said next |
| A run with no socket: an older engine, a dead engine, or a refused connection | Followed as today, with no words row |
| A resume, or a promotion's successor | The new engine opens its own socket. The follower connects to it once the registry names it |
| A declared secret or a session token in what was said | `[redacted]` in its place, before the words leave the engine |
| A slow reader | Loses frames and never slows the run. The next frame it gets says how many it missed |
| Another user's process | Refused at connect |

A program gets two things: the frame, a versioned document sent one JSON line at a time, and a reader in the engine crate that turns a run's socket into a stream of frames.

### Decisions the plan starts from

- **One tap for every adapter.** Where the engine records a note's digest, it also hands what was said to the run's words. Every adapter that emits `Note` speaks with no adapter code, and today that is all three. A contract case holds that every adapter hands over a session's words whole and in order, and never its reasoning.
- **Nothing on disk, nothing in the log.** No new event kind, no new field in an event, no transcript. The log keeps the digest as today, so replay, `verify` and receipts do not change. Words live only in the engine's memory and in its readers' memory.
- **One socket per run, opened with the run's registry and gone with it.** The engine binds it when it begins to drive the run, where it writes `engine.json`, and removes it at every terminal. If a dead engine leaves a socket behind, the next engine for that run replaces it, and a reader that meets it treats the refused connection as no words.
- **The socket lives where it fits, and the registry names it** (the user's choice). It is `scratch/live.sock` when that path fits the system's limit. Otherwise it is `/tmp/yunta-<uid>/<run id>.sock`, in a directory created with mode 0700 and refused when another user owns it. `engine.json` names the path in both cases, so a reader never computes it. When an engine's registry names no socket, the run has no words.
- **Only the user's own processes connect.** The socket is created with mode 0600, and the engine also refuses a peer whose uid is not its own, because `scratch/` is 0755 today.
- **Redacted the way a session's output is.** Before a frame leaves the engine, the run's declared secrets and the tokens minted for the session are taken out, by the redactor that already does this.
- **The run never waits.** Handing a frame over is synchronous and never blocks, like `RunObserver::observe`. Each reader has a bounded queue. When a queue is full the frame is skipped for that reader, and the next frame it gets carries the count it missed.
- **Late readers get the recent past.** The socket keeps the last 64 things said in the run, up to 256 KiB, and hands them to a new reader before anything new. Anything said longer than 16 KiB is cut at a character boundary, and its frame says so.
- **One frame, whoever reads it.** The attached region gets from the observer the same frame that a follower reads off the socket, and the region does not know which feed it came from. The frame names the run, the node, the session, the task when there is one, the seq of the note's digest in the log, and when it was said. It carries the text and whether it was cut.
- **Words are not evidence.** A frame changes no state, moves no spin and resets no quiet clock. Those follow the log, and the log already receives a note's digest each time something is said.
- **A terminal only.** Words go to a person at a terminal. Every other output keeps the bytes it has at the base.

### Assumptions

1. The row comes from `surco::prose::first_sentence` applied to the last thing said. Each working node gets one row, not one per session. When several task sessions run at once, the row shows the latest of them, named by its task.
2. Attached `yunta run` changes at a terminal: it gains the words row (the user's choice). A golden changes only where a session in it says something, and its diff is read before it is kept.
3. MCP and `--json` get nothing. An agent reading a run's state has no need of another agent's words, and words stay at terminals. The attached region, the follower and later `tui` show the same row.
4. A child run has its own socket. A parent's follower shows the child by its row, as today, without the child's words.
5. Only whole messages. No incremental text is read: not Claude's partial messages, not Codex's App Server deltas. Both adapters hand over whole messages today, so they stay on par.
6. A real run proves it. A fragua run is started with `--detach`, with its `reviewer` runners on the real `codex` so that both real CLIs speak (approved, on the user's subscription), then followed at a terminal up to `ship`, about 3M tokens, and its words from both CLIs are recorded. Its time between nodes, read with `yunta stats`, is compared against the 0.741 s that `log-follow` measured. With mock sessions, runs take turns with and without three readers, which shows the time a reader adds and the delay before a frame reaches a reader.
7. `tasks/plan.md` and `tasks/todo.md` are replaced for this module. Those of `log-follow` stay in its commit.
8. The bounds hold what their readers need. The region needs one thing said per node, and the `tui` history needs about a screenful. A busy real run says about 4 KB in 154 s, so 256 KiB holds minutes of it.
9. The two bugs the audit of 2026-11-03 confirmed, a Codex `error` line that ends its session and an agent's commands that outlive the session that ran them, are fixed by `session-ends`, the module after this one in the map (the user's choice, which replaces recording them in `.agents/open.md`). This module neither records nor fixes them.

### Out of scope

- Words off a terminal, including behind an opt-in flag.
- A transcript, any other copy on disk, or a `yunta logs` command.
- Incremental output, the tail of a running command, a session's plan or its retries, and Codex App Server.
- The words of a child run in its parent's region.
- Steering a session or acting on a single session.
- Following a run on another machine.
- The two bugs of assumption 9, which belong to `session-ends`.

## Tech Stack

Rust at the version `rust-toolchain.toml` pins; tokio's Unix sockets (`UnixListener`, `UnixStream::peer_cred`); serde_json; the existing redactor, observer, registry and region. No new dependency. The `net` feature of tokio, which the engine already compiles through axum, is named in the engine's manifest.

## Commands

`cargo` means the wrapper that keeps this worktree's build target and state separate from other checkouts: `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.

```
# Before editing a file, and before finishing with no paths
cargo xtask docs --about <files>

# Build
cargo build -p yunta
cargo build --release --locked -p yunta

# Test
cargo test -p yunta-adapters --test contract
cargo test -p yunta-engine --test <the socket's target>
cargo test -p yunta --test <the row's target>
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

Before a commit, every step of the `ci` job in `.github/workflows/ci.yml` runs, one build at a time, with `CARGO_INCREMENTAL=0`. After the commit, `cargo clean`. `cargo xtask smells` never runs without `--check`.

## Project Structure

```
crates/engine/src/task_cycle/stream.rs        a note also hands what was said to the run's words
crates/engine/src/<the words' module>         NEW: the frame, the in-memory store and its readers, the socket, the reader of a run's socket
crates/engine/src/observer.rs                 the observer also receives what was said; by default it ignores it
crates/engine/src/process_registry.rs, write.rs   engine.json names the socket
crates/engine/src/run/invocation.rs           the socket opens with the registry and closes with it
crates/engine/src/run_dir.rs                  the socket's name under scratch
crates/engine/tests/                          the socket as a black box: redaction, late and slow readers, peers, lifetime, where it lives
crates/adapters/tests/contract/               what a session says, whole and in order, never its reasoning
crates/cli/src/surface/                       the words row, fed by the observer or by the socket
crates/cli/src/commands/status/follow.rs      the follower connects to the socket the registry names
crates/cli/tests/                             the row at a terminal for each adapter through its stub; nothing off a terminal
crates/surco/src/                             the word or the component the row needs, if any
docs/, README.md                              what a person can rely on
.agents/                                      what the module teaches, in its one home
SPEC-run-words.md                             this file
tasks/plan.md, tasks/todo.md                  this module's plan and task list
```

## Code Style

The shape, for illustration. The plan decides the names; `run_tools::words` already names what the engine says to an agent, so the new module must not be confused with it.

```rust
/// What a session said, as a run hands it to whoever listens: redacted before
/// it leaves the engine, never on disk and never evidence. Not the note's
/// digest, which is what the log keeps of the same words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Said {
    pub run_id: RunId,
    pub node_id: NodeId,
    pub session_id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// The seq of the note's digest on the log: where this sits among the
    /// run's events.
    pub seq: Seq,
    pub at: DateTime<Utc>,
    pub text: String,
    /// Whether `text` was cut to the longest a frame carries.
    pub cut: bool,
}
```

The repository's own rules hold: English in the present tense, a `//!` for each module that says how it works, a `///` with a reason for each numeric const, files under 500 lines, functions under 50, and no document cited in code, tests, commits or the pull request.

## Testing Strategy

| Level | What it proves |
|---|---|
| Unit, engine | Every reader gets each frame in order. A full reader skips frames and is told how many. The store keeps the last 64 frames within 256 KiB. A long text is cut at a character boundary. A peer with another uid is refused |
| Engine, black box | A mock session says a declared secret. A reader on the socket gets the frame with `[redacted]`, the log holds only the digest, and no file under the home holds the words. A late reader gets what the store holds. The socket is gone after finish, pause and cancel. A stale socket is replaced on resume. A path too long goes to the short directory. `engine.json` names the path in every case |
| Adapter contract | For every subject, what a session says arrives whole and in order, and its reasoning never does |
| CLI, pty | At a terminal, `yunta run` shows the row with the `claude-code` stub, the `codex` stub and `mock`. `status --follow` on the same kind of run shows the same row |
| CLI, black box | Off a terminal, with `--detach`, and under `--json`, nothing said is printed. A follower of a run with no socket follows it as today |
| Measure | Mock runs with and without three readers: the time a reader adds, and how soon a frame reaches a reader. The real run of assumption 6 |

Rules for these tests:

- Expected values are known apart from the code.
- A test that waits for a run waits on what the run writes, never for a fixed time.
- A test that claims two things happen at once makes them wait on each other.
- Goldens change only where a session in them says something, and their diff is read before it is kept.

Proof on a release build, with `.plans/yunta-tui/proof/run-words/repro.sh`, whose result at the base is kept beside it, and with the real run of assumption 6.

## Boundaries

**Always**
- Work in this worktree, and stage paths by name.
- Reproduce before changing, and measure after.
- One frame for every reader, and one row whoever feeds it.

**Ask first**
- A new dependency, or any change to the log, to an event, or to what an append writes.
- Any output off a terminal.
- Spending tokens beyond the real run of assumption 6.

**Never**
- Words in the log or on disk.
- A handover or a reader that makes the run wait.
- Touching `~/.yunta` or the main checkout's uncommitted files.
- Running `cargo xtask smells` without `--check`.

## Success Criteria

1. With each of `claude-code` (through its stub), `codex` (through its stub) and `mock`, what a session says reaches a reader of the run's socket whole, in the order it was said, with the node, the session, the task and the seq of its digest. Reasoning never reaches a reader.
2. A declared secret, or a token minted for the session, never reaches a reader in what was said: `[redacted]` stands in its place.
3. The log holds what it holds at the base: the note's digest, with no new kind and no new field. No file under the home or the run's scratch holds anything that was said. `status --json` and receipts are byte for byte the same.
4. While an engine drives the run, `engine.json` names the socket. The socket is `scratch/live.sock` when that path fits, and is in the user's short private directory otherwise. It is gone after the run finishes, pauses or is cancelled, and the next engine for the run replaces a stale one.
5. A process that connects gets up to the last 64 frames within 256 KiB, then each new frame. A peer of another uid is refused. A reader that never reads loses frames, is told how many, and does not change the run's time.
6. At a terminal, attached `yunta run` and `yunta status --follow` both show the words row under each working node, and draw the same row for the same run.
7. Off a terminal, with `TERM=dumb`, under `--detach` and under `--json`, nothing said is printed: those outputs are byte for byte what they are at the base for the same run.
8. A follower of a run that has no socket, or whose engine is older, follows it as today.
9. Measured on release builds: mock runs with and without three readers have the same median time between nodes, within noise, and each frame reaches a reader within the bound the plan sets. The real run of assumption 6 shows words from both CLIs, and its time between nodes is in line with 0.741 s.
10. Every step of the `ci` job passes on macOS and Linux.

## Open Questions

None. The three left at the draft were answered on approval: the two bugs go to their own module, `session-ends` (assumption 9), the real run uses the real `codex` (assumption 6), and the row sits after `tasks running:`.
