I have read the whole CLI crate. Here is the audit.

---

# FRONT audit — `crates/cli/src` (15,368 lines, 60 files)

## 1. MAP — how a command runs

```
main.rs:41  main() → tracing init → cli::Cli::parse()
main.rs:81  drive(cli) → builds its own current-thread tokio runtime, block_on(cli.run()),
            shutdown_background()  [deliberate: a thread parked in a terminal read must not
            block process exit — main.rs:66-80]
cli.rs:25   Cli::run → dispatch(command)            (cli.rs:308-399, 93 lines)
            ↓ one match arm per subcommand → commands::X::x(..)
commands/X  Context::load()  (context.rs:31) → project::resolve (project.rs:174)
            ctx.storage() / ctx.async_storage()  (context.rs:52-60)
            yunta_engine::{derive | run_frame | execute_run | …}
            render/… or surface/… or json::print_json
            → Result<Outcome, CliError>
main.rs:54  ask::restore_terminal()  (last, before exit — main.rs:50-53)
main.rs:56  Outcome::Success → 0 | Outcome::Reported → 1 | Err(e) → eprintln!("error: {e}") + 1
```

### Who shares the "open a run" door

There is **no** `open_run(run_id) -> (run_dir, manifest, events)` helper. The only shared piece is `Project::run_dir` (project.rs:32-41), which answers *where* — the search order (current runs root, then default user root). Everything above it is re-typed per command.

| duplicated fragment | copies |
|---|---|
| `project.run_dir(id).unwrap_or_else(\|\| runs_root.join(id))` | 6 — status/mod.rs:44, stats.rs:62, receipt.rs:60, cancel.rs:63, drive.rs:301, mcp.rs:280 |
| `let Some(run_dir) = …run_dir(id) else { refuse }` | 5 — resume.rs:42, resolve_gate.rs:27, verify.rs:60, list/runs.rs:230, gc.rs:88 |
| `events.is_empty() → "no run \`{id}\` …"` | 10 spellings, 3 different sentences — graph.rs:74, stats.rs:53, receipt.rs:53, cancel.rs:43, status/mod.rs:35, mcp.rs:274 (`in {storage}`); resolve_gate.rs:29, resume.rs:44, mcp.rs:360, mcp.rs:389 (`under {runs_root}`, two of them without the "(or the default state root)" clause) |
| `load_yaml(run_dir.join("manifest.yaml"), "run manifest")` | 7 — status/mod.rs:47, stats.rs:65, receipt.rs:62, resume.rs:49, resolve_gate.rs:33, gc.rs:159, mcp.rs:284; plus a hand-rolled 8th in mcp.rs:393 (`read_to_string` + `yaml::parse`) and a 9th in list/runs.rs:240 |

`stats::collect_history` (stats.rs:142) and `collect_raw_history` (stats.rs:194) **do not** use `run_dir` at all — they do `runs_root.join(id).join("manifest.yaml")`, so a run living under the default state root is silently dropped from every history, estimation and budget warning that command computes.

| command | loads a run by | derives state by | renders via |
|---|---|---|---|
| `run` (attached) | creates it (run.rs:335 `create_run_from`) | `execute_run` → `RunReport`; closing re-reads log (drive.rs:344) | `Surface` (region/lines) + `Closing::render` |
| `run --detach` | creates it, hands off (detach.rs:115) | none (nothing to derive yet) | one `println!` (detach.rs:67) or `RunJson::detached` |
| `resume` | `parked()` resume.rs:41 (own run_dir + manifest) | same `drive()` as run | same `Surface` + `Closing` |
| `status` | own block, status/mod.rs:31-47 | `run_frame` (progress.rs:42) **and** `derive` (status/mod.rs:56) — two passes | `progress::summary` + `NodeDisplay` + `decision::block(Page)` |
| `status --json` | same | `derive` + `run_frame` (status/mod.rs:344-345) | `StatusJson` |
| `list --runs` | own block, list/runs.rs:225-251 | `run_frame` per run (runs.rs:253) | `RunRow::render` (own column consts) |
| `list` (workflows) | n/a | `collect_history` + `prior_estimation` | `render_catalog` → String |
| `stats <run>` | own block, stats.rs:50-65 | `compute_run_stats` + `derive` (stats.rs:67,79) | 25 inline `println!`s |
| `stats --workflow` | `collect_history` **and** `collect_raw_history` (two near-identical walks) | engine | inline `println!`s |
| `receipt` | own block, receipt.rs:49-62 | `build_receipt` (engine) | engine renderers |
| `verify` | `run_dir` only; log read twice (verify.rs:40,74) | `verify_chain` + `ArtifactIntegrity` | `println!`/`note` |
| `graph --run` | own `Storage::open` (graph.rs:70) — bypasses `ctx.storage()` | `derive` + `mode_included_nodes` | `NodeDisplay::label` into Mermaid/DOT |
| `cancel` | own block, cancel.rs:39-64 | `derive` + hand-rolled terminal scan (cancel.rs:49) | `println!`/`note` |
| `gc` | `run_dir` + manifest, gc.rs:88,159 | `derive` + hand-rolled terminal scan (gc.rs:67-74) | `println!` |
| `resolve-gate` | own block, resolve_gate.rs:27-33 | engine `resolve_gate` | one `println!` |
| `mcp workflow_status` | own block, mcp.rs:266-284 | `status_json` (shared ✅) | `to_json_string` |
| `mcp resolve_gate` | own block, mcp.rs:386-395 | engine | own `format!` |
| `test` | creates a run in a temp sandbox (test.rs:247-300) | `report.state` | `println!` + `text::problems` |

Two independent scans for "did this run reach a terminal" are hand-written against `EventPayload` in cancel.rs:49-54 and gc.rs:68-74, while `RunPhase`/`RunFrame` already answer it; a third lives in run.rs:83-88 (`count_non_terminal_runs`).

---

## 2. THE BORDER — where human text is produced

`error.rs` (111 lines) is a genuinely good border *design*: `CliError` is a thin sum of **typed module errors** carried transparently (`Project`, `Pack`, `Storage`, `Catalog`, `Run`, `ManifestRead`, `Manifest`, `DetachedResume`, `ResolveGate`, `Worktree`, `FrozenPaths` — error.rs:44-75), plus `Io{context,source}` and one escape hatch `Message(String)`. `main.rs:60` is the only `eprintln!("error: …")`. `warn`/`note` (error.rs:102-110) are the only `warning:`/stderr-note doors.

The escape hatch is where it leaks.

- **`CliError::msg` is used 52 times**, 34 of them `CliError::msg(format!(…))`:

| file | `CliError::msg(format!)` |
|---|---|
| commands/pack.rs | 10 |
| resolve_gate.rs / receipt.rs / cancel.rs | 3 each |
| test.rs / new.rs / mcp.rs / init.rs | 2 each |
| main.rs, graph.rs, status/mod.rs, stats.rs, run.rs, resume.rs, pack_audit.rs | 1 each |

  Several of these *are* the right border (`refused()` in resolve_gate.rs:71-79 adds the CLI's own "`yunta status` shows where it is" to an engine verdict — exactly right). But the run-not-found sentence (§1) and the "no run directory" sentence are phrased at six and five call sites respectively, which is the same fact worded from scratch each time.

- **`String` used as an error type across a whole module boundary**, losing the typed cause:
  - `promote.rs:89` and `promote.rs:109`: `.map_err(|e| e.to_string())` on `create_promotion_successor` and `execute_run` — the engine's typed `RunError`/promotion error is flattened to a string, then re-wrapped at drive.rs:219 as `CliError::msg`. A `RunError` that `CliError` already carries transparently (error.rs:56-57) is destroyed on this path only.
  - `test.rs`: `run_case` returns `Result<Vec<String>, String>` and calls `yunta_core::describe(&e)` / `format!` at 9 sites (test.rs:217, 224, 228, 234, 254, 276, 302, 354, 366, 419).
  - `mcp.rs`: **17** `.map_err(|e| e.to_string())` / `format!` sites (lines 130, 246, 266, 267, 271, 284, 316, 322, 329, 330, 340, 357, 366, 383, 386, 394-395, 406, 412) — every MCP tool returns `Result<String,String>` and therefore re-borders every error a second time, in words the CLI border already owns.
  - `json.rs:32` `to_json_string` returns `Result<String, String>` — deliberate and small.

- **Two borders for the same errors.** `resolve_gate.rs` and `mcp.rs::tool_resolve_gate` are the same operation; the CLI one adds the `NotPaused` advice (resolve_gate.rs:71-79), the MCP one does not (mcp.rs:412 `.map_err(|e| e.to_string())`). A client answering a gate on a running run gets a strictly worse message than a person does.

**Verdict:** `CliError` is typed per module and the border exists; `Message(String)` + the MCP/test/promote `String` channels are three places where it is bypassed.

---

## 3. RENDER VOCABULARY

### Canonical vocabulary (shared, correct)

| source | vocabulary | consumers |
|---|---|---|
| `render/state.rs:27` `StateWord` (6 words) + `NodeDisplay` | node state: `done/fail/run/wait/skip/todo` short, `finished/failed/running/waiting/skipped/never ran` long | status/mod.rs:69,353; view.rs:186,221,260; closing.rs:157; stats.rs:302,335; graph.rs:87-88; test.rs:320 (`expect.nodes` is written in it) |
| `render/escalation.rs:13,20,31` | `option_headline`, `option_tradeoff`, `evidence` | decision.rs:72,150 (status page + closing trailer), ask/decision.rs:55,72-73 (the prompt) |
| `commands/advice.rs` (107 lines) | every `yunta …` command string, `parked_on`, `parked_in_full` | view.rs:47,58-59; closing.rs:191,294-304; progress.rs:85; status/mod.rs:106; decision.rs:81,192,243; cancel.rs:80,95; receipt.rs:74 |
| `surface/view.rs:169` `closed_as` | run terminal: `finished/failed/cancelled/promoted` | view.rs:154-157 (child rows), lines.rs:148,155 (event lines) |
| `status/mod.rs:430` `task_status_label` | `pending/ready/running/done/blocked/failed` | status text + `StatusJson` + test.rs:330 |

### Places that re-type it

| site | what it re-types |
|---|---|
| `status/progress.rs:79-97` `phase_label` | `RunPhase` → `created/running/waiting — …/finished/failed/cancelled/promoted/broken — …` |
| `surface/closing.rs:157-201` `verdict` | the **same** `RunPhase` → `finished/failed — …/cancelled/promoted to …/paused on a decision/paused — …/broken — …/still moving` |
| `commands/drive.rs:419-425` `RunJson::from_report` | `RunTerminal` → `finished/paused/failed/promoted` |
| `commands/test.rs:77-101` `final_state_label` + `terminal_label` | `FinalState`/`RunTerminal` → `finished/paused/failed/promoted` again, a **fourth** mapping |
| `commands/list/runs.rs:79-85` `Standing::heading` | `RunPhase` → `needs you / in flight / closed` |
| `surface/view.rs:41-49` `demand_line` | `RunPhase` → `nothing needs you` / `needs you: …` |
| `surface/lines.rs:82-165` `detail` | every `EventPayload` → its own sentence (84-line function), independent of all of the above |

### The same fact, spelled differently

**`RunPhase::Waiting` — a run stopped on a person — has four spellings:**

| surface | word | cite |
|---|---|---|
| `yunta status`, `yunta list --runs` summary | **`waiting — node \`x\``** | progress.rs:85 |
| closing block after `run`/`resume` | **`paused — node \`x\``** / `paused on a decision` | closing.rs:187,191 |
| `run --json`, `resume --json` | **`"outcome": "paused"`** | drive.rs:422 |
| live region first row, `list --runs` group heading | **`needs you`** | view.rs:45, runs.rs:81 |

A reader who runs `yunta run`, sees `paused`, then runs `yunta status` and reads `waiting`, has to decide whether those are the same state.

**Second drift — task status:** `status` prints `t1: done` (task_status_label, status/mod.rs:430) while the append-only surface prints `task_status_changed on \`x\` — t1 is Done` (lines.rs:96, `{:?}`). Rust identifiers reach the user in **9 places** in lines.rs (`{:?}` on `message_type`:93, `new_status`:96, `phase`:99,108, `severity`:116,138, `operation`:123, `capability`:151, `artifact_kind`:130) — directly contradicting status/mod.rs:429 *"user output never leaks Rust identifiers"*.

**Third — pluralization.** `commands/mod.rs:296` `counted(n, noun)` exists, documented (mod.rs:286-295) as *"the one place… so no message hedges with `(s)` while the number sits right there"*. It is used 9 times. Against it: **25 live `(s)` strings** (stats.rs:238,343,351,410,417,424,431,438,482; verify.rs:42,88,94; gc.rs:95,100,111,113; cancel.rs:141; run.rs:354; pack.rs:137,154; lines.rs:104) and **3 hand-rolled pluralizers** (ask/form.rs:35 `"answer"/"answers"`, lines.rs:78-80 `counted_problems`, pack.rs:111,140-145 `"" / "s"`).

### Width / indent / duration constants

Single-sourced and clean: `LINE_WIDTH` (core text.rs:26, re-exported width.rs:7), `INDENT` + `indent(depth)` (width.rs:22-28), `LABEL_WIDTH` (width.rs:12), `STATE_WIDTH` (state.rs:17), `format_duration`/`format_pct` (units.rs). No duplicate definitions. Two soft exceptions:
- `ask/menu.rs:42-45` defines its own `MARKER = 2` / `GAP = 2` and builds indents with `" ".repeat(...)` rather than `INDENT`/`indent()` — a second indentation vocabulary for the prompt.
- `schema.rs:57` hardcodes `{:<12}` where `LABEL_WIDTH` is 12.
- `TERM` is read twice, independently: surface/mod.rs:110 and glyphs.rs:57.

---

## 4. SURFACES — assessment

The architecture is the strongest part of this crate.

- **`Feed`** (feed.rs:46-104): sender-only, derives nothing, `try_send` and **drops on full** (feed.rs:80-86) because the frame is a copy of an event the log already holds durably. The drop is recovered, not ignored: `Folded::gap()` → one beat of grace (`stale_gap`, painter.rs:235-241) → `reread()` from storage (painter.rs:246-254). This is the correct reading of "Replay": the surface is explicitly *a cache of one invocation*.
- **`Folded`** (fold.rs, 147 lines): contiguous-prefix + waiting-room by `Seq`. Idempotent, order-insensitive, `refill` never regresses (fold.rs:44-46). Exemplary.
- **`Curtain`/`Standing`** (turns.rs): numbered requests so an acknowledgement of an older stand-down can't be mistaken for this prompt's (turns.rs:29-40) — a real bug class closed by the type. `Curtain::none()` gives callers one path with or without a surface.
- **`Region`** (region.rs): no alternate screen, no raw mode, no spinner (region.rs:1-8, 30-33); rows cut to `min(terminal, LINE_WIDTH)` (region.rs:204) with a test that proves the row count is the row count (region.rs:346-392); `close()` orders clear-then-detach for the right reason (region.rs:143-154).
- **`Painter`** (painter.rs): owns the invocation's whole drawing state, holds diagnostics while stood down rather than dropping them (painter.rs:161-179), and follows a promotion successor derived from the log rather than announced (`succeeds`, painter.rs:348-360).

### Where the Lines surface gets its words vs the region — **two independent derivations, confirmed**

| | append-only `Lines` | pinned `Region` |
|---|---|---|
| input | one `StoredEvent` at a time (painter.rs:143-147 → lines.rs:53) | `run_frame(run_id, workflow, folded.settled(), prior, now)` (painter.rs:219-225) |
| words from | `event.body.kind_name()` + `lines.rs::detail` (84 lines of per-payload `format!`) | `view.rs` over `RunFrame` → `StateWord`/`NodeDisplay`/`advice` |
| shared | only `view::closed_as` (lines.rs:148,155) and `format_duration` | — |
| elapsed | recomputed from the first event's timestamp (lines.rs:54-55) | `NodeFrame::elapsed` from the frame |

Under `Delivery::Lines`, `run_frame` is **never called** (painter.rs:218 `let Draw::Live(region) = draw else { return }`). So a CI log gets no demand line, no counters, no node state words, no children tree, no unknown-kinds note — none of the region's vocabulary. This directly contradicts the module's own headline claim, surface/mod.rs:4-9: *"Three surfaces over one model. Every one of them presents the same RunFrame… there is one derivation and one vocabulary behind all three."* The documentation wins over the code (CLAUDE.md, *La documentación gana*), so this is a defect in the code, not the doc.

### State a surface keeps that the log already answers

- `Scrollback::gone: HashSet<NodeId>` (scrollback.rs:32) — "which nodes have already graduated", mutated in `leaving()` (scrollback.rs:59-74). It is derivable from the folded log: a node's graduations are exactly its `node_finished`/`node_failed` events, and the painter already holds a cursor of that shape (`written`, painter.rs:69). The mutable set forces two compensations that a derivation would not need: the "forget anything currently working first" rule (scrollback.rs:64-66) and a `restart()` plumbed through `Region::restart` → `Painter::follow` for promotion (scrollback.rs:92, region.rs:127, painter.rs:263). **Category: state the log answers.**
- `Painter::written: usize` (painter.rs:67) and `stale_gap` (painter.rs:58) are legitimate single-invocation cursors, not duplicated state.
- `Lines::opened` (lines.rs:29) re-establishes the run's start from the first event this invocation saw — on a `resume` that is the seed's first event (painter.rs:103-111 folds the seed without writing), so elapsed is measured from the run's real start. Correct, but it is a second answer to a question `RunFrame::elapsed` already computes.

---

## 5. PROMPTS (`ask/`)

**Shape.** `Console` (ask/mod.rs:129-299) wraps `dialoguer::console::Term`: keys from stdin, drawing on stderr, stdout left to the command's own output (ask/mod.rs:121-123). `Console::open` (158-178) refuses when stdin is not a TTY, and warns-then-refuses when stderr is not one — degradation is explicit and named, and the caller (`human_interaction.rs:63`) parks the run. `dialoguer` appears in exactly 4 files (keys.rs:22, menu.rs:33, ask/mod.rs:23, surface/mod.rs:31) — no scattered prompt libraries.

**Raw mode.** `read_key_raw` turns the line discipline off per read (ask/mod.rs:202-204). The terminal's handed-over `Termios` is captured once at `Console::open` (ask/mod.rs:307-319) and stored in a process-global `PROMPTED_ON` (ask/mod.rs:62), restored by `main.rs:54` **after** the runtime is handed back — because the abandoned read-thread re-arms raw mode between its own reads (ask/mod.rs:249-258). The cursor put-back is idempotent via an `AtomicBool` swap (ask/mod.rs:280), so the prompt's own `Drop` (menu.rs:184-188) and the process's final restore cannot double-emit a show-cursor sequence. This is careful, correct work.

**Ctrl-C bridging.** Raw mode swallows SIGINT, so `Key::CtrlC` → `Stroke::Interrupt` (keys.rs) → `Console::interrupt()` re-raises `SIGINT` at this process (ask/mod.rs:294-298), which reaches the one bridge `cancel_on_ctrl_c` (commands/mod.rs:53-65) → root `CancellationToken`. One source of truth for "a person stopped the run". `NoAnswer::from(io::Error)` maps `ErrorKind::Interrupted` to `Interrupted` (ask/mod.rs:105-116). The read itself is raced against the token (human_interaction.rs:86-91) so an externally cancelled run stops waiting on a person.

**Where offers/tradeoffs come from.** `engine/src/reserved.rs:81` `mod offers` builds every `GateOption` through one private `offer(label, tradeoff)` constructor (reserved.rs:63-69) — "every option declares what it trades off, by construction". The CLI reads them verbatim: `option_headline` = `"{id} — {label}"`, `option_tradeoff` = `detailed("tradeoff", option.tradeoff)` (escalation.rs:13-22). **The CLI retypes no option label and no tradeoff anywhere outside test fixtures** (grep over `"approve"|"abort"|"retry"|"promote"` finds only ask/decision.rs:89-91 and ask/menu.rs:199 inside `#[cfg(test)]`). The only word the CLI adds is the `tradeoff:` label itself, in one place. This is the frontier done right.

**The gap.** Two commands prompt *outside* `ask/` entirely, with their own vocabulary, on stdout, with no Escape, no Ctrl-C bridge and no terminal restoration:
- `init.rs:269-282` `prompt_line` — `print!` + `stdin().read_line`.
- `new.rs:127-142` `prompt_shape` — `println!` menu numbered by hand + `read_line`, re-implementing `menu::choose`'s numbering (menu.rs:100-126) with different rules.

---

## 6. RUN PATHS

`execute_run` is called from **3** places (drive.rs:105, promote.rs:91, test.rs:279); `create_run` from **2** (run.rs:400, test.rs:260); `spawn_detached_resume` from **4** (detach.rs:129, resolve_gate.rs:51, mcp.rs:364, mcp.rs:414).

| path | resolve+check | adapters | fixture | estimate/budget | create | execute | surface | closing |
|---|---|---|---|---|---|---|---|---|
| `run` attached (attached.rs:39) | `runnable` ✅ | `runnable_adapters` ✅ | `load_mock_fixture` (attached.rs:70) | `estimate` ✅ | `create_run_from` ✅ | `drive` ✅ | ✅ | ✅ |
| `run --detach` (detach.rs:45) | `runnable` ✅ | `runnable_adapters` ✅ | refused (run.rs:158) | `estimate` ✅ | `create_and_detach` ✅ | child | none | `println!` |
| `mcp run_workflow` (detach.rs:81 `start_detached`) | `runnable` ✅ | ✅ | — | `estimate(quiet=true)` ✅ | shared ✅ | child | none | `format!` |
| `resume` (resume.rs:67) | — (frozen manifest) | `real_adapters` **direct** (resume.rs:50) | — | none (§8.6) | — | `drive` ✅ | ✅ | ✅ |
| `mcp resume_run` (mcp.rs:352) | — | — | — | — | — | detached child | — | own string |
| `resolve-gate` (resolve_gate.rs:20) | — | — | — | — | — | detached child | — | `println!` |
| `mcp resolve_gate` (mcp.rs:372) | — | — | — | — | — | detached child | — | own strings |
| `test` (test.rs:206) | **own** path join (test.rs:210), **no `check_or_refuse`** | `mock_adapters` | `load_mock_fixture` ✅ | — | **own** `build_manifest`+`create_run` (247,260) | **own** `execute_run` (279) | none | own compare |
| promotion successor (promote.rs:58) | — | inherited | — | — | `create_promotion_successor` | **own** `execute_run` (91) | same observer ✅ | caller's |

**What `run`/`resume` share and do well.** `run.rs::runnable` (187-201) is the single pre-flight (resolve → check → refuse-unrunnable → validate `--adapter` → probe), and `drive.rs::Driving`/`drive`/`finish`/`settle` (drive.rs:41-318) is the single tail, so the two commands genuinely cannot report a stop differently. `Prepared` is the one shape both produce (drive.rs:25-29). This is the right design.

**What the outliers duplicate.**
1. **`test.rs::run_case`** (132 lines, test.rs:206-337) is a fourth run path: it re-resolves the workflow by hand (`cwd/.yunta/workflows/{name}.yaml`, test.rs:210-212 — no catalog, no pack resolution, unlike `resolve_workflow_ref`), **skips `check_or_refuse` entirely**, rebuilds manifest+run+execute inline, and uses `SystemClock`/`SystemIdSource` *directly* (test.rs:236, 273, 286, 288) instead of the injected `ctx.clock`/`ctx.ids` — even though it builds a `Context` two lines earlier (test.rs:216) and throws away everything but `.project.config`. A workflow that `yunta check` rejects can still pass `yunta test`.
2. **`promote.rs`** uses `&SystemClock` at promote.rs:85 and 98 rather than the injected clock that `drive.rs:112` hands `execute_run` — the same invocation stamps its first run from `ctx.clock` and its successor from a clock built on the spot. `PromotionEnv` carries `ids` (promote.rs:30) but not `clock`.
3. **`mcp.rs::tool_resolve_gate`** (54 lines, mcp.rs:372-425) is a line-by-line re-implementation of `resolve_gate.rs` (60 lines) — run_dir lookup, manifest read (by hand, not `load_yaml`), storage open, `yunta_engine::resolve_gate`, `spawn_detached_resume`, final sentence (`"run {id}: resolved \`{option}\`, driving forward independently"`, identical at resolve_gate.rs:59 and mcp.rs:422) — minus the `NotPaused` advice, and with `&yunta_core::SystemClock` (mcp.rs:402) instead of `ctx.clock`.
4. **`mcp.rs::tool_workflow_status`** (mcp.rs:261-294) re-implements `status`'s loading half; only the DTO (`status_json`) is shared.
5. **The "run every case" loop** is written twice: test.rs:136-157 and pack_audit.rs:173-190.
6. `real_adapters` is reached three ways: `ctx.adapters()` (context.rs:64, used only by doctor.rs:30), `runnable_adapters` (run.rs:215), and directly in resume.rs:50.
7. The sentence naming which adapters exist is duplicated prose: commands/mod.rs:228 and doctor.rs:34-35 both hardcode *"only `claude-code` and `codex` are built"* instead of deriving it from the registry.

---

## 7. OUTCOMES / EXIT CODES

**One mapping at the exit** (main.rs:56-63): `Success`→0, `Reported`→1, `Err`→1 with the banner. No command calls `std::process::exit`. `Outcome` is a 2-variant enum with a well-argued doc (error.rs:11-19).

**Two mappings feeding it for the same run**, over two different enums:
- `drive::verdict(&RunReport)` → `RunTerminal::Finished` ⇒ Success (drive.rs:377-382) — used by `--quiet` and `--json`.
- `Closing::outcome()` → `RunPhase::Finished` ⇒ Success (closing.rs:94-99) — used by the human path.

They agree today only because someone keeps them in step; a `RunPhase::Finished`-with-blocking-findings run reports `Reported`… no — closing.rs:159 renders *"finished, holding N blocking findings"* but `outcome()` still matches `RunPhase::Finished` ⇒ **Success**. So a run holding blocking findings exits 0 while its own closing line says nobody has accepted the work. That is a judgement call, but it is made in a different place from the line that describes it.

Ad-hoc combinations elsewhere: verify.rs:32-35, doctor.rs:70-74, test.rs:163-167, check.rs:73-79, status/decision path.

**`--json` documents.** `json.rs:17` defines one `SCHEMA_VERSION: u32 = 3` covering every document, documented as re-stamping all of them on a bump (json.rs:10-16). Four DTOs carry it:

| document | type | shared with |
|---|---|---|
| `run --json`, `resume --json` | `RunJson` (drive.rs:388-406) | `RunJson::detached` for `--detach`; MCP returns plain text instead |
| `status --json` | `StatusJson` (status/mod.rs:163-192) | ✅ MCP `workflow_status` (mcp.rs:288) — one DTO, two doors |
| `stats <run> --json` | `RunStatsJson` (stats.rs:~520) | — |
| `stats --workflow --json` | `WorkflowHistoryJson` | — |
| `receipt --json` | engine-rendered, **no `schema_version` at all** (receipt.rs:26, no field found in `engine/src/receipt/`) | — |

So: **not one document type shared by run/resume/status** — `run`/`resume` share `RunJson`, `status` has `StatusJson`, and they overlap in what they report (`RunJson.outcome: "paused"` vs `StatusJson.summary: "… waiting — node x"` vs `StatusJson.waiting_on`) with different words (§3). `WaitingOnJson` (status/mod.rs:202) is a good tagged shape a program can act on; `RunJson` gives a program only `outcome`+`reason` free text for the same state. And the receipt is a fifth machine document outside the version scheme.

---

## 8. CONTEXT

`Context::load` (context.rs:31-34) reads cwd once and calls `resolve_in`; `resolve_in` (context.rs:40-48) is documented as *"The single place `project::resolve` is called"* — true. `Context` carries `cwd`, `project`, `clock: SystemClock`, `ids: SystemIdSource`, and opens storage / adapters on demand (context.rs:52-66). Config layering is in project.rs: org → user → repo (`load_named_layers`, project.rs:151-166), merged most-specific-last (project.rs:178-179), with `runs_root` / `worktrees_root` / `storage_path` each `config.paths.* || user_root/<default>` (project.rs:181-195) and the state dir created on first use (199-204). `CONFIG_VERSION` refusal at project.rs:136-144 is correct degradation.

**Where it is not one place.**

| read | sites |
|---|---|
| `std::env::current_dir()` | **10** — context.rs:32 (the sanctioned one), plus check.rs:20, list/mod.rs:37, init.rs:285, new.rs:193, test.rs:177, mcp.rs:71, pack.rs:174/287/354/373/493 (5 in pack.rs), pack_audit.rs:22, **and commands/mod.rs:339 inside `check_or_refuse`**, which re-reads cwd behind the back of the `ctx.cwd` its callers already resolved |
| `HOME` | project.rs:96 (`process_env`) **and again** project.rs:129 inside `load_layer` — despite project.rs:89-93 stating the environment is *"read once here at the CLI's boundary so no code below reads it live"* |
| `YUNTA_HOME` / `YUNTA_ORG_CONFIG` | project.rs:97-98 only ✅ — but `process_env()` is *called* three times per resolve (project.rs:104, 108, and drive.rs:102 / promote.rs:90) |
| `TERM` | surface/mod.rs:110 **and** glyphs.rs:57 |
| `NO_COLOR` | surface/mod.rs:111 |
| `YUNTA_GLYPHS`, `LC_ALL`/`LC_CTYPE`/`LANG` | glyphs.rs:53-57 |
| `USER` | identity.rs:16 |
| `PATH` | doctor.rs:21 |
| `<forge>.token_env` | commands/mod.rs:202 |
| `current_exe` | commands/mod.rs:120 |

The *policy* objects are exemplary — `TerminalEnv` (surface/mod.rs:95-114) and `GlyphEnv` (glyphs.rs:41-59) both lift the values out of the process so `Delivery::choose` and `Glyphs::select` are pure functions of their arguments, each with tests (surface/mod.rs:467-494). The problem is only that those two structs plus `process_env()` plus `identity::responder` plus `doctor` are five separate boundaries, not one.

`check.rs` is the sharpest case: it resolves `cwd` itself (check.rs:20), builds its own layer stack (check.rs:32-41), and *then* calls `Context::load()` again at check.rs:61 for the history — two independent resolutions of the same project in one command.

---

## 9. DEFECTS

| # | defect | category | evidence |
|---|---|---|---|
| D1 | No `open_run` door: 6 copies of the run_dir-with-fallback, 5 of the `else`-refusal, 7+2 manifest reads, 10 spellings of "no run" in 3 sentences | duplicated path | §1 table; status/mod.rs:31-47, stats.rs:50-65, receipt.rs:49-62, cancel.rs:39-64, resolve_gate.rs:27-33, resume.rs:41-49, verify.rs:60, graph.rs:70-77, list/runs.rs:225-251, mcp.rs:266-284/358/387, drive.rs:299-310 |
| D2 | `stats::collect_history`/`collect_raw_history` bypass `Project::run_dir` — a run under the default state root is invisible to every history, estimation and budget warning | duplicated path + silent degradation | stats.rs:142, stats.rs:194 vs project.rs:32 |
| D3 | `collect_history` and `collect_raw_history` are the same 40-line walk twice, differing in what they keep | duplicated path | stats.rs:125-164 vs stats.rs:176-214 |
| D4 | A run stopped on a person is `waiting` / `paused` / `needs you` / `"paused"` depending on the surface | retyped vocabulary | progress.rs:85 vs closing.rs:187,191 vs view.rs:45, runs.rs:81 vs drive.rs:422 |
| D5 | `RunPhase`→words mapped 3× and `RunTerminal`→words 2× more, by hand | retyped vocabulary | progress.rs:79-97, closing.rs:157-201, runs.rs:65-85, drive.rs:419-425, test.rs:77-101 |
| D6 | Rust identifiers reach users through `{:?}` on 9 payload enums; `Done` vs `done` for the same task status | text at wrong layer | lines.rs:93,96,99,108,116,123,130,138,151 vs status/mod.rs:430 |
| D7 | `counted()` is the documented single place, yet 25 `(s)` strings and 3 hand-rolled pluralizers remain | Un lugar | mod.rs:286-302 vs §3 list |
| D8 | The append-only surface derives its own words per event; the region derives from `RunFrame`. Under `Delivery::Lines`, `run_frame` is never called — no demand line, counters, state words or children in a CI log | two derivations + doc/code contradiction | lines.rs:82-165 vs view.rs; painter.rs:218; contradicts surface/mod.rs:4-9 |
| D9 | `Scrollback::gone` keeps mutable "already graduated" state the folded log answers, forcing the forget-working rule and a `restart()` chain | state the log answers | scrollback.rs:32,59-74,92; region.rs:127; painter.rs:263 |
| D10 | `mcp::tool_resolve_gate` re-implements `resolve_gate.rs` verbatim, minus the `NotPaused` advice, with its own `SystemClock` | duplicated path + layering | mcp.rs:372-425 vs resolve_gate.rs:20-79 |
| D11 | `mcp` re-borders every error: 17 `map_err(to_string)`/`format!` sites returning `Result<String,String>` past the `CliError` border | text at wrong layer | mcp.rs:130,246,266-271,284,316,322,329-340,357-366,383-412 |
| D12 | `promote.rs` flattens two typed engine errors to `String` (re-wrapped as `CliError::msg` at drive.rs:219) and stamps the successor with `&SystemClock` instead of the injected clock | layering + Núcleo puro | promote.rs:85,89,98,109 vs drive.rs:112 |
| D13 | `test::run_case` is a fourth run path: own workflow resolution (no catalog/packs), **no `check_or_refuse`**, own manifest+create+execute, own `SystemClock`/`SystemIdSource` despite holding a `Context` | duplicated path + Núcleo puro | test.rs:206-337, esp. 210-219, 236, 273, 286-288 |
| D14 | `init` and `new` prompt through `stdin().read_line` on stdout — a second prompt vocabulary with no Escape, no Ctrl-C bridge, no terminal restore, re-numbering a menu `ask/menu.rs` already numbers | duplicated path + layering | init.rs:269-282, new.rs:127-142 vs ask/mod.rs, ask/menu.rs:100-126 |
| D15 | `check_or_refuse` re-reads `current_dir()` inside itself, behind `ctx.cwd`; `check.rs` resolves the project twice; `load_layer` re-reads `HOME` the module doc says is read once | Un lugar / Frontera | mod.rs:339, check.rs:20+61, project.rs:89-93 vs 129 |
| D16 | Two exit-code mappings for one run; a run "finished, holding N blocking findings" exits 0 | two derivations | drive.rs:377 vs closing.rs:94-99, 159-165 |
| D17 | `receipt --json` emits a machine document with no `schema_version`, outside the one-version scheme `json.rs` declares | degradation / contract | receipt.rs:26 vs json.rs:10-17 |
| D18 | `--quiet` prints `run {id}: created at {path}` / `resuming at {path}`, not "the run id and nothing else" as the help and README promise | expression / doc-code drift | attached.rs:60-66, resume.rs:76-78 vs cli.rs:62-66, README.md:161 |
| D19 | README promises `yunta list` shows "modes" (never printed) and spells `yunta graph <workflow\|run_id>` (the CLI takes `<workflow> [--run <id>]`) | expression / doc-code drift | README.md:162,167 vs list/mod.rs:98-131, cli.rs:156-165 |
| D20 | Two hand-written "has this run a terminal event" scans beside `RunPhase`; a third counter in `run.rs` | duplicated path | cancel.rs:49-54, gc.rs:68-74, run.rs:83-88 |
| D21 | "only `claude-code` and `codex` are built" written as prose in two places instead of derived from the registry | Un lugar / Frontera | mod.rs:228, doctor.rs:34-35 |
| D22 | `stats.rs` renders with 25 inline `println!`s (no render-to-String), so nothing in it is assertable; it is also the largest file | size / layering | stats.rs 754 lines, 243-443 |
| D23 | Size signals: stats.rs 754, pack.rs 548, surface/mod.rs 495, painter.rs 479, test.rs 475, region.rs 466. Functions ≥48 lines: `test::run_case` 132, `init` 109, `cancel` 110, `pack::add` 99, `mcp::tool_definitions` 97, `cli::dispatch` 93, `run::create_run_from` 88, `lines::detail` 84, `gc` 84 | size | §measured |
| D24 | `graph.rs` opens `Storage` itself rather than through `ctx.storage()` | layering | graph.rs:70 vs context.rs:58 |
| D25 | `ask/menu.rs` defines its own `MARKER`/`GAP` indentation instead of `INDENT`/`indent()`; `schema.rs` hardcodes `{:<12}` where `LABEL_WIDTH` is 12 | Un lugar | menu.rs:42-45,102, schema.rs:57 vs width.rs:12-28 |
| D26 | `Console::open`'s warnings are emitted before `curtain.lower()`, bypassing `Diagnostics` | ordering (currently unreachable: `Live` requires stderr to be a terminal, which those warnings require it not to be) | ask/mod.rs:164,311 vs human_interaction.rs:63-64, region.rs:10-15 |

---

## 10. IDEAL — the greenfield shape

**Four doors, and nothing else writes for a person.**

1. **One door onto a run.** `Opened { run_id, run_dir, manifest, events, project }`, built by one `Context::open_run(&RunId)` that does the search (`Project::run_dir`), the refusal (one sentence, one place, naming both roots), the manifest read, and the log read. Every one of `status`, `stats`, `receipt`, `verify`, `cancel`, `gc`, `graph`, `resume`, `resolve-gate`, `list --runs`, `drive::settle` and all four MCP tools takes it. `collect_history` walks it too, which closes D2. Kills D1, D2, D3, D20.
2. **One vocabulary, one crate module.** `render/state.rs` already is it for nodes; extend it with the run-level word — `RunPhase → RunWord` in exactly one function, consumed by `progress::summary`, `Closing::verdict`, `Standing::heading`, `RunJson.outcome` and the `demand_line`. Whatever it is called (`paused` reads better than `waiting` for a stopped run, and `needs you` is a framing, not a synonym), it is one word with one spelling, and `--json` emits the same token the text prints. Kills D4, D5.
3. **One border for text.** `CliError` keeps its typed arms; `Message(String)` narrows to genuinely one-off conditions, and every recurring sentence gets a typed arm or an `advice`-style function. The MCP tools return `Result<T, CliError>` and are rendered to a tool result at one adapter — the same border, a different medium. `promote` and `test` return typed errors. Kills D10, D11, D12, part of D13.
4. **Surfaces are layouts over engine-derived facts, never derivations.** `Lines` takes the same `RunFrame` the region takes, and renders *the delta between frames* as lines — so a CI log and a terminal say the same things in the same words, and `lines::detail`'s 84 lines of per-payload prose collapses into the event-kind name plus the frame's own vocabulary. `Scrollback::gone` becomes a high-water seq over the folded log. Kills D6, D8, D9.
5. **One environment boundary.** `Env` (already in core) is read once in `main`, and `TerminalEnv`, `GlyphEnv`, `identity`, `doctor`'s PATH lookup and `process_env` all take it. `cwd` comes from `Context` and from nowhere else — `check_or_refuse` takes `&Path`. Kills D15, part of D21.
6. **One prompt surface.** `init` and `new` ask through `ask::choose`/`ask::ask_line` on a `Console`. Kills D14.
7. **One document version scheme.** The receipt JSON carries `schema_version` like the other four, and `run`/`resume`/`status` emit one `RunDocument` with the run-level word, the tagged `waiting_on`, and the optional `decision` — `StatusJson` is that document with detail, not a different one. Kills D17, part of D7's cousin in D16.

### What is already right and must survive the refactor

| piece | judgement |
|---|---|
| **`Delivery::choose`** (surface/mod.rs:116-145) | **Keep as is.** A pure function of a lifted `TerminalEnv`, three signals in reader order, every downgrade carrying the reason that travels to the reader's first line (lines.rs:35-41), tested for all four branches including the empty-`NO_COLOR` case. This is the model the rest of the crate's env reads should copy. |
| **`Curtain` / `Standing` / `Standby`** (turns.rs) | **Keep.** Numbered requests make stale-acknowledgement unrepresentable; `Curtain::none()` gives one path with or without a surface; `Turn`'s `Drop` (human_interaction.rs:119-125) guarantees the region comes back however the prompt ends. |
| **`Feed` drop policy** (feed.rs:78-88 + painter.rs:235-254) | **Keep.** The only correct reading of "the log is the truth, the surface is one invocation's cache": drop the frame, notice the hole, go back to the log. Do not add backpressure. |
| **`Folded`** (fold.rs) | **Keep unchanged.** Idempotent, order-insensitive, monotone. |
| **`render::state`** (`StateWord`/`NodeDisplay`) | **Keep and extend.** Six words, short and long forms, reused by status, region, closing, stats, graph and the test harness's `expect.nodes` — the one vocabulary that already has no second copy. It is the template for the run-level word item (2) asks for. |
| **`commands::advice`** (advice.rs) | **Keep.** Every `yunta …` string spelled once; renaming a subcommand is one file. `parked_on`/`parked_in_full` correctly split "what a row has room for" from "what a page has room for". |
| **`decision::Layout`** (status/decision.rs:44-177) | **Keep.** One block, two layouts, with the parts decided once and only the room varying — and a test that asserts the trailer is nine parts on nine lines (decision.rs:306-325). Exactly the "layout over facts" shape the whole render layer should have. |
| **`render::escalation` + engine `reserved::offers`** | **Keep.** The frontier is clean: the engine owns the words of a decision, the CLI owns only the shape. Nothing to change. |
| **`run::runnable` + `drive::Driving`/`settle`** | **Keep, and make it the only run path.** `test.rs` and `promote.rs` should enter through it (or through a shared `execute` that takes the injected clock), which is the concrete fix for D12 and D13. |
| **`json::SCHEMA_VERSION`** (json.rs:10-17) | **Keep the one-number-for-all rule**, and bring the receipt inside it. |
| **`error::{warn, note}` + `Diagnostics`** (error.rs:100-110, feed.rs:118-144) | **Keep.** `Diagnostics::raise` falling back to `note` when no painter takes it is the right degradation, and it is what makes the Ctrl-C line land above the region instead of inside it. |
| **`Region`'s no-raw-mode, no-spinner, age-not-animation rule** (region.rs:1-8,30-33) | **Keep.** It is why scrollback, selection and a typed Ctrl-C all still work, and why every row reads with glyphs stripped. |
