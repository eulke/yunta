# Spec: `run-state`

Module of `CAPABILITY-MAP.md`: "The `yunta-replay` crate, cutting the cycle between `replay` and `stats`, each item with one path and a name that says what it is". Status: **approved on 2026-11-07**.
Base: `feature/yunta-tui` at `8c833576`. Depends on: nothing. Needed by: `run-pages` and `control`.

## Objective

What a run is, as its log says, is derived by code that reads nothing but events: replay, the frame a surface draws, the stats, a run's history, the receipt. That code lives inside `yunta-engine`, beside the scheduler, the task cycle, the MCP listener and tokio, so a crate that only reads a run (the pages in `yunta-render`, the control crate, the application) has to depend on the whole engine. This module moves that code into a crate of its own, pure and with no IO, and cuts the one cycle inside it.

Every import of that code changes in this move, so it is also when each item gets the one path and the one name it should always have had: named by what it is, read well where it is called, and reached one way. Nothing a person or an agent sees changes.

### What happens at the base commit (`8c833576`)

Measured on the tree, not taken from the map (the map's "about 2,650 lines" was counted at `34790ab8`):

- **The code is 4,143 lines in eight modules of `crates/engine/src`:** `replay.rs` and `replay/` (1,613), `view/` (1,041), `stats.rs` (412), `receipt/` (338), `verification_effectiveness.rs` (321), `modes.rs` (207), `history.rs` (144), `live.rs` (67). `replay/tree.rs` and `modes.rs` hold 13 inline tests.
- **It reads nothing but its inputs.** None of these files names `tokio`, `rusqlite`, `yunta-storage`, `std::fs`, `std::process`, `std::env`, `Utc::now` or `SystemTime`. They import `yunta-core`, `yunta-surco`, `chrono`, `serde`, `serde_json`, `thiserror`, `std` and each other, and three value types of `process_registry.rs`: `Engine`, `EngineLiveness` and `Prompt`.
- **The value types sit beside the code that reads the disk.** `process_registry.rs` defines them next to `read_registry`, `engine_liveness`, `engine_prompt`, `engine_words` and `Engine::read`, which read `engine.json` through a `lock::OwnerProbe`, and `child_pid`, which takes a `tokio::process::Child`. `Engine::read` has 4 callers; `Engine::this`, which reads nothing, has 20.
- **One cycle.** `replay/activity.rs` counts submissions and finding calls into `Submissions` and `FindingActivity`, which `stats.rs` defines, and `stats.rs` derives from `replay`. Every other edge points one way: `live`, `history`, `receipt` and `verification_effectiveness` read `replay`; `history` reads `stats`; `view` reads `replay`, `stats`, `history`, `live`, `modes` and the registry's value types; `modes` reads none of them.
- **Items have two paths, and some names only read after a rename.**
  - `replay`, `view`, `live` and `receipt` re-export 18 of `yunta-core`'s types (`NodeState`, `NodeWait`, `UnknownKindCount`, `ChildLink`, `Degradation`, `Reroute`, `OpenSession`, `ToolCall`, and ten of `yunta_core::receipt`), so each has a second path.
  - The engine re-exports every item of the eight modules flat at its root. Three need a new name there to make sense alone: `render_json as render_receipt_json`, `analyze as analyze_verification_effectiveness`, `MIN_SAMPLES as VERIFICATION_MIN_SAMPLES`.
- **One derivation, several doors.**
  - The stats have four: `compute_run_stats(workflow, events)`, `compute_run_stats_of(workflow, state, Option<now>)`, `compute_run_stats_at(workflow, events, now)`, and the `stats_observed_at` all three call. Their `None` means "as of the log's last event".
  - The frame has two: `run_frame` from events, `run_frame_of` from a replayed state.
  - The receipt has two: `build_receipt` for a closed run, `receipt_so_far` for an open one.
- **One rule, written twice.** `history::MIN_SAMPLES_FOR_ESTIMATION` and `verification_effectiveness::MIN_SAMPLES` are both 3. The second one's own `///` says it is "same floor, and the same reasoning, as the prior estimation in `history.rs`".
- **Free functions over one type.** `live`'s four functions (`running_since`, `last_event_age`, `open_sessions`, `recent_tool_calls`) each take `(state: &RunState, node, …)`, as `RunState::events_of` and `RunState::answered_unfinished` do as methods. `unknown_kind_counts(state)` is the same.
- **Who reaches it.**
  - Inside the engine, 58 modules outside the eight reach them through `crate::` (`run/`, `run_tools/`, `tasks/`, `scope.rs`, `findings.rs`, `artifacts/shown.rs`, `check/`, `task_cycle/criteria.rs`, `run_hold.rs`), almost all of them `crate::replay`. `derive(events)` alone is called about 200 times.
  - Through `yunta_engine::`, 62 files of the CLI, 66 of the engine's tests and 1 of `yunta-testkit` name what moves. `yunta-render`, `yunta-adapters`, `yunta-storage`, `yunta-core` and `yunta-surco` name none of it.
  - By what each reads, the engine's targets that prove the moving code:
    - `replay` (with `replay/`), `time_spent`, `stats`, `verification_effectiveness`, `live_derivation` and `chronicle` read nothing else of the engine;
    - `logs_from_runs` also reads a pause's sentence through `yunta-render`;
    - `view` builds frames with `yunta-testkit`'s `node_frame`, which stands in the testkit only because the frame stood in the engine;
    - `properties` holds the fold's properties beside runs on a `Bench`.
  - `older_words` reads only `yunta-core` and `yunta-render`, nothing of the engine.
- **What names the paths.**
  - `AGENTS.md` sends "replay, the log store" to `crates/engine/src/replay.rs`, and `xtask/src/smells.rs` (`wildcards_in_folds`) reads that path.
  - `.agents/architecture.md`'s crate table, which `cargo xtask docs --check` holds to every member's manifest, says the engine owns replay. The file stands at 6,045 of its 6,144 bytes.
  - CI's "check each crate in isolation" lists every crate by name, and `release.yml` publishes them in dependency order.
- **The release profile** is `lto = "fat"`, `codegen-units = 1`: a call across two crates is inlined as one inside a crate is.

### Who this is for

- **`run-pages`**, next: the pages move to `yunta-render`, which then depends on this crate instead of reaching into the engine.
- **`control` and `app-core`**: they read a run without hosting one, so they need its state and none of the engine's runtime.
- **Whoever writes the next reader of a run**: one place to look, one path to each item, names that say what they are.
- **A person or an agent**, who sees nothing change: every command, page, receipt, JSON document and golden prints what it prints at the base.

### What each of them gets

1. **A crate for what a run is, as its log says,** at `crates/replay`, named `yunta-replay` (the map's `yunta-state`, renamed at the user's choice). It depends on `yunta-core` and `yunta-surco`, and on `chrono`, `serde`, `serde_json` and `thiserror`, which the workspace already declares. It holds the eight modules and the registry's three value types.
2. **Pure, and held to it.** Its manifest names neither `tokio`, `rusqlite`, `sqlx`, `yunta-storage` nor `yunta-engine`, and a test that reads the manifest says so the day someone adds one, as `no_sql_dependency.rs` does for the engine.
3. **No cycle.** `Submissions` and `FindingActivity` are what replay counts from the log, so they are replay's, and the stats read them from there. A test reads the crate's sources and holds the order of its modules: each names only the modules beneath it.
4. **One path to each item.**
   - The crate's modules are public, and its root re-exports nothing from them; a `yunta-core` type is imported from `yunta-core`, never through this crate.
   - `yunta-engine` re-exports none of it. The engine's modules, its tests, the CLI and `yunta-testkit` import each item from where it lives.
5. **Names by what each thing is,** under "Names and form" below. Each rename changes no behavior; the only changes to code bodies are the cycle's two types, the doors collapsed into one, and `live`'s functions turned into `RunState`'s methods.
6. **Reading the registry stays in the engine.** `read_registry`, `engine_liveness`, `engine_prompt`, `engine_words` and `child_pid` stay where they are. `Engine::read` becomes `read_engine(run_dir, probe, at)`, an engine function that returns the new crate's `Engine`: an inherent method cannot be defined outside its type's crate.
7. **Tests beside their code.** A test stands with the code it proves.
   - A target that proves only the new crate's code moves to `crates/replay/tests` with its fixtures, byte for byte: `replay`, `time_spent`, `stats`, `verification_effectiveness`, `live_derivation`, `view` (as `frame`) and `chronicle`.
   - A target that proves two crates' code splits, each assertion with its owner: `logs_from_runs` asserts the pause value an old log replays to, and `yunta-render` asserts that value's sentence; `properties` moves the fold's, the chronicle's and the frame's properties and keeps those that drive an engine.
   - A type's test samples stand beside the type, behind the crate's `testkit` feature, as `yunta-core`'s do: the frame's samples leave `yunta-testkit`.
   - A test that drives an engine stays in the engine and imports from the new crate. `older_words`, which reads no engine, moves to `yunta-render`.
8. **What names a path says the new one.** That covers:
   - `AGENTS.md`'s row, and the smell counter's path;
   - the crate table of `.agents/architecture.md`: a row for the crate, and the engine's, the CLI's and the testkit's dependencies;
   - the rule tags of the tests that move (`[test engine/logs_from_runs]`, `[test engine/older_words]`);
   - the engine's `//!` and the new crate's;
   - CI's list of crates, and the release's publish order, the new crate published and indexed before `yunta-engine`.

### Names and form

| At the base, in `yunta-engine` | In `yunta-replay` | Why |
|---|---|---|
| `replay` module, `derive(events)` | the crate's root: `RunState::replay(&events)` | Replay is what the crate is; a `replay` module inside `yunta-replay` would say it twice. The call says what is done to what is returned |
| `unknown_kind_counts(state)` | `state.unknown_kinds()` | A read of the state, like its other reads |
| `live::{running_since, last_event_age, open_sessions, recent_tool_calls}(state, node, …)` | methods of `RunState`: `state.open_sessions(&node)` | A module of free functions over one type is that type's methods; `RunState` already reads one node this way |
| `view` module | `frame` | It holds `RunFrame`, `NodeFrame` and the chronicle. `view` is what `yunta-ui` calls its function from a state to a screen, and what the CLI calls its live region |
| `run_frame(…, events, …)`, `run_frame_of(…, state, …)` | `RunFrame::of(…, &state, …)` | One derivation, one door: a caller with events replays them first, once |
| `compute_run_stats`, `compute_run_stats_of`, `compute_run_stats_at`, `stats_observed_at` | `RunStats::at(&state, &workflow, now)`, `RunStats::as_logged(&state, &workflow)` | Two cases, named; no `Option` whose `None` stands for one of them |
| `build_receipt`, `receipt_so_far` | `receipt::closed(…)`, `receipt::so_far(…)` | `Receipt` is a `yunta-core` type, so the module's functions say which receipt they build |
| `receipt::render_json`, re-exported as `render_receipt_json` | removed; its 4 callers write `serde_json::to_string_pretty(&receipt)` | A one-line wrapper that needed a rename to make sense |
| `run_summary(…)`, `prior_estimation(&history)` | `RunSummary::of(…)`, `PriorEstimation::of(&history)` | Constructors of the type they make |
| `MIN_SAMPLES_FOR_ESTIMATION`, `verification_effectiveness::MIN_SAMPLES` | one `history::MIN_SAMPLES` | One rule, one home: both are the fewest past samples a signal across runs needs |
| `verification_effectiveness::analyze`, re-exported as `analyze_verification_effectiveness` | `VerificationFindings::of(&workflow, &history)` | The name reads alone; the module keeps the name of the command's concept |
| `modes::{included_nodes, mode_included_nodes, reached_nodes, dependencies_in_mode}` | `modes::{scheduled, included, reached, dependencies}` | The module already says "mode", and each says which nodes it returns: what a run schedules, what its mode includes, what it reaches, what each waits on |
| `process_registry::{Engine, EngineLiveness, Prompt}` | `registry::{Engine, EngineLiveness, Prompt}` | What a run's registry says, as values. A module named `engine` would be named after another crate |
| `Engine::read(run_dir, probe, at)` | `yunta_engine::read_engine(run_dir, probe, at)` | Reading the disk stays with the engine, beside the registry's other reads |
| 18 `yunta-core` types re-exported through four modules | none | A second path to a core type |

Names that already say what they are stay: `RunState`, `ReplayError`, `TimeSpent`, `Timeline`, `dedup_findings`, `RunFrame`, `NodeFrame`, `RunPhase`, `WaitingOn`, `run_standing`, `node_word`, `chronicle`, `RunStats`, `NodeStat`, `median`, `history`, `RunSummary`, `PriorEstimation`, `budget_caution`, `receipt`, `ReceiptError`, `verification_effectiveness`, `VerificationFindings`, `modes`.

### Decisions the plan starts from

1. **A move that names things right, not a rewrite.** The code bodies keep what they compute, line for line, except the changes "What each of them gets" names. A caller's behavior is the same; only how it reaches the item changes.
2. **No compatibility layer.** No alias, no re-export, no deprecated name: every caller moves to the one path in the same commit.
3. **Nothing of `run-pages` or `control` enters.** `yunta-render` does not depend on the new crate yet; no page moves; `after_for` stays in the CLI, to be split in `run-pages` where its spelling moves.

### Assumptions

1. Nothing a person or an agent reads changes, so the proof is the existing tests and goldens, passing with only their imports and the renamed calls changed, plus the new crate's two tests. No real run and no tokens.
2. This spec's approval grants the changes to `AGENTS.md`, CI, the release workflow and the map's names that it names; they are under "Ask first" for that reason.
3. The crate's version is the workspace's, `0.0.5`, like every internal crate, and it is published with them.
4. `tasks/plan.md` and `tasks/todo.md` are replaced for this module. Those of `surco-screen` stay in its commit, `8c833576`.

### Out of scope

- The pages, `after_for` and the spelling of commands (`run-pages`); `yunta-control` (`control`).
- Any change to what replay derives, to an event, to the log or to `schema_version`.
- Renaming outside what moves: the engine's other modules, the CLI's, `yunta-core`'s types.
- Splitting `yunta-core` or moving anything else out of the engine, which the map leaves out on purpose.

## Tech Stack

Rust at the version `rust-toolchain.toml` pins. A new workspace member with dependencies the workspace already declares in `[workspace.dependencies]`. No new crate from crates.io, so `cargo deny check` sees the same tree.

## Commands

`cargo` means the wrapper that keeps this worktree's build target and state separate from other checkouts: `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.

```
# Before editing a file, and before finishing with no paths
cargo xtask docs --about <files>

# Build
cargo build -p yunta-replay
cargo check -p yunta-replay --locked
cargo build -p yunta

# Test
cargo test -p yunta-replay
cargo test -p yunta-engine --test <target>
cargo test -p yunta --bins
cargo test --workspace --locked --no-fail-fast

# Lint and checks
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo xtask smells --check
cargo xtask docs --check
cargo deny check
```

Before a commit, every step of the `ci` job in `.github/workflows/ci.yml` runs, one build at a time, with `CARGO_INCREMENTAL=0`. After the commit, `cargo clean`. `cargo xtask smells` never runs without `--check`. No `YUNTA_BLESS`: a golden whose bytes change is a defect of this module.

## Project Structure

```
crates/replay/Cargo.toml             core, surco, chrono, serde, serde_json, thiserror
crates/replay/src/lib.rs             what a run is as its log says: RunState, replayed; the crate's map
crates/replay/src/{clock,activity,decisions,refuse,tree}.rs
                                     replay's parts, moved from crates/engine/src/replay/
crates/replay/src/{frame,stats,history,receipt,verification_effectiveness,modes,registry}
                                     the readings of a replayed run, and the registry's values
crates/replay/tests/                 pure; modules in order; the targets that need only this crate
crates/engine/src/process_registry.rs   reads the registry; read_engine returns the crate's Engine
crates/engine/src/**, crates/engine/tests/**, crates/cli/src/**, crates/testkit/src/**
                                     import from yunta_replay and yunta_core, each item one way
Cargo.toml, .github/workflows/{ci,release}.yml, AGENTS.md, .agents/architecture.md, xtask/src/smells.rs,
CAPABILITY-MAP.md                    name the crate and its paths
```

## Code Style

The workspace's own: types that admit only valid states; a `//!` for the crate and each moved module saying how it works and what it keeps; a `///` for each item, with a "Not …" sentence where one is confused with another; a numeric `const` with its reason; no document cited in code, tests or commits. A name is chosen by what the thing is and by how its call reads, never to dodge a clash with an alias.

```rust
//! What a run is, as its log says: the events folded into a [`RunState`],
//! and every reading of a run derived from that state and its inputs: the
//! frame a surface draws, the stats, a workflow's history, the receipt.
//! Pure: nothing here opens a file, reads a clock or spawns a task, so a
//! crate that reads a run needs none of the engine that drives one.

let state = RunState::replay(&events);
let frame = RunFrame::of(&run_id, &workflow, &state, prior.as_ref(), &engine, now);
let stats = RunStats::at(&state, &workflow, now);
```

## Testing Strategy

| Level | What it holds |
|---|---|
| New, `yunta-replay` | Its manifest names no runtime, store or engine crate. Each of its modules names only the modules beneath it |
| Moved | The 13 inline tests and the moved targets, with their fixtures, pass in the new crate unchanged but for paths, renamed calls and the one split assertion; `older_words` passes in `yunta-render` |
| No change | Every other test of the engine, the CLI and the testkit passes with only its imports and renamed calls changed; every golden of the workspace is byte for byte the same |
| Whole | The workspace suite, its count explained against 270 binaries and 4,528 tests: binaries that moved and the new crate's |

- A test asserts an outcome known apart from the code.
- A test whose assertion changes is a defect of this module.

## Boundaries

- **Always:** move a file whole and change only what its new crate and its names need; give each item one path; keep every golden's bytes; run `cargo xtask docs --about` before editing and before finishing; stage paths by name.
- **Ask first:** a new dependency of any crate; a change to what any moved function computes; a test's assertion or a golden's bytes; `AGENTS.md`, CI, the release workflow and the map beyond what this spec names; a name not in "Names and form".
- **Never:** IO, a clock or tokio in the new crate; a second path to any item; an alias kept for a caller; a page, `after_for` or anything of `run-pages` or `control`.

## Success Criteria

1. `crates/replay` is the workspace member `yunta-replay`, holding the eight modules and the three value types. Its manifest names neither `tokio`, `rusqlite`, `sqlx`, `yunta-storage` nor `yunta-engine`, and a test reading it says so.
2. Nothing in replay names the stats, and a test holds the order of the crate's modules.
3. Each moved item has one path: the crate's root re-exports nothing from its modules, it re-exports no `yunta-core` type, and `yunta-engine` re-exports none of it.
4. Every name in "Names and form" stands as written, and no old name is left in the workspace: `derive(`, `run_frame_of`, `compute_run_stats`, `build_receipt`, `render_receipt_json`, `analyze_verification_effectiveness`, `VERIFICATION_MIN_SAMPLES`, `MIN_SAMPLES_FOR_ESTIMATION`, `mode_included_nodes` and `Engine::read` find nothing.
5. The engine reads the registry and nothing else of what moved: no file of `crates/engine/src` defines replay, a frame, the stats, the history, the receipt or the modes.
6. Every golden of the workspace is byte for byte the same. The workspace suite passes, its count explained against 270 binaries and 4,528 tests.
7. The new crate is checked on its own in CI and published before `yunta-engine`. `.agents/architecture.md`'s crate table, `AGENTS.md`'s row and the map name it, and `cargo xtask docs --check` and `cargo xtask smells --check` pass.
8. `cargo deny check` passes as before: the workspace gains no dependency from crates.io.
9. The release binary, built at the base and with the module, differs in size by no more than the base differs from a second build of itself. 15 runs on stub sessions each give a median time between nodes within the base's own spread.

## Open Questions

None open. Answered when the spec was approved, on 2026-11-07:

1. **The crate's name** is `yunta-replay`, at `crates/replay`, with `RunState` and its replay at the root. The map says so, with its rules and its dependency table.
2. **`after_for`** is split in `run-pages`, where its spelling moves to `yunta-render`.
3. **The proof** takes no tokens: the binary's size and 15 runs on stub sessions, base against module.
