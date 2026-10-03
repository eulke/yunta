# Yunta engine audit — run execution & state derivation

Read-only audit. Nothing modified. All line numbers verified against the tree at `/home/user/yunta`.

---

## 1. MAP — the run lifecycle as it actually flows

### The hops

| # | Hop | Function | File:line |
|---|---|---|---|
| 1 | Freeze anatomy + birth | `create_run` | `crates/engine/src/run/create.rs:115` |
| 1a | ↳ read tasks docs / crossings before the dir exists | `birth_registrations` | `create.rs:263` |
| 1b | ↳ accept birth artifacts + register tasks | `register_birth_documents` | `create.rs:310` |
| 2 | Entry point (`run` and `resume` alike) | `execute_run` → `execute_run_at_depth` | `run/mod.rs:314` → `run/exec.rs:99` |
| 3 | Wake once per invocation | `start` | `run/exec.rs:198` |
| 3a | ↳ build ctx + registry + MCP host | `build_ctx` | `run/exec.rs:365` |
| 3b | ↳ verify artifacts + worktree before waking | `verify_before_waking` | `run/exec.rs:308` |
| 3c | ↳ `run_resumed` with orphan policies | `record_resume` → `schedule::resume_policies` | `run/exec.rs:332` → `schedule.rs:143` |
| 3d | ↳ SHA-drift recheck of approved gates | `gate_exec::recheck_approved_gates` | `gate_exec.rs:307` |
| 3e | ↳ freeze mode | `events::run_mode` + `modes::mode_included_nodes` | `exec.rs:254-255` |
| 4 | **Decide** (pure) | `schedule::next_step` | `run/schedule.rs:208` |
| 5 | Dispatch loop | `execute_run_at_depth` match | `run/exec.rs:113-189` |
| 6 | Step handlers | `steps::{broken,finish,run_failed,reroute,gate_exhausted,execute_batch,publish_gate,poll_gate,resolve_internal_gate,ask_questions}` | `run/steps.rs:25,38,114,136,164,309,373,397,409,440` |
| 7 | One node | `node_exec::execute_node` | `run/node_exec.rs:67` |
| 8 | Per-kind | `bash_exec:22`, `prompt_exec:122`, `loop_exec/mod:31`, `parallel_exec:20`, `check_exec:…`, `executor_exec:62`, `workflow_exec/mod:95`; `Gate` arm returns `Broken` | `node_exec.rs:112-203` |
| 9 | Close | `node_close::close_node` (hooks-after → scope → artifacts → `node_finished` → `progress.md`) | `run/node_close.rs:86` |
| 9b | Fail (single writer) | `node_close::fail_with` | `node_close.rs:279` |
| 10 | Pause (single writer of `run_paused`) | `exec::run_paused` / `record_pause` / `pause` | `run/exec.rs:36,76,65` |
| 11 | Post-crash pause | `record_pause_after_crash` | `run/exec.rs:42` |
| 12 | Close run | `steps::finish` (distill → `run_finished` → export → cleanup) | `steps.rs:38` |
| 13 | Promote | `steps::gate_exhausted` promote branch → `promote::create_promotion_successor` | `steps.rs:222-293` → `promote.rs:64` |
| 14 | Out-of-process decision | `escalation::resolve_gate` → consumed by `pre_seeded_resolution` | `escalation.rs:201`, `:259` |

### Where DECIDING and EXECUTING are mixed in one function

`schedule::next_step` is genuinely pure and is the right shape. The mixing is everywhere *else*:

| Site | Decision taken inside an I/O function | Evidence |
|---|---|---|
| `parallel_exec::execute_parallel` | Re-derives orphan/attempt policy for group children inline and **ignores `on_interrupt` entirely** — always restarts, where `schedule::resume_policies` would honour `fail_if_uncertain`/`resume_session` | `parallel_exec.rs:39-47` vs `schedule.rs:143-158` |
| `gate_exec::emit_started` | Derives the node's attempt by counting `node_started` from the log, then emits | `gate_exec.rs:502-518` |
| `questions_exec::execute_ask` | Same attempt derivation, inline, mid-emit sequence | `questions_exec.rs:106-120` |
| `gate_exec::resolve_internal_gate` | Re-decides whether `abort` is engine-appended (a rule `build_internal_gate_escalation` already owns) while emitting events | `gate_exec.rs:424-426` vs `escalation.rs:76-81` |
| `steps::execute_batch` | Budget policy decision (`spent >= cap`) computed by a second `derive()` inside the executor | `steps.rs:317-338` |
| `prompt_exec::execute_prompt` | Skills/run-tools capability *policy* decided inline, interleaved with `emit` calls | `prompt_exec.rs:142-194` |
| `loop_exec::prepare_loop` | Same policy, second copy, interleaved with `emit` | `loop_exec/mod.rs:252-315` |
| `workflow_exec::execute_workflow` | Depth check, "find the open child", budget cascade, mode floor — all decisions — inside a 314-line I/O function | `workflow_exec/mod.rs:95-408` |
| `replay::apply` | Pure, but rebuilds the whole effective-findings Vec on every finding event (see §2) | `replay.rs:339-346` |

---

## 2. DERIVATIONS — every walk over the event log

`e` = events, `n` = declared nodes, `F` = finding events, `T` = tasks.

| Derivation | File:line | Derives | Cost | Duplicates |
|---|---|---|---|---|
| `replay::derive` | `replay.rs:156` | node states, task states+owners, tokens, findings, artifact ledger, broken, unknown kinds | O(e) — **but O(e + F²)**: `apply` re-materialises `state.findings` from `aux.findings.effective()` on *every* finding event (`replay.rs:339-346`), and `effective()` clones every standing finding (`core/events/findings.rs:118`) | — (the canonical one) |
| `replay::dedup_findings` | `replay.rs:380` | findings collapsed by `(location, title.trim().to_lowercase())` | O(F) | **Diverges** from `findings::inherited_findings` (below) |
| `findings::inherited_findings` | `findings.rs:21` | same collapse, key `(location, split_whitespace().join(" ").to_lowercase())` | O(e + F) — re-folds the ledger via `effective(events)` | Second dedup of the same question, **different normalisation** → `RunFrame::blocking_findings` and the inherited findings artifact can disagree |
| `schedule::next_step`'s `NodeHistory` fold | `schedule.rs:286-303` | per-node `starts`, `last_failed_seq`, `last_finished_seq`, `reroutes`, `last_reroute` | O(e) *on top of* the `derive(events)` at `schedule.rs:216` — two passes per scheduler iteration | `starts` duplicates `NodeState::Running{attempt}`, `stats::walk_attempts.max_attempt`, `gate_exec::emit_started`, `questions_exec`, `parallel_exec` |
| `schedule::last_external_ref` | `schedule.rs:187` | node's last `gate_waiting.external_ref` | O(e) | **Byte-identical copy** at `gate_exec.rs:365-375` |
| `schedule::resume_policies` | `schedule.rs:143` | orphans + their `on_interrupt` | O(n) over derived state | Re-implemented (worse) in `parallel_exec.rs:39-47` |
| `escalation::current_escalation` | `escalation.rs:116` | the escalation a parked run waits on | O(e) ×2 — calls `current_mode_name` then `current_step`, which calls `current_mode_name` again **and** a whole `next_step` (= another `derive` + another `NodeHistory` fold) | `current_mode_name` (`escalation.rs:158`) duplicates `yunta_core::events::run_mode` (`core/events/mod.rs:329`) |
| `escalation::pre_seeded_resolution` | `escalation.rs:259` | latest qualifying `gate_resolved` for a node | O(e) | — |
| `stats::stats_observed_at` | `stats.rs:244` | cptv, rework, cache rate, wall clock, per-node stats | O(e) via `walk_attempts` + O(e) via `Activity::of` + O(n·deps) | `walk_attempts.last_terminal/first_started` overlaps `live::since_last_terminal` and `schedule::NodeHistory` |
| `stats::cptv` | `stats.rs:197` | tokens / tasks done | O(T) | Called from `steps::finish`, `run_failed`, `gate_exhausted` — correctly one function |
| `live::running_since` | `live.rs:38` | node's open attempt start | O(e) **per node** | Same "walk back to last terminal" as `since_last_terminal`, `in_flight_tokens`, `stats::walk_attempts` |
| `live::last_event_age` | `live.rs:58` | silence | O(e) per node | — |
| `live::open_sessions` / `recent_tool_calls` | `live.rs:91`, `:126` | sessions / tool calls of the open attempt | O(e) per node each, via `since_last_terminal` (`live.rs:147`) | Both re-scan back to the last terminal — 2 more walks per node |
| `live::live_total_tokens` | `live.rs:178` | derived total + in-flight | O(e) + a fresh `derive` unless `live_total_tokens_of` is used | `in_flight_tokens` (`live.rs:199`) re-implements attempt-window tracking a fourth time |
| `view::run_frame` | `view/mod.rs:176` | the whole snapshot | **O(n·e)** — self-documented at `view/mod.rs:168-175`: `derive` + `stats_observed_at` + `walk_log` once, then 3 per-node walks × n | The documented, accepted cost |
| `view::walk_log` | `view/mod.rs:285` | runner per node, last reroute, reroute count, child links, degradations | O(e) | `runner` duplicates `stats::walk_attempts.runner` (`stats.rs:465-469`); `reroutes` count duplicates `receipt/mod.rs:254-257` |
| `view::phase::standing` | `view/phase.rs:137` | last phase-moving event + standing pause | O(e) from the tail (early-exits) | Reads "how did the run close" from the **end**; `receipt` reads it from the **front** (`receipt/mod.rs:242`) |
| `view::phase::closed` | `view/phase.rs:165` | last `node_failed` / last `promotion_signaled` | O(e) | — |
| `history::run_summary` | `history.rs:34` | one past run reduced | O(e) — full `compute_run_stats` per run | Callers pay O(runs · e) |
| `history::prior_estimation` | `history.rs:123` | median/p90 over history | O(R log R) | — |
| `verification_effectiveness::analyze` | `verification_effectiveness.rs:91` | 5 independent full walks of *every* historical log (`:103,:137,:176,:229,:283`) | O(R · e · 5) | Each sub-walk re-reads criteria/gate/reroute/task events separately |
| `receipt::build_receipt` | `receipt/mod.rs:236` | criteria, baseline, scope, runners, reroutes, unknown kinds, diagnostics | 7 separate O(e) walks + a `derive` (`:253`) | `runner_usage` duplicates `walk_log.runner` and `walk_attempts.runner`; reroute count duplicates `walk_log.reroutes` |
| `tasks::prior_registrations` | `tasks/mod.rs:52` | last registration identity per task | O(e) | — |
| `tasks::plan_registration` | `tasks/mod.rs:95` | what registering a document states | O(T) | — |
| `tasks::crossing` (`crossing.rs:37`) | | what a source run finished that this tree carries | O(e) + a `derive` | — |
| `artifacts::RunArtifacts::of` | used at `gate_exec.rs:72`, `questions_exec.rs:32`, `distill.rs:159`, `promote.rs:145` | held artifacts | O(e) each | Ledger is already in `RunState.artifacts` — four sites re-fold instead of reading it |
| `progress::render_progress` | `progress.rs:19` | `progress.md` | O(e) + `derive` | Called after **every** `node_finished` (`node_close.rs:190`) — O(nodes · e) per run |
| `prompt_exec::orphaned_session` | `prompt_exec.rs:294` | the session an orphan resumes | O(e) | Fifth attempt-window walk |
| `workflow_exec` open-child scan | `workflow_exec/mod.rs:129-155` | last `child_run_created` with no `child_run_finished` | O(e) ×2 | Not in `RunState` although `ChildRunFinished` *is* folded there (`replay.rs:348`) |

**Per scheduler iteration cost, measured:** `ctx.load_events()` (storage read, `exec.rs:121`) → `next_step` = `derive` + `NodeHistory` (2 walks) → `execute_batch` does `derive(events)` **again** for the budget (`steps.rs:325`) → terminal handlers do `ctx.run_view()` = another storage read + another `derive` (`steps.rs:42,115,292,461`, `exec.rs:69`). So ≥2 log reads and ≥3 full replays per iteration.

---

## 3. SIZE & SHAPE

### `xtask/smells.baseline` — every counter and what it measures

| Counter | Value | What it counts (`xtask/src/smells.rs`) |
|---|---|---|
| `copied_test_helpers` | 0 | lines starting `fn git(` / `fn yunta_in(` / `fn init_repo(` / `struct FixedClock` outside the support crate (`smells.rs:259-271`) |
| `exit_failure_in_cli` | 2 | `ExitCode::FAILURE` lines in `crates/cli/src` (`smells.rs:250`) |
| `git_command_new_files` | 2 | **files** containing `Command::new("git")` (`smells.rs:238-248`) |
| `let_underscore_in_prod` | 24 | lines starting `let _ =` outside `#[cfg(test)]` (`smells.rs:200-207`) |
| `prod_fns_over_50_lines` | 81 | function bodies > 50 lines, brace-matched, tests blanked (`smells.rs:208-212`, `functions_over` at `:325`) |
| `space_runs_in_prod_strings` | 0 | ≥8 inner spaces in a string literal — a lost `\` continuation (`smells.rs:272-280`) |
| `src_files_over_500_lines` | 11 | files > 500 lines, **tests included** (`smells.rs:213-224`) |
| `utc_now_outside_clock` | 0 | lines containing `Utc::now` outside `clock.rs` (`smells.rs:225-237`) |

Explicitly **not** counted: unwrap/expect/panic/indexing — delegated to the crate-root `#![deny(...)]` (`smells.rs:169-173`).

### Engine's share of the debt

- **5 of the 11 files > 500 lines** are engine: `run/workflow_exec/mod.rs` 691, `run/gate_exec.rs` 599, `run/schedule.rs` 586, `stats.rs` 566, `worktree/mod.rs` 506. (Others: `core/events/payloads.rs` 989, `core/ids.rs` 860, `cli/commands/stats.rs` 754, `storage/store.rs` 610, `adapters/forge/github.rs` 549, `cli/commands/pack.rs` 548.)
- **56 of the 81 functions > 50 lines (69%)** are in `crates/engine/src`. (cli 15, core 5, storage 3, adapters 2, testkit 1.)
- Files 450–500 (the next tranche to trip the ratchet): `run/loop_exec/mod.rs` 484, `check/error.rs` 471, `run/steps.rs` 465, `run/exec.rs` 455.

### Functions > 45 lines in `crates/engine/src` (63 total; body lines)

| Lines | Function | Structural or incidental |
|---|---|---|
| 370 | `run/schedule.rs:208 next_step` | **Structural** — five decision sections in one body; pure, so it splits cleanly into per-section functions over one shared `NodeHistory`/`RunState` |
| 314 | `run/workflow_exec/mod.rs:95 execute_workflow` | **Structural** — resume-lookup + resolve + check + render + mount + budget + freeze + worktree + link + create + drive |
| 201 | `replay.rs:170 apply` | Incidental — one arm per event kind; the shape is right |
| 185 | `run/parallel_exec.rs:20 execute_parallel` | **Structural** — decides orphan/attempt policy *and* runs the race |
| 184 | `run/workflow_exec/mod.rs:501 drive_child` | **Structural** — 3 near-identical `ChildRunFinished` constructions + promotion chain |
| 155 | `run/loop_exec/integrate.rs:27 integrate_batch` | Structural (rebase + re-verify + fast-forward) |
| 153 | `check/mod.rs:69 check` | Incidental (a list of checks) |
| 151 | `run/prompt_exec.rs:122 execute_prompt` | **Structural** — capability policy + request build + dispatch + close |
| 149 | `run/loop_exec/mod.rs:31 execute_loop` | Structural |
| 141 | `run/questions_exec.rs:26 execute_ask` | **Structural** — ask round + attempt derivation + 3 event emissions + accept |
| 137 | `run/loop_exec/mod.rs:231 prepare_loop` | **Structural** — duplicate of `execute_prompt`'s setup half |
| 132 | `run/node_exec.rs:67 execute_node` | Incidental (kind dispatch) but carries the blackboard-consolidation special case (`:133-146`) |
| 129 | `run/steps.rs:164 gate_exhausted` | **Structural** — ask + 3 consequences incl. a full promotion close inline |
| 123/117/116/115/114/114/112/106/105/105/101 | `attempt.rs:56`, `task_cycle/mod.rs:234`, `escalate.rs:23`, `catalog.rs:86`, `create.rs:115`, `receipt/render.rs:33`, `session.rs:277`, `distill.rs:130`, `attempt.rs:190`, `gate_exec.rs:387`, `node_close.rs:86` | mixed |
| 91 | `process.rs:198 spawn_governed` | Incidental — one coherent mechanism |
| 87 | `run/exec.rs:99 execute_run_at_depth` | Incidental — pure dispatch table; correct shape |
| 71 | `run/exec.rs:365 build_ctx` | Incidental (destructure + restructure) |
| 66 | `run/steps.rs:38 finish` | **Structural** — close + cleanup policy in one |
| …47 more ≥45 | | |

---

## 4. ERRORS & PANICS

### Typed errors — this is the strongest part of the codebase

29 error enums, one per module, all `thiserror`, all preserving cause with `#[source]`/`#[from]`:
`RunError` (`run/mod.rs:100`), `ManifestReadError` (`:72`), `ResolveGateError` (`escalation.rs:293`), `TaskCycleError`, `DispatchError`, `SpawnError`, `WorktreeError`, `LockError`, `ScopeCheckError`, `ScopeExpansionError`, `CheckError`, `AcceptError`, `ObjectError`, `ContextResolveError`, `McpQueryError`, `MountError`, `RunToolsSetupError`, `ReplayError`, `CatalogError`, `TemplateError`, `InputsError`, `ManifestError`, `EventsExportError`, `ReceiptError`, `SkillsError`, `RunnerError`, `RunToolError`, `SubmitError`, `SchemaRangeError`.

`RunError` variants that flatten cause into a `String` rather than keeping it: `Broken { diagnostic: String }` (`run/mod.rs:107`), `Git { context, detail }` (`:165`), `ManifestWrite { detail }` (`:168`). `Git` is notable: `GitError` is a perfectly good typed error (`git.rs:18`) that is stringified at `loop_exec/integrate.rs:316-319` and `worktree/mod.rs:44-49` rather than carried as `#[source]`.

### Panics in prod paths

- **Crate-root denies are real and enforced**: `crates/engine/src/lib.rs:11-17` denies `unwrap_used`, `expect_used`, `panic`, `unreachable`, `indexing_slicing`; `clippy.toml` lifts the panic family in tests; `#![cfg_attr(test, allow(clippy::indexing_slicing))]` at `lib.rs:22`. Same block at all 5 crate roots.
- **A grep over engine prod code (cfg(test) modules masked) finds zero `unwrap`/`expect`/`panic!`/`unreachable!`.** Verified.
- **Two panics the lints cannot see** — `clippy::indexing_slicing` fires on arrays/slices, not on `HashMap`'s `Index`:
  - `run/prompt_exec.rs:137` `let adapter = &ctx.adapters[&chosen.adapter];`
  - `run/loop_exec/mod.rs:244` `let adapter = ctx.adapters[&chosen.adapter].clone();`
  The invariant ("the resolver was given `|a| ctx.adapters.contains_key(a)`", `runner_resolve.rs:44`) is real but lives in a closure two calls away, not in a type. `ctx.adapters.get(&chosen.adapter).ok_or(...)` costs nothing.
- **One string slice** the lint also misses: `task_cycle/session.rs:131` `&hash[..12]` (safe because sha256_hex is 64 ASCII chars — again an untyped invariant).

### Where human text is produced

**Mostly inside the engine, and stored on the log.** The one place this is done right is `node_failed`: `Failure` is a typed enum (`core/events/failure.rs:26`) whose prose is produced by `Display` (`:65-69`) — `node_close::fail_with` (`node_close.rs:279`) is the single writer. But:

- `Failure::Message { outcome: String }` is the dominant arm and is built with `format!` at every failure site: `node_exec.rs:100`, `node_close.rs:169-176`, `node_close.rs:229`, `gate_exec.rs:81-86,247-250,469`, `prompt_exec.rs:267`, `runner_resolve.rs:30-34,66-73`, `loop_exec/mod.rs:73-75,306-309`, `workflow_exec/mod.rs:110-116,166,187,…`.
- `RunPausedPayload { reason: String }` (`core/events/payloads.rs:955-957`) is pure engine prose, built at `schedule.rs:394-398`, `schedule.rs:451`, `schedule.rs:574`, `schedule.rs:583`, `steps.rs:296-299`, `gate_exec.rs:125,277`, `budget.rs:85-88,122-125`, `questions_exec.rs:92-97`.
- `GateWaitingPayload.summary` — engine prose (`escalation.rs:42-46`, `:83-85`, `budget.rs:73`, `gate_exec.rs:95-98`).
- `NodeReroutedPayload.cause` stores `failure.to_string()` (`schedule.rs:435,442`) — a *rendering* of a typed `Failure`, persisted back as a string. Double storage of the same fact in two shapes.
- 275 `format!` calls in `crates/engine/src`, 155 of them under `run/` (19 in `gate_exec.rs` alone).

So: text is produced once *per fact* (good — no two surfaces disagree), but it is produced **inside the engine and frozen into the log**, not at the border. A surface cannot re-render a pause reason, localise it, or restructure it. `Failure` shows the pattern that would fix it; only the artifacts arm uses it.

---

## 5. INJECTION & PURITY

| Rule | Status | Evidence |
|---|---|---|
| `Clock` injected | **Mostly.** One `Clock` on `RunEnv`/`RunCtx`, carried into `RunLog` so every append is stamped once (`run_log.rs:63`). `Utc::now` exists only in `core/src/clock.rs:18` (baseline `utc_now_outside_clock 0` holds). | |
| | **Leak: 3 `SystemClock` constructed inside the engine**, bypassing the injected one | `worktree/mod.rs:210` (`hand_over`), `:379` (mutation lock), `:424` (isolation lock) |
| | The ratchet greps `Utc::now`, so it cannot see `&SystemClock` — the counter measures the symptom, not the rule | `xtask/src/smells.rs:225-237` |
| `IdSource` injected | Yes, everywhere: `ids.mint_run_id(clock.now())` at `promote.rs:79`, `workflow_exec/mod.rs:328` | |
| Randomness | One source: `uuid::Uuid::new_v4()` for the per-session MCP bearer token (`run_tools/listener.rs:93-97`). **Not injected** — but it is a credential, not a decision; the log never carries it. Acceptable. | |
| `std::env` in the engine | **2 leaks.** `RunEnv.ambient` exists precisely to inject the environment (`run/mod.rs:271-276`), yet secrets and MCP auth are read from the ambient process instead | `task_cycle/session.rs:66` `std::env::var(name)`; `run/context_resolve/mcp.rs:53` `std::env::var(var)` |
| `std::process` in the engine | Confined to `process.rs` and `git.rs` (`git_command_new_files 2`) | |
| Sync `std::fs` inside `async fn` | **15 direct sites + 1 indirect.** These block the runtime | `run/node_close.rs:255` (`progress.md`, after every node close); `run/distill.rs:151,173,178,228`; `run/context_resolve/sources.rs:40,237`; `run/context_resolve/knowledge.rs:171`; `run/workflow_exec/mod.rs:174`; `lock.rs:158,178,185`; `worktree/mod.rs:159,230`; `task_cycle/criteria.rs:108`; **indirect:** `process_registry::persist` (`process_registry.rs:96-103`) is called from `add`/`remove`, invoked from inside `spawn_governed` (`process.rs:231`) |
| | Contrast: `create_run` (`create.rs:159-199`) and `export_events_jsonl` (`ctx.rs:176`) do it correctly with `tokio::fs` — so the async path exists and is simply not used consistently | |
| No sleeps for synchronisation | Honoured. One deliberate grace period, documented with rationale: `task_cycle/session.rs:134-138` | |
| Spans | `#[tracing::instrument(fields(run_id, depth))]` on `execute_run_at_depth` (`exec.rs:98`); `fields(run_id, node_id, attempt)` on `execute_node` (`node_exec.rs:63`); `fields(task, …)` on `run_task` (`task_cycle/mod.rs:227`). **Gap:** gate and questions nodes never pass through `execute_node`, so `gate_exec::publish_gate/poll_gate/resolve_internal_gate` and `questions_exec::execute_ask` carry **no node span at all** — "un span por run y por nodo" is not met for two node kinds | |
| **Frontera** | **Clean.** No adapter id, model name, CLI path or binary name appears in engine code — only in doc comments. `ResolveGateError::NotPaused` (`escalation.rs:294-296`) even documents refusing to name a CLI command in its message. Adapter capabilities are consulted through `capabilities().declares(...)`, never by name | |
| | Two host assumptions not declared as capabilities: `GovernedCommand::shell` hard-codes `"sh"` (`process.rs:98`), and git is a direct, unabstracted dependency of the engine (`git.rs`) | |

---

## 6. OWNERSHIP

### What is right

- `spawn_governed` (`process.rs:198`) is the model: `process_group(0)` (`:217-221`), pgid captured (`:228`), RAII registration (`:231` + `process_registry.rs:109-134`), `select!` over cancel/timeout/exit (`:256-263`), `SIGKILL` to the **group** then reap on either non-exit path (`:264-270`), both pipes drained on every path (`:274-275`).
- All three `tokio::spawn` sites keep their handle: stdin writer awaited (`process.rs:233-240, 271-273`), stdout/stderr handles returned and drained (`:303-315`), MCP listener handle owned by `RunToolsSession` and aborted in `Drop` (`run_tools/listener.rs:27-37`).
- Adapter sessions are registered too (`task_cycle/session.rs:169-173`) and get interrupt→grace→kill (`:240-247`).
- `ProcessRegistry::drop` deletes `scratch/engine.json` on every exit; a SIGKILL deliberately leaves it for a post-crash `yunta cancel` (`process_registry.rs:136-145`).
- `WorktreeMutationGuard` releases in `Drop` so an early `?` cannot leak the lock (`worktree/mod.rs:345-353`).

### Gaps

1. **Every git subprocess bypasses `spawn_governed` entirely.** `git.rs:116,132,145` use `tokio::process::Command::new("git")` and `:157,167` use `std::process::Command`. So `git worktree add`, `git rebase`, `git commit`, `git push`, `git diff` run **with no process group, no registration in `scratch/engine.json`, no timeout, and no cancellation**. A `git push` blocked on a credential prompt, or a `rebase` in `integrate.rs`, survives Ctrl-C and is invisible to `yunta cancel`. This is the single clearest breach of "cada subproceso nace en su process group, queda registrado y muere con el árbol completo en todo camino de cancelación".
2. **A failed kill is discarded.** `task_cycle/session.rs:244,246` `let _ = session.interrupt().await; … let _ = session.kill().await;` — if the adapter cannot kill the session, nothing is recorded anywhere.
3. **Registry write failures degrade to `tracing`, not to the log.** `process_registry.rs:72,82,91` emit `tracing::warn!`, while `RunCtx::engine_finding`'s own doc (`ctx.rs:184-189`) says degradations go to the log "never a `tracing` warning that leaves the log silent". Only the *initial* create failure becomes a finding (`exec.rs:233-245`); every later add/remove failure is silent.
4. `read_registry` maps both "absent" and "corrupt" to `None` (`process_registry.rs:157-160`) — a documented silent degradation, but "un archivo inválido se reporta como inválido" says it should be distinguishable.
5. `RunToolsSession::drop` aborts but cannot await the handle (`listener.rs:33-37`) — unavoidable in `Drop`, documented; noted for completeness.

---

## 7. DUPLICATION — both copies cited

| # | Duplicated thing | Copy A | Copy B (+C…) | Divergent? |
|---|---|---|---|---|
| 1 | `last_external_ref` — node's last `gate_waiting.external_ref` | `schedule.rs:187-194` | `gate_exec.rs:365-375` | Byte-identical |
| 2 | "this node's attempt number" | `schedule.rs:293` (`starts+1`, passed as `Execute(node, attempt)`) | `gate_exec.rs:502-511`; `questions_exec.rs:106-115`; `parallel_exec.rs:41-46`; `stats.rs` `max_attempt`; `replay.rs:209` | 6 derivations; `parallel_exec`'s ignores `on_interrupt` |
| 3 | Session setup for a prompt vs a task session | `prompt_exec.rs:142-194` (skills resolve, Skills degradation, run-tools open) | `loop_exec/mod.rs:252-315` (same, reworded) | **Yes**: loop's inline run-tools gating (`:288-315`) omits the `TypedArtifactNeedsRunTools` / `TypedArtifactListenerFailed` checks that `open_run_tools` performs (`runner_resolve.rs:180-232`) — a loop node declaring an interpreted artifact on a run-tools-less adapter is not refused |
| 4 | `gate_waiting` + `gate_resolved` pair, guarded by `!already_recorded` | `steps.rs:199-207` | `gate_exec.rs:431-439`; `gate_exec.rs:449-457`; `gate_exec.rs:559-565`; `budget.rs:43-49` | 5 copies |
| 5 | `run_finished` + `RunMetrics{cptv, tokens}` + `export_events_jsonl` | `steps.rs:43-54` (Done) | `steps.rs:116-127` (Failed); `steps.rs:276-287` (Promoted) | 3 copies |
| 6 | `ChildRunFinished` construction | `workflow_exec/mod.rs:554` (Done) | `:589` (Promoted); `:649` (Failed) | 3 copies |
| 7 | Findings dedup | `replay.rs:380-393` (`title.trim().to_lowercase()`) | `findings.rs:21-43` (`split_whitespace().join(" ").to_lowercase()`) | **Yes** — internal whitespace collapsed in one, not the other |
| 8 | `declares <artifact kind>` predicate | `schedule.rs:175-182` (questions) | `questions_exec.rs:36-41` (questions); `node_artifacts.rs:26-32` (findings) | 3 copies of one shape |
| 9 | "node X asked N question(s) awaiting an answer: …" | `questions_exec.rs:92-97` | `node_close.rs:169-176` | Same sentence, two `format!` |
| 10 | Mode from the log | `core/events/mod.rs:329 run_mode` | `escalation.rs:158-163 current_mode_name` | Yes — `Option` vs sentinel |
| 11 | Template-render-or-fail | `node_exec.rs:286-295 render_or_fail` | `gate_exec.rs:586-598 render_or_fail_here` | Differs only by an `emit_started` before the fail |
| 12 | Runner per node from `runner_resolved` | `view/mod.rs:289-300` | `stats.rs:465-469`; `receipt.rs:377 runner_usage` | 3 walks |
| 13 | Reroute count | `view/mod.rs:302` | `receipt/mod.rs:254-257` | — |
| 14 | "how did the run close" | `view/phase.rs:100-101` (scans from the **tail**) | `receipt/mod.rs:242-247` (`find_map` from the **front**) | Yes in principle (only one `run_finished` should exist, but the two readings disagree if one ever doesn't) |
| 15 | `".yunta/knowledge/distilled"` | `distill.rs:148` (literal) | `distill.rs:244` (`const DISTILLED_DIR`) | Same file, two copies |
| 16 | `"manifest.yaml"` | `create.rs:189` | `workflow_exec/mod.rs:426`; `mounts.rs:167`; + 5 in cli | No const, while `SCRATCH_DIR`/`ARTIFACTS_DIR` have one (`run_dir.rs:19`, core) |
| 17 | The 10-line `#![deny(clippy::…)]` + comment block | `engine/src/lib.rs:11-22` | `core/src/lib.rs:10-21`; `adapters`, `storage`, `cli/main.rs` | 5 copies; `clippy.toml:3` claims they live in `[workspace.lints.clippy]`, **which does not exist in `Cargo.toml`** (only `[workspace.lints.rust] unsafe_code`) |

---

## 8. DEFECTS

| # | Defect | Evidence | Root cause |
|---|---|---|---|
| D1 | Git subprocesses are ungoverned: no process group, no registry, no timeout, no cancellation | `git.rs:116,132,145,157,167` vs `process.rs:198-293` | layering |
| D2 | `parallel_exec` re-decides orphan handling and ignores `on_interrupt` | `parallel_exec.rs:39-47` vs `schedule.rs:143-158` | decide/execute mixed + duplicated derivation |
| D3 | A loop node declaring an interpreted artifact on a run-tools-less adapter is not refused | `loop_exec/mod.rs:288-315` omits `runner_resolve.rs:199-232` | duplicated construction |
| D4 | Two findings-dedup rules that disagree on internal whitespace | `replay.rs:384` vs `findings.rs:38-43` | duplicated derivation |
| D5 | `derive()` is O(e + F²): the whole effective-findings Vec is rebuilt and cloned per finding event | `replay.rs:339-346`, `core/events/findings.rs:117-122` | duplicated derivation |
| D6 | Two full log reads + three full replays per scheduler iteration | `exec.rs:121` → `schedule.rs:216` → `steps.rs:325` → `steps.rs:42/292/461` | duplicated derivation |
| D7 | `progress.md` rewritten (full log read + render + **sync** `std::fs::write`) after every `node_finished` | `node_close.rs:190,252-259` | size/purity leak |
| D8 | Two `HashMap` index panics in prod, invisible to `clippy::indexing_slicing` | `prompt_exec.rs:137`, `loop_exec/mod.rs:244` | purity leak (invariant not in a type) |
| D9 | `encode_ref` swallows a serialization failure into `""`, which later surfaces as a confusing "external_ref isn't valid" | `gate_exec.rs:525` vs `:528-532` | silent degradation |
| D10 | Process-registry add/remove/clear failures go to `tracing`, contradicting `ctx.rs:184-189`'s own rule | `process_registry.rs:72,82,91` | silent degradation |
| D11 | `read_registry` reports "corrupt" as "absent" | `process_registry.rs:157-160` | silent degradation |
| D12 | A failed adapter `interrupt`/`kill` is discarded | `task_cycle/session.rs:244,246` | silent degradation |
| D13 | `git::success(...).unwrap_or(false)` collapses "git could not spawn" into "git exited non-zero" | `distill.rs:251` | silent degradation |
| D14 | `GateStep::StillWaiting` does not say whether `run_paused` was already written; the caller must know which producer it came from — `gate_exec` writes it (`:125,277,555`), `resolve_internal_gate` does not (`:414,440`), and `steps.rs` compensates at `:432-434` vs `:454-463` | `gate_exec.rs` / `steps.rs` | decide/execute mixed (invalid state representable) |
| D15 | `run_paused` has exactly one constructor (`exec.rs:36`) — correct — but `node_finished` has four (`node_close.rs:184`, `gate_exec.rs:188,490,570`, `questions_exec.rs:160`) and `node_started` three with two different attempt rules | as cited | duplicated construction |
| D16 | Gate and questions nodes get no tracing span with `node_id` | no `instrument` in `gate_exec.rs` / `questions_exec.rs` | layering |
| D17 | 15 sync `std::fs` calls inside `async fn` + `persist()` reached from `spawn_governed` | §5 table | purity leak |
| D18 | 3 `SystemClock` values constructed inside the engine | `worktree/mod.rs:210,379,424` | purity leak |
| D19 | Secrets and MCP auth read from the process env while `RunEnv.ambient` exists to inject it | `task_cycle/session.rs:66`, `context_resolve/mcp.rs:53` vs `run/mod.rs:271-276` | purity leak |
| D20 | `next_step` 370 lines / `execute_workflow` 314 / `drive_child` 184 / `execute_parallel` 185 — 5 engine files > 500 lines, 56 engine functions > 50 | §3 | size |
| D21 | `verification_effectiveness::analyze` makes 5 independent full passes over every historical log | `:103,:137,:176,:229,:283` | duplicated derivation |
| D22 | `current_escalation` derives the mode twice and runs `next_step` (= a full `derive` + `NodeHistory`) a second time | `escalation.rs:120` + `:168-177` | duplicated derivation |
| D23 | `RunArtifacts::of` re-folds the artifact ledger at 4 sites although `RunState.artifacts` already holds it | `gate_exec.rs:72`, `questions_exec.rs:32`, `distill.rs:159`, `promote.rs:145` vs `replay.rs:84` | duplicated derivation |
| D24 | `clippy.toml:3` points at `[workspace.lints.clippy]`, which does not exist; the deny block is copy-pasted at 5 crate roots | `clippy.toml` vs `Cargo.toml:73-75` | duplicated construction + doc/code drift |
| D25 | `".yunta/knowledge/distilled"` literal beside its own const; `"manifest.yaml"` has no const at all | `distill.rs:148/244`; `create.rs:189` et al. | duplicated construction |
| D26 | `GitError` (typed, with `#[source]`) is stringified into `RunError::Git { detail: String }` / `WorktreeError::Git { detail: String }` | `run/mod.rs:165`, `worktree/mod.rs:44-49`, `integrate.rs:316-319` | layering (cause not preserved) |

---

## 9. IDEAL

### What a greenfield core looks like here

**One decider, one executor, one derivation per question.**

```
RunState  ← derive(events)                     // one pass, one owner of every reading
Decision  ← decide(&Workflow, &RunState, Policy)  // pure, total, no log re-walk
Effect    ← execute(Decision, Shell)            // I/O only; emits Facts
Fact      ← one constructor per event kind
```

1. **`RunState` absorbs what `next_step` re-walks.** `starts`, `last_failed_seq`, `last_finished_seq`, `reroutes`, `last_reroute`, `last_external_ref`, and the open-attempt window are all folds over the same events `derive` already visits. Fold them there once (`replay.rs:198-371`) and delete `NodeHistory` (`schedule.rs:196-303`), `gate_exec::last_external_ref` (`:365`), the three ad-hoc attempt counts, `prompt_exec::orphaned_session`'s window scan, `live::since_last_terminal`, and `stats::walk_attempts`'s overlap. That is ~6 of the 8 duplicated derivations in §2, gone by construction.
2. **`next_step` becomes five small total functions over that state** — `gate_step`, `waiting_step`, `orphan_step`, `failure_step`, `ready_batch` — composed by one `decide`. Already pure; only the size is wrong.
3. **`Fact` constructors, not payload literals at the call site.** `run_paused` already has exactly one (`exec.rs:36`) and it is the right pattern. Add `node_started(attempt)`, `node_finished(outcome, tokens)`, `gate_decision(escalation, choice)`, `run_closed(terminal, state)`, `child_closed(child, terminal, tokens)`. That kills D15 and duplications #4, #5, #6.
4. **One `SessionPlan` builder** consuming `(ctx, node, chosen, adapter)` and producing `(SessionRequest, Vec<Degradation>)`, used by both `execute_prompt` and `prepare_loop`. Kills #3 and its divergence.
5. **One `Shell`**: every subprocess — git included — through `spawn_governed`; every file write through `tokio::fs` or `spawn_blocking`; every clock read through the injected `Clock`. Kills D1, D17, D18.
6. **Prose leaves the log.** `Failure` already shows it: a typed reason + `Display` at the border. Apply the same to `RunPausedPayload.reason` (a `PauseReason` enum: `Cancelled`, `BudgetExhausted{spent,cap}`, `GateUnanswered{node}`, `UncertainOrphans{nodes}`, `Blocked`, `NodeFailed{node, failure}`) and to `GateWaitingPayload.summary`. The engine then stores facts; surfaces render them.
7. **`run_paused` recorded by the pause path, never by the gate path.** Make the gate return `GateOutcome::Waiting(PauseReason)` and let `steps` record it. D14 disappears with the ambiguous variant.

### What is already right and must be kept — judged

| Piece | Verdict |
|---|---|
| **`RunLog` seam** (`run_log.rs`) | **Keep unchanged.** One writer, one reader, timestamp taken before the blocking hop, observer hung off the single door. The three documented exceptions (`create_run`, `resolve_gate`, `record_pause_after_crash`) are each justified and held to by a test (`observer.rs:29-31`). This is the best-designed piece in the crate. |
| **Observer boundary** (`observer.rs`) | **Keep.** Sync-by-contract, returns nothing, drops frames by policy, and the doc explains *why* each choice differs from `SessionObserver::emit_session_event` (which does return a `Result`, correctly). Textbook. |
| **`RunFrame` / `view/`** (`view/mod.rs:176`) | **Keep.** Pure, `now` injected, `Counter` deliberately has no `done/total` method, `NodeStanding` is an enum not flags, composition is a tree of `ChildLink`s not an average. Its O(n·e) cost is documented at `:168-175` — but it should read the per-node liveness off the folded `RunState` rather than re-walking (item 1), which makes it O(e + n). |
| **`lock` + `hand_over`** (`lock.rs`, `worktree/mod.rs:203-216`) | **Keep the protocol** — one owner record, one liveness probe, `Contention::{Refuse,Wait}`, stolen locks reported as `StoleStaleLock` never silently. **Fix two things:** the sync `std::fs` inside `async fn acquire` (`lock.rs:158,178,185`) and the three engine-constructed `SystemClock`s. |
| **`worktree`** | **Keep.** `WorktreeIntegrity` verifying identity+ancestry but deliberately not content, `branch -d` never `-D`, `WorktreeMutationGuard` releasing in `Drop`, locks in the common git dir so they never appear in a scope diff — all correct and all explained. The git-governance gap (D1) is in `git.rs`, not here. |
| **`reserved` offers** (`reserved.rs`) | **Keep as-is.** `GateOption` is constructible only through `ReservedOption::offer`, which cannot produce an option without a tradeoff — "every option declares what it trades off" is true *by type*, not by discipline. This is the single-constructor pattern the rest of the engine needs and mostly lacks. |
| **`escalation`'s two builders** (`escalation.rs:28,60`) | **Keep.** One construction site each, shared by the live path and `current_escalation`, so a `resolve_gate` from another process reconstructs the identical menu. The duplicated `current_mode_name` (`:158`) and the double `next_step` (`:168`) are the only blemishes. |
| **`pre_seeded_resolution`** (`escalation.rs:259`) | **Keep.** Pure, and the design decision — record only the decision, let the next live engine apply the consequence through the one existing path — is exactly right: zero duplicated consequence logic. |
| **`process.rs` `spawn_governed`** | **Keep and make universal.** It is the correct ownership mechanism; the defect is that git does not use it. |
| **`Failure`** (`core/events/failure.rs`) | **Keep and generalise.** The typed-fact-plus-`Display` pattern should replace `reason: String` and `summary: String` everywhere. |
| **`node_close::fail_with`** (`node_close.rs:279`) | **Keep.** The one writer of `node_failed`. Give `node_finished` the same treatment. |
| **`xtask` smells ratchet** | **Keep, but extend.** It is honest about being line-based and it holds real ground. Its blind spots are the ones this audit found: `utc_now_outside_clock` misses `&SystemClock`; nothing counts sync `fs` in `async fn`, `Command::new` outside `spawn_governed`, or `HashMap` indexing. Three greps would close all three. |
