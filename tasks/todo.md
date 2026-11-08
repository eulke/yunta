# Tasks: `run-pages`

Spec: `SPEC-run-pages.md`. Plan: `tasks/plan.md`. Status: **draft, for approval**. Base: `13cdd69c`.

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
- [ ] `advice` defines neither function, and the moved test passes.

**Verification:**
- [ ] `cargo test -p yunta-replay`, `cargo test -p yunta --bins`

**Dependencies:** Checkpoint C

**Files likely touched:** `crates/replay/src/frame/mod.rs`, `crates/replay/tests/frame.rs`, `crates/cli/src/{commands/advice.rs,commands/advice/tests.rs,commands/status/mod.rs,json/mod.rs,surface/view/mod.rs}`

**Estimated scope:** M

### Task 15: the actions a run offers

**Description:**
- `crates/replay/src/frame/actions.rs`: `Action`, `Effect` and `RunFrame::actions`. `after_for`'s rules move whole: `child_parked_on`, `after_needs_you`, `shows_plan`, `waits_for_tree` and `after_reset` become its private parts.
- `crates/render/src/command.rs`: `effect`, the words of each `Effect`, the same 11 sentences.
- The run page (`after` twice) and the JSON (`after_for` with an agent) take `frame.actions(decision)`, spelled with `command::line` or `line_for` beside `command::effect`.
- `advice::{after, after_for, after_reset}` are gone.
- `advice/tests.rs`'s six tests of what a run is offered move to `crates/replay/src/frame/actions/tests.rs`, asserting `Action`s.

**Fails first:** the six moved tests do not build until `RunFrame::actions` exists, and each then passes for the rule it names.

**Acceptance criteria:**
- [ ] A search for `after_for`, `after_reset` and `advice::after` finds nothing.
- [ ] `status --json`'s next steps and every status and closing golden are byte for byte the same.

**Verification:**
- [ ] `cargo test -p yunta-replay -p yunta-render`, `cargo test -p yunta --bins`, `cargo test -p yunta --test status_cmd --test run_flow`, the parked-runs targets, `pages.sh` against the base

**Dependencies:** Task 14

**Files likely touched:** `crates/replay/src/frame/{mod,actions}.rs`, `crates/replay/src/frame/actions/tests.rs`, `crates/render/src/command.rs`, `crates/cli/src/{run_page/mod.rs,json/mod.rs,commands/advice.rs,commands/advice/tests.rs}`

**Estimated scope:** M

### Checkpoint D: after Tasks 14–15
- [ ] The suites; every golden's digest; the JSON captures byte for byte.
- [ ] The user reviews the phase. Then every step of the `ci` job, commit 3, `cargo clean`.

## Phase 4: one decision, and the words of a parked run

### Task 16: a decision's record and routes, in `escalation`

**Description:**
- `gate::decision`, `gate::record`, `gate::routes` and `Route` move from `crates/render/src/gate.rs` to `crates/render/src/escalation.rs`.
- Their callers change path: render's `gate` and `published`, the CLI's `ask/card`, `ask/decision`, `resolve_gate` and `human_interaction`, and their tests (`render/src/tests/{deciding,routes}.rs`, `cli/src/ask/card/tests.rs`).

**Fails first:** none: a move.

**Acceptance criteria:**
- [ ] `gate` defines none of the four, and every gate and decision golden is byte for byte the same.

**Verification:**
- [ ] `cargo test -p yunta-render`, `cargo test -p yunta --bins`

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
- [ ] `crates/cli/src/commands/status/decision.rs` is gone, and so is `Decided`.
- [ ] The status, closing and gate goldens and the decision's JSON are byte for byte the same.

**Verification:**
- [ ] `cargo test -p yunta-render`, `cargo test -p yunta --bins`, `cargo test -p yunta --test status_cmd`

**Dependencies:** Task 16

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
- [ ] A search for `advice` in `crates/` finds no module, path or test.
- [ ] Every golden is byte for byte the same.

**Verification:**
- [ ] `cargo test -p yunta-render`, `cargo test -p yunta --bins`, the CLI's batches

**Dependencies:** Task 17

**Files likely touched:** `crates/render/src/{pause.rs,pause/tests.rs}`, `crates/cli/src/{commands/mod.rs,commands/advice.rs,commands/advice/tests.rs,commands/list/inbox.rs,commands/status/progress.rs,error.rs,run_page/mod.rs,surface/lines.rs,surface/view/mod.rs}`

**Estimated scope:** M (a move)

### Checkpoint E: after Tasks 16–18
- [ ] The suites; every golden's digest; the decision's JSON.
- [ ] The user reviews the phase. Then every step of the `ci` job, commit 4, `cargo clean`.

## Phase 5: what is left of the CLI's `render`

### Task 19: how far a run has come, in render

**Description:**
- `crates/cli/src/render/counter.rs` moves, with `git mv`, to `crates/render/src/progress.rs`, with its tests.
- `status/progress.rs`'s `summary` joins it, and `receipt::unknown_kinds` moves to `crates/render/src/state.rs`.
- Their callers change path: `list/inbox`, `run_page/facts`, `status/progress`, `json/mod`, `stats/run` and the receipt.

**Fails first:** none: a move.

**Acceptance criteria:**
- [ ] `crate::render::counter` and `progress::summary` are gone, and every progress line is the same.

**Verification:**
- [ ] `cargo test -p yunta-render`, `cargo test -p yunta --bins`

**Dependencies:** Checkpoint E

**Files likely touched:** `crates/render/src/{progress,state,receipt,lib}.rs`, `crates/cli/src/{render/{mod,counter}.rs,commands/status/progress.rs,commands/list/inbox.rs,run_page/facts.rs,json/mod.rs,commands/stats/run.rs}`

**Estimated scope:** M (a move)

### Task 20: each caller replays once

**Description:**
- `status/progress.rs` is deleted: its `frame` and `frame_of` are gone. Their callers replay once and call `RunFrame::of`: `cancel`, `drive`, `list/inbox`, `mcp/tools`, `resume`, `status/{follow,mod}` and `graph`.
- `render::engine` is gone. Its callers (`list/inbox`, `stats/mod`, `status/follow`, `status/mod`) call `yunta_engine::read_engine(run_dir, &SystemProbe, at)`.

**Fails first:** none.

**Acceptance criteria:**
- [ ] A search for `progress::frame` and `render::engine` finds nothing, and no caller replays one log twice (`rs_twice_any.py`).

**Verification:**
- [ ] `cargo test -p yunta --bins`, the CLI's batches

**Dependencies:** Task 19

**Files likely touched:** `crates/cli/src/{commands/status/progress.rs,commands/cancel.rs,commands/drive.rs,commands/list/inbox.rs,commands/mcp/tools.rs,commands/resume.rs,commands/status/{follow,mod}.rs,graph.rs,commands/stats/mod.rs,render/words.rs}`

**Estimated scope:** L (a rename)

### Task 21: a node's words, and the runs a run bore, in render

**Description:**
- `render::node_display(&node)` becomes `impl From<&NodeFrame> for NodeDisplay`, and `render::called` moves to `crates/render/src/state.rs`. Their 8 callers write `NodeDisplay::from(&node)`.
- `crates/cli/src/surface/view/children.rs` moves, with `git mv`, to `crates/render/src/children.rs`, with its tests. Its callers are `run_page/facts`, `surface/chronicle/mod` and `surface/view/mod`, and `surface/mod.rs` re-exports it no more.

**Fails first:** none: a move.

**Acceptance criteria:**
- [ ] `render/words.rs` holds only `RunExit` and `exit`, and every golden is the same.

**Verification:**
- [ ] `cargo test -p yunta-render`, `cargo test -p yunta --bins`

**Dependencies:** Task 20

**Files likely touched:** `crates/render/src/{state,children,lib}.rs`, `crates/cli/src/{render/{mod,words}.rs,surface/{mod,view/mod,view/children,chronicle/mod}.rs,run_page/{facts,story}.rs,commands/stats/run.rs,commands/status/node.rs,graph.rs,json/node.rs}`

**Estimated scope:** L (a move)

### Task 22: the CLI's output owns its streams

**Description:**
- `crates/cli/src/output.rs` moves to `crates/cli/src/output/mod.rs`, and `render/stream.rs` to `output/stream.rs`, both with `git mv`.
- `surface_lines`, `blocks_lines` and `draw` join `output/mod.rs`.
- Their 36 callers change path, from `crate::render::{glyphs, settle_*, stdout_look, stderr_look, …}` to `crate::output::…`.

**Fails first:** none: a move.

**Acceptance criteria:**
- [ ] `crates/cli/src/render/` holds only `words.rs`.

**Verification:**
- [ ] `cargo test -p yunta --bins`, the CLI's batches

**Dependencies:** Task 21

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
- [ ] A search for `crate::render` and `error::Outcome` finds nothing.
- [ ] `a_run_waiting_on_a_clock_exits_on_its_own_and_a_stop_is_not_a_success` passes in its new place.

**Verification:**
- [ ] `cargo test -p yunta --bins`, the CLI's batches, `cargo xtask docs --check`

**Dependencies:** Task 22

**Files likely touched:** `crates/cli/src/{output/exit.rs,output/mod.rs,render/words.rs,error.rs,main.rs}`, and the 39 callers

**Estimated scope:** L (a move)

### Checkpoint F: after Tasks 19–23
- [ ] The suites; every golden's digest; the captures.
- [ ] The user reviews the phase. Then every step of the `ci` job, commit 5, `cargo clean`.

## Phase 6: one form

### Task 24: `Page`, and an answer that hands its page

**Description:**
- `crates/render/src/page.rs`: `Page` and `impl<P: Page + ?Sized> Page for &P`, with the `//!` that maps every page.
- `Answer` in `output/mod.rs` loses `document` and gains `fn page(&self) -> impl Page + '_`, which `emit` draws. `output::show(look, &page)` draws a page for a command with no JSON.
- In place, each of the 11 answers moves its `document` into `impl Page for <Answer>`, and its `page()` returns `self`:
  - `status`, `list/runs`, `stats/mod` (two), `doctor`, `verify`;
  - `open`, `resolve_gate`, `receipt`, `close`, `cancel/page`.

**Fails first:** none: a form. The guard is every golden and the captures.

**Acceptance criteria:**
- [ ] No `fn document` is left in an `impl Answer`.
- [ ] The JSON captures are byte for byte the same.

**Verification:**
- [ ] `cargo test -p yunta-render`, `cargo test -p yunta --bins`, the CLI's batches, `pages.sh` against the base

**Dependencies:** Checkpoint F

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
- [ ] The receipt's and the findings' goldens and the forge's receipt body are byte for byte the same.

**Verification:**
- [ ] `cargo test -p yunta-render -p yunta-adapters`, `cargo test -p yunta --test receipt_cmd`, the engine's `receipt` target

**Dependencies:** Task 24

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
- [ ] The `document/` goldens are byte for byte the same.

**Verification:**
- [ ] `cargo test -p yunta-render`, `cargo test -p yunta --bins`

**Dependencies:** Task 25

**Files likely touched:** `crates/render/src/{shown,spec,plan/*,page,gate,published,escalation/decision}.rs`, `crates/render/src/page/{shown,spec,plan/*}.rs`, `crates/cli/src/{ask/desk.rs,ask/decision.rs,commands/review.rs,commands/status/{mod,node}.rs}`

**Estimated scope:** L (a move)

### Task 27: the gate's page

**Description:**
- `crates/render/src/gate.rs` moves, with `git mv`, to `crates/render/src/page/gate.rs`.
- `GatePage` with `page()`, `whole_page()` and `above()` becomes `page::Gate`: one value, whose placement says whether it is drawn whole or above the prompt that answers it.
- Its callers fill it: `ask/decision`, `published`, and the adapters through `published`.

**Fails first:** none.

**Acceptance criteria:**
- [ ] The `gate/` goldens, `pull-request.md` among them, are byte for byte the same.

**Verification:**
- [ ] `cargo test -p yunta-render -p yunta-adapters`, `cargo test -p yunta --bins`

**Dependencies:** Task 26

**Files likely touched:** `crates/render/src/{gate,page,published,lib}.rs`, `crates/render/src/page/gate.rs`, `crates/render/src/tests/console_page.rs`, `crates/cli/src/ask/decision.rs`

**Estimated scope:** M

### Task 28: the run's pages take the form in place

**Description:**
- `RunPage` implements `Page`.
- `NodePage`, `RunStatsPage`, `WorkflowPage`, the field's page and the doctor's and `verify`'s checklist each become a struct implementing `Page`, still in the CLI.
- Each command fills its page, and its answer's `page()` returns it.

**Fails first:** none.

**Acceptance criteria:**
- [ ] The status, closing, open, field and stats goldens are byte for byte the same.

**Verification:**
- [ ] `cargo test -p yunta --bins`, the CLI's batches

**Dependencies:** Task 27

**Files likely touched:** `crates/cli/src/{run_page/mod.rs,commands/status/{mod,node}.rs,commands/list/runs.rs,commands/stats/{mod,run,workflow}.rs,commands/doctor/mod.rs,commands/verify.rs}`

**Estimated scope:** L (a form)

### Task 29: every other command's page takes the form in place

**Description:**
- Each of the 16 pages of plan decision 7's last five rows becomes a struct implementing `Page`, named as there, still in its command's file. Its command fills it and emits it.
- `drive`, `init`, `review`, `schema` and `verdict` draw through `output::show`.
- `output::document` is left only if `status/follow`, part of the live region, still needs to print a `Doc` that is no page. Otherwise it is gone.

**Fails first:** none.

**Acceptance criteria:**
- [ ] Each of the 16 is a `Page`, and the captures are byte for byte the same.
- [ ] `output::document`'s callers are the live region's alone, or none.

**Verification:**
- [ ] `cargo test -p yunta --bins`, the CLI's batches, `pages.sh` against the base

**Dependencies:** Task 28

**Files likely touched:** `crates/cli/src/commands/{cancel/page,close,open,resolve_gate,gc/page,review,list/mod,init,new,schema,pack,pack/page,pack_audit,pack/scaffold,test/mod,verdict,drive}.rs`

**Estimated scope:** L (a form)

### Checkpoint G: after Tasks 24–29
- [ ] The suites; render's 21 goldens and the CLI's 78 by digest; the captures.
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
- `Inbox`'s `Group` and `RunRow` become `page::field::{Group, Row}` (plan decision 9): a row holds the run, the action it is listed for, and what holds it. The page words it.
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

**Description:** `page::Catalog`, `page::Initialized`, `page::NewWorkflow`, `page::Schema`. The skill `init` writes is a file, not a page, so it stays with `init`.

**Fails first:** none.

**Acceptance criteria:**
- [ ] The `list`, `schema` and `schema tasks` captures and the init and new tests are unchanged.

**Verification:**
- [ ] `cargo test -p yunta --bins`, the CLI's batches, `pages.sh` against the base

**Dependencies:** Task 38

**Files likely touched:** `crates/cli/src/commands/{list/mod,init,new,schema}.rs`, `crates/render/src/page/{catalog,initialized,new_workflow,schema}.rs`, `crates/render/src/page.rs`

**Estimated scope:** M

### Task 40: a pack's pages

**Description:** `page::Installed` (from `pack.rs` and `pack/page.rs`), `page::PackInventory` (from `pack_audit.rs`, with the inventory `pack/page.rs` heads), `page::NewPack` (from `pack/scaffold.rs`; its templates stay with the command).

**Fails first:** none.

**Acceptance criteria:**
- [ ] The pack tests pass unchanged.

**Verification:**
- [ ] `cargo test -p yunta --bins`, the CLI's batches

**Dependencies:** Task 39

**Files likely touched:** `crates/cli/src/commands/{pack,pack/page,pack_audit,pack/scaffold}.rs`, `crates/render/src/page/{installed,pack_inventory,new_pack}.rs`, `crates/render/src/page.rs`

**Estimated scope:** M

### Task 41: `test`'s report, a verdict, and how a driven run stopped

**Description:** `page::Tested`, `page::Verdict`, `page::Stopped`. `drive`'s note of a verdict draws `page::Verdict` through `output`.

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
