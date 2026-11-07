# Implementation Plan: `run-state`

Spec: `SPEC-run-state.md`, approved on 2026-11-07. Status: **approved on 2026-11-07**. Task list: `tasks/todo.md`. Base: `feature/yunta-tui` at `8c833576`. `cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.

## Overview

What derives a run's state from its log leaves `yunta-engine` for a crate of its own, `yunta-replay`, at `crates/replay`. The move goes module by module, bottom up, so the workspace compiles and its suite passes after each phase.

Every caller's import changes in it, so each item takes its one path and its name in the same step: there is no alias, no re-export and no old name at any point. The phases are:
1. what nothing else depends on: `modes` and the registry's values;
2. replay, with the cycle cut under a test that sees it;
3. the stats and the history;
4. the receipt and the verification findings;
5. what a node is doing now, and the frame;
6. the docs, the proof and the close.

## Architecture Decisions

**1. The crate.** `crates/replay/Cargo.toml`:

```toml
[package]
name = "yunta-replay"
description = "What a run is, as its log says: replay and every reading derived from it"
# version, edition, rust-version, license and repository from the workspace

[dependencies]
yunta-core.workspace = true
# The words a frame calls a run and a node by, and their marks.
yunta-surco.workspace = true
chrono.workspace = true
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
thiserror.workspace = true
```

- The root is replay. `lib.rs` holds what `replay.rs` holds today: `RunState`, its fold and its reads, and `dedup_findings`.
- Replay's parts stay private files of the root: `activity`, `clock`, `decisions`, `refuse`, `tree`, and `attempt` for what `live` held.
- What lives in a private part and is part of the root's API is named once, at the root: `TimeSpent`, `Timeline`, `ReplayError`, `Submissions`, `FindingActivity`. That is its only path.
- `RunView`, the events read and the state they replay to, is the engine's alone (`pub(crate)`, 48 uses): it stays in the engine, in `run_log.rs`, beside the log it reads.
- The readings are public modules, each the only path to its items: `modes`, `registry`, `stats`, `history`, `receipt`, `verification_effectiveness` and `frame`. The root re-exports nothing from them.
- Its `//!`:

```rust
//! What a run is, as its log says: the events folded into a [`RunState`],
//! and every reading of a run derived from that state and its inputs: the
//! frame a surface draws ([`frame`]), the stats, a workflow's history, the
//! receipt, the verification findings, and the nodes a mode schedules.
//!
//! Pure: nothing here opens a file, reads a clock or spawns a task. An
//! instant, the registry's word on the engine, a past run's log: each comes
//! in as an argument its caller read. So a crate that reads a run needs none
//! of the engine that drives one, and a reading at a chosen instant is the
//! same in a test as in production.
//!
//! Its modules stand in layers, each naming only the ones beneath:
//! `modes` and `registry`; replay, at the root; `stats`; `history`; then
//! `receipt`, `verification_effectiveness` and `frame`.
```

**2. The two rules, held by tests.**

```rust
// crates/replay/tests/pure.rs
/// What replay derives depends on its inputs alone, so its manifest names no
/// runtime, no store and no engine: the day one is added, this says where
/// the work belongs instead.
#[test]
fn the_crate_reads_nothing_but_its_inputs() {
    let manifest = include_str!("../Cargo.toml");
    for named in ["tokio", "rusqlite", "sqlx", "yunta-storage", "yunta-engine"] {
        assert!(
            !manifest.contains(named),
            "yunta-replay names {named}; what reads a disk, a clock or a process belongs to the engine"
        );
    }
}

// crates/replay/tests/layers.rs
/// The crate's modules from the bottom. A file belongs to the public module
/// its path starts with, and every other file to replay, at the root.
const LAYERS: [&[&str]; 5] = [
    &["modes", "registry"],
    &["replay"],
    &["stats"],
    &["history"],
    &["receipt", "verification_effectiveness", "frame"],
];

/// What replay folds never depends on what is read off it: each module names,
/// through `crate::`, only modules in a layer beneath its own.
#[test]
fn each_module_names_only_the_modules_beneath_it() { /* walks src/ */ }
```

- `layers.rs` reads `src/` at test time (a test may read files; the crate may not).
- `crate::RunState` names the root, and `crate::stats::RunStats` names `stats`. A file in a layer that names its own layer's sibling, or a layer above, fails with both names.

**3. Replay's names.**

```rust
impl RunState {
    /// The state `events` leave a run in, folded in order.
    pub fn replay(events: &[StoredEvent]) -> RunState;
    /// How many events of each kind this binary does not know, which a
    /// newer engine wrote.
    pub fn unknown_kinds(&self) -> Vec<UnknownKindCount>;
    // The field it counts, `unknown_kinds: Vec<(Seq, String)>`, is `unknown_events`:
    // the method and the field are two things, with two names.

    // attempt.rs, what `live` held: what a node is doing now
    pub fn running_since(&self, node: &NodeId) -> Option<DateTime<Utc>>;
    pub fn last_event_age(&self, node: &NodeId, now: DateTime<Utc>) -> Option<Duration>;
    pub fn open_sessions(&self, node: &NodeId) -> Vec<OpenSession>;
    pub fn recent_tool_calls(&self, node: &NodeId, limit: usize) -> Vec<ToolCall>;
}
```

- `UnknownKindCount`, `NodeState`, `NodeWait`, `OpenSession` and `ToolCall` are imported from `yunta_core::events` by whoever names them.
- `Submissions` and `FindingActivity` move from `stats.rs` into `activity.rs`, and are public at the root: what replay counts, which the stats read.

**4. The stats.**

```rust
impl RunStats {
    /// The run's stats as they stand at `now`: an open attempt reports how
    /// long it has been working, and a run still going measures its clock to
    /// `now`. A run that finished or broke stops at its last event.
    pub fn at(state: &RunState, workflow: &Workflow, now: DateTime<Utc>) -> RunStats;
    /// The run's stats as its log stands, with no instant after it: an
    /// attempt still open has no elapsed time, which is unknown, not zero.
    pub fn as_logged(state: &RunState, workflow: &Workflow) -> RunStats;
}
```

- `stats_observed_at` becomes the private `observed(state, workflow, until: Option<DateTime<Utc>>)` behind the two.
- `compute_run_stats`, `compute_run_stats_of` and `compute_run_stats_at` are gone. Their 31 callers replay once and call one of the two.
- `median` stays in `stats`.

**5. The history.**

```rust
impl RunSummary {
    pub fn of(run_id: RunId, workflow_hash: ContentHash, workflow: &Workflow, events: &[StoredEvent]) -> RunSummary;
}
impl PriorEstimation {
    /// What the past runs say a new one costs; `None` below [`MIN_SAMPLES`].
    pub fn of(history: &[RunSummary]) -> Option<PriorEstimation>;
}
/// Fewer independent samples than this say nothing: a number without a
/// distribution behind it reads like knowledge it is not. Every signal
/// read across past runs, an estimate or a verification finding, keeps it.
pub const MIN_SAMPLES: usize = 3;
```

- `budget_caution` and `Percentiles` stay as they are.

**6. The receipt and the verification findings.**

```rust
// receipt.rs: Receipt and its parts are yunta_core::receipt's, named from there
pub fn closed(run_id: &RunId, manifest: &Manifest, events: &[StoredEvent], event_chain: EventChainStatus) -> Result<Receipt, ReceiptError>;
pub fn so_far(run_id: &RunId, manifest: &Manifest, events: &[StoredEvent], event_chain: EventChainStatus) -> Receipt;

// verification_effectiveness.rs
impl VerificationFindings {
    pub fn of(workflow: &Workflow, history: &[Vec<StoredEvent>]) -> VerificationFindings;
}
```

- `render_json` is gone: its callers write `serde_json::to_string_pretty(&receipt)`.
- `verification_effectiveness::MIN_SAMPLES` is gone: it reads `history::MIN_SAMPLES`.

**7. The modes.**

```rust
pub fn scheduled(workflow: &Workflow, mode: &ModeName, left_out: &[LeftOut]) -> Option<HashSet<NodeId>>;
pub fn included(workflow: &Workflow, mode: &ModeName) -> Option<HashSet<NodeId>>;
pub fn reached<'a>(workflow: &'a Workflow, mode: &ModeName, left_out: &[LeftOut]) -> Vec<&'a Node>;
pub fn dependencies(workflow: &Workflow, included: Option<&HashSet<NodeId>>) -> HashMap<NodeId, Vec<NodeId>>;
```

They were `included_nodes`, `mode_included_nodes`, `reached_nodes` and `dependencies_in_mode`; each body stays as it is.

**8. The registry's values, and the read that stays.**

```rust
// crates/replay/src/registry.rs: what a run's registry says, as values
pub struct Engine { pub liveness: EngineLiveness, pub prompt: Option<Prompt>, pub words: Option<PathBuf>, pub at: DateTime<Utc> }
pub enum EngineLiveness { Alive, Dead, Unrecorded, Unknown }
pub struct Prompt { pub node: Option<NodeId>, pub since: DateTime<Utc>, pub pid: Pid }
impl Engine {
    pub fn this(at: DateTime<Utc>) -> Engine;
    pub fn asks_about(&self, node: &NodeId) -> bool;
}

// crates/engine/src/process_registry.rs
/// What the registry under `run_dir` says about the engine driving it, at `at`.
pub fn read_engine(run_dir: &Path, probe: &dyn OwnerProbe, at: DateTime<Utc>) -> Engine;
```

`engine_liveness`, `engine_prompt`, `engine_words`, `read_registry` and `child_pid` stay, returning the crate's values.

**9. The frame.**

```rust
impl RunFrame {
    /// One run's frame: from its replayed state, the workflow its manifest
    /// froze, the history the caller read, what its registry says about the
    /// engine driving it, and the instant it is looked at.
    pub fn of(run_id: &RunId, workflow: &Workflow, state: &RunState, prior: Option<&PriorEstimation>, engine: &Engine, now: DateTime<Utc>) -> RunFrame;
}
```

- `run_frame` and `run_frame_of` are gone.
- `chronicle`, `run_standing`, `node_word`, `Counter`, `TaskCounter` and every frame type keep their names in `frame`.
- `ChildLink`, `Degradation` and `Reroute` are named from `yunta_core::events`.

**10. Callers change in the step that changes what they call.**
- A module's names change first, in place in the engine, with every caller. Then the module moves, with every caller's path.
- The move does all of this:
  - the file moves with `git mv`, so its history follows;
  - the engine's root stops re-exporting it;
  - each caller is changed: the engine's modules (from `crate::`), its tests, the CLI and the testkit;
  - the crate's dependencies are added to each manifest where its first use arrives, along with the crate table.
- Neither step leaves an alias, a re-export or an old name, and the workspace compiles and passes after each. Where a name can only stand in the new crate (a method of `RunState`), its rename and its move are one step.

A step that leaves an old name or a second path is not done.

**11. Tests beside their code.** A test stands with the code it proves, in the step that moves that code:
- A target that proves only the new crate's code moves into `crates/replay/tests/`, with its fixtures and its assertions as they are.
- A target that proves two crates' code splits, each assertion with the crate that owns what it checks. Together they prove what the one did.
- A type's test samples stand beside the type, behind the crate's `testkit` feature, as `yunta-core`'s literal ids and `sample_payloads` do; the crate's own tests enable it through a dev-dependency on the crate.

The survey of the engine's targets, by what each reads:

| Target | Reads | Where it goes | Task |
|---|---|---|---|
| `logs_from_runs`, with `tests/fixtures/logs/` | replay; one assertion reads a pause's sentence through `yunta-render` | splits: replay asserts the `PauseReason` the old log replays to, and `yunta-render`'s `pause.rs` tests assert that value's sentence; the target and its fixtures move, and its tag becomes `[test replay/logs_from_runs]` | 6 |
| `replay`, with `replay/` | replay and `yunta-testkit-core` | moves | 6 |
| `older_words` | `yunta-core` and `yunta-render`, nothing of the engine | moves to `yunta-render`, and its tag becomes `[test render/older_words]` | 6 |
| `time_spent`, `stats` | the stats and the history | move | 8 |
| `verification_effectiveness` | the verification findings | moves | 10 |
| `live_derivation`, with `live_derivation/` | the stats and what `live` held | moves | 11 |
| `view`, with `view/{declared,standing,stops}.rs` | the frame, and the testkit's `node_frame` | moves as `frame`, once the frame's samples stand beside the frame | 13 |
| `chronicle` | the frame's chronicle | moves | 13 |
| `properties` | replay's fold, the frame and the chronicle; and runs on a `Bench` | splits: the properties of the fold, the chronicle and the frame move as `properties`, with the log generator; those that drive an engine stay | 13 |

A target that drives an engine (a `Bench`, a run) stays in the engine.

The frame's samples: `yunta-testkit/src/frames.rs` builds frame values only because the frame stood in the engine. They become associated functions of their types in `frame`, behind `#[cfg(feature = "testkit")]`, and `frames.rs` goes:

```rust
RunFrame { phase, nodes, ..RunFrame::sample(&RUN) }
NodeFrame { attempt: Some(2), ..NodeFrame::sample(&id, standing) }
```

**12. What names a path, in the step that moves it.**
- From Phase 1:
  - the workspace's `members` and `[workspace.dependencies]`;
  - CI's "check each crate in isolation";
  - `release.yml`: publish `yunta-replay` after `yunta-core`, wait for its index, and only then `yunta-engine`;
  - the crate table of `.agents/architecture.md`.
- From Phase 2: `AGENTS.md`'s "replay, the log store" row (`crates/replay/src/lib.rs`), and `xtask/src/smells.rs`'s `wildcards_in_folds` path.
- Phase 6 writes:
  - the engine's `//!`, its map losing what left;
  - the crate's `//!`, and each moved module's;
  - `CAPABILITY-MAP.md`, already done.

## Dependency Order

```
0 baseline: release size, stub runs
1 crate + pure test + modes + registry ── engine, CLI, testkit import them
    └── 2 replay at the root, the cycle cut under layers.rs
            └── 3 stats, history (RunStats::at, PriorEstimation::of, MIN_SAMPLES)
                    ├── 4 receipt, verification_effectiveness
                    └── 5 attempt (RunState's methods), frame (RunFrame::of)
                            └── 6 docs, gate, measure, review, commit
```

Phases 4 and 5 both stand on 3 and on nothing of each other; they run in that order to keep one change in review at a time.

## Phases

### Phase 0: the baseline
- A release build at `8c833576`, twice: the size of each, and how much the two differ.
- `replay_cost.py`, from `session-ends`' `ends_cost.py`: 15 runs on the Claude Code stub, base against base, with medians of `between_secs`, `working_secs` and the wall time. The engine replays its log at every step, so `between_secs` is where a slower replay would show.
- Kept in `.plans/yunta-tui/proof/run-state/`.

### Checkpoint A
- The baseline's numbers, and the spread of the base against itself.

### Phase 1: the crate, its purity, and what nothing else depends on
- The crate's manifest, `lib.rs` and `tests/pure.rs`. `pure.rs` fails first because the crate does not exist.
- `modes` moves with its new names, and `registry` with the three values. `Engine::read` becomes `yunta_engine::read_engine`.
- The workspace, CI, the release order and the crate table follow (decision 12).

### Checkpoint B
- `cargo test -p yunta-replay`, `-p yunta-engine`, `-p yunta --bins` green. `docs --check` and `smells --check` pass.

### Phase 2: replay, and the cycle cut
- `replay.rs` and `replay/` move into the root, with `RunState::replay` and `unknown_kinds`.
- `tests/layers.rs` is written first, and is seen red: `Submissions` and `FindingActivity` first move into `crates/replay/src/stats.rs`, the start of `stats`, so replay names a module above it. Then they move into `activity.rs`, and the test passes.
- About 200 calls of `derive` and the 58 engine modules that reach `crate::replay` change.
- `logs_from_runs` splits and moves, `replay` moves, and `older_words` moves to `yunta-render` (decision 11). `AGENTS.md` and the smell counter's path follow.

### Checkpoint C
- The same suites green, and `layers.rs` seen red, then green. No `derive(` call and no `crate::replay` is left.

### Phase 3: the stats and the history
- `stats.rs` and `history.rs` move, with `RunStats::at` and `as_logged`, `RunSummary::of`, `PriorEstimation::of` and one `MIN_SAMPLES`.
- `time_spent` moves.

### Phase 4: the receipt and the verification findings
- `receipt/` becomes `receipt.rs`, with `closed` and `so_far`; `render_json` is gone. `verification_effectiveness.rs` moves, with `VerificationFindings::of`.

### Checkpoint D
- After Phases 3 and 4: the same suites green, and the engine's receipt goldens byte for byte.

### Phase 5: what a node is doing now, and the frame
- `live.rs` becomes `attempt.rs`, as methods of `RunState`. `view/` becomes `frame/`, with `RunFrame::of`.
- The `view` target moves as `frame`.
- No file of the eight is left in `crates/engine/src`.

### Checkpoint E
- The same suites green, and every golden of the CLI and the engine byte for byte. A search for each old name in the spec's criterion 4 finds nothing.

### Phase 6: what it says, proof and close
- The `//!`s (decision 12), and `cargo xtask docs --about` on the module's diff.
- Every step of the `ci` job, one build at a time, with `CARGO_INCREMENTAL=0`.
- The release size and `replay_cost.py`, base against module.

### Checkpoint F
- The user reviews the whole diff and the measurements. No commit until approved.
- Then: one commit, the push, the pull request's list and test plan, CI on Linux and macOS, and `cargo clean`.

## Verification

| Spec criterion | How |
|---|---|
| 1. The crate, pure | `tests/pure.rs`; `cargo tree -p yunta-replay -e normal` lists no store. It lists `tokio`, which reaches it only through `yunta-core`, whose port and process modules declare it: `core` with no IO is out of the map on purpose |
| 2. No cycle | `tests/layers.rs`, seen red in Phase 2 before the cut |
| 3. One path | A search for `pub use` in `crates/replay/src/lib.rs` finds only its private parts' items; the engine's `lib.rs` names nothing that moved; `yunta_core` types are named from `yunta_core` |
| 4. Names | A search for each old name finds nothing in `crates/` |
| 5. The engine reads only the registry | `crates/engine/src` holds none of the eight files |
| 6. Goldens, suite | `git status` shows no golden changed; the suite's count is explained against 270 binaries and 4,528 tests. The new crate adds its library's binary and `pure` and `layers`, and the moved targets keep their tests |
| 7. CI, release, docs | The gate's `crates` step checks `yunta-replay` alone; `release.yml` reviewed; `docs --check` and `smells --check` |
| 8. No dependency | `cargo deny check`, and `Cargo.lock`'s diff adds only the workspace's own crate |
| 9. Cost | The release size and `replay_cost.py`, base against module, within the base's own spread |

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| A diff of about 190 files hides a change of behavior | High | No assertion changes, no golden changes; each phase compiles and passes before the next; the bodies of moved functions are compared with `git diff -M` against their old files |
| A rename leaves a caller on an old name | Medium | The compiler finds every one: no old name survives as an alias. Criterion 4's search confirms |
| Git loses a file's history | Low | `git mv` before any edit, and each moved file's diff read with `-M` |
| The crate table outgrows `.agents/architecture.md`'s 6,144 bytes | Medium | The row and the dependency lists take about 85 of the 99 bytes left; the engine's row loses "replay". If it does not fit, cut what the code already says, never the table |
| A doc link names `crate::` in the engine | Low | `cargo doc` with `-D warnings`, which the gate runs |
| A call across the crates costs time | Low | `lto = "fat"`, `codegen-units = 1`; Phase 0 and Phase 6 measure it |
| The suite's memory on this Mac | Medium | The gate runs in the foreground or with its steps apart, as the last module's had to |

## Parallelization

None. Each phase changes the imports the next one reads. A worker per caller crate within one phase would edit the same moved files' callers, so the lead moves each module and its callers in one pass, and the compiler checks it.

## Open Questions

None. The spec's three were answered when it was approved.
