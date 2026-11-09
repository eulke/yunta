# Implementation Plan: `run-pages`

Spec: `SPEC-run-pages.md`, approved on 2026-11-07, amended on 2026-11-08. Status: **approved on 2026-11-07, amended on 2026-11-08** (decision 5: the frame measures, what a run's registry says is one `Engine` enum, and render's `engine` says it). Task list: `tasks/todo.md`. Base: `feature/yunta-tui` at `13cdd69c`. `cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.

## Overview

Every page Yunta shows becomes a value in `yunta-render` that one trait draws. What a run offers next becomes data in `yunta-replay`, and every command a reader is told to type becomes a `NextStep`, spelled once.

The work goes bottom up, so that by the time a page moves it names nothing of the CLI:
1. what the pages stand on: the crate edge, `DecisionOn`, the run directory's layout;
2. the commands, as data and spelled once;
3. the actions a run offers;
4. one decision block, the words of a parked run, and what drives a run: measured by the frame, read once as one `Engine` enum, said by render's `engine`;
5. what is left of the CLI's `render` module;
6. one form for every page, changed in place;
7. the run's pages move;
8. every other command's page moves;
9. the review.

Each phase ends with the workspace compiling, its suite green and every golden byte for byte. The user reviews it, and it is committed.

## Architecture Decisions

**1. Every command a reader is told to type is a `NextStep`.** It is never serialized: no payload, schema or JSON document holds one, so its shape is free, and its unused `Serialize`, `Deserialize` and `JsonSchema` derives go. A blank the reader fills is part of the data:

```rust
// crates/core/src/step.rs
/// A command line a person or an agent types, as data: every one Yunta tells
/// a reader to type, and the git command it points a reader to.
/// `yunta_render::command` writes each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextStep {
    Status { run: RunId, node: Option<NodeId>, depth: Depth },
    Needs,
    Follow { run: RunId },
    Resume { run: RunId },
    Cancel { run: RunId },
    Close { run: RunId },
    Review { run: RunId },
    Open { run: RunId },
    Receipt { run: RunId },
    Verify { run: RunId },
    ResolveGate { run: RunId, choice: Choice },
    Run { workflow: Arg<String> },
    Check { workflow: String },
    List,
    ListRuns { reach: Reach },
    Schemas,
    Schema { document: Arg<String>, json: bool },
    Doctor { sessions: bool, full: bool },
    Test { dir: Option<String> },
    Init,
    PackAdd { source: Arg<String> },
    PackUpdate { pack: PackRef, reference: Arg<String> },
    PackRemove { pack: PackRef },
    PackNew { pack: PackRef },
    Gc,
    Help,
    ShowCommit { commit: String },
}

/// An argument of a command line: the value a reader types as it is, or a
/// blank they fill in, written as the argument's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arg<T> {
    Given(T),
    Blank,
}

/// How a `resolve-gate` line chooses on a decision: with no option, which puts
/// the decision to the person at a terminal; with an option left blank for
/// the reader to pick off the menu above it; or with `option`, its words left
/// blank when it asks for some.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Asked,
    Blank,
    Picked { option: OptionId, words: bool },
}

/// Which runs `list --runs` lists: this repository's open ones, the closed
/// ones too, or every run on this machine.
pub enum Reach { Repository, WithClosed, Machine }

```

`yunta_render::command` writes them. `Reader` moves there from the CLI's `error.rs`:

```rust
// crates/render/src/command.rs
/// Who copies a command off a line: a person, who reads a run by its handle,
/// or an agent, who copies its whole id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reader { Person, Agent }

impl Reader {
    /// The run as this reader names it on a line.
    pub fn name_of(self, run: &RunId) -> &str;
}

/// The command line a person types for `step`.
pub fn line(step: &NextStep) -> String;

/// The command line `reader` types for `step`.
pub fn line_for(step: &NextStep, reader: Reader) -> String;

/// What an action does, in the words beside its command.
pub fn effect(effect: &Effect) -> &'static str;
```

What goes away:
- `Spell`, and every parameter that carries it: render's gate, plan, spec, shown, receipt, published and decision block; the forge's `GitHubForge::configure(repo, token)` and `MockForge::new(state)`.
- `yunta_testkit_core::spell`, the copy in `engine/tests/receipt.rs`, and `the_test_spelling_matches_the_cli_for_every_step`.
- Every command function of `advice`. `render::pause::make_room` stops spelling `yunta gc` itself.

In their place, one test in the CLI parses the line of every kind of `NextStep`, for both readers, with the CLI's clap definition: a spelling clap refuses fails it. A test in render writes every kind once.

Seven texts change, because a better form of a command replaces a worse one. The documentation follows each:
- `yunta schema <kind>` becomes `yunta schema <document>`, its argument's one name, which is also what clap calls it. This touches the skill `init` writes, `docs/concepts.md`, `docs/guide.md`, `docs/reference.md`, `docs/troubleshooting.md` and `docs/compatibility.md`.
- "the workflow fails `yunta check`" names the workflow it refused: `yunta check <path>`.
- `--adapter mock` with `--detach` says it "hands the run to another process", since `yunta resume` is not a line to type there.
- `yunta stats` with neither a run nor `--workflow`, or with both, is refused by clap as an argument group. That is the refusal every other usage error gets, and it removes the two hand-written sentences that named `yunta stats` bare.
- Three texts of `yunta pack` named a subcommand bare, a line clap refuses for want of its arguments. Each becomes a whole line with the pack its context knows, chosen by the user on 2026-11-07:
  - "use `yunta pack update` to change its ref, or `yunta pack remove` first" names `yunta pack update <publisher/name> <ref>` and `yunta pack remove <publisher/name>`;
  - "`yunta pack add` it first" names `yunta pack add <source>`;
  - the README `pack new` writes says it was scaffolded by `yunta pack new <publisher/name>`.

**2. What a run offers next is `RunFrame::actions`.** `after_for`'s rules move whole into `crates/replay/src/frame/actions.rs`:

```rust
/// What a reader can do next for a run: the command, and what it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub step: NextStep,
    pub effect: Effect,
}

/// What an action does to the run it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    ContinuesFromLog,
    ContinuesWhereStopped,
    WaitsForReset,
    HandsBackOnceSettled,
    RetriesOnceTreeChanged,
    ReadsPlan,
    ShowsWhatItWaitsOn,
    Stops,
    Closes,
    Certifies,
    FindsWhereLogStops,
}

impl RunFrame {
    /// The actions a reader can take next for this run, `decision` being the
    /// one it stopped on when its log rebuilds one.
    pub fn actions(&self, decision: Option<&GateWaitingPayload>) -> Vec<Action>;
}

impl RunPhase {
    /// What a parked run waits on; `None` for a run nobody has to touch.
    pub fn waiting_on(&self) -> Option<&WaitingOn>;
    /// The node a run paused on a limit's reset is held for, and when it resumes.
    pub fn reset_wait(&self) -> Option<(&NodeId, DateTime<Utc>)>;
}
```

- `advice::parked` and `advice::reset_pause` become those two methods: they are readings of replay's data.
- The run page and the closing take the actions and draw them with `command::line` and `command::effect`. The JSON a program reads draws them with `command::line_for(.., Reader::Agent)`.
- A step one page adds of its own stays the page's: the run page's `open` "in a browser", or the field's listings. It is a `NextStep` beside words of that page.

**3. The run directory a reader resolves is core's.** `ARTIFACTS_DIR` and `OBJECTS_DIR` are already core's, in `workflow/artifacts.rs`. They move to a new `crates/core/src/run_dir.rs` with the compositions that use them:
- `object(run_dir, hash)` was `ObjectStore::path_of`;
- `view(node, name)` was `view_path`;
- `view_of(shown)`;
- `artifacts(run_dir)` was `artifacts_view`.

The engine's `ObjectStore`, `run_dir.rs` and `artifacts/shown.rs` call them, and the engine keeps its working space (`scratch/`, staging, units), which no reader resolves. The run's branch is one value per run, so the caller computes it with `run_branch` and hands it to the page. Each item has one path, `yunta_core::run_dir::…`, and core's root re-export of the two constants goes.

**4. One decision block, in `yunta_render::escalation`.**
- `status/decision.rs` joins it: `section`, `claim_words` and `run_tree_line`.
- So do `gate::decision`, `gate::record`, `gate::routes` and `Route`.
- `run_page::Decided` becomes `escalation::Decision`:

```rust
/// The decision a parked run waits on, as its log rebuilds it: whose it is,
/// the escalation, and the documents it shows.
pub struct Decision<'a> {
    pub on: &'a DecisionOn,
    pub escalation: &'a GateWaitingPayload,
    pub shown: &'a [ShownDocument],
}
```

`DecisionOn` moves from `engine/src/run/escalation.rs` to `core/src/events/gates/escalation.rs`, beside `Escalation`, in Phase 1. The CLI's prompts (`ask/`), `resolve-gate` and `review` take `Route` from `escalation`.

**5. The words of a parked run join `yunta_render::pause`. What drives a run is one `Engine` enum, measured by the frame and said by `yunta_render::engine`.**

`parked_on`, `parked_claim` and `parked_in_full` join `pause`, which says what the log recorded. With decisions 1, 2 and 4, nothing is left in `advice`, and it is deleted. Its tests split by what they hold:
- the actions go to `crates/replay/src/frame/actions/tests.rs`;
- the spelling to `crates/render/src/command/tests.rs`;
- the words to `crates/render/src/pause/tests.rs` and `crates/render/src/engine/tests.rs`.

What a run's registry says is a choice. It becomes one type, read once:

```rust
// crates/replay/src/registry.rs
/// What a run's registry says about the process driving it, as far as the
/// registry can prove. Not the process itself: a reading of what it wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Engine {
    /// The process the registry names is the one that wrote it.
    Alive {
        /// The process: whose terminal a question is asked at.
        pid: Pid,
        /// The question it holds at its terminal.
        prompt: Option<Prompt>,
        /// The socket it serves the run's words on.
        words: Option<PathBuf>,
    },
    /// It names a process that is gone, or a number the host has since given
    /// to another: nothing drives the run.
    Dead,
    /// No registry: none drives the run now, or one is about to write it.
    Unrecorded,
    /// A registry this binary cannot read, or a process the host cannot tell about.
    Unknown,
}

/// The question a live process holds at its terminal: the node it asks about,
/// when one does, and since when. The record `engine.json` holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prompt {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
    pub since: DateTime<Utc>,
}

/// The process driving a run asking a person at its terminal, measured at an
/// instant: whose terminal, the question, and how long it had been asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asking {
    pub pid: Pid,
    pub prompt: Prompt,
    pub waited: Duration,
}

impl Engine {
    /// Whom the live process asks, measured at `at`: the one place an
    /// `Asking` is built. `None` when it asks no one, or none lives.
    pub fn asking(&self, at: DateTime<Utc>) -> Option<Asking>;
}

// crates/engine/src/process_registry.rs
/// What the registry under `run_dir` says about the process driving the run:
/// one read of the file, one probe of its process.
pub fn read_engine(run_dir: &Path, probe: &dyn OwnerProbe) -> Engine;
```

- The pid is the process's, as the registry's `engine_pid` is. The question is `Prompt { node, since }`, and a sentence about a process that asks takes `Asking`, which joins the two.
- `Prompt` is the one type of the record: the engine's `Asked`, its copy in `engine.json`, is gone, and `EngineProcessFile.asking` holds a `Prompt`. The file's bytes stay the same.
- `Engine::this(pid)` is the process reading the run: alive, asking no one a reader elsewhere could answer, naming no socket. Replay reads no process, as it reads no clock, so its caller hands it `Pid::current()`. `asks_about(node)` stays.
- `close_run`, `gc`, the pool's slot and the CLI's `close` match the variant where they matched `EngineLiveness`. The follower takes the socket off `Alive`.
- `engine_liveness`, `engine_prompt` and `engine_words` are gone, and `read_engine` reads the registry once. So is the CLI's `render::engine`, whose callers call `read_engine`. The CLI's `EngineReading` keeps its name, and its `read()` takes no instant.
- The name is the engine's, as the process is everywhere else: `engine.json`, `engine_pid`, `engine.lock`, and what a reader reads. `Driver`, chosen first, is a word this repository retired, what an adapter is not, and the `banned_vocabulary` smell refused it at Task 18e.

The frame measures, at its one instant:

```rust
// crates/replay/src/frame/mod.rs
impl RunFrame {
    pub fn of(
        run_id: &RunId,
        workflow: &Workflow,
        state: &RunState,
        prior: Option<&PriorEstimation>,
        engine: &Engine,
        now: DateTime<Utc>,
    ) -> RunFrame;
}

pub struct RunFrame {
    // …
    /// The person the process driving the run is asking at its terminal,
    /// measured at the frame's instant.
    pub asking: Option<Asking>,
    /// From the log's last event to the frame's instant: how long the run has
    /// stood where it is. `None` for a log with no event.
    pub since_last_event: Option<Duration>,
    /// Where the run's time went: to the frame's instant while it is open, to
    /// its last event once it has ended.
    pub time: TimeSpent,
}
```

- `asking` is `engine.asking(now)`. Each surface keeps the phases it says it under.
- `since_last_event` is zero for a last event stamped after `now`, as both subtractions it replaces were.
- `Engine::at` is gone: `now` is the frame's one instant, and no caller hands it twice.

`yunta_render::engine` says it, one `Asking` at three lengths, as `pause` says one `WaitingOn`:

```rust
// crates/render/src/engine.rs
/// What holds a run whose log says it moves and whose process is gone.
pub const STALLED: &str;
/// Whom the process asks, on a row's one line: the node.
pub fn asking_on(asking: &Asking) -> String;
/// Who asks, at which terminal, and that the answer is given there: beside a
/// headline that says how long, and in a refusal.
pub fn asking_claim(asking: &Asking) -> String;
/// The same, with how long it has asked.
pub fn asking_in_full(asking: &Asking) -> String;
```

- No page takes an `Engine`. The run page, the field, the status summary, the JSON and the live region read the frame. The run page's headline and gist read the same `waited`, and the run page and the field the same `since_last_event`.
- The refusal of a command on a run that asks at its terminal reads the registry with no frame. It holds `read_engine(..).asking(now)` at its own instant, so `refused_hold` takes that instant, and says `asking_claim`, as it does now: the length decides what is said, not the data held.

**6. Nothing of the CLI's `render` module is left.**
- `counter.rs` and `status/progress.rs`'s `summary` become `crates/render/src/progress.rs`.
- `node_display` becomes `impl From<&NodeFrame> for NodeDisplay`, and `called` moves into `crates/render/src/state.rs`.
- `render::engine` is gone already, with decision 5: its callers call `yunta_engine::read_engine`, since each of them changes that line when the reading loses its instant.
- `progress::frame` and `frame_of` are gone: their 9 callers replay once and call `RunFrame::of`.
- `stream.rs`, `surface_lines`, `blocks_lines`, `draw`, `RunExit` and `exit` join `output`, which becomes `crates/cli/src/output/` (`mod.rs`, `stream.rs`, `exit.rs`).
- `Outcome` moves from `error.rs` to `output/exit.rs`.
- `surface::child_row` and `children_by_node` become `crates/render/src/children.rs`, which the run page and the live region both read.

**7. One form for a page.**

```rust
// crates/render/src/page.rs
/// A page a person reads: a value its caller fills with what it read, drawn
/// into one document.
pub trait Page {
    fn document(&self, look: &Look) -> Doc;
}

// crates/cli/src/output/mod.rs
/// What a command hands back: the page a person reads, the JSON a program
/// reads (the answer itself), and the verdict its exit code reports.
pub(crate) trait Answer: serde::Serialize {
    fn page(&self) -> impl Page + '_;
    fn outcome(&self) -> Outcome;
}
```

- `emit` draws `answer.page()` for a person and serializes the answer for `--json`, so the JSON documents keep the types that print them today.
- A page moves in two steps. First its form changes in place: a struct, named by what it shows, implementing `Page`. Then it moves with `git mv`.
- `impl<P: Page + ?Sized> Page for &P` lets a page be handed by reference. While an answer still draws its page in the CLI, it is its own page (`fn page(&self) -> impl Page + '_ { self }`). When its page moves to render, `page()` builds the render value from the answer's fields.
- A command with no JSON draws its page with `output::show(look, &page)`, which takes the place of `output::document(look, doc)`.
- `crates/render/src/page.rs` is the map of every page, and `crates/render/src/page/` holds them:

| Page | From |
|---|---|
| `page::Run` (scopes `Status`, `Closing`, `Browser`), with `run/{facts,story,evidence}.rs` | `cli/src/run_page/` |
| `page::Node` | `cli/src/commands/status/node.rs` |
| `page::Field` (`field::{Group, Row}`) | `cli/src/commands/list/{runs,inbox}.rs` |
| `page::Stats`, `page::History` (`stats/{facts,time}.rs`) | `cli/src/commands/stats/{run,workflow,facts,time}.rs` |
| `page::Doctor`, `page::Verified` (`checklist.rs`) | `cli/src/commands/{doctor/mod,verify}.rs` |
| `page::Gate` | `render/src/gate.rs` (`GatePage`, `page`, `whole_page`, `above`) |
| `page::Plan`, its desk | `render/src/plan/` |
| `page::Spec`, `page::Shown`, `page::Findings`, `page::Receipt` | `render/src/{spec,shown,findings,receipt}.rs` |
| `page::Cancelled`, `page::Closed`, `page::Opened` | `cancel/page.rs`, `close.rs`, `open.rs` |
| `page::Answered`, `page::Collected`, `page::Review` | `resolve_gate.rs`, `gc/page.rs`, `review.rs` (the plan off a terminal, with the commands that answer it) |
| `page::Catalog`, `page::Initialized`, `page::NewWorkflow`, `page::Schema` | `list/mod.rs`, `init.rs`, `new.rs`, `schema.rs` |
| `page::pack::{Installed, Inventory, Updated, Removed, Listed, NewPack}` | `pack/page.rs`, `pack/scaffold.rs` |
| `page::Tested`, `page::TestedCase`, `page::Checked`, `VerificationFindings` as a page | `test/mod.rs`, `verdict.rs`, `stats/workflow.rs`; `drive.rs` has no page of its own |

`receipt::unknown_kinds`, which every page about a run reads, goes to `yunta_render::state` rather than into the receipt's page.

A name the work finds better is settled when the page moves and recorded in the task. `published` stays a module of render: it is what a forge publishes, made of pages, not a page.

**8. A page reads nothing.** `surface::closing::Outline` becomes `page::Places`:

```rust
/// Where a run lives and where its paths are shown from, as the caller found them.
pub struct Places<'a> {
    pub run_dir: &'a Path,
    pub worktree: &'a Path,
    /// The run's own branch.
    pub branch: &'a str,
    /// The branch it started from, as its manifest froze it.
    pub base_branch: &'a str,
    pub isolation: Isolation,
    pub cwd: &'a Path,
    pub home: Option<&'a Path>,
}
```

- The decision comes in as `escalation::Decision`, the documents as `ShownDocument`s, what drives the run through the frame (decision 5), and the closing's exit code as a number. No page holds an `Engine`.
- The stats workflow page stops drawing a sub-document to terminal lines: it appends the findings document's blocks. Its 3 goldens say whether that prints the same. If it does not, the work stops and asks.
- `crates/render/tests/pure.rs` reads render's manifest: core, replay and surco, and no engine, store or runtime.

**9. A row of the field is data.** `Inbox`'s `RunRow` holds `reason`, `command` and `said` already worded. `field::Row` holds instead:
- what the run is;
- the action it is listed for;
- what holds it, as replay's `WaitingOn` and the frame's `Asking`;
- how long it has stood there, the frame's `since_last_event`.

Render words each row. `list --runs --json` serializes the same fields through a serializer in the CLI's `json/`, built on render's words, so its bytes stay the same.

**10. The goldens move with their tests, by `git mv`.** `crates/cli/goldens/{status,closing,open,stats,field}` go to `crates/render/goldens/`, beside its own `blocks`, `document` and `gate`, with no clash. Each test reads `CARGO_MANIFEST_DIR/goldens/<dir>`, so only the crate changes. The CLI keeps no golden.

**11. The smell is `command_outside_command_rs`.**
- It counts every shipped crate but the two below render: core, whose schema titles read `yunta workflow`, and surco, whose catalog's samples are drawn. Neither can name render's `command` module.
- It skips `crates/render/src/command.rs` and the CLI's clap declarations.
- Measured over that reach at the base, it counts one line, `pause::make_room`'s `yunta gc`, which decision 1 removes.
- `xtask/smells.baseline` renames its key and keeps 0.
- Its fix reads "spell the command in yunta-render's command module".

**12. The docs, each in the commit that changes what it says:**
- `.agents/architecture.md`: render's row (core, replay, surco) and its rule's smell;
- render's `//!` map;
- each moved module's `//!`;
- the CLI's `main.rs` map;
- `docs/` for the texts of decision 1;
- `CAPABILITY-MAP.md`.

**13. A commit per phase.** Each one follows the user's approval of its checkpoint, after every step of the `ci` job passes one build at a time with `CARGO_INCREMENTAL=0`, and is followed by `cargo clean`. Phase 9 updates pull request #30.

## Dependency Order

```
0 baseline: goldens' digests, JSON outputs, release size
1 the edge render → replay, pure.rs, DecisionOn to core, core::run_dir, NextStep's derives
    └── 2 NextStep's kinds, yunta_render::command, Spell gone, advice's commands gone, the smell
            └── 3 RunFrame::actions, Effect, RunPhase's readings; after and after_for gone
                    └── 4 escalation's one decision block; pause's words; advice gone;
                          the frame's measures; Engine as one enum; engine's words
                            └── 5 the CLI's render module gone: progress, state, children, output
                                    └── 6 Page, Answer::page, render's documents as pages, every page's form in place
                                            ├── 7 the run's pages, their goldens, Places
                                            └── 8 every other command's page
                                                    └── 9 review, measure, pull request
```

Phase 8 stands on Phase 6 alone, so it could run before Phase 7. Phase 7 goes first because it moves most of the goldens, and so proves the method on the largest page.

## Phases

### Phase 0: the baseline
- The digest of each of the 99 goldens, 78 in the CLI and 21 in render.
- The JSON and terminal outputs of `status`, `status --node`, `status --json`, `list --runs [--json]`, `stats [--json]` and `stats --workflow`. They are printed by a release build at the base, over a copy of a few runs in a scratch home; `~/.yunta` is never touched.
- The release binary's size.
- All kept in `.plans/yunta-tui/proof/run-pages/`. No commit.

### Checkpoint A
- The numbers, and the suite's count at the base: 274 binaries and 4,531 tests.

### Phase 1: what the pages stand on
- `crates/render/tests/pure.rs` is written first, and holds from the start. The edge to `yunta-replay` follows, and the test still holds.
- `DecisionOn` moves to core, and `ShownDocument` and `ShownContent` are named from `yunta_core::shown`; the engine's re-export goes.
- `core/src/run_dir.rs` takes the constants and the four compositions.
- `NextStep` loses its unused derives.
- `.agents/architecture.md`'s row.

### Checkpoint B
- `cargo test -p yunta-core -p yunta-render -p yunta-replay`, the engine in batches, `-p yunta --bins`, and every golden's digest unchanged. Then the gate and commit 1.

### Phase 2: every command, spelled once
- `yunta_render::command` writes the nine kinds `NextStep` holds, with `Reader`.
- `Spell` goes from render, the adapters, testkit-core and the engine's tests. The two copies of the spelling go with it, before `NextStep` grows, so they are never extended only to be deleted.
- `NextStep` grows to every kind, with `Arg`, `Choice` and `Reach`. The clap parse test is written first, and is seen red on a probe.
- Every command function of `advice` goes, and its 44 callers spell through `command`.
- The seven texts change, with the docs and the tests that print them.
- The smell is retargeted.

### Checkpoint C
- The same suites, the adapters' forge tests, the CLI's integration tests, `smells --check` at 0, `docs --check`, and every golden unchanged. The only output that changed is the four texts. Then the gate and commit 2.

### Phase 3: what a run offers next
- `actions.rs`, with `after_for`'s tests moved and seen to pass against `RunFrame::actions`.
- `RunPhase::waiting_on` and `reset_wait`.
- The run page, the closing and the JSON take actions.
- `after` and `after_for` are gone.

### Checkpoint D
- `-p yunta-replay`, `-p yunta --bins`, `status_cmd`, `run_flow`, the parked-runs targets, and the JSON outputs of Phase 0 byte for byte. Then the gate and commit 3.

### Phase 4: one decision, and the words of a parked run
- `escalation` takes the decision block and `Decision`. The gate's page and the run page draw from it.
- `pause` takes the words. `advice` and its tests are gone, its tests split by owner.
- A follower that hears a session's words before it draws their node keeps them: the intermittent `run_words` test, measured at Checkpoint C, holds every time. Added at the user's request.
- Decision 5's second half, at the user's request on 2026-11-08, in four steps, each compiling and passing:
  - the frame carries `asking`, `since_last_event` and `time`, with `Asking`, its test written first;
  - every page reads them, and none reads the engine's reading;
  - what the registry says becomes one `Engine` enum, read once and with no instant, its pid on `Alive` and its question one `Prompt` with the engine's: `Engine::at`, `EngineLiveness`, `Asked` and the CLI's `render::engine` go, so each reader's line changes once;
  - the driving engine's words move from `pause` to `engine`.

### Checkpoint E
- The status, closing and gate goldens byte for byte, the captures, and the decision's JSON the same. Then the gate and commit 4.

### Phase 5: what is left of the CLI's `render`
- `progress.rs`, `NodeDisplay::from`, `called`, `children.rs`.
- `output/` with `stream`, `exit`, `Outcome` and `RunExit`.
- `frame` and `frame_of` are gone.

### Checkpoint F
- No `crate::render` is left in the CLI; the same suites. Then the gate and commit 5.

### Phase 6: one form
- A stop after a crash finds what the session started, every time: the intermittent `session_ends` test, measured at Checkpoint F. Added at the user's request.
- `page.rs` and `Page`.
- Render's documents become pages under `page/`, by `git mv`, losing their positional arguments: gate, plan, spec, shown, findings, receipt.
- `Answer::page`. The CLI's pages change form in place: each a struct that implements `Page`, with nothing moved yet. A page that read while it drew reads first, and its page draws what it read (the user's choice at Checkpoint G).
- `receipt.md` is written with the Markdown look whatever terminal `yunta receipt` runs on, as the forge's body and the engine's receipt are. Found in Task 28.

### Checkpoint G
- Render's 21 goldens and the CLI's 78 byte for byte. Then the gate and commit 6.

### Phase 7: the run's pages
Into `page/`, with their tests, fixtures and goldens:
- the run page;
- the node page;
- the field, with its rows as data;
- the stats and history;
- the doctor's and `verify`'s checklist.

`Places` replaces `Outline`.

### Checkpoint H
- The 78 goldens, now under `crates/render/goldens/`, byte for byte by digest; the Phase 0 outputs the same; `pure.rs`. Then the gate and commit 7.

### Phase 8: every other command's page
- The 16 pages of decision 7's last five rows, each moved with its test.
- What a repository says of itself (`detect`'s data) and what a pack's audit found (the engine's) are core's, the CLI and the engine keeping what reads them and render saying them: the user's choice in Phase 8, so `Initialized` and a pack's inventory can be render's pages.

### Checkpoint I
- No CLI file builds a `Doc` but `output` and the live region, which a search confirms. Then the gate and commit 8.

### Phase 9: the review
- `cargo xtask docs --about` on the whole diff.
- The suite's count explained against the base.
- The release size against Phase 0.
- A review of the module's diff, then pull request #30's list and test plan, CI on Linux and macOS, and `cargo clean`.

### Checkpoint J
- The user reviews the measurements and the pull request.

## Verification

| Spec criterion | How |
|---|---|
| 1. Every page in `page`, one form | Every page implements `Page`; a search for `Doc::new` in `crates/cli/src` finds only `output` and the live region |
| 2. Render reads nothing | `crates/render/tests/pure.rs`; a search for `yunta_engine`, `DateTime` and `chrono` in `crates/render` finds nothing; no page's fields hold an `Engine` |
| 3. Actions | `RunFrame::actions`'s tests, moved from `advice`; a search for `after_for` finds nothing |
| 4. One spelling | The clap parse test over every kind; `smells --check` with `command_outside_command_rs` at 0; a search for `Spell` finds nothing |
| 5. Names | A search for each old name in the spec's table finds nothing in `crates/` |
| 6. Nothing printed changes | 99 goldens' digests against Phase 0's; the Phase 0 outputs byte for byte, but for the seven texts of decision 1; the CLI's integration tests unchanged but for those texts; the suite's count explained |
| 7. Docs, gate | `docs --check`, `smells --check`, `schema --check`, `cargo deny check`, every step of the `ci` job, at each commit |

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| A move changes what a page prints | High | Every golden's digest at each checkpoint against Phase 0's; a move is `git mv` before any edit, read with `-M`; a page's form changes in place before it moves |
| A JSON document changes | High | The JSON keeps the types that print it today; the field's rows serialize through one serializer built on render's words; Phase 0's JSON outputs compared at Checkpoints D, H and I |
| Removing `Spell` changes what a forge publishes | Medium | The adapters' forge tests and the engine's pull request and external gate tests pass unchanged; render's `pull-request.md` golden holds |
| A blank or a new kind spells a line clap refuses | Medium | The clap parse test over every kind and both readers |
| The seven texts ripple into docs and tests | Medium | Each one named in decision 1; `docs_sync` and the docs' own checks pass; nothing else printed changes |
| The stats workflow page prints differently once it composes its findings | Low | Its 3 goldens decide; if they change, the work stops and asks |
| A duration a page prints changes once the frame measures it | Medium | Each measure is the subtraction it replaces, at the instant every caller already passed as `now`; the goldens, the captures and the JSON at Checkpoint E |
| One read of the registry reads differently than three | Low | The registry's tests, rewritten over `read_engine`, read each of the four kinds; the follower's and the parked runs' integration tests |
| `engine.json` changes when `Asked` becomes `Prompt` | Low | A test written before the change pins the `asking` an asking engine writes, and passes before and after it; `live_prompt` reads it from a live run |
| Phases 2, 7 and 8 touch about 60 files each | High | One concern per phase; the compiler finds every caller, since no alias is left; tasks of up to ~5 files each |
| The suite's memory on this Mac | Medium | The engine and the CLI in foreground batches, as in `run-state` |
| `.agents/architecture.md` outgrows its budget | Low | Render's row changes in place, a few bytes |

## Parallelization

Phases 1 to 7 change, one after another, the imports the next phase reads, so they run in order.

Phase 8's 16 pages are independent of each other: each is a command's own file and its test. They could be built by workers in waves on Haiku, with one audit each, if the user chooses that for Phase 8. Otherwise the lead moves them one by one.

## Open Questions

None. The seven texts of decision 1 change by this plan's approval, the three of `yunta pack` by the user's choice during Phase 2.
