# Tasks: `surco-screen`

Spec: `SPEC-surco-screen.md`. Plan: `tasks/plan.md`. Status: **approved on 2026-11-06, in implementation**.

**How these run:** one at a time, with `incremental-implementation` and `test-driven-development`.
- Each task names the test that fails first and why. That test is seen failing for that cause before the code that makes it pass. A test that may pass at once says so, and says what it guards.
- Every task leaves the tree compiling and its tests green.
- `cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.
- Before a file is edited, `cargo xtask docs --about <files>` runs, and what it prints is kept true.
- A golden is blessed only with `YUNTA_BLESS=1`, and its diff is read before it is kept.
- No commit until the user approves at Checkpoint F.

**Three refinements of the plan:**
1. **The targets are drawn by hand in the two plain environments, `ascii-80` and `unicode-60`.** A hand-written escape is a guess, and these two read as the text they are. The colored environments are blessed once the code draws the targets, and read in their diff (plan decision 6 named `unicode-color-100`).
2. **The targets wait outside git** (`.plans/yunta-tui/proof/surco-screen/targets/`). Each moves into its golden in the task whose code draws it. Written straight into the catalog's goldens, they would turn its test red for three tasks.
3. **A task that adds a kind of block touches more than five files.** Each surface's dispatch, the catalog's list of kinds and its goldens all gain one arm or one part. The task is still one thing: one block, drawn everywhere.

## Phase 1: the one table

### Task 1: the style reading

**Description:**
- `crates/surco/src/paint/style.rs` holds `Style`, `Color`, `Ansi` (public now), `style`, `erase` and `page`, re-exported by `paint`.
- `Paint::word`, `Paint::span` and `Paint::erase` write their SGR from `style` and `erase` alone.
- The clap styles (`paint/help.rs`) and the page's style (`surface/html/style.rs`) read `style` and `page`.
- `palette` stays private and unchanged.

**Fails first:** `every_span_paints_as_its_style_reads`, in `paint/style/tests.rs`. It does not compile: there is no `style` yet. Once `style` exists, it guards the one table, with every tone, every own fill and line fill, and every layer drawn through `Paint` and through `style`. What holds the refactor is every existing color golden, unchanged.

**Acceptance criteria:**
- [x] `style`, `erase` and `page` answer every layer, and `Paint`, the clap styles and the page's style read nothing else from `palette`.
- [x] `every_span_paints_as_its_style_reads` passes for the sixteen colors and both grounds of 24 bits.
- [x] Every existing golden of the workspace and the palette's contrast tests pass unchanged.

**Verification:**
- [x] `cargo test -p yunta-surco`, `cargo test -p yunta-render`, `cargo test -p yunta --bins`
- [x] Manual check: no RGB value outside `palette.rs`, with `git grep -n "Rgb(" -- crates/surco/src`.

**Dependencies:** None

**Files likely touched:** `crates/surco/src/paint.rs`, `crates/surco/src/paint/style.rs`, `crates/surco/src/paint/style/tests.rs`, `crates/surco/src/paint/help.rs`, `crates/surco/src/surface/html/style.rs`

**Estimated scope:** M

### Checkpoint A: after Task 1
- [x] Every existing golden is byte for byte the same, `cargo xtask smells --check` passes, and the user sees the reading's signature.

## Phase 2: the targets

### Task 2: target goldens, drawn by hand

**Description:**
- In `.plans/yunta-tui/proof/surco-screen/targets/`, write by hand what each new block and screen should draw, from the spike's frames (`.plans/yunta-tui/out/frames.json`, `spike/src/view.rs`) as Surco's rules allow:
  - **Blocks**, as the catalog's appended part, in `ascii-80` and `unicode-60`: a ribbon of seven nodes, one running with its plow; a legend of five keys, the arrows among them; an attention answered by a command and one answered by a key.
  - **Two screens laid out on a terminal**: a run's, and the same run with an overlay open over it, each at 80×24 in `ascii-80` and 60×14 in `unicode-60`. Since the redo below they are the `tui` module's reference, not this module's targets.
  - **The run's screen** as Markdown and as HTML.
- Beside them, a `NOTES.md` names each place where a rule of Surco's made the target differ from the spike, and the tone each span is meant to have.

**Fails first:** nothing runs: this task writes the targets that Tasks 3 to 6 fail against.

**Redone on 2026-11-06, after the layout moved** (each interface lays a screen out its own way):
- The Markdown and HTML targets follow the screen's new parts: trail, tally, about, a side region and a main one.
- The help overlay is open in both.
- The two terminal screens move to `targets/tui-reference/` as the `tui` module's reference, with the notes about them.

**Acceptance criteria:**
- [x] Every target exists, every line no wider than its columns, every frame exactly its rows.
- [x] `NOTES.md` names each departure from the spike and the tone of each span.

**Verification:**
- [x] Manual check: a script measures each target's rows and widths. The user reads the targets at Checkpoint B.

**Dependencies:** Checkpoint A

**Files likely touched:** none in git.

**Estimated scope:** S

### Checkpoint B: after Task 2
- [x] The user approves the targets. No code draws a new block before this.

## Phase 3: the blocks

### Task 3: the ribbon

**Description:**
- `components/ribbon.rs` holds `Ribbon` and `Stretch`, built as `Ribbon::new(stretches)` and `.plowed(events)`, and `Block::Ribbon` exists.
- The terminal draws names over strokes, each stretch as wide as its name and at least four cells, a gap between two. The plow is the running node's cell at `events % cells`, strong, only when `events` is set. Where the names do not fit, it is the page's furrow.
- A ribbon hangs from a headline, as the furrow does.
- Markdown and the description draw each name with its word. HTML draws the names over the furrow's strokes, every word in its label.
- The catalog gains its sample in a part appended after `closing()`, and the ribbon's targets move into the catalog's goldens.

**Fails first:**
- `every_kind_of_block_stands_in_the_catalog` fails once `ribbon` is a kind with no sample.
- Then `every_block_matches_its_goldens_in_every_environment`, against the targets, until the terminal draws them.
- Then `markdown_says_every_word_the_terminal_says_of_every_block`, until Markdown and HTML say the names.

**Acceptance criteria:**
- [x] The ribbon's catalog part matches its targets in `ascii-80` and `unicode-60`; the other environments, Markdown and HTML are blessed and read.
- [x] With no `events`, no cell is strong; a ribbon with no stretch draws nothing; in `ascii-80` every name is there.
- [x] Each catalog golden changes only by the part appended at its end.

**Verification:**
- [x] `cargo test -p yunta-surco`
- [x] Manual check: the catalog goldens' diff shows only lines added at the end.

**Dependencies:** Checkpoint B

**Files likely touched:** `crates/surco/src/components/{ribbon,mod}.rs`, `crates/surco/src/doc.rs`, `crates/surco/src/surface/terminal/{mod,ribbon}.rs`, `crates/surco/src/surface/markdown/mod.rs`, `crates/surco/src/surface/html/blocks.rs`, `crates/surco/src/components/catalog{,/kinds}.rs`, `crates/surco/goldens/{components,markdown,html}/catalog.*`

**Estimated scope:** M (one arm in each of the files above)

**Found while implementing:**
- The catalog's page grows in one more place than its end: its contents (`<nav class="toc">`) gain a link to the new heading `on a screen`, since the page lists every heading. It is an addition, and nothing that was there changes.
- `markdown/mod.rs` passed 500 lines, so a screen's blocks are written in `markdown/screen.rs`.
- In HTML two stretches are kept apart by a space, so their names read as words; a flex row draws none of it.

### Task 4: the legend

**Description:**
- `components/legend.rs` holds `Legend`, `Binding` and `Keys`, built as `Legend::new().arrows("move").key("enter", "open")`, and `Block::Legend` exists.
- The terminal draws keys in strong, what they do in muted and a faint separator, with `Keys::UpDown` in the glyph set's own arrows.
- Markdown and the description draw a line of `` `keys` does `` pairs. HTML draws a `<button type="button" class="key" data-key="…">` for each binding.
- The catalog's appended part gains its sample, and its targets move in.

**Fails first:**
- `every_kind_of_block_stands_in_the_catalog`, once `legend` is a kind with no sample.
- Then the catalog's goldens against the targets.
- Then `a_legend_draws_each_key_as_a_button_with_its_key`, in the HTML surface's tests, until the buttons carry `data-key`.

**Acceptance criteria:**
- [x] The legend's part matches its targets, and the rest is blessed and read.
- [x] An empty legend draws nothing; `UpDown` reads `↑↓` in Unicode and `up/down` in ASCII.
- [x] Each button carries its key and is `type="button"`.

**Verification:**
- [x] `cargo test -p yunta-surco`

**Dependencies:** Task 3

**Files likely touched:** `crates/surco/src/components/{legend,mod}.rs`, `crates/surco/src/doc.rs`, `crates/surco/src/surface/terminal/{mod,legend}.rs`, `crates/surco/src/surface/markdown/mod.rs`, `crates/surco/src/surface/html/blocks.rs`, `crates/surco/src/components/catalog{,/kinds}.rs`, the catalog's goldens

**Estimated scope:** M (one arm in each)

**Found while implementing:**
- The screen's blocks are drawn as a page in `html/screen.rs`, the twin of `markdown/screen.rs`, and the ribbon moved there with the legend.
- A key's button takes the page's monospaced face from the list on the stylesheet's first line, where every code-like part takes it, rather than from a variable of its own.

### Task 5: the attention

**Description:**
- `components/attention.rs` holds `Attention` and `Answer`, built as `Attention::command(asks, command)`, `Attention::key(asks, keys, does)` and `.of(subject)`, and `Block::Attention` exists.
- The terminal draws a command before what is asked, as the region's row reads today, so a row cut short keeps it. It draws a key after what is asked, which is cut to make room. The block carries no fill.
- Markdown and the description draw its words in a line. HTML draws a tinted `<p role="status">`.
- The catalog's appended part gains both samples, and their targets move in.

**Fails first:**
- `every_kind_of_block_stands_in_the_catalog`, once `attention` is a kind with no sample.
- Then the catalog's goldens against the targets.
- Then `an_attention_cut_short_keeps_its_answer`, until a narrow row keeps the command, and the key at its end.

**Acceptance criteria:**
- [x] Both samples match their targets, and the rest is blessed and read.
- [x] A row cut to fit keeps its answer; in `ascii-80` every word is there.
- [x] The line `Attention::command` draws is the region's line at the base, character for character, for the same command and question.

**Verification:**
- [x] `cargo test -p yunta-surco`

**Dependencies:** Task 4

**Files likely touched:** `crates/surco/src/components/{attention,mod}.rs`, `crates/surco/src/doc.rs`, `crates/surco/src/surface/terminal/{mod,attention}.rs`, `crates/surco/src/surface/markdown/mod.rs`, `crates/surco/src/surface/html/blocks.rs`, `crates/surco/src/components/catalog{,/kinds}.rs`, the catalog's goldens

**Estimated scope:** M (one arm in each)

**Found while implementing:**
- The kind and its two samples came in one step, so `every_kind_of_block_stands_in_the_catalog` was never seen red for `attention`. The reds seen were the catalog's goldens against the targets and `an_attention_cut_short_keeps_its_answer`, while the block drew nothing.
- At 60 cells the samples' questions are cut (`the plan wa…`, `baseli…`). Their whole words stand in `ascii-80`, whose 80 cells hold every word.

### Checkpoint C: after Tasks 3-5
- [x] The catalog's goldens grow only by the appended part, every other golden is unchanged, and no word is lost in `ascii-80`. `cargo xtask smells --check` passes.

## Phase 4: the screen

### Task 6: the screen as data, a pane's width, and the screen as a document and a page

**Description:**
- `components/screen.rs` holds `Screen`, `Region`, `Side` and `Overlay`, named by what they are and built as `Screen::new(main)` with `.trail`, `.tally`, `.about`, `.side`, `.focus_side`, `.attention`, `.legend` and `.overlay`. There are no widths, sizes or scroll positions.
- `Width::exact(cells)` gives a pane exactly its cells, with no floor and no ceiling.
- `Markdown::screen`, `Description::screen` and `Html::screen` draw a screen as plan decision 5 says. The two targets move into `goldens/markdown/screen.md` and `goldens/html/screen.html`.

**Fails first:**
- `every_block_draws_within_an_exact_width`, which draws every block of the catalog at every width from 20 to 59, because there is no `Width::exact`.
- Then `every_surface_says_every_word_a_screen_holds`, because no surface draws a screen.
- Then the two goldens against their targets.

**Acceptance criteria:**
- [x] No line of any catalog block is wider than an exact width from 20 to 59, and none panics.
- [x] Markdown, the description and HTML each say every string the screen holds; the focused region carries `aria-current` in HTML.
- [x] Both goldens match their targets.

**Verification:**
- [x] `cargo test -p yunta-surco screen`, `cargo test -p yunta-surco width`

**Dependencies:** Checkpoint C

**Files likely touched:** `crates/surco/src/components/{screen,mod}.rs`, `crates/surco/src/width.rs` (or where `Width` lives, `look.rs`), `crates/surco/src/surface/markdown/{mod,screen}.rs`, `crates/surco/src/surface/html/{mod,screen}.rs`, the screen's tests and goldens

**Estimated scope:** M

**Found while implementing:**
- `Width::whole`, which the desk lays its columns at, already had the body `Width::exact` needed. It is renamed, with its three callers: the CLI's desk, its tests and the render crate's desk test.
- The exact-width test was red for more than the missing constructor: twelve existing blocks passed an exact width below 60, none panicking. The widest was the decision at 53 cells, the main region's width at 80 columns.
  - At the user's choice, a width now says whether its edge cuts (`Width::cuts`, true only for `Width::exact`). The terminal surface lays each line past a cutting edge out in rows with `Line::rows_within`.
  - A stream keeps its lines as they are. No golden of `yunta-surco`, `yunta-render` or the CLI's units changed.
- `tally` holds a `Count { standing, runs }` rather than a pair, so a count reads by name and says itself (`13 need you`, `1 needs you`). `Screen::main_focused` answers which region the keys act on.
- The HTML target took the ribbon's markup as the catalog draws it, and ends where `Html::body` ends, with no newline. Both goldens equal their targets byte for byte.
- `page.css` lays a screen out in a grid, the side region giving way first on a narrow page. No whole page draws a screen yet.

### Checkpoint D: after Task 6
- [x] The screen's targets match, every block draws within an exact width below the floor, and the user sees the screen's goldens.

## Phase 5: the callers

### Task 7: the card and the desk on a legend

**Description:**
- `Card.keys` and `DeskFoot.keys` become `Legend`, and the terminal draws each inside them as it drew its strings: `<keys> <does>` in muted, joined by the separator, giving way where it gave way.
- `crates/cli/src/ask/{card,desk,menu,form}.rs` build one with `Legend::new().arrows(…).key(…)` where they built strings.

**Fails first:** none can: a refactor, with no new behavior. What guards it is every test of the card and the desk in `yunta-surco`, every prompt test of `yunta`, and their goldens, all passing unchanged. Added: `a_cards_legend_reads_as_its_keys_read`, with the expected foot written out (`↑↓ move · enter chooses · …`). It passes once written, and pins the join the strings had.

**Acceptance criteria:**
- [x] No `keys: Vec<String>` is left in `Card` or `DeskFoot`.
- [x] Every card, desk and prompt test, and every golden of `yunta-surco` and `yunta` outside the catalog, passes unchanged.

**Verification:**
- [x] `cargo test -p yunta-surco`, `cargo test -p yunta --bins`, `cargo test -p yunta --test <each prompt's target>`

**Dependencies:** Checkpoint D

**Files likely touched:** `crates/surco/src/components/{card,desk}.rs`, `crates/surco/src/surface/terminal/{card,desk}.rs` and their tests, `crates/cli/src/ask/{card,desk,menu,form}.rs`

**Estimated scope:** M (mechanical in the prompts)

**Found while implementing:**
- The field is `legend`, named by what it holds: `Card.legend` and `DeskFoot.legend`.
- `2 more options on the card` is no key. It stands in `FootRow::Options { named, more }`, and the desk says it after the keys, where the strings had it.
- `type to filter` is `Keys::Typing`, built with `Legend::typing("to filter")`: whatever the person types, spelled `type`, rather than a key named `type`.
- `Legend::spelled(glyphs)` is the one-tone run a card's, a desk's and a list's foot say.
- `Escape::does` says what Escape does beside `esc`, and `Escape::said` is built from it. A prompt no longer needs a look to say its keys.
- `a_cards_legend_reads_as_its_keys_read` passed against the strings before the change, and after it.

### Task 8: the region's row as an attention

**Description:** `crates/cli/src/surface/view/mod.rs` builds its row as `Attention::command(…)` and draws it through the terminal surface. Its comment about the command coming first moves to the block, where the rule now lives.

**Fails first:** none can: the same line, drawn by Surco. What guards it is the region's and the view's tests and the CLI's goldens, unchanged.

**Acceptance criteria:**
- [x] `view::attention` builds an `Attention` and writes no span of its own.
- [x] Every test and golden of `yunta` passes unchanged.

**Verification:**
- [x] `cargo test -p yunta --bins`, `cargo test -p yunta --test run_flow`, `cargo test -p yunta --test status_follow`

**Dependencies:** Task 7

**Files likely touched:** `crates/cli/src/surface/view/mod.rs`, its tests

**Estimated scope:** S

**Found while implementing:**
- `view::attention` takes the region's look, so Surco cuts the row at the width the region's `fit` cut it at before. The test calls pass an ASCII look 120 cells wide where they passed the glyph set.
- `view::asked`, the row of a run whose engine asks at its own terminal, stays a line of its own. It has no answer a person types or presses here (the answer is given at that terminal), and it says whom it asks in strong, where an attention's question is plain. As an `Attention`, it needs an answer given elsewhere and changes a tone the region paints. Left for the user to place.

### Checkpoint E: after Tasks 7-8
- [x] Every golden outside the catalog is byte for byte the same, across the workspace. `cargo xtask smells --check` passes.

### Task 8b: the row of a run asked at its own terminal, as an attention

Added at Checkpoint E, at the user's choice.

**Description:**
- `Answer` gains `Elsewhere`, built with `Attention::elsewhere(asks)`: an attention answered at another terminal, so only what is asked is said.
- The terminal draws the mark, `needs you`, the subject when there is one, and what is asked, plain, cut at the row's end. Markdown, the description and HTML say the same words.
- `view::asked` builds one and draws it at the region's look, as `view::attention` does.
- The catalog's appended part gains a third attention, `node `implement` asks at its terminal`, its targets written first.

**Fails first:**
- The catalog's goldens against the new targets, while `Elsewhere` draws nothing.
- Then `an_attention_answered_elsewhere_says_what_is_asked`, until the row is the mark, `needs you` and what is asked, cut at its end.

**Acceptance criteria:**
- [x] The third sample matches its targets in `ascii-80` and `unicode-60`, and the rest is blessed and read.
- [x] `view::asked` builds an `Attention` and writes no span of its own; the region's words are the same.

**Verification:**
- [x] `cargo test -p yunta-surco`, `cargo test -p yunta --bins`, `cargo test -p yunta --test status_follow`

**Dependencies:** Checkpoint E

**Files likely touched:** `crates/surco/src/components/attention.rs`, `crates/surco/src/surface/{terminal/attention,markdown/screen,html/screen}.rs` and their tests, `crates/surco/src/components/catalog/on_screen.rs`, the catalog's goldens, `crates/cli/src/surface/{view/mod,region}.rs`

**Estimated scope:** S

**Found while implementing:**
- The page set an attention's question with `esc`, so backticks stood as they were written. It reads it with `words`, as Markdown does, so `` `implement` `` is code on both. No earlier sample had a backtick, and no other golden moved.
- No test drew `view::asked`. `the_row_of_a_run_asked_at_its_terminal_says_whom_it_asks` pins its words. It passes on the strong row as on the plain one, since only the tone changed.

## Phase 6: what it says, proof and close

### Task 9: the `//!` and `///` this module makes true, and the map's `tui` row

**Description:**
- Each new module's `//!` says how it works and what it rejects.
- These say what is true:
  - `components/mod.rs`;
  - `doc.rs`'s `Block`;
  - `surface/mod.rs`: every surface draws every block, and Markdown and HTML every screen, which each interface lays out its own way;
  - `paint.rs` and the crate root's map: the style reading, the screen, a pane's width.
- `paint/style.rs`'s `//!` says that a color has one table, and names the test that holds it.
- `CAPABILITY-MAP.md`'s `tui` row says what it now owns: a screen laid out with Ratatui's `Layout`, and the overlay's box in Surco's glyphs.

**Fails first:** `cargo xtask docs --check`, should a moved term leave a link that names nothing. If it passes at once, it guards what the next change breaks.

**Acceptance criteria:**
- [x] `cargo xtask docs --check` passes, and `cargo xtask docs --about` on the module's diff prints nothing left false.
- [x] No document, code comment or test cites a document, the spec or the plan.

**Verification:**
- [x] `cargo xtask docs --check`, `cargo xtask smells --check`

**Dependencies:** Tasks 1-8

**Files likely touched:** the `//!` of the files above, `CAPABILITY-MAP.md`

**Estimated scope:** S

### Task 10: every step of the `ci` job

**Description:** Every step of the `ci` job in `.github/workflows/ci.yml`, one build at a time, with `CARGO_INCREMENTAL=0`, through `sh .plans/yunta-tui/tools/gate.sh /Users/eulke/development/yunta-tui .plans/yunta-tui/out/gate-surco-screen`.

**Acceptance criteria:**
- [x] Every step passes: fmt, clippy, build, schema, smells, docs, deny, doc, test, core-release, workflows, crates.
- [x] The test step shows no aborted binary, and its count is explained against 270 binaries and 4,504 tests.

**Verification:**
- [x] The gate's logs, in `.plans/yunta-tui/out/gate-surco-screen/`.

**Dependencies:** Task 9

**Files likely touched:** whatever a failing step points at.

**Estimated scope:** S

**Found while implementing:**
- The whole gate stopped in its eleventh step, `workflows`: the session's shell was reaped while the machine ran short of memory. Its first ten steps had passed. `workflows` (66 s) and `crates` (72 s) then passed on their own, in the foreground.
- The test step ran 270 binaries and 4,528 tests, none failed and none aborted: the 4,504 of the base and the 24 this module adds, each inside a binary that existed.

### Checkpoint F: after Tasks 9-10
- [ ] The user reviews the whole diff and the goldens. No commit until approved.

### Task 11: commit the module

**Description:** One commit for the module on `feature/yunta-tui`, staging paths by name, with a message that cites no document. Then the push, the pull request's list and test plan, and CI on Linux and macOS. Then `cargo clean`.

**Acceptance criteria:**
- [ ] The commit holds the module's files and nothing else, including `SPEC-surco-screen.md`, `tasks/plan.md` and `tasks/todo.md`.
- [ ] CI on the pull request passes on Linux and macOS.

**Verification:**
- [ ] Manual check: `git show --stat HEAD`, `git status` and `gh pr checks 30`.

**Dependencies:** Checkpoint F

**Files likely touched:** none beyond the module's.

**Estimated scope:** XS

## The spec's success criteria

| Criterion | Tasks |
|---|---|
| 1. A public style reading, one table with `Paint` | 1 |
| 2. The three blocks and the two compositions | 3, 4, 5, 6 |
| 3. Every surface draws each block, Markdown and HTML each screen; no word lost | 3, 4, 5, 6 |
| 4. A screen with no cell, pixel or color; a pane's exact width | 6 |
| 5. Goldens for each, reviewed before kept | 2, 3, 4, 5, 6 |
| 6. `Card` and `DeskFoot` on `Legend`, the region on `Attention` | 7, 8 |
| 7. No golden changes but the catalog's appended part; the suite passes | Checkpoints A, C, E; 10 |
| 8. No new dependency, `cargo deny` as before | 10 |

## Not in this list

The other modules of `CAPABILITY-MAP.md` (`run-state`, `third-party-notices`, then `run-pages`, `control`, `app-core`, `tui`) have no spec, plan or tasks yet. Each goes through the same flow when this one closes.
