# Tasks: `run-state`

Spec: `SPEC-run-state.md`. Plan: `tasks/plan.md`. Status: **approved on 2026-11-07, in implementation**.

**How these run:**
- One at a time, with `incremental-implementation` and `test-driven-development`, stopping at each checkpoint for the user.
- `cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.
- No commit until Checkpoint F. Paths are staged by name.

**How a module changes:**
- Its names change first, in place, with every caller. Then it moves, with every caller's path.
- Neither step leaves an alias, a re-export or an old name, and the workspace compiles and passes after each.
- A task that moves a file does it with `git mv` before it edits it.
- Tasks that rename or move are larger than five files: one path to each item means a name and its callers change in one step. Each of them is mechanical, the compiler names every caller, and the moved bodies are read with `git diff -M` against their old files.

**What fails first:**
- A rename or a move has no new behavior, so no test can fail for it. Its guard is every test and golden it touches, passing unchanged.
- The two new tests are seen red for their own cause before they pass.

## Phase 0: the baseline

### Task 1: the base's size and cost, against itself

**Description:**
- Two release builds of `yunta` at `8c833576`, each kept apart, with the size of each binary.
- `replay_cost.py`, written from `session-ends`' `ends_cost.py`: the same run of four prompt nodes on the Claude Code stub, taking turns on two binaries for 15 rounds. It prints the median, min and max of `between_secs`, `working_secs` and the wall time.
- Run base against base.

**Fails first:** none: a measurement.

**Acceptance criteria:**
- [x] Both sizes, and their difference, are written down.
- [x] Base against base: the three medians and the spread of each.

**Verification:**
- [x] Manual check: `.plans/yunta-tui/proof/run-state/baseline.txt` holds both, with the commands that made them.

**Dependencies:** None

**Files likely touched:** `.plans/yunta-tui/proof/run-state/{replay_cost.py,baseline.txt}` (outside git)

**Estimated scope:** S

### Checkpoint A: after Task 1
- [x] The user sees the baseline and its spread.

## Phase 1: the crate, its purity, and what nothing else depends on

### Task 2: the crate and the test that keeps it pure

**Description:**
- `crates/replay/Cargo.toml` (plan decision 1).
- `crates/replay/src/lib.rs` with the crate's `//!` and nothing else yet.
- `crates/replay/tests/pure.rs`, `the_crate_reads_nothing_but_its_inputs`.
- What names the crate:
  - the workspace's `members` and `[workspace.dependencies]`;
  - CI's "check each crate in isolation";
  - `release.yml`: publish `yunta-replay` after `yunta-core`, then wait for its index before `yunta-engine`;
  - `.agents/architecture.md`'s crate table.

**Fails first:**
- `cargo test -p yunta-replay --test pure`: no such package, until the crate exists.
- Then, with `tokio` added to the manifest by hand, it fails with its message naming `tokio`. The probe is taken back out.

**Acceptance criteria:**
- [x] `yunta-replay` builds alone (`cargo check -p yunta-replay --locked`), and the purity test passes.
- [x] The crate table names it with its dependencies, and `cargo xtask docs --check` passes.
- [x] `release.yml` publishes it before `yunta-engine` and waits for its index.

**Verification:**
- [x] `cargo test -p yunta-replay`, `cargo xtask docs --check`, `cargo xtask smells --check`

**Dependencies:** Checkpoint A

**Files likely touched:** `Cargo.toml`, `Cargo.lock`, `crates/replay/{Cargo.toml,src/lib.rs,tests/pure.rs}`, `.github/workflows/{ci,release}.yml`, `.agents/architecture.md`

**Estimated scope:** M

### Task 3: the modes, by what each returns

**Description:**
- `modes.rs` moves to `crates/replay/src/modes.rs`, a public module.
- Its functions become `scheduled`, `included`, `reached` and `dependencies` (plan decision 7).
- The engine's root stops re-exporting them, and every caller names `yunta_replay::modes::…`: the engine's `check/`, `run/budget.rs`, `run/schedule*`, `run/loop_exec/respecify.rs` and `tasks/mod.rs`, and any test or CLI file the compiler names.
- The engine and every crate that names `modes` gain `yunta-replay`, and the crate table says so.

**Fails first:** none: a move and a rename. Its guards are the 3 inline tests that move with it, and the engine's scheduling and mode tests.

**Acceptance criteria:**
- [x] `crates/engine/src/modes.rs` is gone, and the old names find nothing in `crates/`.
- [x] Every test of the engine and the CLI passes unchanged but for its paths.

**Verification:**
- [x] `cargo test -p yunta-replay`, `cargo test -p yunta-engine`, `cargo test -p yunta --bins`

**Dependencies:** Task 2

**Files likely touched:** `crates/{engine/src/modes.rs → replay/src/modes.rs}`, `crates/engine/src/lib.rs`, the 7 engine modules named above, `crates/engine/Cargo.toml`, `.agents/architecture.md`

**Estimated scope:** L (mechanical)

### Task 4: the registry's values, and the read that stays

**Description:**
- `crates/replay/src/registry.rs` holds `Engine`, `EngineLiveness` and `Prompt`, with `Engine::this` and `Engine::asks_about` (plan decision 8).
- `crates/engine/src/process_registry.rs` keeps every read, returning those values. `Engine::read` becomes `read_engine(run_dir, probe, at)`.
- Every caller names `yunta_replay::registry::…`: the engine's `view/` (still in the engine), its tests, the CLI and the testkit.

**Fails first:** none: a move. Its guards are `process_registry/tests.rs`, the engine's frame and standing tests, and the CLI's region, status and follow tests.

**Acceptance criteria:**
- [x] `Engine::read` finds nothing. `read_engine` has the 4 callers `Engine::read` had.
- [x] Every test of the engine and the CLI passes unchanged but for its paths.

**Verification:**
- [x] `cargo test -p yunta-replay`, `cargo test -p yunta-engine`, `cargo test -p yunta --bins`, `cargo test -p yunta --test status_follow`

**Dependencies:** Task 3

**Files likely touched:** `crates/replay/src/registry.rs`, `crates/engine/src/{process_registry.rs,lib.rs,view/*.rs}`, `crates/cli/src/render/words.rs` and the CLI files that name `Engine`, `crates/testkit/src/…`, their manifests

**Estimated scope:** L (mechanical)

### Checkpoint B: after Tasks 2-4
- [x] The workspace builds and its tests for the three crates pass; `cargo xtask docs --check` and `cargo xtask smells --check` pass.

## Phase 2: replay, and the cycle cut

### Task 5: replay's names, in place

**Description:**
- In the engine, before anything moves:
  - `derive(events)` becomes `RunState::replay(&events)`;
  - `unknown_kind_counts(state)` becomes `state.unknown_kinds()`;
  - `replay` stops re-exporting `NodeState`, `NodeWait` and `UnknownKindCount`, which every caller names from `yunta_core::events`;
  - the field the count reads, `RunState::unknown_kinds`, is `unknown_events`.
- `tests/schedule.rs`, at 499 lines, splits by behaviour (`schedule/failures.rs`), since the longer name takes it over 500.
- About 200 calls change, in the engine's modules and tests, the CLI and the testkit.

**Fails first:** none: a rename. Its guards are the engine's replay, log and older-words tests, and every golden.

**Acceptance criteria:**
- [x] No call of `derive(` on events, no `unknown_kind_counts` and no path to those three types through the engine is left.
- [x] The workspace's tests pass unchanged but for their calls and paths.

**Verification:**
- [x] `cargo test -p yunta-engine`, `cargo test -p yunta --bins`, `cargo test -p yunta-testkit`

**Dependencies:** Checkpoint B

**Files likely touched:** `crates/engine/src/{replay.rs,lib.rs}` and every module that calls `derive`; the engine's tests; the CLI; the testkit

**Estimated scope:** L (mechanical)

### Task 6: replay moves to the crate's root, and the cycle is cut under a test

**Description:**
- `crates/replay/tests/layers.rs`, `each_module_names_only_the_modules_beneath_it` (plan decision 2), is written first.
- `replay.rs`'s code moves into `crates/replay/src/lib.rs`, and `replay/{activity,clock,decisions,refuse,tree}.rs` into `src/` as private files. `TimeSpent`, `Timeline` and `ReplayError` are named at the root.
- `Submissions` and `FindingActivity` first move to `crates/replay/src/stats.rs`, the start of `stats`, then into `activity.rs`.
- Every `crate::replay` of the engine and every `yunta_engine::` path to replay becomes `yunta_replay::`, including the engine's `stats`, `history`, `live`, `view`, `receipt` and `verification_effectiveness`, still in the engine.
- What follows the move:
  - `RunView` stays the engine's, in `run_log.rs`;
  - `RunState::activity` and `Activity` are public while the engine's stats read them;
  - `logs_from_runs` splits (plan decision 11): its closed pull request asserts the `PauseReason` the log replays to, and `yunta-render`'s `pause.rs` tests assert that value's sentence. Then it moves to `crates/replay/tests/` with `tests/fixtures/logs/`, and the rule tag `[test engine/logs_from_runs]` becomes `[test replay/logs_from_runs]`;
  - the `replay` target, with `replay/`, moves to `crates/replay/tests/`;
  - `older_words`, which reads only `yunta-core` and `yunta-render`, moves to `yunta-render`, and its tag becomes `[test render/older_words]`;
  - `tests/suite_guard.rs` splits by behaviour (`suite_guard/held.rs`), since the import takes it over 500 lines;
  - `AGENTS.md`'s row says `crates/replay/src/lib.rs`, and so does the smell counter's path;

**Fails first:**
- `layers.rs`, while `activity.rs` names `crate::stats`: it names the root naming `stats`, a layer above it.
- It passes once the two types stand in `activity.rs` and `stats.rs` is gone again.

**Acceptance criteria:**
- [x] `layers.rs` was seen red for the cycle, and passes.
- [x] `crates/engine/src/replay.rs` and `replay/` are gone; no `crate::replay` is left in the engine, and the engine's root re-exports nothing of replay.
- [x] `logs_from_runs` and `replay` pass in the new crate, and `older_words` in `yunta-render`, with their assertions as they were but for the one split; `cargo xtask docs --check` and `smells --check` pass.

**Verification:**
- [x] `cargo test -p yunta-replay`, `cargo test -p yunta-engine`, `cargo test -p yunta --bins`, `cargo xtask docs --check`, `cargo xtask smells --check`

**Dependencies:** Task 5

**Files likely touched:** `crates/replay/{src/*.rs,tests/layers.rs}`, `crates/engine/src/run_log.rs`, `crates/engine/src/**` (58 modules), `crates/engine/tests/**`, `crates/cli/src/**`, `crates/testkit/src/**`, `AGENTS.md`, `.agents/architecture.md`, `xtask/src/smells.rs`

**Estimated scope:** L (mechanical, beside one new test)

### Checkpoint C: after Tasks 5-6
- [x] The workspace's tests pass, `layers.rs` was seen red then green, and no golden changed.

## Phase 3: the stats and the history

### Task 7: the stats' and the history's names, in place

**Description:**
- In the engine:
  - `RunStats::at` and `RunStats::as_logged` take the place of `compute_run_stats`, `compute_run_stats_of` and `compute_run_stats_at`;
  - `stats_observed_at` becomes the private `observed`;
  - `RunSummary::of` and `PriorEstimation::of` take the place of `run_summary` and `prior_estimation`;
  - one `history::MIN_SAMPLES` takes the place of `MIN_SAMPLES_FOR_ESTIMATION` and `verification_effectiveness::MIN_SAMPLES`, with its reason.
- Every caller replays once and calls the constructor whose case it has.

**Fails first:** none: a rename. Its guards are `time_spent`, the stats and history tests, the verification tests that read the floor, and the CLI's `stats` goldens.

**Acceptance criteria:**
- [x] None of the seven old names finds anything in `crates/`.
- [x] Every test passes unchanged but for its calls and paths; `yunta stats`' goldens are byte for byte.

**Verification:**
- [x] `cargo test -p yunta-replay`, `cargo test -p yunta-engine`, `cargo test -p yunta --bins`

**Dependencies:** Checkpoint C

**Files likely touched:** `crates/engine/src/{stats.rs,history.rs,verification_effectiveness.rs,view/mod.rs,lib.rs}`, `crates/replay/tests/time_spent.rs` once moved or `crates/engine/tests/time_spent.rs`, the CLI's `commands/stats/` and `check.rs`, the engine's tests that call them

**Estimated scope:** L (mechanical)

### Task 8: the stats and the history move

**Description:**
- `stats.rs` and `history.rs` move to public modules of the crate.
- Every path follows them.
- `time_spent` and `stats` move to `crates/replay/tests/`.
- `RunState::activity` and `Activity` return to `pub(crate)`: the stats read them from inside the crate.

**Fails first:** none: a move. Its guard is `layers.rs`, which now reads `stats` and `history` and must still pass, with every test and golden.

**Acceptance criteria:**
- [x] Both files are gone from the engine, and its root re-exports nothing of them.
- [x] `layers.rs`, `time_spent` and `stats` pass in the crate.

**Verification:**
- [x] `cargo test -p yunta-replay`, `cargo test -p yunta-engine`, `cargo test -p yunta --bins`

**Dependencies:** Task 7

**Files likely touched:** `crates/{engine/src → replay/src}/{stats,history}.rs`, `crates/replay/tests/time_spent.rs`, the engine's and the CLI's files that name them

**Estimated scope:** L (mechanical)

## Phase 4: the receipt and the verification findings

### Task 9: the receipt's and the verification findings' names, in place

**Description:**
- In the engine:
  - `receipt::closed` and `receipt::so_far` take the place of `build_receipt` and `receipt_so_far`;
  - `render_json` is gone, and its 4 callers write `serde_json::to_string_pretty(&receipt)`;
  - `receipt` stops re-exporting `yunta_core::receipt`'s ten types;
  - `VerificationFindings::of` takes the place of `analyze`.

**Fails first:** none: a rename. Its guards are the engine's receipt tests and goldens (`crates/engine/goldens/receipt`), the CLI's `receipt` and `stats --workflow`, and `check`'s tests.

**Acceptance criteria:**
- [x] `build_receipt`, `receipt_so_far`, `render_receipt_json` and `analyze_verification_effectiveness` find nothing.
- [x] The receipt's goldens are byte for byte, and every test passes unchanged but for its calls and paths.

**Verification:**
- [x] `cargo test -p yunta-engine --test receipt`, `cargo test -p yunta-engine`, `cargo test -p yunta --bins`, `cargo test -p yunta --test receipt_cmd`

**Dependencies:** Task 8

**Files likely touched:** `crates/engine/src/{receipt/mod.rs,verification_effectiveness.rs,run/pull_request_exec.rs,lib.rs}`, `crates/engine/tests/receipt*`, the CLI's `commands/{receipt,check}.rs` and `commands/stats/`

**Estimated scope:** M

### Task 10: the receipt and the verification findings move

**Description:**
- `receipt/mod.rs` moves to `crates/replay/src/receipt.rs`, and `verification_effectiveness.rs` to its public module.
- Every path follows them.
- The `verification_effectiveness` target moves to `crates/replay/tests/`.

**Fails first:** none: a move. Its guards are `layers.rs`, the receipt's goldens and every test.

**Acceptance criteria:**
- [x] Both are gone from the engine, and its root re-exports nothing of them; `layers.rs` passes.
- [x] The receipt's goldens are byte for byte.

**Verification:**
- [x] `cargo test -p yunta-replay`, `cargo test -p yunta-engine`, `cargo test -p yunta --bins`

**Dependencies:** Task 9

**Files likely touched:** `crates/{engine/src/receipt/mod.rs → replay/src/receipt.rs}`, `crates/{engine → replay}/src/verification_effectiveness.rs`, their callers

**Estimated scope:** M

### Checkpoint D: after Tasks 7-10
- [x] The workspace's tests pass, and every golden of the engine and the CLI is byte for byte.

## Phase 5: what a node is doing now, and the frame

### Task 11: what a node is doing now, as the run state's reads

**Description:**
- `live.rs` becomes `crates/replay/src/attempt.rs`, a private file of the root, as methods of `RunState`: `running_since`, `last_event_age`, `open_sessions` and `recent_tool_calls`.
- `OpenSession` and `ToolCall` are named from `yunta_core::events`.
- The engine's `view/node.rs` and the CLI call the methods.
- The `live_derivation` target, with `live_derivation/`, moves to `crates/replay/tests/`: the stats and what `live` held are both in the crate now.

**Fails first:** none: a move into the type's crate (an inherent method can only stand there). Its guards are the frame's tests and the CLI's region and follow tests.

**Acceptance criteria:**
- [x] `crates/engine/src/live.rs` is gone, and the four free functions find nothing.
- [x] Every test passes unchanged but for its calls and paths.

**Verification:**
- [x] `cargo test -p yunta-replay`, `cargo test -p yunta-engine`, `cargo test -p yunta --bins`, `cargo test -p yunta --test status_follow`

**Dependencies:** Checkpoint D

**Files likely touched:** `crates/{engine/src/live.rs → replay/src/attempt.rs}`, `crates/replay/src/lib.rs`, `crates/engine/src/{view/node.rs,lib.rs}`, the CLI files that call them

**Estimated scope:** M

### Task 12: the frame's names, in place

**Description:**
- In the engine:
  - `view/` becomes `frame/`;
  - `RunFrame::of` takes the place of `run_frame` and `run_frame_of`;
  - `frame` stops re-exporting `ChildLink`, `Degradation` and `Reroute`, which their callers name from `yunta_core::events`.

**Fails first:** none: a rename. Its guards are the engine's `view` target, the CLI's every page and region test, and their goldens.

**Acceptance criteria:**
- [x] `run_frame(` and `run_frame_of` find nothing, and no module is called `view` among what moves.
- [x] Every test and golden passes unchanged but for calls and paths.

**Verification:**
- [x] `cargo test -p yunta-engine`, `cargo test -p yunta --bins`, `cargo test -p yunta --test run_flow`, `cargo test -p yunta --test status_follow`

**Dependencies:** Task 11

**Files likely touched:** `crates/engine/src/{view → frame}/*.rs`, `crates/engine/src/lib.rs`, the engine's `view` target, the CLI's 37 calls of `run_frame` and `run_frame_of`

**Estimated scope:** L (mechanical)

### Task 13: the frame moves, and nothing of the eight is left in the engine

**Description:**
- `frame/` moves to `crates/replay/src/frame/`.
- The frame's samples stand beside the frame (plan decision 11): `yunta-testkit/src/frames.rs` becomes `RunFrame::sample`, `NodeFrame::sample` and their kin in `frame`, behind `yunta-replay`'s `testkit` feature, and `frames.rs` goes. The CLI's tests and the engine's name them from the types.
- The engine's `view` target moves to `crates/replay/tests/frame.rs`, with `frame/{declared,standing,stops}.rs`, and `chronicle` moves beside it.
- `properties` splits: the properties of the fold, the chronicle and the frame move to `crates/replay/tests/properties.rs`, with the log generator; those that drive an engine stay.
- Every path follows. The engine's root re-exports nothing that moved.

**Fails first:** none: a move. Its guards are `layers.rs`, which now reads `frame`, and every test and golden.

**Acceptance criteria:**
- [x] `crates/engine/src` holds none of the eight modules, and `layers.rs` passes.
- [x] Every name of the spec's criterion 4 finds nothing in `crates/`.

**Verification:**
- [x] `cargo test -p yunta-replay`, `cargo test -p yunta-engine`, `cargo test -p yunta --bins`, `cargo check --workspace --all-targets`

**Dependencies:** Task 12

**Files likely touched:** `crates/{engine → replay}/src/frame/`, `crates/{engine/tests/view* → replay/tests/frame*}`, the engine's and the CLI's files that name them

**Estimated scope:** L (mechanical)

### Checkpoint E: after Tasks 11-13
- [x] Every golden of the workspace is byte for byte, and the workspace's tests pass. The search for each old name finds nothing.

## Phase 6: what it says, proof and close

### Task 14: the `//!` and `///` this module makes true

**Description:**
- The crate's `//!` (its map and layers, plan decision 1) and each moved module's.
- The engine's `//!`, whose map loses what left, and its line on the crate graph.
- `cargo xtask docs --about` on the module's diff, each rule and trap it prints kept true.

**Fails first:** `cargo xtask docs --check`, should a moved item leave a link that names nothing; `cargo doc` with `-D warnings`, should a `crate::` link point at the engine.

**Acceptance criteria:**
- [x] `docs --check` and `cargo doc --workspace --no-deps --document-private-items` with `-D warnings` pass.
- [x] No document, comment or test cites a document, the spec or the plan.

**Verification:**
- [x] `cargo xtask docs --check`, `cargo xtask smells --check`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items`

**Dependencies:** Checkpoint E

**Files likely touched:** `crates/replay/src/{lib.rs,*.rs,frame/mod.rs}`, `crates/engine/src/lib.rs`

**Estimated scope:** S

### Task 15: every step of the `ci` job

**Description:** Every step of the `ci` job, one build at a time, with `CARGO_INCREMENTAL=0`, after a `cargo clean`. It runs through `gate.sh` into `.plans/yunta-tui/out/gate-run-state/`, in the foreground or step by step if the machine is short of memory.

**Acceptance criteria:**
- [x] Every step passes: fmt, clippy, build, schema, smells, docs, deny, doc, test, core-release, workflows, crates (which now checks `yunta-replay` alone).
- [x] The test step shows no aborted binary, and its count is explained against 270 binaries and 4,528 tests.
  - The new crate's library binary holds the 13 inline tests, which leave the engine's.
  - `pure` and `layers` are new.
  - `logs_from_runs`, `replay`, `time_spent`, `stats`, `verification_effectiveness`, `live_derivation`, `frame` and `chronicle` moved, `properties` split in two, and `older_words` moved to `yunta-render`.

**Verification:**
- [x] The gate's logs.

**Dependencies:** Task 14

**Files likely touched:** whatever a failing step points at.

**Estimated scope:** S

### Task 16: the module's size and cost, against the base

**Description:**
- A release build of the module's tree, and its binary's size against the base's two.
- `replay_cost.py`, base against module, 15 rounds.

**Fails first:** none: a measurement.

**Acceptance criteria:**
- [x] The module's size differs from the base by no more than the base's two builds differ from each other, or the difference is explained.
- [x] Each module median stands within the base's own spread from Task 1.

**Verification:**
- [x] Manual check: `.plans/yunta-tui/proof/run-state/after.txt`.

**Dependencies:** Task 15

**Files likely touched:** `.plans/yunta-tui/proof/run-state/` (outside git)

**Estimated scope:** S

### Checkpoint F: after Tasks 14-16
- [x] The user reviews the whole diff, read with `git diff -M`, and the measurements. No commit until approved.

### Task 17: commit the module

**Description:**
- One commit for the module on `feature/yunta-tui`, staging paths by name, with a message that cites no document.
- The push, the pull request's list and test plan, and CI on Linux and macOS.
- Then `cargo clean`.

**Acceptance criteria:**
- [ ] The commit holds the module's files and nothing else, including `SPEC-run-state.md`, `CAPABILITY-MAP.md`, `tasks/plan.md` and `tasks/todo.md`.
- [ ] `git show -M --stat HEAD` shows the moved files as renames.
- [ ] CI on the pull request passes on Linux and macOS.

**Verification:**
- [ ] Manual check: `git show -M --stat HEAD`, `git status` and `gh pr checks 30`.

**Dependencies:** Checkpoint F

**Files likely touched:** none beyond the module's.

**Estimated scope:** XS

## The spec's success criteria

| Criterion | Held by |
|---|---|
| 1. The crate, pure | Task 2 |
| 2. No cycle | Task 6, then Tasks 8, 10 and 13 keep `layers.rs` green |
| 3. One path | Tasks 3-13, each removing its re-exports |
| 4. Names | Tasks 3, 5, 7, 9, 11 and 12; Task 13's search |
| 5. The engine reads only the registry | Task 13 |
| 6. Goldens, suite | Every task, then Task 15 |
| 7. CI, release, docs | Tasks 2, 6, 14 and 15 |
| 8. No dependency | Task 2, then Task 15's `deny` step |
| 9. Cost | Tasks 1 and 16 |

## Not in this list

`after_for`, the pages and `yunta-render`'s dependency on the crate are `run-pages`'. `yunta-control` is `control`'s.
