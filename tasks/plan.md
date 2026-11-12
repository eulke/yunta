# Implementation Plan: `tui`

Spec: `SPEC-tui.md`, approved on 2026-11-11. Base: `feature/yunta-tui` at `152fc892`, where `app-core` ended. `cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.

## Overview

`app-core` left the whole application as values: the state a person has open, what each input does to it, the work it asks of Yunta, a performer that carries that work out, and each moment as a Surco `Screen`. Nothing shows it yet.

This plan builds the first surface, `yunta-tui`. It is a loop that:
- reads crossterm's keys and pastes as strokes;
- hands each request to the performer;
- lays the screen out with Ratatui's `Layout`, by the rules of `surco-screen`'s terminal reference;
- writes the lines Surco draws for each part into Ratatui's cells, in the colors of Surco's one table;
- gives the terminal back on every way out.

Bare `yunta` opens it at a terminal. `yunta review` opens it at a run's decision, over the run and the field.

What a cell engine would otherwise decide goes below it, so the desktop gets it too:
- Surco draws a region in a pane, and the parts of a screen that are not documents.
- `yunta-ui` opens on a step and says how often it wants the time.
- The walk becomes a crate every surface's tests walk.

## Architecture Decisions

### 1. What each layer does

| Layer | Holds, after this module |
|---|---|
| `yunta-surco` | Every line a terminal shows: each pane's document, held when it holds a prompt and scrolled otherwise; the trail with the tally, the rule, a region's title, the attention row, the legend row, and the overlay boxed with its title. The one color table. |
| `yunta-render` | Every sentence, the two this module adds among them: how much room the screen needs, and a page the surface could not show. |
| `yunta-ui` | The application. It gains an opening on a step, one mapping from a step to the stage that takes it, going back past the stages it opened over to where the application opens, how often it wants the time, a page not shown, and the room a screen needs, in render's words, for a surface short of room. |
| `yunta-tui` | Cells. Where each part stands in the terminal, Surco's lines written into Ratatui's buffer, keys turned into strokes, the terminal held and given back, and the loop. It composes no word, no tone and no mark. |
| `yunta` | Whether this process opens the application, what it opens on, the look, the clock, the signals, the system's opener, and the exit code. |

### 2. The crates and their edges

| Crate | `[dependencies]` |
|---|---|
| `yunta-tui` (new, `crates/tui`) | core (the `Clock` it is handed), ui, surco; `ratatui` 0.30.2 (`default-features = false, features = ["crossterm"]`), `crossterm` 0.29.0 (`event-stream`), `tokio`, `futures`, `nix` (`signal`, to stop itself on Ctrl-Z) |
| `yunta-testkit-ui` (new, `crates/testkit-ui`) | ui, surco, core, replay, render, api; testkit, testkit-core, storage (`testkit`), tempfile |
| `yunta` | gains tui |

- **Manifest tests in `crates/tui/tests`:**
  - `yunta-tui`'s own manifest names none of `yunta-api`, `yunta-engine`, `yunta-storage`, `yunta-adapters`, `yunta-render`, `console` or `indicatif`. Every word it shows reaches it through `yunta-ui`.
  - No other crate's manifest under `crates/` names `ratatui` or `crossterm`. Each failure says where the reading belongs.
- **`.agents/architecture.md` gains two rows and changes `yunta`'s.** The file is at 6,116 of its 6,144 bytes. The rows' bytes come out of what the file says twice or a `//!` already says, in the same commit. If no such sentence is found, the work stops for the user rather than raising the cap.
- **The map's crate table and its `tui` row** say the same.

### 3. `yunta-tui`'s parts

```rust
// crates/tui/src/lib.rs, what the binary names
pub struct Application { /* the performer, the look, the clock, what shows a page */ }

impl Application {
    pub fn new(
        performer: Arc<Performer>,
        look: Look,
        clock: Arc<dyn Clock>,
        show_page: fn(&Path) -> std::io::Result<()>,
    ) -> Self;
    /// Holds the terminal and runs `opened` until the person leaves, or until
    /// `asked_to_end` resolves; the terminal is given back on every way out.
    pub async fn run<E>(
        &self,
        opened: (State, Vec<Request>),
        asked_to_end: impl Future<Output = E>,
    ) -> Result<Exit<E>, TerminalFailure>;
}

/// How the application ended, with what asked it to end.
pub enum Exit<E> { Left, Interrupted, AskedToEnd(E) }

/// How the terminal failed the application. `Unheld`: it refused raw mode or the other
/// screen, and the binary prints instead of opening. `Lost`: it failed once held, and the
/// command fails.
pub enum TerminalFailure { Unheld(std::io::Error), Lost(std::io::Error) }
```

| Module | Is |
|---|---|
| `application` | The loop: `Application`, `Exit`, `TerminalFailure`. |
| `full_screen` | The terminal held, as `FullScreen`: the alternate screen, raw mode, bracketed paste, a hidden cursor. Given back on drop, by a panic hook, and before stopping on Ctrl-Z. |
| `keys` | A crossterm key event turned into a `Stroke`, by the table the CLI's `keys.rs` keeps. A paste turned into strokes, or into nothing when the state is not typing. Ctrl-Z, which is the terminal's own key and not a stroke. |
| `layout` | Where each part of a screen stands, as `Areas`, by the reference's rules. Pure: a size and what the screen holds in, areas out. Each number is a `const` with its reason: margin 1, side 22, edge 3, main at least 40, box at most 60 and 2 inside each edge, least terminal 60×14. |
| `cells` | Surco's lines written into Ratatui's buffer: each span's `paint::style` turned into a Ratatui `Style`, and the rest of a filled row erased with `paint::erase`. Nothing at all under `Paint::Plain`. |
| `drawn` | A screen drawn into a frame: the areas from `layout`, each part's lines from Surco, the cursor where a held prompt puts it. It hands back the `ScreenLaid`. |

Names checked against those taken: `Application`, `FullScreen`, `TerminalFailure`, `Areas` and `Exit` are free. `Places` is render's (`page::run::Places`). `Ended`, `Leaving`, `Left` (as a type), `Held`, `Pane`, `Frame`, `Session` and `Host` are taken, or are Yunta's for something else. A Ratatui item is named by its path where it shares a name with Surco's (`ratatui::Terminal`, `ratatui::layout::Layout`).

### 4. The loop

- **Its runtime:** the binary's current-thread runtime. The performer spawns each piece of work on it as `app-core` built it.
- **What it waits on, in one `select!`:**
  - crossterm's `EventStream`: keys, pastes, resizes;
  - the performer's channel of inputs;
  - a tick every `State::TIME_EVERY`, which hands in the clock's time;
  - `asked_to_end`.
- **After each wake,** it drains what is already waiting, hands it all in, and draws once.
  - **Requests:** each goes to `Performer::take`. `ShowPage` goes to `show_page`, and a failure comes back as `Input::Unshown`. `Leave` ends the loop.
  - **What was laid out:** goes back as `Input::Laid` only when it changed from the last draw, which may draw once more.
  - **Synchronized update:** each frame is wrapped in one (crossterm's `BeginSynchronizedUpdate`), the sequence the pty harness reads a whole drawing by.
- **Leaving:**
  - `Leave` after a Ctrl-C stroke is `Exit::Interrupted`; after anything else, `Exit::Left`.
  - `asked_to_end` resolving is `Exit::AskedToEnd`.
- **Ctrl-Z:** gives the terminal back, raises `SIGTSTP`, and takes the terminal again when the process continues, drawing the screen whole.
- **The signals are the shell's.** The smell counter `process_read_below_the_shell` allows `signal::unix::signal(` only in `crates/cli/src/shell.rs`. The shell gains the terminate and hang-up listeners beside SIGINT's. The binary hands the application one future of all three, and keeps which one fired for the exit code. In raw mode, a typed Ctrl-C is a key, not a signal.

### 5. Layout, by the reference's rules

The rules of `.plans/yunta-tui/proof/surco-screen/targets/NOTES.md`, as `layout` holds them:

- **Rows, from the top:**
  - the trail with the tally (1 row);
  - the rule (1);
  - what the screen is about, as the lines Surco draws of it at the inner width;
  - a blank row (only with something about);
  - the regions, each under its title row where it has a title;
  - the attention (1, only with one);
  - the legend (1).
- **Short of rows,** the regions give way first, then what the screen is about, and the attention row last.
- **Columns:** a margin of 1 at each side. The side region, where there is one, takes 22 cells and an edge of 3 (` │ `). It gives way when the main region would get fewer than 40.
- **The overlay's box:** at most 60 cells wide and 2 inside each edge, centred, between the rule and the legend row: its top a third of the way down the rows they leave, never above the row under the rule. Its rows are the lines Surco boxed. It hides what is under it, and its legend stands in the legend row.
- **Under 60×14:** the screen is render's sentence of how much room it needs, centred, and nothing else. A resize lays out again.

### 6. Drawing: Surco's lines, Ratatui's cells

Surco gains what a cell engine needs, so `yunta-tui` composes no word, tone or mark:

```rust
// crates/surco/src/surface/terminal/, beside `scrolled`
impl Terminal {
    /// `region` drawn in a pane `rows` high, at this look's exact width: a region
    /// holding a prompt holds it fitted to the rows its other blocks leave, with
    /// the cursor where it is typed into; any other region scrolled from its sight.
    pub fn pane(&self, region: &Region, rows: usize) -> Paned;  // lines, cursor, laid
    /// The trail, from the outermost, with the tally at the row's end.
    pub fn trail(&self, trail: &[String], tally: &[Count]) -> Line;
    /// The rule under the trail: the empty furrow across the width.
    pub fn rule(&self) -> Line;
    /// A region's title, strong where the keys act on the region, muted elsewhere.
    pub fn title(&self, title: &str, focused: bool) -> Line;
    /// The edge between the side region and the main one: ` │ `, faint.
    pub fn edge(&self) -> Line;
    /// The attention row, tinted to the width; the legend row: every key where they fit,
    /// else the first that fit, a mark that more stand between, and the last.
    pub fn attention(&self, attention: &Attention) -> Line;
    pub fn legend(&self, legend: &Legend) -> Line;
    /// `overlay`'s document boxed in `Shape::Box`'s glyphs, its title in the top edge,
    /// `rows` high at most: its lines, and what was laid out of it.
    pub fn boxed(&self, overlay: &Overlay, rows: usize) -> Paned;
}
```

- **`Paned`** is free. It is not `Placed`, which stays the held block's.
- **The walk's test terminal** and `yunta-tui` both draw a region through `pane`. Which drawing a region gets is then one rule.
- **Goldens:** each new line is in Surco's goldens, ASCII and Unicode, at 60 and 80 cells.
- **`cells`** maps Surco's colors:
  - `Color::Ansi(Red | Green | Yellow | Magenta | Cyan)` to Ratatui's named colors;
  - `Color::Rgb` to `Color::Rgb`;
  - `bold` and `dim` to modifiers.
  - A test over every tone, fill and layer holds that a cell's style is `paint::style` of its span.
- **Width:** Surco and Ratatui measure with the same `unicode-width` 0.2, one version in the lock. Surco cuts every line to its pane, so no cell is written past an edge, and a test over the walk holds that.

### 7. What `yunta-ui` gains

```rust
impl State {
    /// How often a surface hands in the time: the screens say ages and elapsed times
    /// to the second.
    pub const TIME_EVERY: std::time::Duration;
    /// The application opened at `now` on the stage `step` opens, over the stages it
    /// stands over (a run's stage, for a step of a run), with the application as it
    /// opens under them all; none for a step no stage takes.
    pub fn opened_on(now: DateTime<Utc>, step: &NextStep) -> Option<(State, Vec<Request>)>;
}

pub enum Input {
    // …
    /// A page the surface could not show: the person opens it themselves.
    Unshown(PathBuf),
}
```

- **One mapping from a step to the stage that takes it.** `Stage::opened_by` grows to every step that opens a stage: the palette's four, and `Status`, `ResolveGate`, `Review`, `Receipt`, `Verify`, `Open`, `Stats` and `RunGraph` of a run. These route through it:
  - the run's acts (`run/acts.rs::taken`);
  - the field's `w` and `m` (`field.rs:189-190`);
  - the catalog's `s` (`catalog.rs:244`);
  - doctor's `u` (`doctor.rs:157`).

  Acts that are work, or that ask a confirmation, stay with their stage.
- **Back past the last stage of an opening.** `State::back` with nothing under the stage stands still today (`state.rs:338`). Opened on a step, it reads the repository and stands where `State::opened` stands. `esc` from `review`'s run therefore reaches the field, as the user chose.
- **`Input::Unshown`** reaches the stage that asked for the page: the evidence stage, the only one that shows pages (`evidence.rs:147`, `:191`). It says it in render's words, with where the page is.
- **The walk's required lists** gain each: a state opened on `Review`, back from it to the field, and a page not shown.

### 8. The binary

- **`crates/cli/src/application.rs`** decides and opens:
  - The application opens when stdin and stdout are terminals and `TERM` is not `dumb`. It reads the CLI's settled terminal facts, which `ask::Console::attended` and `surface::terminal` already read.
  - It builds the `Performer` from `shell.workspace()`, the look from the stdout look, the clock from the workspace, the page opener, and the end from the shell's signals.
  - `Exit::Left` is success. `Exit::Interrupted` is `Outcome::interrupted()`. `Exit::AskedToEnd` is the exit code of the signal that fired.
  - `TerminalFailure::Unheld` prints as off a terminal; `TerminalFailure::Lost` is the command's failure.
- **Bare `yunta`:** `dispatch.rs:36` opens the application where it may, and otherwise prints `list::field` as today.
- **`yunta review <run>`:**
  - Its checks stay first and unchanged: the run found, the decision it waits on read, a plan among what it shows. A run that fails one is refused in the same words.
  - Then, at a terminal, the application opens on `NextStep::Review { run }`. Off one, it prints as today.
  - `chosen_on` loses `opens`, and review's own terminal path goes.
  - The refusal it writes with `format!` (`review.rs:31`) moves to render, printing the same.
- **The system's opener** moves from `commands/open.rs:47` to `crates/cli/src/browser.rs`. `yunta open` and the application both call it.

### 9. The walk, for every surface

The walk lives in `crates/ui/tests/walk.rs` and `walk/`: the home, the recordings, the terminal of a few rows, and the dialogs' lists. It walks 17,294 states in about 40 s here.

`yunta-tui` needs the states. `yunta-ui` cannot depend on `yunta-tui`. Its manifest refuses `yunta-storage`, which the recordings use to break a log. So the walk moves to a test crate of its own, named in the testkit family:

- **`yunta-testkit-ui`** holds the home, the recordings and the breadth-first walk, yielding each state it reaches with the inputs that reach it. It also holds the Laid inputs, from its own terminal of a few rows drawing through Surco's `pane`.
- **`crates/ui/tests/walk.rs`** keeps every rule it holds today: no panic, every request performed, Html and Markdown, the palettes, helps, remarks, and the required lists.
- **`crates/tui/tests/walk.rs`** lays every state out at 80×24 in ASCII and 60×14 in Unicode on Ratatui's `TestBackend`. It holds:
  - no panic;
  - no cell written past its pane;
  - the legend on the last row and the attention above it;
  - the cursor shown only at a typed prompt;
  - an overlay's legend in the legend row.
- **The cost** is measured when the crate lands. If laying every state out doubles the walk's time on CI, the tui's walk lays out each distinct screen once, which is how the walk already dedupes.

### 10. `deny.toml`

```toml
[[licenses.exceptions]]
# The default hasher of hashbrown, turned on by Ratatui's lru for every crate that shares it.
crate = "foldhash"
allow = ["Zlib"]

[bans]
skip = [
    # …the existing two…
    # libproc's bindgen lags Ratatui on itertools.
    { crate = "itertools:0.13.0", reason = "libproc's bindgen lags ratatui on itertools" },
]
```

`cargo deny check` passes with nothing else, as the probe at the spec measured.

### 11. How it is proven

- **The design first.** The target frames are written before the code that draws them, by a script that states the reference's rules in Python, apart from the Rust:
  - the field, the run with its side region, a decision's card, the plan at its desk, a confirmation, the palette and help;
  - each at 80×24 in ASCII and at 60×14 in Unicode;
  - the room-needed screen at 59×14.

  Each part's lines come from Surco through a small dump of the screen's parts. The user reviews them at Checkpoint A. `yunta-tui`'s `TestBackend` frames must equal them.
- **Keys, paste and color,** each against its table, red first.
- **The pty tests,** through `yunta-testkit`'s `Terminal` and its vt100 screen:
  - bare `yunta` opens on the field, and `q` leaves with exit 0, the main screen back, the cursor shown and the terminal's modes as before;
  - Ctrl-C exits 130, and a resize lays out again at the new size;
  - Ctrl-Z stops the process, and `SIGCONT` draws it again;
  - a panic leaves the terminal given back: the test runs its own binary again in a pty, where it holds the terminal and panics;
  - `yunta review <run>` opens on the plan, an option chosen there is on the run's log, and `esc` walks back to the run and the field.
- **Off a terminal,** `pages.sh`'s captures of bare `yunta`, `review` and every other command equal the base's.
- **A real run.** Fragua, started detached in the sandbox repository, followed in the application in a pty by a Python driver, its gates answered there. Its time between nodes and its tokens come from `yunta stats`, against the same run followed with `status --follow` at the base.
- **Measured:**
  - a frame drawn at 200×60 in a release build, the median of 2,000;
  - the first screen of bare `yunta` against the base's bare `yunta` printing the field, the median of 20 starts;
  - the release binary by crate (`tools/size_by_crate.py`), under 40 MiB.

## Phases

Three phases, each one commit with the suite green: every step of the `ci` job, and the suite in its groups. Each ends at a checkpoint the user reviews, then `cargo clean`.

Inside a phase, each group of work ends green before the next begins, with what it prints compared against the base. A group does not stop for the user. The work stops inside a phase only on a decision that is the user's: a printed text that would change, a choice this plan does not make, a cap that would rise, or a failure whose cause is not found.

### Phase 1: the design proven before the code

- **The base:**
  - the suite's binaries and tests by name, and every golden's digest;
  - `pages.sh`'s captures, with bare `yunta` and `review` at a pty;
  - the release size by crate, and bare `yunta`'s time to print, the median of 20;
  - a real run followed with `status --follow`.

  `.plans/yunta-tui/proof/tui/` holds them.
- **Surco's parts:** `pane`, `trail`, `rule`, `title`, `attention`, `legend`, `boxed` and `Paned`, with goldens. The walk's test terminal draws through `pane`, its states unchanged in number.
- **The targets:** the frames of section 11 and the script that writes them, with each screen's parts dumped from Surco.
- **The crate:** `crates/tui` with its map, its manifest tests, `deny.toml`, and the rows of `.agents/architecture.md` and of the map.
- **`yunta-testkit-ui`:** the walk moved, with `crates/ui/tests/walk.rs` holding every rule it holds today on the same states.

**Checkpoint A:** the user reviews the target frames and Surco's new lines. Every existing golden, capture and test is equal to the base. `cargo deny check` passes.

### Phase 2: the terminal application, and watching

- **`layout`, `cells` and `drawn`,** red against the targets, then green: every target frame equal.
- **`keys` and `full_screen`,** each against its table and its pty test: given back on leave, error, panic and suspend.
- **`application`:** the loop, with what it hands in first: `State::TIME_EVERY`, and `Input::Unshown` with its words in render. `asked_to_end` comes from the shell's signals.
- **Bare `yunta`:** `application.rs` in the binary, the opener in `browser.rs`, and its pty tests. Off a terminal it prints the same.
- **The tui's walk:** every state laid out at both sizes, with its time measured.
- **Watch:** the run followed to its end and onto a promotion's successor, in a pty against the mock, and a real run followed in the application.

**Checkpoint B:** every target frame equal, the walk laid out, the pty tests green. A real run followed in the application at the base's time between nodes. A frame's time at 200×60, the first screen's time, and the release size.

### Phase 3: deciding, `review`, starting, every stage, and the proof

- **`yunta-ui`:**
  - `State::opened_on`, with one mapping from a step to its stage;
  - back past an opening to where the application opens;
  - each in the walk's required lists;
  - the repository asked once as the application opens: two git processes, as the printed field runs, where Checkpoint B measured four.
- **Decide:** a gate and a round answered in the application, in a pty against the mock.
- **`review`:**
  - it opens the application on the plan, its checks first;
  - `chosen_on` without `opens`, and review's terminal path gone;
  - its refusal's words to render.
- **Start:** a run started from the application, detached, and followed.
- **Every other stage, at a pty:** setup, the catalog and packs, stats, the graph, evidence (a page shown, and one not shown), doctor, upkeep and the schemas. One test each, reaching the stage from the palette.
- **The docs:**
  - `README.md`'s bare `yunta` and `review` row;
  - `docs/guide.md`'s application, and the paragraph at `:786`;
  - `.agents/architecture.md`;
  - the map.
- **The proof:** a real run followed and answered in the application; every capture against the base; the measurements; a review of the module, its findings fixed red first.

**Checkpoint C:** the spec's ten success criteria, each with its evidence. The phase commits. PR #30's body gains the module, CI is green on Linux and macOS, and the user decides when the pull request leaves draft.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| A panic or a signal leaves the terminal in raw mode on the other screen | High | `FullScreen` gives it back on drop and in a panic hook. Signals end the loop, which returns through the drop. A pty test holds each way out. |
| The tui's walk doubles the suite's time on CI | Medium | Measured when the crate lands, against the 108 s the Linux runner takes for the walk. It lays out each distinct screen once if needed. Its bound is in states, not seconds. |
| Ratatui and Surco measure a glyph's width differently | Medium | One `unicode-width` in the lock (`deny` refuses two), Surco cuts every line to its pane, and the walk holds that no cell passes an edge. |
| crossterm's `EventStream` on the current-thread runtime stalls the performer's work | Medium | The stream reads on its own thread and wakes the runtime. A pty test follows a run while keys arrive, and the real run's time between nodes is compared. |
| The pty harness reads a half-drawn frame | Low | Each frame is one synchronized update, the sequence the harness already reads whole drawings by. |
| Moving the walk changes what it reaches | Medium | It reaches the same states, counted, before and after the move, and every rule it holds today passes unchanged. |
| A decision's desk in a 60×14 region has fewer rows than the CLI's least desk (12) | Medium | The desk fits its rows (`Desk::drawn`). The 60×14 targets show it at the room it gets, reviewed at Checkpoint A. |
| `.agents/architecture.md` is at its cap | Low | The rows' bytes come out of what the file says twice. Without such a sentence, the work stops for the user. |
| The binary grows past its ceiling | Low | Ratatui and crossterm added about 251 KB to the spike. The size by crate is read at each checkpoint, against 28,450,112 bytes at the base. |
| The disk fills during gates | Low | `CARGO_INCREMENTAL=0`, one build at a time, `df` and `du` before each gate, `cargo clean` after each phase. |

## Decisions This Plan Takes

The spec leaves these to the plan. Each stands unless the review of this plan changes it:

- The names of section 3: `Application`, `Exit`, `TerminalFailure`, `FullScreen`, `Areas`, and the modules `application`, `full_screen`, `keys`, `layout`, `cells`, `drawn`.
- Surco boxes the overlay (`boxed`), so `yunta-tui` draws no glyph of its own. The map's "draws the overlay's box in Surco's glyphs" is held by Surco drawing it.
- Surco draws the attention and legend rows from the screen's values (`attention`, `legend`), beside `pane`, `trail`, `rule` and `title`.
- `State::opened_on`, `State::TIME_EVERY` and `Input::Unshown`, with one mapping from a step to its stage.
- The walk moves to a new test crate, `yunta-testkit-ui`.
- `review`'s checks stay before the application opens, and refuse in today's words.
- The shell owns every signal, and the application is handed one future of them.
- Ctrl-Z is the terminal's own key, never a `Stroke`.
- The targets add the field, the desk, a confirmation and the room-needed screen to the spec's four.
- Three phases: the design proven, the application and watching, then the rest and the proof.

## Open Questions

None. The names in sections 3, 6 and 7 are the plan's proposal for the user's review.
