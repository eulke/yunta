# Implementation Plan: `surco-screen`

Spec: `SPEC-surco-screen.md`, approved on 2026-11-06, its four open questions answered with the recommended choice. Status: **approved on 2026-11-06**, with the catalog's goldens growing at their end (decision 6). The layout was revised the same day: each interface lays a screen out its own way, at the user's choice (decisions 3 and 4). Task list: `tasks/todo.md`.
Base: `feature/yunta-tui` at `717148cf`, on `main` at `34790ab8`, which had not moved when this was written.

## Overview

`yunta-surco` gains three things:
- **A public style reading** that `Paint`, the clap styles and the HTML page all read.
- **What an open interface needs, as data:** `Ribbon`, `Legend` and `Attention` as blocks, and `Screen` and `Overlay` as compositions of documents.
  - Every surface draws each block, and Markdown and HTML draw each screen.
  - A screen holds no cell, pixel or color. The terminal (`tui` module), a GPUI desktop and a page each lay it out their own way, and Surco's tokens give them one look.
- **A width of exactly the cells a pane is given,** so another engine's panes can draw Surco's documents.

Then `Card`, `DeskFoot` and the attached region are rebuilt on `Legend` and `Attention`, and print what they printed. The look is the spike's, as Surco's rules allow. Target goldens are written and reviewed before the code that draws them.

## Architecture Decisions

**1. The style reading, in `paint/style.rs`.**
- `paint.rs` holds 459 lines, so the reading goes in a file of its own, re-exported by `paint`.
- One function answers every layer. `Paint::Plain` has no style. The sixteen colors keep their bold, dim and color of `ansi`, and fill nothing, as today. The 24 bits keep the tone, fill and pill rules of `truecolor_parameters`.

```rust
/// How a span is drawn: what an escape, a cell of another engine or a page's
/// style sets. Not a `Tone`, which is the span's role; this is what that role
/// looks like in one layer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub dim: bool,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
}

/// A color as a layer has it: one of the terminal's sixteen, or 24 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Ansi(Ansi),
    Rgb(u8, u8, u8),
}

/// The style of a span in `tone` on its own fill or else its line's, in `layer`.
pub fn style(layer: Layer, tone: Tone, own: Option<Fill>, line: Option<Fill>) -> Style;

/// The background a filled line is erased with to its edge, in `layer`.
pub fn erase(layer: Layer, line: Option<Fill>) -> Option<Color>;

/// The colors only a page has on `ground`, each with the name a page's style reads it by.
pub fn page(ground: Ground) -> [(&'static str, Color); 5];
```

- `Ansi` becomes public: today's private enum and its codes.
- `Paint::word`, `Paint::span` and `Paint::erase` write their SGR from `style` and `erase`.
- `paint/help.rs`, the clap styles, reads `style`.
- `surface/html/style.rs` reads `style` and `page`.
- `palette` stays private, and its values and contrast tests stay as they are. No color value exists outside it.

**2. The blocks, as data in `components/`.** One file each, with the crate's rule: what is said and its role, never a width, an escape or a glyph. Each is built the way a caller reads it.

```rust
/// The furrow with the names of the run's nodes over it: the signature at
/// the top of a run. Not a `Furrow`, which hangs under a page's headline and
/// names no node.
pub struct Ribbon {
    pub stretches: Vec<Stretch>,
    /// How many events the running node has had: the plow stands a cell
    /// further for each. `None` where the view stands still: no plow.
    pub events: Option<u64>,
}
/// One node's stretch of the furrow: its name and the word it stands at.
pub struct Stretch {
    pub name: String,
    pub standing: Standing,
}

/// The keys a person can press where they are, each with what it does, in
/// the order they are read. Not a `Next`, which says what to type at a shell.
pub struct Legend(pub Vec<Binding>);
/// What is pressed and what it does: `enter` and `open`.
pub struct Binding {
    pub keys: Keys,
    pub does: String,
}
/// What is pressed, as the person reads it: the arrows, which each glyph set
/// draws its own way, or keys spelled as they are typed (`enter`, `esc`, `n`).
pub enum Keys {
    UpDown,
    Typed(String),
}

/// What needs a person, and how they answer it, on any screen. Not a
/// `Concern`, which warns: this asks.
pub struct Attention {
    /// What needs them, when a screen shows more than one: a run's handle.
    pub subject: Option<String>,
    /// What is asked of them.
    pub asks: String,
    pub answer: Answer,
}
/// How a person answers what needs them.
pub enum Answer {
    /// Typed at a shell: said before what is asked, so a row cut short keeps it.
    Command(String),
    /// Pressed here: said after what is asked, which gives way to it.
    Key(Binding),
    /// Given where it was asked, at another terminal: nothing here answers
    /// it, so only what is asked is said.
    Elsewhere,
}
```

- **Built as they read:**
  - `Legend::new().arrows("move").key("enter", "chooses")`;
  - `Attention::command(asks, command)`, `Attention::key(asks, keys, does)` and `.of(subject)`;
  - `Ribbon::new(stretches)` and `.plowed(events)`.
- **`Block` gains `Ribbon`, `Legend` and `Attention`,** each with `From`. A ribbon hangs from a headline, as the furrow does.
- **One with nothing to say draws nothing:** a legend with no binding, a ribbon with no stretch, an attention that asks nothing.

**3. The screen, as data in `components/screen.rs`.** Its parts are named by what they are, never by where they stand, and its types admit only valid states: the main region has the focus unless the side one says it has.

```rust
/// What a whole interface shows, with no cell, no pixel and no color: each
/// interface lays it out its own way. Not a `Doc`: a screen holds documents.
pub struct Screen {
    /// Where the person is, from the outermost: `yunta`, a run, a node.
    pub trail: Vec<String>,
    /// How many runs stand at each word that asks for a look.
    pub tally: Vec<Count>,
    /// What the screen is about, across it: a run's headline, ribbon and gist.
    pub about: Option<Doc>,
    pub main: Region,
    /// A list beside the main region to move through, the first to give way.
    pub side: Option<Side>,
    pub attention: Option<Attention>,
    pub legend: Legend,
    /// Open over the regions; its legend stands for the screen's while open.
    pub overlay: Option<Overlay>,
}
/// How many runs stand at one word: `13 need you`.
pub struct Count {
    pub standing: Standing,
    pub runs: usize,
}
pub struct Region {
    pub title: Option<String>,
    pub doc: Doc,
}
pub struct Side {
    pub region: Region,
    /// Whether the keys act on it rather than on the main region.
    pub focused: bool,
}
/// What opens over a screen and closes with `esc`: the palette, help, a confirmation.
pub struct Overlay {
    pub title: String,
    pub doc: Doc,
    pub legend: Legend,
}
```

- **Built as it reads:** `Screen::new(main)`, `.trail(...)`, `.tally(Standing::NeedsYou, 13)`, `.about(doc)`, `.side(region)`, `.focus_side()`, `.attention(...)`, `.legend(...)` and `.overlay(...)`. Also `Region::new(doc).titled("plan")`.
- **What a screen carries:** no scroll position, no width and no size. Each interface keeps where a person scrolled to and how wide a pane stands.

**4. Each interface lays a screen out; Surco gives a pane its exact width.**
- **Surco lays no screen out on a terminal.**
- **The `tui` module does:**
  - Ratatui's `Layout`;
  - each region's document drawn by `Terminal::lines` at the region's width;
  - the overlay's box in Surco's `corners(Shape::Box)` and joints;
  - the attention row on the tint the style reading gives.
- **The targets drawn for the terminal at 80×24 and 60×14** stay as that module's reference.
- **A GPUI desktop** lays it out with flexbox, and a page with CSS grid.
- **What a pane needs from Surco: `Width::exact(cells)`.** `Width::of` holds a stream between 60 and 120 cells, which a pane of 22 cannot use.
  - `Width::whole`, the desk's, already had no floor and no ceiling. It becomes `Width::exact`, since a pane's own edge bounds it as the desk's does.
  - **A pane's edge cuts what passes it,** where a stream's terminal wraps it. Measured while implementing: twelve existing blocks passed an exact width below 60. The widest was the decision at 53 cells, which is the main region's width at 80 columns.
  - At the user's choice, the terminal surface lays each line past a pane's edge out in rows (`Line::rows_within`, read from `Width::cuts`). A row breaks at its last space, or else at the edge, and each further row hangs under where the line's text starts.
  - A stream keeps its lines as they are, so a word its terminal wraps is still copied whole.
  - A test draws every block of the catalog at exact widths from 20 to 59, and finds no line wider than its width and no panic.

**5. Markdown, the description and HTML.**
- **Markdown and the description:**
  - a ribbon is a line of each name with its word, in order;
  - a legend is a line of `` `keys` does `` pairs;
  - an attention is a line with its words.
  - A screen is its trail and tally, what it is about, the side region and then the main one, each as a section under its title, then its attention and legend. An overlay is a section after them, under its title.
- **HTML:**
  - a ribbon is a row of names over the furrow's strokes, with every word in its label;
  - a legend is a row of `<button type="button" class="key" data-key="…">`, inert on a static page;
  - an attention is a tinted `<p role="status">`.
  - A screen is a `<header>` with the trail and the tally, a `<div class="about">`, a `<main>` with a `<section class="region side">` and a `<section class="region main">`, the focused one with `aria-current`, a `<footer>` with the attention and the legend, and an overlay as a `<dialog open>`.
- **`every_surface_says_every_word_a_screen_holds`** holds both: each surface's words include every string the screen holds, read from its data.

**6. Goldens, and the catalog.**
- **A deviation from the spec.** `every_kind_of_block_stands_in_the_catalog` asks the catalog for a sample of every kind of block. So the three new blocks enter the catalog, and its goldens (`components/catalog.*.txt`, `markdown/catalog.md`, `html/catalog.html`) grow.
  - The new samples go in a part appended after `closing()`. Each of those files then changes only by what is added at its end, or before the page's closing tags.
  - Every other existing golden stays byte for byte. That is the one place where the spec's "no existing golden changes" is narrowed (approved).
  - Rejected: a second catalog for the new blocks. It would split the one reference Surco keeps and make a reader look in two places.
- **A screen's goldens:** `markdown/screen.md` and `html/screen.html`, the run's screen with the help overlay open.
- **Targets first.** The blocks' targets are drawn by hand in `ascii-80` and `unicode-60`, and the screen's as Markdown and HTML. They follow the spike's frames as Surco's rules allow, and the user reviews them before any code draws them. The other environments are blessed once the code draws the targets, and read in their diff.

**7. The callers move without a change to what they print.**
- `Card.keys` and `DeskFoot.keys` become `Legend`.
- `crates/cli/src/ask/{card,desk,menu,form}.rs` build one with `Legend::new().arrows("move").key("enter", "chooses")…` where they built strings.
- `crates/cli/src/surface/view/mod.rs` builds its row as `Attention::command(…)` and draws it through the terminal surface.
- Its row for a run whose engine asks at its own terminal is `Attention::elsewhere(…)`, at the user's choice: the words stay, and what it asks goes from strong to plain.
- Their tests and the goldens of `yunta-surco` (card, desk), `yunta` (`crates/cli/goldens`), `yunta-render` and `yunta-engine` stay byte for byte.

**8. What is said, and where.**
- Each new module gets a `//!`. `components/mod.rs`, `doc.rs`, `surface/mod.rs`, `paint.rs` and the crate root say what is true.
- The rule "a color has one table" is held by the test of decision 1. The `//!` of `paint/style.rs` says so.
- `.agents/architecture.md` stands at 6,045 of its 6,144 bytes, so the rule goes in no line of it: a test holds it.
- No user document changes, because nothing a person runs looks different.
- `CAPABILITY-MAP.md`'s `tui` row says what it now owns: a screen laid out with Ratatui's `Layout`, and the overlay's box in Surco's glyphs.

## Dependency Order

```
1 the style reading ── Paint, clap, HTML read it
        │
2 target goldens, reviewed
        │
3 Ribbon ─ 4 Legend ─ 5 Attention          (blocks, each on every surface, in the catalog)
        │         │          │
        │   7 Card, DeskFoot, prompts
        │                    8 the region
        └── 6 Screen, Overlay, Width::exact, as Markdown and HTML
                     │
                9 docs and the map ─ 10 gate ─ 11 commit
```

The style reading depends on nothing. The blocks need the targets, and the screen needs the three blocks. The migrations need only their block. The docs come last, once what they describe is true.

## Phases

### Phase 1: the one table
- Task: the style reading, and `Paint`, the clap styles and the HTML style read it.

### Checkpoint A
- Every existing golden of the workspace is byte for byte the same. The new test holds one table across every tone, fill and layer.

### Phase 2: the targets
- Task: target goldens for the three blocks in `ascii-80` and `unicode-60`, and the screen as Markdown and HTML. Drawn by hand from the spike's frames. The terminal's screens stay as the `tui` module's reference.

### Checkpoint B
- The user reviews the targets. No drawing code until they are approved.

### Phase 3: the blocks
- Task: `Ribbon` on every surface and in the catalog.
- Task: `Legend` on every surface and in the catalog.
- Task: `Attention` on every surface and in the catalog.

### Checkpoint C
- The catalog's goldens grow only by the appended part, and every other golden is unchanged.
- In `ascii-80` no word is lost.

### Phase 4: the screen
- Task: `Screen`, `Region`, `Side` and `Overlay` as data; `Width::exact`; a screen in Markdown, the description and HTML.

### Checkpoint D
- The screen's targets match, and every block draws within an exact width below the floor.

### Phase 5: the callers
- Task: `Card` and `DeskFoot` on `Legend`, and the CLI's prompts building it.
- Task: the region's row as an `Attention`.

### Checkpoint E
- The CLI's prompt and region tests pass unchanged, and every golden outside the catalog is byte for byte the same.

### Phase 6: what it says, proof and close
- Task: the `//!` and `///` this module makes false, the map's `tui` row, and the docs checker.
- Task: every step of the `ci` job, with the suite's count explained against 270 binaries and 4,504 tests.
- Task: commit the module.

### Checkpoint F
- The user reviews the whole diff. No commit until approved.

## Verification

- **Each task:**
  - `cargo test -p yunta-surco`, and the target of each caller it touches;
  - `cargo xtask docs --about` on its files;
  - `cargo xtask smells --check`.
- **After each bless:** `git status crates/*/goldens` shows new files only, and the catalog files change only at their end.
- **Before the commit:**
  - every step of the `ci` job, one build at a time, with `CARGO_INCREMENTAL=0`, through `sh .plans/yunta-tui/tools/gate.sh`;
  - then `cargo clean` after the commit.
- **No real run.** The run path does not change (the spec's assumption 3).

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| A refactor of `Paint` changes an escape | High | Checkpoint A: every existing color golden (the catalog in `truecolor-dark-100` and `truecolor-light-100`, and each crate's) compares byte for byte before any new code |
| A card's or a desk's keys print differently once they are a `Legend` | High | Their existing tests and goldens, unchanged, before and after; a legend drawn inside them keeps today's muted join |
| A block drawn below the floor of 60 cells breaks a line or panics | Medium | `Width::exact`'s test draws every block of the catalog at every width from 20 to 59 |
| A screen's data leans on one interface | Medium | It holds no width, size or scroll, and HTML draws every screen: the shape is held by a page as well as by a terminal |
| The spike's look against a Surco rule, such as a glyph outside `glyphs` | Low | The rule wins and the target golden says so; the user sees it at Checkpoint B |
| The catalog's goldens grow | Low | The change is only an appended part, and the spec is updated to say so (decision 6) |

## Parallelization

None: one person, and the blocks share the catalog and the surfaces' dispatch.

## Open Questions

Answered on approval: the catalog's goldens grow by a part appended at their end.

1. **The catalog's goldens grow** (decision 6). The new blocks join the catalog, so its three golden files gain an appended part. Every other golden stays byte for byte. The other choice, a second catalog, keeps those files as they are and splits the reference. The recommendation is to append.
