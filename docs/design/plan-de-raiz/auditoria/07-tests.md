Audit complete. All numbers below were reproduced from the tree (the smells baseline by actually running `cargo run -p xtask -- smells`, which rewrote the file byte-identically — `git status` clean).

---

# Yunta test-architecture audit

Judged against `CLAUDE.md` → **Código** ("tests que nombran el comportamiento; la infraestructura de test vive en el crate de soporte; el entorno se inyecta; la sincronización es explícita, nunca un sleep; property tests para replay, idempotencia y resume"), **Un lugar**, **Evidencia**.

**Headline:** the *design* of the testkit is right and the test *names* are exemplary. The *adoption* is not: 46 hand-rolled copies of the canonical run recipe, 8 shadow `Bench` structs, 10 local `event()` builders against a `testkit::stored` with **zero users**, two whole crates (`core`, `adapters`) that never link the support crate at all, and a smells ratchet whose `copied_test_helpers 0` is measuring four literal strings while the real count is in the dozens.

---

## 1. INVENTORY

**110 integration test binaries + 1 shared module (`engine/tests/common/mod.rs`), 48 826 lines, 1 260 integration test fns + 206 unit test fns = 1 466 tests.**

| crate | files | integration tests | unit tests (`#[cfg(test)]` in `src`) | lines of tests |
|---|---|---|---|---|
| `core` | 20 | 265 | 39 | 5 798 |
| `storage` | 1 | 26 | 0 | 721 |
| `adapters` | 6 | 98 | 14 | 3 245 |
| `engine` | 56 (+`common/mod.rs`) | 666 | 24 | 28 690 |
| `cli` | 27 | 205 | 125 | 9 640 |
| `testkit` | — | — | 0 | — |
| `xtask` | — | — | 4 | — |

<details><summary>Full per-file table (file · lines · #tests · what it covers)</summary>

| file | lines | #tests | covers |
|---|---:|---:|---|
| `adapters/tests/claude_code.rs` | 935 | 32 | real claude-code adapter vs `fixtures/claude_code_stub.sh` |
| `adapters/tests/codex.rs` | 961 | 28 | real codex adapter vs `fixtures/codex_stub.sh` |
| `adapters/tests/forge_github.rs` | 561 | 8 | `GitHubForge` vs a local axum stub of GitHub's REST API |
| `adapters/tests/mock.rs` | 691 | 25 | mock adapter fixture semantics (no module doc) |
| `adapters/tests/process_start.rs` | 28 | 2 | process start time from the host process table |
| `adapters/tests/signal.rs` | 69 | 3 | the single signal boundary, errno surfaced |
| `cli/tests/check.rs` | 255 | 8 | `yunta check` end to end (no module doc) |
| `cli/tests/check_keys_cmd.rs` | 49 | 1 | unknown keys named individually |
| `cli/tests/console_interaction.rs` | 599 | 16 | pty-driven `yunta run` question/gate round |
| `cli/tests/docs_sync.rs` | 231 | 2 | README ↔ `--help`; every doc YAML block accepted |
| `cli/tests/factory_packs_cmd.rs` | 186 | 4 | `pack add/--run-tests/check` against real `packs/` |
| `cli/tests/factory_packs_structural.rs` | 59 | 1 | no crate source names a factory pack |
| `cli/tests/gc_cmd.rs` | 280 | 7 | `yunta gc` retention |
| `cli/tests/graph_cmd.rs` | 308 | 7 | `yunta graph` Mermaid/DOT |
| `cli/tests/init_new_cmd.rs` | 271 | 14 | `init`/`new` non-TTY, idempotent |
| `cli/tests/integration.rs` | 22 | 2 | `--version`, bare usage |
| `cli/tests/mcp_flow.rs` | 1 010 | 12 | `yunta mcp` real stdio JSON-RPC round trip |
| `cli/tests/pack_audit_cmd.rs` | 116 | 3 | `pack audit` |
| `cli/tests/pack_ceiling_cmd.rs` | 111 | 3 | executor confirm gate + `declares.permissions` ceiling |
| `cli/tests/pack_cmd.rs` | 535 | 15 | `pack add/remove/list/update` |
| `cli/tests/pack_policy_cmd.rs` | 242 | 6 | `permissions.packs` allow-list |
| `cli/tests/pack_provenance_cmd.rs` | 126 | 1 | manifest freezes pack + version |
| `cli/tests/pack_requires_doctor_cmd.rs` | 113 | 2 | `doctor` vs `requires:` |
| `cli/tests/pack_resolution_cmd.rs` | 127 | 4 | `<publisher>/<name>` resolution |
| `cli/tests/parked_runs.rs` | 623 | 9 | parked run from both sides |
| `cli/tests/receipt_cmd.rs` | 145 | 3 | `yunta receipt` |
| `cli/tests/run_flow.rs` | **2 890** | 52 | the CLI's whole run/status/resume surface |
| `cli/tests/run_surface.rs` | 487 | 13 | what `run`/`resume` draw (pty + `--quiet`) |
| `cli/tests/schema_cmd.rs` | 103 | 6 | `yunta schema` |
| `cli/tests/stats_cmd.rs` | 211 | 5 | `stats` ≤80 cols, no ANSI |
| `cli/tests/status_cmd.rs` | 333 | 5 | `status` on an unclosed document |
| `cli/tests/verification_effectiveness_cmd.rs` | 91 | 2 | dead re-route surfaced in `check`/CLI |
| `cli/tests/verify_cmd.rs` | 117 | 2 | hash-chain CLI face |
| `core/tests/artifact_ledger.rs` | 266 | 10 | artifact fold (+1 property) |
| `core/tests/artifacts.rs` | 165 | 11 | `tasks` named the same at every door |
| `core/tests/config.rs` | 804 | 39 | config layers/merge (no module doc) |
| `core/tests/diagnostic.rs` | 353 | 19 | diagnostics in document vocabulary |
| `core/tests/error.rs` | 22 | 2 | (no module doc) |
| `core/tests/events.rs` | 631 | 16 | event payload shapes (no module doc) |
| `core/tests/finding_ledger.rs` | 263 | 6 | finding fold (+1 property) |
| `core/tests/id_source.rs` | 52 | 3 | injected id source |
| `core/tests/ids.rs` | 338 | 21 | newtype parse-don't-validate |
| `core/tests/integration.rs` | 290 | 8 | (no module doc) |
| `core/tests/pack.rs` | 121 | 5 | `pack.yaml` schema |
| `core/tests/schema_json.rs` | 126 | 5 | schemas generated from the types |
| `core/tests/schema_shape.rs` | 177 | 9 | no tri-state, no accidental order |
| `core/tests/shape.rs` | 182 | 15 | reading an agent-written document |
| `core/tests/shape_roundtrip.rs` | 208 | 4 | render/read/accept round trip (3 properties) |
| `core/tests/strict_keys.rs` | 190 | 13 | unknown keys refused, named |
| `core/tests/tasks_rules.rs` | 298 | 16 | tasks-document rules |
| `core/tests/vocabulary.rs` | 191 | 11 | every closed set spelled once |
| `core/tests/workflow.rs` | 1 048 | 47 | workflow schema (no module doc) |
| `core/tests/yaml.rs` | 73 | 5 | YAML frontier names the failing value |
| `engine/tests/artifact_store.rs` | 158 | 7 | object store, content identity |
| `engine/tests/artifacts.rs` | 668 | 23 | node close over declared artifacts |
| `engine/tests/artifacts_audit.rs` | 171 | 2 | per-node artifact isolation |
| `engine/tests/blackboard.rs` | 402 | 7 | `coordination: blackboard` via real MCP |
| `engine/tests/cancel.rs` | 192 | 2 | `CancellationToken` mid-session |
| `engine/tests/catalog.rs` | 296 | 11 | namespaced workflow resolution |
| `engine/tests/check.rs` | 1 969 | 74 | static validation (no module doc) |
| `engine/tests/common/mod.rs` | 982 | 0 | shared engine fixtures (see §2) |
| `engine/tests/degradation.rs` | 229 | 3 | every degradation lands on the log |
| `engine/tests/escalation.rs` | 673 | 12 | `current_escalation` from the log |
| `engine/tests/events_export.rs` | 114 | 3 | `render_events_jsonl` |
| `engine/tests/external_gate.rs` | 423 | 7 | `external: pull_request` via `MockForge` |
| `engine/tests/factory_packs.rs` | 231 | 1 | fragua full pipeline under mock |
| `engine/tests/git.rs` | 74 | 4 | `GitError` rendering |
| `engine/tests/inputs.rs` | 397 | 18 | `resolve_inputs` |
| `engine/tests/live_derivation.rs` | 560 | 23 | in-flight derivations (+1 property) |
| `engine/tests/manifest.rs` | 465 | 15 | `build_manifest` (no module doc) |
| `engine/tests/mcp_context.rs` | 273 | 3 | `mcp:` source vs a real in-process server |
| `engine/tests/modes.rs` | 411 | 8 | `modes:` end to end |
| `engine/tests/no_sql_dependency.rs` | 15 | 1 | engine's `Cargo.toml` names no SQL driver |
| `engine/tests/observer.rs` | 232 | 4 | observation boundary mirrors the log |
| `engine/tests/pack_audit.rs` | 203 | 5 | `audit_pack` completeness |
| `engine/tests/pack_permissions_ceiling.rs` | 158 | 6 | `declares.permissions` ceiling in `check` |
| `engine/tests/pack_provenance.rs` | 122 | 4 | pack+version frozen in manifest |
| `engine/tests/pack_requires.rs` | 154 | 7 | `requires:` vs merged config |
| `engine/tests/permissions.rs` | 102 | 9 | `command_violation` (pure) |
| `engine/tests/process.rs` | 116 | 3 | `spawn_governed` process groups/timeout |
| `engine/tests/progress.rs` | 255 | 6 | `render_progress` (pure) |
| `engine/tests/promote_knowledge.rs` | 260 | 3 | knowledge-curation reference workflow |
| `engine/tests/promotion.rs` | 749 | 9 | promotion successor at the engine edge |
| `engine/tests/properties.rs` | 308 | 6 | **the replay property suite** (§5) |
| `engine/tests/receipt.rs` | 741 | 12 | receipt formatters + derivation |
| `engine/tests/replay.rs` | 481 | 12 | `derive`/`apply` (no module doc) |
| `engine/tests/resume_integrity.rs` | 298 | 5 | resume verifies stored bytes |
| `engine/tests/resume_worktree.rs` | 351 | 7 | resume verifies worktree identity |
| `engine/tests/run.rs` | 1 405 | 28 | end-to-end bootstrap runs |
| `engine/tests/run_checks.rs` | 615 | 20 | baseline/coverage/findings gates |
| `engine/tests/run_concurrency.rs` | 1 151 | 15 | parallel, join, loop concurrency, orphan resume |
| `engine/tests/run_context.rs` | 914 | 26 | context assembly, stable-first, knowledge layers |
| `engine/tests/run_gates_limits.rs` | 864 | 22 | gates, budget, loop cap |
| `engine/tests/run_questions.rs` | 380 | 6 | `kind: questions` pause/resume |
| `engine/tests/run_scope.rs` | 758 | 11 | scope expansion, 3 modes |
| `engine/tests/run_sessions.rs` | 1 218 | 25 | sessions, skills, distill, fan-out |
| `engine/tests/run_tools.rs` | 952 | 18 | per-run MCP listener via real rmcp client |
| `engine/tests/runner.rs` | 108 | 7 | runner resolution (no module doc) |
| `engine/tests/scope.rs` | 156 | 9 | scope globs (no module doc) |
| `engine/tests/scope_audit.rs` | 44 | 4 | `audited_scope` (pure) |
| `engine/tests/scope_expansion.rs` | 241 | 9 | `scope_expansion::evaluate` (pure) |
| `engine/tests/spans.rs` | 139 | 1 | every run/node span carries run_id/node_id |
| `engine/tests/stats.rs` | 674 | 19 | `compute_run_stats` golden log |
| `engine/tests/submit.rs` | 845 | 17 | interpreted artifact submitted via MCP |
| `engine/tests/task_cycle.rs` | 851 | 18 | pre/post-check, retries (no module doc) |
| `engine/tests/template.rs` | 57 | 6 | template rendering (no module doc) |
| `engine/tests/verification_effectiveness.rs` | 471 | 15 | effectiveness analysis golden logs |
| `engine/tests/view.rs` | 1 174 | 31 | `run_frame` snapshot |
| `engine/tests/workflow_compose.rs` | 1 903 | 21 | `kind: workflow` as linked runs |
| `engine/tests/worktree.rs` | 537 | 16 | worktree lock/liveness (no module doc) |
| `storage/tests/store.rs` | 721 | 26 | append, hash chain, retention (no module doc) |

</details>

**19 of 110** test files carry no `//!` module doc (`core/config.rs`, `core/events.rs`, `core/workflow.rs`, `engine/check.rs`, `engine/replay.rs`, `engine/task_cycle.rs`, `engine/worktree.rs`, `storage/store.rs`, `adapters/mock.rs`, …) while the other 91 do — an inconsistency against **Expresión / Autocontenido**.

---

## 2. INFRASTRUCTURE — what the testkit offers, and who actually uses it

`crates/testkit/src/` is 1 380 lines across 14 modules. The doc at `lib.rs:1-12` states the intent exactly right: *"One canonical copy of every piece of scaffolding… so the fixtures are hermetic by construction rather than by each test remembering to be."*

**Who links it:** `cli`, `engine`, `storage` (`crates/*/Cargo.toml` dev-deps). **`core` and `adapters` do not** — 26 test files, 363 tests, 9 043 lines with zero access to the shared harness.

### Adoption per helper (measured by import or fully-qualified path, 64 files)

| helper | file:line | files using | local re-implementations |
|---|---|---:|---|
| `init_repo` | `repo.rs:50` | **37** | — (the one counter that works) |
| `FixedClock` / `FIXED_NOW` | `clock.rs:11,7` | **26** / **0** | `run_tools.rs:53 struct FrozenClock`; `stats.rs:57`/`live_derivation.rs:25`/`verification_effectiveness.rs:84` `fn base_time()` ×3 |
| `yunta_in!` | `lib.rs:48` | **22** | `docs_sync.rs:15 fn yunta()`, `check_keys_cmd.rs:7 fn yunta()`, `integration.rs:5`, `run_flow.rs:2872` |
| `git` | `repo.rs:16` | **22** | 5 raw `Command::new("git")` sites (see §4) |
| `stdout`/`stderr` | `bin.rs:22,27` | 20 / 17 | — |
| **`Bench`** | `bench.rs:34` | **15** | **8 shadow `Bench` structs + 46 raw `execute_run(RunEnv{…})`** |
| `write` | `repo.rs:62` | 15 | `catalog.rs:13`, `pack_audit.rs:11`, `pack_permissions_ceiling.rs:11` (byte-identical ×3), `cli/tests/check.rs:11` |
| `MOCK_CONFIG` | `bench.rs:24` | 11 | `blackboard.rs:22`, `degradation.rs:21`, `receipt.rs`, `common/mod.rs:117-172` (5 more CONFIG consts) |
| `run_id_from` | `bin.rs:34` | 11 | — |
| `git_output` | `repo.rs:27` | 5 | `run_flow.rs:1872`, `run_flow.rs:1925`, `common/mod.rs:289`, `run_sessions.rs:222` |
| `INITIAL_BRANCH` | `repo.rs:9` | 4 | `factory_packs_cmd.rs:24` uses `-b master`; `docs_sync.rs:151` uses no `-b` at all |
| `ScriptedInteraction` | `interaction.rs:12` | 4 | `common/mod.rs:547 SequencedInteraction`, `common/mod.rs:64 ScriptedAnswers` |
| `accepted` | `events.rs:15` | 4 | — |
| `Captured` | `capture.rs:13` | 5 (all in `cli/src`) | — |
| `run_frame`/`node_frame`/`child_link` | `frames.rs:30,52,74` | 3 / 2 / 3 | `engine/tests/view.rs` builds its own frames inline (1 174 lines) |
| `ApproveEverything` | `interaction.rs:58` | 3 | — |
| `wait_until` / `wait_for` | `wait.rs:35,22` | 3 / 2 | — |
| `wait_until_async` | `wait.rs:57` | 2 | — |
| `task_status_changed` | `events.rs:45` | 3 | — |
| `tasks_document` | `tasks.rs:13` | 2 | — |
| `task_registered` | `events.rs:33` | 2 | — |
| `SourceLog` | `events.rs:77` | 2 | — |
| `RecordingObserver` / `Frame` | `observer.rs:39,14` | 2 / **0** | — |
| `Checkout` | `checkout.rs:21` | **3** | `factory_packs_cmd.rs:45 setup_project()`, `docs_sync.rs:103 check_project()`, + ~20 inline `init_repo`+`write`+`git commit` sequences |
| `Terminal` / `yunta_on_terminal!` | `terminal.rs:42` | **1** / 2 | — |
| `AtClock` | `clock.rs:24` | **1** (`storage/tests/store.rs:493`) | `run_tools.rs:53` |
| `runs_root` | `terminal.rs:319` | **1** | — |
| **`stored`** | `events.rs:62` | **0 test files** (2 `src` `cfg(test)` sites) | **10 local `fn event(…)` builders** |
| `run_yunta`, `WAIT_DEADLINE`, `wait_for_async`, `FIXED_NOW`, `Frame` | — | **0** | — |

### Table of duplicated local helpers (file:line)

**A. Shadow benches — the canonical recipe (`build_manifest → create_run → execute_run`) written by hand**

| shadow | file:line | duplicates |
|---|---|---|
| `struct Bench` | `engine/tests/blackboard.rs:30` (impl `:38`, `execute_run` `:76`) | `testkit/src/bench.rs:34` |
| `struct Bench` | `engine/tests/degradation.rs:31` (`:39`, `:97`) | idem |
| `struct Bench` | `engine/tests/external_gate.rs:63` (`:72`, `:130`) | idem |
| `struct Bench` | `engine/tests/modes.rs:47` (`:54`, `:130`) | idem |
| `struct Bench` | `engine/tests/receipt.rs:298` (`:306`, `:359`, `:471`) | idem |
| `struct Bench` | `engine/tests/run_tools.rs:42` (`:62`) | idem (+ its own `FrozenClock` `:53`) |
| `struct Bench` | `engine/tests/workflow_compose.rs:34` (`:44`, `:150`) | idem |
| `struct BirthBench` | `engine/tests/run.rs:751` (`:760`) | `Bench::born_holding` (`bench.rs:108`) |
| `struct GateBench` | `engine/tests/escalation.rs:197` (`:206`, `:266`, `:416`) | idem |
| `run_with_recording_mock` / `resume_orphan_with_mock` | `engine/tests/common/mod.rs:~763`, `:~869` (`execute_run` at `:799`, `:951`) | idem |

Raw `execute_run(RunEnv{…})` call sites: **46 in `engine/tests`, 1 in `testkit`** (`bench.rs:289`). Raw `create_run(` in engine tests: **40**. `common/mod.rs` opens with `#![allow(dead_code)] #![allow(unused_imports)]` (`:1-2`) — the signature of a shared module nobody owns, compiled into 10 separate test binaries.

**B. Event builders — 10 copies of a `testkit::stored` nobody calls**

| file:line | signature | timestamp | why it can't use `stored` |
|---|---|---|---|
| `core/tests/finding_ledger.rs:24` | `fn event(seq, node, payload)` | `UNIX_EPOCH` | core doesn't link testkit |
| `core/tests/artifact_ledger.rs:18` | `fn event(seq, node: Option, payload)` | `UNIX_EPOCH` | idem |
| `engine/tests/replay.rs:22` | `fn event(seq, node, payload)` | **`Utc::now()`** | `stored` has no node_id |
| `engine/tests/events_export.rs:12` | identical to `replay.rs:22` (byte-for-byte) | **`Utc::now()`** | idem |
| `engine/tests/progress.rs:60` | `fn event(seq, node: &str, payload)` | **`Utc::now()`** | idem |
| `engine/tests/stats.rs:63` | `fn event(index, offset_secs, node, payload)` | `base_time()+offset` | needs a time axis |
| `engine/tests/live_derivation.rs:35` | identical shape to `stats.rs:63` | `base_time()+offset` | idem |
| `engine/tests/verification_effectiveness.rs:90` | `fn event(index, node, payload)` | `base_time()` | idem |
| `engine/tests/properties.rs:158` | `fn event(index, node, payload)` | fixed RFC3339 literal | idem |
| `engine/src/findings.rs:55`, `engine/src/run/distill.rs:311`, `cli/src/surface/painter.rs:412`, `fold.rs:88`, `turns.rs:177` | `fn event(...)` | `UNIX_EPOCH` / `FixedClock` | idem |

**The root cause is a real API gap, not laziness:** `testkit::stored` (`events.rs:62-70`) hard-codes `node_id: None` and `timestamp: UNIX_EPOCH`. Every log that needs a node or a time axis — i.e. almost every log — must rewrite it.

**C. Byte-identical helper functions across ≥2 test files** (md5 of body)

| helper | lines | copies |
|---|---:|---|
| `fn workflow()` | 12 | `engine/tests/check.rs:156`, `progress.rs:47`, `stats.rs:44`, `verification_effectiveness.rs:71` |
| `fn request()` | 17 | `adapters/tests/claude_code.rs:33`, `codex.rs:30`, `mock.rs:12` |
| `async fn drain()` | 8 | same three |
| `fn write()` | 6 | `engine/tests/catalog.rs:13`, `pack_audit.rs:11`, `pack_permissions_ceiling.rs:11` |
| `fn base_time()` | 3 | `live_derivation.rs:25`, `stats.rs:57`, `verification_effectiveness.rs:84` |
| `fn tokens()` | 7 | `live_derivation.rs:88`, `replay.rs:32`, `view.rs:106` |
| `fn write_lines()` | 10 | `claude_code.rs:60`, `codex.rs:57` |
| `fn child_pid_fifo()` | 9 | `claude_code.rs:380`, `codex.rs:582` |
| `async fn grandchild_pid()` | 13 | `claude_code.rs:394`, `codex.rs:596` |
| **`fn debug_of_a_session_request_never_prints_secrets()`** | **27** | `claude_code.rs:523`, `codex.rs:639` — *an entire test duplicated byte-for-byte* |
| `fn yunta()` | 3 | `cli/tests/check_keys_cmd.rs:7`, `docs_sync.rs:15` |
| `fn repo_root()` | 6 | `factory_packs_cmd.rs:11`, `factory_packs_structural.rs:9` |
| `fn write_pack()` | 20 | `mcp_flow.rs:370`, `pack_resolution_cmd.rs:14` |
| `fn cmd()` | 6 | `core/tests/tasks_rules.rs:48`, `engine/tests/task_cycle.rs:10` |
| `fn event()` | 9 | `events_export.rs:12`, `replay.rs:22` |
| `fn config()` | 3 | `manifest.rs:11`, `runner.rs:4` |
| `fn resumes()` | 7 | `resume_integrity.rs:144`, `resume_worktree.rs:151` |
| `static IDS: SeqIdSource` | 1 | **12 files**: `blackboard.rs:22`, `degradation.rs:21`, `escalation.rs:21`, `external_gate.rs:24`, `factory_packs.rs:25`, `mcp_context.rs:31`, `modes.rs:24`, `promote_knowledge.rs:23`, `promotion.rs:28`, `receipt.rs:34`, `spans.rs:22`, `common/mod.rs:20` |

---

## 3. SLEEPS & TIMING

### Classification

| site | form | verdict |
|---|---|---|
| `testkit/src/wait.rs:17` `WAIT_DEADLINE = 10s` + `wait_for`/`wait_until`/`*_async` | poll + `thread::yield_now()`/`tokio::yield_now()` + deadline + caller-composed message | **OK — this is the right primitive**, and the module doc says exactly why (`wait.rs:1-9`). Caveat: it is a *hot spin* (never sleeps), so a 10 s wait pegs a core; and `wait_for_async` yields the task, which on a current-thread runtime starves a condition satisfied only by a blocking thread. |
| **`engine/tests/run_tools.rs:547`** `tokio::time::sleep(Duration::from_millis(100))` — *"Give the graceful shutdown a beat to release the socket."* | **fixed sleep** | **VICE.** The only unconditional sleep in the suite. The condition is observable (the next `serve` must fail); this is `wait_until_async` spelled as a guess. Direct violation of *"la sincronización es explícita, nunca un sleep"*. |
| `engine/tests/task_cycle.rs:676` `"sleep 0.2; test -f never.txt"` | shell sleep **inside the subject** | Defensible: the test is *learned criterion ordering by measured duration*; the 0.2 s is the signal, not a wait. Still wall-clock-dependent (`runs[0].cmd == fast` after learning). |
| `engine/tests/task_cycle.rs:560` `timeout: Some(50ms)` + `tokio::time::timeout(5s, …)` at `:568` | bounded assert | OK — timeout is the subject; the outer 5 s is a fail-loud bound. |
| `cli/tests/run_flow.rs:2104` `until [ -f ../go.txt ]; do sleep 0.05; done` | shell busy-wait in the fixture | Acceptable (it is the *node* holding open), and the test side uses `wait_until` (`:2112`). |
| `cli/tests/run_surface.rs:321,379` `"sleep 30"` | long-running node, killed | OK — a node that outlives the assertion. |
| `adapters/tests/mock.rs:113,131,538,550,565`, `engine/tests/process.rs:39`, `adapters/tests/forge_github.rs:517` | `tokio::time::timeout(50–200ms)` | OK — bounded negative assertions ("no second event arrives"). |
| `engine/tests/blackboard.rs:150` `after_ms: {0,60}` | **latency injection** via `MockStep::after_ms` → `adapters/src/mock/script.rs:69 tokio::time::sleep` | **Judged sound.** It is not a wait — it is the independent variable: `blackboard_fixture(a_first)` staggers which reviewer posts first to prove consolidation is *content-ordered, not arrival-ordered* (`:147-149`). 60 ms is a magic number with no recorded decision, and the pair of runs is a hand-rolled two-point test where a property over arrival permutations would be stronger. |

### Wall-clock dependence where a `FixedClock` would do

| site | reads |
|---|---|
| `engine/tests/replay.rs:26`, `:378` | `chrono::Utc::now()` in the event builder |
| `engine/tests/events_export.rs:16` | idem |
| `engine/tests/progress.rs:64` | idem |
| `engine/tests/artifacts.rs:73`, `blackboard.rs:377` | idem |
| **`testkit/src/events.rs:111`** | `SourceLog::record` appends with **`SystemClock`** — the support crate itself, which exists to make fixtures deterministic, stamps hand-written logs with wall time |
| `engine/tests/run_tools.rs:64` | `Bench::new()` defaults to `Arc::new(SystemClock)` while `FrozenClock` sits 11 lines above at `:53` |
| `storage/tests/store.rs` | **~30 `&yunta_core::SystemClock`** appends, though the crate's dev-dep comment says *"the fixed clock store tests time events with"*; only `:357` uses `FixedClock`, `:493`/`:574` use `AtClock` |
| `engine/tests/worktree.rs:218,248,322,369,392,425,448` | `Utc::now()` for lock-owner records; `:448` asserts `started_at > Utc::now() - 1min` |
| `common/mod.rs:918`, `promotion.rs:194,211`, `resume_*.rs`, `run.rs:1102,1368`, `run_concurrency.rs:213,310,567,1004`, `run_gates_limits.rs:330`, `run_sessions.rs:785`, `run_tools.rs:89,171,408` | `SystemClock` on hand-built prelude events |

The `utc_now_outside_clock` ratchet reads **0** because it only scans `crates/*/src` — none of the above is visible to it.

---

## 4. ENVIRONMENT

### How injection is meant to work (and does, in the core)

`crates/core/src/config/env.rs:36-55` defines `Env { home, yunta_home, org_config, subprocess_vars }` with the right doctrine: *"captured once at a shell boundary so no code below the boundary reads the process itself."* `crates/cli/src/project.rs:94-101 process_env()` is the single boundary. `Bench::with_user_state_root` (`bench.rs:97`) injects `yunta_home` **without mutating the process** — exactly right. `factory_packs.rs:55-61` injects a stub `PATH` through `Env::subprocess_vars` rather than `set_var` — also exactly right, and it says so in a comment.

`testkit::repo::git_command` (`repo.rs:37-45`) pins `GIT_CONFIG_GLOBAL=/dev/null` and `GIT_CONFIG_SYSTEM=/dev/null`; `init_repo` pins `-b main` (`repo.rs:9,51`). `Terminal::open` (`terminal.rs:81-88`) sets `YUNTA_HOME`, `TERM=xterm-256color` and `env_remove("NO_COLOR")`.

### Defects

| # | site | leak |
|---|---|---|
| E1 | **`testkit/src/bin.rs:11-19 run_yunta`** | Sets **only** `YUNTA_HOME`. `YUNTA_ORG_CONFIG` is inherited, and when unset the CLI falls back to `/etc/yunta/config.yaml` (`project.rs:107-110`), which `load_named_layers` merges as the `org` layer (`project.rs:151-166`). **Every one of the 22 files driving the binary through `yunta_in!` reads whatever `/etc/yunta/config.yaml` the host has.** Also inherits `USER` (`identity.rs:16`), `TERM`/`NO_COLOR` (`surface/mod.rs:110-111`, `glyphs.rs:53-57`) — so a developer under `NO_COLOR` measures a different surface than CI. `Terminal::open` has the same `YUNTA_ORG_CONFIG` hole. |
| E2 | **`cli/tests/check_keys_cmd.rs:19-22`** | Runs `yunta check` with **no `YUNTA_HOME` and no `current_dir`** → cwd is `crates/cli`, so project discovery reaches the **repo's own `.yunta/config.yaml`**, plus the developer's real `~/.yunta/config.yaml` and `/etc/yunta/config.yaml`. |
| E3 | **`cli/tests/docs_sync.rs:123-134 check_passes`** | Runs `yunta check` in a temp project but sets no `YUNTA_HOME` → the `user` layer is the developer's real `~/.yunta/config.yaml`. `case_runs` (`:179`) does set it. |
| E4 | **`cli/tests/docs_sync.rs:141-152 case_runs`** | `Command::new("git")` with **no `GIT_CONFIG_*` pin and no `-b`** — inherits `init.defaultBranch`, `commit.gpgsign`, hooks, commit templates. This is precisely the failure mode `xtask/src/smells.rs:256-261` says the ratchet was created to catch. |
| E5 | `engine/tests/mcp_context.rs:261` | `std::env::remove_var(...)` — a **process-global** mutation inside a test binary whose tests run concurrently. Harmless today (the var is a never-set sentinel) but it is the one place the workspace's own rule ("a test injects a value instead of mutating the process") is broken. `adapters/tests/claude_code.rs:7-10` states that rule verbatim. |
| E6 | `engine/tests/factory_packs.rs:54` | Reads the real `PATH` (`std::env::var("PATH")`) to build the injected one. Pragmatic, but the run is no longer hermetic w.r.t. what is on the host PATH. |
| E7 | `cli/tests/run_flow.rs:2872-2873` | `.env("HOME", &home).env_remove("YUNTA_HOME")` — correct and deliberate (it tests the `~/.yunta` fallback), but it bypasses `yunta_in!` because the macro offers no way to express it. A missing testkit capability, not a test bug. |
| E8 | `static IDS: SeqIdSource` ×12 | Shared mutable `AtomicU64` across tests running concurrently in one binary. Ids stay unique, but their **values are not reproducible per test** — contradicting `core/src/id_source.rs:4` ("tests inject a sequential source and get reproducible ids") and `:27` ("Reproducible ids for tests"). `workflow_compose.rs:67` gets this right with a per-bench `SeqIdSource::new("minted")`. |

No test reads or writes the real `~/.yunta` **runs root** (every one sets `YUNTA_HOME` to a temp dir, and `Checkout.home`/`Bench` own their temp trees). The leak is confined to the **config layers** and ambient shell vars.

---

## 5. PROPERTY TESTS

**13 property tests total**, in 6 `proptest!` blocks.

### `crates/engine/tests/properties.rs` (308 lines, 6 properties)

| property | line | what it proves |
|---|---|---|
| `derive_is_deterministic` | `:181` | `derive(&log) == derive(&log)` — purity, no `HashMap` order leakage |
| `the_artifact_fold_is_deterministic_and_holds_every_identity` | `:189` | `ArtifactLedger::of` pure; `every()` returns exactly the `(producer, identity)` set the log accepted |
| `events_round_trip_through_json` | `:212` | wire-form round trip (the `events.jsonl` contract) |
| `derive_is_prefix_monotonic` | `:224` | tokens accumulate, findings only pile up, tasks/nodes once seen stay seen |
| `an_interrupted_run_resumes_to_the_same_final_state` | `:246` | **see defect below** |
| `verifying_an_intact_store_finds_nothing_and_changes_nothing` | `:265` | `ArtifactIntegrity::of` finds no faults over a store holding exactly what the log names; `verified + unverifiable == ledger.every().count()` |

**Generators** (`:62-175`): `payload()` emits 8 kinds — `NodeStarted`, `NodeFinished`, `NodeFailed`, `TaskRegistered`, `TaskStatusChanged`, `FindingPosted`, `ArtifactAccepted`, `RunPaused`. Node ids from `{None, a, b, c}`, task ids from `{t1,t2,t3}`, artifact ids from `{Tasks, Findings, notes.md}` — small pools deliberately, so logs revisit identities (documented at `:55-60`, `:117-119`). Logs are 0–40 entries, `seq` assigned in order, timestamp fixed (`:162`, with the reason stated). The module doc (`:7-10`) is honest that lifecycle validity is *not* enforced — a good decision, since `derive` must interpret any log.

### What is missing per `CLAUDE.md` ("property tests para replay, idempotencia y resume")

1. **The resume property is a tautology.** `properties.rs:252-255`:
   ```rust
   let uninterrupted = derive(&log);
   let _crashed_at_k = derive(&log[..k]);     // computed, bound to `_`, never used
   let resumed = derive(&log);
   prop_assert_eq!(resumed, uninterrupted);
   ```
   This asserts `derive(&log) == derive(&log)` — identical to `derive_is_deterministic`, with `k` generated and discarded. It proves nothing about resume. A real property would either (a) derive the prefix, feed that state into the resume path, and compare, or (b) drive `execute_run` to a cut point and re-enter it — which is what `resume_integrity.rs` / `resume_worktree.rs` / `run_concurrency.rs:844` do, but only as hand-picked examples.
2. **No idempotence property.** Nothing generates *re-delivery*: appending the same logical event twice, re-running a node after a crash, re-submitting an artifact, replaying the same `task_status_changed`. `run_concurrency.rs:844 killing_the_engine_mid_batch_and_resuming_only_reruns_the_orphan` is the single example, at 205 lines.
3. **No hash-chain property.** `storage/src/store.rs:308 verify_chain` (92 lines) has only example tests in `store.rs`; there is no property that an arbitrary append sequence produces a chain `verify_chain` accepts, nor that any single-byte mutation is caught.
4. **`ReplayError` is unexercised by properties.** `engine/src/replay.rs:170 apply` is 201 lines returning `Result<(), ReplayError>`; the generator never produces a log that should be marked `broken`, so the "derive marks a log it cannot follow `broken`" claim at `properties.rs:9` is asserted nowhere.
5. **The generator omits half the payload kinds** — no `RunCreated`, `ChildRunCreated`, `GateWaiting`/`GateResolved`, `AgentSessionOpened`, `ContextAssembled`, `Usage`, `CapabilityDegraded`, `ArtifactWritten` (used only in the integrity property), `FindingUpdated`/`FindingWithdrawn`, `RunFinished`. `core/tests/events.rs:8 fn all_kinds()` (268 lines) enumerates them all — that list and this generator should be the same list.
6. **`properties.rs` does not use the testkit at all** — its own `fn event()` at `:158`.

### The other 7 properties (these are good and must be kept)

- `core/tests/artifact_ledger.rs:228` — fold deterministic, loses no identity.
- `core/tests/finding_ledger.rs:196` — effective set is last-state-per-id in first-post order, modelled against an oracle (`:198-230`). The strongest property in the repo.
- `core/tests/shape_roundtrip.rs:119` `round_trips!` macro ×3 (`tasks`, `findings`, `questions`) — render→read, accept→same value, and *rendering is a function of the value alone* (same meaning ⇒ same bytes ⇒ same hash).
- `engine/tests/live_derivation.rs:531` — usage counts once across a node's terminal.
- `engine/src/tasks/mod.rs:291` — a registration plan never marks `done` a task whose identity changed.

---

## 6. YUNTA'S OWN TESTS (`yunta test`)

**7 cases, 6 fixtures, 2 seed worktrees.**

| case | asserts |
|---|---|
| `.yunta/tests/lint-fix-pauses-after-two-fix-rounds.yaml` (11 l) | `final_state: paused`; `lint: failed`, `fix-lint: finished`, `fmt: never ran` — the re-route budget is spent, the workflow does not loop |
| `.yunta/tests/run-tasks-completes-a-one-task-document.yaml` (16 l) | `inputs: {tasks: plan/tasks.yaml}`, `worktree: worktrees/greeting-crate`; `finished`; `implement/verify: finished`; `tasks: {greeting: done}` — a run **born holding** its document, criteria red→green on a real crate |
| `packs/starter/.yunta/tests/fix.yaml` (7 l) | `finished`; `fix`, `verify` finished |
| `packs/starter/.yunta/tests/review.yaml` (4 l) | `finished` only |
| `packs/fragua/.yunta/tests/quick-pauses-at-ship.yaml` (22 l) | `quick` mode: 8 node states incl. three `never ran`, `tasks: {T001: done}` |
| `packs/fragua/.yunta/tests/standard-pauses-at-approve-plan.yaml` (17 l) | `standard`: pauses at first human decision, `T001: pending` |
| `packs/fragua/.yunta/tests/full-pauses-at-approve-plan.yaml` (15 l) | `full`: same pause point, every node in play |

**How fixtures are written:** plain `sessions:` lists of scripted mock sessions. `.yunta/tests/fixtures/run-tasks.yaml` uses `match_prompt_contains` + `effects:` with inline file contents; `packs/starter/.yunta/tests/fixtures/review.yaml` declares `capabilities: {run_tools: true}` and two outcome-only sessions in declaration order (with a comment explaining why order, not matching, selects them). The case schema is `deny_unknown_fields` (`cli/src/commands/test.rs:43,66`) and the runner renders `{{run.dir}}`/`{{worktree}}` into the fixture before parsing (`:13-16`).

**Overlap with the CLI tests:** substantial and *mostly healthy*.
- `cli/tests/factory_packs_cmd.rs:142` runs `yunta test --dir packs/starter` and asserts `"2 cases, 0 failed"`; `:123` asserts fragua's `"tests: 3 cases, 0 failed"` through `pack add --run-tests`. **So both packs' cases already run under `cargo test`.**
- `cli/tests/run_flow.rs:199,284,330` and `pack_cmd.rs:524` exercise `yunta test` on synthetic projects.
- `cli/tests/docs_sync.rs:154-188 case_runs` executes every documented case block.
- **Gap:** the repo's own two `.yunta/tests/` cases run **only** in the CI step `$yunta test` (`ci.yml:52`). `cargo test --workspace` never runs them — which is why CONTRIBUTING lists `cargo run -p yunta -- test` as a separate command.

**Determinism note:** `yunta test` drives runs with `SystemClock` and `SystemIdSource` (`cli/src/commands/test.rs:272`, `:285-286`) while its own module doc says *"No LLM, no network, deterministic"* (`:4-5`). Assertions are over states only, so nothing flakes — but the run ids and timestamps of the repo's own self-tests are not reproducible.

---

## 7. SMELLS RATCHET

`cargo run -p xtask -- smells` reproduces `xtask/smells.baseline` exactly (file unchanged, `git status` clean). Measurement is line-based over `crates/*/src` (production text with `#[cfg(test)]` modules blanked by `production_only`/`test_mod_mask`, `smells.rs:93-169`) and `crates/*/tests`.

| counter | value | what it measures (`xtask/src/smells.rs`) | top offenders |
|---|---:|---|---|
| `copied_test_helpers` | **0** | `:263-272` — lines in `tests/` **and** `src/` whose trimmed text starts with exactly `fn git(`, `fn yunta_in(`, `fn init_repo(`, or `struct FixedClock` | **The number is true and the claim is false** — see §9 D1. `testkit/src/repo.rs` escapes only because its helpers are `pub fn`. |
| `exit_failure_in_cli` | **2** | `:252-255` — `ExitCode::FAILURE` lines in `crates/cli/src` | `cli/src/main.rs:58`, `cli/src/main.rs:61` — both in `main`, as intended |
| `git_command_new_files` | **2** | `:240-250` — *files* containing `Command::new("git")` in `crates/*/src` | `engine/src/git.rs` (5 sites), `testkit/src/repo.rs` (1). Blind to the **5 further sites in `tests/`**: `docs_sync.rs:143`, `run_flow.rs:1872`, `run_flow.rs:1925`, `common/mod.rs:289`, `run_sessions.rs:222` |
| `let_underscore_in_prod` | **24** | `:200-206` — lines starting `let _ =` outside test modules | `engine/src/process.rs` **4** (`:238,269,272,306`), `adapters/src/mock/script.rs` **3** (`:117,203,208`), `cli/src/pack.rs` **3** (`:278,305,358`), `engine/src/worktree/mod.rs` **2** (`:293,352`), `engine/src/task_cycle/session.rs` **2** (`:244,246`), then 10 singletons incl. `engine/src/run/parallel_exec.rs:176 let _ = result?;` and `engine/src/human_interaction.rs:62 let _ = (questions, interactive);` |
| `prod_fns_over_50_lines` | **81** | `:208-211` — brace-matched body length > 50, literals/comments blanked (`functions_over`, `:330-389`) | **worst single fn:** `engine/src/run/schedule.rs:208 next_step` **370 lines**; then `run/workflow_exec/mod.rs:95 execute_workflow` **314**, `replay.rs:170 apply` **201**, `run/parallel_exec.rs:20 execute_parallel` **185**, `run/workflow_exec/mod.rs:501 drive_child` **184**, `run/loop_exec/integrate.rs:27 integrate_batch` **155**, `check/mod.rs:69 check` **153**, `run/prompt_exec.rs:122 execute_prompt` **151**, `run/loop_exec/mod.rs:31 execute_loop` **149**, `run/questions_exec.rs:26 execute_ask` **141**. **Worst files:** `cli/src/commands/pack.rs`, `engine/src/run/exec.rs`, `engine/src/run/gate_exec.rs`, `engine/src/run/steps.rs`, `engine/src/run/workflow_exec/mod.rs`, `storage/src/store.rs` — 3 each. **`testkit/src/bench.rs:248 run_full` (54 lines) is itself one of the 81.** |
| `space_runs_in_prod_strings` | **0** | `:276-282` / `has_inner_space_run` (`:295-322`) — ≥8 spaces between visible chars inside a literal: the trace of a lost `\` continuation | clean |
| `src_files_over_500_lines` | **11** | `:214-224` — files (counted whole, tests included) > 500 lines in `crates/*/src` | `core/src/events/payloads.rs` **989**, `core/src/ids.rs` **860**, `cli/src/commands/stats.rs` **754**, `engine/src/run/workflow_exec/mod.rs` **691**, `storage/src/store.rs` **610**, `engine/src/run/gate_exec.rs` **599**, `engine/src/run/schedule.rs` **586**, `engine/src/stats.rs` **566**, `adapters/src/forge/github.rs` **549**, `cli/src/commands/pack.rs` **548**, `engine/src/worktree/mod.rs` **506** |
| `utc_now_outside_clock` | **0** | `:226-236` — `Utc::now` lines in `crates/*/src` except `clock.rs` | true for production; blind to the 8 `Utc::now()` sites in `tests/` (§3) |

**Schema check** (`xtask/src/main.rs:64-97`): renders every `yunta_core::schema::all()` entry to `crates/core/schemas/<name>.json` and, under `--check`, diffs against the committed file. Rationale for the location is documented at `:44-50` (inside the package so `include_str!` embeds exactly what CI checked).

**What the ratchet cannot see, by construction:**
- Any duplicated helper not named one of its 4 literals — the 17 byte-identical helper groups of §2C, the 8 shadow `Bench` structs, the 10 `event()` builders, `struct FrozenClock` (`run_tools.rs:53`).
- Anything in `crates/*/tests/`: the file-size and function-size budgets (**34 test files > 500 lines**, up to `run_flow.rs` at **2 890**; **145 test fns > 50 body lines**, up to `core/tests/events.rs:8 all_kinds()` at **268**), `Utc::now`, `Command::new("git")`.
- `let _ =` in tests.

The `xtask` crate itself is excluded from every counter (`crate_src_dirs()` only walks `crates/`), so `smells.rs`'s own `measure()` and `strip_noise` are unmeasured.

---

## 8. CI

### `.github/workflows/ci.yml` — 2 jobs, on every push and PR

**Job `ci` (ubuntu-latest), 9 steps:**

| # | step | command |
|---|---|---|
| 1 | checkout + toolchain | `actions-rust-lang/setup-rust-toolchain@v1`, `rustflags: ""` |
| 2 | fmt | `cargo fmt --all -- --check` |
| 3 | clippy | `cargo clippy --workspace --all-targets -- -D warnings` |
| 4 | deny | `EmbarkStudios/cargo-deny-action@v2`, `command: check` (advisories + licenses + bans + sources) |
| 5 | build | `cargo build --workspace --locked` |
| 6 | schemas | `cargo xtask schema --check` |
| 7 | smells | `cargo xtask smells --check` |
| 8 | rustdoc | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` |
| 9 | test | `cargo test --workspace --locked` |
| 10 | self-tests | `yunta check` over `.yunta/workflows/*.yaml` + `packs/*/.yunta/workflows/*.yaml`; then `yunta test`, `yunta test --dir packs/starter`, `yunta test --dir packs/fragua` |
| 11 | isolation | `cargo check -p <crate>` for the 6 packages |

**Job `musl-build` (ubuntu-latest), 4 steps:** musl target + `musl-tools`; `cargo build --release --locked --target x86_64-unknown-linux-musl -p yunta`; static-link assertion via `! ldd … | grep "=>"`; binary-size ceiling 33 554 432 B (32 MiB against 18.75 MiB measured), reported to `$GITHUB_STEP_SUMMARY` every run with the rationale inline (`ci.yml:90-96`, citing D155).

### `.github/workflows/release.yml` — on `v*.*.*` tags

`test` gate → `build` (4-target matrix: x86_64/aarch64 musl via `cross`, x86_64/aarch64 darwin native) → `release` (checksums + git-cliff notes + gh-release) → `publish-crates` (ordered, with `scripts/wait-for-crate-index.sh`) and `publish-container` (ghcr, amd64+arm64). Clean-container install verification per platform (`release.yml:220-231`). Homebrew tap deliberately not wired, with the reason stated (`:308-311`).

### Gaps

| # | gap |
|---|---|
| C1 | **The release gate is weaker than `ci`.** `release.yml`'s `test` job runs fmt, clippy, deny, `cargo test`, and the self-tests — but **not** `cargo xtask schema --check`, **not** `cargo xtask smells --check`, **not** `cargo doc -D warnings`, **not** the per-crate isolated `cargo check`. A tag can ship with stale committed schemas or a risen smell count. |
| C2 | **`cargo test` runs only on ubuntu-latest, x86_64, glibc.** macOS is built and `--version`-smoked but never tested, though the suite's most platform-sensitive parts are POSIX-specific: `testkit/src/terminal.rs` (openpty/termios/`cfmakeraw`), `adapters/tests/signal.rs` (process-group signals), `engine/tests/process.rs` (`spawn_governed`), `adapters/tests/process_start.rs` (host process table). The musl job builds but never runs a test. |
| C3 | **Pack self-tests are a hardcoded list.** `ci.yml:49` globs `packs/*/.yunta/workflows/*.yaml` for `check`, but `:53-54` names `packs/starter` and `packs/fragua` literally for `test`. A third pack gets checked and never tested. (Mitigated: `cli/tests/factory_packs_cmd.rs` already runs both packs' cases under `cargo test`, so steps `:53-54` are largely redundant with the suite — `:52` `$yunta test` is not.) |
| C4 | The repo's own `.yunta/tests` cases exist only behind `ci.yml:52`; `cargo test --workspace` does not reach them. A developer's local green is not CI's green. CONTRIBUTING is honest about this (`cargo run -p yunta -- test` is listed separately). |
| C5 | No `concurrency:` group, no per-job `timeout-minutes`. Given `WAIT_DEADLINE = 10s` and ~50 tests that drive real processes/pty/sockets, a hang burns the full runner budget. |
| C6 | No coverage measurement, no `--no-default-features` / `--all-features` matrix, no `cargo test --release` (the release profile is `panic = "abort"` + `lto = "fat"` — never exercised by any test). |
| C7 | **Runs locally, not in CI:** nothing — CONTRIBUTING's list is a subset of `ci`. **Runs in CI, not in CONTRIBUTING:** `cargo xtask smells --check`, `cargo doc -D warnings`, the per-crate `cargo check`, the musl static/size job. CONTRIBUTING (`## Building and testing`) mentions `cargo deny`, `schema --check`, per-crate check and pack self-tests but **omits `smells --check`** — a contributor following the guide will be surprised by the ratchet. |

---

## 9. DEFECTS

Category key: **[dup]** helper duplicated outside testkit · **[sleep]** · **[env]** env leak · **[prop]** missing property · **[path]** uncovered path · **[name]** named by mechanism · **[doc]** documentation/claim defect.

| # | cat | defect | evidence |
|---|---|---|---|
| D1 | **[dup][doc]** | `copied_test_helpers 0` is a true count of a false claim. The comment at `smells.rs:256-261` says *"Test infrastructure lives in the support crate, never copied into a test that needs it"*; the measure is 4 literal prefixes. Real copies: 8 shadow `Bench` structs, 10 `event()` builders, 17 byte-identical helper groups, 12 `static IDS`, 1 `FrozenClock`. Per **Evidencia** ("un resultado que no ejecutaste no existe") the counter reports a result nobody measured. | §2, `xtask/src/smells.rs:263-272` |
| D2 | **[dup]** | 46 hand-written `execute_run(RunEnv{…})` + 40 `create_run(…)` in `engine/tests` against 1 in `testkit/src/bench.rs:289`. The bench doc says *"every test observes the same production recipe"* — 15 of 56 engine test binaries do. | §2A |
| D3 | **[dup]** | `testkit::stored` (`events.rs:62`) has **zero** test-file users because it cannot express `node_id` or a timestamp. Ten builders exist because the canonical one is under-specified. The "Un lugar" fix is to widen `stored`, not to delete the copies. | §2B |
| D4 | **[dup]** | `crates/core` and `crates/adapters` do not dev-depend on `yunta-testkit` (`crates/*/Cargo.toml`). 363 tests, 9 043 lines, and consequently `request`/`drain`/`write_lines`/`child_pid_fifo`/`grandchild_pid` and a whole 27-line test duplicated byte-for-byte between `claude_code.rs` and `codex.rs`. | §2C |
| D5 | **[dup]** | `engine/tests/common/mod.rs` (982 lines, `#![allow(dead_code)]`, `#![allow(unused_imports)]`) is a second, unowned support crate compiled into 10 binaries, holding a second `execute_run` recipe (`:799`, `:951`), a second interaction double (`SequencedInteraction :547`, `ScriptedAnswers :64`), 6 CONFIG consts, and a `Command::new("git")` (`:289`). | §2A, `common/mod.rs:1-2` |
| D6 | **[sleep]** | `engine/tests/run_tools.rs:547` `tokio::time::sleep(100ms)` — the only unconditional sleep. `wait_until_async` exists and is exported. | §3 |
| D7 | **[env]** | `testkit::run_yunta` (`bin.rs:11-19`) does not neutralize `YUNTA_ORG_CONFIG`, `NO_COLOR`, `TERM`, `USER`. Every `yunta_in!` test merges the host's `/etc/yunta/config.yaml` as its `org` layer. | E1 |
| D8 | **[env]** | `cli/tests/check_keys_cmd.rs:19` runs `yunta check` with no `YUNTA_HOME` and cwd = `crates/cli` → reads the repo's own config plus the developer's `~/.yunta/config.yaml`. | E2 |
| D9 | **[env]** | `cli/tests/docs_sync.rs:123 check_passes` — no `YUNTA_HOME`; `:141 case_runs` — `git init` with no `GIT_CONFIG_GLOBAL`/`SYSTEM` pin and no `-b`, the exact hermeticity hole `smells.rs:256-261` was written about. | E3, E4 |
| D10 | **[env]** | `engine/tests/mcp_context.rs:261 std::env::remove_var` — process-global mutation in a concurrently-running binary, contradicting `adapters/tests/claude_code.rs:7-10`'s own stated rule. | E5 |
| D11 | **[env]** | `static IDS: SeqIdSource` shared across parallel tests in 12 binaries makes minted ids non-reproducible per test, contradicting `core/src/id_source.rs:4,27`. | E8 |
| D12 | **[prop]** | `an_interrupted_run_resumes_to_the_same_final_state` (`properties.rs:246-256`) binds the crash state to `_crashed_at_k` and never uses it — the assertion reduces to `derive(&log) == derive(&log)`. **The repo has no resume property.** | §5 |
| D13 | **[prop]** | No idempotence property anywhere (re-delivered event, re-run node, re-submitted artifact). | §5 |
| D14 | **[prop]** | No property over the event-log hash chain (`storage/src/store.rs:308 verify_chain`, 92 lines) — only examples. | §5 |
| D15 | **[prop]** | The generator emits 8 of ~20 payload kinds; `EventBody::Unknown` and the `ReplayError`/`broken` path are never generated, so `properties.rs:9`'s own claim is untested. | §5 |
| D16 | **[path]** | The release profile (`panic = "abort"`, `lto = "fat"`, `strip`) is never tested — no `cargo test --release` in either workflow. | §8 C6 |
| D17 | **[path]** | No test runs on macOS, yet `testkit/src/terminal.rs`, `adapters/tests/signal.rs`, `engine/tests/process.rs` and `process_start.rs` are all platform-behaviour tests. | §8 C2 |
| D18 | **[path]** | The release gate skips `schema --check`, `smells --check`, rustdoc and per-crate isolation. | §8 C1 |
| D19 | **[name]** | Test naming is **excellent** — of 1 466 names only 6 are ≤3 words (`derive_is_deterministic`, `reviews_are_paginated`, `probe_reports_healthy`, `nan_is_rejected`, `labels_are_escaped`, `add_refuses_symlinks`), and every one of those still names a behaviour. **No defect found in this category.** |
| D20 | **[doc]** | 19 of 110 test files have no `//!` doc while 91 do. | §1 |
| D21 | **[doc]** | `crates/storage/Cargo.toml` dev-dep comment says testkit is there for *"the fixed clock store tests time events with"*; `store.rs` uses `SystemClock` ~30 times and `FixedClock` once (`:357`). | §3 |
| D22 | **[doc]** | `cli/Cargo.toml` keeps an `nix` dev-dependency with the comment *"drives `yunta run` with a real terminal on stdin (openpty)"* — no `cli/tests` file uses `nix` any more (it moved into `testkit::Terminal`); the only `nix` use left in `cli` is `src/ask/mod.rs:24`, a **normal** dependency concern. Stale declaration. | `grep nix crates/cli/tests` → ∅ |
| D23 | **[doc]** | `cli/src/commands/test.rs:4-5` claims `yunta test` is *"deterministic"* while driving runs with `SystemClock` + `SystemIdSource` (`:272,285-286`). | §6 |
| D24 | **[dup]** | CLAUDE.md's own 500-line/50-line signals are enforced only over `src`. 34 test files exceed 500 lines (`run_flow.rs` 2 890, `check.rs` 1 969, `workflow_compose.rs` 1 903); 145 test fns exceed 50 body lines (`core/tests/events.rs:8 all_kinds()` 268 lines). | §7 |
| D25 | **[sleep]** | `blackboard.rs:148` picks `60` ms with no recorded decision — a threshold that, per **Levantar**, should have been raised rather than chosen alone. | §3 |

---

## 10. IDEAL

### What is already right and must be kept verbatim

1. **`testkit/src/wait.rs`** — the whole file. A single `WAIT_DEADLINE`, a condition poll, a caller-composed failure message ("the bash node never wrote its pid"), and a doc that says *why* a test cannot join another process. This is the model the rest of the harness should be measured against. (Only change: yield with a short `park_timeout`/`tokio::time::sleep(1ms)` instead of a hot spin, and make the deadline scalable by env for loaded CI.)
2. **`testkit/src/terminal.rs`** — the pty harness. `raw()`, `line_discipline_is_back()`, `cursor_is_back()`, `cleared_before()`, `drew_and_cleared()`, `rows_redrawn()` are *behaviour* predicates on the wire, not escape-sequence trivia; the doc at `:1-11` and `:139-144` explains exactly which behaviours exist nowhere else and which one (`setsid`) the workspace's `unsafe_code = "forbid"` puts out of reach. Best piece of test infrastructure in the repo.
3. **`testkit/src/repo.rs`** — `GIT_CONFIG_GLOBAL=/dev/null`, `GIT_CONFIG_SYSTEM=/dev/null`, `INITIAL_BRANCH` as a named constant with the reason. Hermetic by construction.
4. **`testkit::Bench` and `testkit::Checkout` as a pair** — in-process engine vs. compiled binary, each owning its temp world, each with a doc naming its sibling (`checkout.rs:12-19`). The two-layer split is the correct architecture; only adoption is missing.
5. **`RecordingObserver`** (`observer.rs:26-37`) — complete the moment `execute_run` returns, *"with nothing to wait for and no synchronization of its own to get right"*. Sync-free assertion by design.
6. **`frames.rs`** — the "fill in the fields the test is about, take the rest from here" pattern, with the failure mode it prevents spelled out (`:5-8`).
7. **`Env::subprocess_vars`** (`core/src/config/env.rs:48-53`) + `factory_packs.rs:55-61` — PATH injection without touching the process.
8. **`yunta test` as a second test surface** — cases in the product's own vocabulary (`workflow`/`mode`/`inputs`/`worktree`/`fixture`/`expect`), `deny_unknown_fields`, per-case sandbox. It is the only test layer a pack author can write, and `docs_sync.rs:154` makes documented cases executable.
9. **Test naming.** 1 466 names, 6 short ones, zero named by mechanism.
10. **The ratchet's shape** — measure, commit, fail on a rise, "a number can only ever go down" (`smells.rs:8-11`), plus the deliberate refusal to count `unwrap`/`panic` because clippy already denies them (`:193-197`). The instrument is right; its *dictionary* is too small.

### The greenfield architecture

**One harness per layer, and no test may reach past its layer.**

| layer | subject | harness | rule |
|---|---|---|---|
| **pure** | folds, `derive`, `check`, `render_*`, `evaluate` | `testkit::Log` (see below) | no temp dir, no process, no clock |
| **engine** | `execute_run` end to end | **`testkit::Bench`, the only one** | `execute_run` appears exactly once in the workspace's test code |
| **binary** | the compiled `yunta` | **`testkit::Checkout` + `yunta_in!`** | `Command::new(CARGO_BIN_EXE_yunta)` appears exactly once |
| **terminal** | what a person sees and types | **`testkit::Terminal` + `yunta_on_terminal!`** | already true |
| **product** | workflows and packs | `.yunta/tests/*.yaml` | authored in the product's vocabulary |

Concretely:

1. **One event builder.** Replace `stored(run, seq, payload)` with a builder that covers the three axes the 10 copies needed:
   ```rust
   testkit::Log::for_run("run-1")          // run id, default FixedClock instant
       .at(FIXED_NOW)                       // or .base_time() + .after(secs)
       .event(payload)                      // node-less
       .node("build", payload)              // node-bearing
       .build() -> Vec<StoredEvent>
   ```
   Deletes 10 local `fn event()`, 3 `fn base_time()`, 3 `fn tokens()`, 4 `fn workflow()`, and the `Utc::now()` in `replay.rs`, `events_export.rs`, `progress.rs`, `artifacts.rs`, `blackboard.rs` in one move. Ship it with `SourceLog::record` switched from `SystemClock` to an injected clock (`events.rs:111`).

2. **One `Bench`, parameterized where the 8 shadows differ.** The shadows exist for four reasons, each of which is a missing `Bench` capability, not a missing `Bench`:
   - a sabotage window between `create_run` and `execute_run` → `Bench::run_sabotaged(wf, fx, |run_dir| …)` (`degradation.rs:63`)
   - repeated `execute_run` on one created run → `Bench::wake()` / `Bench::wake_with(forge)` (`external_gate.rs:126`, `escalation.rs`, `modes.rs`)
   - a chosen clock / id source → `Bench::with_clock`, `Bench::with_ids` (`run_tools.rs:62`, `workflow_compose.rs:67`)
   - log queries → `Bench::findings_by(node)`, `Bench::group_output(group)`, `Bench::commit_subjects()` (today in `blackboard.rs:110`, `:120`, `common/mod.rs:289`)
   With those five methods, all 8 shadow benches and both `common/mod.rs` runners collapse, and `common/mod.rs` shrinks to what it should be: **workflow and fixture literals**, no execution machinery.

3. **`core` and `adapters` link the testkit.** Move `request()`, `drain()`, `write_lines()`, `child_pid_fifo()`, `grandchild_pid()`, `running()`, `stops_running()` into `testkit::adapter` (behind a feature if the dependency direction bites), and `debug_of_a_session_request_never_prints_secrets` into a single parameterized test over both adapters.

4. **Close the env boundary in one place.** `run_yunta` and `Terminal::open` share one `hermetic(cmd, dir, home)` that sets `YUNTA_HOME`, `YUNTA_ORG_CONFIG` (to a temp empty file, not unset — the default is `/etc/yunta/config.yaml`), `HOME`, `USER`, `TERM`, and removes `NO_COLOR`; plus a `Checkout::with_org_config()` and a `Checkout::without_yunta_home()` for `run_flow.rs:2872`'s legitimate case. Then `check_keys_cmd.rs` and `docs_sync.rs` have no reason to build their own `Command`.

5. **Property coverage that matches the claim.** Four properties, all in `properties.rs`, generating over the **full** `EventPayload` set (share the list with `core/tests/events.rs:8 all_kinds()`):
   - *replay*: `derive` deterministic, prefix-monotonic, and `EventBody::Unknown` carried through — **keep as is**.
   - *resume*: generate a log and a cut `k`, drive the real resume entry point from the prefix, and assert the final derived state equals the uninterrupted one. Replaces the tautology at `:246`.
   - *idempotence*: re-deliver an arbitrary subsequence of events and assert `derive` is unchanged; re-run a node after a crash and assert one `node_finished` per attempt.
   - *chain*: any append sequence verifies; any single-byte mutation is caught by `verify_chain`.

6. **Extend the ratchet's dictionary** rather than trusting its zero. Add, over `tests/` as well as `src/`:
   `execute_run(RunEnv` outside `testkit` · `Command::new("git")` outside `git.rs`/`repo.rs` · `Utc::now` in tests · `struct .*Bench` / `struct .*Clock` outside `testkit` · `fn event(` outside `testkit` · `std::env::(set|remove)_var` · `tokio::time::sleep`/`thread::sleep` outside the mock's `after_ms` · test files > 500 lines · test fns > 50 lines. Commit the measured values as the new baseline in the same change, per the ratchet's own rule.

7. **CI:** make `release.yml`'s gate literally `needs:` the `ci` workflow (or reuse it via `workflow_call`) instead of re-listing a weaker subset; add a macOS `cargo test --workspace` job; glob the pack self-tests (`for p in packs/*/; do yunta test --dir "$p"; done`); add `concurrency` + `timeout-minutes`; add `cargo xtask smells --check` to CONTRIBUTING's list.

8. **Raise, don't decide (CLAUDE.md paso 2 / Levantar).** Three thresholds in the test architecture have no recorded decision and no ADR covers the test architecture at all (`docs/design/adrs.md`, `deuda-consciente.md` — neither mentions `testkit`, the ratchet, or the test layers): `WAIT_DEADLINE = 10s`, `blackboard.rs:148`'s `60` ms stagger, and whether `crates/core`/`crates/adapters` may link the testkit (a dependency-direction question, since `testkit` depends on `core`). Per **La documentación gana / su silencio es un paso 2**, these belong in a proposal before the refactor, not inside it.
