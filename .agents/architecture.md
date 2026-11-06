# Architecture

How Yunta's crates fit and what every run guarantees; a crate's own map is its root `//!`.

## A run

`yunta` reads the config layers and the workflow, and `check` refuses what cannot run. The run loop schedules
nodes, and a task's cycle opens sessions through the port on an adapter, each session in a pooled checkout. The
fence and the rail judge each write at the tool call, and the close judges the diff. Declared criteria the
engine runs decide a task; an agent's word is never evidence. Every fact is an event in the log, replay derives
every state from it, and what a person reads is drawn from that data in Surco.

## Crates

| Crate | Owns | Depends on (`[dependencies]`) |
|---|---|---|
| `yunta-surco` | Surco, the language: its words, marks and figures, a `Doc` that a `Surface` draws, one lexer for code | nothing in the workspace |
| `yunta-core` | domain types, the port, shapes, protection | surco |
| `yunta-storage` | the append-only log; SQLite stays behind its interface | core |
| `yunta-render` | Yunta's own facts and documents, said in Surco | core, surco |
| `yunta-adapters` | `claude-code`, `codex` and `mock` behind the port | core, render, surco |
| `yunta-engine` | scheduling, the task cycle, replay | core, storage, surco |
| `yunta` | the commands | core, storage, engine, adapters, render, surco |
| `yunta-testkit-core` | test scaffolding for the crates below the engine | core |
| `yunta-testkit` | the whole-run test harness | core, testkit-core, storage, adapters, engine, surco |
| `xtask` | smells, the docs checker, schemas | core, surco |

`yunta-engine`'s `[dependencies]` never name `yunta-adapters` or `yunta-render`; its tests may.
`cargo xtask docs --check` holds this table to every member's manifest.

## Product rules

- The engine derives data and never draws; the CLI and the adapters draw.
  [test engine/no_adapter_crate_in_engine::the_engine_library_never_links_the_crate_that_draws]
- What a person reads is a `Doc` a `Surface` draws; new output grows Surco as a principle, a token or a
  component in `crates/surco`, and what Yunta says with it stands in `crates/render`. [smell escape_outside_paint]
  [smell glyph_literal_outside_glyphs] [smell state_phrase_outside_words] [smell look_inside_a_block] [review]
  Paths: `crates/cli/src/**`, `crates/adapters/src/**`, `crates/render/src/**`, `crates/surco/src/**`
- A richer layer adds no fact: every layer of a stream writes the same text, so a person without color reads
  all of it. [review]
  Paths: `crates/surco/src/paint.rs`, `crates/surco/src/surface/**`
- A command a person is told to type is spelled in one module. [smell command_outside_advice]
- A printed sentence carries no dash: a colon explains, a semicolon comes before the hint it ends with.
  [smell dash_in_prod_strings]
- Every adapter (`claude-code`, `codex`, `mock`) offers the same features; a gap is a defect.
  [test adapters/contract]
- A task session's brief names its task, and its contract (notes, scope, criteria) is read through the run's
  tools, never copied into the prompt; what else a node reads, its workflow declares.
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
- One answer pays for one engine: `continue` lifts the token cap only in the engine that took it, and the next
  one stands under the cap again. [test engine/parked_answers]
- A follower only reads: it takes no lock, writes no event and never signals the engine it follows.
  [test cli/status_follow]
- What a session says while it works reaches another process only through its run's socket, redacted: the
  log keeps each note's digest, nothing writes the words to disk, and only a terminal draws them. The summary
  a session ends with is its node's outcome, logged as ever. [test engine/run_words] [test cli/run_words]
- A safety mechanism adds no wall-clock time to a run. [review]
  Paths: `crates/core/src/rail/**`, `crates/engine/src/task_cycle/**`, `crates/engine/src/worktree/**`,
  `crates/adapters/src/**`

## Cross-cutting

- Engine decisions are deterministic and each effect has one owner: the clock, the environment, the filesystem
  and git are reached through their boundary modules. [smell system_clock_outside_boundary]
  [smell env_read_outside_boundary] [smell engine_fs_outside_runtime] [smell git_command_outside_git_rs]
- Replay is a pure fold of the log: two logs with the same facts derive the same state. [test engine/properties]
- A log an earlier binary wrote still replays. [test engine/logs_from_runs] [test engine/older_words]
- No session writes what an agent CLI loads as configuration, at its own path or through a link the base commit
  holds; a person may grant the instruction files. [test engine/paths_deny] [test adapters/own_config]
