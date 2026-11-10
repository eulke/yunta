# Spec: `control`

Module `control` of `CAPABILITY-MAP.md`: "`yunta-api`, with the CLI and MCP mounted on it". It depends on `log-follow` and `run-state`, and starts from `d4e540f8`, where `run-pages` ended.

Status: **approved on 2026-11-09**, with refusals as core data, a workspace of six fields, and the pack lock's race held in this module. While planning it, the user chose that the MCP control plane is a crate of its own (`yunta-mcp`, on the API, with no transport of its own), that what every MCP server of Yunta speaks is a crate below the engine (`yunta-mcp-protocol`), and that the JSON documents are render's; points 9 and 10 say it.

## Objective

`yunta-api` is what can be asked of Yunta with no terminal: read a project and its runs, admit a run, start one or drive one in this process, answer it, cancel it, close it, collect what runs leave, install packs, check a workflow, run a project's cases, check the machine, and set a project up. Each operation returns data, and nothing in the crate prints. The CLI's commands and the MCP server's tools become what an interface is: they read the process once, ask the API, and draw render's page or write a JSON document with what came back.

The reason is the next three interfaces. The terminal application (`app-core`, `tui`) and the desktop ask the same questions the CLI asks today. If each one builds its own answer out of the engine, the same decision gets made three or four times, and the copies drift. That has already happened between two of them, the CLI and MCP.

### What happens at the base commit (`d4e540f8`)

**The CLI is where Yunta's work is built.**
- It has 122 source files and 26,932 lines.
- Outside its tests it names `yunta_engine` in 58 files (289 mentions), `yunta_storage` in 19 (26) and `yunta_adapters` in 8 (14).
- It names more than a hundred distinct items of the engine; the most used are `CheckError` (16), `read_engine` (14), `lock::SystemProbe` (14), `CheckoutPool` (11), `RunError`, `process::Supervision` and `run_redactor`.
- Its manifest names `yunta-engine`, `yunta-storage` and `yunta-adapters` directly, and nothing holds it from naming them.

**Each command assembles its work from the engine's pieces.**
- Attached `yunta run` does it in this order: `Context::load`, `resolve_workflow`, then check (`check`, `check_warnings`, `origin_of`, `check_workflow_refs`), `committer`, `build_manifest`. Then the adapters are served: `check`, `check_mounted`, `resolve_runner`, and a probe of each real adapter. Then the preflight (`check_mode_start`, `check_context_files`, the estimate), the run's creation (`run_redactor`, a checkout from `CheckoutPool` or `prepare_worktree`, `create_run`), `hold_run`, `execute_run` with the CLI's prompts and its live region, the promotions, and last `release_worktree`.
- `yunta resume`, `--detach`, `status`, `status --follow`, `stats`, `list`, the field, `cancel`, `close`, `gc`, `resolve-gate`, `review`, `init`, `new`, `check`, `verify`, `receipt`, `open`, `test`, `doctor` and the six `pack` subcommands each assemble their own sequence the same way. Together the command modules make 125 call sites into the engine, beside the shared modules' (the errors, the live region, the JSON documents, the context), and their own reads and writes of the store.
- `cancel` signals the engine and its process groups itself: SIGINT, then SIGKILL after 15 s. When the engine is dead it records the pause the engine never wrote, and removes `engine.json`.

**MCP builds the same sequences again.**
- `run_workflow` calls the CLI's `run::start_detached`.
- `resume_run`, `resolve_gate` and `answer_questions` each open the run, refuse a run another engine drives, record what they record and spawn `yunta resume`, beside the CLI's own paths for the same things.
- `workflow_status` reads a run the way `status` does.
- Every tool call reads the current directory and builds a `Context`.

**Refusals are the engine's types, worded by the CLI.**
- `CliError` carries eight error enums of the engine (`RunError`, `ResolveGateError`, `CloseRunError`, `ManifestError`, `GitError`, `CatalogError`, `AnswerQuestionsError`, `WorktreeError`), the store's `StorageError` and the adapters' `FixtureError`.
- `error::words` writes the sentence for each, and for `CheckError`, `StoreError` and `LockError`, matching on their variants.
- An application drawing the same refusal would have to write those sentences again.

**The CLI implements two of the engine's traits.**
- `ConsoleInteraction` implements `yunta_engine::HumanInteraction`: a gate's decision and a questions artifact, asked at the terminal, with `Asking` and `QuestionsReply`.
- The live region's `Feed` implements `yunta_engine::RunObserver`, with `Observed` and `said::Said`.
- Every field of `Asking`, `QuestionsReply`, `Observed` and `Said` is a core type already, or an instant.

**The process is read in many places, and the environment more than once.**
- `Context::load` is called in 23 files, and `std::env::current_dir` in 8, the MCP server's every tool call among them.
- `Env::of_process` is reached through `project::process_env()` from `main`, `Context::resolve_in`, `project::user_root`, `project::load_named_layers`, `init` and the rail hook, so one invocation reads its environment several times.
- The clock is the static `process_clock()`, carried in `Context`. `time_until` reads that static and not the injected clock.
- Run ids come from `SystemIdSource`, and the binary a detached resume and the rail command start is `own_binary()`: `current_exe`, else `"yunta"`.
- `Interrupt::ctrl_c` installs a SIGINT stream in five places: `Context::load`, `init`, `mcp`, `test`, and `pack new`'s check. `ProcessSecrets` reads a secret's variable by name when a run or a forge needs it.

**The CLI's context and project are built in the CLI.**
- `Context` holds the directory, the `Project`, the clock, the id source, the `Env`, the interrupt and the rail command.
- `Project` holds the merged config (org, user, repo), the runs root, the checkouts' root and the store's path, and creates the store's directory.

**The store is opened by the CLI and read by its surfaces.**
- `Context::async_storage` and `Context::storage` open it.
- The live region's painter reads a run's events to close a gap, and the follower reads `heads` and `events_after`.

### Who this is for

- **A person at the CLI:** nothing they read, type or get back changes.
- **An agent over MCP:** each tool answers what it answers today, and it does what the CLI does because it is the same function.
- **The terminal application:** it asks `yunta-api` for the field, a run as its log stands, the log after a seq, the heads, whom an engine is asking, the words a session says, and for every action a person takes in it. What comes back is data it draws in Surco with render. It never hosts a run.
- **The desktop, later:** it links the same.

### What each of them gets

1. **`yunta-api`, a library crate in `crates/api`.**
   - It depends on core, storage, replay, engine, adapters, render and surco, as the map says. It needs render because it returns the data render's pages draw and writes the files a run keeps: `receipt.md`, the page `open` writes.
   - It is the only crate an interface reaches the engine, the store or the adapters through.
   - The composition root that builds the adapters a binary carries moves here from the CLI's `commands`.

2. **Control is handed a `Workspace`, and reads nothing of its process.** The `Workspace` holds:
   - the directory it works in;
   - the environment, as core's `Env`, read once;
   - the clock;
   - the id source;
   - the secrets a run or a forge reads by name;
   - the binary that a detached run, a resume handed to another process and the rail hook start: `yunta` itself.

   The CLI builds one in `main`, and the MCP server one when it starts. The API calls no `current_dir`, `current_exe`, `std::env::var`, `args` or `SystemClock`, and installs no signal handler. An operation that can be stopped takes its caller's stop, with the two stages a Ctrl-C has today: stop, then abort. The API may probe whether another process lives, which is how a run's engine is told from a leftover; that is the domain's question, not its own process's context.

3. **Every operation is a function of the API that returns data or a refusal.**
   - **The project:** its config and roots, and the catalog: the repository's workflows and its packs.
   - **Reading:** the field; a run as its log stands, with its frame, its decision, the documents it shows and its plans; stats and a workflow's history; the receipt; verification; the log after a seq, and the heads; whom an engine is asking; the words socket.
   - **Admission:** a workflow's verdict and diagnosis, the preflight with its estimate and cautions, and a run's creation.
   - **Running:**
     - driving a run in this process, with the person port and the observer port its interface brings: an attached run, a resume, a case of `yunta test`, `doctor --session`'s probe, and promotions;
     - starting a run detached, and handing a resume to another process.
   - **Answering:** a gate's decision, a questions artifact's answers, a review.
   - **A run's end:** cancel, with its signals and the pause it records after a crash; close; collect, with its dry run.
   - **Packs:** add, update, remove, list, audit and new.
   - **Setup:** `init` (detection, then the config, the ignore entries and the mechanism skill it writes) and `new` (the skeleton).
   - **The machine:** doctor's checks, detection and the probes.
   - **The test runner:** each case run, its result handed to the caller as it ends.

   What a command warns or notes while it works today reaches its caller as data, in the order it happens. Examples are a budget's caution before a run, an unknown key in a manifest, each problem `gc` meets and what a cancel is waiting for. The CLI says each at the moment it says it now.

4. **A refusal a person reads is data the engine, the API and every interface share, and render says it.**
   - The refusals measured above become core data, as `CheckError`'s findings, a pack's audit and a repository's detection already are, and the engine returns them.
   - The sentences in the CLI's `error::words` become render's, so the application says a refused answer, a run another engine drives or a close that cannot happen in the same words.
   - `CliError` stays the CLI's one error and carries what the API refused.

5. **The ports an interface implements live in core's `port`,** beside the adapter's and the forge's: the person port (`HumanInteraction`, with `Asking` and `QuestionsReply`) and the observer port (`RunObserver`, with `Observed` and the `Said` a session's words arrive as). The engine calls them and re-exports none of them. The CLI's console and live region implement them as they do now.

6. **The CLI's manifest names neither the engine, the store nor the adapters.**
   - Its `[dependencies]` name core, replay, render, surco and api, and external crates.
   - A test of the CLI reads that table, as `no_adapter_crate_in_engine` reads the engine's, and says where the reading belongs when one is added.
   - The tests may still name what they drive.

7. **One function for one thing, whichever interface asks.** Starting a run detached, handing a resume on, recording a gate's decision, recording answers, reading a run and reading the catalog are each one function, which the CLI's command and the MCP tool both call.

8. **Nothing printed changes.**
   - The 99 goldens stay byte for byte, and the CLI's integration tests pass unchanged.
   - The 104 captures of `pages.sh` equal the base's.
   - Every JSON document prints the same, every MCP tool answers the same text, and every exit code is the same.

9. **The MCP control plane is a crate of its own, `yunta-mcp`,** an interface on the API like the CLI.
   - It holds the tools an agent calls, their schemas, and what each asks of the API. It has no transport: `yunta mcp` serves it on stdio, and a server can serve it over HTTP without copying a tool.
   - What every MCP server of Yunta speaks, the protocol revisions it announces and the shape of what it answers, is `yunta-mcp-protocol`, below the engine, whose per-run tools speak it too. Today it is `engine::mcp`.
   - Its manifest names neither the engine, the store nor the adapters, as the CLI's does not, and a test holds both.

10. **The JSON documents are render's,** beside the pages: what Yunta says to a machine, with its `schema_version`, as the pages are what it says to a person. The CLI's `--json` and the control plane's answers write one shape, and a server would write the same.

### Names and form

- **The crate is `yunta-api`, in `crates/api`, its library `yunta_api`,** as the map fixes it. Its modules are named for what an interface asks, its functions for what they do, and its types for what they are. Nothing is a `Manager`, a `Service` or a `Handler`.
- **`Workspace` is what the API is handed,** named by the map. A field holds what it is (`dir`, `env`, `clock`, `ids`, `secrets`, `binary`), never how it was read.
- **One path to each item.** Control's public items carry core, replay and render types and its own. It re-exports nothing of the engine, the store or the adapters, and leaves no alias for an old name. What an interface needs of those crates' data moves to core, as the ports and the refusals do.
- **The CLI's `Context` and `Project` go.** The CLI holds what its process gave it, and the API resolves the project from the workspace.
- **What a page draws, the API returns as render's data** (`CancelState`, `Resolution`, `Cases`, `Needs`, `RunUnread`, …), filled where the work was done. The CLI builds the page and its `Answer` from it.

### Decisions the plan starts from

- **What the CLI keeps:**
  - `clap` and dispatch;
  - `output`: the streams, their look and width, drawing, emitting, and the exit code (`Outcome`, `RunExit`);
  - `surface/`: the attached region and the follower's drawing;
  - the prompts: `ask/`, `human_interaction`, the keyboard and the terminal's hold;
  - serving the control plane on stdio, for `yunta mcp`;
  - the rail hook, help and completions;
  - the program `open` hands the page to.
- **What stays the engine's:**
  - `execute_run` and the scheduler;
  - `CheckoutPool`;
  - `process::Supervision`, which the API builds from the workspace and the caller's stop;
  - the lock and the registry;
  - `git`;
  - the run tools.
- **The order of the work** is the plan's: the ports and the refusals first, since everything after them names them; then the API's crate and its workspace; then the commands, a group at a time, each group a commit with the suite green; and the manifest's test last, once nothing in the CLI names the crates below the API.

### Assumptions

1. **The `Workspace` holds six things, not the map's four.** The map names the directory, the environment, the clock and the engine binary. The id source and the secrets also read the process today (`SystemIdSource`, `ProcessSecrets`), so they enter the same way.
2. **Refusals become core data, and render words them** (point 4). The alternative is a refusal type of the API's own that the engine's errors are mapped into. It would be a second enum for each of the first, and render could not say it, because render cannot read the API. The application would then write the sentences again. Recommended: core.
3. **The person port and the observer port move to core's `port`** (point 5), because an interface implements them and may not name the engine. Every field they carry is already a core type.
4. **The MCP server builds its workspace once, when it starts.** It reads the current directory on every call today, and nothing in the binary changes the directory.
5. **Attached `yunta run`, `yunta test` and `doctor --session` still drive their runs in their own process,** as the map's first assumption keeps it, through the API's drive. The application never does.
6. **The module lands in phases,** each a commit with the suite green, and the pull request stays the initiative's one.
7. **A change on the run path is proven by real runs.** Admission and driving move here, so a release build runs a real workflow before and after, detached and stopped before its `pr` node, its gates answered with `yunta resolve-gate`, its time read from `yunta stats`.
8. `tasks/plan.md` and `tasks/todo.md` are replaced for this module; those of `run-pages` stay in their commits, the last `77091b01`.

### Found while measuring

- **Two `pack` commands at once can lose an entry of `.yunta/yunta.lock`.** `pack add`, `update` and `remove` read the file whole, edit it in memory and write it back, atomically but with no lock between two commands, so the one that writes last drops the other's entry: its tree stays vendored, `pack list` does not name it, `update` says it is not installed and `add` says it is. This module holds it, at the user's choice: a pack command holds the lock file across its read and its write, and a test runs two adds at once and finds both recorded.
- **Two of the CLI's reads go away with the workspace, with nothing printed changing:** reading the environment several times in one invocation, and `time_until` reading the process clock instead of the injected one.

### Out of scope

- `yunta-ui`, the terminal application and the desktop.
- Any change to what a command does, prints or answers.
- How the desktop reaches the API, in-process or through the binary's `--json`. It is `desktop`'s to decide; the API is a library either way.

## Tech Stack

Rust 1.98.0. A new workspace member, `yunta-api`, on the crates the map names, with `tokio` and `tokio-util` as the engine already has them. No crate from outside the workspace is added. The CLI's manifest loses `yunta-engine`, `yunta-storage` and `yunta-adapters`, and gains `yunta-api`.

## Commands

`cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.

```bash
# Before editing a file, and before finishing with no paths
cargo xtask docs --about <files>

# Test
cargo test -p yunta-api
cargo test -p yunta-core
cargo test -p yunta-render
cargo test -p yunta-engine --test <target>
cargo test -p yunta --bins
cargo test -p yunta --test <target>

# Lint and checks
cargo xtask smells --check
cargo xtask docs --check
cargo xtask schema --check
cargo deny check

# The suite, in groups, and the gate, every other step of the ci job
python3 .plans/yunta-tui/tools/suite.py <group> <log dir>
sh .plans/yunta-tui/tools/gate.sh /Users/eulke/development/yunta-tui <log dir> <step ...>
```

## Project Structure

```
crates/api/Cargo.toml          the crates the map names, and no others
crates/api/src/lib.rs          the crate's map: what an interface can ask, and the Workspace
crates/api/src/                one module per subject an interface asks about
crates/api/tests/              each operation, against a sandbox like the CLI's tests use
crates/core/src/port/              the person port and the observer port, beside the adapter's and the forge's
crates/core/src/                   the refusals a person reads, as data
crates/render/src/                 the sentences of those refusals, from the CLI's error::words
crates/cli/src/main.rs             reads the process once and builds the Workspace
crates/cli/src/commands/           each command: parse, ask the API, draw the page or the JSON, exit
crates/mcp-protocol/                        what every MCP server of Yunta speaks, from the engine's mcp
crates/mcp/              the control plane's server: each tool, its schema, what it asks of the API
crates/render/src/document/        the JSON documents, from the CLI's json/
crates/cli/tests/                  the manifest's test, beside the integration tests, unchanged
.agents/architecture.md            the crate table: yunta-api's row, and yunta's edges
```

## Code Style

An interface reads its process once and hands it in; the API returns data, and the interface draws it:

```rust
// crates/api/src/lib.rs
/// What the API is handed instead of reading its process: where it works,
/// the environment it was started with, and the boundaries every decision
/// is injected through.
pub struct Workspace {
    pub dir: PathBuf,
    pub env: Env,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdSource>,
    pub secrets: Arc<dyn Secrets>,
    /// The binary a detached run and the rail hook start: Yunta itself.
    pub binary: PathBuf,
}

// crates/cli/src/commands/cancel.rs
let state = yunta_api::cancel(&workspace, &run_id, &stop).await?;
emit(mode, &CancelAnswer::of(&run_id, state), look)
```

The names here are illustrative; the plan names each item, by what it is.

## Testing Strategy

- **The guard of every move is what prints:**
  - the 99 goldens, byte for byte;
  - the CLI's integration tests, unchanged;
  - the 104 captures of `pages.sh` against the base, through the base's masks;
  - the JSON documents and the MCP flows (`mcp_flow`).
- **The API's own tests.**
  - Each operation is tested in `crates/api/tests` against a sandbox, with no process read: a workspace with a fixed directory, an `Env` built in the test, a fixed clock and id source.
  - What moves from the CLI's unit tests moves with what it tests, by name.
- **The dependencies.**
  - The CLI's manifest test reads `[dependencies]`.
  - The engine's two tests stay.
  - A smell, or the existing ones widened, counts at zero `current_dir`, `current_exe`, `std::env::var`, `SystemClock` and a signal handler in `crates/api/src`.
- **The ports and the refusals.**
  - The engine's tests that drive a person or an observer pass with the ports in core.
  - Each refusal's sentence keeps its test, now render's.
- **The run path:** a release build at the base and at the end runs the same real workflow, and the time between nodes is compared.
- The suite's count is explained against the base's, by name.

## Boundaries

- **Always:**
  - move a file with `git mv` before editing it;
  - keep every golden byte for byte;
  - keep the workspace compiling and passing at every step;
  - read the process only in the CLI's `main` and the MCP server's start.
- **Ask first:**
  - a change to what a command prints, answers or exits with;
  - a change to a JSON document or an MCP tool's answer;
  - a new dependency of any crate.
- **Never:**
  - an interface that names the engine, the store or the adapters;
  - the API reading its process, printing, or opening a terminal;
  - a re-export or an alias left for a caller;
  - a refusal worded in two places;
  - the application hosting a run.

## Success Criteria

1. **`crates/api` is a workspace member,** with the dependencies the map names, and `.agents/architecture.md`'s table says it, which `cargo xtask docs --check` holds.
2. **Neither the CLI's nor the control plane's `[dependencies]` name `yunta-engine`, `yunta-storage` or `yunta-adapters`,** and a test of each holds it.
3. **The API reads nothing of its process:**
   - no `current_dir`, `current_exe`, `std::env::var`, `SystemClock` or signal handler in `crates/api/src`, counted at zero by a smell;
   - the CLI reads its environment once per invocation;
   - the MCP server reads it once per server.
4. **The person port and the observer port are core's `port`,** and the engine re-exports neither.
5. **Every refusal a person reads is core data said by render.**
   - The CLI's `error::words` holds no sentence over what the engine refused.
   - `CliError` names no engine, store or adapter type.
6. **One function for one thing:** the CLI's command and the MCP tool call the same function of the API to start a run detached, hand a resume on, record a gate's decision, record answers, read a run and read the catalog.
   - The control plane is `yunta-mcp`; `yunta mcp` serves it on stdio, and its tools answer as they do today.
   - What every MCP result is shaped like is `yunta-mcp-protocol`'s, which the engine's per-run tools and the control plane both read.
   - The JSON documents are `yunta-render`'s, and the CLI and the control plane write them alike.
7. **Nothing printed changes:**
   - the goldens' digests and the captures equal the base's;
   - the CLI's integration tests pass unchanged;
   - the JSON documents and the MCP tools answer the same.
8. **The run path costs nothing:** a real run before and after, its time between nodes inside the base's own spread.
9. **Two pack commands at once both stand in `.yunta/yunta.lock`:** a test runs two `pack add` of different packs at once, and the lock names both.
10. **The gate passes:** every step of the ci job, with the suite's count explained.

## Open Questions

None. The user approved assumptions 1 and 2 as written, and chose that this module holds the pack lock's race.
