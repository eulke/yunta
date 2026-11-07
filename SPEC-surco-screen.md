# Spec: `surco-screen`

Module of `CAPABILITY-MAP.md`: "Public reading of styles and the new components, with their goldens". Status: **approved on 2026-11-06**.
Base: `feature/yunta-tui` at `717148cf`. Depends on: nothing. Needed by: `app-core`.

## Objective

Surco has the grammar and almost every block an application needs. What it lacks is what exists only when an interface stays open, and a way for a drawing engine other than its own escapes to read its colors. This module adds both to `yunta-surco`, with nothing above it changing what it prints.

### What happens at the base commit (`717148cf`)

- **The colors are private.** `palette` is `mod palette;` in `crates/surco/src/lib.rs`. A color leaves Surco only as an escape that `Paint` writes (`paint.rs`, `truecolor_parameters` and `ansi`). The Ratatui spike had to copy the 25 colors of both grounds into its own table (`.plans/yunta-tui/spike/src/theme.rs`: "`palette` is private in `yunta-surco`, so the table below is a copy of its numbers"). A copy drifts the day a color is retuned, and the palette's contrast tests do not reach it.
- **What a screen needs is drawn outside Surco.** The spike draws, in its own `view.rs`, the node names over the furrow, the keys of each screen, the one row that says what needs you, and the boxes that open on top (the palette, help and a confirmation), with Ratatui calls and its own strings. None of them is a Surco block, so no surface but that one can draw them, and no golden holds them.
- **Keys are strings.** `Card.keys` and `DeskFoot.keys` are `Vec<String>` such as `"enter chooses"`. Each of the CLI's prompts (`ask/card.rs`, `desk.rs`, `menu.rs`, `form.rs`) builds its own, and nothing but a terminal can read which key does what.
- **The attached region's attention row is the CLI's.** `crates/cli/src/surface/view/mod.rs` `attention` builds that line by hand. A second interface that says the same thing would build it again.

### Who this is for

- **`app-core` and `tui`**, the modules after this one. `yunta-app`'s `view(state)` returns a `Screen`. `yunta-tui` lays it out with Ratatui's `Layout`, draws each region's document through Surco's terminal surface at that region's width, and reads colors through the public style reading. Neither invents a component, a color or a glyph.
- **The desktop, later**, in GPUI or as a page: another interface with the same look. It reads the same `Screen` and lays it out its own way. A screen holds no cell, no pixel and no color, so each interface sizes its regions itself, and Surco's tokens give them all one look. The `Html` surface draws every screen from the first day, which proves it, as the map's rule "The desktop stays possible without building it" asks.
- **A person at a terminal today**, who sees nothing change. The attached region, the prompts and every page print what they print at the base commit.

### What each of them gets

1. **A public style reading.** `surco::paint` exposes, for a span in a layer (its tone, its own fill and its line's fill), the style it is drawn in, as data: bold or not, a text color and a background color, each one of the sixteen or 24 bits. `Paint`'s escapes are written from that reading and from nothing else, so "a color has one table" holds by construction: a test draws every tone, fill and layer both ways and compares them. The page colors the `Html` surface reads (`palette::page`) are public through the same module.
2. **Five components, each drawn by every surface:**
   - **Ribbon.** The furrow with the names of the nodes over it: the application's signature, at the top of a run. It carries each node's name and standing, and the running node's count of events, where the plow stands. Only an event moves the plow. With `Motion::Still`, or with no color to draw it bright, no plow is drawn and no fact is lost.
   - **Legend.** The keys of what is on screen, as data: each key with what it does, in the order it is read. A terminal writes them in a row. A page shows each one as a button carrying its key, inert on a static page, for the desktop to wire. `Card.keys` and `DeskFoot.keys` become a `Legend`, and the CLI's prompts build one; every prompt prints the same characters as at the base.
   - **Attention.** The one row that says what needs a person, on any screen: what needs them, what it is about, and how to answer, by a command or by a key. The attached region's row becomes an `Attention`, drawn as at the base.
   - **Overlay.** What opens over a screen and closes with `esc`: a title, a document and its own legend. It covers the palette, help and a confirmation.
   - **Screen.** What a whole interface shows, with no cell, no pixel and no color.
     - Its trail says where the person is, and its tally how many runs stand at each word that asks for a look.
     - What it is about stands across it: a run's headline, its ribbon, its gist.
     - A main region and, beside it, maybe a side list that gives way first, each holding a `Doc`. One of the two has the focus.
     - Then come what needs the person, the keys, and at most one overlay.
     - Surco does not lay a screen out on a terminal. That is the `tui` module's job, with Ratatui's `Layout` and Surco's glyphs, and a desktop's, with its own. `Markdown` and `Html` draw a screen as a document and as a page.
3. **A pane's exact width.** `Width::exact(cells)` gives a document exactly the cells another engine's pane has, below the floor of 60 a stream keeps, so `yunta-tui` draws a region of 22 cells through Surco's terminal surface. A pane's edge cuts what passes it, so the terminal surface lays a line past it out in rows: broken at its last space, or else at the edge, each further row hung under where its text starts (the user's choice, after a measurement found twelve blocks past an exact width below 60).
4. **Goldens for each of them.** The catalog (`components/catalog.rs`) draws each new block in every environment (`unicode-color-100`, `ascii-80`, `unicode-60`, `truecolor-dark-100`, `truecolor-light-100`), as Markdown and as HTML. A screen has goldens as Markdown and as HTML, each reviewed before the code that draws it.

### Decisions the plan starts from

1. **Each interface lays a screen out its own way; Surco holds what is said and how it looks.** This was changed on 2026-11-06, at the user's choice, after reading the approved analysis again.
   - A screen names its regions by what they are, a main one and a side one, never by a width. Three layouts decide sizes:
     - the terminal's, in the `tui` module: Ratatui's `Layout`, each region's document drawn by Surco's terminal surface at its width, and the overlay's box in Surco's glyphs;
     - a GPUI desktop's: flexbox and pixels;
     - a page's: CSS grid.
   - Rejected: Surco laying the screen out into a frame of cells (`Terminal::frame`), the choice first approved. It put cells in the screen's data (`Cells::Fixed(22)`, an overlay's width in cells), which a desktop cannot read, and it put in Surco a layout only the terminal uses.
2. **The style reading is the one table.** `Style { bold, dim, fg, bg }`, with `Color::Ansi(u8)` or `Color::Rgb(u8, u8, u8)`, is computed by one function from the layer, the tone and the fills. `Paint`, `Html` and the later Ratatui bridge all read it. The tuned values, their contrast floors and their tests stay where they are; only the reading becomes public.
3. **New blocks, old blocks unchanged.** `Ribbon`, `Legend` and `Attention` are new `Block` kinds, so any document can hold them. `Overlay` and `Screen` are compositions of documents, not blocks. A screen is not a document, so it is not a block.
4. **Moving code does not change what is printed.** `Card` and `DeskFoot` take a `Legend` in place of their strings, and the attached region builds its rows as an `Attention`. Every golden of `yunta-surco`, `yunta-render`, `yunta` and `yunta-engine` that exists at the base stays byte for byte, as the map's rule holds. A golden this module adds is new. The one exception is the catalog's (`components/catalog.*.txt`, `markdown/catalog.md` and `html/catalog.html`). The catalog must hold a sample of every kind of block, so the new blocks join it in a part appended after its last one. Each of those files changes only by what is added at its end (approved with the plan). One row changes a tone, at the user's choice: the region's row for a run whose engine asks at its own terminal becomes an `Attention` answered there, so what it asks is plain, as an attention's question reads, where it was strong. Its words stay, and no golden holds that row.
5. **What is only drawing stays out of Surco.** The wordmark in blocks and the braille horizon of the spike say nothing. They are the `tui` module's, drawn by Ratatui, and an ASCII terminal loses no fact without them.

### Assumptions

1. No surface but Surco's own three exists yet. `Terminal`, `Markdown` and `Html` each draw every new block; `Markdown` and `Html` draw every screen. The Ratatui bridge and a screen's terminal layout are the `tui` module's and are not written here.
2. Nothing in `yunta` uses `Screen`, `Ribbon` or `Overlay` before `app-core`. The only callers this module changes are the CLI's prompts (`Legend`) and the attached region (`Attention`), with the same output.
3. The proof is the goldens and the tests. No real run, and no tokens: the run path does not change. The region and the prompts print what they printed, as their existing goldens hold, and no engine code changes.
4. The spike's look is the target. Its screens (`.plans/yunta-tui/spike`, frames in `out/frames.json`) are what the new components draw, as far as Surco's rules allow. Where a rule disagrees with the spike, such as a glyph or a cut, the rule wins and the golden says so. The run's screen laid out on a terminal, written as a target before the layout moved, stays beside this module's targets as the `tui` module's reference.
5. `tasks/plan.md` and `tasks/todo.md` are replaced for this module. Those of `session-ends` stay in its commit, `717148cf`.

### Out of scope

- Ratatui, crossterm and any dependency of `yunta-surco` on them. `yunta-surco` gains no dependency.
- `yunta-app`, `yunta-tui`, bare `yunta`, `update` and messages, key handling and focus moving: the `app-core` and `tui` modules. A `Screen` says which region has the focus; nothing here moves it.
- The wordmark, the horizon and mouse capture.
- Laying a screen out on a terminal: the panes, their widths, what gives way, the overlay's box. That is the `tui` module's.
- Typography, sizes, spacing and radii as tokens a desktop reads. They live today only as the `Html` surface's CSS. That is the `desktop` module's, or a module of tokens before it.
- Moving the prompts' models (`card`, `menu`, `form`, `field`, `desk`) to `yunta-app`: the map gives that to `app-core`. Here they only build a `Legend` where they built strings.
- Retuning a color, or changing any existing block.

## Tech Stack

Rust at the version `rust-toolchain.toml` pins. `yunta-surco` as it is: its `line`, `glyphs`, `width`, `paint`, `palette`, `look`, `motion`, `doc` and `components` modules, its three surfaces, and its golden harness (`golden.rs`, `assert_golden`, `ENVIRONMENTS`, `RICH`, behind the `golden` feature). No new dependency in any crate.

## Commands

`cargo` means the wrapper that keeps this worktree's build target and state separate from other checkouts: `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.

```
# Before editing a file, and before finishing with no paths
cargo xtask docs --about <files>

# Build
cargo build -p yunta-surco
cargo build -p yunta

# Test
cargo test -p yunta-surco
cargo test -p yunta-surco catalog
cargo test -p yunta --test <a prompt's or the region's target>
cargo test --workspace --locked --no-fail-fast

# Write a new golden, then read its diff before keeping it
YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine

# Lint and checks
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo xtask smells --check
cargo xtask docs --check
cargo deny check
```

Before a commit, every step of the `ci` job in `.github/workflows/ci.yml` runs, one build at a time, with `CARGO_INCREMENTAL=0`. After the commit, `cargo clean`. `cargo xtask smells` never runs without `--check`. A bless only adds goldens: `git status` shows no existing golden changed, but the catalog's, which change only at their end.

## Project Structure

```
crates/surco/src/paint.rs                 the public style reading; `Paint`'s escapes written from it
crates/surco/src/palette.rs               unchanged values, read through `paint` (the module may stay private)
crates/surco/src/components/              ribbon, legend, attention, overlay and screen, as data
crates/surco/src/doc.rs                   `Block::Ribbon`, `Block::Legend`, `Block::Attention`
crates/surco/src/surface/terminal/        each new block
crates/surco/src/surface/markdown/        each new block, and a screen as a document
crates/surco/src/surface/html/            each new block, a legend's keys as buttons, a screen as a page
crates/surco/src/components/catalog.rs    every new block in every environment
crates/surco/goldens/                     the new goldens; no existing file changes
crates/cli/src/ask/                       the prompts build a `Legend` where they built strings
crates/cli/src/surface/view/mod.rs        the region's row built as an `Attention`
```

## Code Style

The crate's own: a block holds what is said and the role of each part, never a width, an escape or a glyph; types that admit only valid states; a `//!` for each new module, saying how it works and what it rejects; a `///` for each item, with a "Not …" sentence where one is confused with another; a numeric `const` with its reason.

```rust
/// The keys a person can press where they are, each with what it does, in
/// the order they are read. Not a `Next`, which says what to type at a
/// shell: a legend's keys act on the screen it stands under.
pub struct Legend(pub Vec<Binding>);

/// What is pressed and what it does: `enter` and `open`.
pub struct Binding {
    pub keys: Keys,
    pub does: String,
}
```

## Testing Strategy

| Level | What it holds |
|---|---|
| Unit, surco | The style reading for every tone, every own fill and line fill, and every layer, against what `Paint` writes: one table. The contrast floors, unchanged. Each new block with nothing to say draws nothing |
| Goldens, surco | The catalog draws each new block in the five environments, as Markdown and as HTML, and a screen has goldens as Markdown and as HTML. Each golden is written before the code that draws it is accepted, and read in its diff |
| Facts kept | In `ascii-80`, with no color, every word of each new block is still there: the ribbon's names and standings, every key and what it does, what needs a person and how to answer. Every word a screen holds is in its Markdown, its description and its HTML |
| No change | Every existing golden of `yunta-surco`, `yunta-render`, `yunta` and `yunta-engine` is byte for byte the same, but the catalog's, which grow only at their end; the CLI's prompt and region tests pass unchanged |

- A test asserts an outcome known apart from the code: a golden is read and approved, never written from the output and taken as is.
- A bless that changes an existing golden is a defect of this module, not a new golden.

## Boundaries

- **Always:** write the target goldens first and review them before the code that draws them; draw every new block on every surface; keep every word in the plainest stream; run `cargo xtask docs --about` before editing and before finishing.
- **Ask first:** a new dependency of any crate; changing an existing golden; a change to an existing block's data; anything the `tui` or `app-core` module owns.
- **Never:** a glyph outside `glyphs`, an escape outside `paint`, a state word outside `words`; a copy of a color; Ratatui in `yunta-surco`; a clock that moves the plow.

## Success Criteria

1. `surco::paint` exposes a style reading: bold, text color and background, as the sixteen or 24 bits, for any tone, own fill, line fill and layer. A test draws every combination through `Paint` and through that reading and finds the same escapes. No color value exists outside `palette`.
2. `Ribbon`, `Legend` and `Attention` are blocks, and `Overlay` and `Screen` compositions, in `yunta-surco`, each with its `//!` or `///`.
3. `Terminal`, `Markdown` and `Html` each draw every new block, and `Markdown`, the description and `Html` every screen. In `ascii-80` no word of a block is lost, and every word a screen holds is in its Markdown and its HTML.
4. A screen holds no cell, pixel or color: a main region and a side one, never a width, an overlay with no size, and nothing in Surco lays a screen out on a terminal. `Width::exact` draws every block of the catalog at every width from 20 to 59 with no line wider than it.
5. The catalog's new goldens exist for each new block in the five environments, as Markdown and as HTML, and a screen's as Markdown and as HTML. Each was reviewed before it was kept.
6. `Card` and `DeskFoot` take a `Legend`, the CLI's prompts build one, and the attached region builds its row as an `Attention`.
7. No existing golden changes, but the catalog's three, which grow only by a part appended at their end. The workspace suite passes. Its count is explained against 270 binaries and 4,504 tests.
8. `yunta-surco` gains no dependency, and `cargo deny check` passes as before.

## Open Questions

Answered on approval, each with the recommended choice:

1. **Where the screen is laid out.** First answered with Surco laying the screen out into a frame of cells. Changed on 2026-11-06 at the user's choice, after reading the approved analysis again and weighing a GPUI desktop: each interface lays a screen out its own way (decision 1).
2. **The prompts' keys now or with `app-core`.** Recommended: `Card.keys` and `DeskFoot.keys` become a `Legend` in this module, with the same output, so keys are said one way from the start. The other choice is to add `Legend` beside the strings and migrate in `app-core`, which leaves two ways of saying keys for a while.
3. **The plow.** Recommended: the ribbon carries the running node's count of events and the terminal draws the plow from it, only with motion and color, as the spike does. The other choice is to leave the plow to `yunta-tui`, which would have to know the ribbon's cells.
4. **Proof without a real run.** Recommended: the goldens and tests above. The region and the prompts are the only callers that change, and their existing goldens hold their output. Confirm that no real run before and after is wanted.
