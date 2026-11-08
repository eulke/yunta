# Tasks: `run-pages`

Spec: `SPEC-run-pages.md`. Plan: `tasks/plan.md`. Status: **approved on 2026-11-07, amended on 2026-11-08** (Tasks 18c to 18f, the plan's decision 5). Base: `13cdd69c`.

**How these run:**
- One at a time, with `incremental-implementation` and `test-driven-development`, stopping at each checkpoint for the user.
- `cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.
- Before editing a file, `cargo xtask docs --about <files>`; each rule, trap and note it prints is kept true.
- A commit closes each phase, once the user approves its checkpoint and every step of the `ci` job passes. Paths are staged by name. Then `cargo clean`.
- The engine's and the CLI's suites run in foreground batches: `python3 .plans/yunta-tui/tools/suite.py <group>`.

**How a page or an item changes:**
- Its names and its form change first, in place, with every caller. Then it moves, with every caller's path.
- Neither step leaves an alias, a re-export or an old name, and the workspace compiles and passes after each task.
- A task that moves a file does it with `git mv` before it edits it. A file whose page is only part of it is split: the page goes, and the rest stays.
- A task that renames or moves is larger than five files: one path to each item means a name and its callers change in one step. Each such step is mechanical, the compiler names every caller, and the moved bodies are read with `git diff -M` against their old files.

**What fails first:**
- A rename or a move has no new behavior, so no test can fail for it. Its guard is every test and golden it touches, passing unchanged, and the digests and captures of Task 2.
- A new test or a changed text is seen red, for its own cause, before it passes.

## Phase 0: the baseline

### Task 1: the suite's runners kept with the plan

**Description:**
- The scratchpad's `rs_gate_suite.py` and `rs_suite_diff.py` are copied into `.plans/yunta-tui/tools/` as `suite.py` and `suite_diff.py`. `suite.py` takes its log directory as a second argument.
- `rs_engine_batch.py` is not copied: `suite.py`'s `engine-1` to `engine-5` run the same fifths.
- macOS clears an idle scratchpad; the plan's tools survive it.

**Fails first:** none: tools.

**Acceptance criteria:**
- [x] `python3 .plans/yunta-tui/tools/suite.py crates` runs from the plan's folder alone: 83 binaries, 2,162 passed, the gate's own count, and `suite_diff.py` finds the same names.

**Verification:**
- [x] Manual check: both files exist, and neither names the scratchpad.

**Dependencies:** None

**Files likely touched:** `.plans/yunta-tui/tools/{suite,suite_diff}.py` (outside git)

**Estimated scope:** XS

### Task 2: what the base prints, against itself

**Description:**
- `goldens.py`: the SHA-256 of every golden, 78 under `crates/cli/goldens/` and 21 under `crates/render/goldens/`, keyed by file name and directory so a moved file keeps its key.
- `pages.sh`, built on `tools/capture.sh`'s sandbox, `YUNTA_HOME` and mock fixtures:
  - every view `capture.sh` takes;
  - `status last --node <node>`;
  - `stats last --json`;
  - `stats --workflow agent-demo`, and with `--json`;
  - `list`, `schema`, `schema tasks`, `verify last`, `open last` (with `--json`) and `pack list`;
  - `yunta stats` with no argument;
  - `run` on a workflow that fails `yunta check`;
  - `run --adapter mock --detach`;
  - `init`, in a repository of its own, and the skill it writes;
  - last, what changes the sandbox: `new`, `close last` and `gc --dry-run`.
  The texts Task 11 changes are among them: the bare `stats`, the refused run, the `--detach` refusal and the skill.
- A release build of the base. `pages.sh` runs twice on it, and `normalize.py` masks exactly what the two captures differ in: run ids, instants, durations and the sandbox's path.
- The release binary's size.
- All of it goes in `.plans/yunta-tui/proof/run-pages/`.

**Fails first:** none: a measurement.

**Acceptance criteria:**
- [x] `digests-base.txt` lists 99 files.
- [x] The two base captures are equal once normalized, and `normalize.py`'s masks are each justified by a difference between them.
- [x] The size and the commands that made each file are written down.

**Verification:**
- [x] Manual check: `python3 normalize.py base-1 base-2` prints no difference.

**Dependencies:** Task 1

**Files likely touched:** `.plans/yunta-tui/proof/run-pages/{goldens.py,pages.sh,normalize.py,digests-base.txt,base-1/,base-2/,size-base.txt}` (outside git)

**Estimated scope:** M

### Checkpoint A: after Tasks 1–2
- [ ] The user sees the digests, the captures and the size. The base's suite counts 274 binaries and 4,531 tests.

## Phase 1: what the pages stand on

### Task 3: render's purity test, and its edge to replay

**Description:**
- `crates/render/tests/pure.rs`, `the_crate_reads_nothing_but_its_inputs`, as `crates/replay/tests/pure.rs` reads its own: the manifest names no `tokio`, store or `yunta-engine`.
- `yunta-replay` joins render's `[dependencies]`, and its `testkit` feature joins the dev-dependencies.
- `.agents/architecture.md`'s row for render: core, replay, surco.
- Render's `//!` says what it draws from.

**Fails first:** with `yunta-engine` added to render's manifest by hand, `cargo test -p yunta-render --test pure` fails, naming it. The probe is taken back out.

**Acceptance criteria:**
- [x] `cargo test -p yunta-render --test pure` passes with the edge in place.
- [x] `cargo tree -p yunta-render -e normal` names neither the engine nor storage.
- [x] `cargo xtask docs --check` passes with the new row.

**Verification:**
- [x] `cargo test -p yunta-render`, `cargo xtask docs --check`, `cargo deny check`

**Dependencies:** Checkpoint A

**Files likely touched:** `crates/render/{Cargo.toml,tests/pure.rs,src/lib.rs}`, `Cargo.lock`, `.agents/architecture.md`

**Estimated scope:** S

### Task 4: `DecisionOn` beside `Escalation`

**Description:**
- `DecisionOn` and its methods move from `crates/engine/src/run/escalation.rs` to `crates/core/src/events/gates/escalation.rs`.
- It is named `yunta_core::events::DecisionOn`, and the engine's `lib.rs` exports it no more.
- Its callers change path: 11 code files and 5 test files, among them the engine's `run/mod.rs`, the CLI's status, `resolve_gate`, `test/case`, `json` and closing, and the engine's `failed_node_retry`, `parked_answers` and `project_commands`.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] A search for `yunta_engine::DecisionOn` and `run::escalation::DecisionOn` finds nothing.
- [x] The engine's `run/escalation.rs` builds the escalation and no longer defines whose it is.

**Verification:**
- [x] `cargo test -p yunta-core`, `cargo test -p yunta --bins`, and the engine targets it touches (`failed_node_retry`, `parked_answers`, `project_commands`); the whole engine runs at Checkpoint B, once for the phase, since Tasks 4 to 6 change paths the compiler checks

**Dependencies:** Task 3

**Files likely touched:** `crates/core/src/events/gates/escalation.rs`, `crates/engine/src/{lib.rs,run/escalation.rs,run/mod.rs}`, and the callers above

**Estimated scope:** L (a move)

### Task 5: what a gate shows, named from core

**Description:**
- `ShownDocument` and `ShownContent` are core's (`yunta_core::shown`). The engine's `human_interaction.rs` re-exports them, and that re-export goes.
- Their 8 CLI code files and 7 engine test files name them from core.
- The engine's own modules already reach them through `crate::human_interaction`; they now name `yunta_core::shown`.

**Fails first:** none: a path.

**Acceptance criteria:**
- [x] A search for `ShownDocument` or `ShownContent` after `yunta_engine::` or `human_interaction::` finds nothing.

**Verification:**
- [x] `cargo test -p yunta --bins`, the engine targets it touches; the whole engine at Checkpoint B

**Dependencies:** Task 4

**Files likely touched:** `crates/engine/src/{human_interaction.rs,artifacts/shown.rs}`, `crates/cli/src/{ask/decision.rs,commands/resolve_gate.rs,commands/review.rs,commands/status/{mod,decision}.rs,json/mod.rs,run_page/mod.rs,surface/closing/mod.rs}`, `crates/engine/tests/{common/mod,deviation,gate_findings,gate_review,plan_answered,shapes_read,spec}.rs`

**Estimated scope:** L (a path)

### Task 6: the run directory a reader resolves, in core

**Description:**
- `crates/core/src/run_dir.rs`, with its `//!`, holds what a reader finds under a run's directory:
  - `ARTIFACTS_DIR` and `OBJECTS_DIR`, from `workflow/artifacts.rs`;
  - `object(run_dir, hash)`, the body of `ObjectStore::path_of`;
  - `view(node, name)`, from the engine's `artifacts/store.rs` `view_path`;
  - `view_of(shown)`, from `artifacts/shown.rs`;
  - `artifacts(run_dir)`, from `run_dir::artifacts_view`.
- The engine's `ObjectStore` and its other callers call them, and the engine's four functions are gone. The engine's private `ingest::view_path` is another function; it keeps its name only if it still reads as different once `run_dir::view` exists, and is otherwise renamed for what it returns.
- Core's root and `workflow/mod.rs` export the two constants no more. Each item has one path: `yunta_core::run_dir::…`.
- The engine's `run_dir.rs` `//!` says it holds the working space, and that what a reader resolves is core's.
- 17 files name a constant and 15 a function.

**Fails first:** none: a move. Three unit tests in `run_dir.rs` pin `object`, `view` and `artifacts` against literal paths, written with the move: the guard that the layout did not change. Found on the way: `frozen/read.rs` and `frozen/store.rs` also asked the store for an object's path (`stored` now takes the run's directory); an engine test writes objects with `ObjectStore`, so the store stays exported; `ingest`'s private `view_path` returns the view as a diagnostic's text and keeps its name beside `run_dir::view`.

**Acceptance criteria:**
- [x] `crates/engine/src` defines none of the four functions, and a search for `artifacts_view`, `view_path(` (but `ingest`'s) and `ObjectStore::at(..).path_of` outside the engine finds nothing.
- [x] Every test and golden passes unchanged.

**Verification:**
- [x] `cargo test -p yunta-core`, `-p yunta-render`, `-p yunta --bins`, the engine targets it touches, `cargo xtask docs --check`; the whole engine at Checkpoint B

**Dependencies:** Task 5

**Files likely touched:** `crates/core/src/{run_dir.rs,lib.rs,workflow/{mod,artifacts}.rs}`, `crates/engine/src/{run_dir.rs,artifacts/{store,shown,mod,canonical,ingest}.rs}`, and the callers listed by `rp_users.py`

**Estimated scope:** L (a move)

### Task 7: `NextStep` loses the derives nothing uses

**Description:** `Serialize`, `Deserialize` and `JsonSchema` go from `NextStep`: no payload, schema or JSON document holds one.

**Fails first:** none.

**Acceptance criteria:**
- [x] The workspace builds, and `cargo xtask schema --check` passes with `crates/core/schemas/` unchanged.

**Verification:**
- [x] `cargo check --workspace --all-targets`, `cargo xtask schema --check`

**Dependencies:** Task 3

**Files likely touched:** `crates/core/src/step.rs`

**Estimated scope:** XS

### Checkpoint B: after Tasks 3–7
- [x] `cargo test -p yunta-core -p yunta-render -p yunta-replay -p yunta-adapters`, the engine's batches, `-p yunta --bins`.
- [x] Every golden's digest equals the base's.
- [x] The user reviews the phase. Then every step of the `ci` job; commit 1, with `SPEC-run-pages.md`, `CAPABILITY-MAP.md`, `tasks/plan.md` and `tasks/todo.md`; `cargo clean`.

## Phase 2: every command, spelled once

### Task 8: the one module that writes a command

**Description:**
- `crates/render/src/command.rs`, with its `//!`:
  - `Reader`, moved from the CLI's `error.rs`, with `Reader::name_of`, the body of `advice::named`;
  - `line`, and `line_for`, the body of `advice::spell_for`, for the nine kinds `NextStep` holds.
- `advice::spell`, `spell_for` and `named` are gone.
  - Their callers call `command::line` or `line_for`.
  - Where a crate below the CLI still takes a spelling, the CLI hands it `&yunta_render::command::line` until Task 9 takes the parameter away.
- `crates/render/src/command/tests.rs` takes over four tests from `advice/tests.rs`:
  - `spell_names_a_run_by_its_handle_in_the_command_that_takes_it`;
  - `spell_writes_a_decision_with_its_option_and_the_answer_it_asks_for`;
  - `an_agent_is_handed_the_whole_run_id_and_the_text_flag_for_an_option_that_asks`;
  - `a_program_is_handed_the_whole_id_and_a_person_the_handle`.
  
  Each is renamed for `line`.
- `the_test_spelling_matches_the_cli_for_every_step` compares `command::line` with the testkit's copy until Task 9 deletes both.
- `NextStep` keeps its nine kinds here. Growing them waits for Task 10: the two copies that Task 9 deletes match every kind with no wildcard, and growing first would mean extending copies about to go.

**Fails first:** the four moved tests do not build until `yunta_render::command` exists.

**Acceptance criteria:**
- [x] `Reader` has one path, `yunta_render::command::Reader`, and `crate::error::Reader` is gone.
- [x] `advice` defines no `spell`, `spell_for` or `named`, and every printed line is the same.

**Verification:**
- [x] `cargo test -p yunta-render command`, `cargo test -p yunta --bins`, `cargo test -p yunta --test status_cmd --test run_flow`

**Dependencies:** Checkpoint B

**Files likely touched:** `crates/render/src/{lib.rs,command.rs,command/tests.rs}`, `crates/cli/src/{error.rs,error/hold.rs,output.rs,json/mod.rs,commands/{advice.rs,advice/tests.rs,mcp.rs,status/decision_json.rs}}`, and the 15 callers of `advice::spell`

**Estimated scope:** L

### Task 9: no surface takes a spelling

**Description:**
- `Spell` is gone, and every parameter that carried a spelling with it:
  - render's `gate`, `plan/{mod,desk,proof}`, `published`, `receipt`, `shown`, `spec` and `blocks/decision`, each calling `command::line` itself;
  - the adapters' forge: `GitHubForge::configure(repo, token)`, `GitHubForge::new(repo, token)`, `MockForge::new(state)`.
- Gone with it:
  - `yunta_testkit_core::spell` and `spell.rs`;
  - the copy in `crates/engine/tests/receipt.rs`;
  - `the_test_spelling_matches_the_cli_for_every_step`.
- Every test that handed a spelling passes none: render's 6 test files, the adapters' 4 and the engine's 7.
- The CLI's callers pass none: `ask/decision`, `receipt`, `review`, `status/{mod,node,decision}`, `forge` and `test/case`.

**Fails first:** none: a signature. The guard is render's 21 goldens, `pull-request.md` among them, and the adapters' forge tests.

**Acceptance criteria:**
- [x] A search for `Spell`, `spell:` and `spell(` in `crates/` finds nothing.
- [x] Render's goldens and the forge's published bodies are byte for byte the same.

**Verification:**
- [x] `cargo test -p yunta-render -p yunta-adapters`, the engine targets it touches, `cargo test -p yunta --bins`

**Dependencies:** Task 8

**Files likely touched:** the 8 render files, 6 render tests, `crates/adapters/src/forge/{mod,github,github_pulls,mock}.rs`, 4 adapters tests, 7 engine tests, `crates/testkit-core/src/{lib,spell}.rs`, `crates/engine/tests/receipt.rs`, the 8 CLI callers

**Estimated scope:** L (a signature)

### Task 10: every kind of `NextStep`, and the commands that name a run

**Description:**
- `crates/core/src/step.rs` takes plan decision 1's kinds, `Arg`, `Choice` and `Reach`. Its `//!` says `yunta_render::command` writes them.
  - `ResolveGate`'s `option` and `text` become `choice: Choice`, named apart from the CLI's `Answer` trait.
  - Its three builders change with it: `resolve_gate.rs`, `decision_json.rs` and render's `blocks/decision.rs`.
- `command::line_for` writes every kind, for both readers.
- `crates/cli/src/cli/tests.rs`, `every_command_line_parses`:
  - an exhaustive match lists every kind, so a new kind cannot skip it;
  - each line, for both readers, is parsed by `Cli::try_parse_from`;
  - `ShowCommit` is git's, so its line is held to `git show <commit>` instead.
- `advice::{status, status_node, follow, resume, cancel, close, receipt, verify, review, open, decide, resolve_gate}` are gone.
  - Their callers write `command::line(&NextStep::…)`.
  - These are `cancel`, `cancel/{page,registered}`, `close`, `receipt`, `resolve_gate`, `review`, `run_ref`, `status/{mod,node,follow,decision,decision_json}`, `list/{inbox,runs}`, `commands/mod.rs`, `error.rs`, `help.rs`, `run_page/{mod,evidence}.rs` and `surface/{chronicle/mod,view/mod}.rs`.
  - Inside `advice`, `after_for`'s steps are written with `command::line_for` until Task 15 moves it.

**Fails first:** `every_command_line_parses` does not build until the kinds exist. Then, with `Review` spelled `yunta reviews {run}` as a probe, it fails, naming that line. The probe is reverted.

**Acceptance criteria:**
- [x] Every kind is written by `line_for`, for both readers, and parsed by clap.
- [x] `advice` writes none of the commands that name a run, and every printed line is the same.

**Verification:**
- [x] `cargo test -p yunta-render command`, `cargo test -p yunta --bins`, `python3 .plans/yunta-tui/tools/suite.py cli-1` to `cli-5`

**Dependencies:** Task 9

**Found while doing it:**
- Two commands were written by gluing a flag on: `yunta status X --full` and `yunta doctor --full`. So `Status` carries its `depth` (surco's `Depth`), and `Doctor` its `full`.
- `NextStep::status(run)` builds the glance every surface points to most.
- `yunta status needs` is a real line, and `needs` is no run id, so it has its kind, `Needs`.
- The three `pack` texts the user chose to make whole lines need one kind per verb, with its own arguments: `PackAdd`, `PackUpdate`, `PackRemove`, `PackNew`. `Test` carries the `dir` a pack's README names.
- A decision's answer is `Choice`, named apart from the CLI's `Answer` trait, which `resolve_gate.rs` holds beside it.
- A line with a blank is not typed as written, and `<option>` is no valid option id. So the clap test fills each blank before it parses the line.
- In a test build, an id's `From<&str>`, which exists only for tests, panics. Clap prefers it to `FromStr`, so a test that parses an invalid id panics where production is refused cleanly.
- The live region (`painter`, `draw`, `lines`, the chronicle) hands down the run's id in place of its handle, so a quoted failure's step is data.

**Files likely touched:** `crates/core/src/step.rs`, `crates/render/src/{command.rs,command/tests.rs,blocks/decision.rs}`, `crates/cli/src/cli/{mod,tests}.rs`, the callers above, `crates/cli/src/commands/advice.rs`

**Estimated scope:** L

### Task 11: every other command, as kinds

**Description:**
- `advice::{gc, schemas, schema_workflow, test, doctor, init, pack, list, list_runs, list_every_run, list_closed_runs, help, run, show_commit}` are gone.
  - Their callers write `command::line(&NextStep::…)`.
  - These are `init`, `init/skill`, `help`, `list/{mod,runs}`, `run_ref`, `error.rs`, `error/page.rs`, `new`, `schema`, `pack`, `pack/scaffold`, `test/mod`, `commands/mod.rs`, `run`, `doctor/mod`, `close`, `gc/page` and `run_page/story`.
- `doctor/mod.rs`'s `format!("{doctor} --session")` becomes `NextStep::Doctor { sessions: true }`.
- Four sites keep their `advice` call for Task 12, which changes what they say:
  - `stats/mod.rs`'s two refusals (`advice::stats`);
  - `error/page.rs`'s `unrunnable` (`advice::check`);
  - `run.rs`'s `--detach` refusal (`advice::resume_any`);
  - the skill's `yunta schema <kind>` (`advice::schema`).

**Fails first:** none. The guard is the goldens, the captures, and `every_command_line_parses`.

**Acceptance criteria:**
- [x] `advice` holds no command but `stats`, `check`, `resume_any`, `schema` and `pack`, each with the callers Task 12 rewords: the three texts of `yunta pack` keep `advice::pack` until then.
- [x] Every printed line is the same.

**Verification:**
- [x] `cargo test -p yunta --bins`, the CLI's batches

**Dependencies:** Task 10

**Files likely touched:** the callers above, `crates/cli/src/commands/advice.rs`

**Estimated scope:** L (a rename)

### Task 12: the seven texts a better command replaces

**Description:**
- **`yunta stats` with no argument.**
  - Clap refuses it: the run is required unless `--workflow` is present, beside the `conflicts_with` clap already holds.
  - `stats/mod.rs` loses its two hand-written refusals and its `(None, None)` and `(Some, Some)` arms. The second was unreachable, since clap already refused both arguments together.
- **The refused workflow.** `error::page::unrunnable` takes the refused workflow and says `the workflow fails `yunta check <path>``, through `NextStep::Check`. Its callers in `commands/registry.rs` and `commands/run.rs` hand it the path they refused.
- **The `--detach` refusal.** `--adapter mock` with `--detach` says it "hands the run to another process that resolves the adapters `runners:` names and reads no fixture".
- **`yunta schema <kind>` becomes `yunta schema <document>`,** through `NextStep::Schema { document: Arg::Blank, .. }`:
  - in the skill `init` writes, where "to list the kinds" becomes "to list them";
  - in `docs/concepts.md`, `docs/guide.md`, `docs/reference.md`, `docs/troubleshooting.md` and `docs/compatibility.md`, where they name that command.
- `advice::{stats, check, resume_any, schema}` are gone with their last callers.

**Fails first:** each text's test is changed first and seen red, against the old text:
- `stats_cmd`: `yunta stats` alone exits 2 with clap's usage error;
- `context_files_cmd` or the verdict's test: the refusal of a workflow that fails `check` names its path;
- the `--detach` refusal's words;
- `init`'s test: the written skill names `yunta schema <document>`.

**Acceptance criteria:**
- [x] Each of the seven prints its new text, and its test passes; no other capture of Task 2 changes.
- [x] `advice` holds no command, and no doc says `yunta schema <kind>`.
- [x] `cargo test -p yunta --test docs_sync` passes.

**Verification:**
- [x] `cargo test -p yunta --test stats_cmd --test docs_sync --test context_files_cmd`, the CLI's batches, `pages.sh` against the base, normalized

**Dependencies:** Task 11

**Found while doing it:**
- A resume refuses a frozen run that has no file of its own. So `Check` carries `Arg<String>`: the refused file where the refusal knows one (the verdict's `Unrunnable`, a `run` of a file through `preflight` and `servable`), and `yunta check <workflow>` where it does not (`resume`).
- `yunta stats` calls `stats::of_run` or `stats::of_workflow`, named by what the dispatch was handed. The pair clap refuses returns a typed error: the CLI denies `clippy::unreachable`, and a panic is a bug.
- The smells caught what rustfmt's wrapping of each `NextStep` grew, and each was split along a seam:
  - `line_for` writes status, resolve-gate, listings, schema and doctor through helpers;
  - `after_for` shares `goes_on` (resume or close), and the way to a parked child moved into `after_needs_you`;
  - a cancel's `closes` and `goes_on` are methods;
  - `new`'s step that runs the workflow is `runs_it`;
  - `CliError::sentence` is the match `said_to` held;
  - the clap test's samples stand in four lists.
- `list/inbox.rs` and `resolve_gate.rs` passed 500 lines, and their inline tests moved to `tests.rs`. `pack.rs`'s two refusals moved to `pack/page.rs`, which says what a pack command says.

**Files likely touched:** `crates/cli/src/{cli/mod.rs,commands/stats/mod.rs,error/page.rs,commands/registry.rs,commands/run.rs,commands/init/skill.rs,commands/advice.rs}`, `crates/cli/tests/{stats_cmd,context_files_cmd}.rs`, the five docs

**Estimated scope:** L

### Task 13: the smell reaches every crate that can spell through render

**Description:**
- `count_commands` in `xtask/src/smells/language.rs` counts `shipped(root)` but `crates/core` and `crates/surco`, and skips `crates/render/src/command.rs` and `crates/cli/src/cli/`.
- The smell is renamed `command_outside_command_rs`:
  - in `language.rs` and `smells.rs`;
  - in the fix of `smells/baseline.rs`, "spell the command in yunta-render's command module";
  - in `xtask/smells.baseline`'s key, at 0;
  - in the rule at `.agents/architecture.md:43`.
- The counter's `///` says why core and surco are left out: they sit below render and cannot name its module.
- `render::pause::make_room` stops writing `yunta gc` and calls `command::line(&NextStep::Gc)`; it takes no argument, and `doctor` calls it so.

**Fails first:** with the counter widened, `cargo xtask smells --check` fails at 1, on `crates/render/src/pause.rs:131`. Then `make_room` changes, and it passes at 0.

**Acceptance criteria:**
- [x] `cargo xtask smells --check` passes, and `xtask/smells.baseline`'s diff renames one key at 0 and changes no other value.
- [x] `cargo xtask docs --check` passes.

**Verification:**
- [x] `cargo test -p xtask`, `cargo xtask smells --check`, `cargo xtask docs --check`, `cargo test -p yunta --test docs_sync`

**Dependencies:** Task 12

**Files likely touched:** `xtask/src/{smells.rs,smells/language.rs,smells/baseline.rs}`, `xtask/smells.baseline`, `crates/render/src/pause.rs`, `crates/cli/src/commands/doctor/mod.rs`, `.agents/architecture.md`

**Estimated scope:** M

### Checkpoint C: after Tasks 8–13
- [x] The suites, the adapters' forge tests, the CLI's integration tests, `smells --check` at 0, `docs --check`.
- [x] Every golden's digest equals the base's; the captures equal the base's but the seven texts.
- [x] The user reviews the phase. Then every step of the `ci` job, commit 2, `cargo clean`.

## Phase 3: what a run offers next

### Task 14: what a parked phase waits on, read by replay

**Description:**
- `RunPhase::waiting_on` and `RunPhase::reset_wait`, in `crates/replay/src/frame/mod.rs`, are `advice::parked` and `advice::reset_pause`, moved.
- Their callers call the methods: `status/mod.rs`, `json/mod.rs` and `surface/view/mod.rs`.
- `a_run_paused_on_a_reset_names_its_node_and_the_moment_it_resumes` moves from `advice/tests.rs` to replay's frame tests.

**Fails first:** the moved test does not build until `RunPhase::reset_wait` exists.

**Acceptance criteria:**
- [x] `advice` defines neither function, and the moved test passes.

**Verification:**
- [x] `cargo test -p yunta-replay`, `cargo test -p yunta --bins`

**Dependencies:** Checkpoint C

**Files likely touched:** `crates/replay/src/frame/mod.rs`, `crates/replay/tests/frame.rs`, `crates/cli/src/{commands/advice.rs,commands/advice/tests.rs,commands/status/mod.rs,json/mod.rs,surface/view/mod.rs}`

**Estimated scope:** M

### Task 15: the actions a run offers

**Description:**
- `crates/replay/src/frame/actions.rs`: `Action`, `Effect` and `RunFrame::actions`. `after_for`'s rules move whole: `child_parked_on`, `after_needs_you`, `shows_plan`, `waits_for_tree` and `after_reset` become its private parts.
- `crates/render/src/command.rs`: `effect`, the words of each `Effect`, the same 11 sentences.
- The run page (`after` twice) and the JSON (`after_for` with an agent) take `frame.actions(decision)`, spelled with `command::line` or `line_for` beside `command::effect`.
- `advice::{after, after_for, after_reset}` are gone.
- `advice/tests.rs`'s tests of what a run is offered move to `crates/replay/tests/frame/actions.rs`, beside the frame's other tests, asserting `Action`s: `RunFrame::sample` exists under the `testkit` feature, which replay's integration tests enable. A sixth test pins a finished, a broken and a moving run, which no test held. Render's `command::effect` words each effect and `command::said` writes an action for a reader, each with its test.

**Fails first:** the six moved tests do not build until `RunFrame::actions` exists, and each then passes for the rule it names.

**Acceptance criteria:**
- [x] A search for `after_for`, `after_reset` and `advice::after` finds nothing.
- [x] `status --json`'s next steps and every status and closing golden are byte for byte the same.

**Verification:**
- [x] `cargo test -p yunta-replay -p yunta-render`, `cargo test -p yunta --bins`, `cargo test -p yunta --test status_cmd --test run_flow`, the parked-runs targets, `pages.sh` against the base

**Dependencies:** Task 14

**Files likely touched:** `crates/replay/src/frame/{mod,actions}.rs`, `crates/replay/tests/{frame.rs,frame/actions.rs}`, `crates/render/src/command.rs`, `crates/cli/src/{run_page/mod.rs,json/mod.rs,commands/advice.rs,commands/advice/tests.rs}`

**Estimated scope:** M

### Checkpoint D: after Tasks 14–15
- [x] The suites; every golden's digest; the JSON captures byte for byte.
- [x] The user reviews the phase. Then every step of the `ci` job, commit 3, `cargo clean`.

## Phase 4: one decision, and the words of a parked run

### Task 16: a decision's record and routes, in `escalation`

**Description:**
- `gate::decision`, `gate::record`, `gate::routes` and `Route` move from `crates/render/src/gate.rs` to `crates/render/src/escalation.rs`.
- Their callers change path: render's `gate` and `published`, the CLI's `ask/card`, `ask/decision`, `resolve_gate` and `human_interaction`, and their tests (`render/src/tests/{deciding,routes}.rs`, `cli/src/ask/card/tests.rs`).

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] `gate` defines none of the four, and every gate and decision golden is byte for byte the same.

**Verification:**
- [x] `cargo test -p yunta-render`, `cargo test -p yunta --bins`

**Dependencies:** Checkpoint D

**Files likely touched:** `crates/render/src/{gate,escalation,published}.rs`, `crates/render/src/tests/{deciding,routes}.rs`, `crates/cli/src/{ask/card.rs,ask/card/tests.rs,ask/decision.rs,commands/resolve_gate.rs,human_interaction.rs}`

**Estimated scope:** M (a move)

### Task 17: one decision block

**Description:**
- `crates/cli/src/commands/status/decision.rs` moves, with `git mv`, to `crates/render/src/escalation/decision.rs`, with its tests.
- `run_page::Decided` becomes `escalation::Decision` there (plan decision 4).
- `section`, `claim_words` and `run_tree_line` are named from `escalation`, and the gate's page draws the decision through the same block.
- Its callers change path: `decision_json`, `run_page/mod`, `run_page/fixtures`, the closing and `advice::parked_claim`.
- Its tests draw with Surco's `Terminal` instead of the CLI's `surface_lines`.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] `crates/cli/src/commands/status/decision.rs` is gone, and so is `Decided`.
- [x] The status, closing and gate goldens and the decision's JSON are byte for byte the same.

**Verification:**
- [x] `cargo test -p yunta-render`, `cargo test -p yunta --bins`, `cargo test -p yunta --test status_cmd`

**Dependencies:** Task 16

**Found while doing it:**
- `Decision` borrows what it draws, as the plan writes it, so the closing lends the page its decision without a clone. `yunta status` reads the documents a page shows once, in `Reading::documents`: a node's page shows what the node produced, and the run's page what its decision shows.
- What the page says beside the decision is the page's to say: the run page sets `Beside` from the failed nodes whose evidence it quotes, which the closing already did, in place of each caller computing it.
- The section every decision ends a page on is `escalation::section`, since `escalation::Decision` now names the decision itself. `Decision::section` builds the run's on it, and the published page builds its own.
- `claim_words` sits beside `escalation::sentence`, which says the same claim and now calls it.
- The run page fixtures keep one decision, `lint_decision`, which the closing's tests share.

**Files likely touched:** `crates/render/src/{escalation.rs,escalation/decision.rs,gate.rs}`, `crates/cli/src/{commands/status/{mod,decision_json}.rs,run_page/{mod,fixtures}.rs,surface/closing/mod.rs,commands/advice.rs}`

**Estimated scope:** M (a move)

### Task 18: the words of a parked run, in `pause`, and `advice` gone

**Description:**
- `STALLED`, `asking`, `asking_on`, `parked_on`, `parked_claim` and `parked_in_full` move to `crates/render/src/pause.rs`.
- Their callers change path: `list/inbox`, `status/progress`, `error.rs`, `run_page/mod`, `surface/lines` and `surface/view/mod`.
- What is left of `advice/tests.rs` moves to `crates/render/src/pause/tests.rs`.
- `crates/cli/src/commands/advice.rs` and `advice/` are deleted, and `commands/mod.rs` declares no `advice`.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] A search for `advice` in `crates/` finds no module, path or test.
- [x] Every golden is byte for byte the same.

**Verification:**
- [x] `cargo test -p yunta-render`, `cargo test -p yunta --bins`
- [x] the CLI's batches, at Checkpoint E

**Dependencies:** Task 17

**Found while doing it:**
- `advice/tests.rs` was already gone: Task 15 took its tests to replay. `pause`'s own tests move to `pause/tests.rs`, which keeps `pause.rs` clear of the 500-line smell.
- `yunta-render` names no clock type, and the spec adds no crate to it. So this task left a person asked at a terminal said by two functions named by what they take: `asking(prompt)`, and `engine_asking(engine)`, which subtracted the prompt's start from the engine's reading. At the user's request, Tasks 18c to 18f do it right: the frame measures, the registry reads as one `Engine` enum, and render's `engine` names each length.
- `parked_runs/child.rs` is reached by `RunFrame::actions` now, and its `Reached by:` line says so.

**Files likely touched:** `crates/render/src/{pause.rs,pause/tests.rs}`, `crates/cli/src/{commands/mod.rs,commands/advice.rs,commands/advice/tests.rs,commands/list/inbox.rs,commands/status/progress.rs,error.rs,run_page/mod.rs,surface/lines.rs,surface/view/mod.rs}`

**Estimated scope:** M (a move)

### Task 18b: a follower hears what a session said before it listened

Added at the user's request, after the measurement of Checkpoint C.

**Description:**
- **What fails.** `row::a_follower_listens_once_the_registry_names_a_socket` (`crates/cli/tests/run_words/row.rs`) fails about one run in ten alone, and three in ten beside a build. The rate is the same at `737499bd`, before this module.
- **What the failing screen shows.** The node is running for six seconds, and no `says:` row is drawn. So the follower follows the log, but what the session said never reached its region.
- **Measured, not the cause.** A late listener does not lose the words by the socket: `said::Voices` hands a new reader the recent past before what comes next.
- **The hypothesis to prove or drop first.** The past a follower hears when it connects reaches the region through its observer. That can happen before the region has taken the `node_started` its words belong to, and words for a node the region does not hold yet are dropped.
- **The work:**
  - reproduce the miss on purpose, by holding the region's events back while the socket's past arrives, and so name the cause;
  - fix it where it lives, so a follower that hears a session's words early keeps them until their node is drawn, or hears them only once the node is;
  - make the test hold every time.
- **What the work reads:** `follow.rs`, `follow/listen.rs`, the surface's observer and region (`crates/cli/src/surface/`), and `said`'s `Voices`.

**Fails first:** a test that puts a session's words before its node's start, the way the follower can, and finds them drawn. It fails before the fix, for the cause named.

**Acceptance criteria:**
- [x] The cause is named in the task, with the run that shows it.
- [x] `row::a_follower_listens_once_the_registry_names_a_socket` passes 30 runs out of 30 with `tools/repeat_test.py`, alone and beside a build.
- [x] The entry "A follower can miss what a session said before it listened" is deleted from `.agents/open.md`.

**Verification:**
- [x] `python3 .plans/yunta-tui/tools/repeat_test.py <worktree> 30 yunta run_words row::a_follower_listens_once_the_registry_names_a_socket`, `cargo test -p yunta --test run_words --test status_follow`, `cargo test -p yunta --bins`

**Dependencies:** Task 18

**Found while doing it:**
- **The cause.** The region let go of what a node said whenever the frame it redrew did not have the node working, and a node whose start the follower has not read yet is not working. A follower hears the socket's past through the surface's observer as soon as a registry reading names the socket, but reads the node's start off the log at its own pace, up to a tenth of a second later. When the words come first, the redraw between them drops the words, and the stub's session says nothing more: the failing screen of Checkpoint C, a node running for six seconds with no `says:` row.
- **The reproduction.** `surface::region::tests::what_a_node_said_before_its_start_is_shown_once_it_works` hears the words while the node is still to go, then shows it working. It failed for that cause before the fix.
- **Measured after the fix:** 0 of 30 failed alone, and 0 of 30 beside a release build of the CLI in a target of its own, which ran through all 30. Before it: 1 in 10 alone, 3 in 10 beside a build.
- **The fix lives in the region.** It keeps a node's words while the frame has not started the node, and still lets them go once the node stops or when they are another run's. The follower and the socket stay as they are: `said::Voices` already hands a late listener the recent past.

**Files likely touched:** `crates/cli/src/commands/status/{follow.rs,follow/listen.rs}`, `crates/cli/src/surface/{region,view}/*`, `crates/cli/tests/run_words/row.rs`, `.agents/open.md`

**Estimated scope:** M

### Task 18c: the frame measures what a page says of time

Added at the user's request on 2026-11-08, with Tasks 18d to 18f (plan decision 5).

**Description:**
- `crates/replay/src/registry.rs` gains `Asking { pid, prompt, waited }`: the process driving a run asking a person at its terminal, measured at an instant (plan decision 5).
  - `Engine::asking(at)` builds it, and nothing else does: the pid and the question the reading holds, and how long since the prompt's `since`, zero for a prompt stamped after `at`.
  - Task 18e takes the pid from `Alive` instead, where it belongs.
- `RunFrame` gains three fields, measured at its `now`:
  - `asking`: `engine.asking(now)`;
  - `since_last_event`: from the log's last event to `now`, zero for an event stamped after it, `None` for a log with none;
  - `time`: where the run's time went, to `now` while the run is open and to its last event once it has ended, as `state.time_spent` gives it.
- `RunFrame::sample` sets them: no prompt, no event, no time.
- Nothing reads them yet. The pages read them in Task 18d.

**Fails first:** `crates/replay/tests/frame/measures.rs`, new, beside `standing.rs`, fails because the frame carries none of the three. It reads:
- a frame read 3m10s after its engine's prompt began, and 2m after its last event, carries an `asking` with the engine's pid and a `waited` of 3m10s, and `since_last_event` of 2m;
- one read before its last event's stamp has stood zero;
- one whose engine asks no one has no `asking`;
- its `time` is `state.time_spent` to the same instant.

**Acceptance criteria:**
- [x] Each measure is the subtraction it replaces, read at the frame's `now`.
- [x] Every golden is byte for byte the same.

**Verification:**
- [x] `cargo test -p yunta-replay`, `cargo test -p yunta --bins` (with Task 18d's), and `cargo check --workspace --all-targets` with no warning

**Dependencies:** Task 18b

**Found while doing it:**
- `measures.rs` failed for want of the three fields, and passes with them: seven readings, two of them of time known apart from the code (an open run's working time grows by the gap between two reads, and an ended run's does not grow).
- `time` is the frame's own stats' `time`, which `RunStats::at` already reads to `now` while the run is open and to its last event once it has ended.

**Files likely touched:** `crates/replay/src/{registry.rs,frame/mod.rs,frame/sample.rs}`, `crates/replay/tests/{frame.rs,frame/measures.rs}`

**Estimated scope:** S

### Task 18d: every page reads the frame's measures

**Description:**
- The pages read what the frame measured, and none reads the engine's reading:
  - the run page: its headline's `open_for` and `stood` read `asking.waited` and `since_last_event`, its gist reads `asking`, and its facts read `frame.time`. Its `engine` field and `since` are gone.
  - the field: `held`, `said` and the row's `asking` read `frame.asking`, and `time_in_state` is gone for `since_last_event`.
  - the status summary: `summary(frame, glyphs)` and `phase_label` read `frame.asking`.
  - the JSON: `RunDocument::of` takes no engine. `waiting_on` reads `frame.asking`, and `time` reads `frame.time`.
  - the live region: the painter hands the region `frame.asking`, and its row says `asking_on` of it.
- The three sayings of an asking process take one `Asking`, as `pause`'s three take one `WaitingOn`:
  - `pause::asking(prompt)` becomes `asking_claim(&Asking)`;
  - `engine_asking(engine)` becomes `asking_in_full(&Asking)`;
  - `asking_on(prompt)` becomes `asking_on(&Asking)`.
  
  Their tests build an `Asking`.
- The refusal (`CliError::RunAsking`) holds an `Asking`, from `read_engine(..).asking(now)`. `refused_hold` takes the instant, which its two callers in `resume.rs` read off the context's clock. Its words stay the same.
- `status`'s `Reading`, the closing, `drive` and every other reader keep the engine's reading only to build the frame.
- The run page's fixtures build frames by hand and read the page `READ_AFTER` their last event. Each frame carries the measures that instant gives: `since_last_event` of `READ_AFTER`, and its state's `time` to the same instant.

**Fails first:** none. The measures are Task 18c's, and the pages say the same. The guard is every golden, the captures and the JSON captures, byte for byte.

**Acceptance criteria:**
- [x] No file of `run_page/`, `list/inbox.rs`, `status/progress.rs`, `json/` or `surface/painter.rs` reads `.prompt` or `.at` off the engine's reading.
- [x] `time_in_state`, the run page's `since` and `engine_asking` are gone, and no saying of an asking process takes a `Prompt`.
- [x] Every golden's digest equals the base's, the captures equal Checkpoint D's, and the refusal says what it said.

**Verification:**
- [x] `cargo test -p yunta-render -p yunta-replay` (464), `cargo test -p yunta --bins` (346), `cargo test -p yunta --test status_cmd --test status_page_cmd --test status_follow --test three_questions --test run_words --test parked_runs --test mcp_flow`, `cargo test -p yunta --test run_flow guarding`, `pages.sh` against Checkpoint D's captures: 104 views, 0 differ

**Dependencies:** Task 18c

**Found while doing it:**
- The run page's story asked the engine whether it asks about a node. The rule now lives on the question, `Prompt::is_about`, which `Engine::asks_about` and the story through `frame.asking` both read.
- `Region::show` reads the asking off the frame and takes `answerable` alone; the painter's reading returns the state and the frame. The closing's `framed` takes no instant.
- `refuse_if_driven` takes the context: its callers are four, `resume`, `resolve-gate` and two control-plane tools, where the plan counted the two in `resume.rs`.
- The fixtures read a case through `Case::logged`, which measures on the log the case keeps: `needs_you` set its state after its frame, and now hands its log at once.
- The captures were taken with the debug binary the tests built, named as `normalize.py` expects: the CLI prints the same bytes in either profile.

**Files likely touched:** `crates/cli/src/{run_page/{mod,facts,fixtures,tests}.rs,commands/list/inbox.rs,commands/status/{progress,mod}.rs,json/{mod,waiting}.rs,surface/{painter,region,view/mod,closing/mod}.rs,surface/{region,view}/tests.rs,commands/drive.rs,commands/resume.rs,error.rs,error/hold.rs}`, `crates/render/src/{pause.rs,pause/tests.rs}`

**Estimated scope:** L (each page reads a field in place of a subtraction, and three signatures lose the engine)

### Task 18e: what a run's registry says is one `Engine` enum

**Description:**
- `yunta_replay::registry::{Engine, EngineLiveness}` become one enum, `Engine` (plan decision 5): `Alive { pid, prompt, words }`, `Dead`, `Unrecorded`, `Unknown`. It holds no instant: `RunFrame::of`'s `now` is the one.
  - `Alive`'s pid is the registry's `engine_pid`: the process's, where the question used to carry a copy.
  - `Engine::asking(at)` builds an `Asking` from `Alive`'s pid and prompt, in place of reading the prompt's copy of it. `words()` reads `Alive`'s socket, and `asks_about` stays.
  - `Engine::this(pid)` is the process reading the run. Its callers hand it `Pid::current()`: `drive`, the closing, `EngineReading::This` and `yunta test`'s case. The tests hand it a fixed pid.
- `Prompt` loses its `pid` and becomes the one type of the record:
  - it derives serde as the engine's `Asked` did, with the same field attributes;
  - `EngineProcessFile.asking` holds a `Prompt`, and `ProcessRegistry::asking` takes one;
  - `process_registry::Asked` is gone, with its re-export from the engine's root.
- In `crates/engine/src/process_registry.rs`, `engine_liveness`, `engine_prompt` and `engine_words` are gone into one function, `read_engine(run_dir, probe)`, which reads `engine.json` once and probes its process once, with no instant.
- Every reader changes once:
  - the frame (`run_standing`, a node's word) takes `&Engine`;
  - `close_run`, the pool's slot, `gc` and the CLI's `close` match the variant;
  - the refusal (`error/hold.rs`) reads `read_engine(..).asking(now)`;
  - the follower takes the socket off `Alive`;
  - the CLI's `render::engine` is gone. Its callers call `yunta_engine::read_engine(run_dir, &SystemProbe)`: `cancel`, `list/inbox`, `mcp/tools`, `resume`, `stats/mod`, `status/{follow,mod}`, `graph` and `surface/mod`;
  - `surface::EngineReading::read()` takes no instant;
  - the 20 `Engine::this(at)` become `Engine::this(pid)`.
- `CAPABILITY-MAP.md`'s row for `yunta-replay` names `Engine`, `Prompt` and `Asking`.

**Fails first:** `process_registry/tests.rs`'s readings, rewritten to compare whole readings, fail against the struct `read_engine` returned, which named no pid and took an instant:
- a registry whose process wrote it and asks reads `Alive`, with the registry's `engine_pid`, its prompt and its socket;
- a gone process, or a number the host gave to another, reads `Dead`;
- no registry reads `Unrecorded`;
- an unreadable one reads `Unknown`.

**Written first, passing before the change and after it:** a test in `process_registry/tests.rs` pins the `asking` an asking engine writes to `engine.json`, `{"node": …, "since": …}`, and that it reads back. It holds the file's bytes while `Asked` becomes `Prompt`.

The rest is a reshape, guarded by every test and golden.

**Acceptance criteria:**
- [x] A search in `crates/` for `EngineLiveness`, `engine_liveness`, `engine_prompt`, the registry's `engine_words`, `render::engine`, `process_registry::Asked` and `Driver` finds nothing; `Prompt` holds no pid, and `Engine` no instant.
- [x] `read_engine` reads `engine.json` once and probes its process once.
- [x] The pinned `asking` reads and writes the same bytes.
- [x] Every golden's digest and the captures are the same.

**Verification:**
- [x] `cargo test -p yunta-replay -p yunta-render` (464), `cargo test -p yunta-engine --lib` (216), `cargo test -p yunta-engine --test cancel --test live_prompt --test finding_answers --test finding_proofs --test gate_findings --test optional_nodes --test persisted --test run_words`, `cargo test -p yunta --bins` (346), `cargo test -p yunta --test close_cmd --test gc_cmd --test gc_lock --test status_follow --test run_words --test parked_runs --test status_cmd --test status_page_cmd --test three_questions --test mcp_flow --test stalled_runs`, `cargo test -p yunta --test run_flow guarding`, `pages.sh` against Checkpoint D's captures (104 views, 0 differ), the 99 digests against the base, `cargo fmt --check`, `cargo xtask smells --check`, `cargo xtask docs --check`

**Dependencies:** Task 18d

**Found while doing it:**
- **The name.** The task was first written for `Driver`, and `smells --check` refused it: `driver` is a word this repository retired, what an adapter is not (`banned_vocabulary`, 131 lines). The user chose `Engine` on 2026-11-08: the process driving a run is the engine everywhere else, in `engine.json`, its `engine_pid`, `engine.lock` and what a reader reads. The spec and the plan say so.
- **A function past its budget.** `RunFrame::of` grew to 54 lines with the measures. Two seams came out: how long since the last event is the log's clock's to say, `Timeline::since_last(now)`, and the count of reroutes is `reroutes(state)`, beside `task_counter`.
- The follower keeps when it last read the registry as `registry_read_at`, since the reading itself holds no instant; `close` judges the run by `settled_engine`.
- `engine_words` in `crates/engine/tests/log_words.rs` is that test's helper for the words the engine writes to the log, not a reading of the registry.
- `Engine::this(pid)` takes `Pid::current()` in `drive`, the closing, `EngineReading::This` and `yunta test`'s case, and in the tests that frame a run the test process reads.

**Files likely touched:** `crates/replay/src/{registry.rs,clock.rs,lib.rs,frame/{mod,node}.rs}`, `crates/replay/tests/{frame.rs,frame/standing.rs,frame/measures.rs}`, `crates/engine/src/{process_registry.rs,process_registry/{tests,write}.rs,lib.rs,run/{asking,close}.rs,worktree/pool/slot.rs}`, `crates/engine/tests/{cancel,live_prompt,finding_answers,finding_proofs,gate_findings,optional_nodes}.rs`, `crates/cli/src/{render/words.rs,surface/{mod,painter,turns,closing/mod}.rs,error/hold.rs,graph.rs,run_page/fixtures.rs,commands/{close,cancel,resume,drive,drive/watch,mcp/tools,stats/mod,list/inbox,gc/mod,test/case/problems,status/{follow,mod}}.rs}`, the CLI's tests that build an engine's reading, `CAPABILITY-MAP.md`

**Estimated scope:** L (a reshape: the compiler names every reader)

### Task 18f: the driving engine's words, in `yunta_render::engine`

**Description:**
- `STALLED`, `asking_on`, `asking_claim` and `asking_in_full` move from `crates/render/src/pause.rs` to a new `crates/render/src/engine.rs`, and their tests to `engine/tests.rs`.
- `pause`'s `//!` again says only what the log recorded. `engine`'s says what holds a run whose log says it moves: an engine that is gone, or one asking at its terminal. Render's map names `engine`.
- Their callers change path: `list/inbox`, `status/progress`, `error.rs`, `run_page/mod` and `surface/view/mod`.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] `pause` defines none of the four, and every golden is byte for byte the same.

**Verification:**
- [x] `cargo test -p yunta-render` (267), `cargo test -p yunta --bins` (346), `cargo xtask docs --check`, `cargo xtask smells --check`

**Dependencies:** Task 18e

**Found while doing it:**
- The CLI names the module `engine::` beside `yunta_engine::`, which it always writes whole, and beside a local `engine` reading in `list/inbox`'s `run_row`; a module path and a value never resolve to each other, and the compiler confirms each one.
- Render already holds `failure::engine`, the words of a failure inside the engine's own steps. `crate::engine` is what holds a run; neither names the other.

**Files likely touched:** `crates/render/src/{lib.rs,pause.rs,pause/tests.rs,engine.rs,engine/tests.rs}`, `crates/cli/src/{commands/list/inbox.rs,commands/status/progress.rs,error.rs,run_page/mod.rs,surface/view/mod.rs}`

**Estimated scope:** M (a move)

### Checkpoint E: after Tasks 16–18f
- [x] The suites; every golden's digest; the captures; the decision's JSON. Measured after Task 18b, and measured again after Task 18f: `proof/run-pages/checkpoint-e.txt`.
- [x] The user reviews the phase. Then every step of the `ci` job, commit 4, `cargo clean`: commit `f81661f9`.

## Phase 5: what is left of the CLI's `render`

### Task 19: how far a run has come, in render

**Description:**
- `crates/cli/src/render/counter.rs` moves, with `git mv`, to `crates/render/src/progress.rs`, with its tests.
- `status/progress.rs`'s `summary` joins it, and `receipt::unknown_kinds` moves to `crates/render/src/state.rs`.
- Their callers change path: `list/inbox`, `run_page/facts`, `status/progress`, `json/mod`, `stats/run` and the receipt.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] `crate::render::counter` and `progress::summary` are gone, and every progress line is the same.

**Verification:**
- [x] `cargo test -p yunta-render` (270: the counter's 3 tests moved with it), `cargo test -p yunta --bins` (343), `cargo xtask smells --check`, `cargo xtask docs --check`

**Dependencies:** Checkpoint E

**Found while doing it:**
- In render the counter's `progress` would read `progress::progress`. It is the page's `progress` row, beside `tasks_row` and `places`, so it is `progress::row`.
- The summary said a stall as "no process is driving it", the clause `STALLED` opens with, written a second time. `engine::NOT_DRIVEN` is that clause, and `STALLED` is built on the same literal by `not_driven!`, so the two cannot drift.
- `status/progress.rs` keeps only the frames, which Task 20 removes; its `//!` says so much and no more.

**Files likely touched:** `crates/render/src/{progress,state,receipt,lib}.rs`, `crates/cli/src/{render/{mod,counter}.rs,commands/status/progress.rs,commands/list/inbox.rs,run_page/facts.rs,json/mod.rs,commands/stats/run.rs}`

**Estimated scope:** M (a move)

### Task 20: each caller replays once

**Description:**
- `status/progress.rs` is deleted: its `frame` and `frame_of` are gone. Their callers replay once and call `RunFrame::of`: `cancel`, `drive`, `list/inbox`, `mcp/tools`, `resume`, `status/{follow,mod}` and `graph`.
- `render::engine` went with Task 18e, where every reader of the registry changed to `read_engine(run_dir, probe)`.

**Fails first:** none.

**Acceptance criteria:**
- [x] A search for `progress::frame` finds nothing, and no caller replays one log twice (`rs_twice_any.py`).

**Verification:**
- [x] `cargo test -p yunta --bins` (343), the CLI's batches (60 binaries, 858 tests: Task 18f's 861 less the counter's 3, now render's)

**Dependencies:** Task 19

**Found while doing it:**
- Each caller writes `RunFrame::of(run, &manifest.workflow, &state, None, &engine, now)`. The `None` is the history the frame compares with, which only `status`, `drive` and the control plane's status tool read, and set after.
- `graph` framed through `progress::frame`, which replayed the log itself; it now replays once where it frames.
- `rs_twice_any.py` names only tests that replay one log twice on purpose, to hold replay deterministic: none is a caller.
- `status/progress.rs` held Task 19's cut, read before `git rm -f` removed it.

**Files likely touched:** `crates/cli/src/{commands/status/progress.rs,commands/cancel.rs,commands/drive.rs,commands/list/inbox.rs,commands/mcp/tools.rs,commands/resume.rs,commands/status/{follow,mod}.rs,graph.rs}`

**Estimated scope:** L (a rename)

### Task 21: a node's words, and the runs a run bore, in render

**Description:**
- `render::node_display(&node)` becomes `impl From<&NodeFrame> for NodeDisplay`, and `render::called` moves to `crates/render/src/state.rs`. Their 8 callers write `NodeDisplay::from(&node)`.
- `crates/cli/src/surface/view/children.rs` moves, with `git mv`, to `crates/render/src/children.rs`, with its tests. Its callers are `run_page/facts`, `surface/chronicle/mod` and `surface/view/mod`, and `surface/mod.rs` re-exports it no more.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] `render/words.rs` holds only `RunExit` and `exit`, and every golden is the same.

**Verification:**
- [x] `cargo test -p yunta-render` (274: children's 4 tests moved with it), `cargo test -p yunta --bins` (339), `cargo xtask smells --check`, `cargo xtask docs --check`

**Dependencies:** Task 20

**Found while doing it:**
- The node words have 6 callers and `called` 3, the 9 the task counted as 8. `NodeDisplay::from` reads as the conversion it is: `stats/run` hands it to `map_or_else` by name.
- The chronicle's words already name core's `events::children`, so they import render's `child_standing` and `closed_as` by name, and the chronicle's `//!` names where they live.
- `children.rs`'s five functions are render's public words now, where three were the surface's alone.
- The full paths Task 20 wrote for `RunFrame::of` were wrapped by `rustfmt` and pushed `follow` past 50 lines, as `yunta_render::children::closed_as` did `run_words`. A file that imports `RunFrame` names it short, and the long-function count stays at its baseline.

**Files likely touched:** `crates/render/src/{state,children,lib}.rs`, `crates/cli/src/{render/{mod,words}.rs,surface/{mod,view/mod,view/children,chronicle/mod}.rs,run_page/{facts,story}.rs,commands/stats/run.rs,commands/status/node.rs,graph.rs,json/node.rs}`

**Estimated scope:** L (a move)

### Task 22: the CLI's output owns its streams

**Description:**
- `crates/cli/src/output.rs` moves to `crates/cli/src/output/mod.rs`, and `render/stream.rs` to `output/stream.rs`, both with `git mv`.
- `surface_lines`, `blocks_lines` and `draw` join `output/mod.rs`.
- Their 36 callers change path, from `crate::render::{glyphs, settle_*, stdout_look, stderr_look, …}` to `crate::output::…`.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] `crates/cli/src/render/` holds only `words.rs`, beside the `mod.rs` that declares it.

**Verification:**
- [x] `cargo test -p yunta --bins` (339), the CLI's batches (60 binaries, 854 tests: Task 20's 858 less children's 4, now render's)

**Dependencies:** Task 21

**Found while doing it:**
- The callers are 31 paths in 22 files, the 4 settlements in `main.rs`, and the 4 inside `output` itself, which call their own module's functions by name now.
- `output/mod.rs`'s `//!` says what drawing depends on lives there, and `main.rs`'s map says `output` owns the streams a command's output is drawn to.

**Files likely touched:** `crates/cli/src/{output.rs,output/{mod,stream}.rs,render/{mod,stream}.rs,main.rs}`, and the 36 callers

**Estimated scope:** L (a move)

### Task 23: the exit code, in output

**Description:**
- `crates/cli/src/output/exit.rs` takes three things:
  - `Outcome`, from `error.rs`;
  - `RunExit` and `exit`, from `render/words.rs` (moved with `git mv` and cut to them).
- `crates/cli/src/render/` is gone, and so is `main.rs`'s `mod render`.
- Their 39 callers change path, and `main.rs`'s `//!` map says `output` holds the exit code.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] A search for `crate::render` and `error::Outcome` finds nothing.
- [x] `a_run_waiting_on_a_clock_exits_on_its_own_and_a_stop_is_not_a_success` passes in its new place, `output::exit::tests`.

**Verification:**
- [x] `cargo test -p yunta --bins` (339), the CLI's batches (60 binaries, 854 tests), `cargo xtask docs --check`, `cargo xtask smells --check`, the gate's `doc` step

**Dependencies:** Task 22

**Found while doing it:**
- 38 files named `Outcome` from `error` or the exit from `render`; each names them from `output` now.
- Three comments and a `//!` named `crate::render`: the live region's view, the graph's labels and the stats page. Each names where the thing lives now, and the gate's `doc` step found the one that was a link. `output`'s `//!` links `exit()`, since `exit` is a module and a function there.
- The smell `cli_print_outside_output` named `crates/cli/src/output.rs` as the one file that prints. That file is `output/mod.rs` now, and the smell and its test name it.
- **An intermittent test, not this task's.** `session_ends::cancel_after_its_engine_died_leaves_no_command` failed once in the CLI's fifth batch: the stub did not record its child's pid within the testkit's 30 s rendezvous. It passed when the batch ran again, 10 of 10 alone, and in the batches of Checkpoint E and Tasks 20 and 22. What it waits on, a stub starting its child under a session the engine launched, is no code this phase changed.

**Files likely touched:** `crates/cli/src/{output/exit.rs,output/mod.rs,render/words.rs,error.rs,main.rs}`, and the 39 callers

**Estimated scope:** L (a move)

### Checkpoint F: after Tasks 19–23
- [x] The suites; every golden's digest; the captures: `proof/run-pages/checkpoint-f.txt`.
- [x] The user reviews the phase. Then every step of the `ci` job, commit 5, `cargo clean`: commit `1bc7795c`.

## Phase 6: one form

### Task 23b: a stop after a crash finds what the session started, every time

Added at the user's request, after the measurement of Checkpoint F.

**Description:**
- **What fails.** `session_ends::cancel_after_its_engine_died_leaves_no_command` (`crates/cli/tests/session_ends.rs`) failed once in four runs of the CLI's fifth batch: the testkit waited its 30 s for the stub to record the pid of a child it starts, and the pid never came. It passed when the batch ran again, and 10 of 10 alone.
- **What is known.** Both stubs start their two children in the order the test reads their fifos, each child writes its pid before the stub goes on, and a fifo's two ends meet whichever opens first. So the order alone is no deadlock. The test says nothing of what the run did while it waited.
- **The work:**
  - measure the rate, alone and beside a build, as `tools/repeat_test.py` measured Task 18b's;
  - reproduce the miss, and name its cause with what the run left: its detached log, its registry, its event log;
  - fix it where it lives, in the product when the product drops something, in the test or its stub when they assume what is not promised;
  - make the test hold every time, and say when it waits what it waited on.
- **The stubs leave no fifo behind** (added at the user's request, found while doing it). Each stub's `ready` fifo lives beside the fifo its knob names, in the test's own directory, instead of in the user's `TMPDIR` under the stub's pid: it goes with the test's directory, and no two stubs can meet on one name.

**Fails first:**
- a test that puts the run where the miss shows it, and finds what it promised. It fails before the fix, for the cause named;
- `stubs::tests`: each stub is started with a child that never reaches its rendezvous, killed while it waits, and found to have left nothing in the `TMPDIR` it was given. It fails today because the stub's `ready` fifo stays there.

**Acceptance criteria:**
- [ ] The cause is named in the task, with what shows it. **Not met:** no condition tried shows the miss again (below). What it left, and what the test now says when it waits, are in `.agents/open.md`, the open question's home, to be deleted with the fix a next miss explains.
- [x] `session_ends::cancel_after_its_engine_died_leaves_no_command` passes 30 runs out of 30, alone and beside a build, with the stubs as this task leaves them.
- [x] A run of the adapters' contract tests leaves no new fifo in `TMPDIR`: 0 new after 329 tests, where each run left 4 to 6 before.

**Verification:**
- [x] `python3 .plans/yunta-tui/tools/repeat_test.py <worktree> 30 yunta session_ends cancel_after_its_engine_died_leaves_no_command`, alone and beside a release build, 0 of 30 each; `cargo test -p yunta --test session_ends`
- [x] `cargo test -p yunta-testkit-core` (3: the stubs' two tests failed first, on the fifo each left), `cargo test -p yunta-adapters` (329) through `tools/new_fifos.py`, and the gate's `fmt`, `clippy`, `smells` and `docs` steps

**Dependencies:** Checkpoint F

**Files likely touched:** `crates/cli/tests/session_ends.rs`, `crates/testkit-core/src/{adapter.rs,stubs.rs,stubs/tests.rs}`, `crates/testkit-core/stubs/{claude_code,codex}_stub.sh`, and where the cause lives

**Estimated scope:** M

**Found while doing it:**
- **What the miss left.** The stubs make their rendezvous through a fifo `ready`, named by the stub's pid in the user's `TMPDIR`, which lives only between the stub's `mkfifo` and its `rm`. One `claude-stub-ready.<pid>` was born at 13:51:12, three seconds into the failing batch, whose first target was `session_ends`, and no other CLI test starts these children. So the failing run's Claude Code stub did start, and waited on a child that never opened `ready`: the child neither wrote its pid nor reached that line.
- **What does not reproduce it**, with the test reporting what the run left:
  - the test alone: 0 of 30 (and 0 of 10 before);
  - its whole target, its five tests side by side, beside a release build: 0 of 30;
  - the target's binary from four workers at once, beside a release build: 0 of 40;
  - the target right after its binaries were linked again: 0 of 8;
  - the whole fifth batch as the gate runs it: 0 of 10.
- **What is ruled out:**
  - the order of the two fifos, which the stubs and the test share;
  - a probe reaching the children, since the engine only probes `--version`, which a stub answers before it starts any;
  - a sibling's sweep, since a mark names its run;
  - a slow `python3`, which starts in 0.02 s cold, five at once;
  - a stale fifo of the same name, since the miss's fifo was born new.
- **A defect found on the way.** A stub killed between its `mkfifo` and its `rm` leaves its fifo behind in the user's `TMPDIR`. The adapter contract tests do it each run, killing a session the moment its child's pid arrives, and 78 had piled up there.
- **What the task leaves.**
  - `recorded_pid` returns `None` when no pid comes, and `grandchild_pid` panics on that for its other callers.
  - `session_ends` names the command it waited for (`GROUPED`, `ORPHANED`), whether the stub started it (its `ready` fifo is there), and what the run left: `detached.log`, `engine.json` and `yunta status`. A next miss names its cause.
  - Each stub's `ready` lives beside its knob's fifo, in the test's directory, and the 78 fifos earlier runs left in `TMPDIR` were removed once by hand.
- The stubs' test waits for the child with `yield_now`, as the testkit's waits do: the `sleep_outside_mock_latency` smell holds a sleep to its exempt files.

### Task 24: `Page`, and an answer that hands its page

**Description:**
- `crates/render/src/page.rs`: `Page` and `impl<P: Page + ?Sized> Page for &P`, with the `//!` that maps every page.
- `Answer` in `output/mod.rs` loses `document` and gains `fn page(&self) -> impl Page + '_`, which `emit` draws. `output::show(look, &page)` draws a page for a command with no JSON.
- In place, each of the 11 answers moves its `document` into `impl Page for <Answer>`, and its `page()` returns `self`:
  - `status`, `list/runs`, `stats/mod` (two), `doctor`, `verify`;
  - `open`, `resolve_gate`, `receipt`, `close`, `cancel/page`.

**Fails first:** none: a form. The guard is every golden and the captures.

**Acceptance criteria:**
- [x] No `fn document` is left in an `impl Answer`: the 11 answers each have an `impl Page`, and their `page()` returns `self`.
- [x] The JSON captures are byte for byte the same: 104 views, 0 differ from Checkpoint F's.

**Verification:**
- [x] `cargo test -p yunta-render` (274), `cargo test -p yunta --bins` (339), the CLI's batches (60 binaries, 854 tests), `pages.sh` against the base (the four texts of Phase 2, and no other), the 99 digests, `cargo xtask smells --check`, `cargo xtask docs --check`

**Dependencies:** Checkpoint F

**Found while doing it:**
- `emit` draws a person's page through `output::show`, so `show` has its first caller in this task; the commands that still build a bare `Doc` keep `output::document` until Task 29 makes each of them a page, as the plan says.
- The tests that drew an answer called `document` through `Answer`; they name `Page` now, and those that also read the outcome keep `Answer`.
- `page.rs`'s `//!` says the form and that each page of the crate is one of its modules; it lists them as they move in, from Task 25.

**Files likely touched:** `crates/render/src/{page,lib}.rs`, `crates/cli/src/output/mod.rs`, the 11 answers' files, the 6 callers of `output::document`

**Estimated scope:** L (a form)

### Task 25: the receipt and the findings, as pages

**Description:**
- `crates/render/src/receipt.rs` and `findings.rs` move, with `git mv`, to `crates/render/src/page/{receipt,findings}.rs`.
- `receipt::document(receipt, glyphs)` becomes `page::Receipt { receipt }` implementing `Page`, and `findings::{document, run_document}` becomes `page::Findings` with its two forms.
- `fan_out_groups` stays with the receipt's page.
- Their callers fill the values: the adapters' `forge/mod.rs`, the CLI's `receipt`, the engine's `tests/receipt.rs` and `tests/receipt/from_a_log.rs`, and render's own tests.

**Fails first:** none: a form and a move.

**Acceptance criteria:**
- [x] The receipt's and the findings' goldens and the forge's receipt body are byte for byte the same.

**Verification:**
- [x] `cargo test -p yunta-render -p yunta-adapters -p yunta-core` (1,436), `cargo test -p yunta --test receipt_cmd`, the engine's `receipt` target, the 99 digests

**Dependencies:** Task 24

**Found while doing it:**
- **`fan_out_groups` is the receipt's data, not its page.** Grouping fan-out siblings by their base id derives from the runners the receipt holds, and the engine's test reads it apart from any page. Kept with the page, it would have needed a path like `page::fan_out_groups`, which names no page. It is `yunta_core::receipt::Receipt::fan_out_groups(&self)`, which the review check and the engine's test both call.
- **One path to each page.** `page`'s modules are the crate's own, and `page::Receipt` and `page::Findings` are each one public path. `findings::mark` stays crate-visible for render's own test of it.
- `page::Receipt` names core's receipt as `receipt::Receipt`, since the page takes its name. The forge draws it with the Unicode separators it always used, through a `Look` of `Glyphs::Unicode`.
- `ReceiptAnswer`'s page is `page::Receipt` of its receipt, so it keeps no `impl Page` of its own.
- `shown` names `page::findings::{held_document, run_document}` until Task 27 makes every page a page holds a page drawn with its look.

**Files likely touched:** `crates/render/src/{receipt,findings,page,lib}.rs`, `crates/render/src/page/{receipt,findings}.rs`, `crates/adapters/src/forge/mod.rs`, `crates/cli/src/commands/receipt.rs`, `crates/engine/tests/{receipt.rs,receipt/from_a_log.rs}`

**Estimated scope:** M

### Task 26: what a gate shows, as pages

**Description:**
- `crates/render/src/{shown,spec}.rs` and `plan/` move to `crates/render/src/page/{shown,spec}.rs` and `page/plan/`, with `git mv`.
- `shown::document(document, run, form)` becomes `page::Shown { document, run, form }`. `spec::document` becomes `page::Spec`, and `plan::document` becomes `page::Plan`, whose desk (`Reading`, `Spot`, `View`) is the way a terminal reads it.
- `plan_among` and `before_you_decide` stay with `page::Shown`.
- Their callers fill the values: `ask/{desk,decision}`, `review`, `status/{mod,node}`, `escalation/decision`, `published` and `gate`.

**Fails first:** none.

**Acceptance criteria:**
- [x] The `document/` goldens are byte for byte the same.

**Verification:**
- [x] `cargo test -p yunta-render` (274), `cargo test -p yunta --bins` (339), the 99 digests, `smells --check`, `docs --check`

**Dependencies:** Task 25

**Found while doing it:**
- **The desk is `page::desk`, and no page.** It is how a terminal reads a plan a place at a time, so it sits beside the pages, not inside the plan's: `page::desk::{Reading, Spot, View, AlsoShown, about, reading}`. The parts of the plan it reads (`card`, `change`, `head`, `proof`, `steps`, `tree`) are `pub(in crate::page)`: the plan's own, read by the desk and by nothing outside `page`.
- **`plan_among` is the desk's, not `Shown`'s.** Which document a desk opens is the desk's rule, `has_a_place`, now private to it; the gate's `above`, `review` and the decision's desk ask the desk. `before_you_decide` stays with `page::Shown`, as planned.
- **One path to `Form`:** `page::Form`. `shown` no longer re-exports it.
- Inside render, the gate, `published` and the desk's `reading` hold no look in this task, so they draw `page::shown`'s body, crate-visible with the bodies it composes. Task 27 gives each of them a look.
- The CLI fills `page::Shown` and draws it with the look it holds: the browser's status page with the `Html` look, the node page with its own, `review` with its own.

**Files likely touched:** `crates/render/src/{shown,spec,plan/*,page,gate,published,escalation/decision}.rs`, `crates/render/src/page/{shown,spec,plan/*}.rs`, `crates/cli/src/{ask/desk.rs,ask/decision.rs,commands/review.rs,commands/status/{mod,node}.rs}`

**Estimated scope:** L (a move)

### Task 27: the gate's page

**Description:**
- `crates/render/src/gate.rs` moves, with `git mv`, to `crates/render/src/page/gate.rs`.
- `GatePage` with `page()`, `whole_page()` and `above()` becomes `page::Gate`: one value, whose placement says whether it is drawn whole or above the prompt that answers it.
- Its callers fill it: `ask/decision`, `published`, and the adapters through `published`.

**Fails first:** none.

**Acceptance criteria:**
- [x] The `gate/` goldens, `pull-request.md` among them, are byte for byte the same.

**Verification:**
- [x] `cargo test -p yunta-render -p yunta-adapters -p yunta-surco` (1,063), `cargo test -p yunta --bins` (339), the engine's `receipt` target, the 99 digests, `smells --check`, `docs --check`, rustdoc over private items

**Dependencies:** Task 26

**Found while doing it:**
- **`page::Gate` is an enum whose variant is where it is drawn**, as `page::Findings` is one of what it shows: `OnTheForge { run, documents, waits, note, changed }`, `AboveTheCard { shown, escalation }` (what a prompt's card stands under, the plan left to its desk) and `InPlaceOfTheDesk { shown, escalation }` (all of it, for a terminal too small for the desk). `whole_page` and `above` returned `Option<Doc>`; a page with nothing to say is now an empty document, and the prompt's `draw_page` draws nothing for it.
- **What answers the decision is not the page's.** Surco's blocks do not clone, and a page draws from `&self`, so a page holds what it was filled with, never a section built before it. A terminal's card follows the page; `published` ends a pull request's description on the decision a review gives, after the page. `GatePage.form` is gone: the forge's page is always `Form::Published`, a terminal's `Form::Review`.
- **A page that holds another draws it as a page, with its own look.** The gate draws `Shown`; `Shown` draws `Plan`, `Spec` or `Findings`; `published` draws each file the pull request carries as `Shown` with the Markdown look. The desk keeps what else a decision shows as its `Shown` page (`AlsoShown<'a>`), and the pane draws it with the pane's look, as it draws the rest. Each page's body is private to its module, so a page is drawn one way only.
- **A forge's look is surco's:** `Markdown::look()` and `Description::look()` (Unicode glyphs, no paint, no width), as `Html::look()` is. The forge's receipt and the engine's receipt test use it instead of a look built by hand.
- Three files went past 500 lines and split along what they hold:
  - Markdown's `problem` and `refusal` drawers go to `surface/markdown/problem.rs`, as the terminal surface keeps them;
  - the desk's rail tests go to `tests/desk/rail.rs`;
  - the spec page's test goes to `tests/documents/spec.rs`.

  Render's plan tests draw through `documents::plan_page`.

**Files likely touched:** `crates/render/src/{gate,page,published,lib}.rs`, `crates/render/src/page/gate.rs`, `crates/render/src/tests/console_page.rs`, `crates/cli/src/ask/decision.rs`

**Estimated scope:** M

### Task 28: the run's pages take the form in place

**Description:**
- `RunPage` implements `Page`.
- `NodePage`, `RunStatsPage`, `WorkflowPage`, the field's page and the doctor's and `verify`'s checklist each become a struct implementing `Page`, still in the CLI.
- Each command fills its page, and its answer's `page()` returns it.

**Fails first:** none.

**Acceptance criteria:**
- [x] The status, closing, open, field and stats goldens are byte for byte the same.

**Verification:**
- [x] `cargo test -p yunta --bins` (339), the 99 digests, `smells --check`, `docs --check`
- [x] the CLI's batches (60 binaries, 855 tests), run once at Checkpoint G on the tree Tasks 28 and 29 leave

**Dependencies:** Task 27

**Found while doing it:**
- `RunPage`, `NodePage`, `RunStatsPage` and `WorkflowPage` implement `Page`: each one's `document` moved into the impl unchanged.
- **`status`'s body is its page.** `status` shows a run's page or a node's, so `Body` implements `Page` and `StatusAnswer::page` lends it. The answer keeps the inputs of its JSON document beside it.
- **The answer holds the data once; its page lends it.** The field's, the doctor's and `verify`'s pages are `Field`, `Doctor` and `Verified`, named as the plan's table names them so they move without a rename, and private to their command. Each answer holds the runs or the checks once for `--json`, and its `page()` lends them. What draws the field (`headline`, `parts`, `footer`, ...) and the doctor's headline are the page's now, not the answer's.
- `drive`'s closing shows `RunPage` through `output::show`, now that it is a page.
- `RunPage::document`'s doc named `render::draw`, a module Phase 5 removed; it says `output::draw`.
- **A defect, for Task 29.** `yunta receipt` writes `receipt.md` beside the run with the terminal's look. What the run removed reads `1 file, -1` when the command runs with ASCII glyphs and `1 file, −1` with Unicode ones, as do the separators of a cost per verified task. The pull request's body and the engine always draw the receipt with Unicode, so a file's bytes depend on the terminal that ran the command.

**Files likely touched:** `crates/cli/src/{run_page/mod.rs,commands/status/{mod,node}.rs,commands/list/runs.rs,commands/stats/{mod,run,workflow}.rs,commands/doctor/mod.rs,commands/verify.rs}`

**Estimated scope:** L (a form)

### Task 29: every other command's page takes the form in place

**Description:**
- Each of the 16 pages of plan decision 7's last five rows becomes a struct implementing `Page`, named as there, still in its command's file. Its command fills it and emits it.
- `drive`, `init`, `review`, `schema` and `verdict` draw through `output::show`.
- `output::document` is left only if `status/follow`, part of the live region, still needs to print a `Doc` that is no page. Otherwise it is gone.
- `yunta receipt` builds `receipt.md` with the Markdown look, as the forge and the engine do, whatever the terminal it runs on.
- A page that read while it drew reads first, and its page draws what it read: the user's choice at Checkpoint G, so that Tasks 39 to 41 only move files.

**Fails first:** `receipt_cmd::the_receipt_file_is_the_same_whatever_terminal_asks_for_it`: a run removes a line, and the `receipt.md` that `yunta receipt` writes with ASCII glyphs says `1 file, -1` where the Unicode one says `1 file, −1`. Seen failing for that cause.

**Acceptance criteria:**
- [x] Each of the 16 that exists is a `Page` (`page::Stopped` names nothing), with the pages the table did not count, and the captures are byte for byte the same.
- [x] `output::document`'s callers: none. It is private to `output`, which writes what `show` draws of a page and `say` of a line.
- [x] `receipt.md` is the same bytes whatever terminal `yunta receipt` runs on.

**Verification:**
- [x] `cargo test -p yunta --bins` (339), the 99 digests, `smells --check`, `docs --check`
- [x] the CLI's batches (60 binaries, 855 tests); `pages.sh`: 104 views, none differ from Checkpoint F's, and the base's four from Phase 2 the only ones that differ from the base

**Dependencies:** Task 28

**Files likely touched:** `crates/cli/src/commands/{cancel/page,close,open,resolve_gate,gc/page,review,list/mod,init,new,schema,pack,pack/page,pack/audit,pack/needs,pack/scaffold,test/mod,verdict,drive}.rs`

**Done so far:**
- [x] `receipt.md` is drawn with the Markdown look: `receipt_cmd::the_receipt_file_is_the_same_whatever_terminal_asks_for_it`, seen failing for its cause, passes.
- [x] Each a struct implementing `Page`, the answer or the command lending it its data:
  - `Cancelled`, `Closed`, `Opened` and `Answered`, each from its answer's fields;
  - `Collected`, the pass's record, renamed from `Report`;
  - `Initialized`, renamed from `Wrote`;
  - `Schema` (`Documents`, `WorkflowKeys`);
  - `Review`;
  - `Signpost`, what `yunta` alone shows where it has no field to show;
  - `Installed`, `Inventory`, `Updated`, `Removed` and `Listed`, all in `pack/page.rs`.
- [x] What read while it drew reads first, then its page draws, as the user chose at Checkpoint G:
  - **the verdict:** `Verdict::diagnosis` reads into `Diagnosis`, which holds `Unrunnable` and the warnings as problems; `Verdict::checked` gives the page, `Checked`. It is `page::Verdict` in the table, but the CLI's `Verdict` is the judgment itself, and `check` answers with what it did, as `Verified` and `Tested` do.
  - **`new`:** `Written::report` reads the verdict, and `NewWorkflow` draws the file and its `Diagnosis`.
  - **`pack new`:** `check_pack` reads into `PackCheck` (`Refused` with its problems, or `Tested` with its cases' summary), and `NewPack` draws it.
  - **`test`:** `TestedCase`, said as each case ends, and `Tested`, the tally under them.
  - **how effective a workflow's checks were:** `VerificationEffectiveness`, which `check` shows and `stats --workflow` draws inside its page. It wraps replay's `VerificationFindings` because the CLI cannot give a type of replay's a trait of render's; in render the page is that type itself.
  - **the catalog:** `Catalog::read` reads each workflow it offers (`Offered`): a `Readable` workflow with its estimation, or why it is `Unreadable`; and each broken pack. `impl Page for Catalog` draws them with the look's glyphs. The control plane's `list_workflows` draws it with `Markdown.look()`, where it passed `Glyphs::Unicode`.
- [x] Pages the table did not count:
  - a refusal, which is its error's page: `impl Page for CliError`, in `error/page.rs`; `output::show_on_stderr` draws a page with stderr's look, as `show` draws one with stdout's;
  - `LeftRunning`, the run a person stopped following, left running without this terminal.
- [x] The names the user asked for at Checkpoint G's review: each says what it is, not a condition, a format or a string standing in for a variant. A pack's code is one module tree, `commands::pack::{audit, needs, page, scaffold}`; in gc's `collect`, what the pass reclaimed is `reclaimed`, beside the record it writes, `collected`.

**Found while doing it:**
- **A page holds what it says, never blocks drawn before it.** Surco's blocks do not clone, and a page draws from `&self`, so a page that held blocks could not be drawn. So:
  - `Installed` holds its needs as data: `needs::unmet` gives each `Need` (its node, the key, whether a run leaves the node out, what this repository detected), where `pack_needs::blocks` gave blocks. The page says them, each line once, as the reading did.
  - `Inventory` is what heads a pack's inventory: `Installing` or `Updating`, each with whether the executors wait on a person's `--yes`, or `Audited`. Whether they wait is the policy's call, so the command computes it, with `waits_on_yes`.
  - `Listed` (`Empty`, or `Locked`) holds each pack's verdict against the lock as `VendoredPack` rows, not a built checklist.
- **Some pages read while they draw.**
  - The verdict's page is async: it reads the workflow's file and the project's remote while drawing.
  - The catalog reads the disk and the runs' history while drawing.
  - `new` and `pack new` draw what `check` found as the verdict's built blocks (`Found { blocks }`).
  - `test` writes a row as each case ends.

  A `Page` cannot await, nor give away a block it holds, so these six take the form only once what they read is data. The user chose to split them here, so that Tasks 39 to 41 only move files.
- **`drive` has no page of its own.** At the base, it prints the run's closing page, which is `RunPage` and now shown through `output::show`, and a quiet run's verdict, which is that page's headline on stderr. The table's `page::Stopped` names nothing at the base.

**Estimated scope:** L (a form)

### Checkpoint G: after Tasks 24–29
- [x] The suites (275 binaries, 4,552 tests, none failed); render's 21 goldens and the CLI's 78 by digest; the captures. In `.plans/yunta-tui/proof/run-pages/checkpoint-g.txt`.
- [ ] The user reviews the phase. Then every step of the `ci` job, commit 6, `cargo clean`.

## Phase 7: the run's pages

### Task 30: a page's places

**Description:**
- `surface::closing::Outline` becomes `Places` (plan decision 8), still in the CLI. It takes `branch`, which its builders compute with `yunta_engine::run_branch`: `status/mod`, the closing and `drive`.
- The run page's `facts` and `evidence` and the node page name `yunta_core::run_dir` and no engine function.

**Fails first:** none.

**Acceptance criteria:**
- [ ] A search for `yunta_engine` in `run_page/` and `status/node.rs` finds nothing, and every golden is the same.

**Verification:**
- [ ] `cargo test -p yunta --bins`

**Dependencies:** Checkpoint G

**Files likely touched:** `crates/cli/src/{surface/closing/mod.rs,surface/mod.rs,run_page/{mod,facts,evidence,fixtures}.rs,run_page/tests/words.rs,commands/status/{mod,node}.rs,commands/drive.rs}`

**Estimated scope:** M

### Task 31: the run page moves

**Description:**
- `crates/cli/src/run_page/` moves, with `git mv`, to `crates/render/src/page/run/`, with `facts`, `story`, `evidence`, `fixtures` and its tests. `RunPage` becomes `page::Run`, and `Places` moves with it.
- `crates/cli/goldens/status/` and `open/` move to `crates/render/goldens/`.
- Its callers fill it from `yunta_render::page`: `status/mod`, the closing, `json/{mod,facts}` (`gist`, `cautions`, `artifacts_written`, `pull_requests_of`) and `drive`.
- `page::Run` holds no `Engine`, and names no clock type: since Task 18d, what drives the run and every time it says come on the frame.

**Fails first:** none: a move. The guard is the 60 goldens, now read from render.

**Acceptance criteria:**
- [ ] `crates/cli/src/run_page/` is gone, and the 60 goldens' digests equal the base's.
- [ ] `crates/render/tests/pure.rs` passes.

**Verification:**
- [ ] `cargo test -p yunta-render`, `cargo test -p yunta --bins`, `cargo test -p yunta --test status_cmd --test status_page_cmd --test open_cmd`

**Dependencies:** Task 30

**Files likely touched:** `crates/cli/src/run_page/*` to `crates/render/src/page/run/*`, `crates/cli/goldens/{status,open}/` to `crates/render/goldens/`, `crates/render/{Cargo.toml,src/page.rs}`, `crates/cli/src/{commands/status/mod.rs,surface/closing/mod.rs,json/{mod,facts}.rs,commands/drive.rs,main.rs}`

**Estimated scope:** L (a move)

### Task 32: the closing's goldens move

**Description:**
- `the_closing_block_matches_its_goldens` and the closing's other page tests move from `crates/cli/src/surface/closing/tests.rs` to `crates/render/src/page/run/tests/closing.rs`. They build `page::Run` in its closing scope, from the same frames and decisions.
- `crates/cli/goldens/closing/` moves to `crates/render/goldens/`.
- The CLI's `Closing` keeps what it gathers and its outcome. Its own test, of the outcome, stays.

**Fails first:** none: a move.

**Acceptance criteria:**
- [ ] The 12 closing goldens' digests equal the base's.

**Verification:**
- [ ] `cargo test -p yunta-render`, `cargo test -p yunta --bins`

**Dependencies:** Task 31

**Files likely touched:** `crates/cli/src/surface/closing/{mod,tests}.rs`, `crates/render/src/page/run/tests/closing.rs`, `crates/cli/goldens/closing/`

**Estimated scope:** M

### Task 33: the node page moves

**Description:** `NodePage` moves from `crates/cli/src/commands/status/node.rs` to `crates/render/src/page/node.rs` as `page::Node`. What reads the node's documents stays in the command, which fills the page.

**Fails first:** none.

**Acceptance criteria:**
- [ ] `status --node`'s captures are byte for byte the same, and `status_node_cmd` and `status_nodes_cmd` pass.

**Verification:**
- [ ] `cargo test -p yunta --test status_node_cmd --test status_nodes_cmd`, `cargo test -p yunta-render`

**Dependencies:** Task 31

**Files likely touched:** `crates/cli/src/commands/status/{node,mod}.rs`, `crates/render/src/{page.rs,page/node.rs}`

**Estimated scope:** S

### Task 34: the field moves, its rows as data

**Description:**
- `Inbox`'s `Group` and `RunRow` become `page::field::{Group, Row}` (plan decision 9): a row holds the run, the action it is listed for, what holds it (the frame's `WaitingOn` and `Asking`), and how long it has stood there (`since_last_event`). The page words it.
- `list/runs.rs`'s page moves to `crates/render/src/page/field.rs` as `page::Field`, and `list/runs/tests.rs`, with `crates/cli/goldens/field/`, moves to render.
- Gathering the inbox stays in `list/inbox.rs`.
- `list --runs --json` serializes the same fields through a serializer in `crates/cli/src/json/field.rs`, which words each row with render.

**Fails first:** none: a move. The guard is the `list-json` capture, byte for byte.

**Acceptance criteria:**
- [ ] The 3 field goldens' digests and the `list` and `list-json` captures equal the base's.
- [ ] `RunRow` holds no `String` a person reads.

**Verification:**
- [ ] `cargo test -p yunta-render`, `cargo test -p yunta --bins`, `pages.sh` against the base

**Dependencies:** Task 31

**Files likely touched:** `crates/cli/src/commands/list/{runs,inbox}.rs`, `crates/cli/src/commands/list/runs/tests.rs`, `crates/cli/src/json/field.rs`, `crates/render/src/page/field.rs`, `crates/cli/goldens/field/`

**Estimated scope:** M

### Task 35: the stats and the history move

**Description:**
- `crates/cli/src/commands/stats/{run,workflow,facts,time}.rs` move, with `git mv`, to `crates/render/src/page/stats/`, as `page::Stats` and `page::History`, with `run/tests.rs` and `crates/cli/goldens/stats/`.
- `stats/mod.rs` keeps the command, and `json.rs` stays in the CLI.
- The history page appends its findings document's blocks instead of drawing it to terminal lines.

**Fails first:** none: a move. If composing the findings changes a golden, the work stops and asks.

**Acceptance criteria:**
- [ ] The 3 stats goldens and the `stats` captures, text and JSON, equal the base's.

**Verification:**
- [ ] `cargo test -p yunta-render`, `cargo test -p yunta --test stats_cmd`, `pages.sh` against the base

**Dependencies:** Task 31

**Files likely touched:** `crates/cli/src/commands/stats/{mod,run,workflow,facts,time}.rs`, `crates/cli/src/commands/stats/run/tests.rs`, `crates/render/src/page/stats/*`, `crates/cli/goldens/stats/`, `crates/cli/src/run_page/facts.rs` (`format_estimation_line`, now render's)

**Estimated scope:** L (a move)

### Task 36: the doctor's and `verify`'s checklist

**Description:**
- `checklist` and `tallied` move from `verify.rs` to `crates/render/src/page/checklist.rs`, and the two pages to `page::Doctor` and `page::Verified`, which take Surco's `Check`s.
- `CheckRow`, a check's JSON form, moves to `crates/cli/src/json/check.rs`.
- `doctor`'s checks and `verify`'s walk stay in their commands.

**Fails first:** none: a move.

**Acceptance criteria:**
- [ ] The `doctor` and `verify` captures and `verify_cmd` are byte for byte the same.

**Verification:**
- [ ] `cargo test -p yunta --test verify_cmd --test pack_requires_doctor_cmd`, `cargo test -p yunta --bins`

**Dependencies:** Task 31

**Files likely touched:** `crates/cli/src/commands/{verify,doctor/mod}.rs`, `crates/cli/src/json/check.rs`, `crates/render/src/page/{checklist,doctor,verified}.rs`

**Estimated scope:** M

### Checkpoint H: after Tasks 30–36
- [ ] The 78 goldens, now under `crates/render/goldens/`, by digest; the captures; `pure.rs`.
- [ ] `crates/cli/goldens/` is gone.
- [ ] The user reviews the phase. Then every step of the `ci` job, commit 7, `cargo clean`.

## Phase 8: every other command's page

Each task moves its pages to `crates/render/src/page/`, by `git mv` where the file is the page alone, and by a split where the command and the page share it. Each page's test moves with it, and each command fills its page and emits it.

### Task 37: what `cancel`, `close` and `open` answer

**Description:** `page::Cancelled`, `page::Closed`, `page::Opened`.

**Fails first:** none.

**Acceptance criteria:**
- [ ] `close_cmd` and `open_cmd` pass unchanged, and the six `open/` goldens' digests are the base's.

**Verification:**
- [ ] `cargo test -p yunta --test close_cmd --test open_cmd`, `cargo test -p yunta-render`

**Dependencies:** Checkpoint H

**Files likely touched:** `crates/cli/src/commands/{cancel/page,close,open}.rs`, `crates/render/src/page/{cancelled,closed,opened}.rs`, `crates/render/src/page.rs`

**Estimated scope:** M

### Task 38: what `resolve-gate`, `gc` and `review` answer

**Description:** `page::Answered`, `page::Collected`, `page::Review`.

**Fails first:** none.

**Acceptance criteria:**
- [ ] The resolve-gate, gc and review tests pass unchanged.

**Verification:**
- [ ] `cargo test -p yunta --bins`, the CLI's batches

**Dependencies:** Task 37

**Files likely touched:** `crates/cli/src/commands/{resolve_gate,gc/page,review}.rs`, `crates/render/src/page/{answered,collected,review}.rs`, `crates/render/src/page.rs`

**Estimated scope:** M

### Task 39: the catalog, `init`, `new` and `schema`

**Description:** `page::Catalog`, `page::Initialized`, `page::NewWorkflow`, `page::Schema`, each moved as Task 29 left it, what it reads staying with its command (`Catalog::read`, `Written::report`). The skill `init` writes is a file, not a page, so it stays with `init`.

**Fails first:** none.

**Acceptance criteria:**
- [ ] The `list`, `schema` and `schema tasks` captures and the init and new tests are unchanged.

**Verification:**
- [ ] `cargo test -p yunta --bins`, the CLI's batches, `pages.sh` against the base

**Dependencies:** Task 38

**Files likely touched:** `crates/cli/src/commands/{list/mod,init,new,schema}.rs`, `crates/render/src/page/{catalog,initialized,new_workflow,schema}.rs`, `crates/render/src/page.rs`

**Estimated scope:** M

### Task 40: a pack's pages

**Description:** a pack's pages, grouped as `page::pack::{Installed, Inventory, Updated, Removed, Listed, NewPack}` rather than six names with a `Pack` prefix in a flat `page::` (the user's choice at Checkpoint G), from `pack/page.rs` and `pack/scaffold.rs` (`NewPack` with `PackCheck`; the templates and `check_pack`, which reads, stay with the command).

**Fails first:** none.

**Acceptance criteria:**
- [ ] The pack tests pass unchanged.

**Verification:**
- [ ] `cargo test -p yunta --bins`, the CLI's batches

**Dependencies:** Task 39

**Files likely touched:** `crates/cli/src/commands/{pack,pack/page,pack/scaffold}.rs`, `crates/render/src/page/pack.rs`, `crates/render/src/page/pack/{installed,inventory,updated,removed,listed,new_pack}.rs`, `crates/render/src/page.rs`

**Estimated scope:** M

### Task 41: `test`'s report, a verdict, and how a driven run stopped

**Description:** `page::Tested` and `page::TestedCase`, `page::Checked` with `Diagnosis` (the verdict's page; what reads it stays with `Verdict`), and `impl Page for VerificationFindings`, which takes the place of the CLI's `VerificationEffectiveness`. `drive` has no page of its own (Task 29): its closing is `page::Run`, and a quiet run's verdict is that page's headline on stderr.

**Fails first:** none.

**Acceptance criteria:**
- [ ] The `check` captures, `run_flow` and the test command's tests pass unchanged.

**Verification:**
- [ ] `cargo test -p yunta --test run_flow`, the CLI's batches, `pages.sh` against the base

**Dependencies:** Task 40

**Files likely touched:** `crates/cli/src/commands/{test/mod,verdict,drive}.rs`, `crates/render/src/page/{tested,verdict,stopped}.rs`, `crates/render/src/page.rs`

**Estimated scope:** M

### Task 42: no CLI file builds a page

**Description:**
- A search for `Doc::new` and `Section {` in `crates/cli/src` finds only `output` and the live region (`surface/`, `status/follow`, `ask/`).
- Anything else it finds moves to its page.
- `crates/render/src/page.rs`'s `//!` lists every page, and `main.rs`'s map says the CLI reads, works and emits.

**Fails first:** none.

**Acceptance criteria:**
- [ ] The search's result is written into the task, with each hit's reason.

**Verification:**
- [ ] `cargo xtask docs --check`, `cargo test -p yunta --bins`

**Dependencies:** Task 41

**Files likely touched:** `crates/render/src/page.rs`, `crates/cli/src/main.rs`

**Estimated scope:** S

### Checkpoint I: after Tasks 37–42
- [ ] The suites; every golden's digest; the captures but the seven texts.
- [ ] The user reviews the phase. Then every step of the `ci` job, commit 8, `cargo clean`.

## Phase 9: the review

### Task 43: what it says

**Description:**
- `cargo xtask docs --about` on the module's whole diff, with every rule, trap and note it prints kept true.
- Each moved module's `//!`, render's map, `.agents/architecture.md`, `docs/` and `CAPABILITY-MAP.md`, read against the code.

**Fails first:** none.

**Acceptance criteria:**
- [ ] `docs --check` and `smells --check` pass, and no doc names a module that moved under its old path.

**Verification:**
- [ ] `cargo xtask docs --check`, `cargo xtask smells --check`

**Dependencies:** Checkpoint I

**Files likely touched:** whatever the reading finds

**Estimated scope:** S

### Task 44: the proof

**Description:**
- The suite's count against the base's 274 binaries and 4,531 tests, explained target by target with `suite_diff.py`.
- A release build: its size against Task 2's.
- `pages.sh` on it, normalized, against the base: equal but for the seven texts.
- Every golden's digest against the base's.

**Fails first:** none: a measurement.

**Acceptance criteria:**
- [ ] Each difference in count, size and capture is explained in `.plans/yunta-tui/proof/run-pages/after.txt`.

**Verification:**
- [ ] Manual check: `after.txt`.

**Dependencies:** Task 43

**Files likely touched:** `.plans/yunta-tui/proof/run-pages/*` (outside git)

**Estimated scope:** S

### Task 45: the review of the module's diff

**Description:**
- The module's diff against `13cdd69c`, read with `code-review-and-quality`: correctness, names, one path, no alias, no page that reads.
- Each finding the lead decides to fix becomes a task here, and is committed on the user's approval.

**Fails first:** per finding.

**Acceptance criteria:**
- [ ] Every finding has a decision and a reason.

**Verification:**
- [ ] Manual check: the findings and their decisions, written into this task.

**Dependencies:** Task 44

**Files likely touched:** per finding

**Estimated scope:** M

### Task 46: the pull request

**Description:**
- Pull request #30's body:
  - the module's list and its test plan;
  - "Still to come", which no longer lists third-party license notices.
- The push, CI on Linux and macOS, and `cargo clean`.

**Fails first:** none.

**Acceptance criteria:**
- [ ] `gh pr checks 30` passes on Linux and macOS.

**Verification:**
- [ ] Manual check: `gh pr view 30`, `gh pr checks 30`, `git status`.

**Dependencies:** Task 45

**Files likely touched:** none in the tree

**Estimated scope:** S

### Checkpoint J: after Tasks 43–46
- [ ] The user reviews the measurements, the review and the pull request.
