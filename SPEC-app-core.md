# Spec: `app-core`

Module `app-core` of `CAPABILITY-MAP.md`: the application as a value, its stages, screens and work, with the prompts' models moved out of the CLI and every sentence a stage says in `yunta-render`. It depends on `control`, `run-pages` and `surco-screen`, and starts from `92d05277`, where `control` ended.

Status: **approved on 2026-11-10**, with these choices by the user:

- The crate is `yunta-ui`. "app" stays for the desktop application.
- The performer lives in the crate.
- The card and the desk become Surco blocks.
- Every stage the spike had is in this module: setup, stats, the graph, doctor, the catalog as a page of its own and the command palette, beside watch, decide and start. The user's words: "no dejar nada afuera".
- Setting a repository up declares its runners, in every interface: `init -i` asks them as the setup stage does.

## Objective

`yunta-ui` is the application with no terminal and no window. It holds what a person has open, decides what each key and each piece of news does, and says which work it needs done. It also draws each moment as a Surco `Screen`. The terminal application (`tui`) and the desktop are surfaces over it. They read keys or clicks and pass them in, carry out the work it asks for, and lay out the screen it returns. Neither decides anything the other would have to decide again.

The reason is the user's principle for this initiative: every surface plugs in and gets the behavior. `control` did that for what can be asked of Yunta. This module does it for everything a person does with Yunta at a screen: setting a repository up, following runs, answering them, starting them, reading what they cost and how their workflow is drawn, keeping the catalog and the machine, and finding any of it from one palette.

### What happens at the base commit (`92d05277`)

**The prompts live in the CLI.** `crates/cli/src/ask/` has 3,155 lines and 73 tests, with the terminal-free parts mixed into the same files as the terminal parts.

- **Terminal-free models:**
  - `DecisionCard` (`card.rs`): a decision answered as a list, with words when an option asks for them.
  - `PlanDesk` (`desk.rs`): a plan read at its desk.
  - `Round` (`form.rs`): a round of questions.
  - `Field` (`field.rs`): the line a person types on.
  - `Picker` and `Choice` (`menu.rs`): a list to pick from.
  - `Stroke` (`keys.rs`): what a keystroke asks.
  - `Press` (`held.rs`): what a stroke asks of a prompt.
- **Bound to the terminal:**
  - `Console` (`mod.rs`): termios, the alternate screen, and the SIGINT it raises again.
  - `keyboard.rs`: reading a key.
  - The hold loop in `held.rs` (`hold`, `Quiet`, `Place`).
  - `Strokes::read`, which turns a `console::Key` into a `Stroke`.
  - The terminal half of `decision.rs`, and `answer`, `ask_line` and `choose`.
- **The protocol mixes the two.** `Held`, the trait every prompt implements, draws into `Placed`, a terminal's lines with its cursor (`drawn(&Look, rows)`). The same trait takes a `Stroke` and returns a `Press`.
- **Two of the prompts' drawings exist only on a terminal.** The models draw Surco's `Card` and `Desk` components. These are not blocks of a `Doc`, and only the terminal draws them (`surface/terminal/card.rs`, `desk.rs`). `Markdown`, `Html` and a `Screen`, whose regions and overlay hold `Doc`s, cannot draw them.
- **The desk learns what was drawn while drawing it.** `PlanDesk::drawn` writes the rows it laid out into a `Cell<Seen>` (`desk.rs:329`), and the next stroke reads it. A pure model cannot do that.

**Sentences a stage would say live outside render.**

- **The chronicle's words are the CLI's.** `yunta_replay::frame::chronicle` folds a log into `Moment`s, a pure step. What each moment says lives in `crates/cli/src/surface/chronicle/`: `say`, `says_anything` and `line`, and `words.rs` with its siblings. That is about 1,100 lines plus 550 of tests.
  - The same files hold what the live region keeps of a moment (`kept`, `graduation`, `evidence`).
  - The words are a sentence any interface says. What the region keeps is how a terminal's history and pinned rows share the screen.
- **The prompts' own words are the CLI's.**
  - `init -i` asks "project name" and "base branch", each with "(enter keeps `…`)" (`commands/init.rs`).
  - `new -i` says "choose a shape" (`commands/new.rs`).
- **A shape's description is the API's.** `WorkflowShape::does` (`crates/api/src/setup/workflow.rs:65`) returns the sentence a menu shows beside each shape. A stray doc line about a removed function sits on `label`.

**Three things every surface needs are assembled in the CLI.**

- **The graph.** `crates/cli/src/graph.rs` (282 lines) derives a workflow's graph as Mermaid or DOT text. It labels each node with the state a run's frame gives it. Surco already has a `Diagram` block that draws a Mermaid flowchart on the terminal, in Markdown and in HTML, and nothing gives it a workflow.
- **A run's page.** `commands/status/mod.rs` puts render's `Run` and `Node` pages together from a `RunReading`:
  - the documents a decision shows, and what a node produced;
  - the plans, at full depth;
  - the run's places (`run_page_for`, `documents`, `page_body`).

  `page_for_a_browser` does the same for `yunta open`. Then `open.rs` writes `page.html` with `std::fs::write` and spawns the system's opener.
- **Following a run.** `commands/status/follow.rs` (349 lines) and `follow/listen.rs` (65) hold how a run is followed from another process:
  - the log, read past its last seq every 100 ms;
  - the registry, read once a second;
  - the words the sessions say;
  - the end, read from the frame, and up to 30 s of waiting for a promotion's successor along `PromotionChain`.

  Only the CLI's `status --follow` can follow a run.

**What reading hands back borrows.** `OpenedRun<'p>`, `RunReading<'p>`, `DetectedProject<'w>` and `WorkflowHistory<'_>` borrow the project or the workspace that read them. A fetched pack waits for a person's confirmation before it is installed. State that lives across inputs cannot hold a borrow.

**Setting a repository up does less than the spike's setup.**

- `Workspace::detect_project` and `DetectedProject::write` detect a repository and write `.yunta/config.yaml`, the `.gitignore` entries and the skill.
- The runners stay commented out, because "a probe names an adapter and never a model, and the model is the project's to choose" (`render/src/page/initialized.rs`). The page ends at declaring one (`declare::runners`), with `<model>` left blank.
- No pack is installed.
- The spike's setup chose a runner per role and installed a workflow pack.
- A workflow with an agent node cannot start in a repository `init` set up until a person edits the config by hand.

**Every other command already has its page and its operation.** `run-pages` put each command's page in render: `Catalog`, `Stats`, `History`, `Doctor`, `Initialized`, `NewWorkflow`, the pack pages, `Tested`, `Checked`, `Verified`, `Receipt`, `Opened`, `Collected`, `Schema` and `Signpost`. `control` put each operation in `yunta-api`:

- the catalog;
- the packs: fetch, install, update, remove, list, audit and scaffold;
- setup and a new workflow;
- the case suite;
- the field and a run's reading, stats, receipt and verification;
- history, admission, creation and detaching;
- decisions and answers;
- cancelling, closing and collecting;
- doctor.

`RunFrame::actions` says what a run offers next, and render's `command` spells each `NextStep`.

**Surco has a screen, and nothing builds one.** `surco::components::Screen` holds:

- where the person is (`trail`);
- the count at each word (`tally`);
- what it is about;
- a main region and a side region, each a `Doc`;
- the attention, the legend and an overlay.

It holds no cell, pixel or color. `Markdown`, `Description` and `Html` draw it; the terminal leaves its layout to `tui`.

**The spike answers the question in miniature.**

- A whole application is one value, `App::press(key) -> App` (`.plans/yunta-tui/spike/src/app.rs`), with ten views and three overlays.
- A breadth-first walk drew every reachable state.
- It had no type for the work it asked, and its data were constants.

### Who this is for

- **The terminal application (`tui`).**
  - It reads crossterm's keys as strokes and passes them in, along with the answers to the work it carried out through the performer.
  - It lays the returned `Screen` out with Ratatui.
  - It opens a page in a browser and leaves when asked.
  - It decides nothing about Yunta.
- **The desktop, later.** It does the same with clicks and a webview or GPUI, drawing the same `Screen`, and shows a page in a window of its own.
- **The CLI.**
  - Its inline prompts keep their look and keys, built on the models this module moves.
  - `status --follow` follows through the same following as the application.
  - `status`, `open` and `graph` build what they print the way the application builds what it shows.
  - Every command prints what it prints.

### What each of them gets

1. **State, input, work and screen, as values.**
   - **The state** is everything a person has open:
     - the stage they are on;
     - what each stage last read;
     - the run being followed;
     - the prompt in front of them;
     - an overlay;
     - the work in progress.
   - **The input** is what reaches the application:
     - a stroke from the person;
     - a size, when the surface was laid out;
     - the time, when it passes;
     - what a piece of work remarks while it works, and its outcome.
   - **`update(state, input)`** changes the state and returns the work it needs, as data. It reads no disk, clock, terminal or network.
   - **`screen(state)`** returns a Surco `Screen`.
2. **Every stage a person works in.** Each one shows render's pages in the screen's regions, offers its acts in the legend and the palette, and asks for its work through the performer.

   | Stage | What it shows | What a person does there | Through `yunta-api` |
   |---|---|---|---|
   | Signpost | Render's `Signpost`, outside a repository | Leave | `Project::in_a_repository` |
   | Setup | What was detected, then `Initialized` | Give the project's name, its base branch and its runners (each role on an adapter this machine answers for, its model typed), see what will be written, write it; then install a pack | `detect_project`, `DetectedProject::write`, the packs |
   | First run | "What do you want to build?" over the catalog, in a repository set up with no runs | Type a request and start | The catalog, then Start |
   | Field | The runs, grouped by what they need, as far as the reach asks (this repository, with the closed, or the machine) | Open a run, go to what needs a person, resume, cancel, close one or several after one confirmation, widen the reach | `Project::field`, `resume_detached`, `cancel`, `close` |
   | Run | Its ribbon, chronicle, what its sessions say, its page beside them, and a node's page when one is picked | Follow or stop following, pick a node, take every act `RunFrame::actions` offers. A promotion goes on onto its successor. A run asked at another terminal stays read-only | The following (point 7), `OpenedRun`, `RunReading` |
   | Decision | A gate on its card, a plan at its desk, a round of questions | Answer, with words where an option asks; leave it waiting | `waiting_decision`, `decide`, `answer` |
   | Start | The catalog's workflows, their inputs, modes and adapters, then admission's cautions | Type the request, pick and fill, start detached, then follow | `admit`, `create`, `start_detached` |
   | Evidence | `Receipt`, `Verified`, `Opened` | Write a finished run's receipt, verify a run's chain, open its page in a browser | `OpenedRun::receipt`, `Project::verification`, the page written by the API (point 7) |
   | Stats | A run's `Stats`, a workflow's `History` | Read what a run spent and where its time went, and a workflow's past runs | `OpenedRun::stats`, `Project::history` |
   | Graph | The workflow drawn as a `Diagram`, each node labelled with a run's state when it is a run's | Scroll | `read_workflow`, a run's manifest and frame |
   | Catalog | `Catalog`, a workflow's `Checked`, its graph and `History`; the pack pages; `NewWorkflow`; `Tested` | Run a workflow; check it; write a new one by name and shape; run the cases; install, update, remove, audit, list and scaffold packs, confirming what a pack executes | `catalog`, `verdict`, `new_workflow`, the case suite, `packs` |
   | Doctor | `Doctor`, folded or every check | Unfold, open a session per binding after a confirmation (it spends a prompt each), collect when the checkouts pass their budget | `Project::doctor`, `collect` |
   | Upkeep | `Collected`, first as what a collection would reclaim | Collect, after reading what it reclaims | `Project::collect` |
   | Schema | `Schema`, every document or one | Read the shape of a document Yunta reads | `yunta_core::shape` |

   Over every stage:

   - **The palette** lists every act the stage offers and every act always there. Each one carries the command that does the same, spelled by render's `command`, so a person learns the CLI from it. Typed words filter it.
   - **Help** lists the keys.
   - **A page taller than its region scrolls**, on every stage that shows one.
   - **A confirmation** stands before any act that stops, closes, removes or spends.

   **No stage, by nature:** `yunta mcp` is an agent's transport, `yunta rail` is what an agent's CLI runs, and `yunta completions` is a shell's script.
3. **Work carried out once.** A performer in `yunta-ui` carries out each piece of work through `yunta-api`, against the workspace and the stop it is handed.
   - It answers with inputs: what the work remarks while it works, through the API's `Remarks`, and then its outcome.
   - Some work lasts, such as following a run, and keeps answering until the state lets it go.
   - A person stops work in progress the way Ctrl-C stops a command.
   - What the work hands back is data the state owns. A borrowed handle lives in the performer for the length of one piece of work.
   - A refusal or a failure is said by render, in the stage it came from.
   - A surface spawns the performer's work on its own runtime and feeds the answers back. No surface calls the API for what a stage needs.
4. **What the application asks of its surface, apart from its work.** Leaving, and showing a page a browser reads. A terminal hands the page to the system's opener, and the desktop shows it in a window. These are the only two acts a surface does for the application.
5. **The prompts' models, for every interface.**
   - `DecisionCard`, `PlanDesk`, `Round`, the typed line, `Picker`, the menu's choice, `Stroke` and `Press` move to `yunta-ui`, each with its tests.
   - The protocol becomes terminal-free. A prompt takes a stroke, answers a press, says whether it is typing, and draws as Surco blocks.
   - The CLI keeps `Console`, the hold loop, the keyboard and the reading of a `console::Key` into a stroke. It draws the same blocks through Surco's terminal.
6. **Every surface draws a prompt.** The card and the desk become blocks of a `Doc`, and so do the typed line and the picker's list, the two other things a prompt draws.
   - The terminal draws each exactly as it draws it today, fitted to the rows it is held in, with the cursor where it stands. `Markdown` and `Html` draw them too.
   - A decision then stands in a screen's region or overlay like any page.
7. **What every surface needs, assembled once and below the surfaces.**
   - **Following a run** moves out of the CLI's follower into `yunta-api`:
     - the log past its last seq;
     - the registry;
     - the sessions' words;
     - the end, and the wait for a promotion's successor.

     The CLI's `status --follow` and the application's run stage both follow through it, at one pace and with one end.
   - **A run's page and a node's page** are put together in one place, from what the API reads, for `status`, `open` and the run stage.
   - **Writing a run's page for a browser** is the API's.
   - **The graph** moves to render: the workflow's graph as Mermaid and as DOT, and a page that holds it as a `Diagram`. The CLI's `graph` prints its text, and the graph stage shows its page.
8. **Setting a repository up declares its runners, in every interface.**
   - It proposes the roles the repository's workflows name, or the one `declare::runners` proposes when they name none, with `defaults.runner` where a node names no runner.
   - Each role goes on an adapter this machine answers for: the first by default, and any other that answered when a person picks it.
   - The person types each model, which has no default, because the model is the project's to choose.
   - `init -i` asks them on the CLI's prompts, the setup stage asks them on its own, and `DetectedProject::write` writes them declared. A non-interactive `init` writes them commented, as today.
   - Its page then says what was declared, and the step that ends it is a pack to install, through the catalog.
9. **Every sentence a stage says is render's.**
   - What a moment of the chronicle says moves to `yunta-render` (`chronicle`): `say`, `says_anything`, `line`, and every words file, with their tests.
   - What the live region keeps of a moment, and how a settled node leaves it, stays with the CLI's surface.
   - The prompts' words of `init` and `new`, and what each workflow shape does, move to render.
   - So do the words inside every prompt: its legend, its hints, what Escape does there. A prompt in `yunta-ui` keeps where it stands, and render says it as the block it draws.
   - Every new sentence the stages need is render's: the first run's question, setup's heading, the confirmations, the palette's and help's headings, and each act's description.
   - The CLI's region, the follower's lines and the application say each moment, prompt and act in the same words.

### Names and form

- **The crate is `yunta-ui` in `crates/ui`, its library `yunta_ui`.** It is named for what it is: the user interface, apart from the medium that shows it. `yunta-tui` is that interface on a terminal, and the desktop application is that interface in a window.
- **Types are named for what they are, and the plan shows every name.** Nothing is a `Model`, `Controller`, `Store` or `Manager`.
- **No name repeats one the crate reads.** Measured at the base, these candidates are taken, so the plan names each item otherwise:
  - **`Effect`**, as the name for the work the application asks for. It is `yunta_replay::frame::Effect`, what an action does, which the run stage reads.
  - **`Field`**, as the name of the typed line. It is render's field page (`page::Field`) and Surco's field row, and the field stage is the runs' field.
  - **`Choice`**, as the name of a menu's choice. It is `yunta_core::step::Choice`, how a `resolve-gate` line chooses.
  - **`Overlay`**, as the name of the palette, help or a confirmation. It is Surco's overlay region, which they are drawn into.
  - **`Catalog`, `Doctor` and `Stats`**, as the names of stages. They are render's pages, which those stages show.
- **One path to each item.** A model that moves leaves no alias in the CLI. The CLI names it at `yunta_ui::…`, and nothing re-exports it.
- **What a stage shows is render's.** A stage builds render's pages into the regions of a `Screen`. `yunta-ui` writes no sentence.

### Decisions the plan starts from

- **What moves:**
  - from `crates/cli/src/ask/`: the models, the stroke and the press, to `yunta-ui`;
  - from `crates/cli/src/surface/chronicle/`: the words, to render;
  - from `commands/status/follow.rs` and `follow/listen.rs`: following a run, to `yunta-api`;
  - from `commands/status/mod.rs` and `open.rs`: putting a run's page together, and writing it for a browser, below the surfaces;
  - from `graph.rs`: the graph, to render;
  - the prompts' words of `init` and `new`, and `WorkflowShape::does`, to render.
- **What changes for every interface:** setting a repository up declares its runners when a person answers for them (point 8).
- **What the CLI keeps:**
  - `Console`, the hold loop, the keyboard, and the reading of `console::Key` into a stroke (`Strokes`);
  - `human_interaction`, the port the attached run answers through;
  - the live region and what it keeps of a moment;
  - each command's arguments, output mode and exit code, and its choice of which prompt to open;
  - spawning the system's opener for `yunta open`.
- **What Surco gains:** the card and the desk as blocks, drawn by the terminal byte for byte as today, and by `Markdown` and `Html`.
- **What scrolls comes back in as input.** The rows a surface laid out of the desk's pane, or of a page's region, reach `update` as an input, so drawing changes nothing.
- **The order of the work is the plan's:**
  1. the crate and the moves, each with what prints held unchanged;
  2. the blocks;
  3. the words to render;
  4. following, the run's page and the graph, below the surfaces;
  5. then the stages, one slice at a time, each with its work and the walk that holds it:
     - watch (signpost, field, run);
     - decide;
     - start (first run, start);
     - setup;
     - the catalog and its packs;
     - evidence, stats and the graph;
     - doctor and upkeep;
     - schema;
     - the palette, help and confirmations over all of them.

### Assumptions

1. **The application never hosts a run,** as the map decides it. Starting one is `start_detached`. Answering one hands it to a detached resume, as `decide` and `answer` already do.
2. **The performer is async, on `tokio` as the API is,** and every IO it does goes through `yunta-api`.
3. **The module keeps its id, `app-core`,** as `control` kept its id when its crate became `yunta-api`.
4. **Time and size are inputs.** The time feeds what a stage says of age and elapsed time. A running mark turns with the events that arrive, as Surco's `motion` holds: only evidence moves, and no clock turns it.
5. **A stage does what its command does, no more and no less.** The same operation and the same refusals, and an act the CLI confirms with a flag is confirmed on the screen. This holds the user's rule that every interface behaves the same.
6. **The tests run cases and the doctor's sessions are work that lasts.** Their remarks reach the screen while they work, and a person can stop them.
7. **`yunta review` at a terminal opening the application at its decision is the `tui` module's,** with bare `yunta`, as the map places the deletion of the duplicated full-screen loop. This module makes the stage that opens at a decision.
8. **The module lands in phases,** each a commit with the suite green, in the initiative's one pull request.
9. **`tasks/plan.md` and `tasks/todo.md` are replaced for this module.** Those of `control` stay in its commits, the last `92d05277`.
10. **The map changes with this spec:**
    - `yunta-app` becomes `yunta-ui` wherever it is named;
    - the `app-core` row lists every stage;
    - the `tui` row lays out every stage this module makes.

### Found while measuring

- **No surface but the terminal draws a prompt.** Surco's `Html` and `Markdown` have no drawing for `Card` or `Desk`. Point 6 holds it.
- **The desk writes while it draws** (`desk.rs:329`). Point 5's protocol and the size input hold it.
- **A round of questions checks its answers through core** (`AnswersFile::against`, `form.rs:444`), which `yunta-ui` may name. No move to the API is needed.
- **Following, a run's page and the graph exist only in the CLI.** Point 7 holds it.
- **Some of what a stage says is not render's.** The prompts of `init` and `new` are the CLI's, a shape's description is the API's, and every prompt's model writes its own legend and hints (`Escape::does`, `NEEDS_WORDS`, `SKIP`, `FOLDED`). Point 9 holds it.
- **A card fits its rows, as the desk does.** `Card::drawn(look, rows)` and `Desk::drawn(look, rows)` both draw to a terminal's height. Point 6 holds it: a block drawn in a document takes the room it needs, and a prompt held on a terminal is drawn fitted.
- **Several names are taken.** "Names and form" holds it.
- **Setting a repository up stops short of a first run.** With its runners commented out, a workflow with an agent node cannot start until a person edits the config. Point 8 holds it, as the user chose.

### Out of scope

- The `yunta-tui` crate, Ratatui, crossterm, `deny.toml`, and bare `yunta` opening the application: the `tui` module.
- The desktop.
- Any change to what a command prints or exits with, beyond `init -i` asking and writing the runners and its page saying them. The prompts keep their look.

## Tech Stack

Rust 1.98.0. A new workspace member on the crates the map names (`yunta-core`, `yunta-replay`, `yunta-render`, `yunta-api`, `yunta-surco`), with `tokio` for the performer as the API has it. No crate from outside the workspace is added. The CLI's manifest gains `yunta-ui`.

## Commands

`cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.

```bash
# Before editing a file, and before finishing with no paths
cargo xtask docs --about <files>

# Test
cargo test -p yunta-ui
cargo test -p yunta-surco --features golden
cargo test -p yunta-render
cargo test -p yunta-api
cargo test -p yunta --bins
cargo test -p yunta --test <target>

# Lint and checks
cargo xtask smells --check
cargo xtask docs --check
cargo deny check

# The suite, in groups, and the gate, every other step of the ci job
python3 .plans/yunta-tui/tools/suite.py <group> <log dir>
sh .plans/yunta-tui/tools/gate.sh /Users/eulke/development/yunta-tui <log dir> <step ...>
```

## Project Structure

```
crates/ui/Cargo.toml             the crates the map names, tokio, and nothing that knows a terminal
crates/ui/src/lib.rs             the crate's map: the state, its input, update, the work, the screen, the performer
crates/ui/src/                   one module per stage, the overlays, the prompts' models, the work and its performer
crates/ui/tests/                 the walk over every reachable state; each stage against a sandbox
crates/surco/src/components/     the card and the desk as blocks
crates/surco/src/surface/        their drawing by Markdown and Html; the terminal's, unchanged
crates/render/src/chronicle/     what a moment says, from the CLI's surface/chronicle
crates/render/src/               the graph; the prompts' and the stages' words
crates/api/src/                  following a run; a run's page put together and written for a browser
crates/cli/src/ask/              Console, the hold loop, the keyboard, Strokes: the terminal's part
crates/cli/src/surface/          the live region, and what it keeps of a moment
crates/cli/src/commands/         each command's arguments, output and exit code, on what moved
.agents/architecture.md          the crate table: yunta-ui's row, and yunta's edges
CAPABILITY-MAP.md                yunta-ui where it said yunta-app; the app-core and tui rows
```

## Code Style

A surface reads its input, hands it in, carries the work out, and lays out what comes back:

```rust
// crates/tui/src/main.rs, in the module after this one
let work = interface.update(Input::Stroke(stroke));
for asked in work {
    let performer = performer.clone();
    let answers = answers.clone();
    tokio::spawn(async move { performer.perform(asked, answers).await });
}
terminal.draw(|frame| lay_out(frame, &interface.screen()))?;

// crates/ui/src/stage/decision.rs
match self.card.press(stroke) {
    Press::Answer(Reply::Chose(answer)) => vec![Work::Decide {
        run: self.run.clone(),
        option: answer.option,
        words: answer.words,
    }],
    Press::Decline => self.leave(),
    _ => Vec::new(),
}
```

The names are illustrative. The plan names each item by what it is, apart from the names "Names and form" finds taken.

## Testing Strategy

- **The walk.** A test starts from recorded readings of every stage and walks every state reachable with every stroke and every answer to its work. It asserts that:
  - `update` never panics;
  - every piece of work is one the performer knows;
  - every screen draws with `Html` and with `Markdown`, which holds the desktop possible as the map asks;
  - every act the palette lists is one the stage takes, and its command is the one render spells.
- **Each stage against a sandbox,** through the performer and a workspace built in the test, with no process read:
  - Watch follows a mock run to its end, and a promotion onto its successor.
  - Decide answers a parked gate and a questions node.
  - Start admits, creates and starts a mock run detached.
  - Setup detects and writes a sandbox repository with its runners declared, and the CLI's `init -i` writes the same config from the same answers.
  - The catalog checks, writes, runs the cases of, and installs a local pack into a sandbox.
  - Evidence, stats and the graph read a finished mock run.
  - Doctor and upkeep read and collect a sandbox's checkouts.
- **One following.** The CLI's `status --follow` tests pass unchanged on the API's following. A test follows the same log through the application and gets the same end.
- **The dependencies.** A test reads `yunta-ui`'s manifest and refuses `crossterm`, `console`, `nix`, `indicatif`, `ratatui`, the engine, the store and the adapters.
- **The guard of every move is what prints:**
  - the goldens, byte for byte;
  - the prompts' tests, moved by name;
  - the captures of `pages.sh` and the CLI's integration tests, unchanged;
  - the chronicle's and the graph's tests, now render's.
- **The new blocks and pages.** Their goldens in every environment, beside the catalog's. The terminal's drawing of the card and the desk is equal to today's.
- The suite's count is explained against the base's, by name.

## Boundaries

- **Always:**
  - move a file with `git mv` before editing it;
  - keep every golden byte for byte, and the CLI's prompts as they look;
  - keep the workspace compiling and passing at every step;
  - every sentence from render, every IO through `yunta-api`.
- **Ask first:**
  - a change to what a command prints, or to a prompt's keys;
  - a new dependency of any crate.
- **Never:**
  - `yunta-ui` naming a terminal, a window, the engine, the store or the adapters;
  - `update` reading a disk, a clock, a terminal or the network;
  - a screen holding a cell, a pixel or a color;
  - a re-export or an alias left for a caller;
  - a name that repeats one the crate reads;
  - the application hosting a run;
  - a stage doing what its command does not.

## Success Criteria

1. **The crate `yunta-ui` is a workspace member.** The map, `.agents/architecture.md`'s table and the crate's own map say so, which `cargo xtask docs --check` holds.
2. **It knows no terminal and no engine.** Its `[dependencies]` name none of `crossterm`, `console`, `nix`, `indicatif`, `ratatui`, `yunta-engine`, `yunta-storage` or `yunta-adapters`, and a test holds it.
3. **`update` is pure and its work is data.**
   - The walk reaches every state of every stage from its readings.
   - Every screen it reaches draws with `Html` and `Markdown`.
   - Every piece of work it asks for, the performer carries out through `yunta-api`.
4. **Every stage of point 2 works end to end,** through the performer, against a sandbox, as the testing strategy lists. The palette offers every act of each stage with its command.
5. **The prompts' models are `yunta-ui`'s.**
   - The CLI's `ask/` holds only the terminal's part.
   - Every prompt prints and answers as before: its tests moved by name, its goldens byte for byte.
6. **The card and the desk are blocks every surface draws.** The terminal's drawing is unchanged, and `Markdown` and `Html` draw them, with goldens.
7. **What every surface needs is assembled once.**
   - Following a run is the API's.
   - A run's page has one assembly, and so does the graph, now render's.
   - The CLI's `status --follow`, `status`, `open` and `graph` print as before.
8. **Setting up declares the runners.** `init -i` and the setup stage ask each role's adapter and model, and write them declared. A non-interactive `init` writes them commented, and its page is unchanged.
9. **Every sentence is render's.**
   - The CLI's surface holds no sentence of a moment, and no prompt's words.
   - The API holds no shape's description.
   - The region, the follower's lines and the application say a moment alike.
   - The captures equal the base's.
10. **The gate passes:** every step of the ci job, with the suite's count explained.

## Open Questions

None. The last one was decided on 2026-11-10: setting a repository up declares its runners, in every interface (point 8). The user chose it over a setup stage that does only what `init` does today.
