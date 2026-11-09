# Tasks: `control`

Spec: `SPEC-control.md`. Plan: `tasks/plan.md`. Status: **approved on 2026-11-09, in four phases.** Base: `34217b5f` (`feature/yunta-tui`, with rmcp 3.9.0 and rusqlite 0.41.1).

**How these run:**
- One at a time, with `incremental-implementation` and `test-driven-development`, in four phases, each ending at a checkpoint for the user.
- Inside a phase, each group of tasks ends at a group check: the suite's groups green, and the digests, captures, documents and answers against the base where the group touches them. A group check does not stop for the user.
- Inside a phase, the work stops for the user only on a decision that is theirs: a printed text, document or answer that would change, a choice neither the plan nor this list makes, or a failure whose cause is not found.
- `cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.
- Before editing a file, `cargo xtask docs --about <files>`; each rule, trap and note it prints is kept true.
- A commit closes each phase, once the user approves its checkpoint and every step of the `ci` job passes. Paths are staged by name. Then `cargo clean`.
- The engine's and the CLI's suites run in foreground batches: `python3 .plans/yunta-tui/tools/suite.py <group>`.

**How an item moves:**
- Its names and its form change first, in place, with every caller. Then it moves, with every caller's path.
- Neither step leaves an alias, a re-export or an old name, and the workspace compiles and passes after each task.
- A task that moves a file does it with `git mv` before it edits it. A file that is only partly moving is split: the moving part goes, the rest stays.
- A task that renames or moves is larger than five files: one path to each item means a name and its callers change in one step. Each such step is mechanical, the compiler names every caller, and the moved bodies are read with `git diff -M` against their old files.

**How a command moves to the API:**
- The API's operation is named as what a person does (the plan's decision 4) and returns data: render's page data, replay's readings, core's types, or the API's own handles. It prints nothing, and it takes nothing from its process.
- The CLI's command keeps clap, the streams, the prompts and the exit code: it asks the API, then draws render's page or writes render's document.
- When the MCP server asks for the same thing, its tool calls the same function in the same task.
- The CLI's `Context` keeps, until its last user moves, only what the commands still outside the API need. It is built from the API's project and deleted with that user (Task 39).
- No public item of `yunta-api` takes or returns a type of `yunta-engine`, `yunta-storage` or `yunta-adapters`; Task 42 holds it.

**What fails first:**
- A move has no new behavior, so no test can fail for it. Its guard is every test and golden it touches, passing unchanged, and the digests, captures, documents and answers of the base (Tasks 1 to 3).
- A new rule, a new test or a changed text is seen red, for its own cause, before it passes.

## Phase 1: the base, and what moves below the surfaces

Its groups, each green before the next: the base; the ports in core; refusals as core data, said by render; what MCP speaks, and the documents.

### Task 1: the proof's tools, pointed at this module

**Description:**
- `goldens.py`, `pages.sh` and `normalize.py` are copied from `proof/run-pages/` into `.plans/yunta-tui/proof/control/`, unchanged but for their folder. A mask `normalize.py` gains stands for a difference seen between two captures of the base, and says so.
- `documents.py` writes every JSON document a command writes, each command `Command::json` names with `--json`, over the runs `capture.sh` leaves in a sandbox of its own (finished, failed, gated, capped, parked): `status` of each, `list --runs`, `stats` and `stats --workflow`, `verify`, `receipt`, `open`, `doctor`, `schema` and `schema tasks`; then `run --adapter mock`, a refused run, `resolve-gate`, `resume`, `cancel` and `close`. `status --node` takes no `--json`.
- `mcp_answers.py` starts `yunta mcp` over stdio in a sandbox of its own, with a run parked on its questions added as `mcp_flow` adds one. It says `initialize`, asks `tools/list`, and calls each tool: `list_workflows`; `workflow_status` on each run and on one nobody has; `document_shape` of four kinds and one Yunta does not read; `answer_questions` refused, then answered; `resolve_gate` refused, on a finished run, then answered; `resume_run`; `run_workflow` of an unknown workflow, then of a bash one. Each answer is kept as sorted JSON with its text unescaped under it.
- `prints.sh` runs the three into one capture directory, which `normalize.py` compares whole.

**Fails first:** none: tools.

**Acceptance criteria:**
- [x] Each tool runs twice on the base and finds nothing between its two runs.

**Verification:**
- [x] Manual check: the two runs' outputs differ in no line once normalized.

**Dependencies:** None

**Files likely touched:** `.plans/yunta-tui/proof/control/{goldens.py,pages.sh,normalize.py,documents.py,mcp_answers.py,prints.sh}` (outside git)

**Estimated scope:** M

### Task 2: what the base prints and answers

**Description:**
- The suite's binaries and tests by name, in `suite.py`'s groups, into `proof/control/base-suite/`.
- Every golden's digest, `digests-base.txt`.
- A release build, `proof/control/yunta-base`; its size; `prints.sh` from it into `base-1/`, then again into `base-2/`.

**Fails first:** none: measurements.

**Acceptance criteria:**
- [x] `base-1` and `base-2` are equal once normalized; the counts are written in `baseline.txt`.

**Verification:**
- [x] Manual check: `python3 normalize.py base-1 base-2` prints no difference.

**Dependencies:** Task 1

**Files likely touched:** `.plans/yunta-tui/proof/control/` (outside git)

**Estimated scope:** S

### Task 3: a real run on the base

**Description:**
- fragua, started detached from the base's release binary, on the CLI and model the session-ends runs used (conform on `gpt-6-luna`), stopped before its `pr` node.
- Its gates answered with `yunta resolve-gate`; its time between nodes and its tokens read from `yunta stats <run>`.
- The run's id, time and tokens go into `baseline.txt`.

**Fails first:** none: a measurement.

**Acceptance criteria:**
- [x] The run reaches the node before `pr`; its `between_secs` and tokens are recorded.

**Verification:**
- [x] Manual check: `yunta stats <run>` read back from `baseline.txt`: run `HM6VJX` waited at `ship` after 222 s with nothing asked before it; `between_secs` 0.550 s; 2.15M tokens, 1.44M of them cached.

**Dependencies:** Task 2

**Files likely touched:** `.plans/yunta-tui/proof/control/real-base*` (outside git)

**Estimated scope:** S

### Group check: the base
- [x] The base's suite, digests, captures, documents and answers stand, twice equal.
- [x] The real run's time and tokens are recorded.

### Task 4: the person port is core's

**Description:**
- The engine's `Asking` is renamed `DecisionAsked` in place, with every caller: `Asking` is replay's registry reading, which a surface reads beside it.
- `crates/engine/src/human_interaction.rs` moves whole with `git mv` to `crates/core/src/port/person.rs`, a private module: `HumanInteraction`, `QuestionsReply`, `DecisionAsked` and `NoInteraction` each have one path, `yunta_core::port::<item>`, as the session's and the forge's items mostly are read. `core::port`'s `//!` names the port.
- Its `//!` loses what only the engine can say (its links to the engine's items), and says where the console and the MCP tool implement it.
- The engine calls it from core and exports none of it. `NoInteraction`, the port with nobody at it, moves with it: it holds no IO and no engine rule, and the engine's tests, the testkit and the API build it.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] `git grep -n 'yunta_engine::\(HumanInteraction\|QuestionsReply\|Asking\)'` finds nothing; the engine's `lib.rs` names none of them.
- [x] The suite passes unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-core -p yunta-testkit`, then `suite.py engine-1..5` and `cli-1..5`.
- [x] Manual check: `git diff -M --stat` shows the file moved, and its body changed only in paths and the rename.

**Dependencies:** Task 3

**Files likely touched:** `crates/core/src/port/{person.rs,mod.rs}`, `crates/engine/src/{lib.rs,run/ctx.rs,run/escalation.rs,run/mod.rs,run/questions_exec.rs,run/schedule/decision.rs,task_cycle/outcome.rs}`, `crates/cli/src/{human_interaction.rs,ask/form.rs,commands/promote.rs,commands/registry.rs}`, `crates/testkit/src/{interaction.rs,lib.rs}`, eight tests under `crates/engine/tests/`

**Estimated scope:** L (mechanical)

### Task 5: the observer port is core's

**Description:**
- `crates/engine/src/observer.rs` moves with `git mv` to `crates/core/src/port/observer.rs`: `RunObserver` and `Observed`.
- `Said`, what one session said, moves from `engine::said` to the same module with its serde shape. `TEXT_MOST` and the bounding of a frame stay with the engine's voices, which cut the text before they hand it on.
- The `//!`'s list of the sites that append outside the log's door stays true from core: it names them by what they do, not by an engine path core cannot link.
- The engine keeps serving the words socket; `said::connect` stays the engine's until Task 23 moves its reader.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] `git grep -n 'yunta_engine::\(RunObserver\|Observed\|said::Said\)'` finds nothing; the engine exports neither port.
- [x] `crates/engine/tests/observer.rs` passes unchanged, and so does the suite.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-core -p yunta-engine --test observer`, then the suite's groups.
- [x] Manual check: the moved body read with `git diff -M`.

**Dependencies:** Task 4

**Files likely touched:** `crates/core/src/port/{observer.rs,mod.rs}`, `crates/engine/src/{lib.rs,said/mod.rs,run_log.rs,run_tools/host.rs}`, `crates/cli/src/surface/{feed.rs,region.rs,view/mod.rs,view/said.rs,mod.rs}`, `crates/cli/src/commands/{drive/watch.rs,status/follow/listen.rs}`, `crates/testkit/src/{observer.rs,bench/*.rs}`

**Estimated scope:** L (mechanical)

### Group check: the ports
- [x] The suite's count equals the base's, by name; every golden's digest equals the base's.

### Task 6: what a check finds is core's

**Description:**
- `core::check` holds `CheckError` (from `engine/check/error.rs`), `Composition` (from `check/refs.rs`), `Unanswerable` (from `check/sources.rs`) and `UntakenRoute` (from `check/gates.rs`), with their `Display` and their own tests.
- The engine keeps every check that finds them, and imports them from core.
- `check/error.rs` moves whole with `git mv`; the other three types are cut from files that keep their checks, each into a module of its own (`composition.rs`, `unanswerable.rs`, `untaken_route.rs`), private, so each item has one path, `yunta_core::check::<item>`.
- `check/pointer.rs`, where in the workflow file each of them points, moves with them: inherent methods live with their type. Where a node is declared was said twice, in the engine's `check/site.rs` and in `Place::pointer`'s closure; it is said once, `workflow::declared_node`, and `site.rs` goes.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] The engine names the four types only through `yunta_core::check`, and exports none of them.
- [x] Every check test, and the CLI's refusals, pass unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-core`, `suite.py engine-1..5`, `cli-1..5`.

**Dependencies:** Task 5

**Files likely touched:** `crates/core/src/{check/mod.rs,check/error.rs,check/composition.rs,check/unanswerable.rs,check/untaken_route.rs,check/pointer.rs,workflow/place.rs,workflow/mod.rs,lib.rs}`, `crates/engine/src/check/{mod.rs,refs.rs,sources.rs,gates.rs,pointer.rs,site.rs}`, `crates/engine/src/lib.rs`, `crates/cli/src/{commands/verdict.rs,commands/refusals.rs,commands/run.rs,commands/doctor/*.rs,detect/mod.rs,error/page.rs,error/words/*.rs}`

**Estimated scope:** L (mechanical)

### Task 7: a refused gate and a refused close are core's

**Description:**
- `core::refusal` holds:
  - `GateRefusal`: `NotPaused`, `NothingToResolve`, `UnknownOption`, `Unsaid` and `Withheld`, the refusals of `ResolveGateError`;
  - `CloseRefusal`: `AlreadyClosed`, `Driven` and `Moving`.
- No `ViewFault`: `StoreError`, what an artifact view meets, holds `Object(ObjectProblem)`, already core's data, and `Io`, a failure. A view the store lacks an object for is said from `ObjectProblem`, as it is today.
- `ResolveGateError` becomes `Refused(GateRefusal)`, `Storage` and `Documents`; `CloseRunError` becomes `Refused(CloseRefusal)`, `Storage` and `Export`. Each refusal's display is still its code, through `Refused`'s.
- Every match on those variants, in the engine, its tests and the CLI, matches the new shape.

**Fails first:** none: a reshape; the texts are pinned by the CLI's tests.

**Acceptance criteria:**
- [x] No caller tells a refusal from a failure by listing an error's variants: it matches `Refused(_)`.
- [x] The suite passes unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-core`, the engine's `resolve_gate*`, `close*` and `artifact*` targets, then the suite's groups.

**Dependencies:** Task 6

**Files likely touched:** `crates/core/src/{refusal.rs,lib.rs}`, `crates/engine/src/{run/escalation.rs,run/close.rs}`, `crates/cli/src/{error.rs,error/words.rs}`, the engine's tests that match on them

**Estimated scope:** M

### Task 8: render says every refusal

**Description:**
- `crates/render/src/refusal/` takes, with `git mv`, the CLI's `error/words.rs` (as `words.rs`), `error/words/refusals.rs` (as `check.rs`), `error/words/wrapped.rs` and `error/words/tests.rs`: every sentence over what Yunta refuses, said from core's data.
- One path each: `yunta_render::refusal::{gate, close, moving, hold_unclear, inputs, refused_answer, already_installed, not_installed}` and `yunta_render::refusal::check::{said, line, lacking}`. `resolve_gate` over the engine's error becomes `gate` over `GateRefusal`, and `close` takes `CloseRefusal`.
- The CLI keeps, in a new `error/words.rs`, only what still reads an engine error: `resolve_gate` and `close` over the engine's errors, which hand a refusal to render and say a failure below it, `documents` and `project_view`. Their three tests stay with them; the API's `Failure` takes them in Phase 2.
- The words' tests move with them, reading core's types where they read the engine's (`InputsProblem`, not the engine's alias of it).

**Fails first:** none: the words move; their tests pass unchanged where they now live.

**Acceptance criteria:**
- [x] Render holds every sentence over what Yunta refuses; the CLI's `error::words` holds only sentences over an engine error.
- [x] The words' tests pass in render, unchanged but for the types they build.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render`; the CLI's binaries and `check`, `close_cmd`, `gate_review_cmd`, `gate_withheld_cmd`, `pack_cmd`, `pack_requires_doctor_cmd`, `parked_runs` and `run_flow`.
- [x] Manual check: each moved function read with `git diff -M` against its old body.

**Dependencies:** Task 7

**Files likely touched:** `crates/render/src/{refusal/mod.rs,refusal/words.rs,refusal/check.rs,refusal/wrapped.rs,refusal/tests.rs,lib.rs}`, `crates/cli/src/{error.rs,error/words.rs,error/words/tests.rs,error/page.rs,commands/doctor/packs.rs,commands/pack.rs,commands/run.rs}`

**Estimated scope:** L (mechanical)

### Task 9: the CLI carries render's refusal

**Description:**
- `yunta_render::refusal::Refused`, in `refusal/refused.rs`, is one refusal as a value: `UnknownRun`, `AmbiguousRun` with its `Candidate`s (moved from `run_ref`, which hands it the glyphs), `Gate`, `Close`, `Answers`, `Moving`, `Asking`, `HoldUnclear`, `Inputs`, `SurfaceAnswer`, `Object`, `Document`, `Unrunnable` and `UnknownDocument`. It says itself to a `Reader` (`said`), says what goes on (`advice`), gives the blocks under it, and is a `Page`.
- A reply to a node's questions the run refuses was an engine error too: `core::refusal::AnswersRefusal` (`NothingAsked`, `Rejected(Report)`), and `AnswerQuestionsError` carries it as `Refused(_)` beside `Unreadable`, `Write`, `Render` and `Storage`, saying what it said.
- `CliError::Refused(Refused)` replaces `RunNotFound`, `NotPaused`, `NoMenu`, `CloseRefused`, `RunMoving`, `RunAsking`, `RunHoldUnclear`, `AmbiguousRun`, `Document`, `Unrunnable` and `UnknownDocument`. A refusal inside `RunError`, `ManifestError` or `AnswerQuestionsError` becomes it in their `From`; `CliError::from_gate` and `from_close` take the run a gate's or a close's refusal names. What stays an engine type in `CliError` is a failure below a refusal, until the API's `Failure` in Phase 2.
- `carried_words` keeps only failures; `headline_for_a_person`, `blocks` and the sentences of refusals are render's; the CLI's page delegates a refusal to render's and says a failure.
- The page helpers that build render's data from core's go to render: `blocks::diagnostic::document_problems`, `refusal::check::problem` and `refusal::unrunnable`.
- Two smells this raised are brought down where their seam shows: `CheckError::node_pointer` splits into it and `setting_pointer`, and the engine's `escalation` tests keep what `resolve_gate` refuses in `escalation/refused.rs`.

**Fails first:** none: every refusal's text is pinned by the CLI's tests, the captures, the documents and the answers.

**Acceptance criteria:**
- [x] `CliError` names no engine type for a refusal: a refusal reaches it only as `Refused`.
- [x] Every refusal's text, in the tests and the captures, equals the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render`; the engine's `parked_answers` and `escalation`; the CLI's binaries and `check`, `close_cmd`, `gate_review_cmd`, `gate_withheld_cmd`, `pack_cmd`, `parked_runs`, `run_ref_cmd`, `mcp_flow`, `run_flow`, `three_questions`, `status_cmd`, `integration`, `command_output` and `schema_cmd`.
- [x] Manual check: a release build's captures, documents and answers against `base-1`, with `normalize.py`.

**Dependencies:** Task 8

**Files likely touched:** `crates/core/src/{refusal.rs,check/pointer.rs,lib.rs}`, `crates/engine/src/answers.rs`, `crates/engine/tests/{escalation.rs,escalation/refused.rs}`, `crates/render/src/{refusal/refused.rs,refusal/mod.rs,refusal/words.rs,refusal/check.rs,blocks/diagnostic.rs}`, `crates/cli/src/{error.rs,error/page.rs,error/hold.rs,commands/run_ref.rs,commands/verdict.rs,commands/check.rs,commands/registry.rs,commands/run.rs,commands/list/mod.rs,commands/pack/scaffold.rs,commands/close.rs,commands/resolve_gate.rs,commands/test/case.rs,context.rs,main.rs}`

**Estimated scope:** L

### Group check: the refusals
- [x] Every refusal's text is the same, through the CLI's tests and the captures.
- [x] The suite's count equals the base's, by name, but for the tests that moved with their words.

### Task 10: `yunta-mcp-protocol`

**Description:**
- `crates/mcp-protocol`, package `yunta-mcp-protocol`, library `yunta_mcp_protocol`, depending on `rmcp` and nothing in the workspace.
- Its root takes `crates/engine/src/mcp.rs` with `git mv`: `tool_list`, and the `//!` that says why every MCP server of Yunta answers one shape.
- The engine's per-run tools and the CLI's control plane call it; `engine::mcp` is gone.
- `.agents/architecture.md`'s crate table gains its row, and the CI's and the gate's crate-by-crate check name it. The file stands at 6,142 of its 6,144 bytes: the note under the table that the first product rule and its test already say goes.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] The engine's `[dependencies]` name `yunta-mcp-protocol`; nothing names `yunta_engine::mcp`.
- [x] `run_tools`, `tool_names`, `mcp_flow` and the adapters' tests pass unchanged; `cargo xtask docs --check` passes.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-mcp-protocol`; `cargo check -p yunta-mcp-protocol` alone; the engine's `run_tools`, `tool_names`, `mcp_context` and `run_tool_refusal`; the CLI's `mcp_flow` and `rail_cmd`.

**Dependencies:** Task 9

**Files likely touched:** `Cargo.toml`, `crates/mcp-protocol/{Cargo.toml,src/lib.rs}`, `crates/engine/{Cargo.toml,src/lib.rs,src/run_tools/session.rs}`, `crates/cli/{Cargo.toml,src/commands/mcp.rs}`, `.agents/architecture.md`, `.github/workflows/ci.yml`

**Estimated scope:** M

### Task 11: the branch a run works on is core's

**Description:**
- `run_branch` and the prefix of every run's branch move from `engine/worktree/branches.rs` to core, beside `RunId`'s handle: `run_id.branch()` and `RunId::BRANCHES`, a name every crate reads the same way, which needs no IO.
- The engine names the branch from core; the documents of Task 12 and the CLI's pages read it there.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] `git grep -n 'yunta_engine::run_branch'` finds nothing; the suite passes unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-core --lib`; the engine's library and `git`, `resume_worktree` and `checkout_pool_shared`; the CLI's `run_checkouts`, `status_cmd` and `run_flow`.

**Dependencies:** Task 9

**Files likely touched:** `crates/core/src/ids/run.rs`, `crates/engine/src/{worktree/branches.rs,lib.rs}`, `crates/cli/src/{commands/drive.rs,commands/run.rs,commands/status/mod.rs,json/facts.rs}`

**Estimated scope:** M (mechanical)

### Task 12: a run's JSON documents are render's

**Description:**
- `crates/render/src/document/` takes, with `git mv`, the CLI's `json/` and the documents it is built of: `commands/stats/json.rs` (as `stats.rs`) and `commands/status/decision_json.rs` (as `decision.rs`); `ErrorDocument` and `NextCommand` leave `output/mod.rs` for `document/error.rs`. Every document carries its `schema_version`, from render's page data and replay's.
- The code a run's word maps to is part of what a document says, so it is said once there, `document::exit_code`; the CLI's `exit()` reads its outcome off it, and an interrupted invocation is `Outcome::interrupted()`.
- What reads the engine (`decision_of`, `shown_of`) stays the CLI's, in `commands/status/decision.rs`, until Task 21 reads a run through the API. The field's rows take the glyphs the CLI hands them, as they took the process's before.
- Each command's own answer document (`CancelAnswer`, `CloseAnswer`, `DoctorAnswer`, `ListRunsAnswer`, `ResolveGateAnswer`, `VerifyAnswer`) moves to `render::document` with the task that takes its command to the API: its fields are read off what the API returns.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] Render holds a run's documents and their parts; the CLI has no `json` module.
- [x] The documents and the MCP answers equal the base's, byte for byte once normalized.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render`; the CLI's binaries and `status_cmd`, `stats_cmd`, `run_flow`, `mcp_flow`, `run_inbox_cmd`, `verify_cmd`, `receipt_cmd`, `detected_cmd`, `command_output`, `run_words`, `parked_runs` and `stalled_runs`.
- [x] Manual check: a release build's documents and answers against `base-1`.

**Dependencies:** Tasks 10, 11

**Files likely touched:** `crates/render/{Cargo.toml,src/lib.rs,src/document/*.rs}`, `crates/cli/src/{main.rs,output/mod.rs,output/exit.rs,commands/status/mod.rs,commands/status/decision.rs,commands/stats/mod.rs,commands/drive.rs,commands/list/inbox.rs}`, the commands that name a document

**Estimated scope:** L (mechanical)

### Checkpoint A: below the surfaces
- [x] The ports, the refusals' data, what MCP speaks and the documents stand below every surface, and no engine exports them.
- [x] The JSON documents and the MCP answers, byte for byte; every refusal's text the same; the suite's count equals the base's, by name, but for the tests that moved with what they test.
- [x] The user reviews; the phase commits, with `SPEC-control.md`, `CAPABILITY-MAP.md`, `tasks/plan.md` and `tasks/todo.md`: approved on 2026-11-09.

## Phase 2: `yunta-api`, its workspace, and reading through it

Its groups, each green before the next: the crate and its workspace; reading through the API.

### Task 13: the crate, its workspace, its stop and its errors

**Description:**
- `crates/api`, package `yunta-api`, library `yunta_api`, with the dependencies the plan's decision 2 names; its root's `//!` maps the crate by the subjects of decision 4.
- `workspace.rs`: `Workspace`, the six fields of decision 3. `stop.rs`: `Stop`, its two stages, and the engine's `Supervision` built from it inside the crate.
- `error.rs`: `Error { Refused(Refused), Failed(Failure), Stopped }`; `Failure` boxes the cause and says its chain as the CLI's `describe` says it.
- `.agents/architecture.md`'s table gains the row; the crate names what it uses so far (core, render, and the engine from Task 15), and the CI's and the gate's crate-by-crate check name it. The file stands at its budget: a few rows and sentences say the same facts in fewer words.

**Fails first:**
- `Failure`'s words: a test that a store failure wrapped twice says the same line `describe` says for it today, red until `Failure` says its chain.
- `Stop`: a test that stopping cancels what runs and leaves the abort standing, and that aborting cancels both, red until `Stop` holds two tokens.

**Acceptance criteria:**
- [x] The crate builds with no public item that takes or returns an engine, store or adapter type.
- [x] Both tests pass; `cargo xtask docs --check` passes.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api`.

**Dependencies:** Checkpoint A

**Files likely touched:** `Cargo.toml`, `crates/api/{Cargo.toml,src/lib.rs,src/workspace.rs,src/stop.rs,src/error.rs}`, `.agents/architecture.md`

**Estimated scope:** M

### Task 14: the shell reads the process once

**Description:**
- `crates/cli/src/shell.rs` replaces `context.rs`'s reading of the process: the directory, `Env`, the clock, the id source, the secrets, the binary (`current_exe`), and the interrupt, which gives the `Stop`. It builds the `Workspace` and the `Stop` once.
- `main.rs` builds the shell first; the terminal's variables it reads for the look come from the shell's `Env`.
- `process_env()` (five callers), `process_clock()`, `own_binary()`, `time_until`'s own clock and the `current_dir` reads in `init`, `list`, `new`, `pack`, `pack/scaffold` and `test` go: each takes the workspace.
- The MCP server reads its directory once when it starts, not at each call: it holds a shell that shares the stages.
- `main`'s reads go there too: `shell::environment()` and `shell::arguments()`. The router reads the shell once for every command, `completions`, `schema` and the rail hook included, so no arm can read it twice and none is left without it; `route` holds the arms, and no command answers the field.
- The CLI's interrupt trips the API's `Stop`; the context is `Context::of(shell)` or `Context::in_dir(shell, dir)`, its clock and ids the workspace's `Arc`s, its binary the shell's. `system_clock_outside_boundary` allows `shell.rs` in place of `context.rs` now, since the clock is built there.

**Fails first:** none: the same values reach the same places.

**Acceptance criteria:**
- [x] `git grep -n 'current_dir()\|current_exe()\|process_env\|process_clock\|args_os' -- crates/cli/src` finds only `shell.rs`.
- [x] The suite passes unchanged.

**Verification:**
- [x] Tests pass: `suite.py cli-1..5`.

**Dependencies:** Task 13

**Files likely touched:** `crates/cli/src/{shell.rs,context.rs,main.rs,project.rs,cli/dispatch.rs,commands/init.rs,commands/list/mod.rs,commands/new.rs,commands/pack.rs,commands/pack/scaffold.rs,commands/test/mod.rs,commands/mcp.rs}`

**Estimated scope:** L (mechanical)

### Task 15: a project is the API's

**Description:**
- `Workspace::project()` returns `yunta_api::Project`, from `crates/cli/src/project.rs` and `context.rs`: the config layers, the roots, the store's path, a run's directory, the pool and tree a run uses, and the layers `yunta schema`'s config reading names.
- `ProjectError`'s cases become the API's: a missing state root and an unreadable layer are failures; none is a refusal today.
- Measured: resolving a project says nothing on the way, so `Remarks` is born with its first sayer, an estimate's caution (Task 25).
- `Workspace::project()` and `project_at(dir)` resolve it; `config_layers(dir, env)` reads the layers `yunta check` names. The project holds the user root it was resolved under, so finding a run reads no environment.
- The pool a run takes its trees from is the engine's, so it stays on the CLI's context (`Context::pool_of`) until the run path moves; `bound_checkout` was a wrapper of replay's `state.run.checkout()`, which its callers read directly.
- The CLI's `Project` is gone; its `Context` holds the API's `Project` and, until their last user moves, the store and the supervision the remaining commands use.

**Fails first:** none: every project resolves as it did.

**Acceptance criteria:**
- [x] The CLI has no `project.rs`; every project resolves through `Workspace::project()`.
- [x] The suite passes unchanged, and the captures equal the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api`, `suite.py cli-1..5`.

**Dependencies:** Task 14

**Files likely touched:** `crates/api/src/{project.rs,workspace.rs,lib.rs}`, `crates/cli/src/{project.rs,context.rs,error.rs,commands/schema.rs}`

**Estimated scope:** M

### Task 16: the adapters a binary builds are the API's

**Description:**
- `yunta_api::machine` takes from `commands/mod.rs`: `built_adapters`, `built_adapter`, `built_adapter_names`, `built_adapter_ids`, `first_built_adapter`, `real_adapters`, `service_vars`, `declared_facts` and the `Adapters` they make, with the adapters' ids they name. Its test of the example a surface offers moves with it.
- What `check` knows of an adapter, `AdapterFacts`, is check data like the rest: it moves to `core::check`, so `declared_facts` names no engine type.
- Every caller asks the API.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] `commands/mod.rs` names no adapter crate; the suite passes unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api`, `suite.py cli-1..5`.

**Dependencies:** Task 15

**Files likely touched:** `crates/api/src/machine/{mod.rs,adapters.rs}`, `crates/cli/src/commands/{mod.rs,run.rs,resume.rs,doctor/mod.rs,test/case.rs}`

**Estimated scope:** M

### Task 17: detection is the API's

**Description:**
- `crates/cli/src/detect/` moves with `git mv` to `crates/api/src/machine/detection/`, with its tests: what a repository is, what it runs, and its remote.
- `in_repo(workspace, repo, stop)` builds the supervision git reads the remote under from the workspace and the caller's `Stop`, inside the API (`Workspace::supervision`); a stop while it reads is `Error::Stopped`. The CLI's context carries the workspace and gives the stop, for the commands still outside the API.
- `init`, `doctor` and the pages that show `Detected` ask the API.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] The CLI has no `detect` module; the detection tests pass in the API, unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api`, the CLI's `detected_cmd` target, `suite.py cli-1..5`.

**Dependencies:** Task 16

**Files likely touched:** `crates/api/src/machine/detection/*.rs`, `crates/cli/src/{main.rs,commands/init.rs,commands/doctor/mod.rs}`

**Estimated scope:** M (mechanical)

### Group check: the workspace
- [x] The CLI resolves every project through the API; the environment is read once.
- [x] The suite and the captures equal the base's.

### Task 19: the field is the API's

**Description:**
- `Project::field(reach, depth)` returns the rows, the unreadable runs and the runs elsewhere, from `commands/list/inbox.rs`: the registry, the engines that drive, the run branches.
- `yunta list` and `yunta list --runs` ask it.
- Its answer document moves to `render::document` with it, its fields read off what the API returns.
- Found in Phase 1: `list --runs --json` writes each row's reason and words with this terminal's glyphs, so `YUNTA_GLYPHS=ascii` changes the document, while a run's document always writes Unicode. A document reads the same whichever terminal asked for it: the field's rows take Unicode, as a run's do, and the glyphs leave `document::field::rows`.
- Found in Task 19's turn: how deep a listing reads is the page's, never the reading's, so the API takes `Project::field(FieldReach::Here | Machine, &stop)` and returns a `FieldReading`, named apart from render's page; core's `step::Reach` stays the command line's. A project carries the workspace it was resolved from, which its operations read the directory, clock and environment from, so the context's own copy goes. A git the caller's stop cancelled is `Error::Stopped`, in one conversion. render's `FieldDocument` is the whole listing document, which the CLI's answer serializes.

**Fails first:** a test that `list --runs --json` under `YUNTA_GLYPHS=ascii` writes the same document as without it, red while the rows take the terminal's glyphs.

**Acceptance criteria:**
- [x] `list --runs --json` writes one document whatever glyphs the terminal asks for; `list`'s page still draws them.
- [x] `inbox.rs` names no engine or store; `list`'s captures and documents equal the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api`, the CLI's `run_inbox_cmd`, `stalled_runs` and `surface_parity` targets.

**Dependencies:** Task 18

**Files likely touched:** `crates/api/src/field.rs`, `crates/cli/src/commands/list/{mod.rs,inbox.rs}`

**Estimated scope:** S

### Task 20: a run named, opened by the API

**Description:**
- `Project::run(&RunRef)` resolves a reference (`last`, a handle, an id) and returns an `OpenedRun`: its manifest and directory. The resolving moves from `commands/run_ref.rs`; `RunArg`, clap's, stays in the CLI.
- `Context::open_run`, `near_run` and `run_roots` move with it; a run nobody has and a reference that names several are the API's refusals.
- Found in Task 20's turn: a reference names a run and an id opens one, two questions an interface asks apart (a person types a reference, the field and the control plane hold ids), so the API answers `Project::find(&RunRef, &stop) -> RunId` and `Project::run(&RunId) -> OpenedRun { id, dir, manifest, events }`. A candidate of an ambiguous reference holds its field row and render draws it with the reader's glyphs, so the API refuses without reading a terminal. `last` with no run, `needs` with none waiting and a text no id can be are render's refusals, said as before.

**Fails first:** none.

**Acceptance criteria:**
- [x] Every command that names a run opens it through the API; a missed id still says the near one.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api`, `suite.py cli-1..5`.

**Dependencies:** Task 19

**Files likely touched:** `crates/api/src/run/{mod.rs,reference.rs}`, `crates/cli/src/{commands/run_ref.rs,context.rs,cli/dispatch.rs,cli/mod.rs}`

**Estimated scope:** M

### Task 21: a run's reading

**Description:**
- `OpenedRun::reading(depth)` returns a `RunReading`: its state, frame, decision, documents and plans, with the engine that drives it and the branch.
- `status`, `status --node`, `graph` and the JSON's decision read it; the documents of Task 12 take the decision from it.
- Found in Task 21's turn: an opened run is asked again, so `OpenedRun<'p>` keeps the project it was opened in, and `reading()` takes no depth: how deep a page reads is the page's, and the reading answers `waits_on`, `shown`, `produced(node)` and `plans()` when a page asks. `graph` reads `frame()`, which rebuilds no decision, so a run with an unreadable document still draws. The reading's frame carries the estimate a run parked on its budget was told, which reads the workflow's history, so `Project::history` (`WorkflowHistory`), `prior_estimation` and `budget_prior` come here from Task 22, and `status/decision.rs` goes: the drive calls the engine's two readers itself until it moves. A `RunError` the API meets is a refusal, the stop, or a failure, in one conversion.

**Fails first:** none.

**Acceptance criteria:**
- [x] `status/mod.rs`, `graph.rs` and render's documents name no engine; their captures and documents equal the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api`, the CLI's `status_cmd`, `status_node_cmd`, `status_nodes_cmd`, `status_page_cmd` and `graph_cmd` targets.

**Dependencies:** Task 20

**Files likely touched:** `crates/api/src/run/reading.rs`, `crates/cli/src/{commands/status/mod.rs,graph.rs,main.rs}`

**Estimated scope:** M

### Task 22: a run's stats, history, receipt, verification and open page

**Description:**
- `OpenedRun::stats()`, `OpenedRun::receipt()`, `OpenedRun::verification()` and `OpenedRun::opened()` return the data `stats`, `receipt`, `verify` and `open` draw; `stats --workflow` reads `Project::history`, which Task 21 brought.
- `verify`'s answer document moves to `render::document` with it, its fields read off what the API returns.
- Found in Task 22's turn: `OpenedRun::stats()` returns a `StatsReading` (the frame, the counts, the project's prices). `OpenedRun::receipt()` derives the receipt and writes both formats beside the run, which a later node reads, so every interface leaves them; a run not finished is render's `Refused::NoReceipt`. A run is verified by its id, `Project::verification(&id)`, never opened, because the chain answers about a log whose payloads no longer parse; it returns render's `Verification` (the chain, and what reading the objects found), whose `checks()` render says, and `VerificationDocument` writes them. `open` draws the reading Task 21 gave it; the page file it writes and the browser it asks for are how a terminal shows a page, which a desktop shows its own way, so no `opened()` operation. render's documents each have one path at `document::`.

**Fails first:** none.

**Acceptance criteria:**
- [x] Those five commands name no engine or store; their captures equal the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api`, the CLI's `stats_cmd`, `receipt_cmd`, `verify_cmd`, `open_cmd` and `verification_effectiveness_cmd` targets.

**Dependencies:** Task 21

**Files likely touched:** `crates/api/src/run/{stats.rs,receipt.rs,verification.rs,opened.rs}`, `crates/cli/src/commands/{stats/mod.rs,receipt.rs,verify.rs,open.rs}`

**Estimated scope:** M

### Task 23: following a run

**Description:**
- `OpenedRun::follow()` reads the run's heads, its log after a seq, whom its engine asks, and its words over the socket (`said::connect`'s reader).
- `status --follow`, the live region's gaps (`surface/mod.rs`, `painter.rs`, `turns.rs`) and the MCP tool `workflow_status` read it.
- Found in Task 23's turn: what follows a moving run is held by a task that draws it, so the API hands owned handles rather than one borrowing the project: `EventLog` (a run's log, what came past a seq, the runs it holds), `RunRegistry` (the process driving a run, and its successor's registry) and `Words` (what the sessions say on the registry's socket). `offers_a_menu` decides a parked run's menu for the field's rows and the region's alike. The drive opens an `EventLog` for its region and its closing until it moves into the API, which then builds it on the store it drives with. `Pace`, the cadence of a poll over state another process owns, moves to `core::pace`, since the engine, `cancel` and the follower all space their looks with it. The workflow_status tool reads the run as Task 21 left it.

**Fails first:** none.

**Acceptance criteria:**
- [x] The surface and `status --follow` name no engine or store; the follow tests and `workflow_status`' answers equal the base's.

**Verification:**
- [x] Tests pass: the CLI's `status_follow`, `live_motion`, `run_words`, `run_surface` and `mcp_flow` targets.

**Dependencies:** Task 21

**Files likely touched:** `crates/api/src/run/follow.rs`, `crates/cli/src/commands/status/{follow.rs,follow/listen.rs}`, `crates/cli/src/surface/{mod.rs,painter.rs,turns.rs}`, `crates/cli/src/commands/mcp/tools.rs`

**Estimated scope:** M

### Task 18: the catalog is the API's

**Description:**
- `Project::catalog()` returns render's `Catalog` data, from `commands/list/mod.rs`: the workflows here, the installed packs' and their publishers'.
- `yunta list --workflows` and the MCP tool `list_workflows` call it.
- Found in Task 18's turn: each entry's estimate reads the workflow's history off the store, which the API reads from Task 22 on; so the catalog follows it, and `catalog_workflows`, which `doctor` and the forge read, goes with it.
- Found in Task 18's turn: what the catalog offers is read off the repository alone, and `list --workflows` lists in a project that does not resolve, only without estimates; so the catalog is the workspace's, `Workspace::catalog()` and `Workspace::catalog_workflows()`, and the project is tried for the history only.

**Fails first:** none: the same catalog.

**Acceptance criteria:**
- [x] One function reads the catalog for both; `list`'s captures and `list_workflows`' answer equal the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api`, the CLI's `run_inbox_cmd`, `pack_resolution_cmd` and `mcp_flow` targets.

**Dependencies:** Task 22

**Files likely touched:** `crates/api/src/catalog.rs`, `crates/cli/src/commands/{list/mod.rs,mcp/tools.rs}`

**Estimated scope:** S

### Task 24: the wait after a host sleeps is core's

**Description:**
- `SETTLE_AFTER_SUSPENSION` moves from `engine/wakefulness.rs` to `core::clock`, beside the host clock whose awake time it measures: the engine waits it, and the live region's words say it.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] `surface/chronicle/words.rs` reads it from core; `host_suspension` passes unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-engine --test host_suspension`, the CLI's chronicle tests.

**Dependencies:** Task 18

**Files likely touched:** `crates/core/src/clock.rs`, `crates/engine/src/{wakefulness.rs,lib.rs}`, `crates/engine/tests/host_suspension.rs`, `crates/cli/src/surface/chronicle/words.rs`

**Estimated scope:** S

### Checkpoint B: the workspace and reading
- [x] The CLI resolves every project and reads every run through the API; the environment is read once.
- [x] The captures, the documents and the follow tests equal the base's.
- [x] The user reviews; the phase commits.

## Phase 3: running a run, answering it and ending it, through the API

Its groups, each green before the next: admission and running; answers and a run's end.

### Task 25: admission

**Description:**
- `Project::admit(&reference, inputs, mode)` returns an `Admission`: the verdict, the preflight and the cautions. It takes from `commands/verdict.rs`, `refusals.rs`, `committer.rs`, `forge.rs`'s reachability, and `run.rs`'s check, estimate and context files.
- An estimate's caution reaches the caller as a `Remark`.
- `yunta check`, `new`'s verdict and `run`'s preflight ask it.
- Found in Phase 1: the engine names core's `InputsProblem` a second time, `pub use yunta_core::inputs::InputsProblem as InputsError`. The alias goes, and every caller names `InputsProblem`: one path to each item.
- Found in Task 25's turn: `check`, `new` and `graph` ask `Project::verdict(Judged { workflow, path, config, conflicts }, VerdictReach, &stop)`; its `Verdict` answers `passes`, `cautions`, `checked`, `diagnosis` and `refusal`, and the CLI says it. `Project::admit(RunRequest, &remarks, &stop)` takes the inputs read and the adapter pinned (`--input` and `--fixture` are the CLI's flags; a request is `hermetic` for a test case, which freezes none of the person's knowledge), and says each caution and the estimate as a `Remark` in the order the CLI printed them. What a run front refuses is render's value: `NoCommitter`, `ForgeUnreachable`, `Unservable` (core's data: a pin no runner reaches, an unbuilt pin, a mock run with no fixture, no adapter, unhealthy adapters) and `Unchecked` (said with `check::line`, which names the near miss everywhere now). `machine::servable` is the API's, for a run and a resume; `Workspace::committer` answers `Committer::Named | Unnamed`, and a stop while git answers is the caller's stop in `doctor` too. Whether a run reaches a pull request is core's `opens_a_pull_request`; whether a program is on `PATH` is `Env::finds`. `doctor`'s probe and the test runner are admitted like any run. The admission hands the CLI its frozen run (`Admission::frozen`) until Task 26 creates the run in the API.

**Fails first:** none.

**Acceptance criteria:**
- [x] `check.rs`, `verdict.rs` and `refusals.rs` hold no engine call; a refused run's text equals the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api`, the CLI's `check`, `check_keys_cmd`, `context_files_cmd`, `forge_cmd`, `git_identity_cmd`, `run_flow` and `init_new_cmd` targets.

**Dependencies:** Checkpoint B

**Files likely touched:** `crates/api/src/admission.rs`, `crates/cli/src/commands/{check.rs,verdict.rs,refusals.rs,committer.rs,forge.rs,run.rs,new.rs}`

**Estimated scope:** M

### Task 26: creating a run

**Description:**
- `Admission::create()` returns a `CreatedRun`: the run born on the log, its checkout prepared and its hold taken. It takes from `run.rs` (`build_frozen_manifest`, `prepare_worktree`) and `registry.rs`'s creation.
- Found in Task 26's turn: `Admission::create(&remarks, &stop)` returns a `CreatedRun` (its id, directory, tree, manifest, estimate and cautions); the run's hold is taken by whoever drives it, here or in a detached process, so it is the drive's (Task 27) and the hand-off's (Task 28), not the creation's. A run past `limits.max_concurrent_runs` is render's `Refused::TooManyRuns`, and a dead process's lock this checkout takes over is a `Remark`. Until the drive moves, the CLI takes the created run as its `Prepared`.

**Fails first:** none.

**Acceptance criteria:**
- [x] `run.rs` creates no run itself; the run tests and captures equal the base's.

**Verification:**
- [x] Tests pass: the CLI's `run_flow`, `run_checkouts` and `environment_cmd` targets.

**Dependencies:** Task 25

**Files likely touched:** `crates/api/src/admission.rs`, `crates/api/src/drive.rs`, `crates/cli/src/commands/{run.rs,registry.rs}`

**Estimated scope:** M

### Task 27: driving a run here

**Description:**
- `CreatedRun::drive(person, observer, &stop)` and `OpenedRun::drive(...)` run it here with the ports a surface passes, follow its promotions, and return how it ended. They take from `drive.rs`, `registry.rs`'s execution, `promote.rs` and `run/attached.rs`.
- The CLI passes its console and its live region; the test runner and doctor pass the engine's surface with nobody to ask, through the API.
- Found in Task 27's turn, done with Task 28 so no hand-off has two paths meanwhile: a run is driven once this process holds it, so `CreatedRun::hold(&stop)` and `OpenedRun::take_back(&remarks, &stop)` (a parked run: refusals, the reset waited out, the claim, its checkout) both give a `HeldRun`, and `HeldRun::drive(Ports { person, observer }, &remarks, &stop)` executes it, follows its promotions, gives back what it held, hands a run paused on a reset to a detached resume, and returns a `Driven` (its last run, state and `Ending`, which never ends promoted) whose `closing()` and `document()` the CLI draws; `Project::closing` is the follower's too. What a lock on a run refuses, the forge that does not take its token and a wait for a reset are the API's (`Remark::ForgeUnavailable`, `Remark::WaitingForReset`), and `crate::remarks` says every remark for the CLI. The test runner drives its scripted runs and answers their gates its own way, so the registry moves with it, as `commands/test/scripted.rs`, until the cases move into the API; `doctor`'s probe drives through the API. A mock fixture is read by `machine::read_fixture`. `Pace` and the promotion's test are the API's; the test is one test per promise.

**Fails first:** none.

**Acceptance criteria:**
- [x] `drive.rs`, `registry.rs` and `promote.rs` hold no engine call; the run, promotion and live tests equal the base's.

**Verification:**
- [x] Tests pass: `suite.py cli-1..5`.

**Dependencies:** Task 26

**Files likely touched:** `crates/api/src/drive.rs`, `crates/cli/src/commands/{drive.rs,drive/watch.rs,registry.rs,promote.rs,run/attached.rs}`

**Estimated scope:** L

### Task 28: detaching a run, and handing a resume on

**Description:**
- `CreatedRun::start_detached()` and `OpenedRun::resume_detached()` start Yunta's own binary from the workspace; `DetachedResumeError` becomes the API's failure.
- `yunta run --detach`, `yunta resume` and the MCP tools `run_workflow` and `resume_run` call the same functions.
- Found in Task 28's turn: `resolve-gate` and the MCP answer tool hand the run on through `OpenedRun::resume_detached()` too, and a hand-on that fails after the decision or the answers are recorded is still said as recorded-but-not-resumed. `OpenedRun::refuse_if_driven()` is the API's, for every caller that is about to write on a run.

**Fails first:** none.

**Acceptance criteria:**
- [x] One function starts a run detached and one hands a resume on, for the CLI and the tools; `run_workflow`'s and `resume_run`'s answers equal the base's.

**Verification:**
- [x] Tests pass: the CLI's `run_flow`, `parked_runs`, `status_follow` and `mcp_flow` targets.

**Dependencies:** Task 27

**Files likely touched:** `crates/api/src/detach.rs`, `crates/cli/src/commands/{mod.rs,run/detach.rs,resume.rs,mcp/tools.rs,mcp/runs.rs}`

**Estimated scope:** M

### Task 29: a real run after admission and running

**Description:**
- The same fragua run as Task 3, from this phase's release binary, its gates answered the same way.

**Fails first:** none: a measurement.

**Acceptance criteria:**
- [x] It reaches the node before `pr`; its `between_secs` sits inside the base's spread. (Run 01M7147YP06CJVD759QRDF5P1Q: the base's path, at `ship` after 211 s, 2.03M tokens, `between_secs` 0.550052 against the base's 0.550108.)

**Verification:**
- [x] Manual check: `yunta stats <run>` beside `baseline.txt`.

**Dependencies:** Task 28

**Files likely touched:** `.plans/yunta-tui/proof/control/real-g*` (outside git)

**Estimated scope:** S

### Group check: the run path
- [x] The run path's tests and captures equal the base's; the real run sits beside the base's. (147 captures and 99 goldens equal; 4,567 tests, 59 moved, none lost: three renamed with their code.)

### Task 30: a gate's decision

**Description:**
- `OpenedRun::decide(choice)` records a gate's decision, then hands the resume on.
- Who answered is stamped from the workspace's `Env` when the caller claims nobody: `identity.rs` moves to the API, and the console's questions take the responder from there.
- `yunta resolve-gate`, `yunta review` and the MCP tool `resolve_gate` call it.
- Its answer document moves to `render::document` with it, its fields read off what the API returns.
- Found in Task 30's turn: who answered is a rule on core's `Responder`, `Responder::answering(claimed, user)`, which the console, `close` and the API stamp alike, so `identity.rs` goes rather than moving to the API. `OpenedRun::waiting_decision()` reads the decision with what it shows (a run that waits on none is render's `Refused::NoDecision`), and `OpenedRun::decide(choice)` records it and hands the run on; a hand-on that fails after the decision is recorded is said by the API as `decision recorded, but …`, and the engine's gate failures keep their words. `GateAnswerDocument` is render's.

**Fails first:** none.

**Acceptance criteria:**
- [x] `resolve_gate.rs` and `review.rs` hold no engine call; their tests and `resolve_gate`'s answers equal the base's.

**Verification:**
- [x] Tests pass: the CLI's `gate_review_cmd`, `gate_withheld_cmd`, `plan_desk`, `parked_runs`, `git_identity_cmd` and `mcp_flow` targets.

**Dependencies:** Task 29

**Files likely touched:** `crates/api/src/{answers.rs,identity.rs}`, `crates/cli/src/{identity.rs,ask/mod.rs,commands/resolve_gate.rs,commands/review.rs,commands/mcp/tools.rs}`

**Estimated scope:** M

### Task 31: a questions artifact's answers

**Description:**
- `OpenedRun::answer(&node, answers)` records them, then hands the resume on.
- The MCP tool `answer_questions` calls it; a person at the terminal answers through the person port, as now.
- Found in Task 31's turn: the reply is core's `port::QuestionsReply`, the same shape the person port answers with; the API stamps who answered and says a failed hand-on as `answers recorded, but …`.

**Fails first:** none.

**Acceptance criteria:**
- [x] `mcp/answers.rs` and `mcp/tools.rs` hold no engine call; `answer_questions`' answers, a refused one among them, equal the base's.

**Verification:**
- [x] Tests pass: the CLI's `three_questions`, `parked_runs`, `console_interaction` and `mcp_flow` targets.

**Dependencies:** Task 30

**Files likely touched:** `crates/api/src/answers.rs`, `crates/cli/src/commands/mcp/{answers.rs,tools.rs}`

**Estimated scope:** M

### Task 32: cancelling a run

**Description:**
- `OpenedRun::cancel(&stop)` signals the engine that drives it, waits, and records the pause after a crash; what it waits on reaches the caller as a `Remark`. It takes from `cancel.rs` and `cancel/registered.rs`.
- Its answer document moves to `render::document` with it, its fields read off what the API returns.
- Found in Task 32's turn: `OpenedRun::cancel(&remarks, &stop)` lives in `api/src/ending.rs`, the registration wait in `ending/registered.rs`, and the process groups with init's guard in `ending/groups.rs`. What it says while it works is a `Remark` each (`SignallingEngine`, `StoppedWaiting`, `Escalating`, `GroupRefused`, `Unstopped`, `RegistryKept`, `CheckoutKept`), whose sentences are render's `ending`, so the application says them in the same words; a registry it cannot read, a node with no registry and an interrupt that does not reach the engine are failures that say what goes on, as before. `CancelDocument` is render's. `Project::storage()` opens the store for every API operation, in place of seven copies.

**Fails first:** none.

**Acceptance criteria:**
- [x] `cancel.rs` holds no engine call; the cancel tests equal the base's.

**Verification:**
- [x] Tests pass: the CLI's `parked_runs`, `run_flow` and `run_words` targets.

**Dependencies:** Task 29

**Files likely touched:** `crates/api/src/ending.rs`, `crates/cli/src/commands/{cancel.rs,cancel/registered.rs}`

**Estimated scope:** M

### Task 33: closing a run

**Description:**
- `OpenedRun::close()` closes it, releases its checkouts, and returns what close answers; who closed it is stamped as Task 30 stamps a decision.
- Its answer document moves to `render::document` with it, its fields read off what the API returns.
- Found in Task 33's turn: `OpenedRun::close(by, &remarks, &stop)` answers who closed it, stamped in the API from the caller's claim or the workspace's user, and `decide(option, by, free_text)` now stamps the same way: `resolve-gate --option` hands the claim on as given, and only the menu, which asks the person at its terminal, names who answered there. A close's failures are the API's, so the CLI's close error, `from_close` and its words go, the export's sentence to render's `ending`; `CloseDocument` is render's, and the context's `pool_of` goes with its last caller.

**Fails first:** none.

**Acceptance criteria:**
- [x] `close.rs` holds no engine call; the close tests equal the base's.

**Verification:**
- [x] Tests pass: the CLI's `close_cmd` target.

**Dependencies:** Task 32

**Files likely touched:** `crates/api/src/ending.rs`, `crates/cli/src/commands/close.rs`

**Estimated scope:** S

### Checkpoint C: the run's path, its answers and its end
- [x] The run path's tests and captures equal the base's; the real run sits beside the base's.
- [x] `resolve-gate`, `review`, `cancel` and `close`, their tests and the parked runs' flows equal the base's.
- [ ] The user reviews; the phase commits.

## Phase 4: upkeep, the MCP server, the frontier and the proof

Its groups, each green before the next: the project's upkeep through the API; the MCP server, a crate of its own; the frontier; the proof and the review.

### Task 34: collecting

**Description:**
- `Project::collect(Collect::Pass | Collect::DryRun, remarks)` from `gc/mod.rs` and `gc/pool.rs`; each problem `gc` meets reaches the caller as a `Remark`, in order.

**Fails first:** none.

**Acceptance criteria:**
- [ ] `gc` holds no engine call; its tests and the dry run's capture equal the base's.

**Verification:**
- [ ] Tests pass: the CLI's `gc_cmd`, `gc_lock` and `run_checkouts` targets.

**Dependencies:** Checkpoint C

**Files likely touched:** `crates/api/src/collect.rs`, `crates/cli/src/commands/gc/{mod.rs,pool.rs}`

**Estimated scope:** M

### Task 35: the packs are the API's

**Description:**
- `Project::packs()` returns `Packs`: `add`, `update`, `remove`, `list`, `audit` and `new`, from `crates/cli/src/pack.rs` and `commands/pack.rs`, `pack/audit.rs`, `pack/scaffold.rs`. `PackError` becomes the API's.

**Fails first:** none: a move.

**Acceptance criteria:**
- [ ] The CLI's pack commands only parse, ask and draw; their tests equal the base's.

**Verification:**
- [ ] Tests pass: `cargo test -p yunta-api`, the CLI's `pack_cmd`, `pack_audit_cmd`, `pack_ceiling_cmd`, `pack_policy_cmd`, `pack_provenance_cmd`, `pack_resolution_cmd` and `factory_packs_cmd` targets.

**Dependencies:** Checkpoint C

**Files likely touched:** `crates/api/src/packs.rs`, `crates/cli/src/{pack.rs,commands/pack.rs,commands/pack/audit.rs,commands/pack/scaffold.rs}`

**Estimated scope:** M

### Task 36: two pack commands at once both stand

**Description:**
- `yunta_api::packs` holds `.yunta/yunta.lock` with the engine's file lock across each command's read and write, waiting for another pack command.

**Fails first:** the lock's test: it holds the lock, starts an `add` of one pack, sees it wait without writing, adds another pack's entry, releases, and finds both entries in the file. Without the lock, the `add` writes at once and the file holds one entry.

**Acceptance criteria:**
- [ ] The test fails for the lost entry before, and passes after.
- [ ] Two `pack add` of different packs started at once both stand in the lock file.

**Verification:**
- [ ] Tests pass: `cargo test -p yunta-api --test packs`, the CLI's `pack_cmd` target.

**Dependencies:** Task 35

**Files likely touched:** `crates/api/src/packs.rs`, `crates/api/tests/packs.rs`

**Estimated scope:** S

### Task 37: `init` and `new`

**Description:**
- `yunta_api::setup`: `Project::init(...)` and `Project::new_workflow(...)` from `init.rs`, `init/` and `new.rs`, with the committer's check.

**Fails first:** none.

**Acceptance criteria:**
- [ ] `init` and `new` hold no engine call; their tests and captures, the skill `init` writes among them, equal the base's.

**Verification:**
- [ ] Tests pass: the CLI's `init_new_cmd`, `console_interaction` and `runner_step_cmd` targets.

**Dependencies:** Task 36

**Files likely touched:** `crates/api/src/setup.rs`, `crates/cli/src/commands/{init.rs,init/*.rs,new.rs,committer.rs}`

**Estimated scope:** M

### Task 38: doctor's checks

**Description:**
- `Project::doctor(checks)` from `doctor/mod.rs`, `doctor/packs.rs`, `doctor/session.rs` and the forge's checks, in `yunta_api::machine`.
- Its answer document moves to `render::document` with it, its fields read off what the API returns.

**Fails first:** none.

**Acceptance criteria:**
- [ ] `doctor` only parses, asks and draws; its tests and `doctor --json` equal the base's.

**Verification:**
- [ ] Tests pass: the CLI's `detected_cmd`, `forge_cmd`, `git_identity_cmd`, `pack_requires_doctor_cmd` and `runner_step_cmd` targets.

**Dependencies:** Task 37

**Files likely touched:** `crates/api/src/machine/doctor.rs`, `crates/cli/src/commands/{doctor/*.rs,forge.rs}`

**Estimated scope:** M

### Task 39: the test runner

**Description:**
- `Project::cases(&dir, |case| ...)` runs every case and hands each back as it ends, from `test/mod.rs`, `test/case.rs` and `test/case/problems.rs`.
- The CLI's `Context` loses its last user and is deleted.

**Fails first:** none.

**Acceptance criteria:**
- [ ] `yunta test` only parses, asks and draws; its tests equal the base's; the CLI has no `context.rs`.

**Verification:**
- [ ] Tests pass: the CLI's `init_new_cmd`, `factory_packs_cmd` and `run_flow` targets, then `suite.py cli-1..5`.

**Dependencies:** Task 38

**Files likely touched:** `crates/api/src/cases.rs`, `crates/cli/src/{context.rs,commands/test/*.rs}`

**Estimated scope:** M

### Group check: upkeep
- [ ] Those commands' tests, and the lock's, pass.

### Task 40: `yunta-mcp`

**Description:**
- `crates/mcp`, package `yunta-mcp`, library `yunta_mcp`: each tool, its schema, its arguments, what it asks of the API and its answer, from `commands/mcp.rs` and `commands/mcp/`, moved with `git mv`.
- `ControlPlane::new(workspace, stop)` is an `rmcp` handler with no transport.
- `yunta mcp` serves it on stdio; the CLI's `commands/mcp.rs` keeps only that.
- `.agents/architecture.md`'s table gains its row.

**Fails first:** none: a move.

**Acceptance criteria:**
- [ ] The tools' answers equal the base's; `mcp_flow` passes unchanged; the descriptions' test passes in the crate.

**Verification:**
- [ ] Tests pass: `cargo test -p yunta-mcp`, the CLI's `mcp_flow` target.
- [ ] Manual check: `mcp_answers.py` from a release build against `base-1`.

**Dependencies:** Task 39

**Files likely touched:** `Cargo.toml`, `crates/mcp/{Cargo.toml,src/lib.rs,src/tools.rs,src/answers.rs,src/read.rs,src/runs.rs}`, `crates/cli/{Cargo.toml,src/commands/mcp.rs}`, `.agents/architecture.md`

**Estimated scope:** M

### Group check: the MCP server
- [ ] `mcp_flow` and every tool's answer equal the base's.

### Task 41: no surface names the engine, the store or the adapters

**Description:**
- `crates/cli/tests/no_engine_in_the_cli.rs` and `crates/mcp/tests/no_engine_in_the_control_plane.rs` read each crate's `[dependencies]`, as `no_adapter_crate_in_engine` reads the engine's, and fail naming the crate and saying the reading belongs to `yunta-api`.
- The CLI's and `yunta-mcp`'s manifests drop `yunta-engine`, `yunta-storage` and `yunta-adapters`; a CLI test that drives the engine directly takes it as a dev-dependency.

**Fails first:** both tests, against the manifests as they are: they name the three crates.

**Acceptance criteria:**
- [ ] Both tests fail before the drop, for the crates named, and pass after.

**Verification:**
- [ ] Tests pass: both targets, then `suite.py cli-1..5`, `cargo test -p yunta-mcp`.

**Dependencies:** Task 40

**Files likely touched:** `crates/cli/{Cargo.toml,tests/no_engine_in_the_cli.rs}`, `crates/mcp/{Cargo.toml,tests/no_engine_in_the_control_plane.rs}`

**Estimated scope:** S

### Task 42: what reads the process, and what the API shows, held

**Description:**
- The smell `process_read_below_the_shell` counts, outside `crates/cli/src/shell.rs` and the tests, `current_dir`, `current_exe`, `env::args` and a signal handler; its baseline is zero.
- `system_clock_outside_boundary` allows `shell.rs` in place of `context.rs`; `env_read_outside_boundary` holds `crates/api` as every crate.
- A test of the API reads its public items and fails on one that names an engine, store or adapter type.

**Fails first:** the smell's own test, a site with `current_dir` in a fixture outside the shell, red until the counter exists; the API's test against a public item planted in a fixture.

**Acceptance criteria:**
- [ ] `cargo xtask smells --check` passes with the new counter at zero.
- [ ] The API's test passes over the crate.

**Verification:**
- [ ] Tests pass: `cargo test -p xtask`, `cargo test -p yunta-api`.

**Dependencies:** Task 41

**Files likely touched:** `xtask/src/{smells.rs,smells/sites.rs,smells/baseline.rs}`, xtask's smell tests, `crates/api/tests/public_items.rs`

**Estimated scope:** M

### Task 43: the docs say where each thing is

**Description:**
- `.agents/architecture.md`'s table changes `yunta`'s row; each moved module's `//!`; render's and core's crate maps; `AGENTS.md`'s map rows for the control plane and the ports.
- `CAPABILITY-MAP.md`'s `app-core` row gains the chronicle's words, for `yunta-render`.

**Fails first:** `cargo xtask docs --check` against the stale rows.

**Acceptance criteria:**
- [ ] `cargo xtask docs --check` passes; `cargo xtask docs --about` over the diff prints nothing left untrue.

**Verification:**
- [ ] Manual check: each map's row names a file that exists.

**Dependencies:** Task 42

**Files likely touched:** `.agents/architecture.md`, `AGENTS.md`, `CAPABILITY-MAP.md`, `crates/{core,render,api,mcp,mcp-protocol}/src/lib.rs`

**Estimated scope:** M

### Group check: the frontier
- [ ] Both manifest tests, the smell and the API's test hold.

### Task 44: the proof

**Description:**
- The suite's count against the base's, by name, each moved test found where it went.
- Every golden's digest; the captures, the documents and the MCP answers from the final release build; its size against the base's.

**Fails first:** none: measurements.

**Acceptance criteria:**
- [ ] Every difference from the base is a move, explained in `after.txt`; nothing printed changes.

**Verification:**
- [ ] Manual check: `suite_diff.py`, `goldens.py --against`, `normalize.py base-1 final`.

**Dependencies:** Task 43

**Files likely touched:** `.plans/yunta-tui/proof/control/` (outside git)

**Estimated scope:** S

### Task 45: a real run at the end

**Description:**
- The same fragua run as Task 3 from the final release binary.

**Fails first:** none: a measurement.

**Acceptance criteria:**
- [ ] Its `between_secs` sits inside the base's spread.

**Verification:**
- [ ] Manual check: `yunta stats <run>` beside the base's.

**Dependencies:** Task 44

**Files likely touched:** `.plans/yunta-tui/proof/control/real-final*` (outside git)

**Estimated scope:** S

### Task 46: the review

**Description:**
- The module's diff read in four parts (core and render; the API; the CLI; the MCP crates, xtask and docs) by readers of their own on Haiku, with `code-review-and-quality`'s axes and the standing directive.
- The lead checks each finding against the code, the user decides each that stands, and the fixes go in before the commit.

**Fails first:** each fix that changes behavior, red first.

**Acceptance criteria:**
- [ ] Every finding is recorded here with its decision.

**Verification:**
- [ ] Tests pass: the suite's groups after the fixes.

**Dependencies:** Task 45

**Files likely touched:** as the findings say

**Estimated scope:** M

### Checkpoint D: the module
- [ ] The release binary's code against Checkpoint A's, by crate and by copy (`tools/size_by_crate.py`, `tools/duplicates.py`, both built with symbols): found after Phase 2, when the static Linux binary passed its 40 MiB ceiling, that a release build shares no generics, so each crate that named the generic YAML reader on a manifest or a config layer built its own copy of everything that reads them; since then core reads each through one concrete reader (`Manifest::read_frozen`, `ConfigLayer::read`), and the binary went from 32.16 MB to 28.95 MB. Every document more than one crate reads is read through one such reader, and the binary stays at or under that size.
- [ ] The user reviews the proof and the review.
- [ ] The phase commits and is pushed; Task 47 follows, then `cargo clean`.

### Task 47: the pull request

**Description:**
- PR #30's body gains the module; CI on Linux and macOS.

**Fails first:** none.

**Acceptance criteria:**
- [ ] CI green on Linux and macOS.

**Verification:**
- [ ] Manual check: `gh pr checks 30`.

**Dependencies:** Checkpoint D

**Files likely touched:** PR #30's body

**Estimated scope:** XS
