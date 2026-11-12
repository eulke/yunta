# Tasks: `tui`

Spec: `SPEC-tui.md`. Plan: `tasks/plan.md`, approved on 2026-11-11. Status: **approved on 2026-11-11, in three phases.** Base: `152fc892` (`feature/yunta-tui`, where `app-core` ended).

**How these run:**
- One at a time, with `incremental-implementation` and `test-driven-development`, in three phases, each ending at a checkpoint for the user.
- Inside a phase, each group of tasks ends at a group check: the suite's groups green, and the digests and captures against the base where the group touches them. A group check does not stop for the user.
- Inside a phase, the work stops for the user only on a decision that is theirs: a printed text that would change beyond what this list names, a choice neither the plan nor this list makes, a cap that would rise, or a failure whose cause is not found.
- `cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.
- Before editing a file, `cargo xtask docs --about <files>`; each rule, trap and note it prints is kept true.
- A commit closes each phase, once the user approves its checkpoint and every step of the `ci` job passes, one build at a time with `CARGO_INCREMENTAL=0`. Paths are staged by name. Then `cargo clean`.
- The engine's and the CLI's suites run in foreground batches: `python3 .plans/yunta-tui/tools/suite.py <group>`.
- What a task proves outside the repository goes in `.plans/yunta-tui/proof/tui/`.

**How an item moves:**
- Its names and its form change first, in place, with every caller. Then it moves, with every caller's path.
- Neither step leaves an alias, a re-export or an old name, and the workspace compiles and passes after each task.
- A task that moves a file does it with `git mv` before it edits it. A file that is only partly moving is split: the moving part goes, the rest stays.
- A task that moves an item touches every caller in one step, so it can be larger than five files. Each such step is mechanical, the compiler names every caller, and the moved bodies are read with `git diff -M` against their old files.

**Where each test lives:**
- **Surco's new lines:** unit tests beside each module, and goldens in `crates/surco/goldens/`, ASCII and Unicode at 60 and 80 cells.
- **`yunta-ui`'s additions:** `crates/ui/tests/`, through `State` alone, and the stage's own `tests.rs`.
- **`yunta-tui`'s pure parts** (`layout`, `cells`, `keys`): unit tests beside each module.
- **A screen drawn whole:** `crates/tui/tests/screens.rs`, on Ratatui's `TestBackend`, against the target frames in `crates/tui/goldens/screens/`.
- **The terminal held and the loop:** `crates/tui/tests/`, each test running its own binary again in a pty through `yunta-testkit`'s `Terminal`.
- **The binary at a terminal:** `crates/cli/tests/application.rs` and `crates/cli/tests/application/`, one file per slice, through the same pty.
- **The walk:** `yunta-testkit-ui` walks; `crates/ui/tests/walk.rs` holds the application's rules over it, and `crates/tui/tests/walk.rs` lays every state out.

**What fails first:**
- A move has no new behavior, so no test can fail for it. Its guard is every test and golden it touches, passing unchanged, and the base's digests, captures and walk counts (Tasks 1 to 3).
- A new behavior, a new test or a changed text is seen red, for its own cause, before it passes.
- A test of behavior the earlier tasks already compose (a run followed, a gate answered, a stage reached in the binary) is seen red with its rule broken for the run, then the rule is put back.

**Decided in this list, beyond the plan, for the user's approval with it:**
- **The words a surface needs reach `yunta-tui` through `yunta-ui`** (Task 12). The room a screen needs is render's sentence, offered as `yunta_ui::room_needed`. `yunta-tui` depends on no render, as the spec's list of its crates has it, and its manifest test refuses `yunta-render` too.
- **One rule decides where the application opens, for bare `yunta` and `review` alike:** stdin and stdout at a terminal, and `TERM` not `dumb` (Tasks 27 and 37). For `review`, this changes two edges of the base:
  - with stdout piped and stderr at a terminal, the base holds the desk on stderr; `review` now prints the plan's page;
  - with stderr piped, the base warns that nothing was asked and prints the page; `review` now opens the application on stdout.
- **The application keeps the wheel the plan's desk has** (Task 21). While the terminal is held, the wheel sends arrows on the other screen (Surco's `ALT_SCROLL_ON` and `ALT_SCROLL_OFF`), and the mouse is not captured. `docs/guide.md` documents the wheel at the desk, and `review` moves into the application.
- **`ask::decide` loses `opens` with `chosen_on`** (Task 38). With `review` in the application, the CLI opens every decision on its card.
- **`drawn` is public** (Task 18): a Yunta screen drawn into a Ratatui frame. `yunta-tui`'s tests and its walk draw through it. `Application` stays the one item the binary names.
- **The target screens are states built from samples at a fixed instant** (`yunta_testkit_ui::screens`, Task 13), so the frames read alike on every machine.
  - A probe outside the repository draws their parts (Task 14).
  - The frames are `yunta-tui`'s goldens, written by the target script and never blessed from the Rust.
- **`State::TIME_EVERY`, `Input::Unshown` and the opener's own module come in Phase 2** (Tasks 22, 23 and 26), before the loop and bare `yunta` hand them in, so the loop is whole when it lands.
- **`review`'s two pty tests at a terminal change** (Task 37). `esc` from the plan walks back to the run, and `q` leaves, as the user chose with the spec. `review_opens_the_desk_and_esc_leaves_the_run_waiting` and `review_answers_the_decision_the_way_resolve_gate_does` become the application's review tests.

## Phase 1: the design proven before the code

Its groups, each green before the next: the base; Surco's parts; the crates; the targets.

### Task 1: the proof's tools, pointed at this module

**Description:**
- `goldens.py`, `pages.sh`, `normalize.py`, `documents.py`, `mcp_answers.py`, `prompts.py`, `follow.sh`, `followers.py`, `real_run.py`, `base_suite.sh` and `prints.sh` are copied from `proof/app-core/` into `proof/tui/`, unchanged but for their folder.
- `screens.py` drives, at an 80×24 pseudo-terminal in a sandbox of its own, keeping every byte written:
  - bare `yunta` in a repository with runs, which prints the field at the base;
  - bare `yunta` outside a repository, which prints the signpost;
  - `yunta review <run>` on a run parked on a plan, then `esc`.
- `first_screen.py` times bare `yunta` at a pseudo-terminal, from its start until the field's last run is on screen, and prints the median of 20.
- `prints.sh` runs `screens.py` beside the rest into one capture directory, which `normalize.py` compares whole.

**Found in its turn:** `screens.py` and `first_screen.py` share `at_a_terminal.py`, the pty harness `prompts.py` keeps for itself; a key is sent only once what the process writes has settled, so a binary that prints and ends, as the base's, is never typed to. The signpost is captured in the system's temporary directory, since the capture directory stands inside the worktree's repository. `goldens.py` covers the engine's goldens too (6), and the terminal application's once it has some.

**Fails first:** none: tools.

**Acceptance criteria:**
- [x] Each tool runs twice on the base and finds nothing between its two runs, once normalized.

**Verification:**
- [x] Manual check: `python3 normalize.py` over the two runs prints no difference.

**Dependencies:** None

**Files likely touched:** `.plans/yunta-tui/proof/tui/` (outside git)

**Estimated scope:** M

### Task 2: what the base prints and weighs

**Description:**
- The suite's binaries and tests by name, in `suite.py`'s groups, into `proof/tui/base-suite/`.
- Every golden's digest, `digests-base.txt`.
- A release build, `proof/tui/yunta-base`, its size, and its size by crate from a symbol build (`tools/size_by_crate.py`).
- `prints.sh` from the release build into `base-1/`, then again into `base-2/`.
- The walk's count of states, palettes and helps, and its time on this machine.
- `first_screen.py`'s median for the base's bare `yunta`.
- All of it in `proof/tui/baseline.txt`.

**Found in its turn:** the suite by name is `app-core`'s last (`proof/app-core/suite-review`, 4,763 tests ok): since it ran, the tree changed only the lockfile, the walk's bound and the task list, and no test was renamed. 214 goldens; the release is 28,450,128 bytes; 158 views twice equal; the walk reaches 17,304 states, 30 palettes and 70 helps in 35 s; bare `yunta` prints the field at a pty in 26.3 ms, the median of 20.

**Fails first:** none: measurements.

**Acceptance criteria:**
- [x] `base-1` and `base-2` are equal once normalized; the counts, sizes and times are written in `baseline.txt`.

**Verification:**
- [x] Manual check: `python3 normalize.py base-1 base-2` prints no difference.

**Dependencies:** Task 1

**Files likely touched:** `.plans/yunta-tui/proof/tui/` (outside git)

**Estimated scope:** S

### Task 3: a real run followed on the base

**Description:**
- fragua is started detached from the base's release binary in the sandbox repository, on the CLI and model `app-core`'s runs used (Codex on `gpt-6-luna`), with the forge's token pointing at a repository that does not exist.
- It is followed with `yunta status --follow` through a pipe from its start, and stopped before its `pr` node. Its gates are answered with `yunta resolve-gate`.
- Its time between nodes and its tokens are read from `yunta stats <run>`.
- The follower's lines, normalized for ids and times, go into `real-base-follow.txt`. The run's id, time and tokens go into `baseline.txt`.

**Found in its turn:** run `01M76698PPEHGH9415YMBS56Z5` waited at `ship` after 161 s with nothing asked before it; `between_secs` 0.559 s; 1.13M tokens, 0.76M cached. The follower wrote 157 lines and ended where the run parked (exit 3); `followers.py` compares it with a later run's follower, so it is kept as written (`real-base/follow-1.txt`).

**Fails first:** none: a measurement.

**Acceptance criteria:**
- [x] The run reaches the node before `pr`; the follower ended where the run stopped; `between_secs` and tokens are recorded.

**Verification:**
- [x] Manual check: `yunta stats <run>` read back against `baseline.txt`.

**Dependencies:** Task 2

**Files likely touched:** `.plans/yunta-tui/proof/tui/real-base*` (outside git)

**Estimated scope:** S

### Group check: the base
- [x] The base's suite, digests, captures and walk counts stand, twice equal.
- [x] The real run's follower lines, time and tokens are recorded.

### Task 4: a region drawn in a pane

**Description:**
- **`Terminal::pane(&Region, rows) -> Paned`**, in `surface/terminal/pane.rs`, at the look's exact width: the one rule for how a region is drawn.
  - A region whose document holds a prompt's block (a card, a desk, a typed answer, a choice list) draws the blocks above the prompt first. It then holds the prompt with `held`, fitted to the rows they leave, with the cursor counted from the pane's first row.
  - Any other region is scrolled from its `Sight`, as `scrolled` draws it.
- **`Paned { lines, cursor, laid }`** is what a pane hands back. `Placed` stays `held`'s.
- **The walk's test terminal** (`crates/ui/tests/walk/terminal.rs`) lays out every region through `pane`, so the walk and every surface draw a region by the same rule.
- Each `pub` item has its `///`, with a "Not `Placed`" sentence on `Paned`.

**Found in its turn:**
- **The row a prompt never draws on is the inline hold's.** A card, a desk and a choice list each kept the terminal's last row free, the row the cursor is left on; `Terminal::held` keeps it now, and each prompt fills the rows it is given (`held_in`), so a pane loses none. Every test that drew a prompt at a set height draws it at one row fewer, or holds it on its terminal, and says what it said.
- **What stands above a prompt keeps at most half the pane**, from the line nearest the prompt up; what stands under it stays whole. `Doc::prompt_at` says where the prompt stands, as `Doc::decision_at` says where the decision does.
- **`Paned::laid` is what scrolls**: a desk's pane, a scrolled page's lines; nothing for a card, which does not scroll as a page.
- **A choice list still keeps two rows its own lines do not take:** one for the question its caller says above it, which the CLI's hold prints and a pane draws as a block of its own, and its filter's, which it counts twice. A list in a pane has two rows fewer than it could. Giving them back changes what the CLI's pickers print on a short terminal, so it waits for the user at Checkpoint A.
- The terminal tests' samples (`card`, `eight_rows`, `desk`, `numbered`) are shared within the terminal surface, not beyond it.
- The walk reaches 17,304 states, 30 palettes and 70 helps, as at the base.

**Fails first:** `pane/tests.rs`:
- a card under a paragraph is held in the rows the paragraph leaves, with the cursor on the field's row counted from the pane's top;
- a region of prose in fewer rows than it takes scrolls as `scrolled` does, counting the lines below.

**Acceptance criteria:**
- [x] Goldens of a held card, a held desk and a scrolled page, ASCII and Unicode at 60 and 80 cells, are reviewed in the diff.
- [x] The walk reaches the base's count of states, or each difference is named in this task with its cause: a desk laid out as its pane hands back what `held` laid out.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco pane`, `cargo test -p yunta-ui --test walk`

**Dependencies:** Group check: the base

**Files likely touched:** `crates/surco/src/surface/terminal/{mod.rs,pane.rs,pane/tests.rs}`, `crates/surco/goldens/pane/`, `crates/ui/tests/walk/terminal.rs`

**Estimated scope:** M

### Task 5: the trail, the rule and a region's title

**Description:**
- **`surface/terminal/screen.rs`** holds a screen's rows that are no region's document, as a terminal draws them. It starts with three:
  - `Terminal::trail(&[String], &[Count]) -> Line`: the places from the outermost, joined by the glyph set's `within` mark, and the tally at the row's end, each count with its standing's mark and tone. A short row cuts the trail first and keeps the tally whole.
  - `Terminal::rule() -> Line`: the empty furrow across the width, under the trail.
  - `Terminal::title(&str, focused) -> Line`: a region's title, strong where the keys act on the region and muted elsewhere;
  - `Terminal::edge() -> Line`: the edge between the side region and the main one, ` │ `.

**Found in its turn:** the edge between the side region and the main one is a mark too, so Surco draws it (`Terminal::edge`, faint, its cells what the layout leaves for it); the target script found it. The rule is the furrow of no node, which a page about no one run already stands under, so it is drawn by `Furrow`; the desk's `at_ends` and `APART` move up to the terminal surface, where the trail and the desk set a row's two ends by one rule; each count is one span, its mark and its words in its standing's tone.

**Fails first:** `screen/tests.rs`:
- at 60 cells, a trail too long for the row is cut with the ellipsis and the tally stands whole at the row's end;
- a title is strong when focused and muted otherwise;
- the rule takes every cell with the furrow's `ahead` glyph.

**Acceptance criteria:**
- [x] The rows equal the reference's trail and rule rows at 80 cells in ASCII and 60 in Unicode, in goldens reviewed in the diff.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco screen`

**Dependencies:** Task 4

**Files likely touched:** `crates/surco/src/surface/terminal/{mod.rs,screen.rs,screen/tests.rs}`, `crates/surco/goldens/screen/`

**Estimated scope:** S

### Task 6: the attention row and the legend row

**Description:**
- **`Terminal::attention(&Attention) -> Line`**, in `attention.rs`: one row, tinted to the width, its words cut with the ellipsis where they do not fit, and the act it offers kept whole at the end.
- **`Terminal::legend(&Legend) -> Line`**, in `legend.rs`: one row of the bindings in their order where they all fit; where they do not, the first ones that fit, a mark that more stand between, and the last one, which leads to the rest.
- The block lines `attention.rs` and `legend.rs` draw in a document stay as they are.

**Found in its turn:** the attention block already draws one row that keeps its act, and leaves its tint to the screen: the row is that row, filled as a tint. The legend's lines already break between bindings and never inside one: the row is the first of them, cut where a first binding is wider than the row. Both rows join the rows' goldens.

**Fails first:** `attention/tests.rs` and `legend/tests.rs`: the reference's attention at 60 cells stays on one row ending in its act, cut before it; a legend wider than its row keeps its first bindings whole.

**Acceptance criteria:**
- [x] Goldens of both rows at 60 and 80 cells, ASCII and Unicode, are reviewed in the diff; the base's goldens are unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco attention legend`

**Dependencies:** Task 5

**Files likely touched:** `crates/surco/src/surface/terminal/{attention.rs,attention/tests.rs,legend.rs,legend/tests.rs}`, `crates/surco/goldens/screen/`

**Estimated scope:** S

### Task 7: the overlay boxed

**Description:**
- **`Terminal::boxed(&Overlay, rows) -> Paned`**, in `surface/terminal/boxed.rs`, at the look's exact width, which is the box's:
  - the top edge holds the overlay's title, in `Shape::Box`'s corners and level (`+` and `-` in ASCII);
  - each row between the uprights holds a line of the overlay's document, scrolled from its `Sight` in the rows the edges leave;
  - the bottom edge closes it.
- What it laid out of the document comes back in `Paned::laid`, for the next key to scroll from.

**Found in its turn:** what the box holds is drawn by the pane's rule (`Terminal::paned`, which `pane` draws a region by), so a palette's list is held in its box as a picker is in a region; the box is as tall as what it holds, up to its rows. Its marks are the diagram's (`Shape::Box`'s corners, `Stroke::Open`'s level and upright), in `Edges`. The walk's terminal boxes what is open over a screen, with the rows inside it the regions get, and reaches the same 17,304 states.

**Fails first:** `boxed/tests.rs`: the reference's help box at 56 cells, its title in the top edge and its last row the bottom edge; a palette taller than its rows scrolls inside the box.

**Acceptance criteria:**
- [x] Goldens of the palette and help boxed, ASCII and Unicode, are reviewed in the diff.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco boxed`

**Dependencies:** Task 6

**Files likely touched:** `crates/surco/src/surface/terminal/{mod.rs,boxed.rs,boxed/tests.rs}`, `crates/surco/goldens/boxed/`

**Estimated scope:** S

### Group check: Surco's parts
- [x] The suite's groups green; every base golden unchanged, each new one reviewed; the walk's counts against the base's.

**Found in its check:** render's desk golden drew the desk at 36 rows, meaning a terminal of 36; with the reserved row moved to `held`, it draws at 35, and the golden is the same. 214 goldens unchanged, 18 new; the suite is the base's by name and 20 new tests; the walk 17,304 states.

### Task 8: the crate `yunta-tui`

**Description:**
- **`crates/tui`**, package `yunta-tui`, library `yunta_tui`, joins the workspace.
  - Its manifest names, each from the workspace's table with its reason: `yunta-core` (the clock it is handed), `yunta-ui`, `yunta-surco`, `ratatui` 0.30.2 (`default-features = false, features = ["crossterm"]`), `crossterm` 0.29.0 (`event-stream`), `tokio`, `futures`, and `nix` (`signal`, to stop itself on Ctrl-Z).
  - Its `//!` says what the crate is: the application at a terminal, cells and nothing else, composing no word, tone or mark. Its map grows with each task that adds to it.
- **`crates/tui/tests/manifest.rs`:**
  - `yunta-tui`'s `[dependencies]` name none of `yunta-api`, `yunta-engine`, `yunta-storage`, `yunta-adapters`, `yunta-render`, `console` or `indicatif`. Each refusal says where such a reading belongs.
  - No other crate's manifest under `crates/`, nor `xtask`'s, names `ratatui` or `crossterm`. The workspace's own table holds their versions, as it holds every crate's.
- **`deny.toml`:** a `[[licenses.exceptions]]` entry for `foldhash` alone, allowing `Zlib`, with its reason, and a `skip` for `itertools:0.13.0` beside `shlex`'s, with its reason. Nothing is allowed workspace-wide.

**Fails first:**
- the manifest test, with `console` added to `yunta-tui`'s manifest for the run, fails naming `console`;
- the second test, with `crossterm` added to `yunta-render`'s manifest for the run, fails naming `yunta-render`;
- `cargo deny check` refuses `foldhash` and the second `itertools` before `deny.toml` changes, and passes after.

**Found in its turn:** the workspace's table holds `ratatui` (only its crossterm backend) and `crossterm`, as it holds every crate's version, and the crate's manifest takes their features; the test reads every crate's manifest but this one's. Each test was seen red with its rule broken (`console` in this manifest, `crossterm` in render's), and `cargo deny check` refused `foldhash`'s license and the second `itertools` until `deny.toml` named them.

**Acceptance criteria:**
- [x] One `unicode-width`, one `crossterm` and one `ratatui` in `Cargo.lock`; `cargo deny check` passes.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --test manifest`
- [x] Build succeeds: `cargo deny check`

**Dependencies:** Group check: Surco's parts

**Files likely touched:** `Cargo.toml`, `Cargo.lock`, `crates/tui/{Cargo.toml,src/lib.rs,tests/manifest.rs}`, `deny.toml`

**Estimated scope:** M

### Task 9: the crate in the checks and the maps

**Description:**
- `yunta-tui` joins the `ci` job's isolation loop, `tools/gate.sh`'s, and `tools/suite.py`'s crates.
- `.agents/architecture.md` gains `yunta-tui`'s row (cells: the application at a terminal; `core, ui, surco`), and `yunta`'s row gains `tui`.
  - The file holds 6,116 of its 6,144 bytes. The row's bytes come out of what the file says twice or a `//!` already says, in this task.
  - Without such a sentence, the work stops for the user rather than raising the cap.
- `CAPABILITY-MAP.md`'s crate table and `tui` row say the crate's edges, with `yunta-core` among them.

**Found in its turn:** `yunta`'s row gains `tui` when the binary first depends on the crate (Task 27), since the table holds the manifests at every task. The bytes of the row and of `crates/tui/src/**` among the paths the Surco rule's review covers came from two sentences said elsewhere: that a crate's map is its root's `//!` (`AGENTS.md`), and that `cargo xtask docs --check` holds the table to the manifests (the check's own doc). 6,125 of 6,144 bytes.

**Fails first:** none: the checks' lists and the maps; `cargo xtask docs --check` holds the cap.

**Acceptance criteria:**
- [x] `cargo xtask docs --check` passes with the cap unchanged.

**Verification:**
- [x] Build succeeds: `cargo xtask docs --check`, `cargo check -p yunta-tui --locked`

**Dependencies:** Task 8

**Files likely touched:** `.github/workflows/ci.yml`, `.agents/architecture.md`, `CAPABILITY-MAP.md`, `.plans/yunta-tui/tools/{gate.sh,suite.py}` (outside git)

**Estimated scope:** S

### Task 10: `yunta-testkit-ui`, and the sandboxes the application's tests share

**Description:**
- **`crates/testkit-ui`**, package `yunta-testkit-ui`, joins the workspace, `publish = false` as the other testkits.
  - Its manifest names `yunta-ui`, `yunta-surco`, `yunta-core`, `yunta-replay`, `yunta-render`, `yunta-api`, `yunta-testkit`, `yunta-testkit-core`, `yunta-storage` (`testkit`), `tempfile`, `tokio` and `chrono`, each with its reason.
  - Its `//!` says what it is: what every surface's tests of the application share.
- **`sandbox.rs`** takes, from `crates/ui/tests/support/mod.rs`, what makes a test's world:
  - the workspace a test works in, and the one on a `PATH` of its own;
  - the CLIs it stands in for;
  - a pack installed, and a pack with an executor;
  - the instant every test of the application stands at, named for what it is.
- Every caller in `crates/ui/tests/` reads them from `yunta_testkit_ui::sandbox`. `support/mod.rs` keeps the driving, the field made up, and the work performed in a bench's world.
- `yunta-ui`'s `[dev-dependencies]` gain `yunta-testkit-ui`.
- The crate joins the `ci` job's isolation loop, `gate.sh`'s and `suite.py`'s.
- `.agents/architecture.md` gains its row beside the other testkits, its bytes taken as Task 9 takes them.

**Found in its turn:**
- **`drive` is the same, line for line, in `yunta-ui`'s support and the CLI's interface harness, and so is the instant both stand at, which `state.rs` wrote a third time.** Each has one home now: `yunta_testkit_ui::driving::drive` and `sandbox::now`, which the CLI's interface tests read too (`yunta`'s dev-dependencies gain the crate). The CLI's own workspace stays its own: its work runs this build's binary.
- The crate names each dependency as a task first uses it, so its manifest holds none it does not read; the walk's come with Task 11.
- A test binary that reads nothing of `support` no longer declares it (`setup`, `first_run`, `performer`, `walk`).
- `.agents/architecture.md`'s row came from what the file said twice: the adapters' names, which their row says, and replay deriving every state, which a cross-cutting rule says; and from the parts of a task's contract written as a list, which their type holds. 6,133 of 6,144 bytes.

**Fails first:** none: a move. Its guard is every test of `yunta-ui` passing unchanged, by name.

**Acceptance criteria:**
- [x] `crates/ui/tests/support/mod.rs` holds nothing `sandbox.rs` holds; `cargo xtask docs --check` passes.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui`, `cargo test -p yunta-testkit-ui`

**Dependencies:** Task 9

**Files likely touched:** `Cargo.toml`, `Cargo.lock`, `crates/testkit-ui/{Cargo.toml,src/lib.rs,src/sandbox.rs}`, `crates/ui/Cargo.toml`, `crates/ui/tests/support/mod.rs` and every test that reads what moves, `.github/workflows/ci.yml`, `.agents/architecture.md`

**Estimated scope:** L (a move)

### Task 11: the walk, for every surface

**Description:**
- **`git mv`** of `crates/ui/tests/walk/{home.rs,recorded.rs,terminal.rs}` into `crates/testkit-ui/src/`, and of the walk's breadth-first search out of `crates/ui/tests/walk.rs` into `crates/testkit-ui/src/walk.rs`.
- **`Walk`** walks from the application as it opens:
  - each stroke, the laid-out screen from its terminal of a few rows, and each recorded piece of news and answer;
  - each state kept once, by its screen and the kinds of work it waits on, as at the base;
  - its bound in states, not seconds.
- **What a test holds over the walk is a `Checks` of its own**, with two hooks:
  - each state the walk first reaches, with how it was reached (`Reached`: the inputs from the opening, one a line);
  - each input handed, with the state it was handed to and what it asked.

  The walk catches a panic and names the inputs that reach it.
- **`crates/ui/tests/walk.rs`** keeps the application's rules as its checks, unchanged:
  - every request carried out, every screen drawn on Markdown and HTML;
  - `MUST_REACH`, `MUST_SCROLL`, `MUST_OFFER`;
  - the palettes taken row by row, the helps opened, the remarks;
  - nothing unconfirmed (`walk/dialogs.rs`).
- Names checked against those taken: `Walk` is crate-private in the engine and Surco; `Checks` and `Reached` are free.

**Found in its turn:**
- `Walk::over(&recordings).walked(&mut checks)` walks and says how many states it reached; `Checks` has the two hooks, and `reached` is handed the screen as Markdown draws it, which the walk keeps states apart by, so a check does not draw it again. `Reached::inputs` names the way to a state, and `Reached::then` the step a check takes off the walk (a palette's row, help, a remark).
- `Recordings::carried_out` holds that every piece of work asked is one the recordings answer, for the walk and for the interface's palette check alike.
- `Input` is `Clone`: the walk hands one to a copy of a state and to its checks.
- The rule that no work which stops, closes, removes or spends is asked but on its confirmation reads the input it was handed, not its spelling.
- Where a page stands scrolled is the interface's rule (`MUST_SCROLL`), so it stays in `crates/ui/tests/walk.rs`; the terminal of a few rows moved, and `terminal.rs` was split.
- In a crate's `src`, the walk's places are written with tokio's file calls, as the smell counter `sync_fs_in_async` holds, and `Home::built` splits where its places are made.
- The walk reaches 17,304 states, 30 palettes and 70 helps; Tab made to panic on the signpost was found by `opened`, `Outcome(Ticket(1), Repository(Absent))`, `Stroke(Tab)`.

**Fails first:** none: a move. Its guard:
- the walk's counts of states, palettes and helps equal the base's;
- a stage made to panic on a stroke, for the run, is found with the inputs that reach it, as at the base.

**Acceptance criteria:**
- [x] `yunta-ui`'s walk holds every rule it holds at the base, on the same states.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test walk`, `cargo test -p yunta-testkit-ui`

**Dependencies:** Task 10

**Files likely touched:** `crates/testkit-ui/src/{lib.rs,home.rs,recorded.rs,terminal.rs,walk.rs}`, `crates/ui/tests/{walk.rs,walk/dialogs.rs}`

**Estimated scope:** L (a move)

### Task 12: the room a screen needs

**Description:**
- **Render's sentence** of how much room a screen needs: the least columns and rows, and what the terminal has. It sits in `screen.rs` beside the screens' other words.
- **`yunta_ui::room_needed(least, has) -> Doc`**, in `crates/ui/src/room.rs`: the document a surface shows where it has less room than it lays a screen out in. Every word in it is render's, so a surface reaches it with no render of its own.
- `least` and `has` are each a terminal's columns and rows. The sizes are the surface's: `yunta-tui`'s least is its layout's.

**Found in its turn:** the sentence says how to leave, since nothing else stands on that screen: "the application needs a terminal of 60 by 14 at least, and this one is 59 by 14: make it larger, or press q to leave". `yunta-ui` hands it as prose, which a surface lays out as a block.

**Fails first:** render's test of the sentence (`60×14` asked, `59×14` had), and `crates/ui/tests/room.rs`: the document says both sizes.

**Acceptance criteria:**
- [x] The sentence is render's; `yunta-ui` writes none of it.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render screen`, `cargo test -p yunta-ui --test room`

**Dependencies:** Task 11

**Files likely touched:** `crates/render/src/screen.rs`, `crates/ui/src/{lib.rs,room.rs}`, `crates/ui/tests/room.rs`

**Estimated scope:** S

### Task 13: the target screens

**Description:**
- **`yunta_testkit_ui::screens`** builds the states the target frames show. Each is reached through `State::update` from samples (`yunta-core`'s and `yunta-replay`'s `testkit`) at the sandbox's instant, with fixed ids:
  - `field`: the field with runs at several standings;
  - `run`: a run moving, its nodes beside its page, and another run that asks at its terminal in the attention row;
  - `card`: a decision's card;
  - `desk`: the plan at its desk;
  - `confirmation`: a run's stop asked over the field;
  - `palette` and `help`, each over the run.
- Each is the same state on every machine, so the frames drawn of it are too.

**Found in its turn:**
- **The states are the application's own, driven against runs a bench makes,** rather than values built by hand: a repository with a run parked on a gate that shows its plan (`Q78GRR`) and one finished (`EKPBGE`), each with its fixed id, every piece of work answered by the performer, and a person's keys pressed. The screens stand 7m34s after the bench made the runs (`SECONDS_AFTER_THE_RUNS`), as the reference's run has waited.
- **What the stages draw differs from the reference's sketch, and the targets follow the stages:** the field is the only stage with an attention row; no stage titles a region; the run's chronicle is its main region, its ribbon across the screen, and a node's page beside it once picked.
- **Two builds of each are compared, and the test found a difference:** a node's page names the file it produced under the bench's temporary directory, so the run's screen shows the gate's page, which names none.
- The confirmation is stopping the run (`c` on the field), the palette and help stand over the run.

**Fails first:** `crates/testkit-ui/tests/screens.rs`: each state's screen in Markdown holds what it is for (the field's runs, the run's nodes, the card's question, the desk's tasks, the command the confirmation names, the palette's rows, help's keys), and two builds of each are equal.

**Acceptance criteria:**
- [x] No state reads the clock, the machine or a random id.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-testkit-ui --test screens`

**Dependencies:** Task 12

**Files likely touched:** `crates/testkit-ui/src/{lib.rs,screens.rs}`, `crates/testkit-ui/tests/screens.rs`

**Estimated scope:** M

### Group check: the crates
- [x] The suite's groups green; the walk's counts equal the base's; `cargo deny check` and `cargo xtask docs --check` pass.

### Task 14: the parts, as Surco draws them

**Description:**
- **A probe outside the repository**, `proof/tui/probe/`, is a package of its own: a `[workspace]` of its own, path dependencies on `yunta-ui`, `yunta-surco` and `yunta-testkit-ui`, a copy of the workspace's `Cargo.lock`, built into the worktree's target.
- **Its binary `parts`** reads the areas the target script asks for, as JSON: a screen, a look, and each part's columns and rows. It writes each part's lines as Surco draws them:
  - the trail with the tally, the rule, and what the screen is about at the inner width;
  - each region's title and its pane;
  - the attention and legend rows;
  - the overlay boxed;
  - the room needed.

**Found in its turn:** the probe answers two asks, what a screen holds that takes room and each part's lines at the sizes the script worked out; it builds in under a minute beside the workspace's target, and the screens it reads take about two seconds each. The legend row it drew found that the stages' legends are far longer than the reference's: the run's names thirteen keys, so one row kept two of them and lost `/` and `?`, the keys that lead to the rest. Surco's legend row now keeps the first keys that fit, a mark that more stand between, and the last key, `? the keys here` (Task 6).

**Fails first:** none: a tool.

**Acceptance criteria:**
- [x] The probe builds against the worktree and writes the parts of every screen of Task 13.

**Verification:**
- [x] Manual check: the parts of the run at 80 cells read as the reference's rows do.

**Dependencies:** Task 13

**Files likely touched:** `.plans/yunta-tui/proof/tui/probe/` (outside git)

**Estimated scope:** S

### Task 15: the target frames

**Description:**
- **`proof/tui/targets.py`** states the reference's rules in Python, apart from the Rust:
  - **rows:** the trail with the tally, the rule, what the screen is about, a blank row, the regions under their titles, the attention, the legend;
  - **short of rows:** the regions give way first, then what the screen is about, and the attention last;
  - **columns:** a margin of 1; the side region of 22 cells and an edge of 3, giving way when the main region would get fewer than 40;
  - **the box:** at most 60 cells and 2 inside each edge, centred, its top a third of the way down, and its legend in the legend row;
  - **under 60×14:** the room needed, centred, and nothing else.
- It works out each screen's areas, asks the probe for the parts, places them, and writes the frames into `crates/tui/goldens/screens/<screen>.<environment>.txt`:
  - the seven screens of Task 13 at 80×24 in ASCII (`ascii-80`) and at 60×14 in Unicode (`unicode-60`);
  - the room-needed screen at 59×14 (`unicode-59`).
- Each frame is every row of its terminal, one a line, its trailing blanks cut, as `yunta-tui`'s screens test writes a `TestBackend`'s buffer.
- **The script checks each frame:** its rows and cells, the legend on the last row, the attention above it, nothing past a margin or the side region's edge.

**Found in its turn:**
- **A box taller than the reference's covered the trail:** help over the run takes every row. The box stands between the rule and the legend row, at most the rows they leave, its top a third of the way down them and never above the row under the rule; the reference's box stands where it stood.
- **The run's stage sets a node's page beside its chronicle**, where the reference had the nodes beside the page: at 80 columns the page is 22 cells wide. It waits for the user at Checkpoint A.
- The room needed is centred as a block. `checks_fail.py` breaks a frame each way the script checks, and each check fails.

**Fails first:** none: targets. Each of the script's checks is seen to fail on a frame made to break it.

**Acceptance criteria:**
- [x] Fifteen frames; the run's at 80×24 and 60×14 follow the reference's frames row by row, but for what their samples say.

**Verification:**
- [x] Manual check: `python3 targets.py` writes and checks every frame; the user reads them at Checkpoint A.

**Dependencies:** Task 14

**Files likely touched:** `crates/tui/goldens/screens/*.txt`, `.plans/yunta-tui/proof/tui/targets.py` (outside git)

**Estimated scope:** M

### Task 15a: a list keeps only the rows it takes

Decided by the user at Checkpoint A.

**Description:**
- A choice list counts its filter's row once, in the rows its options are fitted to, and leaves only its keys' rows besides.
- The row for the question a caller says above a list is the inline hold's: `Terminal::held` keeps it, beside the terminal's last row.

**Found in its turn:** in a pane a list now fills its rows; at a terminal the CLI's pickers show one option more where the list is cut short. The captures, whose lists are never cut, print the same.

**Fails first:** `crates/ui/src/prompt/picker/tests.rs`: a terminal of five rows holds two of a list's rows, where it took six.

**Acceptance criteria:**
- [x] A list in a pane takes every row it is given; every capture prints as the base.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco`, `cargo test -p yunta-ui --lib`

**Dependencies:** Task 15

**Files likely touched:** `crates/surco/src/surface/terminal/{mod.rs,asked.rs}`, `crates/ui/src/prompt/picker/tests.rs`

**Estimated scope:** XS

### Task 15b: a run's nodes beside its page

Decided by the user at Checkpoint A: the run's stage follows the reference, in every surface.

**Description:**
- **Beside the page:** the run's nodes, one a row in the workflow's order, the pointer on the one a key acts on, each with its standing's mark and how long it worked (`yunta_render::watched::nodes`). The keys act on that list, so it is the region focused.
- **The main region:** the page a person reads, the picked node's or the run's with its chronicle under it, scrolled by the page.
- **Across the screen:** the ribbon, then the rows a person watches the run by: what needs them, what each working node is doing, and how far the run has come.
- `scrolling_beside` is `scrolling_by_the_page`: the page scrolls by PageUp and PageDown while the arrows move along the nodes.
- The target frames of the run, the palette and help are written again.

**Found in its turn:** the pointer, which only the legend named, shows on the list. The walk reaches 21,703 states (17,304 before): the run's page, in the main region, now scrolls as a page, which the run's chronicle at its end did not; a page scrolled is named by the run's handle (`000000` in the walk's home), and no region beside the main one scrolls.

**Fails first:** `crates/render/src/watched/tests.rs`: three nodes at three standings, the pointer on the second, drawn as a list in Unicode and ASCII.

**Acceptance criteria:**
- [x] The run's stage shows its nodes beside its page in every surface; the run's interface tests, the scroll tests and the walk say so.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render watched`, `cargo test -p yunta-ui`, `cargo test -p yunta --test interface`

**Dependencies:** Task 15a

**Files likely touched:** `crates/render/src/{watched/mod.rs,watched/tests.rs,screen.rs}`, `crates/ui/src/{stage.rs,stage/run.rs}`, `crates/ui/tests/{run_stage.rs,scroll.rs,walk.rs}`, `crates/cli/tests/interface/run.rs`, `crates/tui/goldens/screens/`

**Estimated scope:** L

### Group check: the targets
- [x] The fifteen frames are written and checked; the suite's groups green (the phase's gate).

### Checkpoint A: the design proven before the code
- [x] The user reviews the target frames and Surco's new lines: approved on 2026-11-11, with a run's nodes beside its page (Task 15b) and a list keeping only the rows it takes (Task 15a).
- [x] Every existing golden, capture and test is equal to the base; `cargo deny check` passes.
- [x] The phase commits, with `SPEC-tui.md`, `tasks/plan.md` and `tasks/todo.md` (`dc9a5ac7`); then `cargo clean`.

## Phase 2: the terminal application, and watching

Its groups, each green before the next: drawing; the terminal; the loop and bare `yunta`; the walk laid out; watching.

### Task 16: `layout`

**Description:**
- **`layout::areas(size, &Holds) -> Areas`**, in `crates/tui/src/layout.rs`, pure:
  - `size` is the terminal's columns and rows;
  - `Holds` is what the screen holds that takes room: the rows its `about` draws at the inner width, whether it has a side region, an attention, an overlay;
  - `Areas` is where each part stands: the trail, the rule, the about, each region and its title, the attention, the legend, the box; or the room needed alone, under the least terminal.
- **Its numbers are `const`s, each with its reason:** margin 1, side 22, edge 3, main at least 40, box at most 60 and 2 inside each edge, a third down, least terminal 60×14.
- Ratatui's `Layout` splits the rows and columns. A Ratatui item that shares a name with Surco's is named by its path.

**Fails first:** `layout/tests.rs`, one test a rule, each with the target script's numbers:
- the side region stands at 80 and gives way at 60;
- short of rows, the regions give way first, then the about, and the attention last;
- the box is 60 cells at most, 2 inside each edge, its top a third down;
- under 60×14 only the room needed stands.

**Found in its turn:** what the screen holds is `Holds`, the region beside the main one a `Beside` with its edge's cells, Surco's, and whether it has a title, whose row stands only where that region does. The attention row never gives way on a terminal the application lays out on: its fourteen rows always leave the regions their three after the trail, the rule, a title, the attention and the legend, so the rule that it gives way last holds with no branch. `box_columns` says the box's width before its rows are known, so what is open over the screen is measured at it.

**Acceptance criteria:**
- [x] The areas of every target screen are those `targets.py` worked out.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui layout`

**Dependencies:** Checkpoint A

**Files likely touched:** `crates/tui/src/{lib.rs,layout.rs,layout/tests.rs}`

**Estimated scope:** M

### Task 17: `cells`

**Description:**
- **`cells::write(&Line, area, &mut Buffer, layer)`**, in `crates/tui/src/cells.rs`: Surco's lines written into Ratatui's buffer.
  - Each span's style is `paint::style` of its tone, fill and line, turned into a Ratatui `Style`: `Color::Ansi` to Ratatui's named colors, `Color::Rgb` to `Color::Rgb`, bold and dim to modifiers.
  - The rest of a filled row is erased with `paint::erase`.
  - Under `Paint::Plain`, no cell gets a style.
  - A line stops at its area's edge.
- No color is written anywhere else in the crate.

**Fails first:** `cells/tests.rs`: over every tone, fill and layer, each cell's style is `paint::style` of its span; under `Paint::Plain`, every cell's style is the default; a filled row's rest is erased.

**Found in its turn:** a Ratatui cell keeps a color nobody set as `Reset`, so the tests compare a cell with what a cell keeps of the style Surco's table gives.

**Acceptance criteria:**
- [x] `paint::style` and `paint::erase` are the only reading of color in `yunta-tui`.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui cells`

**Dependencies:** Task 16

**Files likely touched:** `crates/tui/src/{lib.rs,cells.rs,cells/tests.rs}`

**Estimated scope:** S

### Task 18: `drawn`

**Description:**
- **`pub fn drawn(frame: &mut ratatui::Frame, screen: &Screen, look: &Look) -> ScreenLaid`**, in `crates/tui/src/drawn.rs`: a Yunta screen drawn into a Ratatui frame.
  - Its areas come from `layout`, and each part's lines from Surco at the area's exact width: `trail`, `rule`, `lines` of the about, `title`, `pane`, `attention`, `legend`, `boxed`.
  - It writes them through `cells`, and sets the cursor where a held prompt puts it, and nowhere else.
  - Under the least terminal it draws `yunta_ui::room_needed`.
- It hands back what was laid out of each region and of the box.
- The crate's map names it as what any Ratatui program draws a Yunta screen with.

**Found in its turn:**
- The fifteen frames were equal at the first drawing: the layout's numbers and the target script's are the same rules, and Surco draws every part either way.
- The cursor is the box's where a box is open, else a region's prompt's, else none: Ratatui hides it where no position is set.
- The smell counter `banned_vocabulary` retires `backend` as what an adapter is not; Ratatui calls what it draws a frame on a backend, its API's word (`ratatui::backend`, `Terminal::backend`), so `crates/tui` says it as `crates/storage` says SQLite's, a test holding each.

**Fails first:** `crates/tui/tests/screens.rs` draws each of `yunta_testkit_ui::screens` on a `TestBackend` of its frame's size and compares the buffer whole with its target in `crates/tui/goldens/screens/`. All fifteen fail before `drawn` draws. The comparison never writes the targets.

**Acceptance criteria:**
- [x] Every target frame equal.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --test screens`

**Dependencies:** Task 17

**Files likely touched:** `crates/tui/src/{lib.rs,drawn.rs}`, `crates/tui/tests/screens.rs`, `crates/tui/Cargo.toml` (its dev-dependencies)

**Estimated scope:** M

### Group check: drawing
- [x] The suite's groups green; every target frame equal. Only `yunta-tui` and `xtask` changed since the phase's gate, and both crates' suites pass.

### Task 19: `keys`

**Description:**
- **`crates/tui/src/keys.rs`** turns crossterm's events into what the application reads, by the table the CLI's `ask/keys.rs` keeps:
  - **a key press** is its `Stroke`: a character, Enter, Backspace, Delete, the arrows, Home and End, PageUp and PageDown, Tab and BackTab, Esc as `Decline`, Ctrl-C as `Interrupt`;
  - **a release** is nothing;
  - **a paste** is its characters, each line break a `PastedBreak`, while the state is typing, and nothing otherwise;
  - **Ctrl-Z** is the terminal's own key, never a stroke: the loop suspends on it.

**Fails first:** `keys/tests.rs`: each key of the CLI's table gives the stroke the CLI gives for it; a release gives nothing; a paste of two lines gives its characters and one break while typing, and nothing while not; Ctrl-Z gives no stroke.

**Found in its turn:** `keys` is two functions and a third for pastes, named for what each says: `stroke`, `suspends` and `pasted`; Ctrl-Z needs no type of its own. A key held repeats its stroke, as a terminal repeats its bytes to the CLI's reader. A key the table does not name is `Stroke::Ignored`, as at a prompt.

**Acceptance criteria:**
- [x] The two key readers give the same stroke for every key the table names.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui keys`

**Dependencies:** Group check: drawing

**Files likely touched:** `crates/tui/src/{lib.rs,keys.rs,keys/tests.rs}`

**Estimated scope:** S

### Task 20: the pty harness grows

**Description:**
- `yunta-testkit`'s `Terminal` gains what a full-screen program needs tested:
  - `resized(columns, rows)`: the pty's size set on its master, which signals the program, and the screen read at the new size;
  - `terminate()` and `hang_up()`, beside `interrupt()`;
  - `stopped()`: whether a signal stopped the program; `continued()`: `SIGCONT` sent to it.
- Its `Screen` gains the modes vt100 reads: bracketed paste, and the mouse not captured.
- `open_sized` serves any program at the pty, as it serves `yunta`: a test's own binary, run again.

**Fails first:** none: a harness, first used in Task 21.

**Found in its turn:** the size is set with rustix's `tcsetwinsize`, which takes no `unsafe` (the workspace forbids it); the run is no session's leader under the harness, so the harness sends the resize's signal itself, as it sends the interrupt. `stopped` and `continued` were written and taken out again: a run under the harness is in a process group no shell controls, which the kernel stops on no `SIGTSTP` (Task 21).

**Acceptance criteria:**
- [x] Every test that uses `Terminal` passes unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-testkit`, `cargo test -p yunta --test plan_desk` (the second at the group check)

**Dependencies:** Task 19

**Files likely touched:** `crates/testkit/src/{terminal.rs,lib.rs}`

**Estimated scope:** S

### Task 21: `full_screen`

**Description:**
- **`FullScreen`**, in `crates/tui/src/full_screen.rs`, is the terminal held: the alternate screen, raw mode, bracketed paste, a hidden cursor, and the wheel sending arrows on the other screen (Surco's `ALT_SCROLL_ON`), with the mouse not captured.
- **It is given back** (Surco's `ALT_SCROLL_OFF` among the rest), each in the order taken back:
  - on drop;
  - by a panic hook installed while it is held, which then calls the hook it found;
  - before a suspend: `suspend()` gives the terminal back, raises `SIGTSTP`, and holds it again when the process continues.
- A terminal that refused raw mode or the other screen is the error it gave; the application says it as `TerminalFailure::Unheld`.
- The release profile aborts on a panic, so no drop runs there; the hook gives the terminal back.

**Fails first:** `crates/tui/tests/full_screen.rs`. The test runs its own binary again in a pty, which holds the terminal and then:
- leaves;
- panics under a hook that aborts after this one, so no drop runs and the hook alone gives the terminal back;
- suspends, is seen stopped, and is continued.

Each leaves the main screen, the cursor shown, the line discipline, bracketed paste off, and the wheel as it was.

**Found in its turn:**
- The tests are the crate's own, in `src/full_screen/tests.rs`: `FullScreen` is the crate's, and the test binary runs itself again in a pty, `the_terminal_held_as_asked` holding as its environment asks and holding nothing run on its own.
- **A stop cannot be seen under a test.** A process in a group no shell controls is not stopped by `SIGTSTP`: the kernel discards it, and the application goes on at once. The test holds what it can read, the terminal given back between the two holdings; the stop itself is proven from a shell with job control (`bash -i` driven by the proof's Python, Task 27). The module's `//!` says so.
- The panic hook found is put back when the holding is dropped. Leaving `Show` out of giving back made all three fail, naming the cursor.

**Acceptance criteria:**
- [x] No way out of the three leaves a mode on.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui full_screen`

**Dependencies:** Task 20

**Files likely touched:** `crates/tui/src/{lib.rs,full_screen.rs}`, `crates/tui/tests/full_screen.rs`

**Estimated scope:** M

### Group check: the terminal
- [x] The suite's groups green; every pty test of the CLI passes unchanged: the harness and the application's crate changed, and their suites and the CLI's twelve pty targets pass.

### Task 22: how often a surface hands in the time

**Description:**
- **`State::TIME_EVERY`**, one second, with its reason: the screens say ages and elapsed times to the second.
- It is the cadence every surface ticks at. The CLI's live region keeps its own, which draws no `State`.

**Found in its turn:** the test stands with the run's own tests (`run_stage.rs`), where a moving run is opened: ten seconds in, then a cadence later, the run across the screen reads `11s`. With a cadence of nothing it read `10s` still.

**Fails first:** `crates/ui/tests/run_stage.rs`: a moving run's screen read a `TIME_EVERY` later says its elapsed time a second on.

**Acceptance criteria:**
- [x] One `const`, with its `///` reason.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test run_stage`

**Dependencies:** Group check: the terminal

**Files likely touched:** `crates/ui/src/state.rs`, `crates/ui/tests/state.rs`

**Estimated scope:** XS

### Task 23: a page not shown

**Description:**
- **`Input::Unshown { page, why }`**: a page the surface could not show, and the error it gave.
- **It reaches the evidence stage that asked for the page**, the only stage that shows pages. The stage says it in render's words (`screen.rs`), with where the page is, so the person can open it themselves.
- **The walk hands it** after each `ShowPage` a state asks for, to the state that asked. `MUST_REACH` gains "a page not shown".

**Found in its turn:**
- **The words are `yunta open`'s**, which render already says (`page::Opened { opened: Some(false) }`): "nothing here could open it: open that file in a browser." `yunta open` says no reason, so the input carries none: `Input::Unshown(PathBuf)`, and every interface says it alike.
- Enter asks for the page again, and the note goes until another comes.
- The walk hands every state each page the recordings wrote (`Recordings::pages_written`), so an input no stage but evidence answers is held not to panic anywhere: 21,814 states.

**Fails first:** `crates/ui/tests/evidence.rs`: a page not shown is said with its path; the walk, with the input added and the stage not yet answering it, never reaches "a page not shown".

**Acceptance criteria:**
- [x] The sentence is render's; the walk reaches it.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test evidence`, `cargo test -p yunta-ui --test walk`

**Dependencies:** Task 22

**Files likely touched:** `crates/ui/src/{input.rs,stage/evidence.rs}`, `crates/render/src/screen.rs`, `crates/ui/tests/{evidence.rs,walk.rs}`, `crates/testkit-ui/src/walk.rs`

**Estimated scope:** M

### Task 24: the shell's signals that end a process

**Description:**
- **`Shell::asked_to_end`** installs interrupt, terminate and hang-up listeners when the application asks for them. `shell.rs` stays the one place a signal is listened to, as the smell counter `process_read_below_the_shell` holds.
- **`Shell::asked_to_end()`** is a future that resolves with which of the three fired, as `EndSignal::{Interrupt, Terminate, HangUp}`.
- **`Outcome::ended_by(EndSignal)`** exits as a shell reports a process that signal ended: 130, 143, 129. `Outcome::interrupted` is the first of them.

**Found in its turn:** installed at `Shell::read`, a terminate or a hang-up would no longer end every other command as it ends any process; `Shell::asked_to_end` installs the three when the application asks for them, still in `shell.rs`. Its future resolves with the `EndSignal` that fired.

**Fails first:** `output/exit.rs`'s test: the three codes.

**Acceptance criteria:**
- [x] `cargo xtask smells --check` passes; every command exits as at the base.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --bin yunta exit`

**Dependencies:** Task 23

**Files likely touched:** `crates/cli/src/{shell.rs,output/exit.rs}`

**Estimated scope:** S

### Task 25: `application`

**Description:**
- **`Application::new(performer, look, clock, show_page)`** and **`run(opened, asked_to_end) -> Result<Exit<E>, TerminalFailure>`**, in `crates/tui/src/application.rs`:
  - it holds the terminal with `FullScreen`;
  - it waits, in one `select!`, on crossterm's `EventStream`, the performer's channel, a tick every `State::TIME_EVERY` handing in the clock's time, and `asked_to_end`;
  - after each wake it drains what is waiting, hands it all in, and draws once with `drawn`, in one synchronized update.
- **What it hands back and carries out:**
  - what was laid out goes back as `Input::Laid` only when it changed;
  - each request goes to `Performer::take`: `ShowPage` to `show_page`, a failure back as `Input::Unshown`; `Leave` ends the loop;
  - a resize draws at the new size, and Ctrl-Z suspends through `FullScreen`.
- **Idle:** while nothing on screen changes, nothing is written.
- **`Exit`:** `Left`; `Interrupted` when `Leave` follows a Ctrl-C stroke; `AskedToEnd` when `asked_to_end` resolved, with what it resolved to (`Exit<E>`), so the binary reads which signal fired.

**Found in its turn:**
- **Ratatui writes its pen's reset and the cursor's hiding with every drawing**, a cell changed or not. A drawing that changes nothing writes no cell, which a test holds apart from a terminal; a screen at rest still writes those few bytes at each tick of the time.
- Each drawing is held in `Synchronized`, which the backend writes into and the loop writes out between Surco's `SYNC_BEGIN` and `SYNC_END`: two handles on one hold, since the backend's own writer is behind an unstable feature.
- The first drawing stands before the application knows where it is, so the signpost is in a later update.
- How the terminal failed the application is `TerminalFailure`, said apart from `Exit`: `Unheld` before it was held, `Lost` once it was (Task 27's turn).
- The tests are the crate's own (`src/application/tests.rs`), the binary run again by its test's exact name. Leaving made always `Left` failed the Ctrl-C test.

**Fails first:** `crates/tui/src/application/tests.rs`. The test runs its own binary again in a pty, where `Application` runs over a sandbox workspace (`yunta_testkit_ui::sandbox`) at a fixed clock, and holds:
- it opens on the field, drawn whole in one synchronized update;
- `q` returns `Left` with the terminal given back, and Ctrl-C returns `Interrupted`;
- `asked_to_end` resolving returns `AskedToEnd`;
- a resize draws at the new size;
- a drawing that changes nothing writes no cell.

**Acceptance criteria:**
- [x] The crate reads no system clock and no environment; `cargo xtask smells --check` passes.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui application`

**Dependencies:** Task 24

**Files likely touched:** `crates/tui/src/{lib.rs,application.rs}`, `crates/tui/tests/application.rs`, `crates/tui/Cargo.toml` (its dev-dependencies)

**Estimated scope:** M

### Task 26: the system's opener, in a module of its own

**Description:**
- **`browser()`** moves out of `commands/open.rs:47` into `crates/cli/src/browser.rs` as `browser::open(page) -> io::Result<()>`, its doc with it. `yunta open` calls it there.
- The application is handed the same function in Task 27, so a page has one way to a browser.

**Fails first:** none: a move. Its guard is `open_cmd.rs` passing unchanged.

**Acceptance criteria:**
- [ ] `commands/open.rs` spawns nothing itself.

**Verification:**
- [ ] Tests pass: `cargo test -p yunta --test open_cmd`

**Dependencies:** Task 25

**Files likely touched:** `crates/cli/src/{main.rs,browser.rs,commands/open.rs}`

**Estimated scope:** XS

### Task 27: bare `yunta` opens the application

**Description:**
- **`crates/cli/src/application.rs`** decides and opens:
  - `opens_here()`: stdin and stdout at a terminal, and `TERM` not `dumb`, from the facts the CLI already settles;
  - `open(shell, look, opened)`: the `Performer` over `shell.workspace()`, the stdout look, the workspace's clock, `browser::open`, and `shell.asked_to_end()`;
  - how it ended, to an outcome: `Exit::Left` is success, `Exit::Interrupted` is `Outcome::interrupted()`, `Exit::AskedToEnd` is `Outcome::ended_by` the signal that fired;
  - `TerminalFailure::Unheld` prints as off a terminal; `TerminalFailure::Lost` is the command's failure.
- **`cli/dispatch.rs:36`** opens the application where it may, and otherwise prints `list::field` as at the base.
- `yunta`'s manifest gains `yunta-tui`.

**Fails first:** `crates/cli/tests/application.rs` and `application/opening.rs`, bare `yunta` at a pty:
- it opens on the field on the other screen; `q` exits 0 with the main screen, the cursor, the line discipline and bracketed paste back;
- Ctrl-C exits 130; `SIGTERM` exits 143 with the terminal given back;
- a resize lays the screen out at the new size, and under 60×14 it says the room it needs;
- Ctrl-Z gives the terminal back and holds it again, drawn whole (the stop itself under `bash -i` in the proof, since the harness's group is no shell's);
- outside a repository it opens on the signpost;
- with `TERM=dumb`, and through a pipe, it prints what the base prints.

**Found in its turn:**
- **Ratatui's `Terminal::clear` asks the terminal where its cursor stands** (`ESC[6n`) and waits two seconds on the answer. The harness's terminal answers nothing, so drawing the screen whole after Ctrl-Z failed the loop, and bare `yunta` then printed the field. The test's first form took that print for a drawing, since the harness's other screen keeps what it last held. The loop now draws afresh by laying out again at the terminal's size (`Terminal::resize`), which asks the terminal nothing. Both Ctrl-Z tests read what was drawn each time the other screen was held (`Terminal::drawn_on_the_other_screen`), and each fails against `clear` put back.
- A terminal that fails after it was held is no longer printed over: `TerminalFailure` splits `Unheld`, which prints as off a terminal, from `Lost`, which is the command's failure.
- The stop itself, under `bash -i` (proof `suspend.py`, `suspend.txt`): Ctrl-Z gives the terminal back before bash says `Stopped` and gives back its prompt; `jobs -l` lists `Suspended: 18`; `fg` holds the terminal and draws the field whole again without asking the terminal anything; `q` leaves with 0, with nothing printed after.

**Acceptance criteria:**
- [x] The captures equal the base's but bare `yunta` at a pty.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test application`

**Dependencies:** Task 26

**Files likely touched:** `crates/cli/src/{main.rs,application.rs,cli/dispatch.rs}`, `crates/cli/Cargo.toml`, `crates/cli/tests/{application.rs,application/opening.rs}`

**Estimated scope:** M

### Group check: the loop and bare `yunta`
- [x] The suite's groups green; every capture equal to the base's but bare `yunta` at a pty, shown beside the base's. (`proof/tui/group-loop.txt`: 158 views, only `screen-bare` and `screen-bare-outside` differ, now the application; 247 goldens, 0 changed, 33 new; 4,830 tests against the base's 4,764, none gone, none aborted; release 30,583,392 bytes.)

### Task 28: the walk laid out

**Description:**
- **`crates/tui/tests/walk.rs`** walks with `yunta_testkit_ui::Walk` and draws every state it reaches with `drawn`, on a `TestBackend` at 80×24 in ASCII and at 60×14 in Unicode. Its checks:
  - no panic;
  - the margins and the side region's edge hold only what `layout` puts there, so no line crosses its pane's edge;
  - the legend on the last row, the attention above it;
  - the cursor shown only at a typed prompt;
  - an overlay's legend in the legend row.
- Its time is measured here and on CI. If it doubles the walk's, it draws each distinct screen once, as the walk already keeps each state once.

**Fails first:** the walk, with a pane made one cell wider than its area for the run: a line crosses the edge, named with the inputs that reach it.

**Found in its turn:**
- **Drawing is the walk's cost:** 2.5 ms a frame in a debug build, against 17 µs for the screen and 30 µs for the checks. Drawing all 43,628 frames took 162 s, three times the interface walk's 53 s, so each distinct screen is drawn once on each terminal: 4,948 of the 21,814 states, in 77 s. A screen is told apart by its Markdown, where each region and the box are scrolled to, and whether a field is open; `Screen` has no `Hash` and no `Debug` to tell it by.
- Each check fails on a mutation of its own: the main pane one cell wider writes in a margin on the first field screen; the edge drawn one row short breaks it on row 23; the cursor never placed leaves an open field without one. The side pane one cell wider stays green, because no side line in the walk's home reaches its last cell.
- The rules are stated in the test apart from `layout`: what the screen is about gives way from its last row while the regions would get fewer than three, then the attention.

**Acceptance criteria:**
- [x] Every state the walk reaches lays out at both sizes; the time is recorded in `proof/tui/` (`walk-laid-out.txt`).

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --test walk`

**Dependencies:** Group check: the loop and bare `yunta`

**Files likely touched:** `crates/tui/tests/walk.rs`

**Estimated scope:** S

### Group check: the walk laid out
- [x] The suite's groups green; the walk's time beside the base's. (No code outside the new test changed since the group check before, whose groups are green; `yunta-tui`'s suite green with the walk, 81 s, beside the interface walk's 58 s and the base's 40 s; `smells --check` and `docs --check` hold.)

### Task 29: watching, at a pty against the mock

**Description:**
- **`crates/cli/tests/application/watching.rs`**, bare `yunta` at a pty:
  - a run started detached on the mock adapter, opened from the field, followed to its end;
  - a run promoted, followed onto its successor;
  - leaving with `q` while a run moves leaves it moving, as `yunta status` reads.

**Fails first:** each test seen red with the performer's news dropped for the run: the run's screen never moves.

**Found in its turn:**
- The run holds on a bash node, as the interface tests' run does, with no adapter: what the application follows is the run's log, which a bash node writes as a session does.
- The three tests wait on the legend's "move along its nodes", which only the following brings, since a run not yet read has no nodes to point at. With the performer's news dropped, each fails there.
- The chronicle's last lines stand below the main region while a run is followed (`… 2 more lines`): the page keeps its top. The run's end is read from the page's head.
- What the tests share with the interface's and the follower's moved into the testkit, each said once: `HeldRun` (started off a terminal or detached), `workflows::PROMOTABLE` (three copies), `run_ids` (which `full_run_id` reads through), `Terminal::choose` (two copies) and `Terminal::process_id`. The field, the opening and the giving back moved to the application tests' root. The one-node `holding` of `close_cmd.rs` and `stalled_runs.rs` is another workflow, and stays.
- The testkit's terminal passed 500 lines: the screen and how a drawing is read moved to `terminal/screen.rs`, and the escape smell names the module whole, as it names paint.

**Acceptance criteria:**
- [x] Following in the application never hosts the run: the run's process is the detached one. (The engine the run's registry names is not the application's process.)

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test application watching`

**Dependencies:** Group check: the walk laid out

**Files likely touched:** `crates/cli/tests/{application.rs,application/watching.rs}`

**Estimated scope:** S

### Task 30: a real run followed in the application

**Description:**
- `follow_in_app.py` drives bare `yunta` at a pseudo-terminal, from the release build of this phase's tree.
- fragua is started detached as in Task 3, opened from the field, and followed in the application until it waits before `pr`. Its gates are answered with `yunta resolve-gate` from another process.
- Its time between nodes and its tokens are read from `yunta stats` and set beside Task 3's.

**Fails first:** none: a measurement.

**Found in its turn:**
- **What a drawing writes is no text to search:** it writes only the cells that changed, moving the cursor over the rest. The driver's first two runs waited for the legend in the bytes, never saw it, and left their runs going; each was cancelled and closed by hand within a few minutes. The driver now reads the screen as a terminal holds it (`pyte`), and cancels and closes its run wherever it ends early.
- **A node named past the side region folded onto a second row** (`review@reviewer-alt` at 22 cells), so the pointer's list stood a row longer than its nodes. The walk never met it, since its home's node ids are short. Surco gains `Block::Rows`, rows a pointer moves along, each cut at the edge it stands at with the ellipsis, never folded. The run's nodes are drawn as rows, its sample stands in the catalog, and `run_stage.rs` holds a long name to one row, red first.

**Acceptance criteria:**
- [x] `between_secs` inside the base's spread; the application followed the run to where it waits. (Run 01M76QB3ACHXCKQEE206QBRC8E: `between_secs` 0.559479 against the base's 0.559435, earlier runs 0.53 to 0.59; at `ship` after 132 s against 161 s; tokens 1,089,373, 913,047 cached; the screen at `ship` shows `◆ ship` and its decision, `real-follow/app-at-ship.txt`; 234 drawings and 3.4 s of processor time in the application; it left with 0.)

**Verification:**
- [x] Manual check: the run's numbers beside the base's in `proof/tui/checkpoint-b.txt`.

**Dependencies:** Task 29

**Files likely touched:** `.plans/yunta-tui/proof/tui/` (outside git)

**Estimated scope:** S

### Task 31: what the phase weighs

**Description:**
- The probe gains a binary `frame`, which draws the run's target screen at 200×60 with `drawn`, 2,000 times in a release build, and prints the median.
- `first_screen.py`'s median for bare `yunta` opening on the field, against the base's.
- The release binary's size by crate, against the base's.
- All of it in `proof/tui/checkpoint-b.txt`.

**Fails first:** none: measurements.

**Found in its turn:**
- **The first screen is measured from a rest.** A process started on a busy machine runs its first steps several times faster than one started after a pause. An application start leaves a pause behind it, so back to back the application read 60 ms against the base's 26. A traced build placed it before `main`, whatever the command. Each start now waits the same rest, and the screen is read as a terminal reads it, after the fact: the base 70.7 ms, this 73.7 ms.
- **The binary grew 2.1 MB, not by the application's own crates** (about 90 KB). The rest is `yunta-ui`'s performer, which bare `yunta` links now, and the engine's and the API's paths and readers it drives, instantiated again.

**Acceptance criteria:**
- [x] A frame under 2 ms; the first screen within 20 ms of the base's; the binary under 40 MiB. (0.45 ms whole at 200x60; +3.0 ms; 30,583,392 bytes.)

**Verification:**
- [x] Manual check: `checkpoint-b.txt` read whole.

**Dependencies:** Task 30

**Files likely touched:** `.plans/yunta-tui/proof/tui/` (outside git)

**Estimated scope:** S

### Group check: watching
- [x] The suite's groups green; the real run and the measurements recorded. (`group-watching.txt`, `checkpoint-b.txt`.)

### Checkpoint B: the terminal application, and watching
- [x] Every target frame equal, the walk laid out, the pty tests green.
- [x] A real run followed in the application at the base's time between nodes; a frame's time, the first screen's and the release size, against the spec's bounds.
- [x] The user reviews; the phase commits; then `cargo clean`.

Decided at the checkpoint:
- A followed run's page keeps its head in sight; the chronicle's newest lines stand below it until the person scrolls.
- The binary's growth stands; the performer's paths it carries are open work in `.agents/open.md`.
- Opening looks the repository up twice, two git processes where the printed field runs one (said as four at the checkpoint): its fix is Task 35a, in this list's next phase.

## Phase 3: deciding, `review`, starting, every stage, and the proof

Its groups, each green before the next: opening on a step; deciding and `review`; starting and every stage; the docs; the proof.

### Task 32: one mapping from a step to the stage that takes it

**Description:**
- **`Stage::opened_by`** grows to every step that opens a stage:
  - the palette's four: `List`, `Doctor`, `Gc { dry_run: true }`, `Schemas`;
  - a run's: `Status`, `ResolveGate`, `Review`, `Receipt`, `Verify`, `Open`, `Stats`, `RunGraph`.
- **These route through it:**
  - the run's acts (`run/acts.rs::taken`);
  - the field's `w` and `m` (`field.rs:189-190`);
  - the catalog's `s` (`catalog.rs:244`);
  - doctor's `u` (`doctor.rs:157`).
- Acts that are work, or that ask a confirmation, stay with their stage.

**Fails first:** `stage/tests.rs`: `opened_by` of a run's `Review` is the run's decision on its plan, and of `Stats` the run's stats; both are none at the base.

**Found in its turn:**
- Three more steps open a stage: `Follow`, which the field offers on a run and which the start's run is followed by; a workflow's `History`; and its `Graph`, when it names one. They join the mapping, and the field's `enter`, the start's run and the catalog's `g` and `h` route through it with `Want::opening`.
- The start with a workflow picked stays built by the catalog and the first run: it needs the catalog already read, which no step carries.

**Acceptance criteria:**
- [x] No stage builds a stage a step opens but through `opened_by`; every test passes unchanged, and the walk's counts equal Phase 2's (21,814 states, 30 palettes, 70 helps).

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui`

**Dependencies:** Checkpoint B

**Files likely touched:** `crates/ui/src/{stage.rs,stage/run/acts.rs,stage/field.rs,stage/catalog.rs,stage/doctor.rs,stage/tests.rs}`

**Estimated scope:** M

### Task 33: the application opened on a step

**Description:**
- **`State::opened_on(now, step) -> Option<(State, Vec<Request>)>`:**
  - it reads the repository as `opened` does, and stands where `opened` stands;
  - over that, the run's stage, for a step of a run, and over that, the stage `opened_by` gives;
  - it is none for a step no stage takes.
- **Going back** walks the stages it opened over, as if the person had walked there: `esc` from `review`'s decision reaches the run, then the field.
- The test helper `opened_on` in `crates/ui/tests/run_stage.rs` is renamed for what it is, so the name reads as the state's alone.

**Fails first:** `crates/ui/tests/state.rs`:
- opened on `Review`, once its readings are answered, the screen is the plan's desk;
- `esc` reaches the run, and `esc` again the field;
- opened on `Cancel`, it is none.

**Found in its turn:**
- The desk opened on `review` equals the desk a person reaches from the run with `p`, the run's own `review`, not through the card: there `esc` goes back to the card, here it leaves the run waiting. The test compares with that walk, which `yunta_testkit_ui::screens::walked_to` gives, the one way every screen of the screens' world is reached now.
- Which run a step names is core's: `NextStep::run`, said for every step, so a new one has to say it.
- The application stacks the stages a step opens over wherever it first stands: the field, the first run, the signpost or the setup.

**Acceptance criteria:**
- [x] Opening on a step asks nothing `opened` and the stages' openings do not ask: each stage opens over the last as a person's step opens it.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test state --test run_stage`

**Dependencies:** Task 32

**Files likely touched:** `crates/ui/src/{state.rs,stage.rs}`, `crates/ui/tests/{state.rs,run_stage.rs}`

**Estimated scope:** M

### Task 34: the walk opens on a step

**Description:**
- **`Walk` starts from each opening it is given.** `crates/ui/tests/walk.rs` walks from `State::opened` and from `State::opened_on` the home's run parked on a plan, `Review`.
- **`MUST_REACH`** gains the plan opened on `review`, and the run reached back from it.

**Fails first:** the walk, with the required lists grown before it opens on the step: it never reaches them.

**Found in its turn:**
- **The home had no run parked on a plan.** It gains one, `reviewing` (`…REV1EW`), standing apart as `deciding` and `asking` do, since every run the field lists multiplies the walk. The planned workflow, its planner and its config move from the screens' world to `planned.rs`, which both make their run with. Its decision is recorded, and the steps the application is opened on are the recordings' own (`Recordings::opened_on`), so every walk opens on the same ones.
- **A state opened on a step looks like the plain opening until it first stands somewhere.** It shows the same screen and waits on the same work, so the walk kept only one of them. The state says how many stages it has yet to open (`State::stages_to_open`), and the walk counts it.
- **Walked side by side, the openings lost states.** A state is kept by what shows, so the opening on `review` reached first some screens the plain opening reaches with other stages under them. About 300 screens the plain walk reaches, the start, the packs, upkeep and a workflow's history among them, went unwalked: 20,923 states. Each opening is now walked whole before the next.
- Two required screens: the plan opened on `review`, whose `esc` leaves the run waiting, and that run's own screen reached back from it.

**Acceptance criteria:**
- [x] The walk's counts are recorded beside Phase 2's: 21,814 states at Phase 2; 22,199 with the plan's decision recorded; 22,388 opened on `review` as well. The terminal's walk draws 5,145 screens on each terminal, in 79 s.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test walk`, `cargo test -p yunta-tui --test walk`

**Dependencies:** Task 33

**Files likely touched:** `crates/testkit-ui/src/walk.rs`, `crates/ui/tests/walk.rs`

**Estimated scope:** S

### Task 35: `review`'s refusal, in render's words

**Description:**
- The refusal `review.rs:31` writes with `format!` (a decision that shows no plan, and the command that says what it does show) moves to render, where every refusal is worded. It prints the same.

**Fails first:** none: a move. Its guard is `plan_desk.rs`'s `review_refuses_a_run_that_waits_on_no_plan_and_says_what_serves` passing unchanged, and the captures.

**Found in its turn:** the refusal is `Refused::NoPlan { run }`, worded beside `NoDecision`. A person's page and an agent's document are the ones a plain message drew: the sentence, no advice, no blocks, the same exit code.

**Acceptance criteria:**
- [x] `review.rs` writes no sentence.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test plan_desk`, `cargo test -p yunta-render`

**Dependencies:** Task 34

**Files likely touched:** `crates/cli/src/commands/review.rs`, `crates/render/src/refusal/` (the module refusals are worded in)

**Estimated scope:** XS

### Task 35a: the repository asked once as the application opens

**Description:**
- **What happens now:** opening asks `Operation::ReadRepository`, whose `Project::repository` runs `Here::of`: where the repository's git directory is, which `.git` says without git, then the runs' branches, `git for-each-ref`. Once the repository is set up it asks `Operation::Field`, whose `Project::field` runs `Here::of` again. Two git processes before the field shows, where the printed field runs one; a git costs about 8 ms here.
- **What should happen:** the application's opening runs git as the printed field does, once.
- **Which assumption might be wrong:** that `repository` needs the run branches at all. It needs only to know whether there is a repository and whether Yunta's layer is in it.
- The owner is the API. No git result is cached across readings, since a person may branch between two.

**Fails first:** `crates/ui/tests/state.rs`, on a workspace whose `PATH` has the testkit's `git` stub, which logs each run and hands it to the real git: from `State::opened` to the field on screen, git ran twice; it must run once.

**Found in its turn:** Checkpoint B said four git processes where two would do. The git directory is read off `.git` without a process, so it was two where one would do. `Project::repository` now asks only for the git directory (`git_common_dir`, which `Here::of` reads too), and the runs' branches are the field's alone.

**Acceptance criteria:**
- [x] Opening on the field runs git once; the first screen's time, from a rest, is no slower than Checkpoint B's 73.7 ms: 65.7 and 65.1 ms against `yunta-watching`'s 73.8 and 73.1, alternating (`proof/tui/first-screen-step-1.txt`), and under the base's 70.7.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api -p yunta-ui`; `first_screen_shown.py` against `yunta-watching`.

**Dependencies:** Task 35

**Files likely touched:** `crates/api/src/field.rs`, `crates/ui/tests/state.rs`

**Estimated scope:** S

### Group check: opening on a step
- [x] The suite's groups green; the captures equal Phase 2's. (`group-step-1.txt`: 158 views, only bare `yunta` at a pty differs from the base, as at Phase 2; 247 goldens, the same 7 changed; 4,839 tests, none gone but the one the testkit's split moved.)

### Task 36: deciding in the application, at a pty against the mock

**Description:**
- **`crates/cli/tests/application/deciding.rs`**, bare `yunta` at a pty:
  - a run parked on a gate, opened from the field, answered on its card, the choice on the run's log;
  - a round of questions answered, the answers on the log;
  - the card's `r` opening the plan's desk, and an option chosen there.

**Fails first:** each test seen red with the decision stage's answer dropped for the run: nothing reaches the log.

**Found in its turn:**
- The gate, the round, the plan with its planner and their configs now live once, in `yunta_testkit::workflows`: the interface's, the desk's and the walk's home's tests read them from there, each as it was. The walk's planned run stays in `testkit-ui`: its plan's words are the target frames'.
- At 80 columns the desk has no rail of its tasks, as the CLI's desk on a narrow terminal: the test reads its keys.
- A run's `events.jsonl` reaches its end after the store does: the tests read the log once it says `run_finished`, as the interface's do.

**Acceptance criteria:**
- [x] Each answer reads back with `yunta status` as `resolve-gate` leaves it: on the log, and the run finished.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test application deciding`

**Dependencies:** Group check: opening on a step

**Files likely touched:** `crates/cli/tests/{application.rs,application/deciding.rs}`

**Estimated scope:** S

### Task 37: `yunta review` opens the application

**Description:**
- **`commands/review.rs`:**
  - its checks stay first and unchanged: the run found, the decision it waits on read, a plan among what it shows; a run that fails one is refused in the same words;
  - where `application::opens_here()`, the application opens on `State::opened_on(now, NextStep::Review { run })`;
  - elsewhere it prints the plan's page and the commands that answer it, as at the base;
  - its own terminal path, through `Console::open` and `chosen_on`, goes.
- **`review`'s pty tests move** from `plan_desk.rs` to `application/review.rs`, saying the chosen behavior:
  - it opens on the plan, over the run and the field;
  - an option chosen there is on the run's log;
  - `esc` walks back to the run, then the field, and `q` leaves with 0;
  - Ctrl-C exits 130 with the run still waiting.

**Fails first:** the new tests against the base's `review`: `esc` at the desk leaves the command instead of reaching the run.

**Found in its turn:**
- `review` takes no output mode: it has no `--json` or `--quiet`, and nothing it prints went through one once `settled` left it. Where no stage took its step, it would print as off a terminal, so it holds no `expect`.
- Its help says it opens the application on the plan, over the run and the field.
- A run parked on its plan reads `needs you` in `yunta status`.

**Acceptance criteria:**
- [x] Off a terminal, `review` prints what the base prints; an unknown run is refused as the base refuses it. (The group check's captures: only `review` at a pty differs among `review`'s; it finds its run before it opens anything, as at the base.)

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test application review`, `cargo test -p yunta --test plan_desk`

**Dependencies:** Task 36

**Files likely touched:** `crates/cli/src/{commands/review.rs,application.rs}`, `crates/cli/tests/{plan_desk.rs,application.rs,application/review.rs}`

**Estimated scope:** M

### Task 38: the CLI opens every decision on its card

**Description:**
- **`chosen_on`** (`resolve_gate.rs:293`) and **`ask::decide`** (`ask/decision.rs:31`) lose `opens`: with `review` in the application, `resolve-gate` and an attached run's gate open on the card, and `r` still opens the desk.
- `human_interaction.rs:133` and `:147` call `decide` without it.
- `Opens` stays `yunta-ui`'s, where the decision stage opens on either.

**Fails first:** none: a parameter every caller passes the same. Its guard is `plan_desk.rs`'s card tests and `console_interaction`'s passing unchanged.

**Found in its turn:** with `review` gone from them, `chosen_on` and `settled` are `resolve-gate`'s alone and private to it. `ask::decide` names `Opens::TheCard` once, where the CLI's card opens.

**Acceptance criteria:**
- [x] No CLI caller names `Opens`.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test plan_desk --test console_interaction`

**Dependencies:** Task 37

**Files likely touched:** `crates/cli/src/{commands/resolve_gate.rs,ask/decision.rs,human_interaction.rs}`

**Estimated scope:** S

### Group check: deciding and `review`
- [x] The suite's groups green; every capture equal to the base's but bare `yunta` and `review` at a pty, each shown beside the base's. (`group-decide.txt`, captures in `decide/`: 158 views, 4 differ, bare `yunta` and `review` at a pty, and `review` decides there and leaves with 0. The two `review` scenarios wait on one word, since a drawing skips the blank between two, and type `q` only where the process still reads; re-captured with them, the base equals base-1. 4,843 tests: `review`'s two pty tests moved from `plan_desk.rs` to `application/review.rs` under what they now hold, and one moved with the testkit's split.)

### Task 39: starting in the application, at a pty against the mock

**Description:**
- **`crates/cli/tests/application/starting.rs`**, bare `yunta` at a pty:
  - from an empty field, the first run: what to build typed, a workflow picked, the run started detached and followed;
  - from the field, `n`: a start, the run started and followed.

**Fails first:** each test seen red with the start's work dropped for the run: no run is listed.

**Found in its turn:**
- **From the field a run starts through the catalog:** `w`, then `enter` on a workflow, which opens the start with it picked; `n` on the field is the run that waits on the person. Picked there, the start asks each input by its name.
- **The first run answers what it asks once it read the repository's workflows**, which it shows under the question: an `enter` that comes before does nothing, so the test waits for them as a person sees them.
- **The engine the application starts stays in the terminal's session**, as one `yunta run --detach` starts from a shell does: its standard streams are null and its log, and the terminal closes only once it ends. The harness tells the run's process ended apart from the terminal closing (`Terminal::exited`), and a guard lets the held run go however the test ends, so a failing test never hangs on it.
- The requesting workflow lives once, in `yunta_testkit::workflows::GREETS`.

**Acceptance criteria:**
- [x] Each run started is detached and listed by `yunta list`: an engine of its own, which goes on once the application leaves.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test application starting`

**Dependencies:** Group check: deciding and `review`

**Files likely touched:** `crates/cli/tests/{application.rs,application/starting.rs}`

**Estimated scope:** S

### Task 40: the repository's stages, at a pty

**Description:**
- **`crates/cli/tests/application/repository.rs`**, bare `yunta` at a pty. Each stage is reached from the palette (`/`), and leaves what its command leaves:
  - setup: a repository never set up opens on it, and the setup is written;
  - the catalog: a workflow checked, a new one written, its cases run;
  - the packs: listed and audited;
  - the documents' shapes.

**Fails first:** each test seen red with its stage's row taken out of the palette for the run.

**Found in its turn:**
- Over the field, `yunta list` is the field's own act (`w`), so taking the rows every stage offers out of the palette leaves it there. The red takes the catalog's and the shapes' steps out of the palette whole. Setup opens on its own, not from the palette; its red is its write dropped.
- A row is reached by its whole name, which the palette ranks over any row it only starts: `yunta list` over the field's `yunta list --runs …`.
- An `escape` that a letter follows at once reads as that letter with Alt, so the tests wait for the stage the escape goes back to before the next key.
- `yunta schema` prints a shape's words as YAML's comments, which the application says as text: the test compares the words.
- Setup escaped through writes what `yunta init` writes; `new` from the catalog writes what `yunta new` writes, its first shape `one-node`, as `new` picks off a terminal.

**Acceptance criteria:**
- [x] What each stage writes reads back as its command writes it.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test application repository`

**Dependencies:** Task 39

**Files likely touched:** `crates/cli/tests/{application.rs,application/repository.rs}`

**Estimated scope:** M

### Task 41: what runs leave, and the machine, at a pty

**Description:**
- **`crates/cli/tests/application/runs_left.rs`**, each from a run's stage:
  - its stats and its graph;
  - its receipt;
  - its page for a browser, shown through a stand-in opener on the test's `PATH` that records the page;
  - and one not shown, when the stand-in fails: the screen says where the page is.
- **`crates/cli/tests/application/machine.rs`**, from the palette:
  - doctor's findings;
  - upkeep's dry run and its collection, asked first.

**Fails first:** each test seen red with its stage's row taken out of the palette, or its act unbound, for the run.

**Found in its turn:**
- **A page is unshown only when no opener can start:** the opener is spawned and not waited on. The application runs on a `PATH` that holds the real git and, where a page is shown, a stand-in opener that records it, so no test opens a browser (`stubs::path_opening_no_browser`).
- Off a terminal, `yunta open` says where the page is; the page the application handed the opener is that one.
- Upkeep has a run to collect only where the repository keeps its runs no day (`storage.retention_days: 0`), and the run collected is gone as `yunta gc` leaves it.
- Over the field, doctor is the field's own act (`m`), as the catalog is: the red takes its step out of the palette whole. Doctor's findings are read by their headline, the rest falling below the region.
- The test of the repository's git lookups counts through `stubs::git_counting`, which the testkit already had.

**Acceptance criteria:**
- [x] The page shown is the one `yunta open` names; the page not shown is said in render's words.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test application runs_left machine`

**Dependencies:** Task 40

**Files likely touched:** `crates/cli/tests/{application.rs,application/runs_left.rs,application/machine.rs}`

**Estimated scope:** M

### Group check: starting and every stage
- [x] The suite's groups green; every stage reached in the application at a pty. (`group-stages.txt`: 4,854 tests, none gone but the two `review` tests moved and the one the testkit's split moved; 158 views, the same four differ; 247 goldens, the same seven changed. At a pty: the signpost, setup, the first run, the field, a run, a decision on its card and at the desk, `review`, a start, a run's numbers, graph, receipt and page shown and not shown, the catalog with its check, a new workflow and the cases, the packs listed and audited, doctor, upkeep and its collection, and a document's shape.)

### Task 42: the docs

**Description:**
- **`README.md`:** bare `yunta` opens the application at a terminal and prints the field elsewhere; `review`'s row says it opens the application on the plan.
- **`docs/guide.md`:**
  - the application: what opens it, its keys, the palette and help, leaving;
  - the paragraph at `:786`: `review` opens the application on the plan, over its run and the field, and the card's `r` still opens the desk.
- **`.agents/architecture.md`:** the rows of Tasks 9 and 10 read true of the closed module.
- **`CAPABILITY-MAP.md`:** the `tui` row closed, with what this module added to `yunta-ui` and Surco.

**Fails first:** none: the docs. `docs_sync.rs` holds the README's command table and every output the docs quote to the binary, and `doc_claims.rs` holds the reference to the schemas.

**Found in its turn:** `.agents/architecture.md`'s rows for `yunta-tui` and `yunta-testkit-ui` already read true of the closed module, so it is unchanged. The map says what each module delivers, with no mark for a closed one: the `tui` row says what it delivered and what it added to `yunta-ui` and Surco.

**Acceptance criteria:**
- [x] `cargo xtask docs --check` passes with every cap unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test docs_sync --test doc_claims`; `cargo xtask docs --check`

**Dependencies:** Group check: starting and every stage

**Files likely touched:** `README.md`, `docs/guide.md`, `.agents/architecture.md`, `CAPABILITY-MAP.md`

**Estimated scope:** S

### Group check: the docs
- [x] The suite's groups green; `cargo xtask docs --check` passes. (Only docs changed since the group check before, whose groups are green: `docs_sync` and `doc_claims` green, `docs --check` and `smells --check` hold.)

### Task 43: the proof

**Description:**
- The suite's count against the base's, by name: moved, renamed, new, and none gone without a reason.
- Every golden's digest; the captures, with bare `yunta` and `review` at a pty shown beside the base's.
- The walk's counts and time, `yunta-ui`'s and `yunta-tui`'s.
- The release binary's size by crate against the base's.
- All of it in `proof/tui/after.txt`.

**Fails first:** none: a measurement.

**Acceptance criteria:**
- [x] Every difference from the base is one this list names. (`proof/tui/after.txt`: three base tests gone, each moved or rewritten under what it now holds; four views differ, bare `yunta` and `review` at a pty; seven goldens changed, Surco's catalog with its rows.)

**Verification:**
- [x] Manual check: `after.txt` read whole.

**Dependencies:** Group check: the docs

**Files likely touched:** `.plans/yunta-tui/proof/tui/` (outside git)

**Estimated scope:** S

### Task 44: a real run followed and answered in the application

**Description:**
- As Task 30, from the final tree's release binary. Its gates are answered in the application, on their cards: the driver reads the card and presses its keys.
- Its time between nodes and its tokens are set beside Task 3's and Task 30's.

**Fails first:** none: a measurement.

**Found in its turn:** the quick mode's only gate is `ship`, answered `abort` on its card in the application as Task 30 answered it with `resolve-gate`: the log records it `by unverified:sandbox`, the pty's user (`follow_and_answer.py`).

**Acceptance criteria:**
- [x] `between_secs` inside the base's spread; every gate answered in the application is on the run's log. (0.562214 against the base's 0.559435.)

**Verification:**
- [x] Manual check: the run's numbers in `after.txt`.

**Dependencies:** Task 43

**Files likely touched:** `.plans/yunta-tui/proof/tui/` (outside git)

**Estimated scope:** S

### Task 45: what the module weighs

**Description:**
- As Task 31, from the final tree: a frame at 200×60, the first screen of bare `yunta`, and the release binary's size by crate.

**Fails first:** none: measurements.

**Acceptance criteria:**
- [x] A frame under 2 ms; the first screen within 20 ms of the base's; the binary under 40 MiB. (0.45 ms; 66 ms against the base's 70 to 72, from a rest; 30,583,392 bytes.)

**Verification:**
- [x] Manual check: the numbers in `after.txt`.

**Dependencies:** Task 44

**Files likely touched:** `.plans/yunta-tui/proof/tui/` (outside git)

**Estimated scope:** XS

### Task 46: the review

**Description:**
- The module's diff, read in four parts by readers of their own:
  - Surco's new lines;
  - `yunta-tui`: `layout`, `cells`, `drawn`, `keys`, `full_screen`, `application`;
  - `yunta-ui`'s additions and the walk's move;
  - the binary (`application.rs`, `review`, the shell's signals, the opener) and its tests.
- Each finding is checked against the code and the base, recorded here with its decision, and each fix is red first.

**Fails first:** none: a review.

**Found in its turn:** 35 findings; 26 fixed, 9 rejected with their reason, U2 by the user.
- Surco:
  - S1, a prompt short of rows took the rows of the blocks under it, where the doc said they stand whole. Fixed in the doc, not the rule: truncating the prompt instead, as first decided, hid the cursor of an open field under a long note, which the terminal's walk caught at 60×14. The pane's doc and its test now say what stands under a prompt gives way from its end when the prompt needs its rows (`what_stands_under_a_prompt_short_of_rows_gives_way_from_its_end`).
  - S2, a choice list held in fewer rows than its least drew more: its keys now give way, then the list's end (`a_list_short_of_rows_keeps_to_them_and_its_keys_give_way`, red first).
  - S3, a legend of one key wider than its row was not cut: it is now (`one_key_wider_than_its_row_is_cut`, red first).
  - S4, the prompt kinds were listed in `prompt_at` and in `held_in`: `doc::Prompt`, which `Block::prompt` gives, is the one list, and `held_in` matches it whole.
  - S5, S6, S8: the trail's doc says a tally wider than the row is cut; the box's says it takes at least its two sides; `APART` says why it is two.
  - S7 rejected: `Paned` and `Placed` hold different frames (a pane's cursor counted from its top, a laid region always), each named apart in its doc.
  - S9 rejected: both spellings reach the one item at its one path.
- `yunta-tui`:
  - T1, `carried_out` took the state's requests last first, so leaving ran before letting go of the work under way: in order now (`what_the_state_asks_is_carried_out_in_order_leaving_last`, red first).
  - T2, a panic unwinding through the held terminal panicked again in its drop: the drop leaves the hook alone while unwinding (`a_panic_unwinding_through_the_holding_gives_the_terminal_back_and_goes_on`, red first).
  - T3, a drawing that changed no cell still wrote the cursor's escapes every tick: the loop keeps the last buffer and discards a drawing equal to it (`a_drawing_that_changes_no_cell_writes_nothing`, without stripping escapes; a mutation that keeps the old buffer after a suspend fails `after_ctrl_z`).
  - T4 rejected: the box hides what is under it, the attention row with the rest, and gives it back when it closes.
  - T5: the colors' doc names the five of the terminal's sixteen Surco paints with.
  - T6: `draw`'s doc says what a layout hands back carries out, and how it ends where that asks to leave; the dropped `Exit` is gone with T1's change.
  - T7: one rule for the margin, `layout::inner_columns`, read by the loop and by the drawing, and one for the box's width, `box_columns`, read by `boxed`; `Holds::overlay` holds the box's whole rows, so its edges are counted in Surco alone.
  - T8: `saturating_add` where what a screen is about takes its blank row.
  - T9: the layout's docs say the rules themselves, and why the least terminal is 60×14, without naming a reference.
- `yunta-ui` and the walk:
  - U1, two openings with as many stages to open were one state to the walk, so the second was never walked: `stages_to_open` gives the stages, which the walk keeps each opening by (`openings_on_two_steps_differ_by_the_stages_they_have_yet_to_open`, red first).
  - U2 rejected by the user at Checkpoint C: the stack under a stage is not in the walk's key, so a `Decline` from the second way to a screen is not walked. Measured: by kind, 22,388 states become 34,351 and the ui walk takes 97 s, not 55; by the stages whole, it passes its bound of 40,000. No check failed on the states it added, and going back the way a person walked is held by the state's and each stage's own tests.
  - U3, U4: the misplaced doc of `laid` is back over it; the walk's doc names all four things it keeps a state by.
  - U5: `Want::opening(step)` is `Want::taking(step)`, apart from `Stage::opening`, what a stage asks as it opens.
  - U6 rejected: the test claims the screen's time at the state's cadence, and the loop reads the same constant.
- The binary and its tests:
  - B1: an interrupt from outside ends the application with 130 and a hang-up with 129 (two tests; a mutation mapping both to a terminate fails them).
  - B2: the Ctrl-Z test under a shell's job control: the process stops for real there, so the tests send what `fg` sends while they wait (`Terminal::go_on`); proven under `bash -i`, where it hung before.
  - B3: `FINISHES` has one home in the testkit's workflows; the palette's helper and following a closed run sit at the tests' root; `Hold`, in the testkit, is the marks a held run says and hears by, which `HeldRun` and the start from the field share, and which gives the word to go on when dropped.
  - N1: `application::opened` is `application::at_this_terminal`.
  - N2 rejected: `review` prints its page where the application does not open, as its help says.
  - N3: the opener is reaped in a thread of its own (`openers_reaped.py`: one ended opener left behind before, none after).
  - N4 rejected: the signal handlers outlive the application only for the moments before the process exits.
  - N5 rejected: the stand-in opener names the system's opener as the binary does, and a difference fails the opener's tests loudly.
  - N6: `process_id`'s doc says it once.
  - N7 rejected: `EndSignal` is the shell's, which listens for it; the output maps it to a code.
  - N8: a terminal a process the run started holds open fails the test at the deadline, in `ended` and in the drop (`a_terminal_a_process_the_run_started_holds_open_fails_rather_than_hangs`, red first).
- Found by the group check after the fixes, in the base's code: the CLI's inline hold wrote the cursor's escape after its frame's synchronized update, so a terminal showed the new frame with the cursor of the one before, and `r_opens_the_desk_on_the_other_screen_and_esc_comes_back_to_the_card` failed under the batch's load. The frame now writes the cursor's state inside its own update (`Console::cursor_escape`): 3 of 80 runs failed from ten workers at once before (`stress_binary.py`), 0 of 80 after.

**Acceptance criteria:**
- [x] Every finding is recorded here with its decision.

**Verification:**
- [x] Tests pass: the suite's groups after the fixes. (4,865 tests, none failed, none aborted: the engine's groups once the fixes were in, `crates` and the CLI's again after the cursor's fix; fmt, clippy, smells and the docs check pass.)

**Dependencies:** Task 45

**Files likely touched:** as the findings say

**Estimated scope:** M

### Checkpoint C: the module
- [x] The spec's ten success criteria, each with its evidence in `after.txt`.
- [x] The user reviews the proof and the review. (Approved 2026-11-12, U2 left out as recommended.)
- [ ] The phase commits and is pushed; Task 47 follows, then `cargo clean`.

### Task 47: the pull request

**Description:**
- PR #30's body gains the module; CI on Linux and macOS. The user decides when the pull request leaves draft.

**Fails first:** none.

**Found in its turn:** CI's `ci` and `test-macos` jobs were cancelled at their 30 minutes with no test failing: the module's two walks took 406 s on Linux, and 83% of a walk was drawing every reached state's screen to compare it, on one core while the runner's others sat idle. The user chose both: the walk expands the frontier's head on every core and takes what it came to in a one-at-a-time walk's order (the same 22,388 states, 30 palettes, 70 helps and 5,145 screens at each size; the interface's walk 55 s to 19 s and the terminal's 85 s to 44 s on this Mac), and the two jobs' limits rise to 45 minutes. What stayed on one thread in the terminal's walk was mostly Ratatui's constraint solver, whose cache is a feature of Ratatui the workspace had left off: at the user's word it is on (`layout-cache`, which brings `critical-section` 1.2.0, MIT or Apache-2.0, and `cargo deny check` passes), each layout solved once for its size: the terminal's walk takes 22 s, and a drawing at 200×60 0.20 ms where it took 0.45.

**Acceptance criteria:**
- [ ] CI green on Linux and macOS.

**Verification:**
- [ ] Manual check: `gh pr checks 30`: ci, musl-build, test-release and test-macos pass.

**Dependencies:** Checkpoint C

**Files likely touched:** PR #30's body

**Estimated scope:** XS
