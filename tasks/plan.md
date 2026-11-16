# Implementation Plan: `live-screens`

Spec: `SPEC-live-screens.md`, approved on 2026-11-14. Base: `feature/live-screens` at `4315c1da`, where `tui` ended on `main`. `cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.

## Overview

`tui` left an application whose frame is the design's and whose content is not: each stage shows the page a CLI command prints, read once, and following a run ends where the run stops.

This plan makes what the application shows a function of what it follows, in four steps:

1. **The ground.** One reader of the log, one fold, one follower in `yunta-api` that hands facts and never ends by itself, with `yunta status --follow` and the application's present following on it. The ledgers learn what a task proves. The design is written down as targets and approved before any code draws it.
2. **The application follows.** `yunta-ui` keeps what it follows in its state, and the screens it has today move by themselves, drawn as they are drawn today: the field, a run's screen, its stats and its graph. Beside it, what a node says in print gains the parts its pane will show.
3. **A run's screen.** The scene of the node pointed at, with its feed, and every decision answered in place.
4. **The field, a new run, a setup,** one screen each, and the proof.

Each step leaves the application working. The path a step replaces is deleted in that step, and a screen's golden changes once, to its approved target.

The design is reached whole: every component it draws and every phrase it says. Six surveys, one for each family of screens (`.plans/yunta-tui/proof/live-screens/sweeps/gap-*.md`), sized the gap at the base; the inventory then counted it: 53 of the design's 66 components and 202 of its 297 phrases differ from today's or have no counterpart, and today's screens say 149 things the design does not. An inventory of them all, screen by screen, is what the targets and the goldens are held to.

## Architecture Decisions

### 1. What each layer holds, after this module

| Layer | Gains | Loses |
|---|---|---|
| `yunta-storage` | `LogReader`: the log through one connection kept open, which refuses to write and says whether another connection committed. | |
| `yunta-replay` | The one incremental fold: `RunState::fold`, `RunState::chronicled`, `RunState::head`. `replay` and `chronicle` are that fold over a slice. | `apply` as a public way in. |
| `yunta-core` | The ledgers' new facts (section 3). One optional field on `agent_message`, the call's session. | |
| `yunta-engine` | The writer of calls stamps the session. Its state is derived by the same fold. Nothing else. | |
| `yunta-api` | Module `follow`: `Followed`, `Following`, `FollowNews`, `RunNews`. `Awaited` beside the two it sums. The field's rows made from states, for the follower and for `yunta list --runs` alike. | `run::follow`, its `Stopped` and `Stalled`. |
| `yunta-render` | Every phrase of the design, each owned once. A pane's parts as values in `blocks`. A feed's rows in `chronicle`. `watched::spent`. The ribbon's names. `page::Node` composed of the same parts; a run's stats and its graph as the design draws them. | `watched::footer`. Each sentence the design says another way. |
| `yunta-surco` | `Feed`, and each piece the design draws, as a component or a principle with its goldens and its place in the catalog. The design's shape for each component it draws another way: the card, the round, a list of choices, the typed line, the palette's and the help's boxes, a confirmation's box, the desk's rail. | The shapes those had. |
| `yunta-ui` | What it follows as a function of its state, and what it keeps of it. `Scene`. Who leads a run's screen. Decisions in place. The field, a new run and a setup as one screen each. | `Stage::Decision`. Every reading the follower replaces. `Outcome::Followed`. |
| `yunta-tui` | The design's sizes. Surco's new pieces drawn. The window's title, and the bell. | The side region as one shape. |
| `yunta-adapters` | The mock's step that waits for a word. | |
| `yunta-testkit`, `yunta-testkit-ui` | `Hold` gives each word a test names. The design's run, recorded and kept. What is followed, recorded for the walk. | Recordings that wait for a following to end. |
| `yunta` | `status --follow` on the follower. `review` opening the run's screen. The performer handed `Channel::Tty`. | |

No crate is added and no edge between the product's crates changes; the test crates' edges are measured in the task list. `.agents/architecture.md` (6,135 of 6,144 bytes) carries the follower's rule by rewording what it says of following today.

### 2. One reader, one fold, one follower

```rust
// yunta-storage, crates/storage/src/log_reader.rs
pub struct LogReader { /* one connection, kept; its path */ }
impl LogReader {
    pub async fn open(path: impl Into<PathBuf>) -> Result<LogReader>; // as Storage::open, then query_only
    pub async fn moved(&self) -> Result<bool>;                        // PRAGMA data_version
    pub async fn heads(&self) -> Result<Vec<RunHead>>;
    pub async fn events_after(&self, run: &RunId, seq: Seq) -> Result<Vec<StoredEvent>>;
    pub async fn events(&self, run: &RunId) -> Result<Vec<StoredEvent>>;
}

// yunta-replay
impl RunState {
    pub fn fold(&mut self, event: &StoredEvent);              // nothing past a break, nothing for a seq already folded
    pub fn chronicled(&mut self, event: &StoredEvent) -> Moment;
    pub fn head(&self) -> Option<Seq>;
}

// yunta-api, crates/api/src/follow/
pub struct Followed { pub field: Option<FieldReach>, pub run: Option<RunId> }
impl Project { pub async fn follow(&self, followed: Followed) -> Result<Following, Error>; }
impl Following {
    pub fn refollow(&mut self, followed: Followed);
    pub async fn next(&mut self, stop: &Stop) -> Result<FollowNews, Error>;
    pub fn awaits_successor(&self) -> bool;
}
```

- **A look** is the follower's unit of work, at the paces the spec states. Each call is one statement on a blocking thread, and no transaction outlives a call.
- **What it hands.**
  - The field as first read, or read again at another reach, arrives as one piece of news holding every run of it, so an interface knows it holds the whole field before it decides where it stands.
  - A run of the field is handed with its state at its head, its manifest, its engine, and whether it offers a menu, worked out once for each head.
  - The run read whole is handed every event from its first, once, and then each batch. Whoever follows folds them with `chronicled`: `status --follow` to print its lines, the application to keep its moments. One fold, the same events, so both agree with the follower's own state.
  - Each document a node of the run read whole hands over arrives once, when the event that hands it over is folded. What a manifest holds that this binary does not read is remarked once for each run read.
- **Errors stay with their run.** A run whose log fails a read is handed as unreadable and read again whole when its head moves.
- **What a run asks** is keyed by its pause: the node `awaits_decision` names with the pause's seq, or a round's node with the seq of its questions. It is read once for each key, on a task of its own, with the state the follower holds.
- **Words** reach the run read whole through its socket, as today, now with how many a slow reader missed.
- **`status --follow`** follows `Followed { field: None, run }` and stops where `!frame.moves() && !following.awaits_successor()`. Its seed lines come from the run's events as the follower hands them, its closing places from `Project::run`, its registry from `OpenedRun::registry`, its words and its successor from `RunNews`. It prints the block and exits with the code it does today.
- **`chronicled` clones only the record of the node its event names,** where the chronicle clones every node's for each event today: a run read whole costs its events, not their square.

**From Phase 1 on there is one follower.** `run::follow` goes in the phase that brings `follow`. The application's present `Operation::Follow` is performed over the new follower until Phase 2 removes the operation: the performer follows the run alone and ends the operation where `status --follow` stops, so the application behaves at Checkpoint A as it does at the base. Its run stage folds what it is handed with `chronicled`, which Phase 2 keeps.

Rejected:
- A push from the engine: a second channel every engine has to feed, and a reader that missed a push is wrong until the next.
- A file watcher: a dependency for each platform, for what one pragma answers.
- A field read again whole on a timer: the base's cost, every second.
- Two readers, one for the field and one for the run: they fold one run twice and show two instants of it on one screen.
- The old follower kept under other names beside the new one for a phase: two followers in one tree, one of them doomed.
- A connection opened read-only: it cannot set a fresh or older log up, so the first open stays `Storage::open`'s.

### 3. What the log says of a task

- **The tasks' ledger** folds `task_registered`'s criteria and scope, and keeps for each task its latest check after its phase, apart from a session's own check.
- **The node's ledger** keeps each session's task, the paths a session's write and edit calls name, and its node-level scope checks.
- **The run** keeps the union of every scope check's paths, which the receipt prints.
- **`agent_message` gains `session`,** optional; a log without it reads as today. The engine's writer of calls stamps it. `cargo xtask schema` regenerates `crates/core/schemas/`.
- **The receipt** reads these and prints the same, held by its goldens and by the receipts of every log the suite keeps, compared before and after.
- **The run path changes in two places,** both in Phase 1: the session stamped on each call, and the engine's state derived by the rewritten fold. Real fragua runs before and after prove neither costs time between nodes.

Rejected: the follower reading the tasks document for a task's proof and scope (two sources for one fact, and the receipt keeps a third reading of raw events).

### 4. What the application follows is a function of its state

```rust
// yunta-ui
impl State { pub fn followed(&self) -> Followed; }
pub enum Request { Perform(Ticket, Operation), LetGo(Ticket), Follow(Followed), CallThePerson, ShowPage(PathBuf), Leave }
pub enum Input { /* … */ FollowNews(FollowNews) }

/// What the state keeps of what it follows, folded from the news. Not `yunta-api`'s
/// `FollowedRun`, one run as the follower hands it.
pub(crate) struct FollowedRuns { field: Option<FieldInView>, in_view: Option<Arc<RunInView>> }
pub(crate) struct RunInView { run: FollowedRun, moments: Vec<Moment>, by_node: BTreeMap<NodeId, Vec<usize>>,
                              handed_over: BTreeMap<NodeId, Vec<Document>>, heard: Heard, awaited: Option<Awaited> }
```

- `State::update` ends by comparing `followed()` with what it last asked, and asks `Request::Follow` when it differs. No ticket, no let-go: the generic rules for work never touch a following.
- The performer keeps one `Following` and calls `refollow`. It is made with its surface's `Channel`.
- Every stage reads `FollowedRuns`: `screen`, `pressed` and `offered` take it. News fold into the data, so the run under a stage over it stays current.
- What is kept sits behind `Arc`, so the walk's clone of a state for each input stays cheap.
- Frames are made at the interface's clock from the kept states, at each draw. What does not depend on the time (whether a run offers a menu) is kept by head, never worked out at a draw.
- **Where the application first stands** (the first request or the field) is decided on the field's first news. The first request's screen stands down only when the person leaves it.
- **A promoted run's successor** becomes the run in view: `followed()` names it.
- **The project's prices,** which a run's stats read, come with the repository's reading as the application opens.

Rejected:
- Lasting operations under tickets, asked and let go by each stage: the bookkeeping that already failed twice, and news that reach only the top stage.
- The data kept in the run's stage: a stage over the run would hear nothing.

**Through Phase 2 the run's screen keeps its page.** The page's headline, furrow and gist are drawn from the kept frame and state at each draw, and the page is read again when the kept run's phase, its pause or its successor changes; the acts it offers read what the run waits on from the run in view. `Stage::Decision` stays as it is. Phase 3 deletes the page, the stage and these two rules with them. The run's `f` goes with the manual following in Phase 2, and returns in Phase 3 as the key that gives the lead back.

### 5. A run's screen

- **`Scene`** (`crates/ui/src/stage/run/scene.rs`) is made by one function of the run in view, what it waits on, the node pointed at and whether the run leads. The module denies wildcard arms. It takes the answers of what already decides part of it (`RunFrame::actions`, `awaits_decision`, `PauseReason`'s own questions, `state::gist`, `desk::plan_among`) and decides only what stands where. Which card stands comes from the escalation the engine rebuilds, never from the log's `Menu`.
- **Who leads** is derived: two `PointedBy` (the node pointer, the finding pointer) and the pane's `ScrolledTo`. Nothing stores "the run leads", so nothing can fall out of step with them.
- **The feed.** `render::chronicle` says which moments a node's feed lists and each row's words. Surco's `Feed` carries the rows in sight with how many stand above and under. The stage hands render only the rows around the place, sized by the last `Laid`, so a draw costs the rows in sight. A region holding a feed is laid out as its feed.
- **A pane's parts** are values in `render::blocks`, each handing its blocks as `FailureDetail` does: what a node hands over, a task's proof, its scope with what was written, the findings and the one pointed at, a standstill's facts. `yunta-ui` composes a pane from them and holds no sentence. `page::Node` composes the same parts, so `yunta status <run> <node>` says what the pane says.
- **Decisions in place.** The prompt models (`Deciding`, `Round`, the desk) stay in `yunta_ui::prompt`, where the CLI reads them too. The run's stage holds the prompt of its scene, keyed by the pause, so news of the same pause never rebuild it. `Stage::opened_by` maps `ResolveGate` and `Review` to the run's stage, opened on the card or the plan.

Rejected:
- The scene in `yunta-replay` (it would name prompts and documents) or in `yunta-render` (it would decide where a person looks).
- A three-valued "who points": the third value, the run with its pane held, is the pane's place said twice.
- The pane scrolled as one region: a node at its two-hundredth call would push what it hands over and says out of sight.
- A feed laid out whole and sliced: the cost of every row at every draw.

### 6. Surco's pieces, each with the first thing that uses it

| Piece | Enters with |
|---|---|
| `Feed`; the meter row | what a node says in print, and the attached run's region (Phase 2) |
| the note; the legend's far keys; a region title's far words; the attention row's cases; a side region at either edge, giving way below 102 columns; the selection fill of a pointed row | a run's screen (Phase 3) |
| the card with its options; the round; a list of choices; a line typed on a card; the desk's rail; a finding's row; a checklist's row; the palette's box with its three columns; the help's box; the bars of where time went and of who spent it; the graph | a run's screen and what stands over it (Phase 3) |
| a boxed line to type in; a row of words with one chosen and marked; the name in block letters and the field in perspective; a confirmation's box; the glance's rows | the field, the first request, a new run, a setup (Phase 4) |

Each has goldens in ASCII and Unicode on its plain and 16-color layers, and its place in Surco's catalog. `yunta-tui` draws them through Surco and composes no glyph.

**One language.** A component the design draws another way is redrawn in Surco, once, and a phrase the design says another way is reworded in render, once. So the CLI's inline prompts and its printed pages change with them, wherever they draw that component or say that thing: no second card, no second sentence for one fact. Each phase blesses the goldens it changes (`YUNTA_BLESS=1`), and their diff is reviewed at its checkpoint with the screens; `docs/` and Surco's reference follow in the same phase. How the CLI's prompts behave does not change.

**A screen takes the design's frame with its own target.** A stage composes its screen's tally, attention row and legend, so each stage adopts them in the phase its target lands: a run's screens and what stands over them in Phase 3; the field, the first request, a new run, a setup and every page kept as it is in Phase 4. No golden passes through a frame nobody approved.

### 7. The field, a new run, a setup

- **The field** draws its rows from the kept states, grouped by render, with the glance of the run pointed at on the right.
- **A new run** is one stage holding every row (request, the workflow's other inputs, workflow, mode, adapter), with what the workflow usually takes read once from its history.
- **A setup** is one stage whose rows are changed in place. Writing asks what `init -i` asks of the API, then installs the pack through `Workspace::packs()`, its audit shown and confirmed first.
- **The first request** stays one stage, drawn as the design draws it.

### 8. The test world

- **The design's run, recorded and kept.** One workflow shaped as the design's (its node ids and kinds, a loop with a task, three reviewers at work at once, two gates) is played on the mock through a round, a plan's gate, a scope asked, findings answered, a failed check retried and the last gate. It is recorded so that two recordings give the same log:
  - three nodes may work at once, and each session is held by a word and let go in a fixed order;
  - the clock advances a fixed step at each reading, so times have the widths real ones have;
  - each pause is a park, an answer and a wake, never an answer at the engine's own terminal.

  Its log, its run directory and its objects are kept under `crates/testkit-ui/fixtures/`, with a tool that records them again and a test that the engine still writes that log.
- **A small run for each scene that run does not pass through** (a cap, a departure, a run that wakes alone, a pull request, a child, no room, orphans, a stop, a stall, a run asked elsewhere, a failure, a promotion, a broken log), and three finished runs of one workflow for what it usually takes. Each is built as the engine's own tests build that state.
- **A scene is the follower reading a store.** A test loads a log cut at its moment into a fresh store with its run directory, writes the registry the scene needs, and follows it. No constructor of news stands beside the follower.
- **The walk** is handed what is followed for each place, to every state whose `followed()` holds it. The fold takes an event once, so news handed again change nothing and the walk stays bounded. Its fingerprint includes `followed()`.
- **The helpers that drive a state** go on taking what is followed until the kept heads equal the log's before the next stroke, since a following has no ticket to wait on.
- **A session held by a word.** `MockStep::AwaitsWord` and `Hold`'s named words let a test let calls go one at a time, and assert an order, never a duration.
- **Pty tests** drive the binary while another process moves the run. They are few: the screens' content is held by goldens and by `yunta-ui`'s tests.

### 9. The targets

- **The inventory comes first.** Every component and every phrase of the design, screen by screen: the frame and rows that draw it, what stands for it today with its `path:line`, and the target that will carry it. It starts from the six surveys and is completed and checked on Opus, frame by frame, at the three recorded sizes and against the spike's source, since the surveys left rows unmatched and read only the wide frames. It also lists what each screen says today and the design does not, with where each goes: said the design's way, or left to the printed page.
- Each target is written by hand as text, from its recorded frame where one exists, with the facts of the run it shows (read from a dump of that run's frame at its moment), the design's own words, and every correction the spec lists. A screen with no frame is designed there, in the design's components.
- Four sizes each: 120×36 and 80×24 in Unicode, 120×36 in ASCII, 60×14. About 60 screens with the palette, the help, a run's numbers and graph, a confirmation and the pages the design draws no frame for, so some 240 frames.
- A checker, not a generator, holds what a hand slips on: each frame's width and rows; the trail, rule and legend rows in their places; every row of the inventory present in its target, word for word; no number cut.
- One page shows each target beside its frame, with each difference tagged by the line of "Where the design is not followed" that explains it, the 60×14 and ASCII frames folded under their screen, and the list of what leaves each screen. The 120×36 frame is the one read against the design; the other three are held by the checker and read where a screen gives way. The user approves it at Checkpoint A.
- The goldens the code draws equal the targets. A target changes only with the user's approval.
- `tui`'s fifteen frames stay as they are until the phase whose golden replaces each: the run, the card, the desk, the palette and the help in Phase 3; the field and the confirmation in Phase 4. Through Phase 2 they hold that the screens moved onto followed data draw what they drew.

### 10. Delivery

One commit a phase, each with every step of the `ci` job green, then `cargo clean`. The spec and the map's row go in Phase 1's commit. A draft pull request is opened with it, since the `ci` job runs only on a pull request; the user decides when it leaves draft.

## The dependency graph

```
LogReader ──────────────┐
RunState::fold ─────────┼──► follow (yunta-api) ──► status --follow, the application's present following
ledgers, call's session ┘          │
        │                          ▼
mock's word, Hold        FollowedRuns in State ──► the present screens, live
        │                          │
        ▼                          ▼
the design's run ──► targets   Scene, lead, feed ──► decisions in place ──► field, new run, setup
 and the small runs  (approved    ▲
                      at A)       │
render's parts, Feed, meter ──────┘──► page::Node
Surco's other pieces ─────────────┘
```

## Phases

Four phases. Inside a phase each group ends green before the next begins, with what it prints compared against the base, and does not stop for the user. The work stops inside a phase only on a decision that is the user's: a printed text that would change beyond what the spec names, a choice this plan does not make, a cap that would rise, a target that cannot be met, or a failure whose cause is not found.

### Phase 1: the ground, and the design written down

- **The base:** the suite's binaries and tests by name; every golden's digest; `pages.sh`'s captures; the receipt of every log the suite keeps; the release size by crate; three real fragua runs with their time between nodes; the application's processor time open for a minute on the machine's field; a drawing's time at 200×60.
- **The reader:** `LogReader`, with its tests on real logs: it refuses a write, `moved` is false until another connection commits, a first open sets a fresh log up.
- **The fold:** `fold`, `chronicled`, `head`; `replay` and `chronicle` on them; the property that a log folded in any chunks equals its replay, a broken log included; the time to read whole a run with a node of ten thousand calls.
- **What a task proves:** the ledgers' facts; the call's session with its schema and the engine's writer; the receipt on them, printing the same.
- **A session held by a word:** the mock's step and `Hold`'s words.
- **The follower:** module `follow` in place of `run::follow`, with each of the spec's tests on real logs; `status --follow` on it, its captures equal; the application's `Operation::Follow` performed over it, its tests and its pty tests green as they stand.
- **The runs the targets show:** the design's run recorded and kept, the small runs and the three past runs, each with the dump of its facts at each moment.
- **The inventory:** every component and phrase of the design, completed and checked from the six surveys, with what leaves each screen.
- **The targets:** written at four sizes with the design's words, checked against the inventory, and laid out on their page.
- **Real runs** after, against the base's three.

**Checkpoint A:** the user approves the targets on their page. Every golden, capture, receipt and test equals the base, but the schema's one field. The follower agrees with `yunta status` on every log the suite holds. Time between nodes inside the base's spread.

### Phase 2: the application follows

- **`yunta-ui` on the follower:** `followed()`, `Request::Follow`, `Input::FollowNews`, `FollowedRuns`; the performer's one following and its channel; where the application first stands.
- **The field** from the kept states, its rows moving to their groups, its pointer on its run.
- **A run's screen as it is drawn today,** from the run in view, through every stop and onto a successor; stats and the graph computed from it.
- **The title and the bell** when a run comes to need the person.
- **Gone:** `Operation::Follow` with the readings of the field, stats and the graph; `Outcome::Followed`; the run's `f`.
- **The test world:** what is followed recorded for the walk; the walk fed, its states counted before and after; the helpers that drive a state.
- **What a node says in print,** beside the groups above and touching none of their files: render's parts in the design's words, `Feed` and the meter row with their goldens; `page::Node` composed of them; the attached run's region on `watched::spent`.
- **At a pseudo-terminal:** a run started, answered, cancelled and finished elsewhere moves the field with no key; a killed engine shows as a stall; a run parked then resumed elsewhere is read on.
- **Measured:** the application's processor time at rest; real runs with the application open on the field and without.

**Checkpoint B:** the application moves by itself, tried by the user on their own field. `tui`'s fifteen frames are equal. The spec's criteria 1 and 4 hold, and 2 as far as today's screen shows it. The walk is bounded and its time is known. Runs keep their time between nodes with the application open. What the node's page and the attached run's meter now print is approved.

### Phase 3: a run's screen

- **Surco's pieces** of the run's screen, each against its goldens, and the components the design redraws: the card, the round, a list of choices, the desk's rail, the palette's and the help's boxes. The CLI's prompts draw them too, their goldens blessed and their diff reviewed.
- **Render's phrases** of a run's screen, each row of the inventory: a card's question and each option with what it does, the sentence for an answer recorded, the legends, the checklist, the findings.
- **The scene and the lead:** `Scene`, the pointers, `ScrolledTo`, with a test for every `Standing` and no wildcard arm.
- **What a node does:** the work, board and findings scenes and a node's own, each with its feed; the ribbon's names; the meter; the sizes in `yunta-tui`.
- **Where a run stands:** standstill, a run that wakes alone, the pull request, a child, asked elsewhere, the receipt, the ends; the run's whole chronicle from the palette.
- **Over a run:** its numbers and its graph as the design draws them; the palette and the help.
- **Decisions in place:** the card with what it rests on, the round, the desk; a prompt put aside and brought back; an answer from elsewhere. `Stage::Decision` and the run's page go, and `review` opens the run's screen.
- **A run's keys** as the spec's table says, with its tally, its attention row and `a` going where it names.
- **What a golden cannot hold:** with no event, two drawings a second apart differ only in times; each behaviour the spec lists for `yunta-ui`'s tests.
- **At a pseudo-terminal:** a held session's calls arrive one by one at the pane's last row; scrolled back they stay; a card appears by itself and is answered there; a run answered from a shell goes on.
- **The walk** over every scene; each scene's golden equal to its target at four sizes, the frames it replaces deleted.

**Checkpoint C:** the spec's criteria 2, 3, 5, 6 for a run's scenes, 7, 8 and 11. No row of the inventory differs on a run's screens. The user tries a real run in the application, and reviews what the CLI's prompts and pages now print.

### Phase 4: the field, a new run, a setup, and the proof

- **Surco's pieces** of these screens, and **render's phrases** of them, each row of the inventory; what the CLI prints of the same things, blessed and reviewed.
- **The field** as the design draws it: its first row, the glance, marking and closing several, the closed, the machine's reach on `e`, `a` going where the attention row names.
- **The first request, a new run, a setup,** each one screen; the setup installing its pack once confirmed; the workflows' `enter` opening the new run's screen.
- **Every page the design draws no frame for** (the machine, upkeep, the workflows, packs, a document's shape, a workflow's history, a run's evidence), as its target designs it: its facts, in the design's frame and components.
- **Every key** as the spec's table says, with help and the palette listing those out of the legend.
- **The docs:** `README.md`, `docs/guide.md`, `.agents/architecture.md`, the map.
- **The proof:** every golden equal to its target; three real fragua runs followed and answered in the application, against the base's; a drawing with a feed of ten thousand rows; the processor at rest; the release size; a review of the module, its findings fixed red first.

**Checkpoint D:** the spec's fifteen criteria, each with its evidence. No row of the inventory differs anywhere. The phase commits, the pull request's body says the module, CI is green on Linux and macOS.

## What runs in parallel

- In Phase 1 the reader, the fold, the ledgers and the mock's step touch four crates and no shared file; the follower waits for the first three. The runs the targets show wait for the ledgers and the mock's step; the targets wait for their dumps and need no code.
- In Phase 2 what a node says in print touches render, Surco and the CLI's node page, and none of the files `yunta-ui`'s move touches.
- In Phase 3 Surco's pieces are independent of the scene's function. The scenes land family by family, each with its goldens.
- In Phase 4 the three screens are three stages with no shared file beyond the keys' table.
- Sweeps and measurements run on Haiku and report; units of code and reviews run on Opus; the lead writes each unit's instructions, decides every finding and runs every gate.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| A target written before its code cannot be met by the data | High | Every run a target shows is recorded in the same phase, and the target is written from its dump. The checker holds each frame's shape. A target that cannot be met stops for the user, never changes silently. |
| Some 240 frames by hand are slow to write and heavy to approve | High | Written family by family in parallel, each from its recorded frame. One page grouped by screen, each difference tagged with its reason, the small sizes folded under their screen and held by the checker. |
| A component or a phrase of the design is missed again | High | The inventory, completed on Opus from the surveys at every recorded size, is what the checker holds each target to, and each checkpoint counts its rows that still differ. |
| The design's phrases change what the CLI prints across many pages and prompts | High | One language is the rule, so the change is meant. Each phase blesses only the goldens of the screens it lands, their diff reviewed at its checkpoint; the captures of every command are compared before and after, and each difference is one the inventory names. |
| A phrase the spike wrote for its demo says something the engine does not do | Medium | The inventory marks each phrase as the design's own or as made up for the demo; a made-up one takes the log's fact in the design's form, shown on the targets' page. |
| The recorded run stops matching what the engine writes | Medium | A test records it again and compares. When the engine's events change on purpose, the tool records it again and the goldens' diff is reviewed. |
| A kept connection holds the engine's checkpoint back, or `data_version` misses a commit | High | No transaction outlives a call. Tests on real logs with a second connection writing. Time between nodes measured with the application open, three runs a side. |
| The walk grows without bound once states are fed news | High | The fold takes an event once; the fingerprint includes `followed()`; the count and the time are measured as Phase 2's first group, before any screen moves onto the data. |
| Phase 1 moves the application's following onto the new follower and changes how it behaves | Medium | Its tests and its pty tests pass as they stand, and the operation ends by the rule `status --follow` stops at. |
| The call's new field breaks a reader of an older or newer log | Medium | The field is optional. The replay property runs over logs with and without it. The schema is regenerated and its diff reviewed. |
| The receipt prints differently from the ledgers | Medium | Its goldens, and the receipt of every log the suite keeps, compared before and after. |
| `status --follow` changes what it prints or where it stops | Medium | Its captures, its own tests, and `followers.py` on real runs, before and after. |
| A draw each second costs too much on a field of many waiting runs | Medium | What does not depend on the time is kept by head. The processor at rest is measured on the user's own field at Checkpoint B. |
| A run with tens of thousands of moments, or many parked runs, weighs on memory or on its first reading | Medium | States only for the field; moments for the one run in view; a draw takes a window; `chronicled` clones one node's record. The reading's time is measured in Phase 1. |
| Phase 3 is the largest phase | Medium | Render's parts and the node's page land a phase earlier. Its groups each end green, and scenes land family by family against targets approved two phases earlier. |
| The suite's time passes CI's 45 minutes | Medium | Each phase's gate reads the walks' and the goldens' time; the walk already runs on every core. |
| A pty test passes alone and fails under load | Medium | Each asserts an order, with a session held by a word. Each new one runs under `stress_binary.py` before its phase's commit. |
| The setup's pack needs the network in a test | Low | The tests install from a local repository, as the packs' tests do. |
| `.agents/architecture.md` is at its cap | Low | The follower's rule rewords the row that speaks of following. A cap that would rise stops for the user. |
| The disk fills during gates | Low | `CARGO_INCREMENTAL=0`, one build at a time, `cargo clean` after each phase. |

## Decisions This Plan Takes

The spec leaves these to the plan. Each stands unless the review of this plan changes it:

- Four phases: the ground and the targets; the application follows; a run's screen; the field, a new run, a setup and the proof.
- Every target is approved at Checkpoint A, on one page, before any code that draws.
- One follower from Phase 1 on: the application's present following is performed over it for one phase, and no second follower stands beside it.
- Through Phase 2 a run's screen keeps its page, drawn from the kept frame and read again when the run's phase, pause or successor changes, and `Stage::Decision` stays. Phase 3 deletes both.
- A screen takes the design's frame with its own target, so no golden passes through a frame nobody approved.
- The follower hands the run read whole its events, and whoever follows folds them; the application, not the follower, keeps the moments.
- The design's run is recorded once, kept in the repository, and held to what the engine writes by a test. A scene is the follower reading a store that holds the log cut at its moment.
- The session on each call, the ledgers' facts and the fold land in Phase 1, so the run path changes in one phase and is proven there.
- Render's parts, `Feed`, the meter and the node's printed page land in Phase 2, beside the application's move.
- `FollowedRuns` and `RunInView` hold their data behind `Arc`. `Request::CallThePerson` carries no payload: the screen says the title.
- A checker of the targets' shape, and no generator of them.
- An inventory of every component and phrase of the design is the checklist of the module: built in Phase 1, counted at each checkpoint.
- One language: the design's components replace Surco's and its phrases replace render's for every interface, so the CLI's prompts and pages change with the application's screens, phase by phase.
- The CLI's `surface::feed::Feed`, a queue of beats, is renamed `Beats` in module `surface::beats`.
- A draft pull request opens with Phase 1's commit.

## Open Questions

None. The names in sections 2 and 4 and the decisions above are the plan's proposal for the user's review.
