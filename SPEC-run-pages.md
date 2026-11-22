# Spec: `run-pages`

Module of `CAPABILITY-MAP.md`: "The pages in `yunta-render`, with the CLI printing exactly the same". Status: **approved on 2026-11-07, amended on 2026-11-08** at the user's request: the frame measures every time a page says, and what a run's registry says is one enum, `Engine`, read once.
Base: `feature/yunta-tui` at `13cdd69c`. Depends on: `run-state`. Needed by: `control` (whose data the pages draw) and `app-core` (whose screens hold them).

## Objective

Every page Yunta shows a person is a document. Most of the run's documents are built inside the CLI: the status page, the block a run leaves, the field, the stats, and the answer every command gives. Most already take data and return a Surco `Doc`, but they compute paths, name engine types and lean on the CLI's own modules for their words and their commands.

The documents `yunta-render` already holds take a second form: free functions that are handed the spelling of commands as a closure. So the terminal application and the desktop cannot show what the CLI shows without the CLI, and a person who reads one page in two places reads two pieces of code.

This module gives every page one home and one form:
- every page Yunta shows is a value in `yunta-render`, built from data its caller read, and drawn into a `Doc` by the one trait every page answers to;
- what a person can do next for a run is data in `yunta-replay`: the actions a run offers, which the CLI spells, a terminal binds to keys and a desktop shows as buttons;
- every command a person is told to type is a value in `yunta-core` and is spelled once, in `yunta-render`.

The CLI keeps what depends on its process: reading, the exit code, the streams, the JSON documents and the live region. Every golden is byte for byte the same, and the only texts that change are four where a better form of a command replaces a worse one, with the documentation following them.

### What happens at the base commit (`13cdd69c`)

Measured on 2026-11-07.

**The run's pages, built in the CLI:**

| Page | Files (code / tests, lines) | Built from | Built by |
|---|---|---|---|
| Run page: `status`, the closing block, `open` | `run_page/{mod,facts,story,evidence}.rs` (1,109 / 1,164) | `RunPage { scope, depth, run_id, engine, frame, state, paths: Outline, decision: Option<Decided>, plans }`, then `document(look)` | `commands/status/mod.rs`, `surface/closing/mod.rs` |
| One node: `status --node` | `commands/status/node.rs` (172) | `NodePage { run_id, node, run_dir, cwd, home, documents, state }` | `commands/status/mod.rs` |
| The decision a page ends with | `commands/status/decision.rs` (435) | the escalation and what it shows | the run page, `resolve-gate`, `review` |
| The progress line | `commands/status/progress.rs` (116), `render/counter.rs` (274) | the frame | status, the field, the live region |
| The field: `list --runs`, bare `yunta` | `commands/list/runs.rs` (294 / 245) over `commands/list/inbox.rs` (499) | `Inbox`, whose rows hold their `reason`, `command` and `said` already worded | `list_runs` |
| A run's stats, a workflow's history | `commands/stats/{run,workflow,facts,time}.rs` (1,138 / 496) | `RunStats`, `RunSummary`, `PriorEstimation`, `VerificationFindings` | `commands/stats/mod.rs` |
| The doctor's report | `commands/doctor/mod.rs` (359, its checks beside it) | `verify`'s `CheckRow`s, a copy of Surco's `Check` that both commands print and serialize | `doctor` |

**Every other command's answer, built beside the command that does its work** (3,606 lines, command and page together):
- the answers of `cancel` (`cancel/page.rs`), `close`, `verify`, `receipt`, `resolve-gate` and `open`;
- `gc`'s pass (`gc/page.rs`);
- the workflow catalog of `list`;
- `init`, `new`, `schema`;
- `pack`'s install and audit pages (`pack/page.rs`, `pack_audit.rs`) and `pack new` (`pack/scaffold.rs`);
- `test`'s report and its verdicts (`test/mod.rs`, `verdict.rs`);
- the report of how a driven run stopped (`drive.rs`).

27 CLI files build a `Doc`. 11 commands implement the CLI's `Answer` trait:
- `document(look) -> Doc` for a person;
- `outcome()` for the exit code;
- `Serialize` for `--json`.

**The documents `yunta-render` already holds, in a second form:** free functions with positional arguments, each handed the spelling of commands.
- `gate::page(GatePage, spell)` and `gate::whole_page(...)`;
- `plan::document(...)`, `spec::document(...)`, `shown::document(...)`;
- `findings::document(...)`, `findings::run_document(...)`;
- `receipt::document(receipt, spell, glyphs)`.

The decision a gate's page ends with is built in `gate` (`decision`, `record`, `routes`, `whole_page`) and again in the CLI's `status/decision.rs`, while render's `escalation` already holds "the words a decision is met by, wherever a person meets it".

**What the CLI's pages still read or name themselves:**
- **Paths from the engine's layout:**
  - `evidence.rs` computes where a failed check's output is kept (`ObjectStore::at(run_dir).path_of`);
  - `facts.rs` names the run's branch (`run_branch`) and its artifacts (`run_dir::artifacts_view`);
  - `node.rs` names `ObjectStore` and `view_path`;
  - `decision.rs` names `view_of`.
- **Engine types:**
  - `DecisionOn`, whose decision it is, defined in `yunta-engine/src/run/escalation.rs`, while the `Escalation` it always travels with is core's (`events/gates/escalation.rs`);
  - `ShownDocument` and `ShownContent`, core's (`yunta_core::shown`), named through the engine's re-export of them.
- **Pieces of the CLI:**
  - `advice`, which 44 files name;
  - `render::counter`, `render::node_display` and `render::called`;
  - `surface::closing::Outline`, where the run lived and where paths are shown from;
  - `surface::child_row` and `children_by_node`, a run's composed runs.
- **Two doors over one:** `status/progress.rs` wraps `RunFrame::of` in `frame()` and `frame_of()`, with 9 callers. That is the pattern `run-state` took out of the engine, written again in the CLI.
- **The caller reads first:**
  - the decision, through `current_escalation`, which reads the documents a gate shows from disk;
  - the engine's word, through `render::engine`, a wrapper over one call of `read_engine`.
- **What a run's registry says is a struct over a choice:** `yunta_replay::registry::Engine { liveness, prompt, words, at }`, with `EngineLiveness { Alive, Dead, Unrecorded, Unknown }`.
  - `prompt` and `words` hold only for a live process, and the type admits a dead one that asks.
  - `read_engine` reads `engine.json` and probes its process three times, once each through `engine_liveness`, `engine_prompt` and `engine_words`, so one reading can mix two instants.
  - `at` is the instant the registry was read, and every caller of `RunFrame::of` hands the same instant again as `now`.
  - `Prompt { node, since, pid }` carries a copy of the registry's `engine_pid`, so the reading keeps the process's identity only while it asks. Without that copy, `Prompt` is the engine's own `Asked { node, since }`, the record `engine.json` holds, under a second name.
- **Each page measures time itself:**
  - how long a person has been asked at a terminal: `advice::asking(prompt, at)`, and again the run page's headline (`open_for`);
  - how long a run has stood where it is: the field's `time_in_state`, and the run page's `since`;
  - where its time went: the run page's facts and the JSON, each with `state.time_spent`, which the frame's own stats already compute.
  
  Each subtracts `chrono` instants. `yunta-render` names no clock type, so the run page could not move as it is written.

**What a person can do next:**
- **The table:** `advice::after_for(frame, decision, reader) -> Vec<(String, &'static str)>` decides the steps by the run's standing and the decision it waits on. It spells each one as a command and words what it does, in one function.
  - Its callers are the run page and the closing (through `after`), and the JSON a program reads (`json/mod.rs`, for an agent).
  - A terminal that bound keys to steps would parse the strings back.

**The commands a person is told to type:**
- **As data, partly:** `yunta_core::step::NextStep` holds nine of them, the ones a page offers for a run: status (whole or one node), resume, cancel, close, receipt, verify, resolve-gate, gc and check.
- **As strings, the rest:** `advice.rs` writes 36 commands as strings:
  - review, open, follow;
  - doctor, `doctor --session`;
  - stats;
  - list, `list --runs`, `--all`, `--full`;
  - schema with a document and `--json`;
  - test, init, `pack <verb>`, run;
  - `--help`;
  - `git show <commit>`.
- **The words of a parked run share the same file:** `STALLED`, `asking`, `asking_on`, `parked`, `parked_on`, `parked_claim`, `parked_in_full`, `reset_pause`.
- **Who a run is named for:** `Reader` (a person, who copies a run's handle; an agent, who copies its whole id) is the CLI's, in `error.rs`.
- **The spelling is the CLI's, `advice::spell_for(step, reader)`, so every crate below the CLI that prints a command takes it as a closure,** `yunta_core::step::Spell`:
  - `yunta-render`'s gate, plan, receipt, spec, published, shown and decision block;
  - `yunta-adapters`' forge, `GitHubForge::configure(repo, token, spell)`;
  - a dozen CLI files.
- **It is written twice more:**
  - `testkit-core/src/spell.rs`, held to the CLI's by a test;
  - `engine/tests/receipt.rs`.
- **The rule and its smell:** "a command a person is told to type is spelled in one module" is held by the smell `command_outside_advice`, which counts only `crates/cli/src`.

**The goldens,** all in `crates/cli/goldens/`, 78 files:
- `status/`: 54;
- `closing/`: 12;
- `open/`: 6 HTML;
- `stats/`: 3;
- `field/`: 3.

They are read by the unit tests beside the pages: `run_page/tests.rs`, `surface/closing/tests.rs`, `list/runs/tests.rs`, `stats/run/tests.rs`. `yunta-render` holds 21 of its own (`document/` 16, `gate/` 4, `blocks/` 1).

The CLI's integration tests print the commands whole: `status_cmd`, `status_page_cmd`, `status_node_cmd`, `status_nodes_cmd`, `stats_cmd`, `open_cmd`, `close_cmd`, `receipt_cmd`, `verify_cmd`, `pack_requires_doctor_cmd`, `run_flow`, `status_follow`, and the commands' own.

**`yunta-render` and `yunta-replay`:**
- `yunta-render` (4,004 lines in its root files) depends on core and surco.
- The pages it would hold read from `yunta-replay`:
  - `frame`: `RunFrame`, `NodeFrame`, `NodeStanding`, `RunPhase`, `WaitingOn`, `Counter`;
  - `registry`: `Prompt` (and `Engine`, which this module takes out of every page's reach);
  - `stats`: `RunStats`, `NodeStat`, `median`;
  - `history`: `RunSummary`, `PriorEstimation`, `MIN_SAMPLES`;
  - `verification_effectiveness`: `VerificationFindings`;
  - the root: `RunState`.
  
  All pure. The map already gives render the edge to replay, and no cycle follows.

### Who this is for

- **The terminal application and the desktop (`app-core`, `tui`)**: a screen's regions hold these pages as `Doc`s through one trait, and a key or a button is bound to an action as data.
- **The control crate (`control`)**: it returns data and never prints, and the pages draw that data.
- **A person at the CLI**: every command prints exactly what it prints today.
- **A contributor**: a page is found in one place and in one form, and a command is spelled in one place.

### What each of them gets

1. **Every page in `yunta-render`, in one form.**
   - Each page is a value its caller fills, and draws through one trait:
     ```rust
     pub trait Page {
         fn document(&self, look: &Look) -> Doc;
     }
     ```
   - They are:
     - the run's pages, the decision, the field, the stats and history, and the doctor's report;
     - every command's answer: cancel, close, verify, receipt, resolve-gate, open, gc, the catalog, init, new, schema, pack's pages, test's report and verdicts, how a driven run stopped;
     - the documents render already holds: the gate's page, the plan, the spec, what a gate shows, the findings, the receipt.
   - The documents render already holds take the same form, and lose their positional arguments and their `spell`.
2. **A page reads nothing.** Its caller hands it everything it prints:
   - where the run lives, its branch and where paths are shown from, as the caller found them. Where an artifact's bytes and its view sit under the run's directory is core's (`yunta_core::run_dir`), beside the `ARTIFACTS_DIR` and `OBJECTS_DIR` core already holds, so the engine that writes there and the page that names it share one layout;
   - the exit code a closing shows;
   - the decision and the documents it shows;
   - the run's frame, which carries what the process driving the run says.
   
   `yunta-render`'s manifest names neither the engine, nor the store, nor a runtime, and `tests/pure.rs` reads it to say so, as `yunta-replay`'s does.
   - **The frame measures every time a page says,** at the one instant its caller hands `RunFrame::of`:
     - how long a person has been asked at a terminal (`RunFrame::asking`, a `registry::Asking { pid, prompt, waited }`, which `Engine::asking(at)` builds and nothing else does);
     - how long the run has stood where it is (`since_last_event`);
     - where its time went (`time`).
     
     A page says durations, and `yunta-render` measures none: the run page's headline and its gist read the same `waited`, and the field and the run page the same `since_last_event`. The one instant a page holds is the one its caller read and handed in: when the page `open` writes was drawn.
   - **What a run's registry says is one enum, `Engine`:** `Alive` with its process's pid, the `Prompt` it holds at its terminal and the socket it serves its words on, `Dead`, `Unrecorded` or `Unknown`, read off the registry with one read and one probe. It is an input of the frame, `close`, `gc`, the pool, the follower and the refusal of a command on a run that asks. No page takes it. Its name is the engine's because that process is the engine everywhere else (`engine.json`, its `engine_pid`, `engine.lock`, and what a reader reads: "its engine is asking"); `driver` is a word this repository retired, what an adapter is not, which the `banned_vocabulary` smell holds.
   - **The pid is the process's, and the question is the question.** `Prompt` is `{ node, since }`, one type for the record `engine.json` holds and the reading returns. A sentence about a process that asks takes `Asking`, which joins the two.
3. **One home for how a decision reads.** The CLI's `status/decision.rs` and the decision `gate` builds become one block in render's `escalation`, which already holds the words a decision is met by. The decision a run waits on, its type and its block, live there, and the gate's page, the run page and the closing all draw it from there.
4. **What a person can do next, as data in `yunta-replay`.**
   - `RunFrame::actions(decision) -> Vec<Action>`, where `Action { step: NextStep, effect: Effect }`.
   - `Effect` is a closed set of what an action does: it continues the run from its log, continues it where it stopped, closes it for good, stops it, certifies it, says where its log stops reading, hands it back once a decision is settled, waits for a limit's reset, and so on.
   - The rules are `after_for`'s, moved whole, and the words of each effect are render's.
   - The run page, the closing, the field and the JSON a program reads all take the actions. A terminal binds keys to them, a desktop draws buttons from them.
5. **Every command a person is told to type is a `NextStep`, spelled once.**
   - `NextStep` grows to the whole set: review, open, follow, doctor (with or without its sessions), the listings, schema, test, init, pack, run, help and `git show`, beside the nine it holds. An argument the reader fills is part of the data (`Arg::Blank`, written as the argument's name; a decision's `Choice`), so a line with `<option>` or `<document>` is a `NextStep` too.
   - `yunta_render::command::line(&step)` writes each one for a person, `line_for(&step, reader)` for either reader, and `Reader` moves with it.
   - Seven texts change because a better form of a command replaces a worse one, and the documentation follows them:
     - `yunta schema <kind>` becomes `yunta schema <document>`, its argument's one name, in the skill `init` writes and in `docs/`;
     - "the workflow fails `yunta check`" names the workflow it refused;
     - the `--detach` refusal of `--adapter mock` names "another process" instead of a bare `yunta resume`;
     - `yunta stats` with neither a run nor `--workflow`, or with both, is refused by clap like every other usage error;
     - three texts of `yunta pack` that named a subcommand bare name a whole line with the pack their context knows: `yunta pack update <publisher/name> <ref>` and `yunta pack remove <publisher/name>` where a pack is already installed, `yunta pack add <source>` where it is not, and `yunta pack new <publisher/name>` in the README a new pack is scaffolded with.
   - Deleted: `Spell`, every parameter that carried it, the copies in `yunta-testkit-core` and the engine's tests, and the test that held two copies to agreement.
   - The smell that holds the rule counts every shipped crate but render's `command`, and is named for it.
6. **The words of a parked run join render's `pause`, the words of the engine driving a run are render's `engine`,** and `advice` is gone.
   - `pause` says what the log recorded: a pause, and what a parked run waits on.
   - `engine` says what holds a run whose log says it moves: a process that is gone (`STALLED`), or one asking a person at its terminal. It says an `Asking` at three lengths, as `pause` says a `WaitingOn`:
     - `asking_on`, a row's one line: the node;
     - `asking_claim`, beside a headline that says how long, and in a refusal: the node and the pid;
     - `asking_in_full`: the node, the pid and how long.
7. **The CLI keeps what depends on its process, in its own modules:**
   - reading a run: its log, its manifest, the decision through `current_escalation`, the registry through `read_engine`, the paths;
   - each command's work, and its `Answer`: the page it shows, with its exit code and its JSON;
   - `output`: the streams' color, width and glyphs, drawing to them, emitting an answer, and the exit code (`Outcome`, `RunExit`);
   - the JSON documents (the map's rule: `json/` stays in `yunta`);
   - the live region and `--follow`.
   
   The work of each command stays where it is until `control` takes it.
8. **Nothing printed changes but the seven texts of point 5, and a decision's record where nothing else on the page says it:**
   - a decision's record, chosen by the user in Phase 9: a decision leaves out the record of a failed node's evidence when the page quotes that evidence above it, so it is said once. At the base the decision left it out whenever the log said the node failed, and a node a live engine asks a person about at its terminal, which stands `needs you` and is not quoted above, had its record said nowhere; now the decision says it.
   - the 78 goldens move with the tests that read them, by `git mv`, byte for byte;
   - render's 21 stay;
   - every CLI integration test passes unchanged, and the JSON documents print the same.

### Names and form

Each item gets the one path and the name it should always have had, where it moves.

| At the base | After |
|---|---|
| `RunPage`, `NodePage`, `ListRunsAnswer::document`, `RunStatsPage`, `WorkflowPage`, `DoctorAnswer::document`, every other answer's `document`, `gate::page`, `plan::document`, `spec::document`, `shown::document`, `findings::{document, run_document}`, `receipt::document` | one value each in `yunta_render::page`, named by what it shows: `page::Run` (its scopes `Status`, `Closing`, `Browser`), `page::Node`, `page::Field`, `page::Stats`, `page::History`, `page::Doctor`, `page::Gate`, `page::Plan`, `page::Spec`, `page::Shown`, `page::Findings`, `page::Receipt`, and each command's answer by what it did (`page::Cancelled`, `page::Closed`, `page::Answered`, `page::Collected`, ...) |
| `Answer::document` in the CLI | `yunta_render::page::Page::document`; the CLI's `Answer` pairs a page with its exit code and its JSON |
| `run_page::Decided` (the decision a run waits on) | `yunta_render::escalation::Decision`: it is not decided yet |
| `status/decision.rs` and `gate::{decision, record, routes}` | one decision block, in `yunta_render::escalation`; `gate::whole_page` becomes `page::Gate` |
| `surface::closing::Outline` | `page::Places`: where the run lived, the paths its caller computed, and where they are shown from |
| `status/progress.rs`'s `frame()`, `frame_of()` | gone: callers replay once and call `RunFrame::of` |
| `status/progress.rs`'s `summary` and the CLI's `render::counter` | `yunta_render::progress`, how far a run has come; it no longer reads like replay's `Counter` |
| `render::node_display(&node)` (CLI) | `NodeDisplay::from(&node)`, in `yunta_render::state` |
| `render::called(workflow, mode)` (CLI) | in `yunta_render::state`, beside a node's words |
| `render::engine(run_dir, at)` (CLI) | gone: callers call `yunta_engine::read_engine` |
| `yunta_replay::registry::{Engine, EngineLiveness}`, a struct over a choice | `yunta_replay::registry::Engine`, one enum: `Alive { pid, prompt, words }`, `Dead`, `Unrecorded`, `Unknown` |
| `yunta_replay::registry::Prompt { node, since, pid }` and the engine's `process_registry::Asked { node, since }` | `yunta_replay::registry::Prompt { node, since }`, one type, which `engine.json` holds; the pid is `Alive`'s |
| `yunta_engine::{engine_liveness, engine_prompt, engine_words}`, and `read_engine(run_dir, probe, at)` over them | `yunta_engine::read_engine(run_dir, probe)`: one read of the registry, one probe of its process |
| `Engine::at`, a second name for the frame's instant | gone: `RunFrame::of`'s `now` is the one instant |
| the subtractions in `advice::asking`, the run page's `since` and `open_for`, the field's `time_in_state`, and `time_spent` in the run page's facts and the JSON | `RunFrame::{asking, since_last_event, time}`, with `yunta_replay::registry::Asking { pid, prompt, waited }`, built by `Engine::asking(at)` |
| `advice::after_for(frame, decision, reader)`, `advice::after(...)` | `RunFrame::actions(decision) -> Vec<Action>`, in `yunta_replay::frame` |
| a `&'static str` beside a spelled command | `Effect`, in `yunta_replay::frame`; its words in `yunta-render` |
| `advice::spell_for`, `advice::spell`, `advice::{review, open, follow, doctor, stats, list_runs, schema, ...}` | `yunta_render::command::{line, line_for}`; every command a `NextStep` |
| `advice::named(run, reader)` | `Reader::name_of(run)` |
| `advice::{parked, reset_pause}`, readings of a phase | `RunPhase::{waiting_on, reset_wait}`, in `yunta_replay::frame` |
| `ObjectStore::path_of`, `view_path`, `view_of`, `run_dir::artifacts_view` (engine) | `yunta_core::run_dir::{object, view, view_of, artifacts}`, with `ARTIFACTS_DIR` and `OBJECTS_DIR` |
| `surface::{child_row, children_by_node}` | `yunta_render::children`, read by the run page and the live region |
| `crate::error::Reader` | `yunta_render::command::Reader` |
| `yunta_core::step::Spell`, every `spell` parameter, `yunta_testkit_core::spell()` | gone |
| `advice::{parked_on, parked_claim, parked_in_full}` | `yunta_render::pause` |
| `advice::{STALLED, asking, asking_on}` | `yunta_render::engine::{STALLED, asking_claim, asking_in_full, asking_on}`: `asking` splits by length, `asking_claim` without how long and `asking_in_full` with it |
| `advice` | gone |
| `yunta_engine::DecisionOn` | `yunta_core::events::DecisionOn`, beside `Escalation` |
| `yunta_engine::{ShownDocument, ShownContent}`, re-exported | named from `yunta_core::shown`; the re-export is gone |
| `Inbox`'s `Group`, `RunRow` (rows holding worded strings) | `page::field::{Group, Row}`: a row is data, and render words it, for the page and for the CLI's JSON alike |
| `verify::CheckRow`, which `doctor` borrows | a check's JSON form, in the CLI's `json/`; `page::Doctor` and `verify`'s page take Surco's `Check`s |
| the CLI's `render` module (`stream`, `draw`, `words`) and `output` | one `output` module: the streams, drawing to them, emitting an answer, `Outcome` and `RunExit` |
| `error::Outcome` (an exit code, beside the errors) | `output::Outcome`, beside `RunExit` |
| the smell `command_outside_advice`, over `crates/cli/src` | `command_outside_command_rs`, over every shipped crate that can name render's `command` module: all but core and surco |

### Decisions the plan starts from

- **One form for a page: a value and `Page::document(look)`.** A page with many inputs reads by their names; a free function of positional arguments does not. One trait lets the CLI's `emit` and the application's screens take any page.
- **Actions are data in `yunta-replay`; their words and their spelling are `yunta-render`'s.** A key a terminal binds and a button a desktop draws stand on the action, never on a string.
- **Every command a person is told is a `NextStep`.** `NextStep` keeps its name, since every one of them is what a reader can type next.
- **Each move goes the way `run-state` went:**
  - names change in place first, then the move;
  - a file moves with `git mv` before it is edited;
  - no alias or re-export is left;
  - the workspace compiles and passes after each step.

### Assumptions

1. The JSON documents stay in the CLI and print the same, read from the same data. Where a document carries a worded field (a row's `command`, `said`, `reason`), it is worded by render.
2. The live region (`surface/`: region, painter, view, scrollback), the prompts and `--follow` stay in the CLI, as the map's row for `app-core` says. The closing block the region leaves is `page::Run`'s closing scope.
3. Each command's work (reading, gathering, cancelling, collecting, installing, testing) stays where it is until `control`.
4. A golden that moves with its test is not touched, in the map's sense: its bytes and its name stay, and only its directory follows the test.
5. `NextStep` is never serialized: no payload, schema or JSON document holds one (measured), so its shape is free and its unused serde and schema derives go. `cargo xtask schema --check` passes with no change to `crates/core/schemas/`.
6. `tasks/plan.md` and `tasks/todo.md` are replaced for this module; those of `run-state` stay in its commit, `13cdd69c`.
7. `CAPABILITY-MAP.md`'s row and its "What moves" line say this module's scope: every page, the actions and every command's spelling.
8. The refusal a command meets on a run whose driving process asks at its terminal (`CliError::RunAsking`) keeps its words. It holds the `Asking` that `read_engine(..).asking(now)` gives at its own instant, with no frame, and says `asking_claim`: where to answer, and not for how long. The length decides what a sentence says, not the data it holds, as under a run page's headline. Saying how long would change what a person reads.

### Out of scope

- Any change to what a page says or how it is laid out, but the seven texts of point 5 and a decision's record of point 8.
- The live region, the prompts, `--follow` and the JSON documents' shape.
- Each command's work, which is `control`'s.
- `third-party-notices`, dropped from the map.

## Tech Stack

Rust 1.98.0. `yunta-render` gains `yunta-replay` as a dependency, and `chrono`, already the workspace's, to name the instant its caller hands the page `open` writes. `yunta-render` reads no clock: every duration it says is measured in `yunta-replay`. Surco's `golden` feature holds the goldens, as it does now.

## Commands

`cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.

```bash
# Before editing a file, and before finishing with no paths
cargo xtask docs --about <files>

# Test
cargo test -p yunta-render
cargo test -p yunta-replay
cargo test -p yunta-core
cargo test -p yunta --bins
cargo test -p yunta --test status_cmd --test stats_cmd --test run_flow --test status_follow

# Lint and checks
cargo xtask smells --check
cargo xtask docs --check
cargo xtask schema --check
cargo deny check

# The gate, every step of the ci job
sh .plans/yunta-tui/tools/gate.sh /Users/eulke/development/yunta-tui .plans/yunta-tui/out/gate-run-pages
```

## Project Structure

```
crates/render/src/page.rs            the Page trait, and the map of every page
crates/render/src/page/              one module per page: run, node, field, stats, history, doctor,
                                     gate, plan, spec, shown, findings, receipt, and each command's answer
crates/render/src/command.rs         every NextStep as the command a reader types, and Reader
crates/render/src/progress.rs        how far a run has come, the one line every surface prints it in
crates/render/src/pause.rs           why a run is parked, now with the words of a parked run
crates/render/src/engine.rs          the words of the engine driving a run: gone, or asking at its terminal
crates/render/src/escalation.rs      the decision a run waits on, its words and its one block
crates/render/goldens/               its 21 goldens, and the CLI's 78, moved with their tests
crates/render/tests/pure.rs          the manifest names no engine, store or runtime
crates/replay/src/frame/actions.rs   what a person can do next for a run, as actions
crates/replay/src/registry.rs        what a run's registry says: Engine, Prompt, and Asking, a live process's prompt measured
crates/core/src/step.rs              NextStep, every command a reader is told to type, and its blanks
crates/core/src/run_dir.rs           where a reader finds an artifact's bytes and view under a run's directory
crates/render/src/children.rs        the runs a run bore, as the rows a person reads them in
crates/core/src/events/gates/        DecisionOn, beside Escalation
crates/cli/src/output/               the streams, drawing, emitting, the exit code
crates/cli/src/commands/             each command: reads, does its work, fills its page, emits it
xtask/src/smells/language.rs         the smell, over every shipped crate but render's command module
```

## Code Style

A page is a value its caller fills; an action is data the page words:

```rust
// crates/replay/src/frame/actions.rs
/// What a person can do next for a run: the command, and what it does.
pub struct Action {
    pub step: NextStep,
    pub effect: Effect,
}

impl RunFrame {
    /// The actions a reader can take next for this run, `decision` being the
    /// one it stopped on when its log rebuilds one.
    pub fn actions(&self, decision: Option<&GateWaitingPayload>) -> Vec<Action>;
}

// crates/render/src/command.rs
/// The command line a person types for `step`.
pub fn line(step: &NextStep) -> String;

/// The command line `reader` types for `step`, the run named the way it copies it.
pub fn line_for(step: &NextStep, reader: Reader) -> String;

// crates/cli/src/commands/status/mod.rs
let page = page::Run { scope: Scope::Status, depth, frame: &frame, state: &state, places, .. };
emit(mode, &StatusAnswer { page, json }, look)
```

## Testing Strategy

- **Every golden moves with its test** to `crates/render`, byte for byte, and passes there unchanged. They are the guard of every move.
- **The CLI's integration tests pass unchanged.** With the goldens, they prove the CLI prints exactly the same.
- **The actions:** `advice/tests.rs`'s assertions move with what they test. The actions a frame offers go to `yunta-replay`, the spelling of every `NextStep` to `yunta-render`, where one test spells each kind.
- **Purity:** `crates/render/tests/pure.rs` reads render's manifest.
- **The frame's measures:** replay's frame tests read a frame at a chosen instant and find how long its person has been asked, how long since its last event, and its time. `read_engine`'s tests read the four kinds of registry, with `Alive`'s pid the one the registry names. A test written before the change pins the `asking` an asking engine writes to `engine.json`, and passes before and after it.
- **One spelling:** the agreement test is deleted with the second copy. A search for `Spell` and for a command written as a string outside `command.rs` finds nothing, and the smell holds it.
- **The schemas:** `cargo xtask schema --check` passes with `crates/core/schemas/` unchanged, since no schema holds a `NextStep`.
- **Every line parses:** one CLI test writes every kind of `NextStep` for both readers and parses it with the CLI's clap definition.
- The suite's count is explained against the base's 274 binaries and 4,531 tests.

## Boundaries

- **Always:**
  - move a file with `git mv` before editing it;
  - change names in place before moving;
  - keep every golden byte for byte;
  - keep the workspace compiling and passing after each step.
- **Ask first:**
  - a move that would change what a page prints;
  - a change to a JSON document's shape;
  - a new dependency of `yunta-render`.
- **Never:**
  - a page that reads the disk, the clock, a process or the environment, or subtracts two instants;
  - an alias or a re-export left for a caller;
  - a command spelled twice;
  - a change to what a person reads.

## Success Criteria

1. **Every page is a value in `yunta_render::page`** and implements `Page`. No CLI file builds a `Doc` but `output` drawing one and the live region. Each command fills its page and emits it.
2. **Render's manifest names core, replay and surco,** and neither the engine, the store nor a runtime, which `tests/pure.rs` holds. No page names a path function of the engine or reads a clock, and none holds an `Engine`: the one instant a page holds, the time `RunInABrowser` was drawn at, is its caller's, and render says every instant in one format.
3. **What a person can do next is `RunFrame::actions`,** returning `Action`s. `after_for` and `after` are gone, and every surface that offered next steps reads the actions.
4. **Every command a person is told to type is a `NextStep`,** spelled only by `yunta_render::command`, and a test parses every kind's line with the CLI's clap:
   - `Spell`, the copies in `yunta-testkit-core` and the engine's tests, and the agreement test are gone;
   - render and the adapters spell directly;
   - the smell finds none elsewhere.
5. **Every name in "Names and form" stands as written:**
   - `advice`, `render::counter`, `render::words`, `render::engine`, `progress::frame` and `progress::frame_of` are gone from the CLI;
   - `EngineLiveness`, `engine_liveness`, `engine_prompt`, `engine_words`, `Engine::at` and the engine's `process_registry::Asked` are gone, and so are `time_in_state` and the run page's `since`; `read_engine` reads the registry once;
   - `engine.json` reads and writes the same bytes;
   - `DecisionOn` and `ShownDocument` come from core, and the engine re-exports neither.
6. **Nothing printed changes but the seven texts of point 5, and a decision's record where nothing else on the page says it:**
   - the 78 goldens are byte for byte the same, in `crates/render/goldens/`;
   - every CLI integration test passes unchanged but where it prints one of those texts;
   - the JSON documents print the same;
   - the suite's count is explained.
7. **The docs and the gate:**
   - `.agents/architecture.md`'s crate table says render depends on core, replay and surco, and its rule names the renamed smell;
   - `CAPABILITY-MAP.md` says this module's scope;
   - `cargo deny check`, `docs --check`, `smells --check` and `schema --check` pass, and every step of the `ci` job passes.

## Open Questions

None. The draft's three are answered:
- every command's answer is a page, and so are the documents render already holds, in the same form;
- `DecisionOn` goes to core, beside `Escalation`;
- the field's groups go to render, with the field.
