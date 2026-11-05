# Capability map: Yunta with a TUI, the CLI and a desktop app

Status: **approved on 2026-11-04**. Each module has its own `SPEC-<id>.md` beside this file, approved before it is planned.
Base: `main` at `34790ab8`.

## Decided

- Bare `yunta` opens the application when stdin and stdout are terminals and `TERM` is not `dumb`. In any other case it prints what it prints at the base commit.
- An agent's words reach other processes through one Unix socket per run, redacted and with nothing on disk. Never through the log.
- Ratatui, with the crossterm backend only. `foldhash` enters through a per-crate exception in `deny.toml`; `itertools` takes a `skip`.
- Surco is the language of everything shown.
- The application never hosts a run: it starts one as a separate process and follows it by reading the log.

## Structure

```
crates/
  surco     yunta-surco     the language; adds Ribbon, Legend, Overlay, Attention, Screen
  core      yunta-core      no change of responsibility
  storage   yunta-storage   adds reading from a sequence, and the head of each run
  state     yunta-state     NEW: what happened. Replay and the read model, pure, no IO
  render    yunta-render    adds every page, since it can read yunta-state
  adapters  yunta-adapters  no change
  engine    yunta-engine    adds the resume guard, parked answers, the words socket
  control   yunta-control   NEW: what can be asked of Yunta, with no terminal
  app       yunta-app       NEW: the application as a value: state, stages, screens, effects
  tui       yunta-tui       NEW: the only crate that knows Ratatui and crossterm
  cli       yunta           the binary: clap, output, attached view, inline prompts, json, mcp, rail
  desktop   (later)         Tauri over yunta-app, with Surco's Html surface
```

New or changed dependencies:

| Crate | Depends on |
|---|---|
| `yunta-state` | core, surco |
| `yunta-render` | core, state, surco |
| `yunta-engine` | core, storage, state, surco |
| `yunta-control` | core, storage, state, engine, adapters, render, surco |
| `yunta-app` | core, state, render, control, surco |
| `yunta-tui` | app, surco, ratatui, crossterm |
| `yunta` | all of the above |
| desktop | app, surco |

What moves:

| What | To | Note |
|---|---|---|
| `replay`, `view`, `live`, `stats`, `history`, `receipt`, `verification_effectiveness`, `modes`, the value types `Engine`, `EngineLiveness`, `Prompt` | `yunta-state` | About 2,650 lines. They import only core, surco and each other. Reading `engine.json` stays in the engine |
| `run_page`, inbox pages, status, stats, doctor | `yunta-render` | They take data; `exit` and disk paths stay outside |
| The `after_for` table and the spelling of commands | `after_for` to `yunta-state`, spelling to `yunta-render` | One table: the CLI spells it, the TUI binds keys, the desktop shows buttons |
| Gathering the inbox, run admission, resolve, cancel, close, gc, packs, doctor's checks, detection, the test runner | `yunta-control` | They return data and never print |
| The models of `card`, `menu`, `form`, `field`, `desk`, `Stroke`, `Press` | `yunta-app` | `hold`, `Console`, `keyboard` and the live region stay in `yunta` |
| `json/` | stays in `yunta` | Only the CLI and MCP use it; the desktop links the library |

## Rules that prevent rework

| Rule | What holds it |
|---|---|
| No interface calls the engine, only `yunta-control` | A test over the dependencies, like `no_adapter_crate_in_engine` |
| `yunta-app` does not know a terminal | The same kind of test |
| `yunta-state` is pure | Its manifest lists neither tokio nor rusqlite, and a test holds that |
| `update` is pure and effects are data | A test walks every reachable state |
| The desktop stays possible without building it | That walk also draws each screen with `Html` |
| Nothing below the interface reads the process | `yunta-control` takes a `Workspace`: path, environment, clock and engine binary |
| A color has one table | `Paint` and the bridge to Ratatui read the same Surco function |
| Moving code does not change what is printed | The existing goldens are not touched during a move |

## Modules

| Id | What it delivers | Depends on |
|---|---|---|
| `run-guard` | `resume` refuses while an engine is alive, and macOS detects a reused pid | nothing |
| `parked-answers` | A run stopped by the token cap can be continued from another process | nothing |
| `log-follow` | `events_after`, the head of each run, and following a run from another process | nothing |
| `run-words` | The per-run socket, in every adapter at once | nothing |
| `surco-screen` | Public reading of styles and the new components, with their goldens | nothing |
| `run-state` | The `yunta-state` crate, cutting the cycle between `replay` and `stats` | nothing |
| `run-pages` | The pages in `yunta-render`, with the CLI printing exactly the same | `run-state` |
| `control` | `yunta-control`, with the CLI and MCP mounted on it | `log-follow`, `run-state` |
| `app-core` | `yunta-app`: stages, screens, effects and moved prompts | `control`, `run-pages`, `surco-screen` |
| `tui` | `yunta-tui`, bare `yunta` and `deny.toml`, in slices: watch, decide, start | `app-core`, `parked-answers` |
| `third-party-notices` | Third-party license notices, included when the binary is distributed | nothing |
| `desktop` | Reserved only: no spec and no code yet | `app-core` |

**Order:** `run-guard`, `parked-answers`, `log-follow`, `run-words`, `surco-screen`, `run-state` and `third-party-notices`, which do not depend on each other → `run-pages` and `control` → `app-core` → `tui`.

**How each module is built:** spec, plan, tasks, implementation, in that order and each with its own approval. The next module starts when this one is closed.

**Delivery:** one branch, `feature/yunta-tui`, in the worktree `/Users/eulke/development/yunta-tui`; one commit per module, with the suite green; one pull request against `main`, when the last module ends.

`tui` does not wait for `run-words`: the words appear when the socket exists, with no change to the screen. Only the "decide" slice needs `parked-answers`.

### `parked-answers`: what it has to respect

- At the base commit nothing answers from another process: `run/budget.rs` asks directly, and `current_decision` (`run/escalation.rs:249`) offers only three kinds of decision. The page admits it: "no option answers this pause here".
- Authorization per invocation is deliberate (`run/budget.rs:8`, test `budget_authorization_is_per_invocation_a_resume_asks_again`): each invocation spends new money. It is kept: **a recorded answer authorizes only the next invocation**. With no answer waiting, a resume asks again.
- The loop cap is milder: with no surface, the node fails with `LoopCapped`, whose remedy is `retry(Precondition::CapLifted)`. It can be answered from another process by raising the cap. What does not exist outside a terminal is "continue for this invocation". The spec decides whether that is needed.
- Scope and departures are already answered from another process (`engine/tests/deviation/surfaces.rs:41`).

## Assumptions

1. Attached `yunta run` does not change for whoever uses it: it keeps its region and its inline prompts.
2. The TUI ships in the same binary, with no feature flag.
3. The desktop is for macOS and Linux, on the same machine, because the engine does not compile on Windows.
4. "Only evidence moves" is kept, runs can be closed several at a time, a run asking at another terminal is shown read-only, and the mouse is not captured.
5. This map and the specs live at the repository root, in English, since the repository holds its root Markdown to that. Plans and task lists live in `tasks/`. They are committed with their module and go in the pull request; commits and the pull request do not cite them, by the repository's own rule.

## Out of this map, on purpose

The from-scratch reorganization does not enter: `core` with no IO, `yunta-policy`, the log's own types, one crate per adapter. It can be done gradually, and nothing in this map prevents it. `yunta-state` is the only part brought forward, because it avoids moving the pages twice.

## Approved with the map

- `yunta-control` and `yunta-app` are separate crates.
- `yunta review` at a terminal opens the application at that decision, and the duplicated full-screen loop is deleted. Outside a terminal it prints as before.
- `third-party-notices` is its own module.
- The names `yunta-state`, `yunta-control`, `yunta-app` and `yunta-tui` are fixed, as are the module ids.
