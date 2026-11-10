# Architecture

How Yunta's crates fit and what every run guarantees; a crate's map is its root `//!`.

## A run

An interface asks `yunta-api` for a run; `check` refuses what cannot run. The
run loop schedules nodes, and a task's cycle opens sessions through the port on an adapter, each in a pooled
checkout. The fence and the rail judge each write at the tool call, and the close judges the diff. Declared
criteria the engine runs decide a task; an agent's word is never evidence. Every fact is an event in the log,
replay derives every state from it, and what a person reads is drawn from it in Surco.

## Crates

| Crate | Owns | Depends on (`[dependencies]`) |
|---|---|---|
| `yunta-surco` | Surco: words, marks, figures, the `Doc` a `Surface` draws, a code lexer | nothing in the workspace |
| `yunta-core` | domain types, the port, shapes, protection | surco |
| `yunta-storage` | the append-only log, SQLite behind its interface | core |
| `yunta-replay` | replay, every reading of a run, no IO | core, surco |
| `yunta-render` | Yunta's facts and documents, said in Surco | core, replay, surco |
| `yunta-adapters` | `claude-code`, `codex` and `mock` behind the port | core, render, surco |
| `yunta-mcp-protocol` | MCP as Yunta speaks it | nothing in the workspace |
| `yunta-engine` | scheduling, the task cycle | core, storage, replay, mcp-protocol, surco |
| `yunta-api` | what interfaces ask of Yunta | core, storage, replay, engine, adapters, render, surco |
| `yunta-mcp` | the control plane an agent asks Yunta through | core, render, api, mcp-protocol, surco |
| `yunta-ui` | the interface apart from its medium | core, replay, render, api, surco |
| `yunta` | the commands | core, replay, render, api, mcp, ui, surco |
| `yunta-testkit-core` | test scaffolding below the engine | core |
| `yunta-testkit` | whole-run test harness | core, testkit-core, storage, adapters, engine, surco |
| `xtask` | smells, the docs checker, schemas | core, surco |

`cargo xtask docs --check` holds it to the manifests.

## Product rules

- The engine derives data and never draws.
  [test engine/no_adapter_crate_in_engine::the_engine_library_never_links_the_crate_that_draws]
- What a person reads is a `Doc` a `Surface` draws; new output grows Surco as a principle, a token or a
  component in `crates/surco`, and what Yunta says with it stands in `crates/render`. [smell escape_outside_paint]
  [smell glyph_literal_outside_glyphs] [smell state_phrase_outside_words] [smell look_inside_a_block] [review]
  Paths: `crates/cli/src/**`, `crates/ui/src/**`, `crates/adapters/src/**`, `crates/render/src/**`,
  `crates/surco/src/**`
- A richer layer adds no fact: every layer of a stream writes the same text, so a person without color reads
  all of it. [review]
  Paths: `crates/surco/src/paint.rs`, `crates/surco/src/surface/**`
- A command a reader is told to type is spelled once. [smell command_outside_command_rs]
- A printed sentence carries no dash: a colon explains, a semicolon comes before the hint it ends with.
  [smell dash_in_prod_strings]
- Every adapter (`claude-code`, `codex`, `mock`) offers the same features; a gap is a defect.
  [test adapters/contract]
- A task session's brief names its task; its contract (notes, scope, criteria) is read through the run's tools,
  never copied into the prompt, and what else a node reads its workflow declares.
  [test engine/run_sessions::a_task_session_reads_its_task_through_its_tools_and_its_check_is_the_close_s]
- Nothing retries unless an input the failure depends on changed. [test engine/check_retry]
- A run stops for a person only for intent, an approval they chose or a fix only they can make; mechanical
  failures go back to the agent, the planner or a wake. [review]
  Paths: `crates/core/src/events/run/**`, `crates/core/src/expansion.rs`, `crates/engine/src/run/escalation.rs`,
  `crates/engine/src/run/escalation/**`, `crates/engine/src/run/*gate*.rs`, `crates/engine/src/run/asking.rs`
- The spec decides what proves a task: a plan or spec that cannot prove the work is refused at handover, never
  in `check()`, and its gate withholds going on. [test engine/spec_governs_gate]
- Modes belong to workflows: the engine's code never names one. [smell mode_name_in_prod]
- A run has one engine: `RunEnv` borrows a hold only `hold_run` makes; a second resume is refused and a dead
  holder replaced. [test engine/run_guard]
- One answer pays for one engine: `continue` lifts the token cap only in the engine that took it.
  [test engine/parked_answers]
- A follower only reads: it takes no lock, writes no event, signals no engine.
  [test cli/status_follow]
- What a session says while it works reaches another process only through its run's socket, redacted: the
  log keeps each note's digest and no words, and only a terminal draws them; the summary a session ends with is
  its node's outcome. [test engine/run_words] [test cli/run_words]
- A safety mechanism adds no wall-clock time to a run. [review]
  Paths: `crates/core/src/rail/**`, `crates/engine/src/task_cycle/**`, `crates/engine/src/worktree/**`,
  `crates/adapters/src/**`

## Cross-cutting

- Engine decisions are deterministic and each effect has one owner: the clock, the environment, the filesystem
  and git are reached through their boundaries. [smell system_clock_outside_boundary]
  [smell env_read_outside_boundary] [smell engine_fs_outside_runtime] [smell git_command_outside_git_rs]
- Replay is a pure fold of the log: two logs with the same facts derive the same state. [test replay/properties]
- A log an earlier binary wrote still replays. [test replay/logs_from_runs] [test render/older_words]
- No session writes what an agent CLI loads as configuration, at its own path or through a link the base commit
  holds; a person may grant the instruction files. [test engine/paths_deny] [test adapters/own_config]
- Nothing a session's CLI started outlives its session, linked to it or carrying its mark, nor a dead engine's
  `yunta cancel` or `yunta close`.
  [test core/process_tree] [test core/process_mark] [test adapters/contract] [test cli/session_ends]
