# Tasks: `app-core`

Spec: `SPEC-app-core.md`. Plan: `tasks/plan.md`. Status: **approved on 2026-11-10, in four phases.** Base: `85f9c8a3` (`feature/yunta-tui`; `92d05277` with control's last checks).

**How these run:**
- One at a time, with `incremental-implementation` and `test-driven-development`, in four phases, each ending at a checkpoint for the user.
- Inside a phase, each group of tasks ends at a group check: the suite's groups green, and the digests and captures against the base where the group touches them. A group check does not stop for the user.
- Inside a phase, the work stops for the user only on a decision that is theirs: a printed text that would change beyond what this list names, a choice neither the plan nor this list makes, or a failure whose cause is not found.
- `cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.
- Before editing a file, `cargo xtask docs --about <files>`; each rule, trap and note it prints is kept true.
- A commit closes each phase, once the user approves its checkpoint and every step of the `ci` job passes. Paths are staged by name. Then `cargo clean`.
- The engine's and the CLI's suites run in foreground batches: `python3 .plans/yunta-tui/tools/suite.py <group>`.

**How an item moves:**
- Its names and its form change first, in place, with every caller. Then it moves, with every caller's path.
- Neither step leaves an alias, a re-export or an old name, and the workspace compiles and passes after each task.
- A task that moves a file does it with `git mv` before it edits it. A file that is only partly moving is split: the moving part goes, the rest stays.
- A task that renames or moves is larger than five files: one path to each item means a name and its callers change in one step. Each such step is mechanical, the compiler names every caller, and the moved bodies are read with `git diff -M` against their old files.

**How a prompt splits** (the plan's decision 1):
- `yunta-ui` keeps where a prompt stands and what a stroke does there. It writes no sentence.
- `yunta-render`'s `prompt` takes where it stands, as plain data, and gives back the Surco component, worded.
- Surco draws the component: in a document, and held at a terminal's height with the cursor.
- The CLI's `ask` holds it on a terminal.

**Where each test lives:**
- A stage's own behavior is tested in `crates/ui/tests/`, through `State` alone, or through the `Performer` against a workspace built in the test.
- Work that spawns Yunta's binary is tested in `crates/cli/tests/`, where `CARGO_BIN_EXE_yunta` exists, driving `yunta_ui` the same way. That work is a run started or resumed detached, and a decision handed to a detached resume.
- The walk lives in `crates/ui/tests/walk.rs`.

**What fails first:**
- A move has no new behavior, so no test can fail for it. Its guard is every test and golden it touches, passing unchanged, and the base's digests and captures (Tasks 1 to 3).
- A new behavior, a new test or a changed text is seen red, for its own cause, before it passes.

**Decided in this list, beyond the plan, approved by the user with it:**
- **`yunta resolve-gate` at a terminal answers a run parked on questions with the round** (Task 23).
  - Today only the attached run, and an agent over MCP's `answer_questions`, can answer it.
  - The decision stage needs the same reading, and every interface behaves the same.
  - It changes what `resolve-gate` does on such a run, which the user approved with this list.
- **What the control plane answers in a sentence is said by render** (Task 24). `run_workflow`, `resume_run` and `answer_questions` build their answers in `yunta-mcp` today. Their text stays the same.
- **`WorkflowShape` moves to core** (Task 15).
  - Render says what each shape does, and render cannot read `yunta-api`.
  - The skeleton each shape writes stays the API's.
- **The page that shows what setup found, before anything is written, is `page::Setup`** (Task 44). `Detected` is core's.

## Phase 1: the crate, and the moves that print the same

Its groups, each green before the next: the base; the words to render; every prompt a block; the prompts to `yunta-ui`.

### Task 1: the proof's tools, pointed at this module

**Description:**
- `goldens.py`, `pages.sh`, `normalize.py`, `documents.py`, `mcp_answers.py` and `prints.sh` are copied from `proof/control/` into `.plans/yunta-tui/proof/app-core/`, unchanged but for their folder.
- `prompts.py` drives the CLI's prompts at a pseudo-terminal (Python's `pty`) in a sandbox of its own, sending keys and keeping every byte written:
  - `init -i`, taking the defaults and then typing over them;
  - `new -i`, picking the second shape after filtering;
  - `resolve-gate` on a parked gate, moving and choosing an option that asks for words;
  - `review` on a run parked on a plan, turning the proof and picking by number;
  - a round of questions, as the attached `yunta run` of a questions workflow asks it.
- `follow.sh` captures `status --follow` through a pipe, on a finished mock run and on a parked one.
- `prints.sh` runs them all into one capture directory, which `normalize.py` compares whole.

**Found in its turn:** `goldens.py` covers Surco's goldens too (132 on the base), since this module adds Surco blocks; each prompt and follower capture names its run's whole id, so the handles it shows are masked.

**Fails first:** none: tools.

**Acceptance criteria:**
- [x] Each tool runs twice on the base and finds nothing between its two runs, once normalized.

**Verification:**
- [x] Manual check: `python3 normalize.py` over the two runs prints no difference.

**Dependencies:** None

**Files likely touched:** `.plans/yunta-tui/proof/app-core/{goldens.py,pages.sh,normalize.py,documents.py,mcp_answers.py,prompts.py,follow.sh,prints.sh}` (outside git)

**Estimated scope:** M

### Task 2: what the base prints

**Description:**
- The suite's binaries and tests by name, in `suite.py`'s groups, into `proof/app-core/base-suite/`.
- Every golden's digest, `digests-base.txt`.
- A release build, `proof/app-core/yunta-base`, and its size. A symbol build, for `tools/size_by_crate.py`.
- `prints.sh` from the release build into `base-1/`, then again into `base-2/`.

**Fails first:** none: measurements.

**Acceptance criteria:**
- [x] `base-1` and `base-2` are equal once normalized; the counts and sizes are written in `baseline.txt`.

**Verification:**
- [x] Manual check: `python3 normalize.py base-1 base-2` prints no difference.

**Dependencies:** Task 1

**Files likely touched:** `.plans/yunta-tui/proof/app-core/` (outside git)

**Estimated scope:** S

### Task 3: a real run followed on the base

**Description:**
- fragua is started detached from the base's release binary, on the CLI and model `control`'s runs used (conform on `gpt-6-luna`). It is followed with `yunta status --follow` through a pipe from its start, and stopped before its `pr` node.
- Its gates are answered with `yunta resolve-gate`. Its time between nodes and its tokens are read from `yunta stats <run>`.
- The follower's lines, normalized for ids and times, go into `real-base-follow.txt`. The run's id, time and tokens go into `baseline.txt`.

**Found in its turn:** run `01M72VBQBW4PYXA5YQFX8VRA0Q` waited at `ship` after 211 s with nothing asked before it; `between_secs` 0.556 s; 2.91M tokens, 2.21M cached; the follower wrote 176 lines and ended where the run parked (exit 3).

**Fails first:** none: a measurement.

**Acceptance criteria:**
- [x] The run reaches the node before `pr`; the follower ended where the run stopped; `between_secs` and tokens are recorded.

**Verification:**
- [x] Manual check: `yunta stats <run>` read back from `baseline.txt`.

**Dependencies:** Task 2

**Files likely touched:** `.plans/yunta-tui/proof/app-core/real-base*` (outside git)

**Estimated scope:** S

### Group check: the base
- [x] The base's suite, digests and captures stand, twice equal.
- [x] The real run's follower lines, time and tokens are recorded.

### Task 4: the crate `yunta-ui`

**Description:**
- `crates/ui`, package `yunta-ui`, library `yunta_ui`, joins the workspace.
  - Its manifest names core, replay, render, api, surco and `tokio`, each from the workspace's table.
  - Its `//!` says what the crate is: the user interface, apart from its medium, writing no sentence and knowing no terminal. Its map grows with each task that adds to it.
- `crates/ui/tests/no_terminal_in_the_interface.rs` reads the manifest's `[dependencies]` and refuses `crossterm`, `console`, `nix`, `indicatif`, `ratatui`, `yunta-engine`, `yunta-storage` and `yunta-adapters`, naming where such a reading belongs, as `no_engine_in_the_cli.rs` does.
- The crate is added to the `ci` job's isolation loop and to `tools/gate.sh`'s.
- `.agents/architecture.md` gains `yunta-ui`'s row and `yunta`'s edge to it. The row's bytes come out of what the file says twice or a `//!` already says. Without such a sentence, the work stops for the user rather than raising the cap.

**Found in its turn:** `yunta`'s row gains `ui` when the CLI first depends on the crate (Task 10), so the table holds its manifests at every task. The row's bytes came from two sentences the API's and `process/mark.rs`'s `//!` already say and from "the CLI and the adapters draw", no longer the whole list: 6,116 of 6,144 bytes.

**Fails first:** the manifest test, with `console` added to the manifest for the run: it fails naming `console`; then `console` comes out.

**Acceptance criteria:**
- [x] `cargo check -p yunta-ui` passes; the manifest test passes on the real manifest.
- [x] `cargo xtask docs --check` passes, with the cap unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui`
- [x] Manual check: the manifest test's message names the crate refused and where its reading belongs.

**Dependencies:** Group check: the base

**Files likely touched:** `Cargo.toml`, `crates/ui/{Cargo.toml,src/lib.rs,tests/no_terminal_in_the_interface.rs}`, `.github/workflows/ci.yml`, `.agents/architecture.md`

**Estimated scope:** M

### Task 5: what a moment of the chronicle says is render's

**Description:**
- `crates/cli/src/surface/chronicle/` moves to `crates/render/src/chronicle/` with `git mv`: `mod.rs`'s words, `words.rs`, `task_words.rs`, `rail_words.rs`, `scope_words.rs`, `baseline_words.rs`, `format_words.rs`, `capability_words.rs`, and their tests.
- `Said`, `say`, `says_anything`, `line` and `evidence` become `pub` at `yunta_render::chronicle::<item>`, one path each.
- `evidence` draws its failure's blocks through `surco::surface::Terminal`, where it called the CLI's `crate::output::blocks_lines`.
- What a terminal keeps above its region stays in the CLI: `kept` and `graduation` go to `crates/cli/src/surface/scrollback.rs`, the terminal's own history, with the tests that test them (`kept_from_event`'s).
- `draw.rs` and `lines.rs` call render. Render's map names `chronicle`, and `pause`'s `//!` stops pointing at the CLI.

**Found in its turn:** `kept` and `graduation` went to `surface/draw.rs`, whose `//!` is what a surface keeps and where it puts it, rather than `scrollback.rs`, the writer. `evidence` hands back its blocks, and the CLI draws them: render draws with no surface here. Three tests that held both a sentence and a keeping split by owner; `lines.rs`'s two promotion tests, which test words, moved to render; the long test `draw.rs` held inline, counted once its module became a file, split into one test per promise.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] `crates/cli/src/surface/chronicle/` is gone; the CLI writes no sentence of a moment.
- [x] The 26 tests pass under render, but those of `kept`, which pass in the CLI, each by name.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render`, `cargo test -p yunta --bins`, `suite.py cli-1..5`
- [x] Manual check: `git diff -M --stat`; `render/tests/pure.rs` passes.

**Dependencies:** Task 4

**Files likely touched:** `crates/cli/src/surface/chronicle/**` → `crates/render/src/chronicle/**`, `crates/cli/src/surface/{mod.rs,draw.rs,lines.rs,scrollback.rs}`, `crates/render/src/{lib.rs,pause.rs}`

**Estimated scope:** L (mechanical)

### Task 6: a moving run's rows are render's

**Description:**
- `crates/cli/src/surface/view/` (`mod.rs`, `said.rs`, `tests.rs`, `said/tests.rs`) moves to `crates/render/src/watched/` with `git mv`. It holds:
  - the row that asks for a person (`asked`, `attention`);
  - each working node's rows with what its session said (`node_rows`, `working`);
  - the footer and the title, and the answering command.
- The items the region calls become `pub` at `yunta_render::watched::<item>`. `QUIET_AFTER`, `DETAIL_DEPTH` and `FURROW_LEAST` move with their reasons.
- The CLI's `region.rs` calls render. Render's map names `watched`.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] `crates/cli/src/surface/view/` is gone; its 23 tests pass under render by name.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render`, `suite.py cli-1..5`

**Dependencies:** Task 5

**Files likely touched:** `crates/cli/src/surface/view/**` → `crates/render/src/watched/**`, `crates/cli/src/surface/{mod.rs,region.rs}`, `crates/render/src/lib.rs`

**Estimated scope:** M

### Group check: the words to render
- [x] The suite's groups green; digests and captures equal to the base's, the follower's capture among them.

### Task 7: the card is a block

**Description:**
- `surco::doc::Block::Card(Card)`.
- **The terminal draws it two ways.**
  - In a document, every option shows.
  - Held, `Card::drawn(look, rows)` becomes `Terminal::held(&Block, rows) -> Placed`, its bytes and cursor as today.
- **A typed row holds the whole text, split at the cursor.**
  - The terminal fits it to its room, as the CLI's `Field::parts` and `fitting` did.
  - The CLI's card and round hand over the whole text where they handed over the fitted parts.
- **`Markdown` and `Html` draw a card:** its question, edge and about, its choices with the one pointed at, the words being typed, a round's rows, and its legend.
- Goldens for the three bodies (choosing, saying, a round) in every environment, beside the card's terminal goldens.

**Found in its turn:** The catalog of every block gained a card, so its goldens (components/catalog.*, markdown/catalog.md, html/catalog.html) grew by it: every other golden is the base's. A round's card is now built with no look: what a field shows is fitted where it is drawn. The fitting is `TypedRow::fitted`, in Surco's terminal module `typed`, with the two tests that held it in the CLI.

**Fails first:** the card's `Markdown` and `Html` goldens (no drawing yet), and a card in a document drawn whole by the terminal.

**Acceptance criteria:**
- [x] `crates/surco/goldens/card/` holds its five terminal goldens unchanged and the new ones; the terminal's card and round tests pass unchanged.
- [x] The CLI's prompts' captures equal the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco --features golden`, `cargo test -p yunta --bins`
- [x] Manual check: the new goldens read as a card would in a browser and a Markdown file.

**Dependencies:** Group check: the words to render

**Files likely touched:** `crates/surco/src/{doc.rs,components/card.rs,surface/terminal/{mod.rs,card.rs,card/round.rs},surface/markdown/mod.rs,surface/html/blocks.rs}`, `crates/surco/goldens/card/`, `crates/cli/src/ask/{field.rs,card.rs,form.rs}`

**Estimated scope:** L

### Task 8: the desk is a block

**Description:**
- `surco::doc::Block::Desk(Desk)`.
- **The terminal draws it two ways.**
  - In a document: its title, its rail as a list, its whole pane, and its foot.
  - Held: `Desk::drawn(look, rows)` becomes the held drawing, at the exact width the CLI's `PlanDesk::look` asked for, its bytes as today.
- **What a held desk laid out is `surco::components::PaneLaid { top, page, most }`.** `LaidDesk` carries it beside its `Placed`, as a surface reports it to the model.
- **`Markdown` and `Html` draw a desk,** and goldens of the desk are added in every environment: the terminal had none.

**Found in its turn:** A desk's pane held lines render had laid out at a terminal's pane measure, so a desk's model needed the terminal's exact width and a page drew it as a terminal's transcript. The pane now holds its `Doc` (`Pane::doc`), and whether the row a pointer stands on stays in sight (`follows_pointer`): the terminal lays it out at the pane's measure and finds the pointer there, as render did, so render's desk goldens are unchanged byte for byte; Markdown and HTML draw it as the document it is. `Placed` carries what a held desk laid out of its pane (`PaneLaid`); `LaidDesk` is gone. Under 90 columns a desk in a document has no rail, as a held one has none.

**Fails first:** the desk's goldens, in every environment.

**Acceptance criteria:**
- [x] The terminal's desk tests pass unchanged; the CLI's `review` capture equals the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco --features golden`, `cargo test -p yunta --bins`

**Dependencies:** Task 7

**Files likely touched:** `crates/surco/src/{doc.rs,components/desk.rs,surface/terminal/desk.rs,surface/markdown/mod.rs,surface/html/blocks.rs}`, `crates/surco/goldens/desk/`

**Estimated scope:** M

### Task 9: the typed answer and the choice list are blocks

**Description:**
- **`surco::components::TypedAnswer { asked: Line, typed: TypedRow }`, as `Block::TypedAnswer`:** a line asked and the answer typed under it, which `ask_line` draws.
- **`surco::components::ChoiceList { choices: Option<Choices>, filter: Option<String>, legend: Legend }`, as `Block::ChoiceList`:** what `choose` draws through `narrowed`.
- **The terminal draws each in a document and held,** their held bytes and cursor as `LineAsked::drawn` and `Picker::drawn` draw them today. `Markdown` and `Html` draw them, with goldens.

**Found in its turn:** `Terminal::held` holds every prompt block; the line asked and the picker draw through it, and the list's `AROUND` went to Surco with them. A test that sizes a short terminal names its rows. Split test files are named `tests.rs`, beside the module they test, since the smell counters read any other file under `src` as production code.

**Fails first:** the two blocks' goldens.

**Acceptance criteria:**
- [x] The `init -i` and `new -i` captures equal the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco --features golden`, `cargo test -p yunta --bins`

**Dependencies:** Task 8

**Files likely touched:** `crates/surco/src/{doc.rs,components/mod.rs,components/typed_answer.rs,components/choice_list.rs,surface/terminal/{mod.rs,card.rs},surface/markdown/mod.rs,surface/html/blocks.rs}`, `crates/surco/goldens/{typed_answer,choice_list}/`

**Estimated scope:** L

### Group check: every prompt a block
- [x] The suite's groups green; the goldens equal the base's but the new ones; every prompt's capture equals the base's.

### Task 10: a stroke, a press and an answerable prompt are `yunta-ui`'s

**Description:**
- `Stroke` moves from `ask/keys.rs` to `crates/ui/src/prompt/stroke.rs`. `Strokes`, which reads a `console::Key`, stays, with its bracketed paste and its six tests.
- `Press<T>` moves from `ask/held.rs` to `crates/ui/src/prompt/press.rs`.
- **The terminal-free half of `Held` becomes `yunta_ui::prompt::Answerable`.**
  - It has `type Answer`, `press`, `typing`, `block(&self, look) -> Block`, and `laid(&mut self, PaneLaid)`, whose default does nothing.
  - `Held::look` goes: the held desk is drawn at its exact width by Surco.
- **The CLI's `hold<P: Answerable>`** draws `Terminal::held(&prompt.block(look), rows)`, and hands a desk what it laid out.
- The five prompts, still in the CLI, implement `Answerable`. Each `block` returns its component as a block, worded as it is today.
- The CLI depends on `yunta-ui`.

**Found in its turn:** `Terminal::held` takes the terminal's columns too: a desk is held at the terminal's exact width, which `Held::look` gave it before, so the loop knows nothing of desks; the CLI's test of that width moved to Surco as `a_held_desk_is_as_wide_as_the_terminal`. With `laid` in place, the desk's `Cell<Seen>` went in this task: it keeps the `PaneLaid` it was handed, and its tests hand it back as the loop does. A shown document in the desk's pane is built with the look it is given: no page of one reads the width.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] `ask/held.rs` holds only `hold`, `Quiet`, `Place` and their arithmetic; its three tests pass.
- [x] Every prompt's capture equals the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui`, `cargo test -p yunta --bins`, `suite.py cli-1..5`

**Dependencies:** Group check: every prompt a block

**Files likely touched:** `crates/ui/src/{lib.rs,prompt.rs,prompt/{stroke.rs,press.rs,answerable.rs}}`, `crates/cli/Cargo.toml`, `crates/cli/src/ask/{held.rs,keys.rs,mod.rs,card.rs,desk.rs,form.rs,field.rs,menu.rs}`

**Estimated scope:** L (mechanical)

### Task 11: what Escape does, and the words every prompt shares, are render's

**Description:**
- `Escape` (`Parks`, `LeavesWaiting`, `KeepsDefault`, `TakesTheFirst`), with `said` and `does`, moves to `yunta_render::prompt::Escape`.
- `ESC`, `ANSWER` and `FOLDED`, and `list_legend`, move to render's `prompt`.
- `no_terminal`'s sentence moves to render. Its fallbacks are said there too: "using detected defaults", "defaulting to `one-node`".
- Every caller names render's.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] `git grep -n 'enum Escape' crates/cli` finds nothing; the prompts' captures equal the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render`, `cargo test -p yunta --bins`

**Dependencies:** Task 10

**Files likely touched:** `crates/render/src/{lib.rs,prompt.rs}`, `crates/cli/src/ask/{mod.rs,field.rs,menu.rs,card.rs,desk.rs,form.rs,decision.rs}`, `crates/cli/src/commands/{init.rs,new.rs,review.rs,resolve_gate.rs}`, `crates/cli/src/human_interaction.rs`

**Estimated scope:** L (mechanical)

### Task 12: the typed line and the picker are `yunta-ui`'s

**Description:**
- `ask/field.rs`'s model moves to `crates/ui/src/prompt/typed_line.rs`. `Field` is renamed `TypedLine`: `Field` is render's page and Surco's row. `Typed` and `Edit` keep their names.
- `LineAsked` moves to `prompt/line_asked.rs`. Its block is render's `prompt::typed_answer`.
- `Picker<T>` moves to `prompt/picker.rs`, and `Choice<T>` is renamed `Alternative<T>`: `Choice` is core's `step::Choice`. Its block is render's `prompt::choice_list`, with its legend's words.
- The CLI's `ask_line` and `choose` hold them.

**Found in its turn:** What the CLI keeps of the two files is in `ask/line.rs` (`ask_line`) and `ask/choose.rs` (`choose`), named for the calls that hold them. `LineAsked` owns the line it asks, so it is a prompt any surface holds. The picker's list and filter are public while the round that reads them is the CLI's (Task 13).

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] The eight tests of the typed line and the eleven of the picker pass under `yunta-ui` by name.
- [x] The `init -i` and `new -i` captures equal the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui`, `cargo test -p yunta --bins`, `suite.py cli-1..5`

**Dependencies:** Task 11

**Files likely touched:** `crates/ui/src/prompt/{typed_line.rs,line_asked.rs,picker.rs,picker/tests.rs}`, `crates/render/src/prompt.rs`, `crates/cli/src/ask/{field.rs,menu.rs,mod.rs}`, `crates/cli/src/commands/new.rs`

**Estimated scope:** L (mechanical)

### Task 13: the round is `yunta-ui`'s

**Description:**
- `ask/form.rs`'s model moves to `crates/ui/src/prompt/round.rs`, and `Settled` is renamed `SettledAnswers`: `Settled` is core's.
- Its words move to render's `prompt::round`: `SKIP`, `NOTHING_KEPT`, its legend, `said_of`, `note`, `shown`, `violations`. The round hands render where it stands, and render builds its `Card`.
- The CLI's `answer` holds it.

**Found in its turn:** Render's `prompt::round` builds the round's card (`card`, `legend`) and says its words (`SKIP`, `NOTHING_KEPT`, `note`, `boolean`); the model hands it the rows and where it stands. The CLI keeps `answer` in `ask/round.rs`. `yunta-ui` gains a dev-dependency on core's `testkit` for the samples its tests build.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] The nineteen tests of the round pass under `yunta-ui` by name; the round's capture equals the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui`, `cargo test -p yunta-render`, `suite.py cli-1..5`

**Dependencies:** Task 12

**Files likely touched:** `crates/ui/src/prompt/{round.rs,round/tests.rs}`, `crates/render/src/prompt/round.rs`, `crates/cli/src/ask/{form.rs,mod.rs}`, `crates/cli/src/human_interaction.rs`

**Estimated scope:** L (mechanical)

### Task 14: the decision card is `yunta-ui`'s

**Description:**
- `ask/card.rs`'s model moves to `crates/ui/src/prompt/decision_card.rs`, and `Answer` is renamed `PickedOption`: `Answer` is core's (questions) and Surco's (attention). `Reply` keeps its name.
- Its words move to render's `prompt::card`: `NEEDS_WORDS`, `A_NOTE`, and the legend's "move", "chooses", "adds a note", "reads the plan", "sends it", "records it" and "goes back".

**Found in its turn:** Render's `prompt::card` says the card's keys (`choosing_legend`, `saying_legend`) and builds the words being said (`saying`, with `NEEDS_WORDS` and `A_NOTE`).

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] The card's twelve tests pass under `yunta-ui` by name; the `resolve-gate` capture equals the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui`, `suite.py cli-1..5`

**Dependencies:** Task 13

**Files likely touched:** `crates/ui/src/prompt/{decision_card.rs,decision_card/tests.rs}`, `crates/render/src/prompt/card.rs`, `crates/cli/src/ask/{card.rs,decision.rs,mod.rs}`

**Estimated scope:** M

### Task 15: the desk is `yunta-ui`'s, and a shape is core's

**Description:**
- `ask/desk.rs`'s model moves to `crates/ui/src/prompt/plan_desk.rs`, with `Asked`, `Leaving` and `Left`.
  - `Seen` and its `Cell` go: `laid` takes what the surface laid out, and `scroll` reads it.
  - Its words move to render's `prompt::desk`: its title, edge, foot, legend and what leaving does.
- **`WorkflowShape` moves to core's `workflow` module, with `parse`, `all` and `label`.**
  - What each shape does, `does`, moves to render's `prompt`, with "choose a shape".
  - The skeleton each shape writes stays the API's, as a function of the shape.
  - The stray doc line on `label` goes.
- **The words `init -i` asks with move to render's `prompt`:** "project name", "base branch", "(enter keeps `…`)".

**Found in its turn:** The desk's `Cell<Seen>` had gone in Task 10. Render's `prompt::desk` says its title, edge and keys (`DeskKeys`). `WorkflowShape` is `yunta_core::WorkflowShape`, in `workflow/skeleton.rs`, its `parse` refusing with core's `SetupRefusal`; the skeletons stay the API's as `skeleton(shape, stem, lint)`. `PlanDesk` keeps its borrow of render's `Reading`: how a stage that owns its decision holds one is Phase 3's.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] The desk's thirteen tests pass under `yunta-ui` by name, with the ones that read `Seen` reading what `laid` was handed.
- [x] The `review`, `init -i` and `new -i` captures equal the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui -p yunta-core -p yunta-api`, `suite.py cli-1..5`

**Dependencies:** Task 14

**Files likely touched:** `crates/ui/src/prompt/{plan_desk.rs,plan_desk/tests.rs}`, `crates/render/src/prompt/desk.rs`, `crates/core/src/workflow/{mod.rs,skeleton_shape.rs}`, `crates/api/src/{setup.rs,setup/workflow.rs,lib.rs}`, `crates/cli/src/ask/{desk.rs,decision.rs}`, `crates/cli/src/commands/{init.rs,new.rs}`

**Estimated scope:** L

### Task 16: `Deciding`, the card with the plan behind it

**Description:**
- `decide`'s turns in `ask/decision.rs` become `yunta_ui::prompt::Deciding`, with `Opens`.
  - It opens on the card or on the plan.
  - The card's `Reply::Read` turns to the desk.
  - The desk's `Left::Back` turns back to the card, or declines when it opened on the plan.
  - `Left::Picked` answers the card's option.
- It answers a `PickedOption`, and says which of the two is in front.
- **The CLI's `decide` holds `Deciding` and keeps the terminal's part:** the alternate screen while the desk is in front, the documents printed where the terminal has no room for a desk, and the attribution.
- `question`, `edge`, `about` and `chose`'s words move to render's `prompt`.

**Found in its turn:** The question was already render's `escalation::claim_words`, which the card and the desk now both read; `edge`, `about` and `chose` are render's `prompt::card`. `Deciding` builds the desk itself (`Deciding::new(card)`, `Deciding::on_plan(card, reading, asked, opens)`), so the desk lists the card's options and Escape there follows what the decision opened on: `PlanDesk::new`, `Leaving` and the card's `reading`, `rows` and `pick` are the crate's own. A surface that holds both alike holds `Deciding` as one `Answerable`; the terminal holds `in_front` (`InFront`) and hands back what it came to through `replied` and `left`, the same turns. The line an answer is attributed with, which the decision and the round both say, is render's `prompt::attributed`; what stays in `ask/` is the terminal's own: its failures to restore itself, and the tests' messages. Sixteen files of the phase had another crate's `use` in the crate's own block; their imports are regrouped.

**Fails first:** `Deciding`'s tests, written against `decide`'s behavior, each seen red by breaking the turn it holds once:
- opening on the plan and leaving declines;
- reading turns to the desk;
- a number picked at the desk answers the card's option.

**Acceptance criteria:**
- [x] `ask/decision.rs` holds no turn between the two; the `resolve-gate` and `review` captures and tests equal the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui`, `suite.py cli-1..5`

**Dependencies:** Task 15

**Files likely touched:** `crates/ui/src/prompt/{deciding.rs,deciding/tests.rs}`, `crates/render/src/prompt.rs`, `crates/cli/src/ask/{decision.rs,mod.rs}`, `crates/cli/src/human_interaction.rs`

**Estimated scope:** M

### Group check: the prompts
- [x] The suite's groups green; every prompt's capture and every golden's digest equal to the base's.
- [x] `crates/cli/src/ask/` holds `Console`, `keyboard`, `hold`, `Strokes`, `NoAnswer`, `Answered`, and `decide`, `answer`, `choose` and `ask_line` as the calls that hold a prompt; nothing else.

### Task 17: Checkpoint A's proof

**Description:**
- The suite's count against the base's, by name: each moved test found under its new owner.
- Every golden's digest, the new goldens listed. The captures from a release build of the phase's tree.
- The manifest test, and `cargo xtask docs --check`.
- All of it in `proof/app-core/checkpoint-a.txt`.

**Found in its turn:** Measured on the final tree, after the attribution's words went to render: base 4,607 tests by name, now 4,627: 114 moved, 4 gone each with its successors named, 24 new. 154 captures equal to the base's. The goldens that changed are the catalog of every block's seven, which gained the four new blocks; 37 new. The release is 49,584 bytes larger (0.18%).

**Fails first:** none: a measurement.

**Acceptance criteria:**
- [x] No test gone without its new name; no capture different; no golden changed but the new.

**Verification:**
- [x] Manual check: `checkpoint-a.txt` read whole.

**Dependencies:** Group check: the prompts

**Files likely touched:** `.plans/yunta-tui/proof/app-core/` (outside git)

**Estimated scope:** S

### Checkpoint A: the crate, and the moves that print the same
- [x] Every sentence of the chronicle, of a moving run's rows and of every prompt is render's; every prompt draws as a block; the prompts' models are `yunta-ui`'s.
- [x] What prints is the base's, byte for byte.
- [ ] The user reviews; the phase commits, with `SPEC-app-core.md`, `CAPABILITY-MAP.md`, the closed modules' specs, `tasks/plan.md` and `tasks/todo.md`.

## Phase 2: what every surface needs, once, and the runners

Its groups, each green before the next: following; a run's page and the graph; answers and steps; what a stage keeps; the runners.

### Task 18: following a run is the API's

**Description:**
- `Project::follow(&RunId) -> Result<Following, Error>`. `Following` owns what it reads: the log, the registry, the clock, the promotion chain, the run's frame and the last seq.
- **`Following::next(&mut self, &Stop)` hands back `RunNews`:**
  - `Events(Vec<StoredEvent>)`: what came past the last seq;
  - `Engine(Engine)`: the engine, as the registry reads it;
  - `Said(Said)`: what a session said;
  - `Successor { run, events }`: the run a promotion handed it to, followed from here;
  - `Stopped` and `Stalled`.

  After `Stopped` or `Stalled` it hands back nothing. A stop tripped is `Error::Stopped`.
- `READ_EVERY`, `REGISTRY_EVERY` and `SUCCESSOR_WAIT` move from the CLI's follower with their reasons. So do `ending` and the listening of `follow/listen.rs`.

**Found in its turn:** `Following` owns no project: it opens a successor beside the run it was promoted from, by the rule the registry's successor already follows, so a task holds it whole. `next` hands back `Result<Option<RunNews>, Error>`; `Engine` comes only when the engine the registry names changed; what a session says comes between looks, as soon as it is heard. `PromotionChain` and `Words` are the API's own, since nothing outside it follows a run any more. The bench gains `store_path`, so a test reads a bench's log as a project's. Each of the five tests was seen red with the behavior it holds broken.

**Fails first:** the API's tests of `Following`, over a log written in the test:
- events past a seq;
- a stop;
- a stall under a registry naming a dead engine;
- a promotion's successor;
- words heard on a socket.

**Acceptance criteria:**
- [x] Each news kind is handed back as the test's log and registry make it; `cargo test -p yunta-api` passes.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api --test following`

**Dependencies:** Checkpoint A

**Files likely touched:** `crates/api/src/{lib.rs,run/follow.rs,run/follow/following.rs,event_log.rs}`, `crates/api/tests/following.rs`

**Estimated scope:** M

### Task 19: `status --follow` follows through the API

**Description:**
- The CLI's follower asks `Project::follow` and draws each `RunNews` on its surface. It keeps opening the surface, delivering events, passing what was said to its observer, the closing block and the exit code.
- `follow/listen.rs` goes: its listening is `Following`'s. The follower's doc stops saying it touches no registry, which it reads.

**Found in its turn:** The follower takes no `RunNews::Engine`: its surface reads the registry itself, as it did. Six follower tests name the API's `following.rs` in their `Reached by:`, and `run_words`' row test its `listening.rs`.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] The follow tests in `status_follow`, `status_follow/edges`, `status_follow/said`, `run_words` and the guarding flow pass unchanged; the follower's captures equal the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test status_follow`, `--test run_words`, `suite.py cli-1..5`

**Dependencies:** Task 18

**Files likely touched:** `crates/cli/src/commands/status/{follow.rs,follow/listen.rs}`, `crates/cli/src/surface/mod.rs`

**Estimated scope:** M

### Group check: following
- [x] The suite's groups green; the follower's captures equal the base's.

### Task 20: a run's page and a node's page own their data

**Description:**
- Render's `page::Run` and `page::Node` lose their lifetime. They own their run id, frame, state, places, decision and plans, and `Places` and `escalation::Decision` own theirs.
- The CLI's status and closing block, the fixtures and the tests build them owned.

**Found in its turn:** The closing block holds its page: `Closing` was twelve fields that mirrored `page::Run`'s, built into a borrowed page at each draw; it is now the page itself, built once. Owning their data let render's run tests drop the values they leaked to borrow for `'static`. Until the API reads the page whole, `status` builds it from clones of its reading.

**Fails first:** none: a change of form.

**Acceptance criteria:**
- [x] The run page's goldens (`render/goldens/status`, `open`) and every capture equal the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render --features golden`, `suite.py cli-1..5`

**Dependencies:** Group check: following

**Files likely touched:** `crates/render/src/page/run/{mod.rs,facts.rs,browser.rs,fixtures.rs,tests.rs,tests/*.rs}`, `crates/render/src/page/node.rs`, `crates/render/src/escalation.rs`, `crates/cli/src/surface/closing/mod.rs`, `crates/cli/src/commands/status/mod.rs`

**Estimated scope:** L (mechanical)

### Task 21: the API reads a run's page whole

**Description:**
- **`OpenedRun::page(self, Depth, &dyn Remarks) -> Result<page::Run, Error>`** reads the page with:
  - the documents the decision it waits on shows;
  - the plans at full depth;
  - its places.

  What its manifest holds that this binary does not read is said as `Remark::UnreadManifestKeys { keys }`, which render words.
- **`OpenedRun::node_page(self, &NodeId) -> Result<page::Node, Error>`** reads a node's page. A node the run does not declare is refused as render's `Refused::NoSuchNode { run, node, declared }`, beside `Refused::NoDecision`, said with the name it is nearest to, as the CLI said it.
- **`OpenedRun::page_for_a_browser(self) -> Result<PathBuf, Error>`** writes `page.html` in the run's folder through Surco's `Html`.
- The CLI's `status`, `print_page` and `open` call them, and its `read`, `note_unknown_keys`, `documents`, `page_body`, `find`, `run_page_for` and `page_for_a_browser` go. `open` keeps spawning the opener.

**Found in its turn:** The run's document a program reads was built twice, by `status --json` and by the MCP's `workflow_status`: it is `OpenedRun::document(remarks)` now, and both ask it. `node_page`, `document` and `page_for_a_browser` take the remarks too, since `status --node`, `--json` and `open` said the unread keys as well. `status` keeps no answer type: a person gets the page, a program the document. The API's tests read a bench's runs through a shared `tests/bench_project`, which the following tests use too. Each of the three tests was seen red with the behavior it holds broken.

**Fails first:** the API's tests:
- a parked run's page holds the documents its decision shows;
- a full page holds the plans;
- an undeclared node is refused naming the nearest.

**Acceptance criteria:**
- [x] `status`, `status --node` and `open`'s tests and captures equal the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api`, `cargo test -p yunta --test status_cmd --test status_node_cmd --test open_cmd`, `suite.py cli-1..5`

**Dependencies:** Task 20

**Files likely touched:** `crates/api/src/{run/reading.rs,run/page.rs,remark.rs,lib.rs}`, `crates/render/src/refusal/refused.rs`, `crates/cli/src/commands/{status/mod.rs,open.rs}`, `crates/cli/src/remarks.rs`, `crates/api/tests/run_page.rs`

**Estimated scope:** L

### Task 22: the graph is render's

**Description:**
- **`crates/cli/src/graph.rs`'s derivation moves to `crates/render/src/graph.rs`:**
  - `mermaid(&Workflow, Option<&Labels>)`, `dot(...)` and `labels(&RunFrame)`;
  - `children_of`, `labelled` and `escape_dot`;
  - its three tests.
- **`page::Graph` shows a workflow as Surco's `Diagram`,** under a headline naming it, and the run when it is a run's.
- The CLI's `graph` keeps its arguments, the verdict it prints beside a file and its exit code, and prints render's text.

**Found in its turn:** The labels a run derives are a type, `graph::Labels::of(frame)`, rather than a map anyone can build. `page::Graph` heads a workflow's graph `as written`, or with the run's handle and the word it stands at. Surco's reader takes no `subgraph`, so a workflow with a `parallel` group keeps its Mermaid source on a terminal, as any diagram it does not read; a workflow with none is drawn, dashed reroutes included. Its goldens hold both, and a run's.

**Fails first:** `page::Graph`'s goldens in every environment.

**Acceptance criteria:**
- [x] `graph_cmd`'s ten tests pass unchanged; the CLI's `graph.rs` holds no derivation.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render --features golden`, `cargo test -p yunta --test graph_cmd`

**Dependencies:** Task 21

**Files likely touched:** `crates/cli/src/graph.rs`, `crates/render/src/{lib.rs,graph.rs,page.rs,page/graph.rs}`, `crates/render/goldens/graph/`

**Estimated scope:** M

### Group check: a run's page and the graph
- [x] The suite's groups green; digests and captures equal to the base's but the new goldens.

### Task 23: the questions a parked node asked, read and answered at a terminal

**Description:**
- **`OpenedRun::waiting_questions(&self) -> Result<WaitingQuestions, Error>`** reads the questions document a parked node handed over, by the hash its `questions_asked` recorded. `WaitingQuestions` holds the node and its `QuestionsFile`. A run waiting on none is refused as `waiting_decision` refuses.
- **`yunta resolve-gate <run>` at a terminal answers a run parked on questions with the round**, through `OpenedRun::answer`, as the attached run asks it. Off a terminal it says how to answer, as for a gate.
- `docs/concepts.md` says so beside the MCP tool.

**Found in its turn:** The node a run is parked on is read off its pause, `Questions` or `AnswersRefused`; the round itself through the engine's `asked_questions`, which reads it as answering it does, by the hash its `questions_asked` named. `resolve-gate` with no option asks `waiting_questions` first: its refusal means the run waits on a decision, or nothing, and the menu goes on as before. What it answers is render's `QuestionsAnswered`, beside `GateAnswered`: answered, left with the questions still waiting, or interrupted; off a terminal it says `resolve-gate` asks them at one. `--by` stands over who answered at the round, as at the menu.

**Fails first:**
- the API's test reading a parked node's questions;
- a console interaction test answering a parked round with `resolve-gate`, then the run going on.

**Acceptance criteria:**
- [x] A parked round is answered from another terminal, and the run resumes detached with the answers on its log.
- [x] Every other `resolve-gate` capture equals the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api`, `cargo test -p yunta --test console_interaction`

**Dependencies:** Group check: a run's page and the graph

**Files likely touched:** `crates/api/src/{answers.rs,lib.rs}`, `crates/cli/src/commands/resolve_gate.rs`, `crates/cli/tests/console_interaction/questions.rs`, `docs/concepts.md`

**Estimated scope:** M

### Task 24: what the control plane answers in a sentence is render's

**Description:**
- The sentences `yunta-mcp`'s tools build go to render, beside `GateAnswered::sentence`: `run_workflow`'s `run_id:` answer, `resume_run`'s and `answer_questions`'. Their text stays the same.
- What a tool says of its own JSON arguments stays the tool's, as clap's words stay clap's.

**Found in its turn:** `run_workflow`'s first line and `resume_run`'s answer are render's `start::started_id` and `start::resumed`, beside the line a run starts with; `answer_questions`' is `QuestionsAnswered::sentence`, beside `GateAnswered::sentence`. "driving forward independently", which a detached start, a resume and an answered round each said, is one constant.

**Fails first:** none: a move.

**Acceptance criteria:**
- [x] `mcp_answers.py`'s capture equals the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-mcp`, `cargo test -p yunta --test mcp_flow`

**Dependencies:** Task 23

**Files likely touched:** `crates/mcp/src/tools.rs`, `crates/render/src/page/answered.rs`

**Estimated scope:** S

### Task 25: every command is a step, and render says what each does

**Description:**
- **`NextStep` gains the commands a person types that it lacks:**
  - `Stats { run }` and `History { workflow }`;
  - `Graph { workflow: Arg<String> }` and `RunGraph { run }`;
  - `NewWorkflow { name: Arg<String>, shape: Option<WorkflowShape> }`;
  - `PackList` and `PackAudit { pack }`.

  `Gc` gains `dry_run`.
- Render's `command` spells each as the CLI accepts it.
- **`command::does(&NextStep) -> &'static str` says what each step does,** beside its line, for the palette and help.

**Found in its turn:** The CLI's test held the one sample of every kind of step; it is core's `step::samples::every_step` now, behind the `testkit` feature, so the CLI's test runs each line through clap and render's says what each does, from one list, and its own test asks a sample of every kind. `History` names a `WorkflowName`. `does` and `line_for` say each family with arguments of its own through a helper, as `status` did. A step's words never say a state's: the listing reads "the ones waiting on you first".

**Fails first:** render's command tests for each new line and for what every step does.

**Acceptance criteria:**
- [x] Every new line parses in the CLI; a test runs each through clap.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render`, `cargo test -p yunta --bins`

**Dependencies:** Task 24

**Files likely touched:** `crates/core/src/step.rs`, `crates/render/src/{command.rs,command/tests.rs}`, `crates/cli/src/cli/tests.rs`

**Estimated scope:** M

### Group check: answers and steps
- [x] The suite's groups green; captures equal to the base's but `resolve-gate` on a parked round, which the user reads at Checkpoint B.

### Task 26: what a stage keeps is owned

**Description:**
- These derive `Clone`, with what they hold: `FieldReading`, `StatsReading`, `WaitingDecision`, `WaitingQuestions`, `WorkflowFile`, `AuditedWorkflow`, `InitializedProject` and `ClosingReading`.
- `DetectedProject` and `FetchedPack` own a clone of their `Workspace` where they borrowed one, and lose their lifetime.
- Every caller compiles unchanged in what it does.

**Found in its turn:** `InstalledPack`, which an install hands back, borrowed the workspace as `FetchedPack` did: it owns its clone too. Core's `Detected` derives `Clone`, since `InitializedProject` holds one.

**Fails first:** none: a change of form.

**Acceptance criteria:**
- [x] `git grep -n "DetectedProject<'\|FetchedPack<'"` finds nothing; the suite passes.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api`, `suite.py cli-1..5`

**Dependencies:** Group check: answers and steps

**Files likely touched:** `crates/api/src/{field.rs,run/stats.rs,answers.rs,workflow.rs,setup/init.rs,packs.rs,drive.rs}`, `crates/render/src/page/field.rs`

**Estimated scope:** M

### Task 27: setting up proposes runners, and writes those declared

**Description:**
- **`Workspace::detect_project` probes the adapters and proposes the roles.**
  - `DetectedProject` carries `probed: Vec<ProbedAdapter>` and `proposed: ProposedRunners { roles, needs_default }`.
  - The roles are those the repository's workflows name, or `implementer` when they name none.
- **`ProjectSetup { name, base_branch, runners: RunnerDeclaration }`.** `RunnerDeclaration` is `Proposed` or `Declared(BTreeMap<RunnerName, RunnerCandidate>)`.
- **`DetectedProject::write(self, ProjectSetup)`.**
  - A declared role is written uncommented, with `defaults.runner` where a node names none.
  - A proposed one is written commented, as today.
  - `write_project`'s parse check reads what was declared.

**Found in its turn:** The role proposed to a project whose workflows name none, `implementer`, was written in two places, render's `declare` and the config `init` writes: it is core's `RunnerName::PROPOSED`. Detection probes the adapters, so `write` no longer does; the roles come from the catalog's workflows, and `defaults.runner` is wanted where `ConfigKey::needed_by` says a node needs it. A declared runner is written in block style, since a model may hold any printable character; a role left without a model is written commented beside the declared ones, and `defaults.runner` is written only where the role it names is declared.

**Fails first:** the API's tests:
- a declared runner written and read back by `ConfigLayer::read`;
- a proposed one written byte for byte as the base writes it.

**Acceptance criteria:**
- [x] `init_new_cmd`'s eighteen tests pass unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api`, `cargo test -p yunta --test init_new_cmd`

**Dependencies:** Task 26

**Files likely touched:** `crates/api/src/{setup/init.rs,setup/init/tests.rs,lib.rs}`, `crates/cli/src/commands/init.rs`

**Estimated scope:** M

### Task 28: what `init` says of its runners

**Description:**
- `Initialized` says each declared role, its adapter and its model.
- It ends on installing a pack (`NextStep::PackAdd`) where every role is declared, and on the runner to declare, as today, where one is proposed.
- The page gets its first goldens: none exist.

**Found in its turn:** `InitializedProject` carries the runners `declared` and those `undeclared`, and the page reads both: the declared in one `runners` field under the adapters, since a field's label is fixed; with none declared it ends as it always did, on the runner to declare; with some, on the ones still to declare, without `defaults`; with all, on `yunta pack add <source>`. Its goldens: proposed, declared, partly declared.

**Fails first:** `Initialized`'s goldens and tests, declared and proposed.

**Acceptance criteria:**
- [x] The non-interactive `init` capture equals the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render --features golden`

**Dependencies:** Task 27

**Files likely touched:** `crates/render/src/page/initialized.rs`, `crates/render/src/tests/initialized.rs`, `crates/render/goldens/initialized/`

**Estimated scope:** S

### Task 29: `init -i` asks each role

**Description:**
- After the name and the base branch, `init -i` asks each proposed role:
  - its adapter, on the picker, among those that answered, the first standing;
  - its model, on a typed line with no default. A line left empty leaves the role proposed.
- The words are render's `prompt`.
- `README.md` and `docs/adapters.md` say that `init -i` declares the runners.

**Found in its turn:** The three existing tests run on a `PATH` where no adapter answers, so `init -i` asks them no runner and they read the same on any machine; the new ones put the scripted `claude` on theirs. A key typed before a list is drawn is discarded, as every prompt discards what was typed before it opened: the test waits for the list. A model that is no model name is refused and asked again; Escape at the list keeps the first adapter, as `init`'s Escape keeps a default.

**Fails first:** console interaction tests:
- `init -i` writes a declared role from the adapter picked and the model typed;
- an empty model leaves it proposed.

**Acceptance criteria:**
- [x] The three existing `console_interaction/init.rs` tests pass, their prompts in order.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test console_interaction`

**Dependencies:** Task 28

**Files likely touched:** `crates/cli/src/commands/init.rs`, `crates/render/src/prompt.rs`, `crates/cli/tests/console_interaction/init.rs`, `README.md`, `docs/adapters.md`

**Estimated scope:** M

### Task 30: a skeleton names the runner the config declares

**Description:**
- `Workspace::new_workflow` writes a skeleton's `runner:` uncommented where the project's config declares that role, as it writes `lint` where the config declares it.

**Found in its turn:** `skeleton` reads the merged config itself, `lint` and the runners alike; each skeleton node's `runner:` line is a placeholder, `{{implementer}}` or `{{planner}}`, written uncommented where the config declares the role with a candidate, and commented as before where it does not.

**Fails first:** `new_names_the_runner_the_config_declares`, in `init_new_cmd`.

**Acceptance criteria:**
- [x] A skeleton written where no runner is declared is byte for byte the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test init_new_cmd`

**Dependencies:** Task 29

**Files likely touched:** `crates/api/src/setup/workflow.rs`, `crates/cli/tests/init_new_cmd.rs`

**Estimated scope:** S

### Group check: the runners
- [x] The suite's groups green; captures equal to the base's but `init -i`'s and the parked round's.

### Task 31: Checkpoint B's proof

**Description:**
- The suite's count by name against Checkpoint A's; every golden's digest; the captures, with the two that change shown beside the base's.
- A real run followed with the API's following, as Task 3's: its follower lines against the base's, its time between nodes and its tokens.
- The release size, by crate.
- All of it in `checkpoint-b.txt`.

**Found in its turn:** The suite: 4,651 ran, Checkpoint A's 4,627 and 24 new; one name the reader missed, its line written with an escape before `ok`. Every capture is the base's but `init -i`'s, which asks the runner, and the new one where it is declared; `prompts.py` answers the runner where a binary asks it. `resolve-gate` on a parked round, the base's refusal beside the round now, the run carried on to its end. The real run reached `ship` in 115 s against the base's 211 s, `between_secs` 0.541 against 0.556; its follower differs from the base's only in what the agents did, and where the base's log recorded a read with no name. The text grew 75,212 bytes, the release 82,688 (0.29%).

**Fails first:** none: a measurement.

**Acceptance criteria:**
- [x] The follower's lines equal the base's but ids and times; `between_secs` inside the base's spread.

**Verification:**
- [x] Manual check: `checkpoint-b.txt` read whole.

**Dependencies:** Group check: the runners

**Files likely touched:** `.plans/yunta-tui/proof/app-core/` (outside git)

**Estimated scope:** S

### Checkpoint B: once, below the surfaces, and the runners
- [x] Following, a run's page, the graph, the parked questions and every command's step stand below the surfaces; what a stage keeps is owned.
- [x] The user reads `init -i`'s new questions and `resolve-gate` on a parked round; every other capture is the base's.
- [x] The user reviews; the phase commits.

## Phase 3: the application, and watching, deciding and starting

Its groups, each green before the next: the values; watching; deciding; starting.

### Task 32: the application's values

**Description:**
- `State`, `Input`, `Request`, `Ticket`, `Operation`, `Outcome`, `News` and `Stage`, with the plan's signatures, each in its own module of `crates/ui/src/`.
- `State::opened(now)` asks `Operation::InRepository` and stands on nothing until it is answered. Outside a repository it opens on `Stage::Signpost`, which shows render's `Signpost`.
- An `Outcome` or `News` for a ticket the state let go changes nothing.
- `render::screen` holds the first words a screen says: the signpost's trail.

**Found in its turn:** The state stands on no stage until it knows where it is (`Option<Stage>`), and where learning it fails its screen says what stopped it and leaving is what is left; `Ticket` is the state's own, minted in order, with no constructor a surface can call. `yunta-ui` depends on `chrono` directly, as render and the API do, for the instant `opened` and `Input::Time` carry. The late-answer test answers `true` first, so a late `false`, taken, would put the signpost on screen. Each test was seen red with its rule broken.

**Fails first:** `crates/ui/tests/state.rs`:
- opening asks where it stands;
- a late outcome is dropped;
- outside a repository the screen is the signpost.

**Acceptance criteria:**
- [x] `State` is `Clone`; the three tests pass through `State` alone.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui`

**Dependencies:** Checkpoint B

**Files likely touched:** `crates/ui/src/{lib.rs,state.rs,input.rs,request.rs,operation.rs,outcome.rs,stage.rs,stage/signpost.rs}`, `crates/render/src/screen.rs`, `crates/ui/tests/state.rs`

**Estimated scope:** L

### Task 33: the performer

**Description:**
- **`Performer::new(Workspace)` and `perform(ticket, operation, inputs)`.**
  - Each operation gets a `Stop` of its own.
  - What the operation remarks is sent as `Input::Remark`, and its end as `Input::Outcome`.
  - An `Error` comes back as `Outcome::Refused`, `Failed` or `Stopped`.
- **`let_go(ticket)`** trips that ticket's stop and drops what the performer holds for it.
- `Operation::InRepository` is carried out first. Each later task adds its operations here.

**Found in its turn:** With `InRepository` the only operation, nothing it asks can be refused: what the test holds is a config that does not read, which comes back as `Outcome::Failed` in the API's words; the first operation that refuses brings its test. The performer passes no remarks until an operation says some, and `let_go` is first held by the run stage's following (Task 37). The test's workspace has a `YUNTA_HOME` and an org layer of its own and a fixed `PATH`, so nothing of the machine's is read.

**Fails first:** `crates/ui/tests/performer.rs`, against a workspace in a temporary directory:
- `InRepository` answers;
- a refused operation comes back as render's refusal.

**Acceptance criteria:**
- [x] The performer reads nothing of its process: the workspace is the test's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test performer`

**Dependencies:** Task 32

**Files likely touched:** `crates/ui/src/{performer.rs,performer/held.rs}`, `crates/ui/Cargo.toml`, `crates/ui/tests/performer.rs`

**Estimated scope:** M

### Task 34: the walk

**Description:**
- **`crates/ui/tests/walk.rs` starts from `State::opened` and applies every stroke and every outcome a pending ticket can have.**
  - The strokes: every `Stroke` but `Insert`, and `Insert` of each key a stage binds and of one letter.
  - Each operation is answered by its recording.
- **The recordings come from `walk/home.rs`**, which builds a scratch home with runs at every standing, a parked gate, a plan, a questions node, packs and a repository never set up. `walk/recorded.rs` performs each distinct operation once on a fresh copy and keeps its outcome.
- **States are deduped** by their screen in Markdown and the operations they wait on.
- **What it holds:**
  - `update` never panics;
  - every request is one the performer carries out;
  - every screen draws with `Html` and `Markdown`;
  - every palette row is a step its stage takes, spelled as `command::line` spells it.

**Found in its turn:** The walk answers a piece of work with each outcome recorded for it in every state that waits on it, so an outcome is `Clone`, and with it render's `Refused` and the two types it held that were not, `Unrunnable` and replay's `ReceiptError`. The state says the work it waits on (`State::waiting_on`), which the walk dedups by beside the screen. The palette's rows are held once the palette is built (Task 55). The walk was seen red with a stage made to panic on `Tab`, naming `opened`, the answer and the stroke.

**Fails first:** a stage that panics on a stroke, made to for the run, is found by the walk with the strokes that reach it.

**Acceptance criteria:**
- [x] The walk covers the signpost and finishes in under a minute; it grows with each stage.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test walk`

**Dependencies:** Task 33

**Files likely touched:** `crates/ui/tests/{walk.rs,walk/home.rs,walk/recorded.rs}`

**Estimated scope:** M

### Task 35: the sandboxes

**Description:**
- **`crates/ui/tests/support/mod.rs` drives a `State` and a `Performer` until no ticket is pending:** `drive(state, performer, strokes) -> State`.
- **`crates/cli/tests/interface/mod.rs` does the same where the work spawns Yunta,** with the workspace's binary `CARGO_BIN_EXE_yunta` and the mock adapter.

**Found in its turn:** How a request is carried out is the performer's, once, for every surface and harness: `Performer::take` starts work under its ticket in a task of its own, stops work let go, and hands back what only a surface does, a page or leaving. Each harness's `drive` is the loop around it; the CLI's lives in `tests/interface.rs`, the root its stage groups sit beside, as `console_interaction.rs` holds its own, its workspace running this build's `yunta`. `tests/support` is shared by several binaries of `yunta-ui`, so it allows what one of them leaves unused, as the engine's `common` does. Each harness has a test driving the signpost to quiet and to `Leave`.

**Fails first:** none: harnesses, first used in Task 36.

**Acceptance criteria:**
- [x] Both drive the signpost to quiet.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui`, `cargo test -p yunta --test interface`

**Dependencies:** Task 34

**Files likely touched:** `crates/ui/tests/support/mod.rs`, `crates/cli/tests/{interface.rs,interface/mod.rs}`

**Estimated scope:** S

### Group check: the values
- [x] The suite's groups green; the walk covers the signpost.

### Task 36: the field

**Description:**
- `Stage::Field(FieldStage)`, shown in a repository with runs. `FieldStage` holds the reading, the row pointed at, the rows marked, the reach and the depth.
- **What a person does there:**
  - open the run pointed at;
  - go to what needs them (`NextStep::Needs`);
  - resume (`Operation::Resume`);
  - cancel one run, after a confirmation;
  - close one run or every marked one, after one confirmation naming each;
  - widen the reach to the machine, or the depth to the closed runs.
- The screen: render's `Field` in its main region, the tally of each standing, the attention, and the legend from `render::screen`.
- A repository set up with no runs opens on the first run (Task 41), a stage this task points to.

**Fails first:**
- `crates/ui/tests/field.rs`: marking and closing two runs asks one confirmation naming both, then `Operation::Close` with both;
- `crates/cli/tests/interface/field.rs`: two mock runs closed, the field read again.

**Found in its turn:** The pointer and the picks are the field's own: render's `Field` takes the run pointed at and the runs picked and says them on their rows, Surco's `RunLine` draws them (a pointer, and `■`, `+` in ASCII), and the page says the order it lists runs in (`Field::listed`), which the pointer moves along. The state asks no work it already waits on, and a reading asked again lets go the one of its kind under way (`Operation::reads`); without that the walk grows without end. The walk goes breadth first, keeps a state once it is new, and keys the work a state waits on by kind, since it answers every piece of work of a kind with the same recordings: 2,459 states in about 4 s, and a panic set at full depth with a run picked is found with the inputs that reach it. Opening the run pointed at and going to what needs a person bind with the run stage (Task 37). Render's field page keeps its row in `page/field/row.rs`. The CLI harness's workspace takes the state root the testkit hands the binary, so the runs the binary starts are the field's.

**Acceptance criteria:**
- [x] The walk covers the field at every reach and depth.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test field --test walk`, `cargo test -p yunta --test interface`

**Dependencies:** Group check: the values

**Files likely touched:** `crates/ui/src/{stage/field.rs,dialog.rs,dialog/confirmation.rs,performer.rs}`, `crates/render/src/screen.rs`, `crates/ui/tests/field.rs`, `crates/cli/tests/interface/field.rs`

**Estimated scope:** L

### Task 37: a run, followed

**Description:**
- `Stage::Run(RunStage)`. `RunStage` holds:
  - the run's page, from `Operation::ReadPage`;
  - its moments, folded by replay's `chronicle` from the events `Operation::Follow` hands back as `News::Run`;
  - what its sessions said, and whether it follows.
- The screen:
  - the trail;
  - the ribbon;
  - the chronicle in render's words in the main region, the moving rows of `render::watched` under it;
  - the run's page beside it.
- A promotion's successor is followed on, its page read again. The run's stop or stall reads its page once more and stops following.

**Fails first:**
- `crates/ui/tests/run_stage.rs`: news of events grows the chronicle;
- `crates/cli/tests/interface/run.rs`: a mock run followed to its end, and a promotion onto its successor, as `status_follow/edges` makes one.

**Found in its turn:** The run a following reads is the API's `FollowedRun`: the manifest it froze, every event read, the state they derive and its engine. The following folds each piece of news into it, and an interface keeps a copy and folds the same news by the same `took`; `RunNews::Successor` carries the successor whole. Render owns what a watched run shows: the line a pipe writes for a moment (`chronicle::logged`), the chronicle as a document (`chronicle::told`), what the sessions said last (`watched::Heard`, from the CLI's region), the rows (`watched::rows`) and the ribbon (`watched::ribbon`); the CLI's region and pipe draw through them, their tests unchanged. A stage opens over another and goes back to it (`Want::Open`, `Want::Back`), letting go the readings it waits on, and work is let go by what it is (`Want::LetGo`). A piped follower writes nothing for the log it attached to, while the stage keeps the run's whole story: the follower's lines close the stage's chronicle, word for word. Enter opens the run pointed at and `n` the one waiting on the person; the field is read again on return. Surco's `Look` carries motion: the watched rows and the ribbon turn as the look they are drawn with says, a page read rather than watched (plain, Markdown, HTML, a pipe) stands still, and the CLI's looks carry what `YUNTA_MOTION` asked, so the stage turns its marks as its surface's reader asked. The walk records news beside outcomes: 2,651 states in about 4 s.

**Acceptance criteria:**
- [x] The run stage's chronicle says each moment in the words `status --follow` writes through a pipe.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test run_stage`, `cargo test -p yunta --test interface`

**Dependencies:** Task 36

**Files likely touched:** `crates/ui/src/{stage/run.rs,performer.rs}`, `crates/render/src/screen.rs`, `crates/ui/tests/run_stage.rs`, `crates/cli/tests/interface/run.rs`

**Estimated scope:** L

### Task 38: a run's node, and what it offers

**Description:**
- **Picking a node** stops following, and shows its page (`Operation::ReadNodePage`) in place of the run's.
- **What `RunFrame::actions` offers is bound to keys and listed:**
  - stopping, after a confirmation;
  - going on (`Operation::Resume`);
  - answering, which opens the decision (Task 39).

  The receipt, verifying and the page in a browser lead to the evidence stage (Task 49).
- **A run asked at another terminal** is shown as asking there, with only what the frame offers: stopping it.

**Fails first:** `crates/ui/tests/run_stage.rs`:
- a node picked shows its page and stops following;
- a run asking at its terminal offers only stopping.

**Found in its turn:** A pointer moves along the run's nodes (`j`, `k`); Enter picks the node pointed at, stops following and shows its page in place of the run's, and Esc goes back to the run's page. An act is bound to a key only where `RunFrame::actions` offers it, read with the decision the run's page says it stopped on: `c` stops and `x` closes after a confirmation, `r` goes on and reads the run again, and `w` opens the child a run waits on over it. Answering, the receipt, verifying and the page for a browser are offered by the decision and evidence stages. The legend says each bound act in render's words for its effect (`run_legend`), and a run that comes back into view follows again and reads its page or its node's again. The node page travels as an outcome, so render's `Node` is `Clone`. The walk reaches node pages too: 4,449 states in about 10 s.

**Acceptance criteria:**
- [x] Every key the run stage binds is a step of `RunFrame::actions`, or moves within the stage.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test run_stage --test walk`

**Dependencies:** Task 37

**Files likely touched:** `crates/ui/src/stage/run.rs`, `crates/render/src/screen.rs`, `crates/ui/tests/run_stage.rs`

**Estimated scope:** M

### Group check: watching
- [x] The suite's groups green; the walk covers the signpost, the field and a run.

### Task 39: a gate, decided

**Description:**
- `Stage::Decision(DecisionStage)`. `Operation::ReadDecision` reads `WaitingDecision`.
- The stage holds a `Deciding`. The card stands in the main region, with the documents the decision shows (render's `Gate::AboveTheCard`) beside it. The desk takes the screen when it is in front, and `Input::Laid` reaches it.
- **What a person does there:**
  - pick an option, with words where it asks: `Operation::Decide`, handed to a detached resume by the API;
  - leave the decision waiting, back to where they came from.

**Fails first:**
- `crates/ui/tests/decision_stage.rs`: reading turns to the desk, and a number picked there decides;
- `crates/cli/tests/interface/decision.rs`: a parked gate decided, and the run resumed detached.

**Found in its turn:**
- **A decision travels as a value.** Render's desk `Reading` and `AlsoShown` own the plan and the documents they read, so `PlanDesk` and `Deciding` carry no lifetime and are `Clone`, as the state is.
- **One value puts a decision, wherever it was read.** `prompt::DecisionPut` holds the escalation, its documents, the change, the routes, the node and the workflow and mode; `DecisionPut::waiting` puts it from the API's `WaitingDecision`, and `DecisionPut::asked` from what the engine driving a run asks with. The card holds it and says its edge and the lines under its question in the glyphs it is drawn with. `Deciding::put` builds the card and the plan behind it. The CLI's `Ask` is gone, and `resolve-gate`, `review` and a run's own prompt put their decisions from the same value.
- **The waiting decision.** `WaitingDecision` carries the routes, the change and the workflow and mode its menu is drawn from, in place of the run's state, so the CLI and the application read one decision.
- **Opening a decision from a run.** The run stage opens it with `a` where the run stopped on a menu (the step its attention row names) and with `p` where `RunFrame::actions` offers reading the plan; its acts moved to `stage/run/acts.rs`. The desk takes the screen while the plan is in front, and what a surface laid out reaches it through `Input::Laid`. `State::typing` says when a field is open, for a surface's pastes.
- **A detached resume stands where the workspace stands.** It inherited the process's environment, not the workspace's `Env`, so a resume handed on from a workspace given its own environment never found its run. `Env::handed_on`, the reverse of `Env::of_vars`, hands the child the workspace's environment; in a shell they are the same.
- **The card is `resolve-gate`'s.** Every line the application's card draws on an 80-column terminal is on the screen `resolve-gate` holds; a card built with another Escape fails that.
- **The walk.** It tries strokes only where a state waits on at most one piece of work, and types nothing into an open field (`Stroke::types`). With every key the stages bind, it reaches the decision in 6,292 states in about 10 s.
- **Files over 500 lines.** `config/env.rs`'s tests stand in `env/tests.rs`, and the desk's finding of a plan among a decision's documents in `desk/among.rs`.

**Acceptance criteria:**
- [x] The card's legend, words and refusals are the CLI's `resolve-gate`'s, drawn as blocks.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test decision_stage`, `cargo test -p yunta --test interface`

**Dependencies:** Group check: watching

**Files likely touched:** `crates/ui/src/{stage/decision.rs,performer.rs}`, `crates/render/src/screen.rs`, `crates/ui/tests/decision_stage.rs`, `crates/cli/tests/interface/decision.rs`

**Estimated scope:** M

### Task 40: a round, answered

**Description:**
- **A run parked on questions opens its round in the decision stage.** `Operation::ReadQuestions` reads `WaitingQuestions`.
- **The `Round` answers through `Operation::Answer`.** Nothing is recorded until the last answer, and Escape leaves with nothing recorded.

**Fails first:** `crates/cli/tests/interface/questions.rs`: a parked round answered, and the run resumed with the answers on its log.

**Found in its turn:**
- **Reading the round first.** The decision stage reads what a run waits on as `resolve-gate` reads it with no option: its round first, and its decision where the refusal says it is parked on no round. A person who came to read the plan reads the decision alone.
- **One rule for a waiting round.** `yunta_api::waiting_round` names the node a run's round waits at, for the API's `waiting_questions` and for the run stage, which offers `a` on a run parked on a round as on one parked on a menu.
- **What a round is.** It is `prompt::Round`, the one the attached run asks, `Clone` now: its words and the refusals it says under a question are the run's. Nothing is recorded until its last answer, and a round of no questions is answered with nothing. `Operation::Answer` records the answers on `Channel::Tty`, the channel of every surface the application has today; core's `Answer` is `Eq`, so an operation compares whole.
- **The walk.** The runs it decides and answers stand in worlds of their own, since every run the field lists multiplies where the pointer and the picks stand: 2,426 states in about 4 s, the decision and the round among them.

**Acceptance criteria:**
- [x] The round's words and refusals are the attached run's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test interface`

**Dependencies:** Task 39

**Files likely touched:** `crates/ui/src/{stage/decision.rs,performer.rs}`, `crates/cli/tests/interface/questions.rs`

**Estimated scope:** S

### Group check: deciding
- [x] The suite's groups green; the walk covers the decision stage on a gate, a plan and a round.

### Task 41: the first run

**Description:**
- **`Stage::FirstRun(FirstRunStage)`,** in a repository set up with no runs. It holds the request being typed on a `TypedLine`, and the catalog from `Operation::ReadCatalog`.
- **The screen:** the first run's question from `render::screen`, over the catalog's page.
- **Enter takes the request to the start stage,** with the request already typed.

**Fails first:** `crates/ui/tests/first_run.rs`: a set-up repository with no runs opens here, and typed words reach the start stage.

**Found in its turn:**
- **Where the application opens.** It asks the repository it works in as far as Yunta goes: `yunta_api::Repository`, none, never set up or set up, read by `Project::repository`, which takes the place of `in_a_repository` for `yunta` alone too; the repository's config layer has one path, `repository_layer`, which `init` and the layers read. A set-up repository with no run opens on the first run.
- **While the runs are read.** The screen says so, and that Yunta was never set up in the repository where it was not; the walk told the two apart only once the screen did.
- **A stage that takes another's place.** `Want::Replace` puts a stage in the place of the one the person is at, what that one opened over staying under it. The first run gives way to the field once its repository has a run, and a started run's stage takes the start's place.
- **The question.** It is a `LineAsked` over render's `Catalog`, whose types are `Clone`. Enter takes what was typed to the start stage.

**Acceptance criteria:**
- [x] The walk covers the first run.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test first_run --test walk`

**Dependencies:** Group check: deciding

**Files likely touched:** `crates/ui/src/{stage/first_run.rs,performer.rs}`, `crates/render/src/screen.rs`, `crates/ui/tests/first_run.rs`

**Estimated scope:** S

### Task 42: a run started

**Description:**
- **`Stage::Start(StartStage)`:**
  - the workflow, picked off the catalog on a `Picker`;
  - each input it declares, on a `TypedLine`, with the request typed in the input the workflow reads it from;
  - its mode and its adapter, each on a `Picker`, defaulting as `yunta run` defaults.
- **`Operation::Start` admits, creates and starts the run detached, as `yunta run --detach` does.** Admission's cautions come back as remarks and stand on the screen. The run is then followed (Task 37).
- **A refusal is said by render,** and the person stays where they were, to change what was refused.

**Fails first:** `crates/cli/tests/interface/start.rs`:
- a mock workflow started detached and followed to its end;
- an input left empty, refused as `yunta run` refuses it.

**Found in its turn:**
- **The request is a value.** `RunRequest` owns what it asks with and compares whole, so `Operation::Start` carries the request `yunta run`, the control plane, a case and a doctor probe build alike; core's `RunAdapter` is `Eq`.
- **The questions.** The workflow is picked off the catalog, the request typed at the first run stands in the input the workflow declares as its request, each other input is asked on a `LineAsked` in the workflow's order (one left empty is left out, as an unset `--input` is), the mode where the workflow declares any, and the adapter, none first, which leaves it to the runners. Escape goes back (`Escape::GoesBack`); a refusal is said and the questions are asked again.
- **What admission says.** The words and weight of every remark are `yunta_ui::remark`'s, which the CLI's `remarks.rs` says through now, unchanged: the start stage shows what admission said under the questions. An input's type label is render's `inputs::type_label`, for the catalog and the question alike.
- **The walk.** A list is narrowed by one character at most, since its filter takes characters though a paste answers nothing there: 2,507 states in about 4 s, the first run and a start among them.

**Acceptance criteria:**
- [x] What `Operation::Start` asks is the `RunRequest` `yunta run` builds from the same choices.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test interface`, `cargo test -p yunta-ui --test walk`

**Dependencies:** Task 41

**Files likely touched:** `crates/ui/src/{stage/start.rs,performer.rs}`, `crates/render/src/screen.rs`, `crates/cli/tests/interface/start.rs`

**Estimated scope:** M

### Group check: starting
- [x] The suite's groups green; the walk covers every stage built so far.

### Task 43: Checkpoint C's proof

**Description:**
- The suite's count by name, every golden's digest and the captures, against Checkpoint B's.
- The walk's count of states and screens.
- All of it in `checkpoint-c.txt`.

**Fails first:** none: a measurement.

**Found in its turn:** The captures against Checkpoint B's differ in nine views, each saying the version of Claude Code installed on this machine, which moved from 2.1.319 to 2.1.322 between the two: the machine's, not the code's. The suite ran 4,691, 4,650 at Checkpoint B; `env.rs`'s tests ran under `config::env::tests` once they moved beside it. The release is 66,080 bytes over Checkpoint B's. The walk reaches 2,507 states, each a screen with the kinds of work it waits on.

**Acceptance criteria:**
- [x] Nothing the CLI prints changed in this phase.

**Verification:**
- [x] Manual check: `checkpoint-c.txt` read whole.

**Dependencies:** Group check: starting

**Files likely touched:** `.plans/yunta-tui/proof/app-core/` (outside git)

**Estimated scope:** S

### Checkpoint C: watching, deciding, starting
- [x] The walk covers the signpost, the field, a run, a decision and a start; each works end to end against its sandbox.
- [x] The user reviews; the phase commits.

## Phase 4: every other stage, the dialogs, and the proof

Its groups, each green before the next: setup; the catalog; what a run leaves and costs; the machine; the dialogs; the proof and the review.

### Task 44: setup

**Description:**
- **`Stage::Setup(SetupStage)`,** in a repository never set up. `Operation::DetectProject` reads it, and the performer holds the `DetectedProject` under its ticket.
- **The screen shows `page::Setup`, render's page of what was found and would be written.** `Detected` is core's, hence the name.
- **What a person answers:** the name and the base branch on `TypedLine`s, and each proposed role's adapter and model, as `init -i` asks them.
- **`Operation::WriteSetup { ticket, setup }` writes it.** The screen shows `Initialized`, then offers a pack to install (Task 48).

**Fails first:**
- `crates/ui/tests/setup.rs`: the answers become the `ProjectSetup` `init -i` would write;
- render's goldens of `page::Setup`.

**Found in its turn:**
- **What was detected, apart from where it is written.** The API's `ProjectDraft` is what detecting found, a value any holder keeps (the repository, name, base branch, what it runs, the adapters probed, the runners proposed); `DetectedProject` binds it to its repository and writes it. `ProjectDraft::answered` holds `init -i`'s rules (an empty line keeps what was detected; a runner with no model stays proposed), so `yunta init -i` and the application answer through it; `writes()` says what setting up writes, from the same constants the write uses.
- **What the performer holds.** `performer/held.rs` keeps the stops and the projects detected under one lock: a project is kept only while its ticket is held, so a detection let go as it ends leaves nothing, and `let_go` drops both. `Operation::WriteSetup { held, setup }` names the ticket the project was detected under; one let go fails with render's `SETUP_LET_GO`. `Stage::held` says what a stage holds, and the state lets it go on back, replace and leaving (`Request::LetGo` before `Request::Leave`).
- **The questions, as `init -i` asks them.** The name and the base branch on `LineAsked`s that say what Enter keeps; Escape keeps what was detected, as `init -i`'s console. Where an adapter answers, each proposed runner's adapter on a `Picker` whose Escape takes the first, then its model; a model name refused is said and asked again. A refused or failed write finds the repository again and asks again, since writing takes the project.
- **Where the application opens.** A repository never set up opens on its setup (the opening note that said so is gone); what was written is `page::Initialized`, and Enter goes on to the first run until packs join (Task 48).
- **Sizes.** `init.rs` passed 500 lines: the config's YAML and its tests are `init/config_yaml.rs`. `DecisionStage::answered` passed 50 with the two outcomes: the round read is a function of its own, and the performer's decided and cancelled acts are functions as their siblings are.
- **An adapter's CLI is found on the environment's `PATH`, never the process's.** Probing for setup found the machine's CLIs whatever the workspace said. Core's `process::SearchPath` is now where a program started by its name is found (a path names itself; a bare name is the first executable file by it), and `Env.path` is one. An adapter's probe takes the search path and a session's request carries the run's, so the probe asks the CLI the session runs: `init`, `doctor`, `check`'s suggestions and a run's admission probe on the workspace's. `git`, the shell a node runs in and a pack's required programs find theirs through it too, and `Env::find` is gone. `core/tests/cli_search_path.rs` fails with the probe searching the process's `PATH`.
- **The walk** records a setup written in a repository of its own (`to_set_up`), with a stand-in `claude` on its `PATH`, and must reach the setup, a runner's adapter and model, and what it wrote: 2,528 states in about 3.6 s, the same on any machine.

**Acceptance criteria:**
- [x] A setup left before writing writes nothing, and its detected project is dropped.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test setup`, `cargo test -p yunta-render` (render's goldens run in its plain suite: it has no `golden` feature)

**Dependencies:** Checkpoint C

**Files likely touched:** `crates/ui/src/{stage/setup.rs,performer.rs,performer/held.rs}`, `crates/render/src/{page.rs,page/setup.rs,screen.rs}`, `crates/ui/tests/setup.rs`

**Estimated scope:** M

### Group check: setup
- [x] The suite's groups green; the walk covers setup.

### Task 45: the catalog

**Description:**
- `Stage::Catalog(CatalogStage)` shows render's `Catalog`, with a workflow pointed at.
- **For the workflow pointed at:**
  - its check, `Operation::Check`, shown as `Checked`;
  - its graph, which leads to the graph stage (Task 51);
  - its history, which leads to the stats stage (Task 51);
  - running it, which leads to the start stage with it picked.

**Fails first:** `crates/ui/tests/catalog.rs`: a workflow that breaks a rule shows its check with the rule named.

**Found in its turn:**
- **`check` is the API's.** `yunta check`'s whole flow (the workflow read in one pass, the project's layers or one `--config` file, the verdict, what its past runs say) was the CLI command's own. It is `Project::check`, answering a `WorkflowCheck` (the `Checked` page, whether it passes, the history's findings); the command prints it unchanged (its 17 tests) and `Operation::Check` shows it. `Checked` and `Diagnosis` are `Debug` and `Clone`, as an outcome is.
- **A pointer over sections.** Surco's `Lead::Pointer { on }`: a terminal keeps its place before every section of the list so the titles stand in line, as a list of runs keeps its pointer column; Markdown and HTML draw it before the one pointed at. In Surco's catalog of components, blessed.
- **The catalog pointed at.** Render's `PointedCatalog` draws `Catalog`'s sections led by the pointer's place; `Catalog`, which `yunta list` and `list_workflows` print, carries none. Goldens in every environment, an unreadable workflow among them.
- **Where it opens.** From the field, with `w` (its legend says so); the palette (Task 55) offers it too. `j`/`k` move around its ends, `c` checks the workflow pointed at (Escape lets the check go), Enter opens the start with it picked. A workflow that does not read starts nothing; its section says why.
- **A start with no request typed.** `StartStage::picked` opens on a workflow picked off the catalog: the input it reads its request from is asked as every other is, and Escape goes back to the catalog.
- **Its graph and its history** join with their stages (Task 51).
- **The walk** records a check of `greet` and must reach the catalog and a check: 3,834 states in about 5 s.

**Acceptance criteria:**
- [x] The check's page is the one `yunta check` prints.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test catalog`

**Dependencies:** Group check: setup

**Files likely touched:** `crates/ui/src/{stage/catalog.rs,performer.rs}`, `crates/render/src/screen.rs`, `crates/ui/tests/catalog.rs`

**Estimated scope:** M

### Task 46: a new workflow, and the cases

**Description:**
- **A new workflow:** a name on a `TypedLine` and a shape on a `Picker`. `Operation::NewWorkflow` writes it, and the screen shows `NewWorkflow` with its diagnosis.
- **The cases:** `Operation::RunCases` runs the project's, each case handed back as `News::Case` as it ends. `Tested` grows on the screen, and a person can stop them.

**Fails first:** `crates/ui/tests/catalog.rs`:
- a workflow written in a sandbox;
- two cases, each seen as it ends, the second stopped by `LetGo`.

**Found in its turn:**
- **A new workflow, as `new -i` asks it.** In the catalog, `n` asks the name on a `LineAsked`, then the shape on the list `new -i` offers (Escape takes the first, `one-node`). `Operation::NewWorkflow` parses the stem, writes the skeleton and checks it, as `yunta new` does; what stops the check is said beside the page, the file written whatever did (`NewWorkflowWritten.diagnosis`). Escape or Enter goes back to the workflows, read again with the new one pointed at. `yunta new` says where it wrote through `Workspace::shown`, as the application does.
- **The cases, as they end.** `t` runs them: `Operation::RunCases` hands every name back first (`News::CasesNamed`), then each case as it ends (`News::Case`), what their runs say as remarks, and the tally last (`Outcome::Tested`). Each case ends once: one heard again is the one shown. Escape stops them, keeping what ended and saying they were stopped. The cases are a reading: leaving the catalog lets them go.
- **A run on any thread.** Running the cases drives a run inside the performer's task, which a runtime may move between threads, so the engine's boxed futures say they are `Send` (the measurement aside, a step, a node's kind, a child run); `moves_before` takes whether a measurement is under way rather than holding the measurement across its awaits. `cases.rs`'s `running_the_cases_can_move_between_threads` holds it at compile time, the engine's run as what reaches it.
- **The walk** binds `t` and runs on the testkit's deep stack, since its recordings run a case whole inside the test: 5,093 states in about 6.4 s, the new workflow named and written and the cases run and stopped among them.

**Acceptance criteria:**
- [x] The skeleton and the cases' page are those `yunta new` and `yunta test` write.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test catalog`

**Dependencies:** Task 45

**Files likely touched:** `crates/ui/src/{stage/catalog.rs,performer.rs}`, `crates/ui/tests/catalog.rs`

**Estimated scope:** M

### Task 47: the packs, listed, audited, removed and scaffolded

**Description:**
- **`Operation::ListPacks`** shows render's listed packs, and **`Operation::AuditPack`** a pack's audit.
- **Removing a pack** asks a confirmation, then `Operation::RemovePack`.
- **Scaffolding a pack:** a name on a `TypedLine`, then `Operation::ScaffoldPack` and its check.

**Fails first:** `crates/ui/tests/packs.rs`: a pack removed after its confirmation, and the listing read again.

**Found in its turn:**
- **The packs, pointed at.** From the catalog with `p`. Surco's `Checklist::pointing_at` draws the pointer in a row's margin, so every row stands where it would without it (a file draws it before the row); render's `PointedPacks` is `pack list`'s rows with the pointer on one, and `Listed` carries none. Goldens in every environment, and in Surco's catalog of components, blessed.
- **The acts.** `a` audits the pack pointed at, its cases run (Escape lets the audit go); `x` asks a confirmation, then removes it, says so over the listing and reads it again; `n` asks the new pack's publisher and name, writes it and runs its case, as `pack new` does: a name that is no pack reference is refused as render's `InvalidId`, and what stops the check is said beside the page. `pack new` says where it wrote through `Workspace::shown`, as the application does.
- **Shared pages.** A listing, an audit and a new pack's page are read whole and never changed, and some of their parts hold errors that do not clone: they travel in an `Arc`. `Listed`, `VendoredPack`, `NewPackCheck` and `AuditedPack` are `Debug`.
- **Work in families.** `Operation` and `Outcome` nest a family that grows on its own under an enum of its own: `Operation::Workflow(WorkflowOperation)` and `Operation::Pack(PackOperation)`, answered as `Outcome::Workflow(WorkflowOutcome)` and `Outcome::Pack(PackOutcome)`. A stage that never asks for a family names it in one line, and the performer carries each family out in a module of its own (`performer/{run,workflow,pack}.rs`). `Operation::kind` is what kind of work it is, whatever it names, a family's included: what a reading asked again supersedes, and what the walk answers alike. The walk reaches the same 6,860 states before and after.
- **The walk** records the packs none installed and one installed (`support::with_a_pack_installed`), its audit, its removal last, and a new pack named and named by nothing: 6,860 states in about 8.5 s.

**Acceptance criteria:**
- [x] Each page is the one the matching `yunta pack` command prints.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test packs`

**Dependencies:** Task 46

**Files likely touched:** `crates/ui/src/{stage/catalog.rs,stage/catalog/packs.rs,performer.rs}`, `crates/ui/tests/packs.rs`

**Estimated scope:** M

### Task 48: a pack installed or updated from the copy its audit read

**Description:**
- **`Operation::FetchPack { source }` and `Operation::FetchUpdate { pack, reference }`** fetch a pack, and the performer holds the `FetchedPack` under its ticket.
  - The screen shows its inventory.
  - Where it ships executors, a confirmation names them.
- **`Operation::InstallPack { ticket }`** installs the held copy. It never fetches again.
- **The screen** shows `Installed` or `Updated`, with what the project lacks for its workflows.
- **Leaving before confirming** lets the ticket go, and the temporary checkout goes with it.

**Fails first:** `crates/ui/tests/packs.rs`:
- a local pack with an executor installed after its confirmation, from the commit its audit read;
- a pack left unconfirmed leaves no temporary checkout.

**Found in its turn:**
- **Confirmed after the audit.** The API took the confirmation of what a pack executes when it fetched it (`--yes`). `FetchedPack::confirmed` gives it once a person read the audit, `updating_to` says the ref an update moves to, and `checkout` where the copy stands; the fetched pack and its install are `packs/fetched.rs`, which `packs.rs` (past 500 lines) no longer holds.
- **The copy held.** `PackOperation::Fetch` and `FetchUpdate` keep the `FetchedPack` under their ticket: the performer's `held` keeps what work found as a `Found`, a project detected or a pack fetched, under one lock with the stops. `PackOperation::Install { held, confirmed }` takes that copy and vendors it, never fetching again; one let go fails with render's `PACK_LET_GO`. A stage lets a held copy go itself with `Want::Release`, and the state lets it go when the stage is left.
- **On the screen.** `i` asks where a pack is and `u` the ref the one pointed at moves to (`PackAsked`); the inventory is `yunta pack add`'s and `update`'s; Enter installs, a confirmation naming the executors first where they wait on a person; Escape lets the copy go, and its temporary checkout with it. An update that ships no executor installs as soon as it is fetched, as `yunta pack update` does. What installing came to is `Installed`, with what the pack's workflows need, or `Updated`, and the packs are read again. The install flow is `packs/install.rs`.
- **The walk** records a fetch of a pack with an executor, an update to a ref named by nothing, and an install from the copy one performer fetched: 7,832 states in about 9.5 s, the executors confirmed and a pack installed among them.

**Acceptance criteria:**
- [x] What is installed is the commit the audit read, even where the source moved between the two.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test packs`

**Dependencies:** Task 47

**Files likely touched:** `crates/ui/src/{stage/catalog/packs.rs,performer.rs,performer/held.rs}`, `crates/ui/tests/packs.rs`

**Estimated scope:** M

### Group check: the catalog
- [x] The suite's groups green; the walk covers the catalog and the packs.

### Task 49: what a run leaves

**Description:**
- **`Stage::Evidence(EvidenceStage)`:**
  - the receipt of a finished run, `Operation::WriteReceipt`, shown as `Receipt`;
  - a run's chain verified, `Operation::Verify`, shown as `Verified`;
  - its page in a browser, `Operation::WritePageForBrowser`, then `Request::ShowPage` with its path, and `Opened`.
- The run stage's actions that `RunFrame::actions` offers for these lead here.

**Fails first:** `crates/ui/tests/evidence.rs`: a finished mock run's receipt written, and its page asked to be shown at the path the API wrote.

**Found in its turn:**
- **What a run leaves is a family.** `Operation::Evidence(EvidenceOperation)`: `WriteReceipt`, `Verify` and `WritePageForBrowser`, each a reading (it derives from the log and writes beside the run, changing neither), answered as `Outcome::Evidence(EvidenceOutcome)`; the performer's `evidence` module calls the API's `receipt`, `verification` and `page_for_a_browser`, which `yunta receipt`, `verify` and `open` call.
- **Reached from the run.** The run stage binds the receipt a finished run offers to `e` and the verification a broken one offers to `v` (`RunFrame::actions`), and offers the page for a browser on `o` for every run, worded by `command::does`. Each opens `Stage::Evidence(EvidenceStage)` over the run; Escape goes back to it.
- **A page shown by its surface.** `Want::Show` asks `Request::ShowPage` with the path the API wrote, as soon as it is written; the stage shows `Opened` (what opening it came to is the surface's), and Enter shows it again.
- **The walk** records a finished run's receipt, verification and page, and binds `e`, `v` and `o`: 8,206 states in about 10 s, the receipt and the page among them.

**Acceptance criteria:**
- [x] The receipt's two files and `page.html` are those `yunta receipt` and `yunta open` write.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test evidence`

**Dependencies:** Group check: the catalog

**Files likely touched:** `crates/ui/src/{stage/evidence.rs,stage/run.rs,performer.rs}`, `crates/ui/tests/evidence.rs`

**Estimated scope:** M

### Task 50: Surco draws a group

**Description:**
- **Surco's flowchart reader reads a `subgraph`:** `subgraph <id>["<label>"]`, the nodes declared until its `end`, and a link that names the group's id, which links the group.
- **The terminal draws a group as a box holding its nodes,** laid out with the rest of the chart and titled with its label, in every glyph set. Markdown keeps the source as written; HTML draws the group as it draws the chart.
- A chart Surco still does not read keeps its source, as today.
- `page::Graph`'s goldens of a workflow with a `parallel` group draw the group, no longer its source; `yunta graph` prints the same text.

**Fails first:**
- Surco's flowchart tests: a subgraph read with its nodes and the links to and from it; a link inside the group;
- the terminal's goldens of a grouped chart, in every glyph set.

**Found in its turn:**
- **The reader reads groups.** `Flowchart.groups` holds each `Group { node, members }`: the node a `subgraph <id>[label]` names (a link may name it before it opens; a group opened twice is refused), the nodes named inside it until its `end`, a group inside another among them. A group's own `direction` is refused, so the chart keeps its source.
- **A chart taken apart a level at a time** (`diagram::nest::Nest`): outside, a group is one node; inside, its own chart; a link from or to a node inside a group is the group's at the outer level, with the node it starts or ends at (`Ends`). A link between a group and a node it holds is drawn by no surface: the chart keeps its source.
- **The terminal** places each level as it places a chart with no group, and draws a group as a box of lines titled on its top edge, its chart inlaid; a link crosses the edge from the node inside it, where the cells between are clear. **HTML** draws a dashed rect, its title, and the inner drawing translated inside. **Markdown** keeps the source.
- **Layering weighs where along a group a link leaves or reaches** (`layer::Sides`): without it, a link from a node inside a group and the group's own link tie and merge in one column.
- **What has no room** falls to the outline, which says each group and what it holds after the chains.
- **`yunta graph`** prints Mermaid or DOT as before; `page::Graph` draws the box. The HTML layout's links moved to `layout/links.rs`, the reader's tests to `flowchart/tests.rs`.

**Acceptance criteria:**
- [x] `page::Graph`'s grouped goldens draw the group; every other diagram golden is unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco`, `cargo test -p yunta-render`, `cargo test -p yunta --test graph_cmd`

**Dependencies:** Task 49

**Files likely touched:** `crates/surco/src/diagram/{flowchart.rs,layer.rs,place.rs,down.rs,across.rs,outline.rs}`, `crates/surco/goldens/`, `crates/render/goldens/graph/`

**Estimated scope:** L

### Task 51: what a run cost, and how a workflow is drawn

**Description:**
- **`Stage::Stats(StatsStage)`:** a run's `Stats` (`Operation::ReadStats`), or a workflow's `History` (`Operation::ReadHistory`), reached from a run or from the catalog.
- **`Stage::Graph(GraphStage)`:** `page::Graph` of a workflow (`Operation::ReadWorkflow`), or of a run, labelled with its frame.

**Fails first:** `crates/ui/tests/stats_graph.rs`:
- a finished mock run's stats;
- its graph labelled with each node's word.

**Found in its turn:**
- **Four readings, as the steps name them.** `Operation::ReadStats`, `ReadHistory`, `ReadGraph` (a workflow the catalog offers, by the name it offers it under, read as `yunta graph <name>` reads it) and `ReadRunGraph` mirror `NextStep::Stats`, `History`, `Graph` and `RunGraph`; they are answered as `Outcome::Stats(StatsReading)`, `Outcome::History(HistoryReading)` and `Outcome::Graph(page::Graph)`, the page owning what it is drawn from as `Run` does.
- **A workflow's history is read owned.** `WorkflowHistory` borrows the project, so the API reads it as a `HistoryReading` (`WorkflowHistory::reading`: each run's summary, and the findings of the workflow the latest run froze); `yunta stats --workflow` reads through it (its tests unchanged), and render's `History` borrows its findings.
- **Reached from the run and the catalog.** Every run offers `s` (what it cost) and `g` (its graph) beside `o`, worded by `command::does`; the catalog binds `g` (the graph as written) and `h` (the past runs, by the name the workflow's file gives it, which its runs are kept under; nothing for one that does not read). Each opens a stage over the one it came from.
- **`f` shows every row** of a run's stats, as `--full` does, and folds them again.
- **The legends a page opened over another shows are named by what they are:** `over_the_run_legend` (was `evidence_legend`) and `over_the_workflows_legend` (was `checked_legend`).
- **The walk** binds `s`, `g` and `h` and records each reading: 8,562 states in about 11 s, a run's stats and graph, a workflow's past runs (some and none) and its graph among them.
- **Not done here: scrolling** (Task 52). The graph's row says a person scrolls it, but no stage's main region scrolls (a run's page, a receipt and a check are as tall); only the plan desk's pane does, through `PaneLaid`.

**Acceptance criteria:**
- [x] The pages are those `yunta stats` and `yunta graph` draw.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test stats_graph`

**Dependencies:** Task 50

**Files likely touched:** `crates/ui/src/{stage/stats.rs,stage/graph.rs,performer.rs}`, `crates/ui/tests/stats_graph.rs`

**Estimated scope:** M

### Group check: what a run leaves and costs
- [x] The suite's groups green; the walk covers evidence, stats and the graph.

### Task 52: a page taller than its region scrolls

**Description:**
- **What a person asked to see is the state's; what fits is the surface's.** Surco's `Region` gains `top`, the first of its lines a person asked to see, as a desk's `Pane` has. A surface that lays a region out at a height hands back what it laid out, as it hands back a desk's pane, through `Input::Laid`; `PaneLaid` becomes what a surface laid out of any region that scrolls, named for that. Markdown and HTML draw the whole page, as today.
- **One owner of how a region scrolls.** What the plan desk does with what was laid out (a line, a page, the first and the last first line, never past the end) becomes one value of `yunta-ui`'s. The desk and every stage that shows a page hold it; nothing else moves a `top`.
- **Every stage whose main region is a page scrolls with it:** a run's page and a node's, the receipt, a check, a run's stats and a workflow's past runs, the graph. The stages after this task (doctor, upkeep, schema) hold it from the start. A page read again, as a followed run's is, keeps the line a person scrolled to, as far as the new page reaches.
- **The legend offers the scroll keys** where what was last laid out is taller than its region, in render's words.

**Fails first:** `crates/ui/tests/scroll.rs`:
- a run's graph laid out shorter than it is: PageDown moves its first line a page down, never past the last first line, and PageUp moves it back;
- a followed run's page read again keeps the line scrolled to;
- the plan desk's scroll tests pass unchanged.

**Found in its turn:**
- **Where a person scrolled to is the state's, against the screen's first word.** Surco's screen said an interface keeps where a person scrolled to; with the plan desk's mechanism chosen for every page, a region carries what of it a person asked to see, a `Sight` (its first line, and whether the line a pointer stands on stays in sight), and the screen's `//!` says so. The desk's `Pane` holds the same `Sight` in place of its two fields.
- **What was laid out is `Laid`, and of a screen `ScreenLaid`.** `PaneLaid` is `surco::components::Laid` (`Placed::laid`, `Answerable::laid`). A run's page stands beside its nodes, not in the main region, so a screen's input is `Input::Laid(ScreenLaid { main, side })`, and each stage takes what it scrolls: a decision's desk and a page from `main`, a run's page from `side`.
- **The terminal lays a scrolled region out in one place.** The desk's scroll rows are `Terminal::scrolled(doc, sight, rows)`, which the desk's pane calls and the TUI calls for each region; the desk's goldens are unchanged.
- **One owner in `yunta-ui`:** `scroll::Scroll` keeps the line asked and the last `Laid`, moves by the arrows, PageUp, PageDown, the space bar, Home and End, and rewinds when its region holds another page. Render's desk `Spot` no longer keeps `top`; `Reading::desk` takes it.
- **Where each stage stands.** Evidence, stats and the graph scroll their main region. The run scrolls its page beside by the page, the arrows moving along its nodes; it keeps the line when the same page is read again, and rewinds for a node's page, the run's again and a successor's. What a person watches a run by reads at its end (`Sight::at_the_end`). The field, the catalog, start, the first run, setup and a decision keep the row pointed at in sight (`Sight::following_pointer`).
- **The legend** leads with render's `scrolling` (`↑↓ scroll`, `pgup pgdn by the page`), or for a run `scrolling_beside` (`pgup pgdn scroll its page`), once what was laid out is taller than its region.
- **The walk** hands what an 80-by-4 terminal laid out of each state's screen as one more input, and tells states apart by where each region is scrolled to; `MUST_SCROLL` names the pages it must scroll (a run's page beside its nodes, its receipt, its stats, a workflow's past runs, a graph): 10,885 states in about 19 s. Laying the screen out after every input instead took 64 s.
- **Proof:** four tests in `scroll.rs` and four in Surco's `scrolled/tests.rs`; ten mutations, each caught by the test that names it.

**Acceptance criteria:**
- [x] Every stage that shows a page scrolls by what a surface laid out of it, through the one owner the plan desk scrolls through.
- [x] Markdown's and HTML's screens are unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test scroll`, `cargo test -p yunta-ui --test walk`, `cargo test -p yunta-surco`

**Dependencies:** Group check: what a run leaves and costs

**Files likely touched:** `crates/surco/src/components/{screen.rs,desk.rs}`, `crates/ui/src/{input.rs,stage.rs,prompt/plan_desk.rs}`, `crates/ui/src/stage/{run.rs,evidence.rs,stats.rs,graph.rs}`, `crates/render/src/screen.rs`, `crates/ui/tests/scroll.rs`

**Estimated scope:** L

### Group check: scrolling
- [x] The suite's groups green; the walk scrolls every stage that shows a page.

### Task 53: doctor

**Description:**
- **`Stage::Doctor(DoctorStage)`:** `Operation::Doctor { sessions: false }`, shown as `Doctor`, folded or every check.
- **A session per binding** is opened after a confirmation that says it spends a prompt each. Its remarks show as they come, and a person can stop it.
- **A checkouts finding over its budget** leads to upkeep.
- **Its page scrolls** through `Scroll`, as every page does, and the walk's `MUST_SCROLL` names it.

**Fails first:** `crates/ui/tests/doctor.rs`: the findings of a sandbox, unfolded; the sessions asked only after the confirmation.

**Found in its turn:**
- **Reached from the field with `m`** ("checks this machine" on its legend); the palette offers it too (Task 55). An empty field opens on the first run, so a test opens doctor over a field with a run.
- **`Operation::Doctor { sessions }`**, a reading without sessions and an act with them (they spend), carried out by the performer's `machine` module through the API's `doctor`, which `yunta doctor` calls; answered as `Outcome::Doctor(Vec<Finding>)`. The page is render's `Doctor` over those findings, so a test compares against the findings the performer handed back: doctor's own checks differ between two calls (the free disk, a new binary's first run on macOS).
- **A confirmed act goes back to the stage that asked for it** (`Stage::confirmed`): the state handed the operation straight to the performer, so the stage never knew its sessions ran. Each stage asks for the work itself; doctor stands as opening a session per binding, shows what the sessions remark as it comes, and Escape lets them go and says they were stopped (`SESSIONS_STOPPED`).
- **Its page scrolls** through `Scroll`; the walk binds `m`, records both operations, reaches the findings, the confirmation and the sessions stopped, and scrolls the findings: 14,670 states in about 27 s.
- **Not done here: a checkouts finding over its budget leading to upkeep**, whose stage is Task 54's; it is done there.
- Four mutations of the stage, each caught by the test that names it.

**Acceptance criteria:**
- [x] The page is the one `yunta doctor` and `--full` print.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test doctor`

**Dependencies:** Group check: scrolling

**Files likely touched:** `crates/ui/src/{stage/doctor.rs,performer.rs}`, `crates/ui/tests/doctor.rs`

**Estimated scope:** M

### Task 54: upkeep and the shape of a document

**Description:**
- **`Stage::Upkeep(UpkeepStage)`:** `Operation::Collect { dry_run: true }`, shown as `Collected`. After a confirmation, `Operation::Collect { dry_run: false }`, its remarks shown as they come.
- **`Stage::Schema(SchemaStage)`:** every document's shape, and the one picked, from core's `shape`, with no operation.
- **Both pages scroll** through `Scroll`, as every page does, and the walk's `MUST_SCROLL` names them.
- **Doctor leads here** where its checkouts finding is over its budget, or the disk under the part kept free (`u`, from Task 53), and reads its checks again when the person comes back.

**Fails first:** `crates/ui/tests/upkeep_schema.rs`:
- a sandbox's checkouts collected after the dry run's confirmation;
- `tasks`' shape picked.

**Found in its turn:**
- **Render's `screen` is split by family of stages** (`field`, `catalog`, `run`, `setup`, `start`, `machine`), as it neared 500 lines; every item moved whole, compared item by item, and every caller's path stays `screen::<item>`.
- **`Operation::Collect { dry_run }`**, a reading under a dry run and an act otherwise, carried out by the performer's `machine` module through the API's `collect`, which `yunta gc` calls; answered as `Outcome::Collected` (render's `Collected` now derives `Debug` and `Clone`).
- **Upkeep is offered on doctor's `u` always**, not only past a budget: its dry run removes nothing, the finding's own fix still names `yunta gc`, and the walk's checkouts never pass one. Doctor checks again when the person comes back (`Stage::returned` takes the stage mutably).
- **Collecting runs once confirmed** (`Stage::confirmed`), and Escape stops it. What a pass remarks stands above its page, as `yunta gc` says it before the page that counts "problems above"; doctor's remarks stand the same way.
- **The shapes open from the workflows with `s`.** The documents are a picker, each row with what it is for in the words `yunta schema` lists, narrowed by typing: a person picks from them, so the list is not `yunta schema`'s page itself. The one picked is render's `Schema::of`: a workflow's keys, or a document's annotated example, the text `yunta schema <name>` prints (`Schema::Artifact`, drawn as Markdown).
- **The walk** records both passes and reaches the dry run, the confirmation, the collection, the documents and a shape, scrolling upkeep and a shape: 17,908 states in about 33 s. Its time rises with each stage (11, 19, 27, 33 s).
- Four tests, one per promise; six mutations, each caught by the test that names it.

**Acceptance criteria:**
- [x] The pages are those `yunta gc --dry-run`, `yunta gc` and `yunta schema` print.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test upkeep_schema`

**Dependencies:** Task 53

**Files likely touched:** `crates/ui/src/{stage/upkeep.rs,stage/schema.rs,performer.rs}`, `crates/ui/tests/upkeep_schema.rs`

**Estimated scope:** S

### Group check: the machine
- [x] The suite's groups green; the walk covers doctor, upkeep and schema.

### Task 55: the palette

**Description:**
- **`Dialog::Palette(Palette)` over every stage,** opened by its key.
  - It lists the steps the stage offers, then those always there.
  - Each row has its line from `command::line` and what it does from `command::does`.
  - The rows are a `Picker<NextStep>`, and typed words filter them.
- **Choosing a row does what its key does on the stage**, or leads to the stage that does it.
- **An overlay taller than its box scrolls, as a region does:** `Overlay` gains a `Sight`, and `ScreenLaid` what was laid out of it; the palette keeps the row pointed at in sight.

**Fails first:** `crates/ui/tests/palette.rs`: on a parked run, the palette lists answering first and every always-there step after; "doc" filters to doctor.

**Found in its turn:**
- **Opened with `/`,** as the spike opened it, over any stage where nothing is typed: where words are typed, `/` is typed. Every stage's legend ends with it, through render's `over_every_stage`.
- **A stage offers `Act`s,** each a step as a command line names it and the key that takes it there (`Stage::offered`): the field (following, resuming, cancelling and closing the run pointed at, the run waiting on the person, the field's reach and its closed runs, the workflows, doctor), a run (what its frame offers, answering first, then the page of the node pointed at), the catalog and its packs (`catalog/acts.rs`), a page for a browser written, doctor and upkeep. A stage whose prompt is the act (a decision's card, a question, a form) offers none of its own; nor does a new pack, whose command needs the name it is given.
- **A row is taken by its key,** so choosing it does what pressing the key does (`Chosen::Pressed`); a test holds the two as one act. **What every stage offers** (`Stage::always`: the workflows, doctor, upkeep and the shapes) opens the stage that takes it (`Stage::opened_by`), and is not listed where the stage offers it already or is it (`Stage::showing`).
- **Each row is its command, spelled by `command::line`, with `command::does`;** typed words narrow the rows by their command.
- **An overlay has a `Sight`,** and `ScreenLaid` what a surface laid out of it: the palette keeps the row pointed at in sight, which a test holds on a terminal of four rows.
- **The walk** opens the palette over every state and takes every row of each palette it has not met, as an input it holds: 28 palettes, and `MUST_OFFER` names a row of every stage that offers acts and those every stage offers. Opening it as a stroke of every state would have multiplied the states by its rows. 17,908 states in about 34 s.
- Four tests; six mutations, each caught by the test that names it.

**Acceptance criteria:**
- [x] The walk holds every row of every stage's palette.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test palette --test walk`

**Dependencies:** Group check: the machine

**Files likely touched:** `crates/ui/src/{dialog.rs,dialog/palette.rs,state.rs}`, `crates/render/src/screen.rs`, `crates/ui/tests/palette.rs`

**Estimated scope:** M

### Task 56: help, and every confirmation

**Description:**
- **`Dialog::Help`** lists the keys of the stage and those always there, worded by `render::screen`, and scrolls through `Scroll` where it is taller than its box.
- **Every act that stops, closes, removes or spends stands behind `Dialog::Confirmation`,** saying what it does and the command that does the same. These are cancel, close, removing a pack, a pack's executors, doctor's sessions and collecting.
- Escape leaves a dialog with nothing done.

**Fails first:** `crates/ui/tests/dialogs.rs`: each of those acts asks before its operation, and Escape leaves it undone.

**Found in its turn:**
- **Which work needs confirming is the operation's** (`Operation::needs_confirming`): cancelling, closing, removing a pack, installing one whose executors were confirmed, a session per binding, collecting. The walk holds that a stroke asks such work only as Enter on a confirmation standing over the state (`walk/dialogs.rs`), the palette's rows too.
- **A confirmation names the command that does the same**, spelled by `command::line` under what it asks (`Confirmation::new(asks, same, then)`, render's `DOES_THE_SAME`). A pack's install is confirmed against the command that fetched it (`PackFetched::installs_as`: `pack add` with its source, or `pack update` with its ref), which the performer knows and the stage did not.
- **Help opens with `?`** over any stage where nothing is typed, and lists the keys of the screen under it, then those every stage answers to (`/`, `?`, Ctrl-C), as render's `help` table: it is built from that screen's legend, so it never says a key the screen does not take. It scrolls through `Scroll`, the overlay's `Sight` and what a surface laid out of it; Escape or `?` closes it. Every stage's legend ends with `/` and `?`.
- **A broken run's verification had no proof in the application:** `v` is offered only on a run whose log stopped making sense, which no test built. `evidence.rs` now breaks a run's log behind the engine's back (a node finished that never started, through `yunta-storage`'s test feature, a dev-dependency) and holds that its evidence is verified as `yunta verify` says it.
- **The walk** opens help over each legend it has not met and holds that it lists each key and closes with nothing done: 17,908 states, 28 palettes taken row by row and 64 helps opened, in about 36 s. Its dialogs moved to `walk/dialogs.rs`, its terminal to `walk/terminal.rs`.
- Seven tests; seven mutations, each caught by the test that names it, the unconfirmed cancel by the walk as well.

**Acceptance criteria:**
- [x] No operation that stops, closes, removes or spends is requested without a confirmation; the walk holds it.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test dialogs --test walk`

**Dependencies:** Task 55

**Files likely touched:** `crates/ui/src/{dialog.rs,dialog/confirmation.rs,state.rs}`, `crates/render/src/screen.rs`, `crates/ui/tests/dialogs.rs`, `crates/ui/tests/walk.rs`

**Estimated scope:** M

### Group check: the dialogs
- [x] The suite's groups green; the walk covers every stage and every dialog.

### Task 57: the proof

**Description:**
- The suite's count against the base's, by name: moved, renamed, new, and none gone without a reason.
- Every golden's digest; the captures, with `init -i`'s and the parked round's shown beside the base's.
- The walk's count of states and screens. The release binary's size by crate and by copy against the base's (`tools/size_by_crate.py`, `tools/duplicates.py`).
- All of it in `proof/app-core/after.txt`.

**Fails first:** none: a measurement.

**Found in its turn:**
- **The suite:** 4,751 against the base's 4,607. Of the 127 only in the base, 122 moved under another path with their names, five were renamed or split with what they hold (the desk's width to Surco, the search path to core, three of the chronicle's split by owner), each named in `after.txt`; none is gone without a reason. 149 are new.
- **The goldens:** 208 of the base's, none gone; 76 new. Seven changed, Surco's catalog of every block (`components/catalog.*`, `markdown/catalog.md`, `html/catalog.html`), by lines added and no line of the base's changed but the page's table of contents: a chart with a group (Task 50), the card (Task 7), the desk (Task 8), the choice list (Task 9), sections led by the pointer (Task 45) and a pack's row (Task 47).
- **The captures:** 155 views, nine differ. Eight say the version of Claude Code installed on this machine, 2.1.319 at the base and 2.1.322 now: the machine's. The ninth is `init -i` asking each runner's adapter and model (Task 29), with `prompt-init-i-declared.txt` new beside it. `resolve-gate` on a parked round asks the round and the run goes on (Task 23), where the base refused.
- **The walk:** 17,908 states, 28 palettes taken row by row and 64 helps opened, in about 35 s.
- **The release** on macOS is 28,433,584 bytes, 181,872 over the base's: its text grew 177,456 bytes, `yunta_core`, Surco, the API, `core` and `alloc` most, and `yunta_ui` new at 14,012; 275 names gained a copy, the clones of the event payloads and of a followed run that the application's outcomes clone among them. The static Linux binary is measured against its ceiling by CI on the pull request.
- The suite's `cli-4` failed once in the dialogs group check, `session_ends`' `cancel_with_its_engine_alive_leaves_no_command` waiting past its ten seconds for a fake `codex`'s first lines, right after a relink; alone it passed five times and its target three, and `cli-4` passed whole again. The first log is kept beside the proof.

**Acceptance criteria:**
- [x] Every difference from the base is one this list names; the static Linux binary stays under its ceiling (CI measures it on the pull request).

**Verification:**
- [x] Manual check: `after.txt` read whole.

**Dependencies:** Group check: the dialogs

**Files likely touched:** `.plans/yunta-tui/proof/app-core/` (outside git)

**Estimated scope:** S

### Task 58: a real run at the end

**Description:**
- As Task 3, from the final tree's release binary: followed with `yunta status --follow`, its gates answered with `yunta resolve-gate`, its time read from `yunta stats`.

**Fails first:** none: a measurement.

**Found in its turn:** run `01M75FC961DHT02FC7JNJKJBFM` waited at `ship` after 166 s with nothing asked before it (the base's 211 s, Checkpoint B's 115 s); `between_secs` 0.529 s, the base's 0.556 and Checkpoint B's 0.541; 2.15M tokens, 1.50M cached. Its follower wrote 156 lines and ended where the run parked (exit 3). With ids, times, counts and what agents said masked (`followers.py`), 111 kinds of line stand in both; the rest is what each run's agents did (an implementer that edited where the base's wrote, a reviewer that read a file where the base's called its tools), none a line the follower says differently.

**Acceptance criteria:**
- [x] The follower's lines equal the base's but ids and times; `between_secs` inside the base's spread.

**Verification:**
- [x] Manual check: the run's numbers beside the base's in `after.txt`.

**Dependencies:** Task 57

**Files likely touched:** `.plans/yunta-tui/proof/app-core/real-after*` (outside git)

**Estimated scope:** S

### Task 59: the review

**Description:**
- The module's diff, read in four parts by readers of their own:
  - the moves to render and the prompts' split;
  - the API's following, pages, questions and runners;
  - `yunta-ui`'s values, performer and walk;
  - the stages and dialogs.
- Each finding is checked against the code and the base, and recorded here with its decision.

**Fails first:** none: a review.

**Found in its turn** (four readers, one part each; each finding checked against the code and the base):
- **The moves to render and the prompts' split.**
  - `init -i`'s two new questions: named in this list (Task 29); no change.
  - A picker answered lost the option it answered, so the shapes' list came back without the document picked: fixed, the answer is the value pointed at and the list keeps every option (`upkeep_schema.rs`).
  - A field row's reason and words match the same arms twice: moved as the base had them; not this module's to change.
  - The prompts' look reads its motion from the terminal's environment: a prompt draws no motion; no change.
- **The API's following, pages, questions and runners.**
  - A session's words coming faster than a look reset the following's wait, so the log went unread: fixed, `Pace::beat_after` spaces a look from the last one (`pace.rs`'s test).
  - A successor made while the run was read was missed: fixed, the promotion chain is read before the run.
  - Events read before the registry could read a run that ended as stalled: fixed, the registry first, said at the line.
  - A CLI named by a relative path with a directory was looked for from the process's directory and started from the session's: fixed, resolved against where the session runs (`cli_search_path.rs`).
  - `init` wrote the base branch raw, so `#dev` read back empty: fixed, quoted as the name is (`config_yaml`'s test).
  - A following started before the run's first event skipped it: fixed, the whole log is read until an event is.
  - The successor's directory was worked out in the following and in the registry: fixed, `RunRegistry::run_dir` is the one.
  - The listening's doc said a refused socket is tried again once a second: it is tried at each look, and the doc says so.
  - `ProjectDraft::answered` takes any adapter, and `init -i` takes the detected default on a console that fails: every interface offers only adapters that answered, and the name and base branch are taken the same way; no change.
- **`yunta-ui`'s values, performer and walk.**
  - A reading asked again let go an act of its kind under way: doctor's sessions were dropped when doctor checked again on the way back from upkeep. Fixed: a reading supersedes only readings, and doctor checks nothing over its sessions (`state/tests.rs`, `doctor.rs`).
  - A let-go that came before the work's task first ran found nothing to stop: fixed, `take` holds the work as under way before it spawns it (`performer/tests.rs`).
  - Work that panicked never answered its ticket: fixed, it answers as failed, in render's words (`broke_inside`); no operation panics on demand, so no test drives it.
  - An answer reached whatever stage stood: a decision went back on another run's decision, a start gave way to a run another start asked for, a run followed again on another run's going on. Fixed: each takes only its own (`stage/tests.rs`).
  - A closing that failed partway lost what it had closed: fixed, `Outcome::Closed` carries the failure beside what closed, and the field is read again (`field.rs`).
  - The walk's "a confirmation" matched other screens' words: it now matches what only a confirmation says.
  - The walk never handed a remark: it hands one of each piece of work under way, beside the walk, which never panics and asks nothing (walked on, a stage that keeps remarks would never end); its clock standing still is said in its doc.
  - Strokes tried only where one piece of work runs, states told apart by their screen, recordings answered by kind, HTML drawn but not compared, a confirmation's operation not compared with what it shows: the walk's bounds, each in its doc; an answer under a ticket let go is dropped by the state, which `field.rs`'s readings hold.
  - The crate's doc said it names no store, which a test now does: it says the library.
- **The stages and dialogs.**
  - Leaving the packs did not read the workflows again: fixed (`packs.rs`).
  - No page standing in the catalog's place scrolled: fixed, a workflow's check, the cases, a pack's audit, a fetched pack's inventory and what installing one came to scroll by their keys and start at their first line when another stands (`catalog/page.rs`; `scroll.rs`; the walk scrolls a pack's audit).
  - Setup's and start's question could stand below what shows: fixed, their region reads from its end with the pointer in sight (`Sight::at_the_end_with_the_pointer`).
  - The palette's close row named the run pointed at while `x` closed the runs picked: fixed, it names the one run `x` closes, and none where several are picked (`palette.rs`).
  - Resuming asks no confirmation, and Escape stops a collection or the sessions with none: the acts confirmed are those this list names (Task 56), and stopping work under way is as Ctrl-C stops a command; no change.
  - Escape's taking the first, written by hand in setup and a new workflow: both take the picker's `first`.
  - A run followed again kept a node's page under "stops following": fixed with the routing.
  - A collection stopped left the dry run's list on offer: fixed, the dry run runs again.
  - Doctor's legend offered `s` while its checks ran: fixed (`DoctorChecks`).
  - The dialogs' test presses `c` and `x` only: the walk holds the rule for every key.
  - `LABEL_CELLS`' reason line: Surco's before this module; not changed here.
- Eleven mutations of the fixes, each caught by the test that names it; the walk reaches 17,304 states in about 35 s.
- **The release after the fixes**, captured again against the base: the same nine views differ (the machine's Claude Code version, and `init -i`'s runners), and it is 28,450,112 bytes, 198,400 over the base's; `after.txt` holds the final suite, goldens, walk, captures, size and real run.
- **The suite after the fixes:** 4,765 green. `testkit-core`'s `a_pid_never_recorded_ends_the_wait_instead_of_hanging_it`, which this module does not touch, waited past its ten seconds once in the `crates` group; alone it passed five times and the group passed whole again. The first log is kept beside the proof.

**Acceptance criteria:**
- [x] Every finding is recorded here with its decision.

**Verification:**
- [x] Tests pass: the suite's groups after the fixes.

**Dependencies:** Task 58

**Files likely touched:** as the findings say

**Estimated scope:** M

### Checkpoint D: the module
- [x] Every stage of the spec works end to end against its sandbox; the walk covers every stage and dialog; what prints is the base's but the two changes this list names.
- [x] The user reviews the proof and the review.
- [x] The phase commits and is pushed; Task 60 follows, then `cargo clean`.

### Task 60: the pull request

**Description:**
- PR #30's body gains the module; CI on Linux and macOS.

**Found in its turn:** The walk's one-minute cap failed CI on Linux: its runner walks the same 17,294 states in 108 s that this machine walks in about 40 s. A clock bounds the machine, not the walk, so the cap is gone and the walk is bounded by the states it visits (`MOST_STATES`), alike on every machine.

**Fails first:** none.

**Acceptance criteria:**
- [ ] CI green on Linux and macOS.

**Verification:**
- [ ] Manual check: `gh pr checks 30`: ci, musl-build, test-release and test-macos pass.

**Dependencies:** Checkpoint D

**Files likely touched:** PR #30's body

**Estimated scope:** XS
