# Implementation Plan: `app-core`

Spec: `SPEC-app-core.md`, approved on 2026-11-10. Base: `feature/yunta-tui` at `92d05277`, where `control` ended. `cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.

## Overview

Every surface Yunta will have gets the whole application by plugging in: the terminal application next, the desktop later. The CLI already gets every operation from `yunta-api`. What a person does at a screen still lives in the CLI, or nowhere:

- the prompts' models, mixed with the terminal in `ask/`;
- the words of the chronicle, of the region's rows and of every prompt;
- following a run, putting a run's page together, and the graph;
- every stage the spike drew, which nothing builds.

This plan moves the first three groups below the surfaces, printing what they print today. It then builds `yunta-ui`: the state a person has open, what each input does to it, the work it asks of Yunta as data, the performer that carries that work out through `yunta-api`, and each moment as a Surco `Screen`. Every stage is built, from the signpost to the command palette. It also makes setting a repository up declare its runners, in every interface, as the user chose.

## Architecture Decisions

### 1. What each layer is

The layers of `control` stand. One layer is added between the API and the surfaces.

| Layer | Is | Holds |
|---|---|---|
| `yunta-render` | what Yunta says | It gains the words of the chronicle, of a moving run's rows and of every prompt, the graph, and what each stage's screen says. It reads nothing. |
| `yunta-api` | what can be asked of Yunta | It gains following a run, a run's page read whole, and writing that page for a browser. A value a stage keeps owns its data. |
| `yunta-ui` | the user interface, apart from its medium | What a person has open, what each input does to it, the work it asks of Yunta, the performer that does that work, and each moment as a Surco `Screen`. It writes no sentence and knows no terminal. |
| Surfaces | the interfaces | The CLI, `yunta-mcp`, and next the TUI. Each reads its input and hands it in. The TUI carries out what `yunta-ui` asks and lays out the screen it returns. |

A prompt splits across four crates, each holding one concern:

| Concern | Crate | Example |
|---|---|---|
| Where the prompt stands, and what a stroke does there | `yunta-ui` | `DecisionCard` points at its second option. |
| What it says, as the Surco block it draws | `yunta-render` (`prompt`) | "enter chooses", "esc leaves the run waiting". |
| How that block is drawn | `yunta-surco` | the card on a terminal, fitted to 20 rows with the cursor; in HTML. |
| Holding it on a terminal | `yunta` (`ask`) | Raw mode, keys read as strokes, the alternate screen. |

### 2. The crates and their edges

| Crate | `[dependencies]` in the workspace |
|---|---|
| `yunta-ui` | core, replay, render, api, surco; `tokio` |
| `yunta` | core, replay, render, api, mcp, ui, surco |

- **A test reads `yunta-ui`'s manifest** (`no_terminal_or_engine_in_the_interface`), as `no_engine_in_the_cli` reads the CLI's. It fails, naming where the reading belongs, if `crossterm`, `console`, `nix`, `indicatif`, `ratatui`, `yunta-engine`, `yunta-storage` or `yunta-adapters` appears.
- **`.agents/architecture.md`'s table gains `yunta-ui`'s row and changes `yunta`'s.** The file stands at 6,140 of its 6,144 bytes. The row's bytes come out of what the file says twice or a module's `//!` already says, in the same commit. If no such sentence is found, the work stops for the user rather than raising the cap.
- **The map, the crate's own `//!` and `cargo xtask docs --check` agree.**

### 3. The application as values

```rust
// crates/ui/src/lib.rs, what a surface names
pub struct State { /* the stage, its dialog, what each stage read, the work in progress */ }

impl State {
    /// The application as it opens at `now`, and what it asks first: whether it stands
    /// in a repository, and whether that repository was set up.
    pub fn opened(now: DateTime<Utc>) -> (State, Vec<Request>);
    /// What `input` does: the state it leaves, and what it asks for.
    pub fn update(&mut self, input: Input) -> Vec<Request>;
    /// This moment, as a screen any surface lays out.
    pub fn screen(&self, look: &Look) -> Screen;
}

/// What reaches the application.
pub enum Input {
    Stroke(Stroke),
    /// What a surface laid out of the desk's pane: its first line, a page, the last first line.
    Laid(PaneLaid),
    Time(DateTime<Utc>),
    /// What a piece of work remarks while it works.
    Remark(Ticket, Remark),
    /// What lasting work hands back while it lasts: a followed run's news, a case that ended.
    News(Ticket, News),
    /// What a piece of work came to.
    Outcome(Ticket, Outcome),
}

/// What the application asks for after an input.
pub enum Request {
    /// Work Yunta does, which the surface hands its performer.
    Perform(Ticket, Operation),
    /// The state no longer wants this work: stop it if it is under way, and drop what
    /// the performer holds for it.
    LetGo(Ticket),
    /// A page a browser reads: a terminal hands it to the system's opener, the desktop
    /// shows it in a window.
    ShowPage(PathBuf),
    Leave,
}

/// Carries out an `Operation` through `yunta-api`, against the workspace it was made with,
/// and hands back what it remarks, its news and its outcome as inputs.
pub struct Performer { /* the workspace; each ticket's stop; what is held for a confirmation */ }

impl Performer {
    pub fn new(workspace: Workspace) -> Self;
    pub async fn perform(&self, ticket: Ticket, operation: Operation, inputs: UnboundedSender<Input>);
    pub fn let_go(&self, ticket: Ticket);
}
```

- **`Operation` has one variant per thing a stage asks of Yunta.**
  - Reading: the repository and the field, a run's page and a node's, following a run, the decision it waits on, the catalog, a workflow and a run's graph, stats and history, a verification, doctor, a collection's dry run, the packs, a check, detecting a project.
  - Acting: deciding, answering, cancelling, closing several runs, resuming, starting, writing a project's setup, writing a new workflow, running the cases, fetching, installing, updating, removing and scaffolding a pack, writing a receipt, collecting, writing a run's page for a browser.

  Each variant is named by its verb (`Operation::Decide { run, option, words }`).
- **`Outcome` has one variant per operation's data.** It also has three for an operation that did not do what it was asked: `Refused` (render's `Refused`, said by render), `Failed` (what the failure says), and `Stopped`.
- **A `Ticket` is the state's number for a piece of work.** An answer to a ticket the state let go is dropped, so leaving a stage can never be undone by a late answer.
- **What the performer holds between two pieces of work:**
  - a fetched pack, while a person reads its audit and confirms what it executes;
  - a detected project, while a person answers its setup.

  It holds each under the ticket that fetched or detected it, and drops it at `LetGo`. A pack is installed from the copy its audit read, never from a second fetch.
- **The state is `Clone`.** The walk branches it once per input.

### 4. Stages and dialogs

`Stage` is the enum of where a person is. Each variant holds a type of its own, in `stage::<name>`, with the `Stage` suffix: `Field`, `Catalog`, `Doctor` and `Stats` are render's pages, which those stages show.

| `Stage::` | Holds | Shows (render) | Asks (`yunta-api`) |
|---|---|---|---|
| `Signpost` | nothing | `Signpost` | `in_a_repository` |
| `Setup(SetupStage)` | what was detected, the answers given, the runners proposed | the detected project, then `Initialized` | `detect_project`, `DetectedProject::write` |
| `FirstRun(FirstRunStage)` | the request being typed, the catalog | the first run's question over the catalog | `catalog`, then Start |
| `Field(FieldStage)` | the reading, the row pointed at, the rows marked, the reach | `Field` | `field`, `resume_detached`, `cancel`, `close` |
| `Run(RunStage)` | the run's page, its moments, what its sessions said, the node picked, whether it follows | `Run`, `Node`, the chronicle, the moving rows | `follow`, `page`, `node_page` |
| `Decision(DecisionStage)` | the `Deciding` or the `Round` in front of the person | the card, the desk, the round | `waiting_decision`, `decide`, `answer` |
| `Start(StartStage)` | the request, the workflow, its inputs, mode and adapter, the cautions | the catalog, admission's cautions | `admit`, `create`, `start_detached` |
| `Evidence(EvidenceStage)` | which of the three, and what it read | `Receipt`, `Verified`, `Opened` | `receipt`, `verification`, the page for a browser |
| `Stats(StatsStage)` | a run's stats or a workflow's history | `Stats`, `History` | `stats`, `history` |
| `Graph(GraphStage)` | the workflow, and a run's frame when it is a run's | `Graph` | `read_workflow`, a run's page |
| `Catalog(CatalogStage)` | the catalog, the workflow pointed at, what a pack act is waiting on | `Catalog`, `Checked`, `NewWorkflow`, `Tested`, the pack pages | `catalog`, `verdict`, `new_workflow`, the cases, the packs |
| `Doctor(DoctorStage)` | the findings, folded or whole | `Doctor` | `doctor`, `collect` |
| `Upkeep(UpkeepStage)` | the dry run, then what was collected | `Collected` | `collect` |
| `Schema(SchemaStage)` | every document, or the one picked | `Schema` | none: a shape is core's, read with no IO |

- **What stands over a stage is a `Dialog`.** `Overlay` is Surco's region it is drawn into. The variants are `Palette(Palette)`, `Help` and `Confirmation(Confirmation)`.
- **Every act is a `NextStep`.** The palette lists the steps a stage offers, then those always there, each spelled by `command::line` and described by render.
  - The run stage offers what `RunFrame::actions` says, so a key, a palette row and the CLI's advice are one table.
  - `NextStep` gains the commands it lacks: `Stats`, `History`, `Graph`, `NewWorkflow`, `PackList` and `PackAudit`.
  - `Gc` gains its dry run.
  - Render's `command` says what each step does, beside its line.

### 5. The prompts, moved

What moves from `crates/cli/src/ask/` to `yunta_ui::prompt`, by name, with its tests, and what is renamed because the name is taken in a crate `yunta-ui` reads:

| At the base | In `yunta_ui::prompt` | Why |
|---|---|---|
| `Stroke` | `Stroke` | |
| `Press<T>` | `Press<T>` | |
| `Held` (its terminal-free half) | `Answerable` | `Held` is core's (`process::Held`). A prompt is answered stroke by stroke. |
| `Field`, `Typed`, `Edit` | `TypedLine`, `Typed`, `Edit` | `Field` is render's page and Surco's row. |
| `LineAsked` | `LineAsked` | |
| `Picker<T>`, `Choice<T>` | `Picker<T>`, `Alternative<T>` | `Choice` is core's (`step::Choice`). |
| `Round`, `Settled` | `Round`, `SettledAnswers` | `Settled` is core's (findings). |
| `DecisionCard`, `Answer`, `Reply` | `DecisionCard`, `PickedOption`, `Reply` | `Answer` is core's (questions) and Surco's (attention). |
| `PlanDesk`, `Asked`, `Leaving`, `Left` | the same names | |
| `decide`'s card-and-desk turns (`decision.rs`) | `Deciding` | The flow between a card and the plan behind it is pure. The CLI and the decision stage both hold it. |
| `Escape` | render's `prompt::Escape` | What Escape does is said, so its data stands where its words are. |

```rust
// crates/ui/src/prompt/answerable.rs
/// A prompt a person answers stroke by stroke, drawn as one block on any surface.
pub trait Answerable {
    type Answer;
    fn press(&mut self, stroke: Stroke) -> Press<Self::Answer>;
    /// Whether a stroke types: what a surface hands a character to.
    fn typing(&self) -> bool;
    /// The prompt as it stands, as the block render words and Surco draws.
    fn block(&self, look: &Look) -> Block;
    /// What a surface laid out of it, for a prompt that scrolls by what was shown.
    fn laid(&mut self, laid: PaneLaid) {}
}
```

- **The CLI keeps the terminal's half:** `Console`, the hold loop (`hold`, `Quiet`, `Place`), `keyboard`, `Strokes` and its bracketed paste, `NoAnswer`, `Answered`, and `decide`, `answer`, `choose` and `ask_line` as the calls that hold a prompt on a terminal.
- **The hold loop draws the prompt's block fitted to the terminal's rows.** It hands the desk what was laid out (`laid`) where the desk wrote a `Cell<Seen>` while drawing (`desk.rs:329`).
- **Each moved model gives up its words.** `NEEDS_WORDS`, `A_NOTE`, `SKIP`, `NOTHING_KEPT`, `FOLDED`, every legend and `Escape::does` move to render's `prompt`. `prompt` builds each prompt's Surco component from where it stands.

### 6. Every prompt is a block

- **`surco::doc::Block` gains four variants:**
  - `Card(Card)`;
  - `Desk(Desk)`;
  - the typed line;
  - the picker's list with its filter.

  Each wraps the component that exists today, and the plan's tasks name the last two after the components they wrap.
- **The terminal draws each block two ways.**
  - In a document, the block takes the room it needs.
  - Held, fitted to a height, it draws today's bytes, with the cursor where the typed row stands (`Placed`).

  `Card::drawn(look, rows)` and `Desk::drawn(look, rows)` become that held drawing.
- **`Markdown` and `Html` draw each block,** with goldens in every environment, beside the catalog's.

### 7. Every sentence is render's

| From | To | What |
|---|---|---|
| `crates/cli/src/surface/chronicle/` | `render::chronicle` | `Said`, `say`, `says_anything`, `line`, `evidence` and the seven words files, with their 26 tests. `kept` and `graduation`, what a terminal keeps above its region, stay with the CLI. `crate::output::blocks_lines` gives way to Surco's terminal. |
| `crates/cli/src/surface/view/` | `render::watched` | A moving run's rows: what needs a person, each working node with what its session said, the footer, the title. The CLI's region and the run stage draw the same rows. |
| the prompts' models; `init.rs`, `new.rs`; `WorkflowShape::does` | `render::prompt` | Every prompt's words and the block it draws. Also "project name", "base branch", "(enter keeps `…`)", "choose a shape", and what each shape does. The stray doc line on `WorkflowShape::label` goes. |
| new | `render::screen` | What each stage's screen says: its trail, its regions' titles, its legend, the first run's question, each confirmation, the palette's and help's headings. |

### 8. Assembled once, below the surfaces

- **Following a run is `yunta-api`'s.**
  - `Project::follow(&RunId) -> Following` owns what it reads: the log, the registry, the clock, the promotion chain.
  - `Following::next(&Stop)` hands back `RunNews`: events past the last seq, the engine as the registry reads it, what a session said, the successor a promotion handed it to, and the run's stop or stall.
  - `READ_EVERY`, `REGISTRY_EVERY` and `SUCCESSOR_WAIT` move with their reasons.
  - The CLI's `status --follow` draws the news on its surface and prints its closing block. The run stage does the same with the same news.
- **A run's page is read whole by `yunta-api`.**
  - Render's `Run` and `Node` pages own what they are drawn from, as `Verification` and doctor's findings already do.
  - `OpenedRun::page(depth, &Remarks)` and `OpenedRun::node_page(&NodeId)` return them, with the documents a decision shows, the plans at full depth, and the places. What the run's manifest holds that this binary does not read is said as a remark.
  - `OpenedRun::page_for_a_browser()` writes `page.html` in the run's folder and returns its path.
  - `status`, `open`, the closing block and the run stage take the same page. The CLI keeps spawning the system's opener.
- **The graph is render's.** `render::graph` writes a workflow as Mermaid and as DOT, each node labelled with a frame's word when there is one. `page::Graph` holds it as Surco's `Diagram`. The CLI's `graph` prints its text, and the graph stage shows its page.
- **What a stage keeps is owned and `Clone`.**
  - `FieldReading`, `StatsReading`, `WaitingDecision`, `WorkflowFile` and the other readings a stage keeps derive `Clone`.
  - `DetectedProject` and `FetchedPack` own a clone of their `Workspace` where they borrowed one. `Workspace` is `Clone`, and its parts are `Arc`s.

### 9. Setting a repository up declares its runners

```rust
// crates/api/src/setup/init.rs
pub struct ProjectSetup {
    pub name: String,
    pub base_branch: String,
    pub runners: RunnerDeclaration,
}

/// How `init` writes the runners.
pub enum RunnerDeclaration {
    /// Commented out, each proposed role on the first adapter that answered, its model
    /// left blank: what a non-interactive `init` writes.
    Proposed,
    /// As a person answered: each role on the adapter they picked, with the model they typed.
    Declared(BTreeMap<RunnerName, RunnerCandidate>),
}
```

- **Detection probes the adapters.** `DetectedProject` carries the probed adapters and the roles it proposes, so a person picks among them before anything is written. The roles are those the repository's workflows name, or `implementer` when they name none, with `defaults.runner` where a node names no runner.
- **A person answers each role.** They pick its adapter among those that answered, the first by default, and type its model, which has no default. A role left without a model stays proposed.
- **What writes and says it:**
  - `DetectedProject::write(ProjectSetup)` writes declared roles uncommented.
  - `Initialized` says what was declared, and ends on installing a pack when every role is declared.
  - `init -i` asks on the CLI's prompts, and the setup stage asks on its own.
- **A skeleton names the runner the config declares.** `yunta new` names it uncommented, as it names `lint` when the config declares it.

### 10. How it is proven

- **The walk.**
  - It runs from recorded readings of every stage: a scratch home with runs at every standing, a parked gate, a plan, a questions node, packs, and a sandbox repository never set up.
  - It applies every stroke and every outcome a pending ticket can have, and dedupes states by the screen each draws, in Markdown, and the tickets they wait on.
  - It holds that `update` never panics, that every request is one the performer carries out, and that every screen draws with `Html` and `Markdown`.
  - It also holds that every palette row is a step the stage takes, spelled as `command::line` spells it.
- **The sandboxes.** Each stage is driven through the `Performer` against a workspace built in the test, with the mock adapter and no process read: the cases the spec lists.
- **What prints.** Each move is guarded by:
  - every golden's digest;
  - `pages.sh`'s captures;
  - the CLI's integration tests, including the follow, init, new, review and resolve-gate tests and the console interaction tests;
  - the moved tests, by name.

  `init -i`'s new questions are the one change to what prints. Their tests are written first and fail first.
- **A real run, before and after.** Fragua, detached, followed with `yunta status --follow` to its first gate. The follower's output and the time between nodes are compared with the base's.

## Phases

Four phases, each one commit with the suite green: every step of the `ci` job, the suite in its groups. Each ends at a checkpoint the user reviews, then `cargo clean`.

Inside a phase, each group of work ends green, with what it prints compared against the base, before the next begins. A group does not stop for the user. The work stops inside a phase only on a decision that is the user's: a printed text that would change, a choice this plan does not make, or a failure whose cause is not found.

### Phase 1: the crate, and the moves that print the same
- **The base:**
  - the suite's binaries and tests by name;
  - every golden's digest;
  - `pages.sh`'s captures, adding `init -i`, `new -i`, `review` and `resolve-gate` at a pseudo-terminal;
  - a real run followed.

  `.plans/yunta-tui/proof/app-core/` holds them.
- **The crate:** `crates/ui` with its map and its manifest test. The CLI depends on it. The rows of `.agents/architecture.md` and of the map are updated.
- **The words to render:** `render::chronicle` and `render::watched`, with their tests. The CLI's region and lines draw through them.
- **Every prompt a block:** the four blocks, drawn in a document and held, by the terminal, `Markdown` and `Html`, with goldens. The card's and the desk's terminal tests pass unchanged.
- **The prompts to `yunta-ui`:**
  - the models under their names, `Answerable`, and `Deciding` from `decide`;
  - every prompt's words, `Escape` among them, to `render::prompt`;
  - the hold loop on `Answerable` and `laid`.

  The CLI's `ask/` keeps only the terminal's half.

**Checkpoint A:** every golden's digest, every capture and every test's outcome equal to the base's, with the moved tests counted by name; the manifest test holds.

### Phase 2: what every surface needs, once, and the runners
- **Following in `yunta-api`:** `Following`, `RunNews`, and `status --follow` on them, its tests unchanged.
- **A run's page read whole:**
  - render's `Run` and `Node` own their data;
  - `OpenedRun::page`, `node_page` and `page_for_a_browser`;
  - `status`, `open` and the closing block on them.
- **The graph in render:** `render::graph`, `page::Graph`, and the CLI's `graph` on them, its ten tests unchanged.
- **Steps and what a stage keeps:**
  - `NextStep`'s new steps, and what each step does in render's `command`;
  - the readings a stage keeps derive `Clone`;
  - `DetectedProject` and `FetchedPack` own their workspace.
- **The runners at setup:**
  - `ProjectSetup` and `RunnerDeclaration`, with detection probing;
  - `Initialized` saying what was declared;
  - `init -i` asking each role;
  - the skeleton naming a declared runner.

  The tests are written first and fail first.

**Checkpoint B:** the captures equal the base's but `init -i`'s, which the user reads; the follow, status, open and graph tests unchanged; a real run followed with the API's following beside the base's.

### Phase 3: the application, and watching, deciding and starting
- **The values:**
  - `State`, `Input`, `Request`, `Ticket`, `Operation`, `Outcome`, `News`;
  - the `Performer`, with what it holds and its stops;
  - `render::screen`'s first words;
  - the walk's harness and the sandboxes' harness.
- **Watching:**
  - the signpost;
  - the field, with its reach, resuming, and cancelling or closing one run or several after one confirmation;
  - the run stage, following through `Following`: its page, the chronicle, the moving rows, a node's page, the actions `RunFrame::actions` offers, a promotion followed on, and a run asked at another terminal shown read-only.
- **Deciding:**
  - a gate on its card and its plan at the desk, through `Deciding`;
  - a round of questions;
  - leaving it waiting.
- **Starting:**
  - the first run's question over the catalog;
  - the start stage, from the request to admission's cautions;
  - a run started detached and followed.

**Checkpoint C:** the walk over these stages; watching, deciding and starting end to end against the sandboxes.

### Phase 4: every other stage, the dialogs, and the proof
- **Setup:** detected, answered, runners declared, written, then a pack offered.
- **The catalog:**
  - a workflow's check, graph and history;
  - a new workflow by name and shape;
  - the cases, each shown as it ends;
  - the packs: list, audit, add and update with the executors confirmed on the copy audited, remove, scaffold.
- **Evidence, stats and the graph:** the receipt written, a run verified, its page shown in a browser; a run's stats and a workflow's history; a workflow's or a run's graph, a `parallel` group drawn as a box holding its nodes once Surco reads a `subgraph`.
- **Doctor, upkeep and schema:** the findings, folded or whole; the sessions after a confirmation, each remark shown as it comes and stoppable; the dry run and then the collection; every document's shape.
- **The dialogs:** the palette over every stage, filtered by typed words; help; every confirmation. The walk now covers every stage.
- **The proof and the review:**
  - the suite's count against the base's, by name;
  - every golden's digest and the captures;
  - the real run followed at the end;
  - the release binary's size by crate against the base's;
  - the module's diff read in four parts by readers of their own, each finding checked and decided.

**Checkpoint D:** the user reviews the proof and the review. Then the commit, the pull request's body, and CI on Linux and macOS.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| A prompt draws differently once it is a block | High | The card's, round's and desk's terminal tests and goldens pass unchanged. The held drawing is today's `drawn`. Captures of `init -i`, `new -i`, `review` and `resolve-gate` at a pseudo-terminal are compared byte for byte. |
| Splitting each prompt's words from its state changes a legend or a hint | High | The words move by string, and their tests move with them. The prompts' 66 terminal-free tests pass unchanged. |
| The walk's states explode | Medium | States are deduped by the screen they draw and the tickets they wait on. Readings are a fixed scratch home, and a stroke that changes nothing visible is one state. |
| The follower's move changes its timing | Medium | The follower's constants and loop move as they are. Its tests in `status_follow`, `run_words` and the guarding flow pass unchanged. The real run's follower output is compared with the base's. |
| Owned pages cost a clone per page | Low | A frame and a state are cloned once per page read, the same order of work as drawing them. The real run's time between nodes is compared. |
| `.agents/architecture.md` is at its cap | Low | The new row's bytes come out of what the file says twice or a `//!` already says. Without such a sentence, the work stops for the user. |
| The binary grows past its ceiling | Medium | The release size by crate is measured at each checkpoint (`tools/size_by_crate.py`). Each document is read through core's one reader, as `control` left it. |
| A held fetched pack or detected project outlives its stage | Low | The performer drops it at `LetGo`. A test leaves a pack's audit unconfirmed and finds the temporary checkout gone. |
| The disk fills during gates | Low | `CARGO_INCREMENTAL=0`, one build at a time, `cargo clean` after each phase. |

## Decisions This Plan Takes

The spec leaves these to the plan. Each stands unless the review of this plan changes it:

- `Request`, `Ticket`, `Operation`, `Outcome`, `News`, `Performer`, `State`, `Input`, `Dialog`, and the stage types with their `Stage` suffix.
- Every act is a `NextStep`.
- The four prompt blocks.
- The prompts' words in render's `prompt`.
- `Deciding`.
- Render's `Run` and `Node` pages own their data, and the API reads them whole.
- `Following` and `RunNews`.
- A pack is installed from the copy its audit read.
- The renames of section 5.

## Open Questions

None. The names in sections 3 to 9 are the plan's proposal for the user's review.
