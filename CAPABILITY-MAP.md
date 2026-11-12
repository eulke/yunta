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
  replay    yunta-replay    NEW: what happened. Replay and the read model, pure, no IO
  render    yunta-render    adds every page, since it can read yunta-replay, and the JSON documents a machine reads
  mcp-protocol yunta-mcp-protocol NEW: how Yunta speaks MCP: the protocol revisions it announces, the shape of what it answers
  adapters  yunta-adapters  no change
  engine    yunta-engine    adds the resume guard, parked answers, the words socket
  api       yunta-api       NEW: what can be asked of Yunta, with no terminal
  mcp       yunta-mcp       NEW: the MCP server an agent asks Yunta through, on the API, with no transport of its own
  ui        yunta-ui        NEW: the application with no terminal and no window: state, stages, screens, work
  tui       yunta-tui       NEW: the only crate that knows Ratatui and crossterm
  cli       yunta           the binary: clap, output, attached view, inline prompts, the control plane on stdio, rail
  desktop   (later)         Tauri over yunta-ui, with Surco's Html surface
```

New or changed dependencies:

| Crate | Depends on |
|---|---|
| `yunta-replay` | core, surco |
| `yunta-render` | core, replay, surco |
| `yunta-mcp-protocol` | nothing in the workspace; `rmcp` |
| `yunta-engine` | core, storage, replay, mcp-protocol, surco |
| `yunta-api` | core, storage, replay, engine, adapters, render, surco |
| `yunta-mcp` | core, render, api, mcp-protocol, surco |
| `yunta-ui` | core, replay, render, api, surco |
| `yunta-tui` | core (the clock it is handed), ui, surco, ratatui, crossterm |
| `yunta` | core, replay, render, api, mcp, surco, and `yunta-ui` and the TUI as they come; its dependencies never name the engine, the store or the adapters |
| desktop | ui, surco |

What moves:

| What | To | Note |
|---|---|---|
| `replay`, `view`, `live`, `stats`, `history`, `receipt`, `verification_effectiveness`, `modes`, the value types `Engine`, `Prompt`, `Asking` | `yunta-replay` | About 2,650 lines at the base, 4,143 at `8c833576`. They import only core, surco and each other. Reading `engine.json` stays in the engine |
| Every page a command shows: `run_page`, the node, the field, stats and history, doctor, and the answer of every other command; and render's own gate, plan, spec, shown, findings and receipt documents | `yunta-render`, as values of one `Page` trait, in `page::` | They take data; reading, the exit code, the streams and the JSON stay in `yunta`. The decision block is one, in `escalation` |
| The `after_for` table and the spelling of commands | `after_for` to `yunta-replay` as `RunFrame::actions`; every command a person is told to type a `NextStep`, spelled once in `yunta-render`'s `command`; both in `run-pages` | One table: the CLI spells it, the TUI binds keys, the desktop shows buttons. `advice` and `Spell` are gone |
| Gathering the inbox, run admission, resolve, cancel, close, gc, packs, doctor's checks, detection, the test runner | `yunta-api` | They return data and never print |
| The models of `card`, `menu`, `form`, `field`, `desk`, `Stroke`, `Press` | `yunta-ui` | `hold`, `Console`, `keyboard` and the live region stay in `yunta` |
| What a moment of the chronicle says; the prompts' words of `init` and `new`; what a workflow shape does; the graph as Mermaid and DOT | `yunta-render` | What the live region keeps of a moment stays in `yunta`. Every sentence a stage says is render's |
| Following a run from another process (`status --follow`'s loop and its listening); a run's page put together and written for a browser | `yunta-api` and below the surfaces | `status --follow` and the application follow alike; spawning the system's opener stays with each surface |
| `json/` | `yunta-render`, as the documents a machine reads, beside the pages a person reads | The CLI's `--json`, the control plane's answers and a server read one versioned shape. Moved at the user's choice on 2026-11-09, when MCP left the binary |
| The control plane's MCP server: its tools, their schemas and what each asks | `yunta-mcp`, on `yunta-api` | Transport-agnostic: `yunta mcp` serves it on stdio, and a server can serve it over HTTP |
| What every MCP result Yunta serves is shaped like (`engine::mcp`) | `yunta-mcp-protocol` | The engine's per-run tools and the control plane announce the same protocol and answer in the same shape |

## Rules that prevent rework

| Rule | What holds it |
|---|---|
| No interface calls the engine, only `yunta-api` | A test over the dependencies, like `no_adapter_crate_in_engine` |
| `yunta-ui` does not know a terminal | The same kind of test |
| `yunta-replay` is pure | Its manifest lists neither tokio nor rusqlite, and a test holds that |
| `update` is pure and effects are data | A test walks every reachable state |
| The desktop stays possible without building it | That walk also draws each screen with `Html` |
| Nothing below the interface reads the process | `yunta-api` takes a `Workspace`: the directory, the environment, the clock, the id source, the secrets and Yunta's own binary |
| A color has one table | `Paint` and the bridge to Ratatui read the same Surco function |
| Moving code does not change what is printed | The existing goldens are not touched during a move |

## Modules

| Id | What it delivers | Depends on |
|---|---|---|
| `run-guard` | `resume` refuses while an engine is alive, and macOS detects a reused pid | nothing |
| `parked-answers` | A run stopped by the token cap can be continued from another process | nothing |
| `log-follow` | `events_after`, the head of each run, and following a run from another process | nothing |
| `run-words` | The per-run socket, in every adapter at once | nothing |
| `session-ends` | A Codex retry notice no longer ends its session, and no command an agent started outlives its session | nothing |
| `surco-screen` | Public reading of styles; the ribbon, legend and attention blocks; a screen as data, drawn as Markdown and HTML; a pane's exact width; with their goldens | nothing |
| `run-state` | The `yunta-replay` crate, cutting the cycle between `replay` and `stats`, each item with one path and a name that says what it is | nothing |
| `run-pages` | Every page in `yunta-render`, in one form; what a run offers next as actions; every command spelled once; the CLI printing exactly the same | `run-state` |
| `control` | `yunta-api`, with the CLI and MCP mounted on it | `log-follow`, `run-state` |
| `app-core` | `yunta-ui`: every stage a person works in (signpost, setup, first run, field, run, decision, start, evidence, stats, graph, catalog and packs, doctor, upkeep, schema), with the palette, help and confirmations over them; their screens, their work and its performer; the prompts' models moved; following a run, a run's page and the graph assembled once, below the surfaces; every sentence in `yunta-render`; setting a repository up declaring its runners, in every interface | `control`, `run-pages`, `surco-screen` |
| `tui` | `yunta-tui`, bare `yunta` and `deny.toml`, laying out every stage `app-core` makes, in slices: watch, decide, start, then the others. It lays a screen out with Ratatui's `Layout`, draws each pane through Surco's terminal at `Width::exact`, and draws the overlay's box in Surco's glyphs; `surco-screen`'s `tui-reference` frames are its reference | `app-core`, `parked-answers` |
| `desktop` | Reserved only: no spec and no code yet | `app-core` |

**Order:** `run-guard`, `parked-answers`, `log-follow`, `run-words`, `session-ends`, `surco-screen` and `run-state`, which do not depend on each other → `run-pages` and `control` → `app-core` → `tui`.

**How each module is built:** spec, plan, tasks, implementation, in that order and each with its own approval. The next module starts when this one is closed.

**Delivery:** one branch, `feature/yunta-tui`, in the worktree `/Users/eulke/development/yunta-tui`; one commit per module, or one per phase where its plan is committed in phases, each with the suite green; one pull request against `main`, when the last module ends.

`tui` does not wait for `run-words`: the words appear when the socket exists, with no change to the screen. Only the "decide" slice needs `parked-answers`.

### `parked-answers`: what it has to respect

- At the base commit nothing answers from another process: `run/budget.rs` asks directly, and `current_decision` (`run/escalation.rs:249`) offers only three kinds of decision. The page admits it: "no option answers this pause here".
- Authorization per invocation is deliberate (`run/budget.rs:8`, test `budget_authorization_is_per_invocation_a_resume_asks_again`): each invocation spends new money. It is kept: **a recorded answer authorizes only the next invocation**. With no answer waiting, a resume asks again.
- The loop cap is milder: with no surface, the node fails with `LoopCapped`, and `retry` from another process starts its count over, so it goes on from the tasks done: five chained tasks under a cap of 2 went 2 of 5, 4 of 5, finished. Nothing raises the cap, which the run froze. `continue` for one invocation stays a terminal's, and is not needed elsewhere.
- Scope and departures are already answered from another process (`engine/tests/deviation/surfaces.rs:41`).

## Assumptions

1. Attached `yunta run` does not change for whoever uses it: it keeps its region and its inline prompts.
2. The TUI ships in the same binary, with no feature flag.
3. The desktop is for macOS and Linux, on the same machine, because the engine does not compile on Windows.
4. "Only evidence moves" is kept, runs can be closed several at a time, a run asking at another terminal is shown read-only, and the mouse is not captured.
5. This map and the specs live at the repository root, in English, since the repository holds its root Markdown to that. Plans and task lists live in `tasks/`. They are committed with their module and go in the pull request; commits and the pull request do not cite them, by the repository's own rule.

## Out of this map, on purpose

The from-scratch reorganization does not enter: `core` with no IO, `yunta-policy`, the log's own types, one crate per adapter. It can be done gradually, and nothing in this map prevents it. `yunta-replay` is the only part brought forward, because it avoids moving the pages twice.

## Approved with the map

- `yunta-api` and `yunta-ui` are separate crates.
- The control plane's MCP server is a crate of its own, `yunta-mcp`, an interface on `yunta-api` with no transport of its own, so a server can serve it where `yunta mcp` serves stdio; what every MCP server of Yunta speaks is `yunta-mcp-protocol`, below the engine, which serves the per-run tools; and the JSON documents are `yunta-render`'s. Chosen by the user on 2026-11-09, while planning `control`.
- `yunta review` at a terminal opens the application at that decision, and the duplicated full-screen loop is deleted. Outside a terminal it prints as before.
- `third-party-notices` was dropped on 2026-11-07, at the user's choice, when its spec was read: the license stands in each package's definition, and nowhere else.
- The names `yunta-api`, `yunta-ui` and `yunta-tui` are fixed, as are the module ids. `yunta-app` became `yunta-ui` on 2026-11-10, at the user's choice, when `app-core`'s spec was approved: "app" names the desktop application, and the crate is the user interface apart from its medium; the module keeps its id. `yunta-control` became `yunta-api` on 2026-11-09, at the user's choice, while planning `control`: the one API every interface calls, which reads, acts and keeps a project, wider than control; the module keeps its id. `yunta-state` became `yunta-replay` on 2026-11-07, at the user's choice, when its spec was approved: "state" is `RunState` and `yunta-ui`'s own state, and replay is what every module of the crate does.
- `session-ends` was added on 2026-11-05, after `run-words`: the two bugs the agent audit of 2026-11-03 confirmed, still present at `3456baea`.
