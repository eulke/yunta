# Implementation Plan: `control`

Spec: `SPEC-control.md`, approved on 2026-11-09. Base: `feature/yunta-tui` at `d4e540f8`, with `77091b01` and the pool's fix for a cut-off add on top. `cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.

## Overview

Every interface Yunta has, and every one it will have, plugs into the same behavior and gets it; none of them holds any. Today the CLI holds most of it: 125 call sites into the engine across its command modules, the error sentences, the reading of its process in many places, and an MCP server that builds the CLI's sequences again.

This plan moves that behavior below the surfaces:
- the work into a new crate, `yunta-api`;
- the refusals' data into core and their words into render;
- the two ports an interface implements into core's `port`;
- the JSON documents into render.

It also makes the MCP server an agent asks Yunta through an interface of its own, `yunta-mcp`, over what every MCP server of Yunta speaks, `yunta-mcp-protocol`. Nothing a person, an agent or a script reads changes.

## Architecture Decisions

### 1. What each layer is

A behavior that two surfaces need lives below both of them, once. A surface reads its process, carries a request in and an answer out, draws, and takes input; it decides nothing another surface would decide again.

| Layer | Is | Holds |
|---|---|---|
| `yunta-core` | the domain | Types and rules every crate shares, with no IO: documents, events, the ports an engine calls, and the data of every refusal a person reads. |
| `yunta-storage` | the log | The append-only store behind its interface. |
| `yunta-replay` | what happened | Every reading of a run, derived from its log, with no IO. |
| `yunta-mcp-protocol` | how Yunta speaks MCP | The protocol revisions every MCP server of Yunta announces, and the shape of what it answers. |
| `yunta-engine` | how a run runs | Scheduling, the task cycle, the checkouts, git, the lock, the registry, the per-run tools. |
| `yunta-adapters` | the agent CLIs | `claude-code`, `codex` and `mock` behind the port, and the forge. |
| `yunta-render` | what Yunta says | To a person, as words and pages; to a machine, as versioned JSON documents. It reads nothing. |
| `yunta-api` | what can be asked of Yunta | Each operation an interface asks for, done with the engine, the store and the adapters, handed a workspace, returning data. It prints nothing, knows no transport, and reads nothing of its own process. |
| Surfaces | the interfaces | Each reads its process once and builds a `Workspace`. Each draws render's pages or writes its documents, and takes a person's input through the ports. |

The surfaces are:
- `yunta`, the CLI: clap, the streams, the attached region, the terminal's prompts, the exit code;
- `yunta-mcp`: the tools an agent calls;
- later, `yunta-app` and the TUI;
- the desktop.

### 2. The crates and their edges

| Crate | `[dependencies]` in the workspace |
|---|---|
| `yunta-mcp-protocol` | none (`rmcp`) |
| `yunta-engine` | core, storage, replay, mcp-protocol, surco |
| `yunta-api` | core, storage, replay, engine, adapters, render, surco |
| `yunta-mcp` | core, replay, render, api, mcp-protocol, surco |
| `yunta` | core, replay, render, api, mcp, surco |

- A test of the CLI and a test of `yunta-mcp` each read the crate's `[dependencies]`, as `no_adapter_crate_in_engine` reads the engine's. Each fails, saying where the reading belongs, if `yunta-engine`, `yunta-storage` or `yunta-adapters` is named.
- `.agents/architecture.md`'s table gains the three rows and changes `yunta`'s, which `cargo xtask docs --check` holds.

### 3. The `Workspace` and the `Stop`

```rust
// crates/api/src/workspace.rs
/// What an interface hands the API instead of the API reading its process:
/// where it works, the environment it was started with, and the boundaries
/// every decision is injected through.
pub struct Workspace {
    /// The directory the request is about: the repository a command runs in.
    pub dir: PathBuf,
    /// The environment the process started with, read once.
    pub env: Env,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdSource>,
    /// What a run or a forge reads by name: a secret's value.
    pub secrets: Arc<dyn SecretSource>,
    /// The binary a detached run, a resume handed on and the rail hook start:
    /// Yunta itself.
    pub binary: PathBuf,
}

/// How the caller stops an operation, in the two stages a Ctrl-C has:
/// stop what runs, then abort what tears it down.
pub struct Stop {
    stop: CancellationToken,
    abort: CancellationToken,
}
```

- The CLI builds both in one module, `crates/cli/src/shell.rs`: the boundary where the process enters. It replaces `context.rs`, and its `Interrupt` gives the `Stop`.
- The MCP server's caller builds them once when it starts; for `yunta mcp`, that is the shell.
- `Context`, the CLI's `Project`, `process_env()` called in five places, `process_clock()` and `time_until`'s own clock are gone.
- The API resolves the project from the workspace, as `Workspace::project()`.

### 4. The API's operations, by what an interface asks

Each operation reads as what a person does with Yunta. A handle is opened once and asked again: the project, then a run of it.

```rust
let project = workspace.project()?;                       // config layers, roots, the store
let catalog = project.catalog().await?;                   // render's Catalog data
let field = project.field(Reach::Here, depth).await?;     // rows, unreadable runs, elsewhere
let run = project.run(&run_id)?;                          // OpenedRun: its manifest and directory
let reading = run.reading(depth).await?;                  // state, frame, decision, documents, plans
run.cancel(&stop).await?;                                 // CancelState
run.close().await?;                                       // what close answers
run.decide(choice).await?;                                // a gate's decision, then the resume handed on
run.answer(&node, answers).await?;                        // a questions artifact's answers, then the same
let admission = project.admit(&reference, inputs, mode).await?;   // verdict, preflight, cautions
let created = admission.create().await?;                  // the run, its checkout and its hold
created.start_detached().await?;                          // or created.drive(person, observer, &stop)
project.collect(Collect::Pass).await?;                    // gc, or its dry run
project.packs().add(&source, confirmed).await?;           // and update, remove, list, audit, new
project.doctor(checks).await?;                            // doctor's checks
project.cases(&dir, |case| ...).await?;                   // the test runner, each case as it ends
```

The crate's modules are named by subject:
- `workspace`, `project`, `catalog`, `field`;
- `run`: a run opened, its reading, its stats and history, its receipt and verification, the page `open` writes, and following it (heads, the log after a seq, whom an engine asks, the words);
- `admission`, `drive`, `detach`, `answers`, `ending` (cancel and close), `collect`;
- `packs`, which holds the lock;
- `setup` (`init`, `new`), `machine` (doctor's checks, detection, the probes, and the adapters a binary builds), `cases`.

Each public item has one path. Types that would meet render's (`page::Run`) are named apart: `OpenedRun`, `RunReading`. A note or caution a command says while it works (an estimate's caution, an unknown key, each problem `gc` meets, what a cancel waits on) reaches the caller through a sink it passes, in order, and the CLI says it then.

### 5. Errors: what was refused, what failed, and the stop

```rust
// crates/api/src/error.rs
/// Why the API did not do what it was asked.
pub enum Error {
    /// What Yunta refused: render says it, with the commands that go on.
    Refused(Refused),
    /// What failed below the API: the store, git, the disk, an adapter, a
    /// workflow the catalog does not hold; said in its own words, with its causes.
    Failed(Failure),
    /// The caller's stop.
    Stopped,
}
```

**What a refusal is.** A refusal has words of its own and advice; the CLI branches on its kind today. Its data goes to core, in the engine's terms, and the engine returns it:
- **`core::check`:** `CheckError`, `Composition`, `Unanswerable`, `UntakenRoute`, from `engine/check`. Every field is already a core type. The engine keeps the checking.
- **`core::refusal`:** `GateRefusal` (the five refusal variants of `ResolveGateError`), `CloseRefusal` (`AlreadyClosed`, `Driven`, `Moving`), and `ViewFault` (what `StoreError` says of an artifact view).
- **Already core:** `InputsProblem`, `ObjectProblem`, and a refused answer's `Refusal`, `Claim` and `FailureMenu`.
- The engine's `ResolveGateError` and `CloseRunError` each become `Refused(<the core refusal>)` beside their failure variants (`Storage`, `Documents`, `Export`).

**Render's `refusal::Refused`** gathers every kind a person reads:
- a workflow that cannot run here (`Unrunnable`), and a document that does not read;
- a gate's answer refused, and a close refused;
- a run another engine drives, one whose engine asks at its terminal, or one whose holder nobody can tell (built by the API from the lock and the registry);
- inputs that do not satisfy, a refused answer, an object or a view the store lacks;
- a reference that names several runs, and a document Yunta does not read.

Render also holds `said(&Refused, Reader)` and `advice(&Refused, Reader)`, moved from the CLI's `error::words` and `CliError::sentence`, and a page for a refusal. Every interface says a refusal in one place, the TUI included.

**A failure** is said by its own chain of causes, as `describe` says it today. `Failure` holds it as `Box<dyn Error + Send + Sync>`, so no interface names an engine, store or adapter type and none branches on one. The text a person reads is the same.

`CliError` becomes `Refused`, `Failed` and `Interrupted`, beside what is the CLI's alone (its usage of the process). Its page draws render's.

### 6. The ports an interface implements, in core's `port`

- **The person port:** `HumanInteraction`, with `QuestionsReply` and what a decision is asked with. That type is named `DecisionAsked`, because `Asking` is replay's registry reading, which a surface reads beside it.
- **The observer port:** `RunObserver`, with `Observed` and `Said`, the words a session says.
- The engine calls them and re-exports none; the engine keeps serving the words socket.
- Every field is a core type or an instant. They touch 8 engine files and 14 test files.

### 7. MCP

- **`yunta-mcp-protocol`** is what every MCP server of Yunta speaks: the protocol revisions and the shape of a tool list, from `engine::mcp`. The engine's per-run tools speak it.
- **`yunta-mcp`** is the server an agent asks Yunta through:
  - each tool, its schema, its arguments, what it asks of the API, and its answer, from the CLI's `commands/mcp`;
  - an `rmcp` handler with no transport;
  - `yunta mcp` serves it on stdio, and a server can serve it over HTTP without copying a tool.
  - Its tools call the same functions of the API the CLI's commands call.

### 8. The JSON documents are render's

`crates/render/src/document/` holds what the CLI's `json/` holds: every document with its `schema_version`, written from render's page data and replay's, beside the pages. The CLI's `--json` and the MCP server's answers write the same shapes. The documents' tests move with them.

### 9. The pack lock

`yunta_api::packs` holds `.yunta/yunta.lock` with the engine's file lock across each command's read and write, waiting for another pack command rather than refusing it. A test holds the lock, starts an `add`, sees it wait without writing, releases the lock, and finds both entries in the file.

### 10. What reads the process, held

- `crates/cli/src/shell.rs` is the one place that reads the process: the directory, the environment, the clock, the id source, the secrets, the binary, and the interrupt.
- A smell, `process_read_below_the_shell`, counts at zero, outside it and the tests, `current_dir`, `current_exe`, `env::args` and a signal handler.
- `system_clock_outside_boundary` allows the shell in place of `context.rs`, and `env_read_outside_boundary` holds the API as it holds every crate.
- `Env`'s terminal variables, which `main` reads for the look, stay with the shell.

### 11. Behavior another surface needs that this plan does not move

These stay in the CLI after this module because the map gives them to `app-core`:
- the terminal's prompts' models (`card`, `menu`, `form`, `field`, `desk`);
- the live region's words, `surface/chronicle`: how each event of a run reads as a line, which the TUI's run screen needs.

The map's `app-core` row gains the chronicle's words, for `yunta-render`, so no behavior is left in a surface unscheduled.

## Phases

Four phases, each one commit with the suite green: every step of the `ci` job, the suite in its groups. Each ends at a checkpoint the user reviews, then `cargo clean`. Inside a phase, each group of work ends green, with what it prints compared against the base, before the next begins; a group does not stop for the user. The work stops inside a phase only on a decision that is the user's: a printed text that would change, a choice this plan does not make, or a failure whose cause is not found.

### Phase 1: the base, and what moves below the surfaces
- **The base:**
  - the suite's binaries and tests by name;
  - every golden's digest;
  - `pages.sh`'s captures from a release build;
  - every JSON document and every MCP tool's answer over a copy of a few runs in a scratch home;
  - a real run: fragua detached, stopped before its `pr` node, its gates answered with `yunta resolve-gate`, its time read from `yunta stats`.
  `.plans/yunta-tui/proof/control/` holds them.
- **The ports in core:** `HumanInteraction`, `QuestionsReply`, `DecisionAsked`, `RunObserver`, `Observed` and `Said` move to `core::port`. The engine calls them from there, and the CLI's console and live region implement them from there.
- **Refusals as core data, said by render:**
  - `core::check` from the engine's check types, and `core::refusal`. The engine's errors split into refusal and failure.
  - Render's `refusal::Refused`, with its `said`, its `advice` and its page, from the CLI's `error::words` and `CliError`. The words' tests move with them.
  - `CliError` carries render's `Refused`.
- **What MCP speaks, and the documents:** `yunta-mcp-protocol` from `engine::mcp`, with the engine on it; `render::document` from the CLI's `json/`.

**Checkpoint A:** every refusal's text, the JSON documents and the MCP answers are the same, through the CLI's tests, the captures, the documents and the answers.

### Phase 2: `yunta-api`, its workspace, and reading through it
- **The crate and its workspace:**
  - the crate, `Workspace`, `Stop`, `Error`, `Failure`;
  - the project, from `project.rs` and `context.rs`;
  - the adapters a binary builds, the catalog and detection;
  - `shell.rs` builds the workspace once.
- **Reading through the API:**
  - the field;
  - a run's reading for `status`, `--node`, `stats`, a workflow's history, `receipt`, `verify`, `open`'s page, `graph`'s frame, and the JSON's decision;
  - following a run, for `status --follow`, the live region's gaps and `workflow_status`.

**Checkpoint B:** the CLI resolves every project and reads every run through the API; the environment is read once; the captures and the follow tests are unchanged.

### Phase 3: running a run, answering it and ending it, through the API
- **Admission and running:**
  - the verdict for `check` and `new`; the preflight; creation; driving here with the ports, and promotions;
  - detaching, and handing a resume on;
  - `run`, `resume` and `run_workflow` ask the API;
  - a real run after this group.
- **Answers and a run's end:**
  - `resolve-gate`, `review` and answering questions;
  - `cancel` and `close`, with their signals and the pause after a crash.

**Checkpoint C:** the run path, its real run beside the base's; those commands' tests and the parked runs' flows.

### Phase 4: upkeep, the MCP server, the frontier and the proof
- **The project's upkeep through the API:** `gc`; every `pack` command, with the lock and its test; `init` and `new`; `doctor`'s checks; the test runner, each case handed back as it ends.
- **The MCP server, a crate of its own:** `yunta-mcp` from `commands/mcp`, on the API and `yunta-mcp-protocol`; `yunta mcp` serves it on stdio.
- **The frontier:**
  - the CLI's and `yunta-mcp`'s manifests drop the engine, the store and the adapters, with their tests;
  - the smell for process reads;
  - `.agents/architecture.md`'s table; each moved module's `//!`; render's and core's maps.
- **The proof and the review:**
  - the suite's count against the base's, by name;
  - every golden's digest and the captures;
  - the JSON documents and the MCP answers;
  - a real run at the end beside the base's;
  - the module's diff read in four parts by readers of their own, each finding checked and decided.

**Checkpoint D:** the user reviews the proof and the review. Then the commit, the pull request's body, and CI on Linux and macOS.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| The move is large: about 200 call sites, ten error types, two ports, three new crates | High | Phases in dependency order, each compiling and green; a command group at a time; `git mv` for whole files. |
| A refusal's text drifts when its words move to render | High | The CLI's tests and the captures pin every refusal; the words' unit tests move with them byte for byte. |
| A failure's text changes when it is boxed | Medium | `describe` over the box says the same chain; tests that read a failure's text pass unchanged. |
| The run path slows | Medium | Real runs on the base, after admission and running, and at the end; the time between nodes inside the base's spread. |
| An async API borrowing the workspace fights lifetimes | Medium | Handles own what they need (`Arc` boundaries, an owned `Stop`), and `Supervision` is built inside the API. |
| The MCP server answers differently once it is a crate | Medium | `mcp_flow` and every tool's answer captured on the base and compared once the server is its own crate. |
| The pack lock's test is timing-dependent | Low | It holds the lock itself and watches the second command wait; no race decides it. |
| The disk fills during gates | Low | `CARGO_INCREMENTAL=0`, one build at a time, `cargo clean` after each phase. |

## Open Questions

None. The user chose the refusals' place, the workspace's fields, the pack lock, the two MCP crates and their names, and the documents' place; the other names above stand unless the review of this plan changes them.
