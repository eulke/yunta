# Spec: `tui`

Module `tui` of `CAPABILITY-MAP.md`: the terminal application. `yunta-tui` lays out every screen `yunta-ui` makes, with Ratatui on crossterm. Bare `yunta` opens it at a terminal, and `yunta review` opens it at a run's decision. It depends on `app-core` and `parked-answers`, and starts from `152fc892`, where `app-core` ended.

Status: **approved on 2026-11-11**, with these choices by the user:

- `yunta review` opens the decision over its run and the field: after answering, `esc` walks back into the application, and `q` leaves.
- The inline prompts keep the CLI's own key reader, as the map places them. crossterm reads keys only in `yunta-tui`.

## Objective

At a terminal, a person types `yunta` and gets the whole application on one screen. They can follow every run, answer the runs that wait on them, start runs, read what runs cost and how their workflow is drawn, and keep the catalog and the machine. Every key and every piece of news works as `yunta-ui` decides. Every word is `yunta-render`'s, and every mark is Surco's. Off a terminal, `yunta` prints what it prints today.

`app-core` built the application with no medium: the state, its stages, the work it asks for and each moment as a Surco `Screen`. This module is the first surface over it. The rule the user set for the initiative holds here: a surface plugs in and gets the behavior. `yunta-tui` reads keys, carries out the work through the performer, lays out the screen and restores the terminal. It decides nothing that the desktop would have to decide again.

### What happens at the base commit (`152fc892`)

**Bare `yunta` prints the field.** `cli/dispatch.rs:36` sends no subcommand to `commands::list::field`. That prints the repository's runs at a glance, or the signpost outside a repository, with no terminal check on this path.

**`yunta review` holds the plan desk on the terminal's other screen.**
- At a terminal, `commands/review.rs:46` calls `chosen_on(…, Opens::ThePlan)`, which reaches `ask::decide`.
- On a terminal of at least 40×12 (`DESK_COLUMNS`, `DESK_ROWS`), the desk is held on the alternate screen (`ask/decision.rs:76`); on a smaller one the documents are printed instead.
- It exits 0 when a person decides or declines, and 130 when interrupted (`resolve_gate.rs:271`).
- The same `ask::decide` serves `resolve-gate` at a terminal (`resolve_gate.rs:288`, on the card) and an attached run's gate (`human_interaction.rs:133`, `:147`). There, `p` opens the same desk on the alternate screen.

**The CLI reads keys its own way, and nothing in the workspace knows Ratatui or crossterm.**
- `ask/held.rs:116-130` turns `ECHO`, `ICANON` and `ISIG` off with nix's termios. It keeps output processing on, so the live region's newlines still return the carriage while a prompt is held.
- `console` 0.16.6 reads the bytes. `ask/keys.rs:105` and `:126` parse escape sequences by hand, and `keys.rs:55` turns a `console::Key` into a `Stroke`.
- `crossterm` and `ratatui` appear in no manifest and not in the lockfile. `crates/ui/tests/no_terminal_in_the_interface.rs:9` refuses both, with `console`, `nix` and `indicatif`, in `yunta-ui`.

**`yunta-ui` is ready for a surface, and no surface uses it yet.**
- `State::opened(now)` asks to read the repository, then stands on the signpost, a setup, a first run or the field (`state.rs:58`).
- `State::update(Input) -> Vec<Request>`, `State::screen(&Look) -> Screen`, `State::typing()`, `State::waiting_on()`.
- `Performer::take(self: &Arc<Self>, Request, &UnboundedSender<Input>) -> Option<Request>` starts work under its ticket. It hands back what only a surface can do: `ShowPage(PathBuf)` and `Leave`.
- `Input::Laid(ScreenLaid)` is how a surface reports what it laid out of each region, for the next key to scroll from.
- The binary names none of these. Only `crates/cli/tests/interface*` drives them.

**Surco draws a pane, and leaves the screen to a cell engine.**
- `surface/mod.rs` says: "The terminal draws no screen: an engine of cells lays one out and draws each pane's document here at `Width::exact`."
- `Terminal::on(look)` offers:
  - `lines(&Doc)`, `block(&Block)`;
  - `held(&Block, columns, rows) -> Placed { lines, cursor, laid }`, a prompt fitted to its rows with the cursor where it is typed into;
  - `scrolled(&Doc, Sight, rows) -> (Vec<Line>, Laid)`.
- A `Line`'s spans carry a semantic `Tone` and a `Fill`, not a color.
- `paint::style(layer, tone, own, line) -> Style { bold, dim, fg, bg }` and `paint::erase(layer, line)` are the one color table. Their `//!` names "any other engine that draws Surco's lines in cells of its own" as a reader.
- The legend, the attention and the ribbon are blocks `Terminal::block` draws.
- The trail, the tally, the rule under them and a region's title have no terminal lines in Surco.
- Which of `held` and `scrolled` draws a region is decided by each caller. The walk's test terminal (`crates/ui/tests/walk/terminal.rs`) scrolls every region, while the CLI's hold draws prompts.

**The reference is drawn and its rules are written.** `.plans/yunta-tui/proof/surco-screen/targets/tui-reference/` lays out the run's screen at 80×24 in ASCII and at 60×14 in Unicode, the second also with help open. Its `NOTES.md` states the rules the frames follow:
- **Margins:** a cell at each side.
- **Under the trail:** the empty furrow as a rule.
- **Across the screen:** what the screen is about, then a blank row, then the regions.
- **The side region:** 22 cells and an edge of ` │ `. It gives way when the main region would get fewer than 40 cells.
- **Region titles:** strong when the region has the keys, muted otherwise.
- **The bottom rows:** the attention row, tinted, then the legend.
- **Short of rows:** the regions give way first, and the attention row last.
- **The overlay's box:** at most 60 cells and two cells inside each edge, centred, its top a third of the way down, with its title in the top edge. While it is open, its legend stands in the legend's row.

**The spike is the earlier sketch.** `~/.claude/plans/yunta-tui/spike/` runs Ratatui 0.30.2 (`default-features = false, features = ["crossterm"]`) on crossterm 0.29.
- It writes Surco's lines cell by cell into Ratatui's buffer (`theme.rs:164`), from a copy of the palette that `paint::style` has since replaced.
- Its side region (31 cells, from 100 columns) predates the reference.
- It polls every 900 ms with no runtime, and handles no paste and no resize. It restores the terminal only on a normal exit.
- It drew a frame in 0.45 ms at 200×60.

**What `cargo deny` says, measured.** With `ratatui` 0.30.2 and `crossterm` 0.29.0 added in a throwaway worktree, it raises exactly two refusals:
- `foldhash` 0.2.0 is licensed `Zlib`. It enters through the workspace's own `hashbrown` 0.17.1, whose default hasher Ratatui's `lru` turns on.
- `itertools` is in two versions: 0.14.0 from Ratatui and 0.13.0 from `bindgen`, a build dependency of `libproc`.

On crates.io today, Ratatui 0.30.2, `ratatui-crossterm` 0.1.2 and crossterm 0.29.0 are each the newest release, all MIT, needing Rust 1.88 against the workspace's 1.98.

### Who this is for

- **A person at a terminal**, who types `yunta` or `yunta review <run>` and works in the application until they leave.
- **The desktop, later.** It plugs into the same `State` and `Performer` and finds nothing in `yunta-tui` it would have to repeat: every decision this module needs that is not about cells goes to `yunta-ui`, Surco or render.
- **The CLI**, whose every other command, inline prompt and live region print and behave as they do today.

### What the module delivers

1. **Bare `yunta` opens the application** when stdin and stdout are terminals and `TERM` is not `dumb`, as the map decides.
   - In any other case it prints what it prints today.
   - The CLI's color, glyph and motion settings choose the application's look.
   - It opens on what `State::opened` reads first.
2. **`yunta review <run>` at a terminal opens the application at that run's decision, on its plan.**
   - The run is found before the application opens. A run that cannot be found is refused as today, and the application does not open.
   - Off a terminal, `review` prints the plan and the commands that answer it, as today.
   - The CLI's review path through `chosen_on` is deleted. With it goes the `opens` parameter, since no command opens a decision on its plan anymore.
   - `resolve-gate` and an attached run's gate keep their inline card and its desk.
3. **Leaving.**
   - `q`, or `esc` where nothing is under the stage, leaves with exit code 0.
   - Ctrl-C leaves with 130, as every interrupted command exits.
   - The terminal is left exactly as it was found.
4. **`yunta-tui`, the only crate that knows Ratatui and crossterm.**
   - It depends on `yunta-ui`, `yunta-surco`, `yunta-core` (for the `Clock` it is handed), `ratatui`, `crossterm`, `tokio` and `futures`.
   - It takes a `Performer` the binary built, the binary's `Look`, the workspace's clock, what shows a page, and where to open. It hands back how the person left.
   - It names no API, engine, store or adapter type, and a manifest test holds that.
5. **One loop, driven by what arrives.** It wakes on four things: a key or paste from crossterm's `EventStream`, an input from the performer's channel, a resize, and the time.
   - **The time:** read from the clock the binary hands in, never from the system's, and handed in at the cadence `yunta-ui` says its screens are written for: once a second, as the CLI's region redraws.
   - **Drawing:** the screen is drawn after each batch of inputs. What was laid out goes back as `Input::Laid` only when it changed.
   - **Requests:** each one goes to `Performer::take`. `ShowPage` goes to what shows a page, and `Leave` leaves.
   - **Idle:** nothing is drawn while nothing arrives. Motion turns with the events that arrive, never with a clock.
6. **Keys.**
   - **Key presses:** one function turns a crossterm key event into a `Stroke`, with the table `keys.rs` keeps for the CLI: a character, Enter, Backspace, Delete, the arrows, Home/End, PageUp/PageDown, Tab/BackTab, Esc as `Decline`, Ctrl-C as `Interrupt`.
   - **Releases:** ignored.
   - **Paste:** bracketed paste is on. While `State::typing()` is true, a paste becomes its characters, with each line break a `PastedBreak`. Otherwise a paste answers nothing.
   - **The mouse:** not captured, as the map assumes, so a person's terminal still selects and copies.
7. **Layout, by the reference's rules,** with Ratatui's `Layout`.
   - Every rule in `NOTES.md` holds as written above.
   - The least terminal is the smaller reference, 60×14. Under it, the screen says in render's words how much room it needs, and lays out again when the terminal grows.
8. **Drawing, all in Surco.**
   - **Panes:** each pane's document is drawn through Surco's terminal at `Width::exact` of its cells.
   - **Which drawing:** Surco gains one rule for drawing a region in a pane (`Terminal::pane`, or the name the plan finds free). A region holding a prompt's block is held, fitted to its rows, with the cursor where it is typed into. Any other region is scrolled from its `Sight`. `yunta-tui` and the walk's test terminal both call it.
   - **The parts that are not documents:** Surco gains their terminal lines, so `yunta-tui` composes no words and no tones:
     - the trail with the tally beside it,
     - the rule under them,
     - a region's title, strong or muted,
     - the glyphs of the overlay's box (`Shape::Box`, `+` in ASCII).
   - **Color:** read only through `paint::style` and `paint::erase`, then carried to Ratatui's `Style`. Under `Paint::Plain`, a cell gets no style at all.
   - **The cursor:** shown only where a held prompt places it.
9. **The terminal is held and always given back.**
   - **Held:** the alternate screen, raw mode, bracketed paste and a hidden cursor.
   - **Given back on every way out:** leaving, an error, a panic (a hook restores the terminal before the panic is printed), and a terminating or hang-up signal.
   - **Suspending:** Ctrl-Z gives the terminal back and stops the process. `fg` holds the terminal again and draws the screen whole.
   - **A terminal that cannot be held:** `yunta` prints as off a terminal, as the CLI's console does today when it cannot open.
10. **What `yunta-ui` gains, so every surface gets it.**
    - **Opening on a step:** the state can open on a step a command line names. `NextStep::Review { run }` opens the run's decision on its plan, over the run, over the field, so going back reaches them as if the person had walked there. One mapping from a step to the stage that takes it replaces the two at the base: `Stage::opened_by` for the palette's steps, and the run's acts.
    - **The time cadence:** how often its screens want the time, stated once.
    - **A page not shown:** a page the surface could not show comes back as an input, and the stage says so in render's words, with where the page is.
11. **`deny.toml`.**
    - A license exception for `foldhash` alone, allowing `Zlib`, with its reason.
    - A `skip` for `itertools` 0.13.0, with its reason, beside the existing one for `shlex`.
    - Nothing is allowed workspace-wide.
12. **The docs follow.**
    - `README.md`: bare `yunta`, and `review`'s row.
    - `docs/guide.md`: the application, and the paragraph at `:786`.
    - `.agents/architecture.md`: the `yunta-tui` row and the binary's edge.
    - The map: its `tui` row, with what this spec adds to `yunta-ui` and Surco, and `yunta-tui`'s dependencies (`yunta-core` joins them).

### Names and form

- The crate is `yunta-tui` in `crates/tui`, with the library `yunta_tui`. The map fixes both.
- Modules are named for what they are. The plan names them:
  - the loop;
  - the keys turned into strokes;
  - the layout of a screen in cells;
  - the cells Surco's lines are written into;
  - the terminal held and given back.
- Names taken where this module works:
  - `Terminal`: Surco's surface, Ratatui's terminal, and the testkit's pty.
  - `Screen`, `Region`, `Pane`: Surco and the CLI.
  - `Frame`: Ratatui, and four in the workspace.
  - `Layout`: Ratatui.
  - `Held`, `Host`, `Painter`, `Left`, `Leaving`, `Departure`: taken in the workspace.
- Free: `Application`, `Session`, `Keyboard`, `Viewport`. A Ratatui item is named by its path where a Surco item shares its name.

### Decisions the plan starts from

1. **The application never hosts a run,** as the map decides. Starting, answering and stopping are the performer's, through the API.
2. **The binary chooses the look and owns the clock.** `yunta-tui` reads no environment and no system clock of its own: the CLI's color policy, glyphs and motion arrive in the `Look` it is handed, and the time comes from the workspace's clock.
3. **Showing a page stays the binary's.** The system's opener moves from `commands/open.rs:47` to a module named for what it does. `yunta open` and the application both call it, so it has one path.
4. **The slices the map names are the plan's phases:** the crate, the layout and the terminal; watch; decide and `review`; start; then every other stage. Every stage exists in `yunta-ui` already, so one layout serves them all, and the phases order the proof.
5. **`tasks/plan.md` and `tasks/todo.md` are replaced for this module.** Those of `app-core` stay in its commits, the last `152fc892`.
6. **The module lands in phases,** each a commit with the suite green, in the initiative's one pull request, which leaves draft when this module closes.

### Assumptions

1. Attached `yunta run`, `resolve-gate` and every inline prompt keep their look and keys, on the CLI's own hold. Only `review` moves to the application.
2. The least terminal is 60×14. A person with less is told how much room is needed.
3. Hyperlinks are not drawn. Ratatui's cells carry none, and every link's words are on screen.
4. Ctrl-Z suspends, as any full-screen program does. It needs a signal raised, which `nix` gives and the CLI already depends on.
5. The pty harness in `yunta-testkit` drives the end-to-end tests. No new test crate is added.

### Found while measuring

- **The map's "duplicated full-screen loop" is one hold at this commit.** `review` shares `ask::decide` with `resolve-gate` and an attached run's gate. Deleting review's call is all the map's deletion leaves, and the desk's hold stays for the card's `p`.
- **A run asking at another terminal is already shown read-only,** as the map assumes. A question asked live is on no log until it is answered, and `RunFrame::actions` (`actions.rs:57-61`) offers such a run only its stop: "the question is answered where it is asked". The attention row sends the person there. Nothing in this module changes it.
- **The spike's copied palette is gone.** `paint::style` is the one table now, so the map's rule "a color has one table" holds by construction.
- **`foldhash` is not Ratatui's alone.** It is the default hasher of the workspace's `hashbrown`, which Ratatui's `lru` turns on for every crate that shares it. The exception names the crate, not its path.
- **`.agents/architecture.md` is at 6,116 bytes** of the cap its check holds. The new row needs room taken elsewhere or a raised cap.
- **No item in Surco lays out the trail, the tally or the rule for a terminal.** A cell engine would otherwise compose words and tones that are Surco's. Point 8 holds it.
- **A page the system's opener cannot open has no way back to the screen.** `Request::ShowPage` carries no ticket and no input answers it. Point 10 holds it.

### Out of scope

- The desktop.
- The CLI's inline prompts, its live region and their keys.
- The mouse.
- Any change to what a command prints or exits with, beyond bare `yunta` and `review` at a terminal.

## Tech Stack

Rust 1.98.0. A new workspace member `yunta-tui`, on:
- `yunta-ui`, `yunta-surco` and `yunta-core`;
- `ratatui` 0.30.2 with `default-features = false, features = ["crossterm", "layout-cache"]`: each layout is solved once for its size, which brings `critical-section` 1.2.0 (MIT or Apache-2.0);
- `crossterm` 0.29.0 with `event-stream`;
- `tokio` and `futures`, already in the workspace;
- `nix`, already in the workspace, for suspending.

The binary gains `yunta-tui`. Every new crate is MIT, or offered under MIT or Apache-2.0.

## Commands

`cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.

```
# Before editing a file, and before finishing with no paths
cargo xtask docs --about <files>

# Test
cargo test -p yunta-tui
cargo test -p yunta-ui
cargo test -p yunta --test <target>

# Lint and checks
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets -- -D warnings
cargo xtask smells --check
cargo xtask docs --check
cargo deny check

# The suite, in groups, and the gate, every other step of the ci job
python3 .plans/yunta-tui/tools/suite.py <group> <log dir>
sh .plans/yunta-tui/tools/gate.sh /Users/eulke/development/yunta-tui <log dir> <step ...>
```

## Project Structure

```
crates/tui/                      yunta-tui: the loop, keys, layout, cells, the terminal held
crates/tui/tests/                layout goldens on Ratatui's TestBackend, the walk laid out
crates/ui/src/                   opening on a step, the time cadence, a page not shown
crates/surco/src/surface/        a region drawn in a pane; the trail, tally, rule, title and box for a terminal
crates/render/src/screen*        the words this module adds: room needed, page not shown
crates/cli/src/                  bare yunta and review opening the application; the opener in a module of its own
crates/cli/tests/                pty tests of bare yunta and review
deny.toml                        foldhash's exception, itertools' skip
README.md, docs/guide.md         the application
.agents/architecture.md          yunta-tui's row, the binary's edge
CAPABILITY-MAP.md                the tui row
```

## Code Style

The loop reads what arrives, hands it in, carries out what is asked, and draws:

```rust
// crates/tui/src/application.rs (illustrative names)
loop {
    let input = tokio::select! {
        Some(event) = keyboard.next() => match keys::stroke(event?, state.typing()) {
            Some(stroke) => Input::Stroke(stroke),
            None => continue,
        },
        Some(input) = answers.recv() => input,
        _ = cadence.tick() => Input::Time(clock.now()),
    };
    for request in state.update(input) {
        match performer.take(request, &sender) {
            Some(Request::ShowPage(page)) => shown(show_page(&page), &mut state),
            Some(Request::Leave) => return Ok(left),
            _ => {}
        }
    }
    let laid = terminal.draw(|frame| layout::screen(frame, &state.screen(&look), &look))?;
    if laid != last_laid { state.update(Input::Laid(laid)); }
}
```

The time comes from the clock the binary hands in. The smell counter `system_clock_outside_boundary` allows the system clock only in core's clock and the CLI's shell, and `yunta-tui` stays outside that boundary. Every numeric `const` carries its reason, and every sentence is render's.

## Testing Strategy

- **Target goldens first.** The screens of the run, a decision, the palette and help are laid out at 80×24 in ASCII and at 60×14 in Unicode on Ratatui's `TestBackend`, as the reference's rules lay them. They are written and reviewed before the code that draws them.
- **The walk, laid out.** Every state the walk reaches is laid out at both sizes. No panic. No cell is written past its pane's edge. The legend and the attention keep their rows. The cursor shows only at a typed prompt. The plan decides how `yunta-tui`'s tests reach the walk's states; a test feature of `yunta-ui` is the expected form.
- **Keys and paste.** Every crossterm key the table names gives its stroke. A paste while typing gives its characters and breaks, and while not typing gives nothing.
- **Color.** A cell's style is `paint::style` of its span, for each layer. Under `Paint::Plain`, no cell has a style.
- **At a terminal, end to end, in a pty:**
  - bare `yunta` opens on the field, and `q` leaves with the screen and the terminal's modes as they were;
  - Ctrl-C exits 130, and a resize lays out again;
  - a panic inside the application leaves the terminal given back;
  - `yunta review <run>` opens on the plan, and an answer given there is recorded on the run's log.
- **Off a terminal.** Bare `yunta` and `yunta review` print what the captures hold, byte for byte.
- **`yunta-ui`'s additions,** each red first: opening on a step, the cadence and a page not shown, each in the walk's required lists.
- **A real run.** A fragua run is started detached in the sandbox repository, followed in the application in a pty, and its gates are answered there.
  - Its time between nodes and its tokens are read from `yunta stats`, against the same run followed with `status --follow`.
  - The application never hosts the run, so the run's time does not change.
- **Measured.**
  - A frame drawn at 200×60 in a release build, as the spike measured.
  - The first screen of bare `yunta` against the base's bare `yunta` printing the field, median of 20.
  - The release binary's growth, under the 40 MiB ceiling.

## Boundaries

- **Always:**
  - Stage paths by name.
  - Hold every rule `cargo xtask docs --about` prints.
  - Write each test red first.
  - Give every sentence to render and every mark and tone to Surco.
  - Keep the terminal given back on every way out.
  - Run the ci job's steps with `CARGO_INCREMENTAL=0` before each phase's commit, then `cargo clean`.
- **Ask first:**
  - Any crate beyond those named here.
  - Any change to what the CLI prints or how its prompts behave.
  - A cap raised in a check.
  - Any departure from the reference's rules.
- **Never:**
  - Ratatui or crossterm outside `yunta-tui`.
  - A color outside `paint::style`.
  - A clock read below the surface.
  - The mouse captured.
  - A run hosted by the application.
  - `git add -A` or `git stash`.
  - Citing a document in code, tests, commits or the pull request.

## Success Criteria

1. Bare `yunta` at a terminal opens the application on the field, the signpost, a setup or a first run, as the repository stands. Off a terminal, or with `TERM=dumb`, it prints what the base prints, byte for byte.
2. `yunta review <run>` at a terminal opens on that run's plan, and an answer given there is on the run's log. Off a terminal it prints as the base does, and an unknown run is refused as the base refuses it.
3. Leaving exits 0, and Ctrl-C exits 130. On every way out (leaving, an error, a panic, a signal, a suspend), the terminal's modes, screen and cursor are as they were.
4. Every state the walk reaches lays out at 80×24 and at 60×14 with no panic and nothing past an edge. The run, decision, palette and help screens equal their reviewed goldens.
5. `yunta-tui` is the only manifest naming `ratatui` or `crossterm`, names no API, engine, store or adapter type, and reads no system clock. Every color in it comes from `paint::style` or `paint::erase`.
6. `yunta-ui` opens on a step, states its time cadence and says a page it could not show, each held by a test and by the walk.
7. `cargo deny check` passes with one license exception (`foldhash`, `Zlib`) and one new `skip` (`itertools` 0.13.0).
8. A real fragua run followed and answered in the application keeps its time between nodes within the noise of the same run followed with `status --follow`.
9. A frame at 200×60 is drawn in under 2 ms in a release build. The first screen arrives within 20 ms of the base's bare `yunta` (median of 20). The release binary stays under 40 MiB.
10. The suite, every ci step and CI on Linux and macOS are green. `README.md`, `docs/guide.md`, `.agents/architecture.md` and the map say what changed.

## Open Questions

None. Both were answered when the spec was approved:

1. **Where `yunta review` leaves a person:** over its run and the field, so `esc` after answering walks back into the application, and `q` leaves. Leaving as soon as the decision is recorded was the other choice.
2. **One key reader, or two:** two, as the map places them. Moving the inline hold onto crossterm would need its raw mode, which turns output processing off, and the live region's newlines need it while a prompt is held.
