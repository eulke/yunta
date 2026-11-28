# Tasks: `live-screens`

Spec: `SPEC-live-screens.md`. Plan: `tasks/plan.md`, approved on 2026-11-14. Status: **approved on 2026-11-15, in four phases.** Base: `4315c1da` (`feature/live-screens`, where `tui` ended on `main`).

**How these run:**
- One at a time, with `incremental-implementation` and `test-driven-development`, in four phases, each ending at a checkpoint for the user.
- Inside a phase, each group of tasks ends at a group check: the suite's groups green, and the digests and captures against the base where the group touches them. A group check does not stop for the user.
- Inside a phase, the work stops for the user only on a decision that is theirs: a printed text that would change beyond what this list names, a choice neither the plan nor this list makes, a cap that would rise, a target that cannot be met, or a failure whose cause is not found.
- `cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.
- Before editing a file, `cargo xtask docs --about <files>`; each rule, trap and note it prints is kept true.
- A commit closes each phase, once the user approves its checkpoint and every step of the `ci` job passes, one build at a time with `CARGO_INCREMENTAL=0`. Paths are staged by name. Then `cargo clean`.
- The engine's and the CLI's suites run in foreground batches: `python3 .plans/yunta-tui/tools/suite.py <group>`.
- What a task proves outside the repository goes in `.plans/yunta-tui/proof/live-screens/`.
- The lead orchestrates: surveys and measurements on Haiku, units of code and reviews on Opus; the lead writes each unit's instructions, decides every finding and runs every gate.

**How an item moves:**
- Its names and its form change first, in place, with every caller. Then it moves, with every caller's path.
- Neither step leaves an alias, a re-export or an old name, and the workspace compiles and passes after each task.
- A task that moves a file does it with `git mv` before it edits it. A file that is only partly moving is split: the moving part goes, the rest stays.
- A task that moves or deletes an item touches every caller in one step, so it can be larger than five files. Each such step is mechanical, the compiler names every caller, and the moved bodies are read with `git diff -M` against their old files.

**How a screen reaches its design:**
- The inventory lists every component and every phrase of the design, screen by screen. A screen's task names the rows it closes and the targets it must equal.
- A target is text written by hand and approved by the user at Checkpoint A. A golden equals its target; a target changes only with the user's approval.
- A component the design draws another way is redrawn in Surco once, and a phrase the design says another way is reworded in render once. The CLI's goldens that change with it are named in the task, blessed there (`YUNTA_BLESS=1`), and their diff is reviewed at the phase's checkpoint.
- The targets are committed at Checkpoint A in `crates/tui/goldens/targets/`, in four environments: `unicode-120`, `unicode-80`, `ascii-120`, `unicode-60`. The task that makes the code draw a screen moves its targets (`git mv`) into `crates/tui/goldens/screens/`, where `crates/tui/tests/screens.rs` holds the code to them.
- A screen's golden changes once: from what it draws at the base to its approved target, in the phase that lands it. A base frame, with its builder, is deleted in the task that first changes what it draws, never edited.

**Where each test lives:**
- **The reader:** `crates/storage`, on real logs.
- **The fold:** `crates/replay/tests/`, with its property over logs folded in chunks.
- **The ledgers:** beside each ledger in `crates/core`, and the receipt's goldens.
- **The follower:** `crates/api/tests/`, on real logs in a test home.
- **Surco's pieces:** unit tests beside each module, and goldens in `crates/surco/goldens/`, ASCII and Unicode, on the plain and 16-color layers.
- **Render's parts and phrases:** beside each module in `crates/render`, and its goldens.
- **`yunta-ui`:** `crates/ui/tests/`, through `State` alone, and a stage's own `tests.rs`.
- **A screen drawn whole:** `crates/tui/tests/screens.rs`, on Ratatui's `TestBackend`, against the targets in `crates/tui/goldens/screens/`.
- **The binary at a terminal:** `crates/cli/tests/application.rs` and `crates/cli/tests/application/`, one file a slice, through `yunta-testkit`'s pty.
- **The walk:** `yunta-testkit-ui` walks; `crates/ui/tests/walk.rs` holds the application's rules over it, and `crates/tui/tests/walk.rs` lays every state out.

**What fails first:**
- A move has no new behavior, so no test can fail for it. Its guard is every test and golden it touches, passing unchanged, and the base's digests, captures and walk counts.
- A new behavior, a new test or a changed text is seen red, for its own cause, before it passes.
- A screen's golden is red against its approved target before the code that draws it, and green after.
- A test at a pseudo-terminal asserts an order, never a duration: a session is held by a word, and what must stand on the screen stands before the next word is given.

**Decided in this list, beyond the plan, for the user's approval with it.** Each phase lists its own decisions under its heading; these are the ones a person meets:
- **One hundred and two target screens, 408 frames, at Checkpoint A** (Task 35 to Task 42), where the plan counted about sixty: each standing of a node's own scene, each typed line, each key that changes what a decision shows, the overlays over the field and over a prompt put aside, each step of a setup. Their page also shows what `yunta status --follow` and the receipt print after the design's words, beside what they print at the base.
- **A pack is fetched from a directory inside its repository** (Task 147): a source `<repository>//<directory>[@ref]`, which `yunta pack add` takes too. The setup's pack stands at `packs/fragua` of the Yunta repository, which the base cannot fetch.
- **The setup fetches its pack as it opens** (Task 149, Task 150), to propose the runners that pack's workflows name. Nothing is written or installed until the person confirms; a pack that cannot be reached is said on its row, and the setup is written without it.
- **A role is declared with every candidate the machine answers for, in order** (Task 148), as the design draws it (`claude-code/… then codex/…`). `yunta init` and `yunta init -i` write the same list.
- **A prompt answers alike on every interface** (Task 113, Task 118): a number picks on the CLI's card too, and an option's words are typed at the desk's foot in the CLI's desk too.
- **A line asked of a person is drawn in the design's box wherever it is asked** (Task 79, Task 124): on a card, at the desk's foot, in a round, at the first request, and at the CLI's line prompts (`yunta init -i`).
- **The receipt's rows take the design's words** (Task 88), so `receipt.md` and a pull request's body print them from then on.
- **A decision whose document cannot be read is said in place of its card, with why** (Task 103, the target `card-unread`): a screen the design does not draw, shown on the targets' page.
- **Removing the run's `f` in Phase 2 (Task 60) changes `tui`'s two `help` frames by that row:** the only base frames that change before their target lands.
- **The node's printed page shows its feed's last twenty rows** and how many stand above them (Task 67).
- **Surco's reference** (`~/.claude/plans/surco-v3/tools/reference/`, and its artifact) follows each phase that changes a component (Task 68, Task 123, Task 160).

## Phase 1: the ground, and the design written down

Its groups, each green before the next: the base; the reader; the fold; what a task proves; a session held by a word; the follower; the runs the targets show; the inventory; the targets; real runs after.

**Decided in this list, beyond the plan:**
- **The follower is changed in place, never doubled** (Task 12 to Task 18). The review proposed to rename the old follower and keep it beside the new one until Phase 2 (its first finding); the plan rejects two followers. So `run::follow` moves up to `follow` first (`git mv`, a move), then is reshaped in steps: a look reads through the kept reader, inside the API alone; it never ends by itself, its three callers moving with it; it follows what `Followed` names and hands the run read whole its events, every caller moving at once; it tells a slow reader how many words it missed; it follows the field. At no task do two followers stand in the tree.
- **The follower's news, in full** (the plan names the types and leaves the arms):
  - `FollowNews::Field(Box<FollowedField>)`: the field as far as it reaches, read whole (each run at its head as a `FollowedRun`, those it cannot read, how many it leaves to other repositories, whether it is this repository's), on the first look and on a `refollow` to another reach. `FollowNews::Joined(Box<FollowedRun>)`: a run that came into the field after. `FollowNews::Whole { run, events }`: the run read whole, at its head, with every event from its first. `FollowNews::Run { run, news }`: what came to a run held. `FollowNews::Unreadable(UnreadableRun)`, `FollowNews::Left(RunId)`, and `FollowNews::Elsewhere(usize)`: how many runs the field leaves to other repositories, once it changes.
  - `RunNews::{Events, Engine, Said { said, missed }, Successor { run, events }, Awaited(Option<Awaited>), AwaitedUnread { why }, Document { node, document }, Remark(Remark)}`. `Stopped` and `Stalled` go: they are readings of a frame.
  - `Document::{Read(ShownDocument), Unread { shown, path, why }}`, the name plan section 4 already gives `RunInView.handed_over`. Not `ShownDocument`, what a decision shows.
  - A decision whose shown document fails today's shape arrives as `RunNews::AwaitedUnread { why }`: `Awaited` keeps the two arms the spec gives it, and `WaitingDecision`, which the CLI's prompts read, keeps its type. The application keeps the why and shows it in place of the card (the target `card-unread`).
  - `Followed` is `Debug, Clone, PartialEq, Eq`; `FollowNews`, `RunNews`, `FollowedField`, `Awaited` and `Document` are `Debug, Clone`. `Following::next` may be dropped between two looks and loses no news: what a look read waits in the following's queue until it is handed, so a caller cancels `next` to `refollow`.
  - A `Remark` is said by `yunta status --follow` through the CLI's remarks, as `yunta status` says it; the application says it once as a note.
- **`RunState::chronicled` hands back `Option<Moment>`**, where the plan writes `Moment`: none for an event at or below the head, so news handed twice add no moment and the walk stays bounded. **`head` moves past a break** while the ledgers stay at it, so a follower never reads a broken log's tail again; it is left out of `RunState`'s equality, as the clock and the activity are.
- **`FollowedRun::took` folds with `chronicled` and hands back the moments**, and works the menu out once for the new head. A receiver that keeps moments takes `chronicle(&events)` from `Whole` and `took`'s moments after it: one fold, the same events.
- **The follower paces itself on tokio's monotonic time** (its looks, the registry each second, the directories every ten seconds, the successor's 30 s), and stamps frames with the workspace clock alone. At the base the registry's second is read off the workspace clock, so a test under a clock that stands still never reads it again.
- **Where the ledgers keep what the plan gives "the run"**: the union of every scope check's paths sits in `NodeLedger`, which every `scope_checked` reaches; `RunLedger` folds run events only. `TaskLedger::apply_criteria` becomes `apply_checks`, since it folds a task's scope check too. `scope_checked` leaves the replay test's audit kinds.
- **`LogReader::statements`** counts what the reader ran, so a unit test of the follower (`crates/api/src/follow/tests.rs`, where the reader is in reach) holds that a look where nothing moved costs one pragma.
- **The targets are committed in `crates/tui/goldens/targets/`**, as `<screen>.<environment>.txt` in four environments, `unicode-120`, `unicode-80`, `ascii-120` and `unicode-60`, apart from `goldens/screens/`, which keeps `tui`'s fifteen frames under the same screen names (`field`, `desk`, `palette`, `help`). The task that draws a screen moves (`git mv`) its targets from `goldens/targets/` into `goldens/screens/`, where `crates/tui/tests/screens.rs` holds the code to them; a base frame, and its builder in `crates/testkit-ui/src/screens.rs`, is deleted in the task that first changes what it draws, never edited. `room-needed` is no target: it stands only below 60×14, so it has none of the four environments, and its base frame is the one kept.
- **The inventory is Markdown tables of fixed columns**, one file a family under `proof/live-screens/inventory/`, which a coverage tool and the targets' checker read; a phrase with a value in it writes the value as `{…}`. Each row names the spec's element it carries and the survey's row it comes from, so a later task cites a row by either.
- **A line asked of a person is drawn in the design's box wherever it is asked**: on a card (an option that asks for words, a note), at the desk's foot, in a round's typed answer, in a setup's typed line. The box belongs to the row a person types on, so every screen with a typed line draws it alike. A list narrowed by typing is not a line asked: it keeps the design's row, `type to filter` after the pointer.
- **Only the design's run is recorded and kept**: it is long, and three of its nodes work at once. The small runs and the three past runs are built in each test process with fixed ids under the stepping clock, as today's screens world builds its runs, and a test builds each twice and compares. Phases 2 to 4 take them as `yunta_testkit_ui::small_runs`, built in-process. They are sequential but one: the loop with two tasks at work at once, whose sessions are held by `awaits_word` and let go in a fixed order, as the design's run is recorded, so it too builds the same twice.
- **What recording a run needs is the test harness's own**: `SteppingClock` in `yunta-testkit-core`; `Bench::answer_parked`, moved from the engine tests' `common` (every caller there switched), and `Bench::answer_round`. A log is loaded into a fresh store through `yunta-testkit`, so `yunta-testkit-ui` gains no edge to `yunta-storage`.
- **Each target's cut is code**: `yunta_testkit_ui::cut::TARGET_CUTS` names, for each target, the runs it shows at their moments (each run and its seq), the registry, the node pointed at and the keys pressed, and `Cut` loads several runs at their moments into one store, so a field is a cut too. The goldens of Phases 2 to 4 take a target's state from it by the target's name. `proof/live-screens/dumps/INDEX.md` mirrors it, written from it by the probe's `dump`.
- **101 target screens, where the plan counts about 60**: each standing of a node's own scene, each typed line on a card or the desk, each key that changes what a decision shows (its proof, what is decided, an answer recorded, a prompt put aside), the overlays over the field and over a prompt put aside, and each step of a setup is a screen of its own (Task 35 to Task 41).
- **The rewordings `yunta status --follow` and the receipt take later** (the meter at a terminal's end, the next steps' words, the receipt's rows, `receipt.md` and the pull request's body) are shown on the targets' page beside the base's prints for the user to accept. Criterion 12 holds at Checkpoint A; after it the two differ from the base only by the design's phrases and components, each difference a row of the inventory, approved again at the checkpoint of the phase that makes it (B or C).
- **New proof tools**, measured on the base before any change: `at_rest.py` (the application's processor time at rest), and the probe's `receipts` (the receipt of every kept log), `whole` (the time to read whole a run with a node of ten thousand calls) and `dump` (a cut's facts).

**Needs from other phases:**
- Phase 2 removes `Operation::Follow`, `News::Following` and `Outcome::Followed`, which this phase keeps working over the new follower, and keeps the run stage's fold by `chronicled`.
- In Phases 2 to 4, the task that draws a screen moves (`git mv`) its targets from `goldens/targets/` into `goldens/screens/`, takes its state from `yunta_testkit_ui::cut::TARGET_CUTS` by the target's name, and carries each decision the user takes on the targets' page about what a screen says today and the design does not; the task that first changes a base frame deletes it with its builder. Every target written here is landed by one later task; `room-needed` stays the one base frame kept.
- Phase 2 folds every arm of the follower's news (`FollowNews::{Field, Joined, Whole, Run, Unreadable, Left, Elsewhere}`, `RunNews::{Events, Engine, Said, Successor, Awaited, AwaitedUnread, Document, Remark}`), makes `FieldInView` from `FollowedField` and its rows by `FollowedRun::row(now)`, and keeps `AwaitedUnread`'s why in `RunInView`. Phase 3 reads the ledgers' facts, `RunNews::Document`, `Awaited` and the call's session that this phase delivers, shows `AwaitedUnread`'s why in place of the card, and says a `Remark` once as a note.
- Phases 2 and 3 change what `yunta status --follow` and the receipt print only by the rewordings the user accepts on the targets' page, each approved again at Checkpoint B or C.
- Phase 4 measures again with this phase's tools (`at_rest.py`, the probe's `frame` and `whole`, `real_run.py`) against the base recorded here.

*In parallel:* Task 1 alone, then Task 2; Task 3 runs once Task 2's release build stands.

### Task 1: the proof's tools, pointed at this module

**Description:**
- `goldens.py`, `pages.sh`, `normalize.py`, `documents.py`, `mcp_answers.py`, `prompts.py`, `follow.sh`, `followers.py`, `real_run.py`, `follow_in_app.py`, `follow_and_answer.py`, `base_suite.sh`, `prints.sh`, `screens.py`, `at_a_terminal.py`, `first_screen.py` and the probe (`frame`, `parts`) are copied from `proof/tui/` into `proof/live-screens/`, unchanged but for their folder.
- `at_rest.py`: bare `yunta` at an 80×24 pseudo-terminal in a sandbox whose store holds runs at rest (one finished, one parked on a gate, one stopped), moved to the machine's field by a key it is given (`a` at the base), left a minute; it prints the process's processor time over that minute as a share of one core, the median of three.
- The probe gains three binaries:
  - `receipts`: the receipt of every log under `crates/replay/tests/fixtures/logs/`, each with a manifest naming the nodes its log names, and of each real run given on the command line, as JSON;
  - `whole`: a run whose one node made ten thousand calls, stored, read whole as the application reads it (the following's first reading, then the chronicle of its events), the median of 20;
  - `dump` is added later (Task 25).
- `follow_in_app.py` (a real fragua run opened from the field of bare `yunta` and followed in the application) and `follow_and_answer.py` (the same, each gate answered on its card) are the drivers Phases 2 and 4 run their real runs with; each drives a real run, so here they are copied and not run.
- `prints.sh` writes the receipts beside its other captures, which `normalize.py` compares whole.

**Found in its turn:** sixteen tools and the probe's `frame` and `parts` are copied and pointed here; `at_rest.py` and the probe's `receipts` and `whole` are new, `whole` taking the number of calls; `prints.sh` writes `receipts.txt`. Twice equal: 159 views of `prints.sh`, 247 golden digests, the suite by name, the symbol builds. Medians twice: first screen 61.1 and 62.2 ms; a drawing 0.203 and 0.201 ms; a run read whole 3,952 and 4,204 ms at ten thousand calls; the application at rest 0.110% and 0.098% of a core, a spread of a tenth of the value at a size fifty times under the 2% the spec bounds, so the base is recorded as that range. `normalize.py` compares the final screen where an application capture's bytes differ, since drawings land in another order between runs. A parked run cancelled reads "needs you", so the tool's stopped run is a gate answered `abort`. The suite held one failure of this module's own making: a line of Spanish in the spec raised a smell counter; the line is in English. The probe's `whole` calls `Project::follow`, so the tasks that reshape the follower point it again.

**Fails first:** none: tools.

**Acceptance criteria:**
- [x] Each tool but the drivers of a real run runs twice on the base and finds nothing between its two runs, once normalized; the timing tools print medians within 10% of each other.

**Verification:**
- [x] Manual check: `python3 normalize.py` over two runs of `prints.sh` prints no difference.

**Dependencies:** None

**Files likely touched:** `.plans/yunta-tui/proof/live-screens/` (outside git)

**Estimated scope:** M

### Task 2: what the base prints, weighs and costs

**Description:**
- The suite's binaries and tests by name, in `suite.py`'s groups, into `proof/live-screens/base-suite/`.
- Every golden's digest, `digests-base.txt`.
- A release build, `proof/live-screens/yunta-base`, its size, and its size by crate from a symbol build (`tools/size_by_crate.py`).
- `prints.sh` from the release build into `base-1/`, then into `base-2/`, receipts included.
- The walk's count of states, palettes and helps, and its time.
- The probe's `frame` (a drawing at 200×60, median of 2,000), `whole` and `at_rest.py`.
- All of it in `proof/live-screens/baseline.txt`.

**Found in its turn:** the base, in `proof/live-screens/`: 314 binaries and 4,865 tests, none failing; 247 goldens; 159 views of `prints.sh`, twice equal, the receipts of 8 kept logs among them; the walk 22,388 states (ui 19.3 s; tui 5,145 screens at each size, 22.0 s); the release 30,599,936 bytes, 27,617,320 of text; a drawing whole 0.202 ms; a run of ten thousand calls read whole 3,948 ms; the application at rest between 0.091% and 0.110% of a core; the first screen 60.9 ms. `yunta-base` is Task 1's release build, installed as it was. `size_by_crate.py` compares two binaries, so the base is sized against an empty object. The walk's package list is a zsh array: as one string cargo reads it as a filter and runs no test.

**Fails first:** none: measurements.

**Acceptance criteria:**
- [x] `base-1` and `base-2` are equal once normalized; every count, size and time is written in `baseline.txt`.

**Verification:**
- [x] Manual check: `python3 normalize.py base-1 base-2` prints no difference.

**Dependencies:** Task 1

**Files likely touched:** `.plans/yunta-tui/proof/live-screens/` (outside git)

**Estimated scope:** S

### Task 3: three real runs on the base

**Description:**
- `real_run.py` starts fragua detached three times from the base's release binary in the sandbox repository, on the CLI and model `tui`'s runs used, with the forge's token pointing at a repository that does not exist; each run is stopped before its `pr` node and its gates are answered with `yunta resolve-gate`.
- Each is followed with `yunta status --follow` through a pipe from its start (`follow.sh`), its lines kept for `followers.py`.
- Each run's time between nodes (`between_secs`) and tokens are read from `yunta stats <run>`; the three runs' spread is the one the phase's later runs are held to. Its log and manifest are kept for `receipts`.

**Found in its turn:** three runs from `yunta-base`, each to `ship`, answered `abort` and closed: `01M7EV6MJ3YHVXVSZNWPEEBE90` (166 s, between 0.555 s, 1.33M tokens), `01M7EVC9Q4JRSMPC5KQP053APM` (156 s, 0.536 s, 1.00M), `01M7EVHEXNXTG39PKX8B3CPCY1` (136 s, 0.540 s, 1.08M). The time between nodes spreads over 0.019 s around a median of 0.540 s, the bound the phase's later runs are held to. Planner on sonnet-4-6, executor and mechanical on haiku-4-5, both reviewers on codex `gpt-6-luna`. `follow.sh` captures mock runs only; `real_run.py` starts its own follower through a pipe, and each ended at the decision on `ship`. The followers differ from each other, and from the previous module's, only in what the agents did. `real_run.py` takes the binary as an absolute path.

**Fails first:** none: measurements.

**Acceptance criteria:**
- [x] Three runs reach the node before `pr`; each follower ended where its run stopped; the spread of `between_secs` and the tokens are in `baseline.txt`.

**Verification:**
- [x] Manual check: `yunta stats <run>` read back against `baseline.txt`.

**Dependencies:** Task 2

**Files likely touched:** `.plans/yunta-tui/proof/live-screens/real-base*` (outside git)

**Estimated scope:** S

### Group check: the base
- [x] The base's suite, digests, captures, receipts and walk counts stand, twice equal; the real runs' spread, the drawing, the read whole and the processor at rest are recorded.

*In parallel:* the reader's group, the fold's, what a task proves and the session held by a word touch four crates; the fold and the ledgers share `crates/replay/src/lib.rs` and `crates/replay/tests/replay.rs`, so Task 8 waits for Task 5. Otherwise they may run at once, each closing at its own check; the follower's group starts once the first three are green.

### Task 4: `LogReader`, the log through one connection kept open

**Description:**
- **`yunta_storage::LogReader`**, in `crates/storage/src/log_reader.rs`: one `Storage` kept behind an `Arc`, opened as `Storage::open` opens (so a first open on a fresh or older log sets it up, taking the lock and writing, as every open does today), then `PRAGMA query_only = ON`, so its connection refuses every write. Its `///`: "Not `AsyncStorage`, which opens a connection for each call."
  - `open(path)`, `moved()` (`PRAGMA data_version` against the last reading: whether another connection committed since), `heads()`, `events_after(run, seq)`, `events(run)`: each one statement on a blocking thread, no transaction held between two calls, so it never holds the engine's checkpoint back.
  - `statements()`: how many statements it ran, which a follower's test reads.
- `Storage` gains `data_version` beside its readings in `store/reading.rs`.
- The crate's `//!` map names `log_reader`.

**Found in its turn:** `Storage`'s connection is private to `store`, so the reading of `data_version` and the switch that refuses writes are `Storage`'s own, in `store/reading.rs`, and `LogReader` calls them. A reading of `data_version` that fails is `StorageError::DataVersion`, a variant of its own. `statements` counts the calls made after the open, one statement each. `async_storage.rs`'s `//!` no longer says it is the only way async code reaches the log.

**Fails first:** `crates/storage/src/log_reader/tests.rs`, on real logs in a temporary directory, failing before for want of the type:
- an insert on the reader's connection is refused as read only;
- `moved` is false while nothing else wrote since the reader opened, true once after a `Storage` on the same path appended, then false again, and a false one costs one statement;
- a reader opened where no log is leaves one in WAL that `Storage::open` reads;
- `heads`, `events` and `events_after` equal `Storage`'s on the same log.

**Acceptance criteria:**
- [x] No public item of `yunta-api` names `LogReader` (`public_items` passes as it stands).
- [x] `cargo xtask docs --check` and `cargo xtask smells --check` pass.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-storage log_reader`, `cargo test -p yunta-storage --test store`

**Dependencies:** Group check: the base

**Files likely touched:** `crates/storage/src/lib.rs`, `crates/storage/src/log_reader.rs` (new), `crates/storage/src/log_reader/tests.rs` (new), `crates/storage/src/store/reading.rs`

**Estimated scope:** S

### Group check: the reader
- [x] `python3 .plans/yunta-tui/tools/suite.py crates` green; every golden's digest equals the base.

*In parallel:* Task 5 then Task 6 (they share `crates/replay/src/lib.rs`).

**Found in its check:** the crates' group 120 binaries and 2,745 tests, none failing; against the base, four new names, the reader's own; 247 goldens unchanged.

### Task 5: `RunState::fold` and `head`, with `replay` on them

**Description:**
- **`RunState::fold(&mut self, event: &StoredEvent)`**, in `crates/replay/src/lib.rs`: an event at or below the head moves nothing; past it, the head moves to its seq; once the log broke nothing else moves; otherwise the event is applied, and a refusal marks the state broken with why, as `replay` marks it today.
- **`RunState::head(&self) -> Option<Seq>`**: the seq of the last event the state took, past a break too. Left out of `PartialEq`, as the clock and the activity are.
- **`replay` is `fold` over a slice.** The engine's `RunView::of` derives its state by `replay`, so the engine's state is derived by the fold with no change to its code; its suites guard it and the real runs after (Task 43) measure it.
- **`apply` is private.** Its callers outside replay take `fold`: the follower's `FollowedRun::take` (`crates/api/src/run/follow/followed.rs`), which no longer folds past a break nor replays the whole log on a refusal (the base's follower folded past the event where a log broke); and `a_kind_that_moves_no_state_says_so_by_name` in `crates/replay/tests/replay.rs`, where a refused event reads as broken, which moves the state.
- Replay's `//!` says the one fold and its two rules (once a log broke, and an event already taken).

**Found in its turn:** the fold stands in a private module of its own, `crates/replay/src/fold.rs` (`replay`, `fold`, `head`, `apply`, `apply_owners`, `apply_run`): with it in `lib.rs` the file and its `//!` passed the size the smell counters hold at zero. The tasks that name `lib.rs` for the fold edit `fold.rs`. The engine has no caller of `apply`: it reaches the fold through `replay`. In the API, `FollowedRun::take` folds, so it no longer folds past a break. A refused event reads as broken in `a_kind_that_moves_no_state_says_so_by_name`. The property that redelivers an event at its own seq now holds by the fold's rule alone, before the artifact and finding ledgers' keying is reached; no log of the suite repeats a seq. proptest's regressions file for the properties is kept, as the engine's and the store's are.

**Fails first:** `crates/replay/tests/properties.rs`, red before for want of `fold` and `head`, and red with `fold` made a bare `apply`:
- `folding_a_log_in_any_chunks_equals_replaying_it`: an arbitrary log cut at arbitrary points and folded chunk by chunk equals `RunState::replay` of it, head included;
- `a_broken_log_folds_as_it_replays`: a log with a refused event in its middle, folded in chunks, has the same `broken` as its replay, and its head at its last event;
- `an_event_already_taken_moves_nothing`: a log folded, then folded again whole, is unchanged.

**Acceptance criteria:**
- [x] `apply` has no caller outside `yunta-replay`.
- [x] Every test of replay, the engine and the API passes unchanged but `replay.rs`'s audit test; a test whose log repeats a seq, if any, is named here with the seq it is given.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-replay --test properties --test replay --test logs_from_runs`, `cargo test -p yunta-api --test following`
- [x] `python3 .plans/yunta-tui/tools/suite.py engine-1` to `engine-5` green

**Dependencies:** Group check: the base

**Files likely touched:** `crates/replay/src/lib.rs`, `crates/replay/tests/properties.rs`, `crates/replay/tests/replay.rs`, `crates/api/src/run/follow/followed.rs`

**Estimated scope:** M

### Task 6: `RunState::chronicled`, cloning only the node its event names

**Description:**
- **`RunState::chronicled(&mut self, event: &StoredEvent) -> Option<Moment>`**, in `crates/replay/src/frame/chronicle.rs`: the event folded as `fold` folds it, and the moment it is, read against the state at that event. `None` for an event at or below the head. Past a break the event still earns its moment and moves nothing else, as the chronicle does today.
- It clones only the record of the node a node event names, before folding it: `node::happening::Happening::of` takes that record (`Option<&NodeRecord>`) in place of the whole ledger before the event, and `worked` reads it (`crates/core/src/events/node/happening.rs`). A run read whole then costs its events, where today each event clones every node's record, its calls among them.
- **`chronicle(events)`** is `chronicled` over a slice, from a fresh state.
- The probe's `whole` is run again; its time beside the base's goes into `proof/live-screens/after-fold.txt`.

**Found in its turn:** `chronicled` hands `None` for an event at or below the head, and an event past a break still earns its moment, as the chronicle gave it at the base. `Happening::of` and `worked` take the one record of the node the event names. The head's check is `RunState::has_taken`, private to the crate, which `fold` and `chronicled` share, and `apply` is private again. A run with a node of ten thousand calls is read whole in 20.3 ms where the base takes 3,948 (median of 20), and twice the calls take 1.9 times as long where they took 3.7. The chronicle's properties stand in a submodule of the same test target (`tests/properties/chronicle.rs`), since with the new ones `properties.rs` passed the size a smell counter holds.

**Fails first:** `crates/replay/tests/properties.rs`, red before for want of `chronicled`:
- `chronicling_a_log_in_any_chunks_equals_its_chronicle`: the moments of the chunks, joined, equal `chronicle` of the whole, and the state equals its replay;
- `an_event_handed_twice_earns_one_moment`: a chunk handed again adds no moment;
- `the_chronicle_of_a_broken_log_has_a_moment_for_every_event`.

**Acceptance criteria:**
- [x] Every render and CLI golden drawn from a chronicle is unchanged; `chronicle`'s tests pass as they stand.
- [x] `whole`'s median falls from the base's; the read whole of ten thousand calls grows with the calls, not their square (the time of 5,000 and of 10,000 calls are written beside each other).

**Verification:**
- [x] Tests pass: `cargo test -p yunta-replay --test properties --test chronicle`, `cargo test -p yunta-render chronicle`, `cargo test -p yunta-core --test node_ledger`
- [x] Manual check: the probe's `whole` against `baseline.txt`

**Dependencies:** Task 5

**Files likely touched:** `crates/replay/src/frame/chronicle.rs`, `crates/replay/src/lib.rs`, `crates/core/src/events/node/happening.rs`, `crates/replay/tests/properties.rs`

**Estimated scope:** M

### Group check: the fold
- [x] `suite.py crates`, `engine-1` to `engine-5` and `cli-1` to `cli-5` green; digests, captures and receipts equal the base's.

*In parallel:* Task 7 first, then Task 8, then Task 9, then Task 10 (each reads the last's types).

**Found in its check:** 314 binaries and 4,875 tests, none failing: the base's by name and ten new (the reader's four, the fold's three, the chronicle's three); 247 goldens unchanged; `prints.sh`'s 159 views and the receipts equal to the base's.

### Task 7: a call names its session

**Description:**
- **`AgentMessagePayload` gains `session: Option<SessionId>`** (`crates/core/src/events/session/payloads.rs`), skipped when absent, with its `///`: the session the call was made in; absent on a log older than the field, which reads as today.
- **The engine's writer of calls stamps it**: `tool_use` in `crates/engine/src/task_cycle/stream.rs` names the session the stream opened (`ctx.opened`).
- **`session::Happening::Called` carries the session** (`crates/core/src/events/session/happening.rs`); render's chronicle words read it with `..` and say the same words (`crates/render/src/chronicle/words.rs`).
- Every construction of the payload names the field: `None` in the samples and in every test that builds one; the compiler names every caller (`add_field.py`).
- `cargo xtask schema` regenerates `crates/core/schemas/events.json`; compared with its descriptions taken out (the trap on unit variants), its diff is the one field.

**Found in its turn:** `AgentMessagePayload::session` is left out of the JSON when absent; `events.json` gains that one property. `Happening::Called` carries it, and render's chronicle prints the same words. `stream.rs` stood at exactly 500 lines, so `usage`, `completed` and `over_cap` moved to `task_cycle/stream/token_cap.rs`; render's `session_words` passed 50 lines, so its two run-tool arms share `run_tool_call`. All 247 goldens equal the base's; of `prints.sh`'s 159 views, 158 equal the base's and the receipts are byte-equal. The one that differs, `prompt-review`, is a race of the capture the base has too (the base's own binary gives it in 2 of 3 captures): it sometimes catches `approve-plan` still running.

**Fails first:** `crates/engine/tests/run_sessions/messages.rs`: a mock session's tool call on the log names the session its `agent_session_opened` names. It fails to compile before the field, and then reads `None` until the writer stamps it.

**Acceptance criteria:**
- [x] Every kept log replays and its receipt is the base's; `logs_from_runs` passes unchanged.
- [x] `cargo xtask schema` leaves no other difference; `documents.py`'s captures differ from the base's only by `session` on calls.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-engine --test run_sessions`, `cargo test -p yunta-core --test schema_json --test tool_action`, `cargo test -p yunta-replay --test logs_from_runs`
- [x] Build succeeds: `cargo xtask schema`, then `git diff crates/core/schemas/`

**Dependencies:** Group check: the base

**Files likely touched:** `crates/core/src/events/session/payloads.rs`, `crates/core/src/events/session/happening.rs`, `crates/core/src/events/samples/session.rs`, `crates/engine/src/task_cycle/stream.rs`, `crates/render/src/chronicle/words.rs`, `crates/core/schemas/events.json`, and every test that builds the payload, the compiler naming every caller (`crates/core/tests/tool_action.rs`, `crates/render/src/page/run/tests.rs`, `crates/render/src/chronicle/tests/calls.rs`, `crates/replay/tests/frame.rs`, `crates/replay/tests/live_derivation/messages.rs`, `crates/engine/tests/schedule.rs`)

**Estimated scope:** M (a field added at every construction, the compiler naming every caller)

### Task 8: the tasks' ledger learns a task's proof and scope

**Description:**
- **`TaskRecord` gains** (`crates/core/src/events/tasks/ledger.rs`), each with its `///`:
  - `criteria` and `scope`, folded from `task_registered`;
  - `checked`: its latest `criteria_checked` after its phase (`Phase::Post`), as the log has it: each criterion's command, exit code and tail. A pre check and a session's own check (`task_check_answered`, still audit) are apart;
  - `scope_checked`: its latest `scope_checked`, its diff and violations, which marks a path written on a log whose calls name no session.
- `TaskLedger::apply_criteria` becomes `apply_checks`, folding a task's scope check beside its criteria; its one caller in `RunState`'s dispatch follows.
- `scope_checked` leaves `AUDIT_KINDS` in `crates/replay/tests/replay.rs`: it now moves a ledger.

**Found in its turn:** `TaskRecord` gains `criteria` and `scope` (the latest registration's), `checked` (the latest check after its phase, never a session's own) and `scope_checked`. A second registration under one id takes the latest criteria and scope and clears neither check, so the receipt's latest check after the work reads the same. A check that names a task never registered is kept on no task: the engine writes one only for a dispatched task, which a registration precedes, so real logs print the same; only `distill.rs`'s unit test builds one, and distill reads raw events. The fold's one caller of `apply_checks` is in `fold.rs`.

**Fails first:** `crates/core/tests/task_ledger.rs`, red for want of the fields:
- a task registered with two criteria and a scope keeps both;
- a pre check, a red post check, a session's own check, then a green post check leave the green post check as `checked`;
- a task's scope check is kept on it, and one naming no task on none.

**Acceptance criteria:**
- [x] `a_kind_that_moves_no_state_says_so_by_name` passes with `scope_checked` moving.
- [x] Every frame and page reads the same: render's goldens unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-core --test task_ledger`, `cargo test -p yunta-replay --test replay`

**Dependencies:** Task 5, Task 7

**Files likely touched:** `crates/core/src/events/tasks/ledger.rs`, `crates/replay/src/lib.rs`, `crates/core/tests/task_ledger.rs`, `crates/replay/tests/replay.rs`

**Estimated scope:** S

### Task 9: the node's ledger learns each session's task and what it wrote

**Description:**
- In `crates/core/src/events/node/ledger.rs` and `ledger/sessions.rs`, each with its `///`:
  - `OpenSession` gains `task`, from `agent_session_opened`;
  - `NodeRecord` gains every session's task, kept past the session's close, where only a task's last session is kept today (`TaskRecord::last_session`);
  - `NodeRecord` gains the paths each session's write and edit calls name, kept for the run, under the call's session, or under none for a call from a log older than it;
  - `NodeRecord` gains its scope checks that name no task;
  - `NodeLedger` keeps the union of every scope check's diff and violations over the run, which the receipt prints;
  - `ToolCall` gains the call's `session`.
- `NodeLedger::apply` folds `scope_checked`.

**Found in its turn:** `OpenSession` gains `task` and `ToolCall` gains `session`. `NodeRecord` keeps, past a session's close, `session_tasks`, `written` (the paths each session's write and edit calls name) and `scope_checks` (the node's own, naming no task). `NodeLedger::scope_union` hands the union of every scope check's diff and violations, folded before the ledger looks for the event's node, so a check under no node still counts, as the receipt counts it. The union and its fold stand in `ledger/scope.rs`, since `ledger.rs` would pass the size a smell counter holds. The findings' ledger needed no new test of a finding posted twice: `a_sequence_the_engine_never_writes_leaves_the_state_unmoved` posts one twice and fails when the keying is broken.

**Fails first:** `crates/core/tests/node_ledger.rs`, red for want of the fields: two task sessions open on one loop node, each editing its own path, then both close; each session's task is kept, each path under its session, a call with no session under none, a scope check naming no task on the node, and the union holds a task's and the node's checks.

**Acceptance criteria:**
- [x] Frames and pages read the same: render's goldens and replay's frame tests unchanged.
- [x] `replay.rs`'s audit test passes as Task 8 left it.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-core --test node_ledger`, `cargo test -p yunta-replay`

**Dependencies:** Task 8

**Files likely touched:** `crates/core/src/events/node/ledger.rs`, `crates/core/src/events/node/ledger/sessions.rs`, `crates/core/tests/node_ledger.rs`, `crates/replay/src/frame/sample.rs`

**Estimated scope:** M

### Task 10: the receipt reads the ledgers and prints the same

**Description:**
- `crates/replay/src/receipt.rs`: the criteria come from each task's `checked`, the scope from the node ledger's union, and the reroutes from the node records' counts. `closed` and `so_far` keep their signatures and read no event past replaying them.

**Found in its turn:** only `crates/replay/src/receipt.rs` changes: the criteria come from each task's `TaskRecord::checked`, the scope from `NodeLedger::scope_union`, the reroutes from the node records. The receipts of the 8 kept logs equal the base's byte for byte, and those of the three real runs and five mock runs equal what the base's binary prints, the event chain apart. On a broken log, criteria, scope and reroutes stop at the break, as the rest of the receipt does. No kept log and no run holds a check that names a task never registered. With `checked` made to keep the first check after the work, two kept logs and core's ledger test went red, but the engine's receipt tests and all of replay's stayed green: no receipt test holds a retried task's last verdict, and `yunta-replay` has no test named for the receipt. The count of reroutes is worked out twice, here and in `frame::reroutes`. Task 10a adds the test and gives the count one home.

**Fails first:** none: what the receipt reads moves, and it prints the same. Its guards: the engine's receipt goldens (`crates/engine/goldens/receipt/`), `receipt_cmd`, and the probe's `receipts` over every kept log and the base's three real runs, equal to the base's. With `checked` made to keep the first post check, the engine's retried-task receipt is seen red.

**Acceptance criteria:**
- [x] `receipts`' output equals `receipts-base.json`; the receipt goldens are unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-replay receipt`, `cargo test -p yunta-engine --test receipt`, `cargo test -p yunta --test receipt_cmd`
- [x] Manual check: the probe's `receipts` against the base's

**Dependencies:** Task 9

**Files likely touched:** `crates/replay/src/receipt.rs`

**Estimated scope:** S

### Task 10a: a retried task's last verdict, and one count of reroutes

**Description:**
- **The receipt of a task checked red after its work, sent back, and checked green** counts it green once: a test of the receipt itself holds it, where only the tasks' ledger's own test and the kept logs held it. It stands in `yunta-replay`'s tests, beside the frame's and the chronicle's, over a log built as that crate's tests build theirs.
- **The count of reroutes has one home.** `frame::reroutes` (`crates/replay/src/frame/mod.rs:321`) and the receipt each sum the node records' reroutes; the node's ledger says it once, and both read it.

**Found in its turn:** `crates/replay/tests/receipt.rs` holds a task registered, checked red before its work, red after it, sent back and checked green: the receipt counts it green once, and the test goes red with `checked` kept at the first check after the work. `NodeLedger::reroutes` sums a run's reroutes, read by `RunFrame::of` and by the receipt; `frame::reroutes` is gone. `node/ledger.rs` stands at 499 lines, one under the size a smell counter holds, so its next addition takes a submodule.

**Fails first:** the receipt's test, seen red with `TaskRecord::checked` made to keep the first check after the work, then the rule put back. The count's move has no new behaviour: the frame's and the receipt's tests and goldens pass unchanged.

**Acceptance criteria:**
- [x] The test fails under the mutation and passes without it.
- [x] One function sums a run's reroutes, and `grep` finds no second sum in `crates/replay/src`.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-replay`, `cargo test -p yunta-core --test node_ledger`, `cargo test -p yunta-engine --test receipt`

**Dependencies:** Task 10

**Files likely touched:** `crates/replay/tests/` (the receipt's test, new), `crates/replay/src/receipt.rs`, `crates/replay/src/frame/mod.rs`, `crates/core/src/events/node/ledger.rs`

**Estimated scope:** S

### Group check: what a task proves
- [x] Every suite group green; the schema's diff is the one field; captures equal the base's but `session` on calls; receipts equal the base's.

**Found in its check:** 314 binaries and 4,881 tests, none failing: the base's by name, sixteen new and two moved into the chronicle's submodule; the schema's diff is the call's `session` alone; `prints.sh`'s 159 views equal the base's, and `receipts.txt` byte for byte.

### Task 11: a session held by a word

**Description:**
- **`MockStep::AwaitsWord { word, after_ms }`** (`type: awaits_word`), in `crates/adapters/src/mock/fixture.rs`, played in `script.rs`: the session goes on once the file `word` stands; it ends as its stop ends it once the directory holding the word is gone, so no session outlives its test; it honors the session's stops; it is polled at a `Pace`, as the smell's sites allow. Its `///` says it holds a session as `Hold` holds a node.
- **`Hold::word(&self, name) -> PathBuf`**, the path a fixture names, and **`Hold::give(&self, name)`**, which writes it (`crates/testkit/src/held.rs`). `let_go` stays the held node's word; the module's `//!` says a session is held by the same marks.

**Found in its turn:** `MockStep::AwaitsWord { word, after_ms }` (`type: awaits_word`) says nothing, goes on once the file `word` names stands, and ends with no outcome, as a stop ends a session, once the word's directory is gone; it honours a kill and an interrupt. `Hold` gains `word(name)` and `give(name)`, and `go` is the word `go`. The wait stands in `mock/word.rs`, since `script.rs` would pass the size a smell counter holds, and reuses `Stops::awaited`. `yunta-adapters` takes `yunta-testkit` as a dev-dependency to reach `Hold`, as the engine does. With the step made to go on at once, the test that reads the second call before its word is given goes red; neither test waits on time, and 200 runs beside eight busy loops all pass. A relative `word` ends the session at once, since its directory does not stand, and nothing refuses one: Task 11a makes the fixture refuse it.

**Fails first:** `crates/adapters/tests/mock/held.rs`, under both dialects, red before since the fixture refuses an unknown step:
- a session that reads `a`, awaits a word, then reads `b`: with the word never given and its directory removed, it ends having read `a` and not `b`;
- with the word given once `a` is read, `b` comes, then the session completes.

**Acceptance criteria:**
- [x] The test asserts an order and no duration; it passes 200 times under `stress_binary.py` beside busy loops.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-adapters --test mock held`
- [x] Manual check: `python3 .plans/yunta-tui/tools/stress_binary.py` on the mock binary

**Dependencies:** Group check: the base

**Files likely touched:** `crates/adapters/src/mock/fixture.rs`, `crates/adapters/src/mock/script.rs`, `crates/adapters/tests/mock.rs`, `crates/adapters/tests/mock/held.rs` (new), `crates/testkit/src/held.rs`

**Estimated scope:** M

### Task 11a: a word a fixture names is a place that can stand

**Description:**
- A fixture whose `awaits_word` names a relative path is refused as it is read, with the step and why: such a word's directory is the empty path, which never stands, so the session would end at once and a test would read a held session that was never held. The refusal says the step by its place (`steps[1].word`), as the fixture's other refusals do.
- `crates/testkit/src/lib.rs`'s `//!` says `held` holds a run and gives the words a held session waits for.

**Found in its turn:** the refusal is made as the fixture is read, by the reader of the step's `word`, where its other refusals of shape are made: `` `steps[1]`: `word` is `read-b`, a relative path; a word is an absolute path, where its test gives it ``. The place stops at the step, since a step's fields are read from a buffered copy the YAML layer's path does not follow, so the message names `word` right after it, as an unknown key in a step is said. Every step is read through that reader, the steps of a session and of what runs before a stop among them.

**Fails first:** `crates/adapters/tests/mock/held.rs`: a fixture with `word: go` is refused naming `steps[N].word`. Red: it parses, and its session ends with no outcome.

**Acceptance criteria:**
- [x] The refusal names the step and says a word is an absolute path; a fixture with an absolute word plays as before.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-adapters --test mock`

**Dependencies:** Task 11

**Files likely touched:** `crates/adapters/src/mock/fixture.rs`, `crates/adapters/tests/mock/held.rs`, `crates/testkit/src/lib.rs`

**Estimated scope:** S

### Group check: a session held by a word
- [x] `suite.py crates` green; every golden's digest equals the base.

*In parallel:* Task 12 to Task 21 run in order, Task 14, Task 16 and Task 18 each after the task it follows: each changes the follower's module, `crates/api/src/follow/`.

**Found in its check:** the crates' group 121 binaries and 2,759 tests, none failing; 247 goldens unchanged.

### Task 12: the follower moves up to `follow`

**Description:**
- `git mv crates/api/src/run/follow.rs crates/api/src/follow.rs` and `crates/api/src/run/follow/` to `crates/api/src/follow/`; the crate root declares `mod follow` and takes its re-exports from it; `run/mod.rs` loses `mod follow` and its `pub use`. Paths inside the module follow (`crate::OpenedRun`, `crate::run::read_manifest`); `OpenedRun::registry` stays in `follow.rs`.
- The crate's `//!` map names `follow`.

**Found in its turn:** four renames (`follow.rs`, and `followed.rs`, `following.rs`, `listening.rs` under `follow/`), read with `git diff -M`: two whole, `follow.rs` changed by its two paths, `followed.rs` by the fold's edit alone. No public path changes. `OpenedRun::registry` stays in `follow.rs`. Three tests' `Reached by:` lines named the old paths (`crates/cli/tests/run_words/row.rs`, `status_follow.rs`, `status_follow/edges.rs`) and name the new ones. The renames stand in the index, as `git mv` leaves them. The probe outside the repository calls `project.follow(&run)` and `following.followed()`, which this move leaves as they were.

**Fails first:** none: a move. Its guard: every test of the API, the CLI and the application passes unchanged, since `yunta_api::{Following, FollowedRun, RunNews, RunRegistry}` keep their paths; `git diff -M` shows the files renamed.

**Acceptance criteria:**
- [x] `crates/api/src/run/` holds no follower.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api`, `cargo check --workspace --all-targets`

**Dependencies:** Group check: the reader, Group check: the fold, Group check: what a task proves

**Files likely touched:** `crates/api/src/lib.rs`, `crates/api/src/run/mod.rs`, `crates/api/src/follow.rs` (new), `crates/api/src/follow/followed.rs` (new), `crates/api/src/follow/following.rs` (new), `crates/api/src/follow/listening.rs` (new)

**Estimated scope:** S

### Task 13: a look through the kept reader

**Description:**
- **A look reads through a `LogReader`** the following opens and keeps (`crates/api/src/follow/following.rs`): `moved()` every 100 ms, and the events past the head only after another connection committed; the registry once a second while the run's log says it moves or asks; `PromotionChain` reads heads and a new run's first events through the same reader (`crates/api/src/event_log.rs`).
- Every pace is a `///`-reasoned const on tokio's monotonic time; the workspace clock only frames. The registry's second, read off the workspace clock at the base, goes onto tokio's time with the rest.
- What `next` hands and where a following ends stay the base's: only what a look costs changes, and no file outside `yunta-api` moves.
- `follow`'s `//!` says what a look reads, and when.

**Found in its turn:** `Project::follow` opens the reader before it reads the run, so an event committed during that read is caught by a look. A look costs one `moved()`, and reads past the last event held only after another connection committed; the registry is read once a second on tokio's time while the frame says the run moves or asks; the successor's wait is on tokio's time and asks the chain again only after `moved()`. `PromotionChain` reads through the same reader. `EventLog::events_after`, `EventLog::runs` and `promoted_from` had no caller left and are gone; `EventLog` keeps `open` and `events`, which the CLI's surface uses, and `Following` keeps its `EventLog` only for `log()` until the task that hands the run read whole its events. With a look made to read the log's tail each time, the test goes red: `1 looks where nothing moved ran 2 statements`. The walk's counts and `follow.sh`'s captures equal the base's.

**Fails first:** `crates/api/src/follow/tests.rs` (new), where the reader's `statements` is in reach: a look at a store where nothing moved costs one statement. At the base a look reads the log's tail through a connection opened for it.

**Acceptance criteria:**
- [x] `following.rs`'s tests, `status_follow`'s tests and the captures of `status --follow` pass as they stand.
- [x] The application's pty tests and the walk's counts are the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api follow::`, `cargo test -p yunta-api --test following`, `cargo test -p yunta --test status_follow --test application`

**Dependencies:** Task 12

**Files likely touched:** `crates/api/src/follow.rs`, `crates/api/src/follow/following.rs`, `crates/api/src/follow/tests.rs` (new), `crates/api/src/event_log.rs`

**Estimated scope:** M

### Task 14: a follower that never ends by itself

**Description:**
- **`Following::next(&mut self, stop) -> Result<RunNews, Error>` never ends by itself.** `RunNews::Stopped`, `RunNews::Stalled`, `Ending` and the `ended` flag go: where a run stands is a reading of its frame.
- **`Following::awaits_successor(&self) -> bool`**: the run followed closed promoted and its successor has not come, for at most `SUCCESSOR_WAIT`. Each look asks for the successor; the wait no longer holds a look.
- **Every caller stops by one rule**, `!frame.moves() && !following.awaits_successor()`:
  - `status --follow` reads a stall from its frame's standing and prints the page as it does at the base, else the closing block;
  - the performer ends `Operation::Follow` with `Outcome::Followed` there, so the application stops following where it does at the base;
  - the run stage loses its arms for the two news that went.
- The module's `//!` says a following ends only by its caller's rule or its stop.

**Found in its turn:** `Following::next` hands `Result<RunNews, Error>` and never ends by itself; `RunNews::Stopped`, `RunNews::Stalled`, `Ending` and the `ended` flag are gone. `status --follow` and the performer stop where `!frame.moves() && !awaits_successor()`, and `status --follow` reads a stall off the frame's standing. A look hands the events before the engine's news, so a caller that stops on the events has them. `awaits_successor` stays true while a successor found waits to be handed. The performer ends at once on a run that does not move, where the base ended after one look. The follower listens for words only while the frame says the run moves or asks. A successor that never comes brings no news, so both callers bound `next` by `Following::SUCCESSOR_WAIT`, made public for it: two callers holding one rule of the follower's. The next task gives the follower a piece of news for a wait that ran out, and takes the bound and the public constant away.

**Fails first:**
- `crates/api/tests/following.rs`: `a_run_parked_then_resumed_from_another_process_is_read_on`: a run parks, its pause is handed and the following goes on; another process appends the resume and what follows, which are handed. At the base the following ended at the park.
- `crates/api/tests/following.rs`: `an_engine_killed_shows_as_a_stall_and_the_following_goes_on`: its registry rewritten to a gone pid, with nothing committed, the next `Engine` news names it gone within two seconds of the follower's pace, the frame reads stalled, and `next` still waits.

**Acceptance criteria:**
- [x] `status_follow`'s tests and the captures of `status --follow` equal the base's.
- [x] The application's pty tests (`crates/cli/tests/application/watching.rs` among them) and the walk's counts are the base's.
- [x] No caller reads where a following ended from a news.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api --test following`, `cargo test -p yunta --test status_follow --test application`, `cargo test -p yunta-ui --test run_stage --test walk`

**Dependencies:** Task 13

**Files likely touched:** `crates/api/src/follow/following.rs`, `crates/api/src/follow/followed.rs`, `crates/api/tests/following.rs`, `crates/cli/src/commands/status/follow.rs`, `crates/ui/src/performer/run.rs`, `crates/ui/src/stage/run.rs`

**Estimated scope:** M (the follower's end and its three callers, the compiler naming every caller)

### Task 15: what is followed, and the run read whole handed its events

**Description:**
- **`Followed { run: Option<RunId> }`** (the field joins in Task 17), `Debug, Clone, PartialEq, Eq`; **`Project::follow(&self, Followed) -> Result<Following, Error>`**; **`Following::refollow(&mut self, Followed)`**, which follows the new run from here, keeping what it holds.
- **`FollowNews::{Whole { run, events }, Run { run, news }}`**, what `next` hands, `Debug, Clone` as `RunNews` is; **`RunNews::Successor { run, events }`**: the successor read whole.
- **`next` may be dropped between two looks and loses no news**: what a look read waits in the following's queue until it is handed, so a caller cancels `next` to call `refollow`. `Following`'s `///` says so.
- **`FollowedRun { run, manifest, state, engine, menu }`** loses `events`. `menu` is whether it offers a menu, worked out once for each head; `FollowedRun::offers_a_menu` is deleted for it. **`FollowedRun::took(&mut self, &RunNews) -> Vec<Moment>`** folds `Events` with `RunState::chronicled`, hands their moments back, and works the menu out at the new head.
- `Following::{followed, dir, registry, log, frame}` go.
- **Every caller moves, the compiler naming each:**
  - `status --follow` follows `Followed { run: Some(id) }`; it keeps the `FollowedRun` of `Whole` and folds each batch with `took`; it seeds its surface with `Whole`'s events and its prior with their first instant; it takes its log from `Project::event_log`, its registry from `OpenedRun::registry`, its closing places from `Project::run`, its words from `Said`, and a successor's events from `Successor`. What the following remarks beside a run's events, `status --follow` says through the CLI's remarks (`crate::remarks::say`), as `yunta status` says its own, never on its surface; the one such remark, a manifest's unknown keys, the following hands from Task 20. It prints what it prints at the base.
  - The application: `News::Following` carries the `FollowNews` the follower hands, and `News::Run` is deleted (`crates/ui/src/news.rs`). The performer hands each and ends at the rule on its own copy. The run stage keeps the `FollowedRun`, takes its moments from `chronicle(&events)` on `Whole` and from `took` on each batch, and no longer folds the whole log again on every news. A successor stands in its run's place, as at the base.
- The `//!` of `follow` and of the run stage say the one fold.

**Found in its turn:** `Followed { run }`, `Project::follow(Followed)` and `Following::refollow` stand; the news are `FollowNews::{Whole, Run}`, with `RunNews::Successor { run, events }` and `RunNews::NoSuccessor`, handed once when a run that closed promoted has waited its time and none came. `SUCCESSOR_WAIT` is private and no caller bounds `next`. `FollowedRun` holds no events and keeps `menu`, worked out on a frame at the head's last event; `took` hands back the batch's moments. `Project` and `PromotionChain` derive `Clone`: a look reads a refollowed run whole, and asks the chain on a copy, so a look dropped loses no successor. `dropping_next_between_looks_loses_no_news` holds a queue the base already had, so its red is the missing names alone. `status --follow` reads `Project::run` for its registry and its closing places, two more reads of the log at its start. No interface keeps following past `NoSuccessor` yet. The probe's `whole` follows `Followed { run }` and measures 21.3 ms at ten thousand calls. `follow/following.rs` stands at 494 lines and `stage/run.rs` at 483: their next additions take a submodule.

**Fails first:**
- `crates/api/tests/following.rs`: `the_run_read_whole_is_handed_every_event_from_its_first_once`: `Whole`'s events equal the store's log, and the next batch is only what came past; `refollow_onto_another_run_hands_it_whole`; `dropping_next_between_looks_loses_no_news`: a `next` dropped while a look's batch waits, then `next` again, hands that batch whole and once.
- `crates/ui/tests/run_stage.rs`: `news_of_events_grows_the_chronicle` hands `Whole` cut at the first event, then a batch, and the chronicle grows by the batch alone; handed the batch again, it does not grow.

**Acceptance criteria:**
- [x] `status --follow`'s captures, `status_follow`'s tests and `followers.py` against the base's lines equal the base's.
- [x] The application's pty tests pass as they stand; the walk's counts are the base's, or each difference is named with its cause.
- [x] No `FollowedRun` holds an event; `News::Run` and `FollowedRun::offers_a_menu` stand nowhere in `crates/`.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api --test following`, `cargo test -p yunta --test status_follow --test application --test interface`, `cargo test -p yunta-ui`

**Dependencies:** Task 14

**Files likely touched:** `crates/api/src/lib.rs`, `crates/api/src/follow.rs` (new) and `crates/api/src/follow/followed.rs` (new) and `crates/api/src/follow/following.rs` (new), each moved there by Task 12, `crates/api/tests/following.rs`, `crates/cli/src/commands/status/follow.rs`, `crates/ui/src/news.rs`, `crates/ui/src/performer/run.rs`, `crates/ui/src/stage/run.rs`, `crates/ui/tests/run_stage.rs`, `crates/ui/tests/scroll.rs`, `crates/ui/tests/decision_stage.rs`, `crates/ui/tests/palette.rs`, `crates/cli/tests/interface/run.rs`

**Estimated scope:** L (every caller of the follower in one step, the compiler naming every caller: the review's first finding named eight of these files; `status --follow`, `decision_stage.rs`, `palette.rs` and the CLI's interface test complete it)

### Task 16: a slow reader told how many words it missed

**Description:**
- **`RunNews::Said { said, missed }`** in place of `RunNews::Said(Said)`: `Words::next` (`crates/api/src/follow.rs`) keeps the `missed` of the engine's `Heard`, which the base drops, and the following's channel of words (`follow/listening.rs`) carries it to the news.
- `status --follow` hands `said` to its surface as at the base and lets `missed` pass; the run stage reads `said` as at the base. Phase 2 keeps the count.

**Found in its turn:** `RunNews::Said { said, missed }` carries the count the engine puts in its `Heard` when a reader falls behind, which the base threw away; `Words::next`, the words' channel and the following's receiver carry the whole `Heard`. The test says a thousand words on a runtime of one thread, where nothing reads meanwhile, and holds that some were missed and that those handed and those missed are the thousand; with the follower made to hand `missed: 0` it goes red. The words' tests stand in `crates/api/tests/following/said.rs`, since with the new one `tests/following.rs` would pass the size a smell counter holds. The file near that size is the test file, at 457 lines; `follow/following.rs` itself stands at 441.

**Fails first:** `crates/api/tests/following.rs`: `a_slow_reader_is_told_how_many_words_it_missed`: more words than the engine's queue keeps, said while the reader does not read, arrive with `missed` above zero. Red before for want of the field.

**Acceptance criteria:**
- [x] `status_follow`'s tests (`said.rs` among them) and the captures of `status --follow` equal the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api --test following`, `cargo test -p yunta --test status_follow`, `cargo test -p yunta-ui --test run_stage`

**Dependencies:** Task 15

**Files likely touched:** `crates/api/src/follow.rs`, `crates/api/src/follow/listening.rs`, `crates/api/src/follow/following.rs`, `crates/api/tests/following.rs`, `crates/cli/src/commands/status/follow.rs`, `crates/ui/src/stage/run.rs`

**Estimated scope:** M (one field from the engine's `Heard` to the news, and its two readers, the compiler naming every caller)

### Task 17: the field followed

**Description:**
- **`Followed` gains `field: Option<FieldReach>`**; `status --follow` and the performer name `field: None`.
- **`crates/api/src/follow/field.rs`**, the field's side of a look:
  - on the first look, and on a `refollow` to another reach, the field read whole arrives as one `FollowNews::Field(Box<FollowedField>)`, `FollowedField` being `Debug, Clone`: each run of it at its head as a `FollowedRun`, those it cannot read with why, how many it leaves to other repositories, and whether it is this repository's. Which repository is here is asked of git once for each reach;
  - then each look: `moved()`, and only after a commit `heads()` and the events past each moved run's head, handed as `FollowNews::Run { news: RunNews::Events }` and folded into its state; a run new to the log is read and handed `Joined`, or counted elsewhere once and read no further, the count handed as `Elsewhere`; a run gone from the heads is `Left`;
  - once a second, the registry of each run whose log says it moves or asks, its change handed as `RunNews::Engine`; every ten seconds, that each run held still has its directory;
  - **errors stay with their run**: a run whose log or manifest fails a read is `Unreadable`, with why, and read again whole when its head moves; a run whose directory went is `Unreadable` until it leaves the log. The rest go on.
- `status --follow`, the performer and the run stage follow no field: each lets the field's arms pass.
- `follow`'s `//!` says a look's paces and what each costs.

**Found in its turn:** `Followed { field, run }`; `follow/field.rs` holds the field's side of a look and `FollowedField`; the news gain `Field`, `Joined`, `Unreadable`, `Left`, `Elsewhere`. A run read whole that is also in the field is one run to the follower: its events are read once and handed once, and whichever side reads it first hands the other its copy. "Never disagree" is two tests, one for each side of that hand-off, each red under its own mutation. An unreadable run is read again when its head moves. The field reads a run's directory and manifest before its log, so a run of another repository costs no statement. `Here` is visible across the crate, so runs are placed by one rule. `FollowedRun::took` on a `Successor` replaces the run, so a field's row never folds one. `RunState`'s equality leaves the head out, so the tests compare heads too. No test plants a log that fails a read. `follow/following.rs` and `follow/field.rs` stand at 481 lines each, `stage/run.rs` at 489.

**Fails first:** `crates/api/tests/following/field.rs` (new) and `crates/api/src/follow/tests.rs`, on real logs written by benches, red for want of the field:
- a store where nothing moves costs one statement a look, with three runs held;
- followed in the field, each run's state equals the replay of its log at every look;
- an engine killed is handed as its run's `Engine` news within two seconds, with nothing committed;
- a run that cannot be read is said with why and the rest go on;
- a collected run passes through its missing directory and leaves (collection's two passes: its directory, then its rows);
- a run parked then resumed from another process is read on past its resume.

**Acceptance criteria:**
- [x] The first piece of news holds the whole field; no frame crosses the API.
- [x] `status_follow`'s tests and the application's pty tests pass as they stand.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api --test following`, `cargo test -p yunta-api follow::`, `cargo test -p yunta --test status_follow --test application`, `cargo test -p yunta-ui`

**Dependencies:** Task 16

**Files likely touched:** `crates/api/src/follow.rs`, `crates/api/src/follow/field.rs` (new), `crates/api/src/follow/following.rs`, `crates/api/src/follow/tests.rs`, `crates/api/tests/following.rs`, `crates/api/tests/following/field.rs` (new), `crates/cli/src/commands/status/follow.rs`, `crates/ui/src/performer/run.rs`, `crates/ui/src/stage/run.rs`

**Estimated scope:** L (the field in `follow`; its three callers outside the API only name `field: None` and let the field's arms pass, so they move as one with it, the compiler naming every caller)

### Task 18: a field's row is a followed run's

**Description:**
- **`FollowedRun::row(&self, now) -> FieldRow`** (`crates/api/src/follow/followed.rs`): its frame at `now` with its menu.
- `Project::field` (`crates/api/src/field.rs`, for `yunta list --runs` and `find`) makes its rows through it, from runs read as the follower reads them, so a row is made one way for every interface.

**Found in its turn:** `FollowedRun::row(now)` hands the run's frame at `now` and its menu, and `Project::field` reads each run as the follower does (its directory, its manifest, then its log, the log only where the manifest places the run here) and makes its rows with it; `run_row` is gone. The follower and the listing share one reading of where a run stands, `Placed::of` in `crate::field`. The JSON lists rows in run-id order, which for the ids Yunta makes is the order they were created in. A run whose log fails a read and whose manifest places it in another repository counts as elsewhere, where the base placed it by its branch. The test of a log that fails a read plants one with `Storage::corrupt_event_payload`, behind the store's `testkit` feature, which `yunta-api`'s dev-dependencies turn on as the CLI's and the interface's do: a run broken before the first look comes unreadable in the `Field` news, one broken after comes as `Unreadable`, and the third is read on each round. It passed at once, the field's side already keeping a fault with its run, and went red with the read made to pass the error up. The captures of every command equal the base's.

**Fails first:** `crates/api/tests/following/field.rs`, red for want of `row`:
- a run's row and its whole reading agree at every look: followed in the field and as the run read whole, its row equals `Project::field`'s at one instant;
- an engine killed reads stalled in its row, with nothing committed;
- a run parked then resumed from another process stands in its new group.

**Acceptance criteria:**
- [x] `yunta list --runs` prints what it prints at the base (captures).

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api --test following`, `cargo test -p yunta --test run_inbox_cmd --test stalled_runs`

**Dependencies:** Task 17

**Files likely touched:** `crates/api/src/follow/followed.rs`, `crates/api/src/field.rs`, `crates/api/tests/following/field.rs`

**Estimated scope:** S

### Task 19: what a run asks, read once for each pause

**Description:**
- **`Awaited::{Decision(Box<WaitingDecision>), Round(Box<WaitingQuestions>)}`** in `crates/api/src/answers.rs`, beside the two it sums, `Debug, Clone`. Its `///`: "Not `WaitingOn`, where a run is parked."
- `OpenedRun::waiting_decision` and `waiting_questions` read through two functions of a run's manifest, directory and state (and, for a round, its events), which the follower calls with the state it holds.
- **The follower keys what the run read whole asks by its pause**: the node `awaits_decision` names with the pause's seq, or a round's node with the seq of its questions. It reads once for each key, on a task of its own, off the look's path, and hands `RunNews::Awaited(Some(..))`. It hands `Awaited(None)` once the run waits on none, and when a read finds no decision where the log said one waits: it is answered. A reading that fails on a shown document's shape hands `RunNews::AwaitedUnread { why }`, and the following goes on.
- `status --follow` and the run stage let the two arms pass: the CLI's prompts and the decision stage read what a run asks as at the base.

**Found in its turn:** `Awaited::{Decision, Round}` stands in `answers.rs`; `WaitingDecision::read` and `WaitingQuestions::read` hand an `Option`, and the two readings a command makes go through them, printing the same. `follow/awaited.rs` works a pause's key out only when the held run's head or identity changes, and reads each new key on a task of its own through the following's reader, now shared; a read whose key changed meanwhile is dropped. The state holds no seq for a round's questions, so a round is keyed by its node and how many rounds that node asked. A key needs a standing pause and a frame that waits on a person, so nothing is read while the engine asks at its own terminal. Every read that fails is `AwaitedUnread`, a missing object and a log's error among them, or the following would end. `Awaited(None)` is handed only after something awaited was handed for that run; a run newly held starts with nothing awaited. The news carry no key of the pause, which an interface needs to keep what a person typed by it: Task 19a hands it. `follow/following.rs` stands at 491 lines, `stage/run.rs` at 494, `lib.rs`'s `//!` at its 60.

**Fails first:** `crates/api/tests/following.rs`, red for want of `Awaited`:
- `what_a_run_asks_is_read_once_for_each_pause`: a run parked on a gate, followed over many looks and an event that is no pause, hands one `Awaited`; a second pause hands a second;
- `a_decision_answered_elsewhere_reads_as_answered`: an answer recorded by another process hands `Awaited(None)`;
- `a_shown_document_that_fails_its_shape_arrives_unreadable`: a spec broken in the run's objects hands `AwaitedUnread`, and the next news still comes.

**Acceptance criteria:**
- [x] `waiting_questions`' tests and `resolve-gate`'s pages pass unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api --test following --test waiting_questions`, `cargo test -p yunta --test gate_review_cmd`

**Dependencies:** Task 18

**Files likely touched:** `crates/api/src/answers.rs`, `crates/api/src/lib.rs`, `crates/api/src/follow/following.rs`, `crates/api/src/follow/awaited.rs` (new), `crates/api/tests/following.rs`, `crates/cli/src/commands/status/follow.rs`, `crates/ui/src/stage/run.rs`

**Estimated scope:** M (two new arms reach the two matches on the news outside the API, the compiler naming every caller)

### Task 19a: what a run asks names its pause

**Description:**
- The follower keys what a run asks by its pause (`crates/api/src/follow/awaited.rs`) and hands the reading without the key, so an interface that keeps what a person typed by the pause would have to work the same key out again. The key is the follower's to say: the news of what a run asks carry it, on `RunNews::Awaited`'s reading and on `RunNews::AwaitedUnread` alike.
- The key is a public type of `yunta-api`, named for what it is: one pause of a run a person can answer, the gate or the round it stands on. Its `///` says what makes two keys equal, and that a run parked again on the same pause keeps its key. It is `Debug, Clone, PartialEq, Eq`.
- `status --follow` and the run's stage read through it and do what they do.

**Found in its turn:** the key is `AnswerablePause { run, on }`, its fields private, so no code but the follower makes one; `RunNews::Awaited(Option<(AnswerablePause, Awaited)>)` and `RunNews::AwaitedUnread { pause, why }` carry it. A round parked again keeps its key; a decision's key holds the seq of the run's pause, so a run parked again on a decision is a new pause with a new key, and the `///` says both. The three callers match the arms without reading inside them and compile unchanged. Two refollows with no look between them (one run, another, the first again) never hand the first run's awaited again, since what is asked still holds that run at its head: Task 19b.

**Fails first:** `crates/api/tests/following/awaited.rs`: a run parked on a gate, answered, and parked on a second gate hands two keys that differ; the first gate read again after a refollow hands the key it handed. Red: the news hold no key.

**Acceptance criteria:**
- [x] No caller works a pause's key out but the follower (`grep` finds the key's making in `follow/awaited.rs` alone).
- [x] `crates/api/tests/public_items.rs` passes: the key names no type below the API.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api --test following --test public_items`, `cargo test -p yunta --test status_follow`, `cargo test -p yunta-ui --test run_stage`

**Dependencies:** Task 19

**Files likely touched:** `crates/api/src/follow.rs`, `crates/api/src/follow/awaited.rs`, `crates/api/src/lib.rs`, `crates/api/tests/following/awaited.rs`, and each match on the two arms, which the compiler names (`crates/cli/src/commands/status/follow.rs`, `crates/ui/src/stage/run.rs`, `crates/api/src/follow/followed.rs`)

**Estimated scope:** M

### Task 19b: a run held again is asked again

**Description:**
- Two refollows with no look between them (one run, then another, then the first again) leave the first run's awaited news unhanded: what the follower keeps of what is asked still holds that run at its head, so it takes the run as already asked and hands nothing, and an interface that dropped what it kept when the run left its view waits for news that never come.
- `refollow` forgets what was asked of the run it lets go, so a run held again is read and handed again. The forgetting is `awaited.rs`'s own, called where `refollow` lets a run go.

**Found in its turn:** `Asking::forget` clears what was worked out for the run let go and starts a new turn, which stops the read under way and drops what an earlier read hands later: without the new turn a read of the first run could land after the second's look and be handed as the second's. `ask` and `forget` share `Asking::next_turn`. At the base of the change the test's news never came within its ten-second guard. The test also holds that a refollow to the run already held hands nothing new: after the answer the next news is `Awaited(None)`; a mutant that forgets on every refollow fails it, through the guard. `follow/following.rs` stands at 492 lines.

**Fails first:** `crates/api/tests/following/awaited.rs`: a run parked on a gate is followed and its awaited handed; the following refollows another run and the first again with no look between; the first run's awaited is handed again, with the key it had. Red: no news come.

**Acceptance criteria:**
- [x] The test passes, and a refollow to the same run it holds hands nothing new.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api --test following`

**Dependencies:** Task 19a

**Files likely touched:** `crates/api/src/follow/awaited.rs`, `crates/api/src/follow/following.rs` (one line), `crates/api/tests/following/awaited.rs`

**Estimated scope:** S

### Task 20: the documents a node hands over, and the unknown keys said once

**Description:**
- **`Document::{Read(ShownDocument), Unread { shown, path, why }}`**, in `crates/api/src/follow/document.rs`, `Debug, Clone`: a document a node handed over, as the follower read it. Its `///`: "Not `ShownDocument`, what a decision shows."
- For the run read whole, each document one of its nodes hands over is read once, when the event that hands it over is folded, and handed as `RunNews::Document { node, document }`; the documents handed before the run was read whole are handed with it, each once. They are read through the engine's reading of a node's produced documents, split at the document, so one that fails today's shape is `Unread` in its place and the rest are read (`crates/engine/src/artifacts/shown.rs`). A task's title comes from the tasks document.
- A manifest's unknown keys are said once for each run the follower reads, as `RunNews::Remark(Remark::UnreadManifestKeys { .. })`, never at each look.
- `status --follow` says the remark through the CLI's remarks (`crate::remarks::say`), as `yunta status` says it, and lets `Document` pass; the run stage lets both pass, and Phase 3 says the remark once as a note.

**Found in its turn:** `Document::{Read, Unread}` stands in `follow/document.rs`, with `RunNews::Document { node, document }` and `RunNews::Remark`. The documents handed over before a run is read whole come right after `Whole`, each once; a later one comes once a look folds the event that hands it over, and a look works them out again only after the head moves. Each is read on its own, so one that fails its shape comes `Unread` and the rest are read; a document stays owed until it is read, so a look dropped half way loses none, and `refollow` forgets what was owed of the run it lets go. The engine's reading is split at the document (`documents_handed_over`, `produced_document`), and the node's page reads through the same two, printing the same. The remark is said for the run read whole and for each successor, never for the field's runs, and again when that run is read whole again. A plan read on its own is not judged beside its spec, as the node's page judges it. The remark of a manifest's unread keys is built in two places, `run/page.rs` and `follow/document.rs`: the next task gives it one. `follow/following.rs` stands at 498 lines and `stage/run.rs` at 497.

**Fails first:**
- `crates/api/tests/following.rs`, red for want of the news:
  - `each_document_a_node_hands_over_is_handed_once`: a planner's tasks document arrives once, its tasks' titles in it, and no further look hands it again;
  - `a_handed_document_that_fails_its_shape_arrives_unread`;
  - `a_manifests_unknown_keys_are_said_once_for_each_run_read`.
- `crates/cli/tests/status_follow/edges.rs`: a moving run whose manifest holds a key today does not read, followed, says it once in the words `yunta status` says it with. At the base `status --follow` of a moving run says nothing of it.

**Acceptance criteria:**
- [x] `yunta status <run> <node>` prints what it prints at the base (captures); the captures of `status --follow` equal the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api --test following --test run_page`, `cargo test -p yunta-engine artifacts`, `cargo test -p yunta --test status_follow`

**Dependencies:** Task 19

**Files likely touched:** `crates/api/src/lib.rs`, `crates/api/src/follow/following.rs`, `crates/api/src/follow/document.rs` (new), `crates/engine/src/artifacts/shown.rs`, `crates/api/tests/following.rs`, `crates/cli/src/commands/status/follow.rs`, `crates/cli/tests/status_follow/edges.rs`, `crates/ui/src/stage/run.rs`

**Estimated scope:** M (two new arms reach the two matches on the news outside the API, the compiler naming every caller)

### Task 21: one fold for every reader, and a reader that only reads

**Description:**
- **The follower agrees with `yunta status` on every log the suite holds**, in `crates/api/tests/following.rs`: each log under `crates/replay/tests/fixtures/logs/`, and a broken one, loaded into a store with a manifest naming its nodes. The state the follower holds, the state `Whole`'s events fold to, and `RunState::replay` of the log are equal; its frame equals `Project::run`'s at one instant.
- **The reader only reads**: `two_followers_change_nothing` (`crates/cli/tests/status_follow/edges.rs`) holds the one `Following` that `status --follow` and the application both use; `LogReader`'s refusal (Task 4) holds its connection.
- **`.agents/architecture.md`**: the follower's rule says that past a first open, which sets a log up as every open does, a follower takes no lock, writes nothing and signals no engine, and that every reader folds a log by replay's one fold. Its bytes come out of what the file says twice or a `//!` already says; without such a sentence the work stops for the user rather than raising the cap.

**Found in its turn:** `crates/api/tests/following/kept_logs.rs` loads each kept log into a store of its own, and one broken past its middle by a node terminal that names no node, reads it whole from its middle, on in three batches, and whole again: the run kept, the run read whole again, the state its events fold to and `RunState::replay` are equal at one head, and their frames equal the frame a command reads. It passes at once, the follower folding by replay's fold since the fold landed, and goes red with `took` made to fold past a break. The remark of a manifest's unread keys is built by `run::unread_keys`, beside `read_manifest`. The follower's rule in `.agents/architecture.md` costs 129 bytes; three clauses the file says elsewhere make its room (who asks for a run, which the crates' table says; that what a person reads is drawn in Surco, which its product rule says; that replay is a pure fold, which the new rule and replay's `//!` say), and the file stands at 6,138 of its 6,144 bytes: the user reviews the cuts at the checkpoint.

**Fails first:** the agreement test, seen red with `took` made to fold past a break, as the base's follower did.

**Acceptance criteria:**
- [x] `cargo xtask docs --check` passes with the cap unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api --test following`, `cargo test -p yunta --test status_follow`
- [x] Build succeeds: `cargo xtask docs --check`

**Dependencies:** Task 20

**Files likely touched:** `crates/api/tests/following.rs`, `.agents/architecture.md`

**Estimated scope:** S

### Group check: the follower
- [x] Every suite group green; captures equal the base's but `session` on calls; the application's pty tests and the walk's counts against the base's; `followers.py` on a `status --follow` of a held run against the base's.

*In parallel:* Task 22 first; then Task 23 and Task 24, which touch disjoint files; then Task 25.

**Found in its check:** 315 binaries and 4,914 tests, none failing: the base's by name but the one test of a following that ended at a stall, which the test of a following that goes on replaces, three moved into submodules and fifty new. 247 goldens unchanged. `prints.sh`'s 159 views equal the base's (`prompt-review` by its known race, equal in two of three retakes), the receipts byte for byte. The application's 29 pty tests pass. The walk's counts are the base's (22,388 states; 5,145 screens at each size; 30 palettes; 70 helps). A run of ten thousand calls is read whole in 21.5 ms against the base's 3,948; a drawing takes 0.202 ms against 0.202.

### Task 22: what recording a run needs

**Description:**
- **`SteppingClock`** in `crates/testkit-core/src/clock.rs`: each reading of `now` a fixed step after the last, from a fixed instant, and its awake time likewise, so times on a recorded run have the widths real ones have. Not `RushingClock`, whose wall stands still.
- **`Bench::answer_parked(&self, option, said)`**, the write `yunta resolve-gate` makes on a parked run, moved from `crates/engine/tests/common/mod.rs` (`answer_parked`, `answer_parked_saying`) into `crates/testkit/src/bench/driving.rs`; every engine test that called it calls the bench's.
- **`Bench::answer_round(&self, node, answers)`**: the answers to a parked round recorded as `yunta answer` records them, through the engine's `answer_questions`.

**Found in its turn:** `SteppingClock::by(step)` stands in `yunta-testkit-core`: each reading is one step after the last, the wall and the awake time moving together, so the host never reads as asleep. `Bench::answer_parked` is the engine tests' helper moved, signing as `mcp` and stamping with the bench's clock; `Bench::answer_round` answers a round as `yunta resolve-gate` with no option does, on the `tty` channel, signed `unverified:<user>` (there is no `yunta answer`). Both stand in `bench/answering.rs`, since `bench/driving.rs` is at 495 lines. Twenty caller files call the bench, and three test binaries that took `common` for this helper alone no longer declare it; the callers' 159 tests pass by the same names. A clock that steps at each reading does not make two recordings equal: the engine reads the time once every real second while a run is driven and at every append, so an event's time would hang on how long a wake takes. The recording's clock is therefore one the recording itself moves, between the words it gives: Task 23 makes it and takes `SteppingClock` away if nothing else reads it.

**Fails first:** `crates/testkit-core/src/clock.rs`'s tests: two readings of a stepping clock stand one step apart. The move has no new behavior: the engine's tests that answer a parked run pass unchanged, by name.

**Acceptance criteria:**
- [x] `crates/engine/tests/common/mod.rs` holds no answering of a parked run.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-testkit-core clock`, `python3 .plans/yunta-tui/tools/suite.py engine-1` to `engine-5`

**Dependencies:** Group check: a session held by a word, Group check: the follower

**Files likely touched:** `crates/testkit-core/src/clock.rs`, `crates/testkit/src/bench/driving.rs`, `crates/engine/tests/common/mod.rs`, and the twenty engine tests that answer a parked run, the compiler naming every caller

**Estimated scope:** L (a mechanical move of one helper, the compiler naming every caller)

### Task 23: the design's run, recorded and kept

**Description:**
- **`yunta_testkit_ui::designed`** (`crates/testkit-ui/src/designed.rs`): a workflow shaped as the design's, with fragua's node ids and kinds (`grill` asking a round, `brief`, `plan` handing tasks over, `spec`, `approve-plan` showing the plan, `implement` a loop with one task, `review` fanned out three ways and working at once beside `conform`, `fix-findings`, `lint`, `fix-lint`, `tests` a check, `ship` a gate, `pr`), on the mock adapter with `max_parallel_nodes: 3`. It is played through a round answered, the plan approved, a scope asked and granted, the design's four findings (a blocking, two major, a minor) posted and answered, a check that fails and is retried, the last gate, and on to its end through a forge the bench fakes.
- **It is recorded so that two recordings give the same log** (plan section 8):
  - each session's steps are held by `awaits_word` and let go in a fixed order, each next word given once the event the last one let go is on the log;
  - the clock is a `SteppingClock`;
  - each pause is a park, `answer_parked` or `answer_round`, then a wake, never an answer at the engine's own terminal.
- **Kept** under `crates/testkit-ui/fixtures/designed/`: its workflow, config and mock fixture; its log as JSONL; its run directory and its objects.
- **`crates/testkit-ui/tests/designed.rs`**: records it again in a fresh place and holds the log equal to the kept one, so a change to what the engine writes fails here. Run with `YUNTA_BLESS=1`, it writes the kept fixture: `.plans/yunta-tui/tools/record_designed.sh` is that command.
- `yunta-testkit-ui`'s `//!` names `designed`; its manifest gains `serde_json`, an external crate, and no workspace edge.

**Found in its turn:** `yunta_testkit_ui::designed` records fragua in `standard` mode on the mock adapter, three nodes at once: 276 events, kept under `crates/testkit-ui/fixtures/designed/` (288K), a recording taking some 18 s; `record_designed.sh` records it again. Sessions are held by words; the suite and lint by a `cargo` stub the recording arms; the three reviewers each by a `git` stub on the first `git` call they make, without which they swapped scripts and session ids between recordings; gates are parked, answered and woken; `pr` opens on a fake forge. The clock is `HostClock`, which stands until moved, and `SteppingClock` is gone; events between two moves share an instant. Before keeping, the place's paths, the pool's name, the machine's temp dir and the manifest's hash are written one way. Two recordings give the same bytes: 20 runs of 20, twice. Every kept `duration_ms` is 0 and truly so, none being a check held while the clock moved; the failed suite's time, 32 s, is its node's span on the log, which a test holds. The moments, by seq: the round asked 18 and answered 21; `plan` at 3 calls 42 and at 7 calls 48; the tasks accepted 51 to 54; `approve-plan` parked 78; `implement` at 8 and 12 calls 99 and 104; the scope asked 114 and 146; three reviewers at work 184 to 203; the findings answered 223 to 230; lint sent back 234; `tests` failing 251; `ship` 263; the pull request 274; finished 276. What the log cannot hold of the design's run: a note's and a summary's words (kept as a size and a digest), a command whole, the adapter's name (`mock` where the design shows codex), `approve-plan`'s ten minutes (the run's pause; the gate itself works no time), the diff's size (7 files, +68 −25), the pull request's number; the retry is offered only once a wake finds the tree edited.

**Fails first:** `designed.rs`: no kept log stands, then two recordings are compared; any difference between them is a hold the recording lacks, fixed before the log is kept.

**Acceptance criteria:**
- [x] Two recordings are equal byte for byte; three reviewers are at work at one seq of the log; no two events stand at one instant.
- [x] The kept run finishes, and its receipt holds criteria, a baseline, a scope, findings and a pull request.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-testkit-ui --test designed` (twice), and under `repeat_target.py` 20 times

**Dependencies:** Task 22

**Files likely touched:** `crates/testkit-ui/Cargo.toml`, `crates/testkit-ui/src/lib.rs`, `crates/testkit-ui/src/designed.rs` (new), `crates/testkit-ui/fixtures/designed/` (new), `crates/testkit-ui/tests/designed.rs` (new), `Cargo.lock`

**Estimated scope:** L

### Task 24: the small runs, and three past runs

**Description:**
- **`yunta_testkit_ui::small_runs`** (`crates/testkit-ui/src/small_runs.rs`): one function for each scene of the spec's table the design's run does not pass through, each a bench run with a fixed id under the stepping clock, built as the engine's own tests build that state:
  - a gate with no plan, `Menu::Console`; a departure; a token cap; a loop cap; an escalation that carries no menu;
  - a run that wakes alone (`AwaitingReset`); a pull request (`ExternalGate`, `Menu::Published`); a child parked (`ChildPaused`);
  - no room; orphans; blocked; a stop (`Cancelled`) and a gate aborted;
  - a stall (a registry naming a gone pid) and a run asked at another terminal (a registry naming a live pid with its prompt);
  - a failure with its menu; a finished run that reports blocking findings; a promotion with its successor; a broken log; a run created, no node started;
  - a loop with two tasks at work at once, their calls naming their sessions, its sessions held by `awaits_word` and let go in a fixed order, as the design's run is recorded;
  - a run in a mode that leaves a node out;
  - a plan gate answered `adjust`, which sends `plan` back (E16's mark);
  - a round whose first answers are refused (`AnswersRefused`), then answered;
  - a gate whose shown document fails today's shape, so its pause is read as `AwaitedUnread`.
- **`small_runs::past`**: three finished runs of one workflow of the test home's repository in one mode, with different spend and times, so a new run of it says what it usually takes.
- **`crates/testkit-ui/tests/small_runs.rs`**: each built twice gives the same log; each stands where its scene needs, its phase, pause and menu asserted.

**Found in its turn:** `yunta_testkit_ui::small_runs` builds 27 runs in five modules (`asking`, `standstills`, `waiting`, `ends`, `work`) and `past()`, three finished runs of one workflow; each builds the same twice (20 runs of 20, the loop with two tasks at once among them). None writes a registry: a cut writes the engine fact its scene needs. A run parked on a departure owed or on a loop's cap is rebuilt by the engine as a failure's menu with that cause; `Menu::Departure`, `Menu::LoopCap` and `LoopOverrun` come only where an engine asks at its own terminal, and the spec's two rows say so. `Blocked` is written by hand, no path of the engine reaching it. `abort` on a failure parks the run on that failure again and never closes it failed, as the engine's own test holds, so `ends::failed` runs with `defaults.on_failure: abort`. `PullRequestClosed`, `CancelledAfterCrash` and `Recorded` have no run: each shares its scene with a row that has one. The design's recording gives its place, its timeline and its kept log to the small runs (`designed/{place,script,kept}.rs`); `Bench::successor` makes a promotion's successor.

**Fails first:** `small_runs.rs`, red for want of the module; each run's stand is asserted before it is used.

**Acceptance criteria:**
- [x] Every row of the spec's scenes table is reached by the design's run or a small run; the list of which is written at the module's `//!`.
- [x] The runs the design's run does not reach stand as their targets need, each asserted by name: two tasks at work at one seq, each call naming its task's session; a node the mode leaves out standing skipped; `plan` sent back after `adjust` and at work again; `AnswersRefused` on the log, then the round answered; a gate whose shown document reads as unread.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-testkit-ui --test small_runs`, and under `repeat_target.py` 20 times, the loop with two tasks at once among them

**Dependencies:** Task 22

**Files likely touched:** `crates/testkit-ui/src/lib.rs`, `crates/testkit-ui/src/small_runs.rs` (new), `crates/testkit-ui/tests/small_runs.rs` (new)

**Estimated scope:** M

### Task 25: a scene is a cut log followed, and the dumps

**Description:**
- **`yunta_testkit_ui::cut::Cut`** (`crates/testkit-ui/src/cut.rs`): one run or several, each run's log cut at one of its moments, loaded into one fresh store at each event's own seq and instant, beside each run's directory and objects, with the registry each needs written (none, a gone engine, a live one, one asking at its terminal), and the `Project` over it, which a test follows with `Project::follow`. Several runs in one store are a field at its moment. No constructor of news stands beside the follower. The logs are loaded through `yunta-testkit`, which reaches the store.
- **`cut::TARGET_CUTS: &[TargetCut]`**, the committed table: for each target screen of Task 35 to Task 41, by its name, the runs it shows (each a run `designed` or `small_runs` builds, and its seq; none for a page that shows no run), the registry, the node pointed at and the keys pressed. **`Cut::of_target(name)`** loads a target's cut from it. `TargetCut`'s `///`: "Not `Cut`, the store a target's cut loads." Both names are free in `crates/`.
- **`crates/testkit-ui/tests/cut.rs`**: the design's run cut at the plan's gate and followed hands a `Whole` whose state equals the replay of the log to that seq, and `Awaited` the plan's decision; at every tenth seq of the design's run, the follower's state equals the replay of the cut; the design's run and two small runs loaded at their moments into one store and followed as the field hand one `Field` holding the three at their heads; every row of `TARGET_CUTS` names a run that is built and a seq its log holds.
- **The probe's `dump`** writes, for each cut, the facts the follower hands: each node's standing, attempt, time and tokens; the run's phase, pause and times; what it asks, with its question and options; the documents handed over, with the tasks' titles; the ledgers' proof and scope for each task; the findings; the moments. They go to `proof/live-screens/dumps/<run>/<seq>.txt`.
- **`dumps/INDEX.md`** mirrors `TARGET_CUTS`, written from it by `dump`, and adds for each row of the spec's scenes table and each element of its elements table the target whose cut shows it.

**Found in its turn:** `yunta_testkit_ui::cut` loads each run's log up to its seq into a fresh store at each event's own seq and instant, copies its directory, points its manifest at the cut's repository and writes the registry the scene needs (none, gone, alive, asking), each run placed here, elsewhere or unread. The cut's clock stands at the moment the event after the cut came, so a parked run has waited as long as the person took to answer. `TARGET_CUTS` names 101 targets: 83 on a cut and 18 in a place of the home. `yunta-testkit` gains `loaded(store, events)` and `WrittenRegistry`. The probe's `dump` writes 44 run dumps, 3 field dumps and `INDEX.md`, twice equal. `asked-elsewhere` is cut at seq 6: the engine writes `gate_waiting` with its answer. `still-unread` is asked with a flag of its own, `Asked::NotYet`. Nine targets stand in a place the home does not hold whole: `new-run`, `new-run-mode` and `new-run-inputs` (the past runs' workflow declares no input and one mode), `new-run-refused` (nothing there is refused), `workflow-graph` (the home's workflow has one node), and the setup's four with a pack (the home holds no pack for a setup to install). Task 25a builds those places.

**Fails first:** `cut.rs`, red for want of `Cut` and `TARGET_CUTS`.

**Acceptance criteria:**
- [x] `TARGET_CUTS` names a cut for every target of Task 35 to Task 41, among them a cut of each of Task 24's runs of a loop with two tasks at once, a node left out, a node sent back, answers refused and a shown document unread; each cut's dump is written.
- [x] `INDEX.md` names the same cuts as `TARGET_CUTS`, and a target for every row of the scenes table and every element.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-testkit-ui --test cut`
- [x] Manual check: the dumps and `INDEX.md` written twice are equal

**Dependencies:** Task 23, Task 24

**Files likely touched:** `crates/testkit-ui/src/lib.rs`, `crates/testkit-ui/src/cut.rs` (new), `crates/testkit-ui/tests/cut.rs` (new), `crates/testkit/src/events.rs`, `.plans/yunta-tui/proof/live-screens/probe/`, `.plans/yunta-tui/proof/live-screens/dumps/` (outside git)

**Estimated scope:** M

### Task 25a: the places the starting screens stand in

**Description:**
- Nine targets stand in a place the test home does not hold whole, so their targets would be written from no dump. The home gains them, and `TARGET_CUTS` names each:
  - **A repository set up as the design's**: `yunta/fragua` installed as a pack, beside the two workflows the design lists under it (`lint-fix`, `run-tasks`), one of them declaring an input beside the request; and three finished runs of `yunta/fragua` with different costs, so a new run says what it usually takes. `new-run`, `new-run-mode`, `new-run-inputs` and `workflow-graph` stand there. The three past runs are built as cheaply as a finished fragua run can be built the same twice; where the design's kept run serves as one of them, it does.
  - **A new run refused**: the same place with a start that admission refuses for a reason a person can mend there (an input left empty), for `new-run-refused`.
  - **A repository never set up, with its pack in reach**: the reference pack, `packs/fragua` copied into a local repository committed under the binary's version tag, and git pointed at it for the Yunta repository's URL (`url.<local>.insteadOf`, through the workspace's subprocess variables); `setup-audit`, `setup-confirm` and `setup-written` stand there. And one with the pack out of reach, for `setup-unreachable`.
- The reference pack and the environment that points at it are `yunta-testkit`'s, where Phase 4's setup takes them from.
- The probe's `dump` writes what each of those places holds that a screen shows: the workflows with what each does, fragua's modes, its nodes and where it stops for a person, each past run's tokens and time, the adapters the place answers for, who git commits as, the forge, the pack's audit as `yunta pack add` shows it, and the roles the pack's workflows name. `INDEX.md` names each place's dump for its targets.

**Found in its turn:** the home gains three places, opened by `Home::workspace_at`: `SettingUp` (a Rust workspace named `yunta` never set up, `claude` and a `codex` stub answering, git reading `Home::reference_pack` for the Yunta repository's URL), `PackOutOfReach` (the same, git pointed at nowhere) and `WithFragua` (fragua as a pack, `lint-fix`, `run-tasks` with a required document beside its request, the design's config, two past fragua runs in `quick` mode, the design's run finished, one run that needs the person and one stalled). `NeverSetUp` and `ToSetUp` are gone, no target standing in them. The reference pack is `yunta-testkit`'s: `packs/fragua` committed under the binary's version tag with a fixed author and date. The two past runs cost 11.6 s to build, so the home builds in 15.8 s where it took 2.35; with the design's run their median is 2.17M tokens and 1h07m, as the design shows. `INDEX.md` has no target left without what it shows: 78 on a cut, 23 in a place. The catalog lists the repository's own workflows before a pack's, where the design lists fragua first; fragua's own description is not the design's words; with fragua installed the setup proposes five roles where the design shows four; doctor reports the forge's token unset. Phase 4 still makes `Packs::fetch` take a directory inside a repository at a tag (the dump audits a copy at the root meanwhile), fetches with the workspace's git variables, and installs `WithFragua`'s pack from the Yunta repository's directory.

**Fails first:** `crates/testkit-ui/tests/cut.rs`: every row of `TARGET_CUTS` that names a place of the home finds what the row says it needs (a workflow with two modes, one with an input beside the request, three finished runs of one workflow, a pack in reach, a pack out of reach). Red: the places do not hold them.

**Acceptance criteria:**
- [x] No row of `INDEX.md` says its target's cut could not be made whole.
- [x] The three past runs build the same twice, and the home builds in the time it took plus what they cost, said in the report.
- [x] The dumps written twice are equal.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-testkit-ui --test cut --test small_runs`, `cargo test -p yunta-testkit`
- [x] The walk's counts are the base's: `cargo test -p yunta-ui --test walk`, `cargo test -p yunta-tui --test walk`

**Dependencies:** Task 25

**Files likely touched:** `crates/testkit/src/{lib.rs,reference_pack.rs (new)}`, `crates/testkit-ui/src/{home.rs,sandbox.rs,cut/targets.rs,small_runs.rs}`, `crates/testkit-ui/tests/cut.rs`, `.plans/yunta-tui/proof/live-screens/probe/` and `dumps/` (outside git)

**Estimated scope:** M. The home's builder changes with every place it adds; the compiler names what reads it.

### Task 25b: a run's screen is seen in its field, at the instant it stands at

**Description:** three things the first targets showed, each in how a cut is made.
- **A moving run is seen as its last event lands.** `Cut::now()` stands at the moment the event after the cut came, which is right for a run that waits (it has waited as long as the person took to answer) and wrong for one at work: `work`, cut at the plan's third call, reads `for 10m27s` where its last event stands at 7m34s, the design's own time. A run whose engine is alive and whose frame moves is seen at its last event's instant; a run that waits is seen as today.
- **A run's screen stands in a field.** Every cut of a run's screen holds the run alone, so its tally reads `nothing needs you` and no run's screen ever shows the attention row, which the design puts on every screen. Each such cut holds, beside the run in view, one run that needs the person and one stalled, both placed here and both older than the design's run, so their ages are true at every cut: the two the design's own field shows first of each kind (`design/field.wide.txt`: `EZRPE9`, `quick`, its node `grill` failed, 40 days; `EKPBGE`, `standard`, no process driving it, 11 days), built as small runs under a clock that starts that long before the design's run. The tally then counts them and the attention row names the one that waits.
- **A finding that blocks, open.** `findings` is cut where one major finding stands; no target shows a blocking finding before it is answered, whose mark the inventory holds. `findings` is cut where the four findings are posted, the blocking one among them, none answered, the reviewers still at work.
- The dumps and `INDEX.md` are written again: each run's dump says the field it stands in (the tally's counts, the attention row's run with what it asks and how long it waited).

**Found in its turn:** `Stands::Run` holds the run in view and `Stands::runs` adds the two beside it, so a run's screen is never built without its field. `small_runs::beside::not_logged_in` is `EZRPE9`, a fragua run in `quick` whose `grill` session was refused, parked on that failure; `long_stalled` is `EKPBGE`, in `standard`, its engine gone. They start 40 and 11 days before the design's run, so they read `40d` and `11d` at every cut; both run a five-node stand-in named fragua, and `EKPBGE` stalls at its first node where the design's had six done. A run whose engine is alive is seen at its last event's instant: `work` reads 7m34s, 2m34s at work and 5m00s waiting. The tally counts one that needs the person, one stalled and one running, and the attention row names `EZRPE9`, in the engine's words for its failure. `findings` is cut at seq 200: four findings, the blocking one among them, none answered, three reviewers at work. `HostClock::at` starts a clock earlier. The design's own times at the findings differ from the recording's by a few minutes.

**Fails first:** `crates/testkit-ui/tests/cut.rs`: the cut of `work` followed and framed at its clock reads the design's time, 7m34s; its field holds one run that needs the person and one stalled, each older than the run in view; the cut of `findings` holds a blocking finding with no answer. Red on all three.

**Acceptance criteria:**
- [x] Every target of a run's screen names a cut with the two runs beside it; a target of the field, of a starting screen or of a page keeps its own.
- [x] The two runs build the same twice; the dumps written twice are equal.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-testkit-ui --test cut --test small_runs`

**Dependencies:** Task 25a

**Files likely touched:** `crates/testkit-ui/src/{cut.rs,cut/targets.rs,small_runs.rs}`, `crates/testkit-ui/src/small_runs/` (the two runs), `crates/testkit-ui/tests/{cut.rs,small_runs.rs}`, `.plans/yunta-tui/proof/live-screens/probe/` and `dumps/` (outside git)

**Estimated scope:** M

### Group check: the runs the targets show
- [x] `suite.py crates` and the engine's groups green; the design's run recorded twice equal; every small run built twice equal; every cut of `TARGET_CUTS` dumped, and `INDEX.md` mirrors it.

*In parallel:* Task 26 first; then Task 27 to Task 32, which write one file each and run at once; then Task 33.

**Found in its check:** the crates' group 124 binaries and 2,824 tests, none failing, the design's run recorded again equal to the kept log and each small run built twice equal among them; the engine's five batches as at the fold's check and one more. The nine targets that stand in a place the home does not hold whole are Task 25a's.

### Task 26: the inventory's form, and what holds it to the frames

**Description:**
- **The form**, at the head of `proof/live-screens/inventory/inventory.md`: one table a family, `inventory/<family>.md`, with the columns `id` (`<family>.<screen>.<C|P|T><nn>`), `screen` (the seed, or seed and key), `element` (the id of the spec's elements table it carries, C1 to M1 or W1, or none), `survey` (the id of the gap survey's row it comes from, as `gap-decisions.md` S7, or none), `kind` (component, phrase, or today only), `the design says` (a phrase's words exactly, a value as `{…}`; a component's name and what it draws), `frames` (its rows at wide, narrow and ASCII, `—` where a size drops it), `spike` (`view.rs:<line>`), `own` (the design's, or made up for the demo with the log's fact that stands for it), `today` (`path:line`, or none), `verdict` (same, differs, missing), `target` (the target screens that carry it). A today-only row says what a screen says today and the design does not, and proposes where it goes: said the design's way where a person acts on it, or left to the printed page.
- **`.plans/yunta-tui/tools/inventory_cover.py <family>`**: every run of text in each of the family's frames, at the three sizes and one key away, split where two spaces or more stand, that no row's words cover, the family's data words aside (handles, paths, numbers, names of nodes and workflows). A row whose words stand in no frame is said too.

**Found in its turn:** the form gains a thirteenth column, `gives way` (`never` or `yes`), since the targets' checker has to know the three things that never give way; `today` holds today's exact words with their `path:line` at the base, which the tool checks with `git show`; a row only today's screen says takes the verdict `to the screen` or `to the printed page`; a row a departure changes names it as its target (`none: D<n>`), and a made-up one is `none: D20`; a survey's own ids stand where they are unique, else `L<line>`; each family's file lists the spike's data apart from its rows; each state belongs to one family, so `work+esc` and `work+n` are the field's and the starting screens', and `field+enter` is the field's. The tool is `inventory_cover.py <family>`, with `--all` for the join's checks.

**Fails first:** `inventory_cover.py` on an empty `run` family lists every text of `work.wide.txt`; one row added, its phrase leaves the list.

**Acceptance criteria:**
- [x] The form names every column the targets' checker (Task 34) reads, and the `element` and `survey` columns a later task cites a row by.

**Verification:**
- [x] Manual check: the tool's two runs above

**Dependencies:** Group check: the runs the targets show

**Files likely touched:** `.plans/yunta-tui/proof/live-screens/inventory/` (outside git), `.plans/yunta-tui/tools/inventory_cover.py` (outside git)

**Estimated scope:** S

### Task 27: inventory of a run at work

**Description:** `inventory/run.md`, from `sweeps/gap-run.md`, completed and checked on Opus:
- the frames `work` and `build` at wide, narrow and ASCII, and one key away (`event`, `j`, `k`, `q`, `esc`, `n`; what `s`, `g`, `help` and `palette` open belongs to Task 30); the states further on the `build event` path; the spike's `view.rs` for each;
- the elements C1 to C6, E1, E3, E4, E16, H1 to H3, N1 to N3, P0 to P2 and M1, and every phrase of their panes, headings, legends and notes;
- what the run's screen says today and the design does not (its page, its chronicle from the first line, its acts' rows), each with where it goes.

**Found in its turn:** `inventory/run.md` holds 15 components (2 same, 10 differ, 3 missing), 38 phrases (17 same, 8 differ, 13 missing; 2 made up by the spike) and 25 rows only today's screen says (17 to the screen, 8 to the printed page). Some of the run's words stand only on other families' frames: the mark of a node sent back at `plan+2+enter`, `session opened` at `field+enter`, `granted by you` at `scope+enter` and on the receipt, `green` on ship; `the engine read it and accepted it` stands on no frame. A feed's newest row never gives way but has no fixed words, so no row can say so: the targets' checker holds it another way. The spike cuts a node's summary at the edge with no ellipsis, so both cut forms are data. The application's run page ends with `the whole story, node by node`, a row only today's says.

**Fails first:** none: an inventory. `inventory_cover.py run` lists every text before it is written.

**Acceptance criteria:**
- [x] `inventory_cover.py run` prints nothing.
- [x] Its foot counts components and phrases, same, differs and missing, beside the survey's counts, each difference explained.

**Verification:**
- [x] Manual check: `python3 .plans/yunta-tui/tools/inventory_cover.py run`

**Dependencies:** Task 26

**Files likely touched:** `.plans/yunta-tui/proof/live-screens/inventory/run.md` (outside git)

**Estimated scope:** S

### Task 28: inventory of the findings and the receipt

**Description:** `inventory/review-receipt.md`, from `sweeps/gap-review-receipt.md`, completed and checked:
- `review` and `receipt` at the three sizes and one key away; the review's states one, two and three events on (`design_frames.py 79`, `239`, `501`); `view.rs`'s `findings`, `receipt`, `severity` and legend;
- the elements P7 and P9: the count, a finding a row with its severity, the pointed finding, `fixed:` with its answer, the checklist, `cost`, `changed`, `pull request`, the harvested field;
- what today's findings page and receipt say that the design does not, each with where it goes.

**Found in its turn:** `inventory/review-receipt.md` holds 15 components (1 same, 11 differ, 3 missing), 44 phrases (20 same, 21 differ, 3 missing) and 16 rows only today's screen says, all left to the printed page. Five of the survey's verdicts were wrong and are corrected: the trail, the tally and the ribbon differ from today's; the run's headline, `at work`, `waiting on you` and `pull request` stand today word for word. `{…} files, +{…} -{…}` differs from today's only in Unicode, where today prints U+2212. `github {…}`, the pull request's value, is made up by the spike. A declined finding's answer has no wording in the design and no target.

**Fails first:** none: an inventory; `inventory_cover.py review-receipt` lists every text before.

**Acceptance criteria:**
- [x] `inventory_cover.py review-receipt` prints nothing; its foot counts against the survey's.

**Verification:**
- [x] Manual check: `python3 .plans/yunta-tui/tools/inventory_cover.py review-receipt`

**Dependencies:** Task 26

**Files likely touched:** `.plans/yunta-tui/proof/live-screens/inventory/review-receipt.md` (outside git)

**Estimated scope:** S

### Task 29: inventory of the decisions

**Description:** `inventory/decisions.md`, from `sweeps/gap-decisions.md`, completed and checked:
- `ask`, `scope`, `failure`, `ship` and `plan` at the three sizes, read frame by frame where the survey read only the wide ones; one key away (`ask+enter`, `plan+1`, `+2`, `+3`, `+d`, `+p`, `+h`, `+l`, `+tab`, and each card's numbers and `enter`); the departure's card on the `scope enter event enter` path; the notes an answer leaves (`plan 2 enter`);
- the elements P3 to P6, P8, and W1's card sentences: each card's question, each option's name with what it does, the sentence that says an answer is recorded;
- what today's card, round and desk say that the design does not, each with where it goes.

**Found in its turn:** `inventory/decisions.md` holds 25 components (8 same, 13 differ, 4 missing), 100 phrases (37 same, 31 differ, 32 missing; 10 made up by the spike) and 32 rows only today's screens say (21 to the screen, 11 to the printed page), with 72 texts that are data. The ship's gist, `Plan and its tests registered. Approve?` and `adjust` are the workflow's own words, so data. Three of the survey's verdicts were wrong: a round's `esc` says `esc leaves the run waiting` today, the run's page already has the headline, and a gate's gist is its own message, not missing. `… 1 more option` and the legend cut to `esc` at 80×24 are the spike's defects. Some rows only today's says go to targets whose run has to show them: an optional question and `(no answer)` on `round`, `r read the plan` on `ship`, the caps' words on `budget` and `loop-cap`.

**Fails first:** none: an inventory; `inventory_cover.py decisions` lists every text before.

**Acceptance criteria:**
- [x] `inventory_cover.py decisions` prints nothing; its foot counts against the survey's (45 components and 50 phrases that differ or are missing).

**Verification:**
- [x] Manual check: `python3 .plans/yunta-tui/tools/inventory_cover.py decisions`

**Dependencies:** Task 26

**Files likely touched:** `.plans/yunta-tui/proof/live-screens/inventory/decisions.md` (outside git)

**Estimated scope:** M

### Task 30: inventory of the palette, the help, a run's numbers and its graph

**Description:** `inventory/overlays.md`, from `sweeps/gap-overlays-pages.md`, completed and checked:
- every seed's `+palette`, `+help`, `+s` and `+g` at the three sizes; `view.rs`'s palette, help, stats and graph;
- the element C7 (the palette's box, its three columns and words; the help's box), a run's numbers (the sentence under the ribbon, each labelled row, where its time went with its bar and what each mark means, who spent it with a bar a role) and its graph (its headline and its nodes);
- what today's palette, help, stats and graph say that the design does not, each with where it goes.

**Found in its turn:** `inventory/overlays.md` holds 14 components (6 same, 8 differ), 66 phrases (20 same, 37 differ, 9 missing) and 27 rows only today's screen says (19 to the screen, 8 to the printed page). The survey cited some wrong words: the palette says `command::does`'s words, not the effect words help uses; the findings fact and the facts block are today's own. Facts nobody acts on (the nodes' table, a runner's cost, the guard line, the `f` toggle, the loading text) go to the printed page. Each act today offers and the design does not name (the field's palette and help, a document's shape, close) takes words proposed in the design's manner, for the targets' page.

**Fails first:** none: an inventory; `inventory_cover.py overlays` lists every text before.

**Acceptance criteria:**
- [x] `inventory_cover.py overlays` prints nothing; its foot counts against the survey's (42 of 60 components, 55 of 87 phrases).

**Verification:**
- [x] Manual check: `python3 .plans/yunta-tui/tools/inventory_cover.py overlays`

**Dependencies:** Task 26

**Files likely touched:** `.plans/yunta-tui/proof/live-screens/inventory/overlays.md` (outside git)

**Estimated scope:** S

### Task 31: inventory of the field and its confirmation

**Description:** `inventory/field.md`, from `sweeps/gap-field.md`, completed and checked:
- `field` at the three sizes and one key away (`field+r`, `field+enter`, `field+j`), the confirmation to close (state 114 of `out/frames.json`, `view.rs:1310-1320`);
- the elements C2, C5, E1, E2, E5 and F1 to F4: the first row, the groups, a run a row, the glance with its rows and `what you can do`;
- what today's field says that the design does not (its reach row, the groups render has and the frames never draw), each with where it goes.

**Found in its turn:** `inventory/field.md` holds 23 components (4 same, 11 differ, 8 missing), 49 phrases (14 same, 13 differ, 22 missing) and 21 rows only today's screen says (13 to the screen, 8 to the printed page). The help and the palette over the field are the overlays family's states. Two of the design's phrases stand on no frame (`continue it from its log`, `continue it where it stopped`): no state points at a stalled or a cancelled run, so their rows name the target `r` reaches. The glance's rows are missing today, though some of its words stand on a run's page. No target draws the confirmation to cancel a run, and the design draws no mark for a run picked.

**Fails first:** none: an inventory; `inventory_cover.py field` lists every text before.

**Acceptance criteria:**
- [x] `inventory_cover.py field` prints nothing; its foot counts against the survey's.

**Verification:**
- [x] Manual check: `python3 .plans/yunta-tui/tools/inventory_cover.py field`

**Dependencies:** Task 26

**Files likely touched:** `.plans/yunta-tui/proof/live-screens/inventory/field.md` (outside git)

**Estimated scope:** S

### Task 32: inventory of the first request, a new run and a setup

**Description:** `inventory/starting.md`, from `sweeps/gap-starting.md`, completed and checked:
- `empty`, `compose` and `setup` at the three sizes, the narrow and ASCII frames the survey did not read among them; one key away (`compose+tab`, `+type`, `+enter`, `+esc`; `empty+type`; `setup+enter`); the `setup enter enter enter` path and the note it leaves;
- the elements S1 to S3: the name in block letters, the boxed line, the workflows with what each does, the field in perspective, `workflow`, `mode` with its chosen mark, `what runs`, `what it usually takes`, what the machine answers for, `what this repository is`, `who does the work`, the row that says what `enter` writes;
- what today's first run, start and setup stages say that the design does not, each with where it goes.

**Found in its turn:** `inventory/starting.md` holds 17 components (1 same, 8 differ, 8 missing), 54 phrases (5 same, 23 differ, 26 missing) and 28 rows only today's screens say (11 to the screen, 17 to the printed page). The tool sees a line as cut only when it reaches the last column, and the spike keeps a cell of margin, so the setup's sentence cut at 80×24 took a row of its own for the cut words; the join corrects the tool's rule and takes that row out. The box's faint words (`add support for installing a pack globally`) are the design's placeholder (`spike/src/view.rs:344`), kept as its phrase. What today's screens say while they find, write, admit and let a setup go has no printed page and no frame: the targets design each.

**Fails first:** none: an inventory; `inventory_cover.py starting` lists every text before.

**Acceptance criteria:**
- [x] `inventory_cover.py starting` prints nothing; its foot counts against the survey's.

**Verification:**
- [x] Manual check: `python3 .plans/yunta-tui/tools/inventory_cover.py starting`

**Dependencies:** Task 26

**Files likely touched:** `.plans/yunta-tui/proof/live-screens/inventory/starting.md` (outside git)

**Estimated scope:** S

### Task 33: the inventory joined and counted

**Description:**
- `inventory/inventory.md` joins the six families: what several screens share (the trail, the tally, the rule, the legend, the attention row, the note) is one row naming every screen it stands on; ids stay unique.
- Each element id of the spec's elements table (C1 to M1, W1) is named in the `element` column of at least one row; each row the surveys hold is named in a row's `survey` column; each line of "Where the design is not followed" by the rows it changes.
- The count by family and in all: components and phrases, same, differs, missing, own and made up, and today-only rows by where each goes, beside the spec's measure (some 270 components of which about 200 differ, some 380 phrases of which about 270), each gap explained.

**Found in its turn:** the inventory joined holds 66 components (13 same, 37 differ, 16 missing) and 297 phrases (95 same, 112 differ, 90 missing; 15 made up by the spike), with 149 rows only today's screens say (81 to the screen, 68 to the printed page). Fifty-one shared rows (14 components, 37 phrases) stand in `inventory.md` in place of 149 family rows, each glyph named once. The tool counts a text as cut when it reaches the column before the last, a survey's own count table gives no rows, and `--all` holds that today's backticked words are refused only on a row's own targets; a value cannot cross a wrapped line. The tally is one shared row, `differs`. The run family also reads `work+event+event`, where `the engine read it and accepted it` stands, on the target `work-moved`. The attention row's row names only the field's screens: the targets' tasks carry it to every screen. The surveys' first sizing (some 270 components and 380 phrases) counted a component and a phrase once for each screen; the spec and the plan say the inventory's counts.

**Fails first:** none: a join. `inventory_cover.py --all` prints nothing.

**Acceptance criteria:**
- [x] Every element id, every survey row and every departure line is named; ids are unique; the counts are written.

**Verification:**
- [x] Manual check: `python3 .plans/yunta-tui/tools/inventory_cover.py --all`

**Dependencies:** Task 27, Task 28, Task 29, Task 30, Task 31, Task 32

**Files likely touched:** `.plans/yunta-tui/proof/live-screens/inventory/inventory.md` (outside git)

**Estimated scope:** S

### Group check: the inventory
- [x] `inventory_cover.py --all` prints nothing; the counts stand in `inventory.md`. The suite is untouched since the last check.

*In parallel:* Task 34 first; then Task 35 to Task 41, which write disjoint files and run at once; then Task 42.

### Task 34: the targets' checker

**Description:** `.plans/yunta-tui/tools/targets_check.py <directory> [family]`, a checker and no generator, over the directory it is given: `crates/tui/goldens/targets` in this phase, `crates/tui/goldens/screens` once a later task moves a target there and draws it. It reads the `<screen>.<environment>.txt` frames of the four environments whose screen the inventory's `target` column names, and lets any other file be:
- **shape**: each frame's rows and cells are its size's (120×36, 80×24, 60×14); the trail on the first row, the rule on the second, the legend on the last, the attention row above it where one stands; the side region's edge only from 102 columns, the legend's far keys only from 100;
- **the inventory**: every row of a target's screens stands in it word for word, `{…}` matched as a value; a row of the three that never give way (the row that says what `enter` does, a card's options, a feed's newest row) stands at every size; no target says a phrase the inventory gives another form (its today's form, where its verdict differs);
- **cuts**: no number is cut, a cut ends in the ellipsis, a list that hides rows says how many above and under.
- `.plans/yunta-tui/tools/targets_check_fails.py` breaks a frame each way the checker checks, and each check fails.

**Found in its turn:** `targets_check.py` holds shape, inventory, cuts and feed; `targets_check_fails.py` proves it on 32 cases, each rule on a frame made to pass and one made to break it. Calibrated on the design's own 9,648 frames it names only the work pane's feed and two cuts at the edge of 80×24, both listed departures. A feed is found by its heading; the row above the legend is blank, the attention row or a note; rows only today's says are held at the two widest sizes; `screen:field` names the screen where `field` is also a family. With `cancel-run` the targets are 101 screens, 404 frames. Twenty-seven targets have no row yet (the screens the design draws no frame for, and the two confirmations), ten rows ask a size one of their screens drops, and the row of a run picked still asks the stopped run's mark: Task 34a settles the three before the targets are written.

**Fails first:** none: a tool. Each check is seen to fail on a frame made to break it.

**Acceptance criteria:**
- [x] `targets_check_fails.py` reports every check failing on its broken frame, each in a directory of its own given to the checker.

**Verification:**
- [x] Manual check: `python3 .plans/yunta-tui/tools/targets_check_fails.py`

**Dependencies:** Group check: the inventory

**Files likely touched:** `.plans/yunta-tui/tools/targets_check.py` (outside git), `.plans/yunta-tui/tools/targets_check_fails.py` (outside git)

**Estimated scope:** S

### Task 34a: the inventory ready for seven hands

**Description:**
- **Rows for the screens with no row.** Twenty-seven targets have none: the screens of where a run stands and the pages the design draws no frame for, `close-run` and `cancel-run`. The two families with no frame of their own get a file each (`inventory/stands.md`, `inventory/pages.md`) in the form, where the task that designs a screen writes its rows as it writes the target; `close-run` and `cancel-run` take their rows in `overlays.md`.
- **Seven agents write at once, and none edits a file another edits.** A target's task adds its screens to the rows that name them, shared rows among them, without touching `inventory.md` or another family's file: the form says how (a family's own list of the shared rows its screens carry, read by both tools as part of each row's targets), and both tools read it.
- **A row asks a size only of the screens that stand at it.** Ten rows ask a size one of their own screens drops, since the join kept the first screen's `frames` cell (`tokens` and `{…} in / {…} out ({…} cached)` on `field` at 80×24, where the glance does not stand; `field.field.P10`, `starting.setup.P08`, `starting.empty.C05`, `starting.compose.C05`, `decisions.plan.P11`, `run.work.P08`, `shared.work+event.P01`). Each says its sizes for each screen.
- **The mark of a run picked** is `◉`, `*` in ASCII, where the base draws the stopped run's `■` (`crates/surco/src/glyphs.rs:187-193`): `field.field.T20` says so, for the targets' page to show.

**Found in its turn:** `inventory/stands.md` and `inventory/pages.md` stand in the form with no row yet: the recording has no frame of their screens, so each row there is one only today's says, and its `screen` cell names targets. Each family's file lists, under `## Rows its targets carry`, the rows of other files its screens carry, and both tools read a listed screen as one of that row's targets, so a targets' task edits its own file alone. A row says its sizes for each target (`field (n—)`); nine rows needed it, not ten: on the desk at 80×24 `decisions.scope.P10` stands only by a listed defect. `targets_check.py crates/tui/goldens/targets <family>` is the one command a targets' task runs, and it checks the family's inventory file too. Rows only today's says are held at the two widest sizes, so the words of the screens with no frame are not held at 80×24. The joined counts in `inventory.md` go stale as rows are added; the page's task takes them again from `--count all`.

**Fails first:** `targets_check_fails.py` gains a case for each: a screen whose family lists a shared row and lacks its words fails by that row's id; a row held at a size its screen drops no longer fails; two families' lists naming one shared row both hold. Each red before the tools read the lists.

**Acceptance criteria:**
- [x] `inventory_cover.py --all` prints nothing, and each family prints nothing.
- [x] `targets_check_fails.py` passes with the new cases; on an empty targets directory the checker reports no `no row` for a screen whose rows are written, and says which screens still wait for their task to write them.
- [x] The form says, for a targets' task, exactly which file it edits and how it is checked.

**Verification:**
- [x] `python3 .plans/yunta-tui/tools/inventory_cover.py --all`, `python3 .plans/yunta-tui/tools/targets_check_fails.py`

**Dependencies:** Task 33, Task 34

**Files likely touched:** `.plans/yunta-tui/proof/live-screens/inventory/` and `.plans/yunta-tui/tools/{inventory_cover.py,targets_check.py,targets_check_fails.py}` (outside git)

**Estimated scope:** M

### Task 35: targets of a run at work

**Description:** written by hand in the four environments, from each screen's recorded frame where one exists, with its cut's facts (its row of `TARGET_CUTS`, its dump), the design's words, and every correction the spec lists, into `crates/tui/goldens/targets/<screen>.<environment>.txt`:
- `work` (a session node at work: what it hands over, what it says, its feed closing the pane), `work-moved` (the design's `work+event`: one event on), `work-command` (a command or a check at work: what it runs, its result and the output's tail once it has them), `created`;
- `still-answered` (an answer recorded, its resume not yet on the log, `r` where no engine takes it), `still-unread` (a pause on the log, what it asks not yet read), `wakes-alone` (`AwaitingReset`: when it goes on, no tint, no spinner);
- `board` (one task: its title and attempt, proved red or green and by which command, what it keeps, its scope with paths written and granted), `board-moved` (the design's `build+event`), `board-tasks` (two tasks at work, each feed row naming its task, from the small run of a loop with two tasks at once);
- `findings` (three reviewers at once), `findings-answered` (the pointed finding with its answer, cut where the design's four findings are posted). Every finding row of `review-receipt.md` stands in one of the two, so the review's states between take no target: Phase 3's scenes assert them;
- `feed-back` (the feed scrolled back: rows above and under counted, `f follow what it is doing` under the nodes);
- a node's own scene: `node-working`, `node-needs-you` (the prompt put aside), `node-ended`, `node-never-ran`, `node-skipped` (from the small run whose mode leaves a node out), `node-sent-back` (E16's mark, from the small run whose plan gate is answered `adjust`).

**Found in its turn:** 19 screens, 76 frames, and `targets-notes/CONVENTIONS.md`, which the six targets' tasks after it follow: the rows every screen shares at each size, what gives way at 80×24 and at 60×14, how a block is headed and a row pointed, how a list says the rows it hides (`… {n} above`, `… {n} under`), the tally's rule (it counts the field the screen stands in and leaves a zero out; at 60×14 it keeps its marks and counts where its words do not fit beside the trail) and the attention row's (its own row at every size, never giving way; cut so `· a go there` stays whole). The lead set `work` beside the design's frame before the rest were written, which is what Task 25b corrects; the screens were then drawn again from its dumps. `what it says` stands on no target: the log keeps a note's size and digest, and no cut hears a session. The feed lists each attempt's start and the engine's moments in the chronicle's words beside the calls. The `Sent back…` note stands on no target, no cut answering there. `run.md` gains 29 rows only the targets say. Each choice for the person to see is an `asks:` line of `targets-notes/run.md`.

**Fails first:** none: targets; `targets_check.py crates/tui/goldens/targets run` fails on each screen before it is written.

**Acceptance criteria:**
- [x] `targets_check.py crates/tui/goldens/targets run` passes: 19 screens, 76 frames.
- [x] Every row of `inventory/run.md` and `review-receipt.md`'s findings rows names a target here that carries it.

**Verification:**
- [x] Manual check: `python3 .plans/yunta-tui/tools/targets_check.py crates/tui/goldens/targets run`

**Dependencies:** Task 34

**Files likely touched:** `crates/tui/goldens/targets/` (new): `work`, `work-moved`, `work-command`, `created`, `still-answered`, `still-unread`, `wakes-alone`, `board`, `board-moved`, `board-tasks`, `findings`, `findings-answered`, `feed-back`, `node-working`, `node-needs-you`, `node-ended`, `node-never-ran`, `node-sent-back`, `node-skipped`, each `.{unicode-120,unicode-80,ascii-120,unicode-60}.txt` (new)

**Estimated scope:** M

### Task 36: targets of a decision in place

**Description:** written as Task 35 writes them, into `crates/tui/goldens/targets/`. A line asked of a person stands in the design's box wherever it is asked: a round's typed answer, an option that asks for words, a card's note, the desk's foot.
- `round` (`you asked`, the card over the nodes list and the pane, the question in turn, its typed answer boxed), `round-answered` (an answered question folded to one row), `round-refused` (a refused answer under its question, from the small run whose first answers are refused), `round-aside` (the note `esc` leaves on a round: nothing recorded, the run stays waiting);
- `desk` (the plan's desk as the design draws it), `desk-confirm` (its confirmation row), `desk-words` (an option that asks for words, its typed line boxed in place of the options), `desk-proof` (`p`, the plan's proof), `desk-decided` (`d`, what is decided), `desk-change` (the change's pane);
- `card` (a gate with no plan, `Menu::Console`: what the gate shows above the card, the change by file), `card-words` (a typed line boxed at the card's foot), `card-recorded` (`ship` answered: the note an answer leaves, in the design's sentence for its option), `card-unread` (a pause whose shown document fails today's shape: its why in place of the card, from the small run of a shown document unread; a screen the spec does not draw);
- `failure`, `failure-recorded` (its answer's note), `scope`, `scope-recorded` (the grant's note), `departure` (the engine's options), `budget`, `loop-cap`, `ship`.

**Found in its turn:** 22 screens, 88 frames, the checker holding; `decisions.md` gains nine rows only today's says. The cut of an answer recorded stood at the pause with `enter` pressed, so its screens still read `needs you` where the design shows the run gone on; `card` was drawn from a run whose pause the engine rebuilds no menu for; no cut holds a question answered by typing, so no target boxes one: Tasks 41a and 41b correct the three. Where a decision stands no attention row stands, typed line or not. Each choice for the person is an `asks:` line of `targets-notes/decisions.md`.

**Fails first:** none: targets; `targets_check.py crates/tui/goldens/targets decisions` fails on each screen before it is written.

**Acceptance criteria:**
- [x] `targets_check.py crates/tui/goldens/targets decisions` passes: 22 screens, 88 frames; a card's options stand at 80×24 and 60×14.
- [x] Every row of `inventory/decisions.md` names a target here that carries it; `round`, `desk-words` and `card-words` draw their typed line in the design's box.

**Verification:**
- [x] Manual check: `python3 .plans/yunta-tui/tools/targets_check.py crates/tui/goldens/targets decisions`

**Dependencies:** Task 34

**Files likely touched:** `crates/tui/goldens/targets/` (new): `round`, `round-answered`, `round-refused`, `round-aside`, `desk`, `desk-confirm`, `desk-words`, `desk-proof`, `desk-decided`, `desk-change`, `card`, `card-words`, `card-recorded`, `card-unread`, `failure`, `failure-recorded`, `scope`, `scope-recorded`, `departure`, `budget`, `loop-cap`, `ship`, each in the four environments (new)

**Estimated scope:** M

### Task 37: targets of where a run stands

**Description:** written as Task 35 writes them, into `crates/tui/goldens/targets/`:
- the standstills: `standstill` (a pause no option answers here, in the log's words, `r` and `x`), `no-room` (what is free against what is needed, `u` then `r`), `orphans` (and blocked: what is left half done and what to check), `stopped` (cancelled, after a crash, a gate aborted, a sentence recorded: where and why), `stalled`;
- `pull-request` (the link and how it stands, `o`), `child` (the trail up to its parent), `asked-elsewhere` (the node, since when, the process that asks; nothing here);
- `receipt` (criteria, baseline, scope, review, the event chain, cost, change, pull request), `reported` (the findings that block, under the report);
- `failed`, `promoted` (the successor named, as the scene stands while the person leads), `broken` (where the log stops making sense).

**Found in its turn:** 13 screens, 52 frames, the checker holding; `stands.md` holds 33 rows, written as the screens were designed. The field in perspective under the receipt redoes the spike's horizon cell for cell. `promoted`'s cut pressed no key though its target is the screen while the person leads; `asked-elsewhere` holds the asking process's id, which a test puts in. Each choice for the person is an `asks:` line of `targets-notes/stands.md`.

**Fails first:** none: targets; `targets_check.py crates/tui/goldens/targets stands` fails on each screen before it is written.

**Acceptance criteria:**
- [x] `targets_check.py crates/tui/goldens/targets stands` passes: 13 screens, 52 frames.
- [x] Every receipt row of `inventory/review-receipt.md` names a target here; every standstill takes the design's frame and components, its design noted on the page.

**Verification:**
- [x] Manual check: `python3 .plans/yunta-tui/tools/targets_check.py crates/tui/goldens/targets stands`

**Dependencies:** Task 34

**Files likely touched:** `crates/tui/goldens/targets/` (new): `standstill`, `no-room`, `orphans`, `stopped`, `stalled`, `pull-request`, `child`, `asked-elsewhere`, `receipt`, `reported`, `failed`, `promoted`, `broken`, each in the four environments (new)

**Estimated scope:** M

### Task 38: targets of what stands over a run

**Description:** written as Task 35 writes them, into `crates/tui/goldens/targets/`:
- `palette` (its box, three columns, every act the code offers among its rows, the ones the design does not name among them), `palette-narrowed` (typing narrows it, in the design's row, `type to filter` after the pointer, no box), `palette-aside` (the palette over a prompt put aside, its row that answers what the run asks among the rows; the design's `ask+palette`), `receipt-palette` (the palette over a finished run's receipt), `help` (its box, the keys out of the legend listed);
- `field-palette` and `field-help` (the palette and the help over the field);
- `stats` (a run's numbers as `work+s` draws them), `graph` (as `work+g` draws it);
- `close-run` (a confirmation over a run: what closing does and the command that does the same);
- `cancel-run` (the confirmation to stop a run, in the same box: what stopping does and the command that does the same; no frame draws it, so it is designed here in the design's components);
- `chronicle` (the run's whole chronicle opened from the palette, a feed of its own, its newest moment in sight).

**Found in its turn:** 12 screens, 48 frames, the checker holding. Five of its screens stand over another family's (a round put aside, the receipt, a stopped run, the field twice), cell for cell as those stood when it was written, so they are drawn again when those change. A run's numbers were cut at the plan's third call, where few of the design's facts stand. `yunta resolve-gate` answers a round today, so the palette's row for one is true, not the recording's defect. Each choice for the person is an `asks:` line of `targets-notes/overlays.md`.

**Fails first:** none: targets; `targets_check.py crates/tui/goldens/targets overlays` fails on each screen before it is written.

**Acceptance criteria:**
- [x] `targets_check.py crates/tui/goldens/targets overlays` passes: 12 screens, 48 frames.
- [x] Every row of `inventory/overlays.md` names a target here that carries it.

**Verification:**
- [x] Manual check: `python3 .plans/yunta-tui/tools/targets_check.py crates/tui/goldens/targets overlays`

**Dependencies:** Task 34

**Files likely touched:** `crates/tui/goldens/targets/` (new): `palette`, `palette-narrowed`, `palette-aside`, `receipt-palette`, `help`, `field-palette`, `field-help`, `stats`, `graph`, `close-run`, `cancel-run`, `chronicle`, each in the four environments (new)

**Estimated scope:** S

### Task 39: targets of the field

**Description:** written as Task 35 writes them, into `crates/tui/goldens/targets/`:
- `field` (the first row, the groups, a run a row, the glance of the run pointed at with `what you can do`, the attention row, the tally);
- `field-closed` (`d` lists the closed), `field-machine` (`e` reaches every run on the machine), `field-unreadable` (a run that cannot be read, with why);
- `field-marked` (two runs marked with `space`, the marks as the design draws them), `field-close` (the confirmation to close, naming every run it closes), `field-note` (the note an act leaves, in the attention row's place: `r` on a run that waits on a decision) and `field-resumed` (the note `r` leaves on a run it resumes, added in the second round);
- the glance of other runs: `field-glance-stalled` (a stalled run pointed at, `r` and `x` among what you can do), `field-glance-stopped` (a stopped run pointed at). The glance of a run that needs the person is `field`'s.

**Found in its turn:** 9 screens, 36 frames; one row of the inventory, the mark of a node a mode leaves out, stood on no target, no run of the field's cut holding one. The cut's runs are the test home's, so the rows read `default` with short furrows and the run pointed at has spent nothing, far from the design's field: Task 41a gives the field the design's. The mark of a run picked is `◉`, and `+` in ASCII, the pointer's being `*`. Each choice for the person is an `asks:` line of `targets-notes/field.md`.

**Fails first:** none: targets; `targets_check.py crates/tui/goldens/targets field` fails on each screen before it is written.

**Acceptance criteria:**
- [x] `targets_check.py crates/tui/goldens/targets field` passes: 9 screens, 36 frames.
- [x] Every row of `inventory/field.md` names a target here that carries it.

**Verification:**
- [x] Manual check: `python3 .plans/yunta-tui/tools/targets_check.py crates/tui/goldens/targets field`

**Dependencies:** Task 34

**Files likely touched:** `crates/tui/goldens/targets/` (new): `field`, `field-closed`, `field-machine`, `field-unreadable`, `field-marked`, `field-close`, `field-note`, `field-resumed`, `field-glance-stalled`, `field-glance-stopped`, each in the four environments (new)

**Estimated scope:** S

### Task 40: targets of the first request, a new run and a setup

**Description:** written as Task 35 writes them, into `crates/tui/goldens/targets/`:
- `first-request` (S1);
- `new-run` (S2: the boxed request and its hint, `workflow`, `mode` with the chosen one marked, `what runs` with where it stops for a person, `what it usually takes` from the three past runs, what the machine answers for), `new-run-mode` (the focus on the mode), `new-run-inputs` (a row for each other input the workflow declares and one for the adapter, in the design's components), `new-run-refused` (an admission refused, said in place);
- `setup` (S3, with the committer), `setup-changing` (a row changed in place: a choice among the adapters, a role's model being typed in the design's box);
- `setup-audit` (S4: the pack's audit, what `yunta pack add` shows before it installs), `setup-confirm` (its confirmation, as the close's), `setup-written` (the page that says what was written and what the installed pack still needs, from which `enter` goes on to the first request), `setup-unreachable` (a pack that cannot be fetched: no audit, and the write row writes the setup alone).

**Found in its turn:** 11 screens, 44 frames, the checker holding; `starting.md` gains 21 rows only today's says. A new run opens on the workflow's first mode, as `yunta run` does; the adapter's row stands between the mode and what runs; fragua stands first, by how often each workflow ran here. The hint under the request was the input's own description where the design has a sentence; the forge read unreachable in four lines for want of a token: Task 41b draws the design's hint, and Task 41a the place's forge. The over-budget caution stands on no target, the place's checkouts being within budget. Each choice for the person is an `asks:` line of `targets-notes/starting.md`.

**Fails first:** none: targets; `targets_check.py crates/tui/goldens/targets starting` fails on each screen before it is written.

**Acceptance criteria:**
- [x] `targets_check.py crates/tui/goldens/targets starting` passes: 11 screens, 44 frames; the row that says what `enter` writes stands at 80×24 and 60×14.
- [x] Every row of `inventory/starting.md` names a target here that carries it.

**Verification:**
- [x] Manual check: `python3 .plans/yunta-tui/tools/targets_check.py crates/tui/goldens/targets starting`

**Dependencies:** Task 34

**Files likely touched:** `crates/tui/goldens/targets/` (new): `first-request`, `field-empty`, `new-run`, `new-run-mode`, `new-run-inputs`, `new-run-refused`, `setup`, `setup-changing`, `setup-audit`, `setup-confirm`, `setup-written`, `setup-unreachable`, each in the four environments (new)

**Estimated scope:** M

### Task 41: targets of the pages the design draws no frame for

**Description:** each keeps the facts it shows today and is designed in its target, in the design's frame (trail, tally, rule, attention row, legend) and with the design's components, from today's screen and its page:
- `machine` and `upkeep` (read when opened, as today);
- `workflows` (with their checks), `workflow-checked`, `workflow-history`, `workflow-graph` (a workflow's graph, drawn as a run's is), `cases`, `new-workflow`, `packs`, `pack-audited`;
- `document-shape`; a run's evidence as `EvidenceStage` shows it: `evidence-receipt` (its receipt), `evidence-verified` (its verification), `evidence-page` (the page for a browser);
- `signpost` (outside a repository, as today).
Each target notes, for the page, which component of the design it takes for each part of today's screen. `room-needed` is no target: it stands only below 60×14, so it has none of the four environments, and its base frame `room-needed.unicode-59.txt` stays the one base frame kept.

**Found in its turn:** 15 screens, 60 frames, the checker holding; `pages.md` holds 56 rows, written as the screens were designed. No place these pages open in had a dump, so their facts were read from the code: Task 41a writes the dumps and Task 41b draws them again. The disk's figures and a pack's commit change with the machine, so a test masks them. Each choice for the person is an `asks:` line of `targets-notes/pages.md`.

**Fails first:** none: targets; `targets_check.py crates/tui/goldens/targets pages` fails on each screen before it is written.

**Acceptance criteria:**
- [x] `targets_check.py crates/tui/goldens/targets pages` passes: 15 screens, 60 frames.
- [x] Every fact today's page of each screen shows stands in its target (captures of today's screens read beside them).

**Verification:**
- [x] Manual check: `python3 .plans/yunta-tui/tools/targets_check.py crates/tui/goldens/targets pages`

**Dependencies:** Task 34

**Files likely touched:** `crates/tui/goldens/targets/` (new): `machine`, `upkeep`, `workflows`, `workflow-checked`, `workflow-history`, `workflow-graph`, `cases`, `new-workflow`, `packs`, `pack-audited`, `document-shape`, `evidence-receipt`, `evidence-verified`, `evidence-page`, `signpost`, each in the four environments (new)

**Estimated scope:** M

### Task 41a: the cuts corrected after the first targets

**Description:** the seven families' first targets, set beside the design's frames, showed nine cuts wrong or thin. Each is corrected where a cut is made, and the dumps are written again:
- the design's run records cached tokens, in the proportion the design's numbers show;
- the field's cut holds the design's run asking at its own terminal at `tests`, pointed at first, the runs that need the person oldest first, one stalled, and a run whose mode leaves a node out in sight;
- a run's numbers and its graph are cut where the run needs the person at its last gate;
- an answer recorded is cut at the pause, `enter`, and the log on to the next node's start;
- `card` stands on a run parked at a gate that shows a document that is not a plan, with the menu the engine rebuilds;
- one round holds a question answered by typing;
- `promoted` presses the keys that make the person lead;
- the places the pages stand in each have a dump;
- the place of a new run reads its forge as the design's where doctor at a glance can find it without the network, and its committer by a name.

**Found in its turn:** the design's run recorded again with each usage step reading 96% of its input from the cache (26,054,000 in, 1,064,000 out, 25,011,840 cached; seqs, times and `run/` as before), every `Q78GRR` dump with it; the field's cut holds the design's run asking at its own terminal at `tests` beside `EZRPE9` (quick, leaving `spec` out), `C0NS0E`, `T0KENS`, `B10CKD`, `EKPBGE` stalled, two stopped, one running, two closed, one unreadable, and each pointed run's glance; `stats` and `graph` cut at `ship` (seq 263); the three recorded answers press `enter` and go on to the next node's start (`TargetCut::goes_on_to`), their dumps ending with the note; `card` on `RN0TES`, a gate showing `release-notes.md` with its own menu; `REFVSE`'s round asks a typed answer first; `promoted` presses `j` `k`; dumps for the places `Worked`, `SetUp`, `Packed` and `Outside`; the committer a made-up name. The forge stays unset: doctor builds the GitHub forge from the token against `api.github.com` and a workspace cannot hand it a fake one, so a set token would go over the network. The shared tally's singular is a second row, `shared.work.T01`, since the tools hold every word of a row on every target; the picked mark's ASCII form is `+`. The disk fell under the pool's floor (the larger of a twentieth of the disk and 10 GB) while a sibling checkout built, and runs were refused for room: a trap is recorded. The checker names nine problems these corrections leave for Task 41b: the three recorded screens' tally, the picked mark in two ASCII frames, the `╌` no field target draws.

**Fails first:** `crates/testkit-ui/tests/cut.rs` and `tests/designed.rs`, one assertion for each: the kept log holds cached tokens; the field's cut holds a run asking at its terminal first, a run whose furrow holds a node left out; the cut of `stats` needs the person; a recorded target's cut moves after its keys; the cut of `card` hands a decision; a round's cut holds a question with no choices; red on each.

**Acceptance criteria:**
- [x] Two recordings of the design's run give the same bytes, and the kept log is the new one.
- [x] `TARGET_CUTS` names every target, and no row of `INDEX.md` says its cut could not be made.
- [x] The dumps written twice are equal.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-testkit-ui`, `cargo test -p yunta-testkit`

**Dependencies:** Task 35 to Task 41

**Files likely touched:** `crates/testkit-ui/src/{designed/,cut/,small_runs/,home.rs}`, `crates/testkit-ui/fixtures/designed/`, `crates/testkit-ui/tests/{cut.rs,designed.rs,small_runs.rs}`, `.plans/yunta-tui/proof/live-screens/probe/` and `dumps/` (outside git)

**Estimated scope:** M. Nine cuts change in the one table that names them.

### Task 41b: the targets brought to the corrected cuts

**Description:** each family's author reads its targets' dumps anew and corrects what changed; `targets-notes/CONVENTIONS.md` gains what the six families found missing; the three recorded screens, `card`, a round's typed answer, `promoted`, the field, the hint and the machine's facts of a new run, the pages, a run's numbers and its graph, and each box over another family's screen are drawn again. The family of what stands over a run goes last.

**Found in its turn:** each family's author redrew from the corrected cuts and cleared the page's check (268 findings on the first round's notes, none after): 102 screens, 408 frames, every family's checker holding, `inventory_cover.py --all` printing nothing. The run family's 76 frames stood (the cache is a part of the input, so no total moved) and `CONVENTIONS.md` gained the rules six families found missing (a typed screen, a box over a screen, a side region at the right, the headline's mark and time for each standing, the tally on a screen that follows nothing, what gives way at 60×14 on a pane with no feed, the picked mark `+` in ASCII, Surco's minus). The three recorded screens show the run gone on with its note; `card` stands on `RN0TES`; `round-refused` boxes a typed answer; the field is drawn from its design-like cut; `stats` and `graph` stand at `ship` with the cache; the pages read their places' dumps. The second round added one target, `field-resumed` (the note `r` leaves on a run it resumes), so `field-note` shows the design's `Nothing to resume…` on a run that waits on a decision; at the base `r` says nothing on either and hands a resume to a detached process. The receipt's page says the screen's rows and keeps the printed detail after them; its criteria read `1 of 1 green, red before the work · 2 guards green`, the guards a phrase proposed. The log holds a criterion's check before the work and the tasks' ledger kept only the one after: Task 41c. The forge stays out of reach in every place, so no target shows the design's forge row. Each choice for the person is an `asks:` line of its family's notes.

**Fails first:** none: targets; the checker fails on a screen whose dump changed until it is drawn again.

**Acceptance criteria:**
- [x] `targets_check.py crates/tui/goldens/targets` passes for every family: 102 screens, 408 frames.
- [x] `inventory_cover.py --all` prints nothing.

**Verification:**
- [x] Manual check: both commands above.

**Dependencies:** Task 41a

**Files likely touched:** `crates/tui/goldens/targets/`, `.plans/yunta-tui/proof/live-screens/{inventory,targets-notes}/` (outside git)

**Estimated scope:** M

### Task 41c: a task's check before the work, kept on the task

**Description:** the receipt's first row says a task's criteria were red before the work, and the tasks' ledger keeps only the check after the work (`TaskRecord::checked`), so no reader of the ledger can say it; the second round of the targets found the row borne out by a session's summary alone.
- **`TaskRecord::checked_before`**: the task's latest check before the work (`Phase::Pre`), as the log has it: each criterion's exit code where the work has something to prove, each guard's where it has to hold. Not `checked`, the check after the work, which the task is proved by.
- `TasksLedger::apply_checks` keeps it. Nothing reads it until the receipt's rows take the design's words (Task 88), so every print stays as it is.
- The probe's dump prints it with its task, and the dumps are written again.

**Found in its turn:** `TaskRecord::checked_before` keeps the task's latest check before the work beside `checked`, each kept by its phase in `apply_checks`; the design's run's task holds its check from seq 87, its criterion red (exit 101) and its two guards green. No print, golden or kept log moved; the dumps gain the field in each task's record and no target's facts change. The ledger's file stands at 495 lines.

**Fails first:** the ledger's tests: a log with a check before the work and one after keeps each on its own field. Red: the record has no check before the work.

**Acceptance criteria:**
- [x] The design's run's task holds its check before the work with its criterion red and its two guards green (`crates/testkit-ui/tests/designed.rs`).
- [x] Every golden, capture and receipt equals the base.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-core tasks`, `cargo test -p yunta-replay`, `cargo test -p yunta-testkit-ui --test designed`

**Dependencies:** Task 11, Task 41a

**Files likely touched:** `crates/core/src/events/tasks/ledger.rs` and its tests, `crates/testkit-ui/tests/designed.rs`, the probe's dump (outside git)

**Estimated scope:** S

### Task 42: the targets' page

**Description:** `.plans/yunta-tui/tools/targets_page.py` writes `proof/live-screens/targets/page.html`, which the lead publishes for the user:
- grouped by screen: each target at 120×36 beside the design's frame at the same size, where one exists; the 80×24, ASCII and 60×14 targets folded under it;
- each difference from the frame tagged by the line of "Where the design is not followed" that explains it; a difference no line explains is listed in red at the page's head;
- for each screen, what it says today and the design does not, with the proposed place (the design's way, or the printed page), each for the user to decide;
- the pages with no frame, alone, with the components each takes; the screens the spec does not draw (`card-unread`, a pause whose shown document cannot be read), each for the user's word;
- the rewordings the CLI's prints take in later phases, each written by hand into `targets/prints/` beside the base's print: `yunta status --follow` at a terminal ending with the meter, the next steps' words, the receipt's rows, `receipt.md` and the pull request's body. Each difference from the base is tagged by the inventory row whose phrase or component it takes, for the user to accept;
- the inventory's counts.

**Found in its turn:** `targets_page.py` reads the targets, the notes, the design's frames, the inventory and the prints, writes `proof/live-screens/targets/data.js` and `check.txt`, and holds each family's notes to its targets (`--check <family>`: a 120×36 row that differs from the design's frame and no `differs:` line names, a line with no reason, a print's difference with no inventory row); `targets_page_fails.py` proves each check on a doctored copy, 13 cases. The page itself is `targets/page.html`, written by the lead and published beside `data.js`: each screen's target beside its frame at the four sizes, the rows that differ marked, each difference with its reason and the departure or element it cites, what leaves the screen, what is asked of the person, the inventory's rows it carries, the eighteen reworded prints beside the base's, the inventory's counts, and a box a screen where the person writes a correction, kept in the page's own store. The tool writes the data and not the page, which is static. Writing the prints by hand showed four things the list had wrong, each ruled and written into Tasks 68, 86, 88, 130 and 131: the attached region keeps the run's gist row above its meter; a next step says its act in the palette's words; a printed receipt takes the design's rows in the pane's order and keeps after them what the inventory leaves to the printed page; Surco's minus stays. The page was checked by running its script over every view and by one offscreen snapshot; no browser on this machine opens it whole.

**Fails first:** none: a page. The tool fails when a target has no row in the inventory, or a difference from a frame or from a base print no tag.

**Acceptance criteria:**
- [x] Every target and every reworded print is on the page; no difference is untagged; `targets_check.py crates/tui/goldens/targets` passes for every family.

**Verification:**
- [x] Manual check: the page opened and read whole by the lead before the checkpoint

**Dependencies:** Task 35, Task 36, Task 37, Task 38, Task 39, Task 40, Task 41

**Files likely touched:** `.plans/yunta-tui/tools/targets_page.py` (outside git), `.plans/yunta-tui/proof/live-screens/targets/` (outside git)

**Estimated scope:** S

### Group check: the targets
- [x] `targets_check.py crates/tui/goldens/targets` passes for every family: 102 screens, 408 frames; the page is written; `cargo test -p yunta-tui` passes, the base's fifteen frames untouched.

### Task 43: three real runs after

**Description:**
- A release build of the phase; `real_run.py` three times as Task 3 ran it; each followed with `yunta status --follow` through a pipe.
- Each run's `between_secs` and tokens from `yunta stats <run>`, against the base's spread; the followers' lines against the base's with `followers.py`; the release size by crate against the base's.
- The probe's `frame`, `whole` and `at_rest.py` again.
- All of it in `proof/live-screens/checkpoint-a.txt`.

**Found in its turn:** three runs from the tree's release, each to `ship`, answered `abort` and closed: `01M7GE7Q6EAFB8CR5VBDHEFDZ9` (171 s, between 0.546 s, 1.85M tokens), `01M7GEG3V9FGSGJGWN66Y5H0PD` (171 s, 0.532 s, 1.25M), `01M7GENQS0FK9XB7KJRXW6QDCX` (136 s, 0.546 s, 1.08M). The time between nodes runs from 0.532 to 0.546 s against the base's 0.536 to 0.555: the six values interleave, and the one under the base's lowest is 3.8 ms faster. The first run's tokens are the agent's work, its implement session sent back once. Every call of the three logs names its session, the codex node's among them, and it is the id its node's session opened with. The followers differ from the base's only in what the agents did. The release weighs 31,046,352 bytes, 446,416 over the base's: `serde_path_to_error` +139 KB, `yunta_core` +109 KB, `yunta_api` +77 KB, `serde_saphyr` +53 KB, `yunta_replay` −22 KB. A drawing whole takes 0.202 ms against 0.202; a run of ten thousand calls is read whole in 22 ms against 3,948; the application at rest spends 0.104% of a core, inside the base's 0.091% to 0.110%.

**Fails first:** none: measurements.

**Acceptance criteria:**
- [x] Each run's `between_secs` is inside the base's spread, or the work stops for the user with the cause found.
- [x] `followers.py` finds the followers alike; the release stays under 40 MiB.

**Verification:**
- [x] Manual check: `yunta stats <run>` read back against `baseline.txt`

**Dependencies:** Group check: the targets

**Files likely touched:** `.plans/yunta-tui/proof/live-screens/real-after*` (outside git)

**Estimated scope:** S

### Checkpoint A: the ground, and the design written down
- [x] The user approves the targets on their page, each beside its frame, gives their word on the screens the spec does not draw, decides each line of what a screen says today and the design does not, and accepts the rewordings `yunta status --follow` and the receipt take later.
- [x] Every golden, capture, receipt and test equals the base, but `session` on calls in the schema and the documents (criterion 12 holds here); the follower agrees with `yunta status` on every log the suite holds, a broken one included; time between nodes inside the base's spread; the read whole, the drawing and the processor at rest are written beside the base's.
- [x] The phase commits after every step of the `ci` job passes (`sh .plans/yunta-tui/tools/gate.sh`, `CARGO_INCREMENTAL=0`, one build at a time), its paths staged by name, with `SPEC-live-screens.md`, `tasks/plan.md`, `tasks/todo.md`, `CAPABILITY-MAP.md`'s row, `.agents/architecture.md`, `crates/core/schemas/events.json`, `crates/testkit-ui/fixtures/`, `crates/tui/goldens/targets/` and the code of Task 4 to Task 25; a draft pull request opens with it; then `cargo clean`.

## Phase 2: the application follows

Its groups, each green before the next, but group 7, which touches none of the files of groups 2 to 6 and runs beside them from the phase's start, its last task (Task 68) waiting on Task 58 alone: the walk's bound, measured first; `yunta-ui` on the follower; the field; a run's screen as it is drawn today; the title and the bell; gone; what a node says in print; at a pseudo-terminal, and measured.

**Decided in this list, beyond the plan:**
- **Group 1 carries the state's and the performer's half of following** (Task 44, Task 45): `Request::Follow`, `Input::FollowNews`, the performer's one following, `State::followed()`, `FollowedRuns` and `RunInView`. The walk can be fed only what a state follows, and a helper can wait only on what a state keeps; no stage reads the kept data until group 2, so the walk's bound is still measured before any screen moves onto it. Group 2 keeps the rest the plan gives it.
- **A helper knows the state caught up by `State::caught_up(heads)`**, with the log's heads read by `yunta_testkit::events::heads` at the store `Performer::workspace()` names. `yunta-ui` links no store (`crates/ui/tests/no_terminal_in_the_interface.rs`), and after an act only the log says what the state has yet to hear (`crates/cli/tests/interface/field.rs` closes two runs and reads the field after).
- **The repository's reading carries where the project keeps its runs and its prices** (`RepositoryReading`, Task 49), and `FollowedRuns` keeps both beside what is followed: the field page names the store (`crates/ui/src/stage/field.rs:98`), a run's stats are costed at the prices (`crates/api/src/run/stats.rs:35`), and neither is followed. Every stage then reads them from the one value it is handed (review findings P5, P8).
- **What the state keeps of the run in view holds every piece the follower hands** (Task 45), where the plan's `RunInView` leaves two out. `awaited` is an `Option<AwaitedReading>`: what the run waits on a person for, read, or unread with its why (`RunNews::AwaitedUnread`), so Phase 3 shows the why in place of the card (the target `card-unread`, the spec's "unreadable in place"). `remarks` keeps a manifest's unknown keys (`RunNews::Remark`), which Phase 3's run stage says once as a note; the field keeps none, since it says no run's remarks. A document handed over unread is kept with its why, and `HandsOver` shows it so (Task 66).
- **A promotion moves every stage that shows the predecessor onto its successor** (`Stage::succeeded`, Task 55): the run's stage, its stats and its graph. `followed()` reads the run from the run's stage, so a stage left on the predecessor would ask to follow it again.
- **The run's page is read again through `Stage::followed_on`**, which the state calls on the stage the person is at after each piece of followed news (Task 54); a stage under is read again on return, as today (`crates/ui/src/stage/run.rs:94-109`).
- **Removing the run's `f` (Task 60) changes `tui`'s two `help` frames by that row, for the user's approval:** they are the only base frames that change before their target lands. The help over a run lists every key of the run's legend (`crates/render/src/screen/run.rs:99-108`), so `help.ascii-80.txt` loses its row `f       follows`, its box one row shorter, and `help.unicode-60.txt` says `… 6 more lines` for `… 7 more lines`. The plan has `f` go in this phase; Checkpoint B shows the two.
- **`watched::footer` stays for the application's run screen until Phase 3** replaces that screen; only the attached run's region moves to the meter here (`watched::spent`, Task 68). The run frames draw the footer's counter (`run.ascii-80.txt` row 5, `== = 1 of 2 nodes done ; …`), and they stay equal through this phase.
- **The window's title names the run in view where it needs the person, else the field's run waiting longest among those answered here**, as the attention row chooses; **`Request::CallThePerson` is asked once each time a run's standing turns to needs you after the field's first news**, so the runs that already need the person when the application opens call nothing (Task 58). A changed title gives the one before back first (`TITLE_RESTORED`, then `title_saved`), so leaving restores the window's own once.
- **The bell is the project's `terminal.bell`**, read by the binary and handed to `yunta-tui`'s `Application` (Task 59), as the attached run reads it from its manifest (`crates/cli/src/surface/mod.rs:216`).
- **The printed node page shows its feed's last twenty rows** and how many stand above them; what stands above the feed it prints whole (Task 67).
- **Surco's `Feed` lands as a block and its drawing** (Task 63); "a region holding a feed is laid out as its feed" lands with the first pane that holds one, in Phase 3.
- **The application's tests stand on fields the follower reads.** `support::on_the_field` follows a store of stopped runs a bench makes, where it handed rows made up (`crates/ui/tests/support/mod.rs:22-67`), since no constructor of news stands beside the follower. It is a mechanical task of its own (Task 50), before the field's first news decides where the application stands.

**Needs from other phases:**
- Phase 1's `yunta_api::follow`: `Followed { field, run }` with `Debug`, `Clone`, `PartialEq`, `Eq`; `Project::follow(Followed) -> Following`; `Following::refollow`, which keeps every run it holds; and `Following::next(&Stop)`, which never ends by itself and loses nothing when its future is dropped between two looks, so one task can wait on it beside the next `Followed`.
- Phase 1's `FollowNews` and `RunNews`, each `Clone` and `Debug`, in these arms: `FollowNews::{Field(Box<FollowedField>), Joined, Whole { run, events }, Run { run, news }, Unreadable, Left, Elsewhere}` and `RunNews::{Events, Engine, Said { said, missed }, Successor { run, events }, Awaited, AwaitedUnread { why }, Document { node, document }, Remark}`. The field read whole holds each run at its head as a `FollowedRun`, the runs it cannot read, how many it leaves elsewhere and whether it is the repository's; the run read whole hands every event from its first once, then each batch; what a run waits on a person for is read once per pause; each document a node hands over is read once, as `Document::{Read, Unread}`. Following a promoted run, the follower hands its successor and goes on following it.
- Phase 1's `FollowedRun`, keeping no events, with `frame(now)`, `row(now)` (the one way `yunta list --runs` makes a row), `menu` worked out once for each head, and `took`; `RunState::chronicled` and `RunState::head`; `yunta_api::Awaited`.
- Phase 1's `Operation::Follow` performed over the follower, with `News::Following` carrying `FollowNews` and `Outcome::Followed` as Phase 1 leaves them, and the run's stage folding what it is handed with `chronicled`.
- Phase 1's ledgers: a task's criteria and its latest post-phase check, its scope, the paths its sessions wrote, every session's task, a node's scope checks; and `agent_message`'s session.
- Phase 1's `MockStep::AwaitsWord` and `Hold`'s named words; the design's run kept under `crates/testkit-ui/fixtures/designed/`, its log as JSONL; the small runs as `yunta_testkit_ui::small_runs`, built in-process; and `yunta_testkit_ui::cut`, whose table names each target's cut and whose `Cut` loads runs at their moments into one store.
- Phase 1's inventory and targets, approved at Checkpoint A: in this phase the rows of the phrases of P1, P2 and C6, the feed's rows and its counts, and the meter's row of the target `work`.
- Phase 1's base, in `proof/live-screens/`: the walk's states, palettes, helps and time; every golden's digest; `pages.sh`'s captures; three real fragua runs with their time between nodes; the application's processor time at rest; the release size by crate; and the tools that made them (`real_run.py`, `follow_in_app.py`, `at_rest.py`).

### Task 44: the performer follows what it is asked to

**Description:**
- **`Request::Follow(Followed)`** (`crates/ui/src/request.rs`): what the application follows, handed whole each time it changes. It has no ticket: it is never let go, only followed anew.
- **`Input::FollowNews(FollowNews)`** (`crates/ui/src/input.rs`): a piece of what the follower handed.
- **The performer keeps one following** (`crates/ui/src/performer/following.rs`, new). The first `Request::Follow` makes it with `Project::follow` on a task of its own; each later one goes to that task, which calls `refollow` between two looks. Each piece of news goes to the inputs as `Input::FollowNews`. Dropping the performer stops the task, so a test that drops its performer leaves no reader looking. `Performer::take` answers `Request::Follow` with nothing for the surface.
- `State::update` takes `Input::FollowNews` and keeps nothing of it yet; no state asks `Request::Follow` before Task 45.
- Knowledge: `crates/ui/src/performer.rs`'s `//!` says the performer keeps one following, made on the first request and followed anew on each, and that it never ends by itself.

**Found in its turn:** `FollowingTask` (`crates/ui/src/performer/following.rs`) holds the one following behind a `watch::Sender<Followed>`: the first `Request::Follow` spawns its task, each later one reaches it between two looks, and dropping the performer ends it. The test also fails where each request starts a following of its own (the field read twice). A look that fails whole ended the task without a word, since `FollowNews` has no arm for it and the list said nothing of it: Task 44a. `yunta-ui` names no feature of tokio though it uses three: Task 44a.

**Fails first:** `crates/ui/tests/performer.rs`: a performer working in a bench's world, handed `Request::Follow` with the field, hands back the field read whole as `Input::FollowNews`, under no ticket; handed then a `Followed` naming the field and the parked run, it hands the run read whole and no second reading of the field. Red: the performer has no following.

**Acceptance criteria:**
- [x] A performer keeps one following, whatever number of `Request::Follow` it takes.
- [x] What it does for `Perform` and `LetGo` is unchanged: `crates/ui/tests/performer.rs`'s other tests pass as they stand.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test performer`, `cargo test -p yunta-ui`

**Dependencies:** Phase 1's follower

**Files likely touched:** `crates/ui/src/{request.rs,input.rs,performer.rs,state.rs}`, `crates/ui/src/performer/following.rs` (new), `crates/ui/tests/performer.rs`

**Estimated scope:** M

### Task 44a: a look that fails is told, and the following goes on

**Description:** a look at the log can fail whole (`Following::next` hands an `Error` where the store cannot be read), which is not a run that cannot be read (`FollowNews::Unreadable`, news of its own). As Task 44 left it, the performer's following ends at such a failure without a word, and the application goes on showing what it last read as if it were now.
- **The performer's following goes on after a look that fails.** `next` paces its own looks, so the next one comes a pace later; only `Error::Stopped` ends the task.
- **`Input::LookFailed(String)`** (`crates/ui/src/input.rs`): a look at what is followed failed, in the words a failed piece of work is said in (`Outcome::Failed`'s, made the same way). Under no ticket, as `FollowNews`. Not `FollowNews::Unreadable`, one run the field cannot read.
- `State::update` takes it and keeps nothing of it yet (Task 45 keeps it).
- `crates/ui/Cargo.toml` names the features of tokio the crate uses (`rt`, `sync`, `macros`), as every crate of the workspace names its own.
- Knowledge: `performer/following.rs`'s `//!` says a failed look is told and the following goes on.

**Found in its turn:** a look that fails is handed as `Input::LookFailed` in the words a failed work is said in, and the following goes on at the follower's own pace, now `Following::LOOKS_EVERY`, an associated constant, where a module's `READ_EVERY` was private. A following that cannot be made (its project or its store cannot be opened) ended the task the same way, and once the state asks only when what it follows changes nothing would have asked again: making it is its first look, told and tried again at that pace with the newest `Followed` asked. The log is made to fail whole by the test kit, `yunta_testkit::events::{set_log_aside, put_log_back}`, which takes `rusqlite` for it, so the interface's crate names no store; a following that cannot be made is a real log set to mode 000 and back. `yunta-ui` names the features of tokio it uses (`rt`, `sync`, `macros`, `time`).

**Fails first:** `crates/ui/tests/performer.rs`: a performer following a field whose store is made unreadable between two looks hands `Input::LookFailed` with its why, and, the store readable again, hands the field's next news with no second `Request::Follow`. Red: the task ended at the failure, so no news comes.

**Acceptance criteria:**
- [x] A following outlives a look that fails; a failure is handed once a failed look, never in a loop tighter than the follower's own pace.
- [x] `crates/ui/tests/performer.rs`'s other tests pass as they stand.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test performer`, `cargo test -p yunta-ui`

**Dependencies:** Task 44

**Files likely touched:** `crates/ui/src/{input.rs,state.rs}`, `crates/ui/src/performer/following.rs`, `crates/ui/Cargo.toml`, `crates/ui/tests/performer.rs`

**Estimated scope:** S

### Task 45: what the state follows, and what it keeps of it

**Description:**
- **`State::followed() -> Followed`**: the field, at the reach of the field's stage wherever the field or the first request stands among the stages, and at `Here` before any stage stands in a repository set up; and the run of the nearest run's stage, under every stage over it. Nothing on the signpost or a setup. `Stage` gains what `followed()` reads of a stage: the run a run's stage shows, and the reach a field's stage reads.
- **`State::update` ends by comparing `followed()` with what it last asked**, and asks `Request::Follow` where it differs; never twice for the same.
- **`FollowedRuns { field: Option<FieldInView>, in_view: Option<Arc<RunInView>> }`** in `crates/ui/src/followed_runs.rs` (new). Each is changed by `Arc::make_mut`, so a walk's clone of a state shares them.
  - `FieldInView` is made from Phase 1's `FollowedField` and the reach it was asked at: the field's runs by id, each a `FollowedRun` behind `Arc`, the runs it cannot read, how many it leaves elsewhere, whether it is the repository's, and its reach. `FieldInView::rows(now)` makes its rows, each run's `FollowedRun::row(now)`, the one way `yunta list --runs` makes them.
  - `RunInView { run: FollowedRun, moments, by_node, handed_over, heard, awaited: Option<AwaitedReading>, remarks }`. `AwaitedReading::{Read(Awaited), Unread { why }}` is what the run waits on a person for, as the follower read it.
- **Every arm folds, in Phase 1's spelling, with no wildcard arm:**
  - `FollowNews::Field` replaces the field; `Joined` adds a run to its runs, taking it out of those it cannot read; `Unreadable` moves a run among those it cannot read, with why; `Left` takes a run out of the field; `Elsewhere` replaces how many it leaves elsewhere.
  - `FollowNews::Whole` makes the run in view, its moments `chronicle(&events)` indexed by node. `FollowNews::Run` folds its `RunNews` into each run kept under that id, the field's and the one in view.
  - `RunNews::Events` folds with `FollowedRun::took`, each event once: one at or below the head changes nothing; the run in view keeps the moments `took` hands back, indexed by node. `Engine` replaces the run's engine.
  - The rest change the run in view alone. `Said` goes into `heard`, with how many words were missed. `Successor` makes the successor, read whole, the run in view, which Task 55 moves the stages onto. `Awaited` sets `awaited` to `AwaitedReading::Read`, or to none; `AwaitedUnread` sets it to `AwaitedReading::Unread` with its why, which Phase 3 shows in place of the card (the target `card-unread`). `Document` keeps the document under its node in `handed_over`, read, or unread with its why. `Remark` is kept in `remarks`, once, for Phase 3's run stage to say once as a note; about a run the field alone holds, it changes nothing.
  - News handed again change nothing.
- **`FollowedRuns::look_failed: Option<String>`** keeps what `Input::LookFailed` (Task 44a) says, and the next piece of news clears it: what was read before stays, said to be as old as the last look that read. The field's stage and a run's stage say it where they say a reading that failed at the base (Task 52, Task 53), so the person is never shown a stale field as a live one.
- **`State::caught_up(&BTreeMap<RunId, Seq>) -> bool`**: the field it follows is read; every run it keeps stands at the head the map gives it; the run it follows is read whole; and where that run waits on a person with a menu or a round, what it waits for is read for that pause. What a test asks before it gives a stroke.
- No stage reads any of it yet.
- Names said free in `crates/`: `FollowedRuns`, `FieldInView`, `RunInView`, `AwaitedReading`, `caught_up`. `FollowedRuns`' `///` carries "Not `yunta-api`'s `FollowedRun`, one run as the follower hands it."; `FieldInView`'s "Not `yunta-api`'s `FollowedField`, the field as the follower hands it whole."; `AwaitedReading`'s "Not `yunta-api`'s `Awaited`, which only a read that succeeded hands."
- Knowledge: `crates/ui/src/state.rs`'s `//!`: what the state follows is a function of where the person is, and what is followed folds into what the state keeps, which every stage reads; the line "news of lasting work goes to the stage" is kept to the cases' news. `followed_runs.rs`'s `//!`: what it keeps, why behind `Arc`, that the fold takes an event once, and that a remark waits with the run in view to be said once.

**Found in its turn:** `State::followed()` and the asking at the end of `update`; `crates/ui/src/followed_runs.rs` with `FollowedRuns` (and `look_failed`), `FieldInView`, `RunInView`, `AwaitedReading`; the fold in `crates/ui/src/state/followed.rs`, since `state.rs` reached 500 lines. `RunInView` also keeps `missed`, the words missed, and `no_successor`, since `NoSuccessor` folds. News of what is not followed is dropped, and a `Whole` or `Joined` at or below the head held changes nothing. `AwaitedReading` carries its pause, `Read { pause, awaited }` and `Unread { pause, why }`, and `caught_up` is false while the reading kept is for another pause than the one the run stands at: `AnswerablePause::seq()` says the event it was read for, and a round is keyed by `QuestionRound::asked_at` (new in the gates' ledger), where the follower counted a node's rounds. `caught_up` also asks that every run of the log stands in the field, read, unread or counted elsewhere. Git runs twice at opening until Task 51 takes the field's own reading out: the guard in `crates/ui/tests/state.rs` asserts the two by name, and waits with a local `caught_up` helper that Task 47 folds into `drive`. `Remark` gains `PartialEq`; three tests that matched request lists whole expect the `Follow`.

**Fails first:** `crates/ui/tests/followed.rs` (new):
- a look that failed is kept with its why, and the next news of the field clears it;
- in a repository set up, the repository read, the state asks `Request::Follow` with the field here before it stands anywhere;
- a run's stage opened from the field asks it with the run; its stats opened over it ask nothing more; going back to the field asks it without the run;
- every piece of news a bench's world hands, handed twice, leaves the state's `Debug` as handed once;
- a bench's run parked on a decision whose shown document is broken in its objects, as Phase 1's following test breaks it: the state keeps the why as what the run waits on, and the next news of the run still folds;
- `caught_up` is false until the field and the run are read whole, and true at the log's heads.

Red: there is no `followed()`.

**Acceptance criteria:**
- [x] `followed()` names the run under its stats, its graph, its decision, its evidence, the palette and help.
- [x] The fold matches every arm of `FollowNews` and `RunNews` with no wildcard, so an arm added to either fails to compile here.
- [x] No screen changes: `tui`'s fifteen frames in `crates/tui/goldens/screens/` are the guard, and pass unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test followed`, `cargo test -p yunta-ui`, `cargo test -p yunta-tui --test screens`

**Dependencies:** Task 44

**Files likely touched:** `crates/ui/src/followed_runs.rs` (new), `crates/ui/src/{state.rs,stage.rs,lib.rs}`, `crates/ui/tests/followed.rs` (new)

**Estimated scope:** M

### Task 46: the walk fed what is followed, and its bound measured

**Description:**
- **`Recordings` records what is followed.** A state is opened with a performer in each place where the recordings read a field today (`crates/testkit-ui/src/recorded.rs:82-85`) and in the reviewing run's, and opened on `NextStep::Follow` of each run whose page they read today (`:191-205`) and of the reviewing run `review` opens on. Every `Input::FollowNews` it takes is kept, until it is `caught_up` with the log; then the same at the machine's reach. Each piece is kept by what it is about: the field at its reach, or the run it names. `Recordings::followed(&Followed)` hands them.
- **The walk hands each recorded piece about a state's `followed()`** as `Input::FollowNews`, beside its strokes, whatever the state waits on. The fingerprint includes `followed()`. Every `Request::Follow` a state asks is one the recordings answer, or the walk names the inputs that asked it.
- **`crates/ui/tests/walk.rs` holds criterion 4's rule:** every state whose trail names a run, under any stage over the run's, follows that run.
- **The walk's states, palettes, helps and time**, with the news fed and no screen reading them, are written beside the base's in `.plans/yunta-tui/proof/live-screens/walk-fed.txt`. A count past `MOST_STATES` stops for the user: it is a cap.
- Knowledge: `crates/testkit-ui/src/recorded.rs`'s and `walk.rs`'s `//!`: what is followed is recorded by place and handed to every state that follows it, and the fold takes an event once, which bounds the walk.

**Found in its turn:** the recordings keep what is followed by place (`crates/testkit-ui/src/recorded/followed.rs`, since `recorded.rs` reached 690 lines), the walk hands each state the recorded news of what it follows and fingerprints `followed()`, and the ui walk holds that a trail naming a run follows that run. With `followed()` in the fingerprint the walk reaches 45,507 states where the base reached 22,388: a stage over the field here and the same stage over the field at the machine's reach draw alike and follow apart. That passed the cap of 40,000, which stops for the user: the user chose to raise the cap, and `MOST_STATES` is 60,000, rather than tell states apart by the run alone. Fed, the ui walk takes 59.5 s (30 palettes, 70 helps) and the tui walk 59.7 s (5,215 screens at each size, 70 more than the base: the states opened on following a run), measured beside another session's builds, against 19.3 s and 22.0 s at the base. The places never set up are told they are set up by the field's recordings, as `set_up()` does; nothing recorded follows an empty field at the machine's reach.

**Fails first:** the new rule in `crates/ui/tests/walk.rs`, run once with `followed()` made to forget the run under its stats: it names the inputs that reach the stats over the run.

**Acceptance criteria:**
- [x] The walk ends, and every rule it holds at the base still holds.
- [x] Its states, palettes, helps and time stand in `walk-fed.txt` beside the base's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test walk`, `cargo test -p yunta-tui --test walk`, `cargo test -p yunta-testkit-ui`
- [x] Manual check: `walk-fed.txt` read beside the base's counts.

**Dependencies:** Task 45

**Files likely touched:** `crates/testkit-ui/src/{recorded.rs,walk.rs}`, `crates/ui/tests/walk.rs`, `.plans/yunta-tui/proof/live-screens/walk-fed.txt` (outside git)

**Estimated scope:** M

### Task 47: the helpers that drive a state wait for what is followed

**Description:**
- **`Performer::workspace()`**: the workspace a performer works through, whose project names the store it follows.
- **`yunta_testkit::events::heads(store) -> BTreeMap<RunId, Seq>`**: each run's last seq, read with `Storage::heads`.
- **`drive`** (`crates/testkit-ui/src/driving.rs`) presses its next stroke once the state waits on nothing and is `caught_up` with the heads of its performer's store; it takes inputs until then.
- **`screens.rs`'s states**, driven by `drive`, stand caught up; **`crates/cli/tests/interface/run.rs`'s `followed_through`** waits the same way. `followed_through` takes inputs until the run in view no longer moves and the state is caught up, and calls `followed` on the run's first reading as followed news.
- Knowledge: `driving.rs`'s `//!`: a following has no ticket to wait on, so a helper waits on the log.

This task runs beside Task 46: they touch disjoint files.

**Found in its turn:** `drive` presses a stroke once the state waits on nothing and is caught up with the heads of its performer's store, asks again at its start for what the state follows (a state made by an earlier `drive` would never catch up), and panics after 30 s of waiting with what the state follows; `yunta_testkit::heads(store)` reads the heads, the one way (the two copies Tasks 45 and 46 wrote are gone); `screens::World` is public; `followed_through` waits until the run in view no longer moves and the state is caught up. Two things it found outside its files: a performer handed a request with other inputs than its following hands to kept feeding the first, dropped ones, so the following is made again for the inputs that ask; and a promoted run's successor, handed by the follower after the run's stage had moved onto it through its own following, was thrown away and left no run in view, so the fold keeps it as the run in view (a test in `crates/ui/tests/followed.rs`, red first; Task 55 takes the stage's own following out). The opening's guard uses `drive`.

**Fails first:** `crates/testkit-ui/tests/driving.rs` (new): the screens world driven with no stroke stands caught up with its store's heads, its parked run's decision read. Red: `drive` returns once the state waits on nothing, before what is followed arrives.

**Acceptance criteria:**
- [x] `tui`'s fifteen frames pass unchanged, and two builds of each screen are equal (`crates/testkit-ui/tests/screens.rs`).
- [x] The interface's end-to-end tests pass: `crates/cli/tests/interface.rs`.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-testkit-ui`, `cargo test -p yunta-tui --test screens`, `cargo test -p yunta --test interface`

**Dependencies:** Task 45

**Files likely touched:** `crates/testkit-ui/src/{driving.rs,screens.rs}`, `crates/testkit-ui/tests/driving.rs` (new), `crates/testkit/src/events.rs`, `crates/ui/src/performer.rs`, `crates/cli/tests/interface/run.rs`

**Estimated scope:** M

### Group check: the walk's bound, measured first
- [x] The suite's groups green; the walk's states and time beside the base's in `walk-fed.txt`; `tui`'s fifteen frames and every golden's digest equal the base's.

**Found in its check:** 320 binaries and 4,977 tests; the base's by name but the ones Phase 1 moved or replaced, and 119 new. 655 goldens, none changed or gone, the 408 new ones the targets. Three tests failed beside another session's build and pass alone: two of the engine's process tests saw a member of a command's group after its outcome, and the test kit's wait for a pid never recorded timed out; run again three times each, the engine's `process` binary failed once more on a third test. None is of this module's files: the race is recorded as open work in `.agents/open.md`.

### Task 48: the performer takes its channel from its surface

**Description:**
- **`Performer::new(workspace, channel)`**: a recorded answer carries the channel the performer was made with, where it hard-codes `Channel::Tty` (`crates/ui/src/performer/run.rs:113-119`, its comment goes).
- The binary hands `Channel::Tty` (`crates/cli/src/application.rs:41`); every test makes its performer with `Channel::Tty`.
- Mechanical over every caller of `Performer::new`, which the compiler names.
- Knowledge: `crates/ui/src/performer.rs`'s `//!` says a surface hands its channel at construction, so `crates/ui/src/lib.rs`'s "names no terminal" holds.

**Found in its turn:** `Performer::new(workspace, channel)` keeps the channel and `run::answered` takes it; the binary hands `Channel::Tty`. The unit test under `crates/ui/src/` records no answer and is made with `Channel::Mcp`, so the criterion that `Channel::Tty` is named nowhere in `crates/ui/src/` holds over the task's own "every test makes its performer with `Channel::Tty`". The compiler named two callers beyond the list, `crates/ui/tests/followed.rs` and `crates/testkit-ui/src/recorded/followed.rs`.

**Fails first:** `crates/ui/tests/performer.rs`: a round answered by a performer made with `Channel::Mcp` is on the run's log under that channel. Red: every answer says `Tty`.

**Acceptance criteria:**
- [x] `Channel::Tty` is named nowhere in `crates/ui/src/`.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui`, `cargo test -p yunta-testkit-ui`, `cargo test -p yunta --test interface`, `cargo test -p yunta-tui`

**Dependencies:** Group check: the walk's bound, measured first

**Files likely touched:** `crates/ui/src/{performer.rs,performer/run.rs}`, `crates/cli/src/application.rs`, every caller of `Performer::new`: `crates/testkit-ui/src/{recorded.rs,screens.rs}`, `crates/tui/src/application/tests.rs`, `crates/ui/src/performer/tests.rs`, `crates/ui/tests/{catalog.rs,dialogs.rs,evidence.rs,first_run.rs,packs.rs,performer.rs,scroll.rs,setup.rs,state.rs,stats_graph.rs,upkeep_schema.rs,support/mod.rs}`, `crates/cli/tests/{interface.rs,interface/decision.rs,interface/field.rs,interface/questions.rs,interface/run.rs,interface/start.rs}`

**Estimated scope:** L (mechanical: the compiler names every caller)

### Task 49: the repository read with where its runs are kept and its prices

**Description:**
- **`RepositoryReading { repository, storage, prices }`** in `crates/ui/src/outcome.rs`, carried by `Outcome::Repository(Box<RepositoryReading>)`: where Yunta stands in the repository, where the project keeps its runs, and what it prices tokens at (`config.pricing`). Its `///` says "Not `yunta_api::Repository`, which says only how far Yunta went there." Name said free in `crates/`.
- The performer reads them with the repository (`Operation::ReadRepository`).
- The state keeps the storage and the prices in `FollowedRuns`, which every stage is handed from Task 51 on.
- Mechanical over every constructor of `Outcome::Repository` and every match on its payload, which the compiler names; a stage's `Outcome::Repository(_)` arm is unchanged.

**Found in its turn:** `RepositoryReading { repository, storage, prices }` rides `Outcome::Repository`, filled by the performer from the project's store path and its prices, and the state keeps both on `FollowedRuns` (`storage` an `Option` until the repository is read). The reading moved into `repository_read`, and the kit's `read_as`, which hands a place's real store and prices, into `recorded/followed.rs`, each to keep a function or a file under its count. No test holds yet that the state keeps the two: the field's stage (Task 51) and a run's stats (Task 57) are the first to read them.

**Fails first:** `crates/ui/tests/performer.rs`: the repository of a project whose configuration prices a model, read, says those prices and the project's store. Red: the outcome carries neither.

**Acceptance criteria:**
- [x] The application opens where it opens at the base: `crates/ui/tests/state.rs` and `setup.rs` pass with their constructors changed.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui`, `cargo test -p yunta-testkit-ui`

**Dependencies:** Task 48

**Files likely touched:** `crates/ui/src/{outcome.rs,performer.rs,state.rs}`, `crates/ui/src/followed_runs.rs` (new) from Task 45, `crates/testkit-ui/src/recorded.rs`, `crates/ui/tests/{performer.rs,state.rs,setup.rs,support/mod.rs}`

**Estimated scope:** L (mechanical: the compiler names every constructor)

### Task 50: the application's tests stand on fields the follower reads

**Description:**
- **`support::on_the_field`** (`crates/ui/tests/support/mod.rs:38-67`) **follows a store of stopped runs a bench makes**, where it handed the state a reading it made up: each run with the id the test names, stopped as the engine's own tests stop a run, beside one another in one store (`Bench::beside`). The state is opened with a performer in that bench's world and driven with `drive` until it waits on nothing and is caught up with the store's heads (Task 47).
- Its callers name ids where they made rows up, and become tokio tests; `support::stopped` (`:22-36`) goes. The helper waits on what is followed, so its callers stand on the field alike before and after Task 51 decides where the application first stands.
- Mechanical over every caller of `support::on_the_field`, which the compiler names.
- Knowledge: `support/mod.rs`'s `//!` says the field a test stands on is read by the follower from runs a bench made.

**Found in its turn:** `support::on_the_field(runs)` makes a bench's world, each named run beside it stopped at the loop's first check (a token stopped before creation would refuse the creation, which runs git under it), and drives a state opened there until it is caught up; `support::stopped` is gone, and thirteen files' tests became tokio tests, `followed.rs` among them, a caller the list missed. The callers' binaries take 12.4 s where they took 8.8 s. The helper drops its world once the field is read, so the state keeps the path of a store that is gone: the first task whose stage reads the store (Task 51) has the helper hand its world back with the state.

**Fails first:** none: a helper moved under its callers. Its guard: every caller's assertions pass unchanged, on runs of the ids they name.

**Acceptance criteria:**
- [x] No test stands on a field it made up: `support::stopped` is gone, and `support::on_the_field` hands the state no reading of its own.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui`

**Dependencies:** Task 49

**Files likely touched:** `crates/ui/tests/support/mod.rs`, and every caller of `support::on_the_field`: `crates/ui/tests/{catalog.rs,decision_stage.rs,dialogs.rs,doctor.rs,evidence.rs,field.rs,packs.rs,palette.rs,run_stage.rs,scroll.rs,state.rs,stats_graph.rs,upkeep_schema.rs}`

**Estimated scope:** L (mechanical: the compiler names every caller)

### Task 51: where the application first stands, decided on the field's first news

**Description:**
- **In a repository set up, the repository read asks no field** (`crates/ui/src/state.rs:249-264` goes). `followed()` names the field, and the state stands once the field's first news is folded: on the first request where it holds no run and none it cannot read, on the field otherwise. Until then the screen says the runs are being read, as at the base.
- **The first request stands while the person is on it**, whatever the field's news say. Coming back to it with runs in the kept field gives the field its place (`FirstRunStage::returned` reads the kept field; its `Outcome::Field` arm goes, `crates/ui/src/stage/first_run.rs:97-102`).
- **The field's stage is made from the kept field at the instant**: `FieldInView::rows(now)`, each kept run's `FollowedRun::row(now)`, with the runs it cannot read, how many it leaves elsewhere and whether it is here, handed to `FieldStage::read` as the `FieldReading` it takes today, with the storage `FollowedRuns` keeps; Task 52 has the stage read the kept field at each draw.
- Knowledge: `crates/ui/src/state.rs`'s `//!` (where it first stands, and when); `first_run.rs`'s `//!` (it stands until the person leaves it).

**Found in its turn:** in a repository set up the repository read asks no field: the state stands once the field's first news is folded (`stands_on_first_news`), on the field where it holds a run, read or not (`FieldStage::holds_a_run`), on the first request where it holds none; the field's stage is made from the kept field (`FieldStage::kept`), and `Stage::returned` takes `&FollowedRuns` and the instant. The opening's guard asserts one run of git again. The criterion that `field.*` and `confirmation.*` pass unchanged was wrong: the base read a field's rows once, at the performer's clock, so a field 454 s old said `0s` beside a run's screen saying `7m34s`; a row's age is now worked out at the interface's clock, and seven lines of four frames move from `0s` to `7m` (`field.ascii-80`, `field.unicode-60`, `confirmation.ascii-80`, `confirmation.unicode-60`: the run's age and `the newest closed 7m ago`), for the person to see at Checkpoint B. `screens.rs` writes no frame, so they were written from what the code draws. Coming back to the field asks nothing and takes the kept field (`FieldStage::returned`), brought forward from Task 52, since a test of the opening on a review could not pass with the return reading the field again. The walk reaches 48,120 states, 34 palettes, 70 helps, and draws 5,641 screens at each size. Its fingerprint holds the screen, the work waited on, the scroll tops, the stages yet to open and `followed()`, and nothing the state keeps of what is followed: a stage over the field is one state whatever field is kept under it, each of those fields being reached as a screen of its own.

**Fails first:**
- `crates/ui/tests/state.rs`: in a repository set up, the state stands nowhere and asks no `Operation::Field` once the repository is read; the field's first news stands it on the field; and in a repository with no run, the field's first news, empty, stands it on the first request. Red: it asks the field.
- `crates/ui/tests/first_run.rs`: a run started in the same store while the request is typed leaves the request and its characters standing; coming back from the start finds the field.

**Acceptance criteria:**
- [x] `field.*` and `confirmation.*` in `crates/tui/goldens/screens/` are the guard of this screen moving onto followed data: they move only in the ages their rows say, seven lines from `0s` to `7m`, worked out at the interface's clock where the base froze them at the reading's.
- [x] A repository with no run opens on the first request, decided on followed news alone.
- [x] Opening on the field runs git once again: `crates/ui/tests/state.rs`'s `opening_on_the_field_runs_git_as_the_printed_field_does` asserts one run, where it asserts two between Task 45 and this task (the field's own reading and the following's).

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui`, `cargo test -p yunta-tui --test screens`, `cargo test -p yunta --test interface`

**Dependencies:** Task 50

**Files likely touched:** `crates/ui/src/{state.rs,stage/first_run.rs}`, `crates/ui/src/followed_runs.rs` (new) from Task 45, `crates/ui/tests/{state.rs,first_run.rs}`

**Estimated scope:** M

### Group check: `yunta-ui` on the follower
- [x] The suite's groups green; every golden's digest and `pages.sh`'s captures equal the base's; the walk's counts beside `walk-fed.txt`'s.

**Found in its check:** 320 binaries and 4,985 tests, none failing: the base's by name but five (four Phase 1 moved or replaced, and the return to the field's, renamed by Task 51), and 125 new. 655 goldens: four changed, the field's and the confirmation's frames in the ages Task 51 moved, none gone, the 408 new ones the targets. `prints.sh`'s 159 views equal the base's. The walk after Task 51: 48,120 states, 34 palettes, 70 helps, 5,641 screens at each size, in `walk-fed.txt`.

### Task 52: the field from the kept states

**Description:**
- **`FieldStage` keeps no reading and no storage**: its pointer, its marks, its reach, its depth and what it said. `Stage::screen`, `pressed` and `offered` hand it `&FollowedRuns`, and the field is drawn from the kept states at the interface's clock at each draw: its rows are `FieldInView::rows(now)`, each run's `FollowedRun::row(now)` with its menu as the follower kept it for its head, so a draw walks no engine.
- **Rows move to their groups as their runs move**, and the tally and the attention row count them with no stroke.
- **The pointer and the marks are kept by run.** A pointer whose run left the field stands on the first run listed; a mark whose run left goes.
- **`a` reaches the machine by what is followed**: `followed()` names the field at the stage's reach, and the field's news at that reach replace it.
- **No act asks the field again** (`crates/ui/src/stage/field.rs:244-268`); coming back to it already asks nothing, since Task 51 (`FieldStage::returned`).
- **A look that failed is said where the field says a reading that failed at the base** (`FollowedRuns::look_failed`, Task 45), over the field as it was last read, and goes with the next news.
- Knowledge: `field.rs`'s `//!` (drawn from what the state keeps, framed at each draw, the pointer kept by run).

**Found in its turn:** `FieldStage` keeps its pointer, its picks, its reach, its depth and what it said, and `screen`, `pressed` and `offered` frame the kept field at the interface's clock each time; the pointer is kept by run, a pick whose run left goes at the next key, no act reads the field again, and `a` only follows the other reach. No golden moved. Where the reach changes, the field kept stays in sight until the other reach's first news (`FollowedRuns::asked`), since the one sentence for a field being read says `this repository's runs`, which is wrong at the machine's reach. A look that failed is said as a note over the field last read and goes with the next news, with its test. `support::on_the_field` hands its bench back with the state, and gains `stopped_beside` and `performer_in`; seven tests of `evidence`, `stats_graph` and `scroll` drove a field opened in one world in another where its run had finished, and now press `d` first, a finished run standing with the closed. `stage.rs` reached 511 lines: its mapping of steps moved to `stage/steps.rs`. The test of a reading asked again is replaced here, before Task 61. The walk reaches 47,737 states (fewer: nothing waits on a field's reading), 34 palettes, 70 helps, and draws 5,742 screens at each size (more: each kept field draws its own).

**Fails first:** `crates/ui/tests/field.rs`:
- a run of the field parked by its bench while the field stands, its news handed and no stroke, moves under needs you; the tally and the attention row say it; the pointer stays on the run it stood on. Red: the field stands on the reading it was made with.
- with no news, two drawings a second apart differ only in their ages.
- `a` asks to follow the machine's field, and the pointer stays on its run.

**Acceptance criteria:**
- [x] `field.*` and `confirmation.*` in `crates/tui/goldens/screens/` are the guard, and pass unchanged.
- [x] `crates/cli/tests/interface/field.rs` closes two picked runs and finds them among the closed runs with no reading asked.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test field`, `cargo test -p yunta-ui`, `cargo test -p yunta-tui --test screens`, `cargo test -p yunta --test interface field`

**Dependencies:** Group check: `yunta-ui` on the follower

**Files likely touched:** `crates/ui/src/{stage/field.rs,stage.rs}`, `crates/ui/src/followed_runs.rs` (new) from Task 45, `crates/ui/tests/field.rs`, `crates/cli/tests/interface/field.rs`

**Estimated scope:** M

### Group check: the field
- [x] The suite's groups green; `field.*` and `confirmation.*` equal the base's; the walk's counts written beside `walk-fed.txt`'s.

**Found in its check:** run on the groups its one task can reach, `crates` and the CLI's five: 175 binaries and 3,434 tests, none failing. The engine's five groups were left out, green at the check before with no source of theirs changed since (Task 52 touched `crates/ui/` and one test of the CLI's interface, which the engine does not depend on); the whole suite runs again at the next check. `field.*` and `confirmation.*` equal what Task 51 left, the seven ages apart from the base, and no other golden moved: 655 goldens, four changed, none gone. The walk after Task 52: 47,737 states, 34 palettes, 70 helps, 5,742 screens at each size.

### Task 53: a run's rows, nodes and chronicle from the run in view

**Description:**
- **`RunStage` keeps no `followed`, `moments` or `heard`.** Its screen, its acts and its pointer read `FollowedRuns::in_view` where it is the stage's run: the ribbon, the rows across the screen, the nodes beside, and the chronicle under the page from the kept moments, the frame made at the interface's clock at each draw.
- **Whether the run offers a menu is the run in view's `FollowedRun::menu`**, kept for its head, where the stage read the `FollowedRun` its own following handed.
- **The pointer stays on its node**, and falls to the first node where its node is gone.
- **A look that failed is said on the run's screen where it says a reading that failed at the base** (`FollowedRuns::look_failed`, Task 45), over the run as it was last read, and goes with the next news.
- **Picking a node shows its page while the run's rows go on moving with its news** (criterion 4). The stage still asks its own following for the page it reads again where that following ends or hands a successor; Task 54 and Task 55 replace those, and what that following hands is no longer kept.
- Knowledge: `crates/ui/src/stage/run.rs`'s `//!` (the rows, nodes and chronicle are the run in view's).

**Found in its turn:** `RunStage` keeps no `followed`, `moments` or `heard`: its ribbon, rows, nodes, chronicle, acts and pointer read the run in view where it is the stage's run, framed at the interface's clock at each draw, the pointer kept by node; from its own following the stage takes only a successor. No golden moved and the run's screen held no frozen age. A look that failed is a note under the page, as on the field. What a node said is let go once it no longer works, as the stage did at the base: the fold keeps what was heard to the nodes at work when the run's head moves (`RunInView::took`), and words from before their node last started or ended are neither heard nor counted, so news handed again changes nothing and the words missed are counted once; the run in view's `impl` moved to `followed_runs/in_view.rs`, and the two tests to `tests/run_stage/said.rs`, each file having passed 500 lines. The walk passed its cap of 60,000: the recordings answered a stage's own following by kind and the base's stage took any run read whole, so the deciding, asking and reviewing runs' stages all showed one run, and each now draws its own: 62,648 states, 41 palettes, 88 helps, 6,953 screens at each size. The user's standing word: the cap goes up with a margin whenever new real screens pass it, reported and not stopped for, and the work stops only if a walk takes past three minutes on an idle machine. `MOST_STATES` is 80,000.

**Fails first:** `crates/ui/tests/run_stage.rs`: with a node picked, the run's next events, handed as followed news, move its rows and its chronicle. Red: picking lets the stage's following go, and the stage reads only what its following hands.

**Acceptance criteria:**
- [x] `run.*`, `palette.*`, `help.*`, `card.*` and `desk.*` in `crates/tui/goldens/screens/` are the guard, and pass unchanged.
- [x] The chronicle ends with what `status --follow` writes through a pipe for the same run (`crates/cli/tests/interface/run.rs`).

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test run_stage`, `cargo test -p yunta-ui`, `cargo test -p yunta-tui --test screens`, `cargo test -p yunta --test interface run`

**Dependencies:** Group check: the field

**Files likely touched:** `crates/ui/src/{stage/run.rs,stage.rs}`, `crates/ui/tests/run_stage.rs`, `crates/cli/tests/interface/run.rs`

**Estimated scope:** M

### Task 54: the run's page drawn with the kept frame, and read again when the run turns

**Description:**
- **At each draw the page stands as `page::Run { frame, state, .. }`** with the kept frame at the instant and the kept state, so its headline, its furrow and its gist move with the run.
- **`Stage::followed_on(&mut self, &FollowedRuns) -> Vec<Want>`**: what the stage the person is at asks once the state folded a piece of followed news; the state calls it after each fold. The run's stage asks the page it shows, the run's or its picked node's, again where the kept run's phase, its pause or its successor differs from those it was last read at. Its re-read where its own following ends goes (`crates/ui/src/stage/run.rs:303-306`). Name said free in `crates/`.
- Knowledge: `run.rs`'s `//!` (the page's headline, furrow and gist are the kept frame's; the page is read again when the run turns), replacing the sentence that says it is read once more "once the run stops or stalls".

**Found in its turn:** the run's page is drawn at each draw with the run in view's frame and state, so its headline, furrow and gist move with the run; `Stage::followed_on(&FollowedRuns)`, called after each fold, has the run's stage read the page it shows again when the kept run's phase, pause or id (a successor) differs from the one it last asked at, and the reading at the end of the stage's own following is gone. The page read as the stage opens counts for the run as first kept, so the first fold asks nothing. No golden moved. Leaving a picked node shows the run's page as last read, where only its decision and plans can lag, as at the base. `stage/run/page.rs` is new, `run.rs` nearing 500 lines, and the fold moved into `State::folded`. The walk: 60,620 states, 41 palettes, 88 helps, 6,893 screens at each size (fewer: the page draws at the walk's clock, and a following's end asks no page).

**Fails first:** `crates/ui/tests/run_stage.rs`: a run that finishes while its screen is open says finished in its headline with no stroke, and the stage asks its page once more, once. Red: the headline is the page's as read when it opened.

**Acceptance criteria:**
- [x] The frames of Task 53 pass unchanged.
- [x] `crates/cli/tests/application/watching.rs`'s run followed to its end reaches `FINISHED` with no key.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test run_stage`, `cargo test -p yunta-tui --test screens`, `cargo test -p yunta --test application watching`

**Dependencies:** Task 53

**Files likely touched:** `crates/ui/src/{stage/run.rs,stage.rs,state.rs}`, `crates/ui/tests/run_stage.rs`

**Estimated scope:** S

### Task 55: a promoted run's successor becomes the run in view

**Description:**
- **Folding a successor, the state moves every stage that shows the predecessor onto it** (`Stage::succeeded(run, by)`, name said free): the run's stage takes the successor's id, starts its page at the first line and reads it. `followed()` then names the successor, which the follower already follows.
- The stage's successor from its own following goes (`crates/ui/src/stage/run.rs:256-261`).
- Knowledge: `crates/ui/src/state.rs`'s `//!` (a promotion's successor takes the place of its run under every stage).

**Found in its turn:** `FollowedRuns::took` says the run a fold replaced and its successor, and `State::succeeded` moves the stage the person is at and each stage under it onto the successor (`Stage::succeeded`): the run's stage takes its id, drops a picked node and puts the page at its first line; `RunStage::heard` and `went_to` are gone, the stage's own following moving nothing. Already true: the page read again where the kept run's id changes (Task 54), `followed()` naming the successor (Task 45). Task 47's path for a successor handed after the stage had moved is removed with its reason, its test becoming this task's. No golden moved; the walk's counts stand (60,620 states, 6,893 screens at each size). As at the base, after a promotion `f` or a pick lets go the following of the successor while the stage waits on the predecessor's, which is never let go: Task 60 takes the stage's own following out.

**Fails first:** `crates/ui/tests/scroll.rs`'s `a_successors_page_starts_at_its_first_line`, made on followed news, and `crates/ui/tests/followed.rs`: once a promoted run's successor is folded, `followed()` names it and the trail ends with its handle. Red: the stage hears no successor from what is followed.

**Acceptance criteria:**
- [x] `crates/cli/tests/interface/run.rs`'s promotion and `crates/cli/tests/application/watching.rs`'s `a_promotion_is_followed_onto_its_successor` pass unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test scroll`, `cargo test -p yunta-ui --test followed`, `cargo test -p yunta --test interface run`, `cargo test -p yunta --test application watching`

**Dependencies:** Task 54

**Files likely touched:** `crates/ui/src/followed_runs.rs` (new) from Task 45, `crates/ui/src/{state.rs,stage.rs,stage/run.rs}`, `crates/ui/tests/scroll.rs`, `crates/ui/tests/followed.rs` (new) from Task 45

**Estimated scope:** M

### Task 56: the acts read what the run waits on from the run in view

**Description:**
- **`offered_in` reads the decision from `RunInView::awaited`** once it is read (`AwaitedReading::Read`), in place of the page's (`crates/ui/src/stage/run.rs:402-413`); answering stays offered from the kept menu or a round of questions.
- This task runs beside Task 57: they touch disjoint files.

**Found in its turn:** `RunStage::offered_in` takes the decision from `RunInView::awaited` where it is read and its pause is the one the run stands at, and none otherwise; answering is offered from the kept menu or a round, as before; the page's decision is read for no act. No golden moved. The walk reaches 69,926 states, 41 palettes, 88 helps, and draws 7,719 screens at each size (94 s and 89 s): a run's legend now differs before and after what it waits on is folded, and both are walked. `run_stage.rs` stands at 499 lines: its next test takes a submodule.

**Fails first:** `crates/ui/tests/run_stage.rs`: the run parked on its plan offers reading it (`p`) once what it waits on is folded, with its page never read. Red: the acts read the page's decision.

**Acceptance criteria:**
- [x] The palette over a parked run offers answering first, as at the base (`crates/ui/tests/palette.rs`).
- [x] The frames of Task 53 pass unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test run_stage`, `cargo test -p yunta-ui --test palette`, `cargo test -p yunta-tui --test screens`

**Dependencies:** Task 55

**Files likely touched:** `crates/ui/src/stage/{run.rs,run/acts.rs}`, `crates/ui/tests/run_stage.rs`

**Estimated scope:** S

### Task 57: stats and the graph from the run in view

**Description:**
- **A run's stats are made at each draw** from the run in view: `RunStats::at(state, workflow, now)`, its frame, and the prices `FollowedRuns` keeps. `StatsStage` asks no reading for a run; a workflow's past runs are read as at the base.
- **A run's graph is made at each draw**: `Graph { workflow, run: Some((run, frame)) }` from the run in view. `GraphStage` asks no reading for a run; a workflow's graph is read as at the base.
- Until the run in view is its run, each says the run is being read, as at the base. Each takes `Stage::succeeded`.
- Knowledge: `stats.rs`'s and `graph.rs`'s `//!` (a run's are made from the run in view, so they move with it).

**Found in its turn:** a run's stats and its graph are made at each draw from the run in view: the stats from `RunStats::at` and the frame at the instant, costed at `FollowedRuns::prices`, the graph from the run and its frame; neither stage asks a reading of a run, and until the run in view is the stage's run each says the base's note of a reading under way; `StatsStage::succeeded` and `GraphStage::succeeded` move onto a successor. A workflow's history and its graph as written are still read as the stage opens. The tests are the first to hold that the state keeps the prices (`avg of 1 priced model`), on a priced world of their own, `support::on_the_field` writing a config with none. No test holds `succeeded` on the two stages. `Outcome::Stats`, `Operation::ReadStats` and `ReadRunGraph` and their recordings are unused, for Task 61. No golden moved. The walk: 70,019 states, 41 palettes, 88 helps, 7,757 screens at each size (95.2 s and 92.9 s).

**Fails first:** `crates/ui/tests/stats_graph.rs`: a run's stats over its screen change with the run's next events, with no stroke, and at one instant equal what `yunta stats` prints at the project's prices; its graph labels a node with its new word once the node moves. Red: each is read once.

**Acceptance criteria:**
- [x] `crates/cli/tests/application/runs_left.rs` passes unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test stats_graph`, `cargo test -p yunta --test application runs_left`

**Dependencies:** Task 55

**Files likely touched:** `crates/ui/src/{stage/stats.rs,stage/graph.rs,stage.rs}`, `crates/ui/tests/stats_graph.rs`

**Estimated scope:** M

### Task 57a: a run's stats and its graph move onto its successor, held by a test

**Description:** Task 57 made `StatsStage::succeeded` and `GraphStage::succeeded` and named no test for them: stats or a graph opened over a run that is promoted would go on showing the predecessor, closed, if either were lost.
- A test over each: opened over a run, the run promoted, its successor folded, the stage shows the successor's handle and its figures (the graph, its nodes' words), its scroll at the first line, and `followed()` names the successor.
- Each is seen to fail with its `succeeded` taken out, and the stage put back as it was.

**Found in its turn:** two tests in `crates/ui/tests/stats_graph.rs` open stats and the graph over a stopped run, wake it answering `promote`, and hold the successor's handle in the trail, its figures as `yunta stats` and `yunta graph --run` print them, the scroll at its first line and `followed()` naming it; each is red with its stage's `succeeded` emptied, and both with the rewind alone taken out. They keep one following across the promotion, for two reasons found: a following started once the successor is already in the log never hands it (`PromotionChain::starting_at` counts every run logged as seen, and the run gets `NoSuccessor` after thirty seconds), and `drive` stands caught up before a successor comes: Task 58a.

**Fails first:** `crates/ui/tests/stats_graph.rs` (or a submodule of it): the two tests, red with `succeeded` made to do nothing on the stage each holds.

**Acceptance criteria:**
- [x] Stats and the graph over a promoted run show its successor with no stroke.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test stats_graph`

**Dependencies:** Task 57

**Files likely touched:** `crates/ui/tests/stats_graph.rs`

**Estimated scope:** S

### Group check: a run's screen as it is drawn today
- [x] The suite's groups green; `tui`'s fifteen frames equal the base's; the application's tests at a pseudo-terminal green; the walk's counts written beside `walk-fed.txt`'s.

**Found in its check:** 320 binaries and 4,995 tests, none failing, the application's at a pseudo-terminal among them: the base's by name but eight (four Phase 1 moved or replaced, four of the field's readings this phase replaced), and 138 new. 655 goldens: the four frames of Task 51's ages changed, eleven of `tui`'s fifteen equal to the base's, none gone. The walk after Task 57: 70,019 states, 41 palettes, 88 helps, 7,757 screens at each size, in `walk-fed.txt`.

### Task 58: the screen's title, and calling the person

**Description:**
- **`Screen::title: Option<String>`** in `crates/surco/src/components/screen.rs`: the window's title while a run needs the person; none leaves the window's own. Its `///` carries "Not a region's title." Markdown and HTML draw none of it.
- **The state sets it**: render's `watched::title` of the run in view where it needs the person; else of the field's run that waited longest among those answered here, which a run asking at its own terminal never is; none otherwise.
- **`Request::CallThePerson`**: folding followed news, `State::update` asks it once for each run whose standing turns to needs you after the field's first news. News of the same pause ask nothing more. The performer hands it back to the surface. Name said free in `crates/`.
- Knowledge: `crates/ui/src/state.rs`'s `//!` (when the state calls the person); `request.rs`'s `///`.

**Found in its turn:** `Screen::title`, `Request::CallThePerson`, which the performer hands back to its surface, and `crates/ui/src/state/calling.rs`, `state.rs` standing at 499 lines; the fold hands back requests. Decided: a field read whole, and a run read whole as it opens, call nobody, and a run that joins the field or a successor calls where it already needs the person; needing the person is standing `NeedsYou`, so a broken run neither calls nor takes the title; each run that needs the person keeps its row as of the news that last moved it, where framing the run in view at every draw took a third of a drawing and eleven seconds of the walk. A test that uses `drive` makes its following anew at each call and cannot see a call: `calling.rs`'s tests keep one following. No golden moved. The walk: 70,019 states, 7,757 screens at each size, 102 s and 96 s beside another build.

**Fails first:** `crates/ui/tests/calling.rs` (new): a bench's run parked while the field stands: the screen's title names it, and `Request::CallThePerson` is asked once; the next news of that pause ask nothing; a run that needed the person as the application opened asks nothing; the run answered, the title is none. Red: the screen has no title.

**Acceptance criteria:**
- [x] No screen's cells change: the fifteen frames pass unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test calling`, `cargo test -p yunta-surco`, `cargo test -p yunta-tui --test screens`

**Dependencies:** Group check: a run's screen as it is drawn today

**Files likely touched:** `crates/surco/src/components/screen.rs`, `crates/ui/src/{state.rs,request.rs,performer.rs}`, `crates/ui/tests/calling.rs` (new)

**Estimated scope:** M

### Task 58a: a run promoted before it is followed hands its successor

**Description:** a following of a run that closed promoted awaits a successor made after the following began (`PromotionChain::starting_at` counts every run already in the log as seen), so a run opened once its successor is in the log never moves onto it, and reads `NoSuccessor` thirty seconds later. Found by Task 57a. Phase 3's rule that a successor takes the screen while the run leads needs it true of a run promoted yesterday as of one promoted now.
- **A following that reads whole a run already closed promoted hands its successor**, read whole, as `RunNews::Successor`, right after the run's `Whole`, where the log already holds one; it awaits one, as today, where the log holds none, and says `NoSuccessor` only where none is and none comes.
- **A chain is followed to its end**: a successor that itself closed promoted with its own successor in the log hands that one the same way.
- **`State::caught_up`** is false while the run in view closed promoted and neither a successor nor `NoSuccessor` has folded, so a helper that drives a state waits for it.
- `yunta status --follow` on a run promoted before it is followed goes on with its successor at once, where at the base it waited for one that had already come: the report says what it printed before and after.
- Knowledge: the chain's `///` and `following.rs`'s `//!` say a successor already in the log is handed as one that comes.

**Found in its turn:** only a successor's own `run_created.promoted_from` ties it to the run it succeeds, so finding one meant reading runs: `LogReader::first_events` (and `Storage::first_events`) reads every run's first event in one statement on the `(run_id, seq)` key, and the follower asks it where it read each new run's log whole; the kept chain is gone, so nothing counted as seen hides a successor made between two looks. A run read already closed promoted is handed its successor right after its `Whole`, and a chain is followed to its end. The wait for a successor runs from the close's own instant: a run promoted longer ago than the wait, with none in the log, reads `NoSuccessor` in the look that reads it, and `NoSuccessor` is final (the run is asked about no more, and a run that names it later joins the field as any other). `RunInView::caught_up` waits for the successor or `NoSuccessor`. `yunta status --follow` on a run promoted before: with its successor in the log it prints the successor's page at once, where the base printed the promoted run's and said nothing of a successor (the task's "it waited" was wrong: the base's CLI checked only that the frame no longer moved); with none and promoted long ago, the run's page at once, as the base; with none and promoted a moment ago, the same page once thirty seconds from the close have passed, where the base printed it two seconds after. The successor's code moved to `crates/api/src/follow/successor.rs`. No golden moved; the walk's counts stand.

**Fails first:**
- the follower's tests (`crates/api/tests/following/`): a following begun on a run whose promotion and successor are already in the log hands the run read whole, then its successor read whole. Red: nothing comes of the successor.
- `crates/ui/tests/followed.rs`: a state opened on a run promoted before is not caught up until its successor is folded.

**Acceptance criteria:**
- [x] A run opened after its promotion shows its successor, in the application and in `yunta status --follow`.
- [x] A run promoted with no successor in the log reads as today: awaited, then `NoSuccessor`.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api`, `cargo test -p yunta-ui --test followed --test stats_graph --test scroll`, `cargo test -p yunta --test status_follow`, `cargo test -p yunta --test interface`

**Dependencies:** Task 57a

**Files likely touched:** `crates/api/src/follow/following.rs` and the chain's own file, `crates/api/tests/following/`, `crates/ui/src/state/followed.rs` (or where `caught_up` stands), `crates/ui/tests/followed.rs`

**Estimated scope:** M

### Task 59: the terminal's title and its bell

**Description:**
- **`yunta-tui` writes the window's title**: `surco::paint::title_saved` when the screen's title comes, `TITLE_RESTORED` then `title_saved` when it changes, `TITLE_RESTORED` when it goes and on every way out (`full_screen.rs`'s giving back).
- **It rings `surco::paint::BELL` on `Request::CallThePerson`** where `Application::new` is handed `bell`. The binary hands the project's `terminal.bell` (`config.terminal_bell()`), as the attached run does (`crates/cli/src/surface/region.rs:156-170`).
- Knowledge: `crates/tui/src/application.rs`'s `//!` (what only a terminal does: the title and the bell); `crates/cli/src/application.rs`'s `//!` (what it hands).

**Found in its turn:** `WindowTitle` (`crates/tui/src/full_screen.rs`), owned by the holding and shared with the panic hook and the loop, saves the window's title at the first one the application gives and gives it back once, on a drop, a panic or a suspend; each drawing gives the window the screen's title, and `Request::CallThePerson` rings the bell at the next drawing, after the title, where the project's config asks for a bell. After leaving, the window reads what it read before the application opened: the test holds one save and one restore, the restore after the last drawing; fifty stressed runs of it pass. A terminal with no title stack ignores both and keeps the run's title, as the attached run leaves it today; a project whose config cannot be read rings no bell. No golden moved, and nothing here reaches the walk.

**Fails first:** `crates/cli/tests/application/calling.rs` (new), at a pseudo-terminal: bare `yunta` on the field of a repository whose run, held by a word, parks once let go: the drawing writes the title naming the run after it parks, then the bell where the configuration says `terminal.bell: true`, and none where it does not; leaving gives the title back. Each an order. Red: nothing writes it.

**Acceptance criteria:**
- [x] The application's tests at a pseudo-terminal pass; the new one passes under `stress_binary.py`.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test application`, `cargo test -p yunta-tui`
- [x] `python3 .plans/yunta-tui/tools/stress_binary.py` on `calling`, no failure.

**Dependencies:** Task 58

**Files likely touched:** `crates/tui/src/{application.rs,full_screen.rs}`, `crates/cli/src/application.rs`, `crates/cli/tests/application.rs`, `crates/cli/tests/application/calling.rs` (new)

**Estimated scope:** M

### Group check: the title and the bell
- [x] The suite's groups green; the fifteen frames equal the base's.

**Found in its check:** 321 binaries and 5,010 tests, none failing: the base's by name but the eight this phase and Phase 1 moved or replaced, and 153 new. 655 goldens: the four frames of Task 51's ages changed, the other eleven of `tui`'s fifteen equal to the base's, none gone.

### Task 60: the run's own following, and its `f`, go

**Description:**
- **The run's stage asks no following.** `Operation::Follow`, `News::Following`, `Outcome::Followed`, the performer's `run::followed`, the stage's `follows` and the run's `f` go, with every caller; `render::screen::run_legend` loses `follows`. `News` keeps the cases' news. Picking a node lets nothing go.
- **The two help frames lose the `f` row**, for the user's approval at Checkpoint B: `help.ascii-80.txt` its row `f       follows`, its box one row shorter, `help.unicode-60.txt` its `… 7 more lines` for `… 6 more lines`, edited by hand as `tui`'s frames are written. No other base frame changes before its target lands.
- Tests that held the stage's following go or follow: `run_stage.rs`'s `stopping_following_lets_it_go_and_following_again_asks_anew` goes; the rest read followed news.
- `docs/guide.md`'s "The application at a terminal" says the field and a run's screen move as their runs do, with no key, and a run stays followed under every screen over it.
- Mechanical: every `Outcome` match that names `Followed` and every caller of what goes, which the compiler names.

**Found in its turn:** `Operation::Follow`, `News::Following`, `Outcome::Followed`, `run::followed`, the stage's `follows` and `following_or_not`, the key `f` and the legend's `follows` are gone with their recording; the stage asks only its page, and picking a node or resuming lets nothing go. The tests read followed news through one helper, `support::read_whole`, where five copies stood. `docs/guide.md` says the field and a run's screen move as their runs do, with no key to press. Two frames move, as the list's head says: `help.ascii-80` loses the row `f  follows` and shows the page's row the box, one row shorter, uncovers (`assignee: lead`), and `help.unicode-60` counts `… 6 more lines` where it counted seven. The walk: 63,842 states, 41 palettes, 72 helps, 6,683 screens at each size (fewer: the states that waited on the run's own following or toggled `f` are gone).

**Fails first:** none: a deletion. Its guard: the suite, the fifteen frames equal but the two named rows of `help.*`, and the walk's counts beside Task 46's.

**Acceptance criteria:**
- [x] No code names `Operation::Follow`, `Outcome::Followed` or a following a stage asks.
- [x] Only the two `help.*` frames differ from the base, by the `f` row.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui`, `cargo test -p yunta-testkit-ui`, `cargo test -p yunta-tui`, `cargo test -p yunta --test interface`, `cargo test -p yunta --test application`, `cargo test -p yunta --test docs_sync`

**Dependencies:** Group check: the title and the bell

**Files likely touched:** `crates/ui/src/{operation.rs,outcome.rs,news.rs,performer.rs,performer/run.rs}`, `crates/ui/src/stage/{catalog,decision,doctor,evidence,field,first_run,graph,run,setup,start,stats,upkeep}.rs`, `crates/ui/src/stage/catalog/cases.rs`, `crates/render/src/screen/run.rs`, `crates/testkit-ui/src/recorded.rs`, `crates/ui/tests/{run_stage.rs,decision_stage.rs,palette.rs,scroll.rs}`, `crates/cli/tests/interface/run.rs`, `crates/tui/goldens/screens/help.{ascii-80,unicode-60}.txt`, `docs/guide.md`

**Estimated scope:** L (mechanical: the compiler names every caller)

### Task 61: the readings of the field, a run's stats and its graph go

**Description:**
- `Operation::Field`, `Operation::ReadStats`, `Operation::ReadRunGraph`, `Outcome::Field`, `Outcome::Stats`, the performer's `run::field_read`, `run::stats_read` and `run::graph_read`, their recordings and their kinds go, with every caller. `Outcome::Graph` stays for a workflow's graph; `yunta_api::Project::field` and `StatsReading` stay for `yunta list --runs` and `yunta stats`.
- The tests that asked these readings follow: `crates/ui/tests/field.rs`'s `a_reading_asked_again_lets_go_the_one_under_way` goes, the behaviour it held gone; `crates/ui/src/state/tests.rs` reads followed news.
- Mechanical: every arm and caller, which the compiler names.

**Found in its turn:** `Operation::{Field, ReadStats, ReadRunGraph}`, `Outcome::{Field, Stats}`, `run::{field_read, stats_read, graph_read}`, their recordings and every arm on them are gone; no stage asked any since Tasks 51, 52 and 57. The state's test of a reading asked again letting go the one of its kind under way stays, on two runs' pages where it used the field (the task's text had it read followed news: wrong). Two tests can no longer say "asks no reading of the field" and hold that coming back asks no work at all. No golden moved; the walk's counts are Task 60's.

**Fails first:** none: a deletion. Its guard: the suite and the fifteen frames as Task 60 leaves them, and the walk's counts.

**Acceptance criteria:**
- [x] The application reads none of the field, a run's stats or its graph: each is followed.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui`, `cargo test -p yunta-testkit-ui`, `cargo test -p yunta-tui`, `cargo test -p yunta --test interface`

**Dependencies:** Task 60

**Files likely touched:** `crates/ui/src/{operation.rs,outcome.rs,performer.rs,performer/run.rs,state.rs,state/tests.rs}`, `crates/ui/src/stage/{catalog,decision,doctor,evidence,field,first_run,graph,run,setup,start,stats,upkeep}.rs`, `crates/testkit-ui/src/recorded.rs`, `crates/ui/tests/{field.rs,stats_graph.rs,support/mod.rs}`

**Estimated scope:** L (mechanical: the compiler names every caller)

### Group check: gone
- [x] The suite's groups green; the fifteen frames equal the base's but the two `help.*` rows; the walk's states and time written beside `walk-fed.txt`'s.

**Found in its check:** run on the groups its two tasks can reach, `crates` and the CLI's five: 127 results in the first and 59 binaries in the others, some 3,500 tests, none failing; the engine's groups were left out, green at the check before with no source of theirs changed since (the two tasks touched `crates/ui/`, `crates/testkit-ui/`, one legend of `crates/render/` and the guide). 655 goldens: six changed, the four frames of Task 51's ages and the two `help` frames of Task 60, none gone. `prints.sh`'s 159 views equal the base's. The walk: 63,842 states, 41 palettes, 72 helps, 6,683 screens at each size, in `walk-fed.txt`.

### Task 62: the CLI's queue of beats, named for what it is

**Description:**
- `git mv crates/cli/src/surface/feed.rs crates/cli/src/surface/beats.rs`, then `Feed` becomes `Beats`, with every caller, so `Feed` names Surco's rows alone. Name said free in `crates/`.
- This task, Task 63 and Task 66 touch disjoint files and run at once.

**Found in its turn:** `crates/cli/src/surface/feed.rs` is `beats.rs` and `Feed` is `Beats` in every caller, its fields and locals with it; no `Feed` stands in `crates/cli/src/`. One file beyond the list, `crates/cli/tests/run_words/row.rs`, whose `Reached by:` line named the old file.

**Fails first:** none: a rename. Its guard: the CLI's tests pass unchanged; the moved body read with `git diff -M`.

**Acceptance criteria:**
- [x] No `Feed` stands in `crates/cli/src/`.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --bin yunta surface`, `cargo test -p yunta --test run_surface`

**Dependencies:** None

**Files likely touched:** `crates/cli/src/surface/{feed.rs,mod.rs,painter.rs,turns.rs}`, `crates/cli/src/surface/beats.rs` (new), moved from `feed.rs`

**Estimated scope:** S

### Task 63: Surco's `Feed`

**Description:**
- **`surco::components::Feed { rows, before, after }`** (`components/feed.rs`, new) and `Block::Feed`: the rows in sight, the newest last, with how many stand above them and under them.
- **On a terminal** each row is cut at the edge and ends in the ellipsis, never folded, and the counts stand above and under in the inventory's words, where they are not zero. Markdown and HTML say the same rows and counts.
- Goldens in `crates/surco/goldens/feed/` (new) at Surco's environments, plain and 16-color, ASCII and Unicode; its place in the catalog, whose goldens are blessed.
- A new `Block` variant reaches every caller that matches on `Block`, which the compiler names.
- Knowledge: `components/mod.rs`'s `//!` names it; `feed.rs`'s `///` carries "Not the CLI's `Beats`, a queue of what a run's surface draws."

**Found in its turn:** `Feed { rows, before, after }` (`components/feed.rs`), `Block::Feed`, its drawing in `surface/terminal/feed.rs`, and Markdown and HTML saying the same rows and counts through one function; ten goldens written by hand first in `goldens/feed/` (scrolled back; at its end with a cut row). The counts read `… {n} above` and `… {n} under`, muted, where they are not zero; a row is cut at the edge, never folded, and a cut falls before a number it would split or end on. Eleven goldens blessed, 120 lines added and none removed: the catalog's seven (the feed at its end) and the every-block's four (the feed before `next`). The target's sample row `call yunta_task` names a run tool where a smell counter reads, so the goldens' sample is `search  global pack`; `markdown/mod.rs` stood at 500 lines, so its `line` moved to `markdown/text.rs`; `Terminal::laid` passed 50 lines, so a heading's drawing is its own method. The feed's rows stand at the heading's column with no indent, as the target draws them, which on the node's printed page (Task 67) differs from the blocks that indent under a heading. The rule that a cut never splits a number lives in the feed's drawing alone, `Line::cut` still cutting inside one: Task 73 makes it one rule for the attention row too. At the base the catalog's `KINDS` leaves out `rows`.

**Fails first:** the feed's goldens, written by hand from the inventory's rows before the drawing, red; and a unit test: a row past the edge ends in the ellipsis and stays one row, and a number in it is never cut.

**Acceptance criteria:**
- [x] The catalog and every-block goldens change by the feed alone, their diff reviewed at Checkpoint B.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco`
- [x] `YUNTA_BLESS=1 cargo test -p yunta-surco`, then the goldens' diff read.

**Dependencies:** Checkpoint A (the inventory's rows of the feed)

**Files likely touched:** `crates/surco/src/components/feed.rs` (new), `crates/surco/src/{doc.rs,components/mod.rs,components/catalog/kinds.rs,surface/terminal/mod.rs,surface/markdown/mod.rs,surface/html/blocks.rs}`, `crates/surco/goldens/feed/` (new)

**Estimated scope:** L (a new block reaches every caller that matches on `Block`)

### Task 64: Surco's meter row

**Description:**
- **`surco::components::Meter`** (`components/meter.rs`, new): a count with its label, one bar a node, and times each with its label. `Terminal::meter` (`surface/terminal/meter.rs`, new) draws it as one row joined by the separator, the bars through `bars::sparkline`; at a narrow width the bars narrow first, then the last time goes, and no number is cut. Name said free in `crates/`.
- Goldens in `crates/surco/goldens/meter/` (new), plain and 16-color, ASCII and Unicode; its place in the catalog.

**Found in its turn:** `Meter { count, bars, times }` and `Time { figure, label, by_person }` (`components/meter.rs`), drawn by `Terminal::meter(&Meter) -> Line` (`surface/terminal/meter.rs`), which a surface calls; also `Block::Meter`, Markdown and HTML drawing the same row. Five goldens written by hand in `goldens/meter/` from row 34 of `work`. Where the task says the bars narrow first, the 60-column targets drop the sparkline whole though seven bars would fit, so the targets govern: one bar a node, or none; after the bars each time goes whole, from the last; the count is cut only where it alone passes the edge. Seven goldens blessed, each by the meter's row alone: the catalog's five environments, `markdown/catalog.md` and `html/catalog.html`. A new block reaches every surface, so `doc.rs`, `markdown/mod.rs`, `html/blocks.rs` and the catalog's sample changed beyond the list.

**Fails first:** the meter's goldens, written by hand from the meter's row of the approved target `work`, red; a unit test: at 60 cells the row keeps its count whole.

**Acceptance criteria:**
- [x] The meter at 120 columns draws what the meter's row of `work.unicode-120.txt` draws, from the same figures.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco`

**Dependencies:** Task 63

**Files likely touched:** `crates/surco/src/components/meter.rs` (new), `crates/surco/src/surface/terminal/meter.rs` (new), `crates/surco/src/{components/mod.rs,components/catalog/kinds.rs,surface/terminal/mod.rs}`, `crates/surco/goldens/meter/` (new)

**Estimated scope:** M

### Task 65: a node's feed, in render's chronicle

**Description:**
- **`chronicle::in_feed(&Happening) -> bool`**: the moments a node's feed lists, of every attempt: each call of its sessions, and what the engine does there between them (a session opened, a write refused, a file formatted, a check and its result, an artifact handed over, a finding posted, a scope asked and answered); a row names each attempt where it opens.
- **`chronicle::feed(moments, before, after, look) -> Feed`**: a call is its verb in a column as wide as the widest verb in sight, then its object as `target::said` names it; an engine's moment is `chronicle::line`'s words without their subject; the newest row strong. Where the node's tasks work at once, a row names its task after its verb, from the ledger's session task. The feed's heading says the open attempt's calls. Names said free in `crates/`.
- Knowledge: `chronicle/mod.rs`'s `//!` (the feed is one more surface of the same words).

**Found in its turn:** `chronicle::in_feed`, `chronicle::feed` and `chronicle::feed_heading` (`what it is doing · 3 calls`) in `chronicle/feed.rs`; the joining of `say`'s words has one owner, so a feed's row says the chronicle's. `feed` takes the ledger's sessions' tasks too, and a call names its task where calls of more than one stand in sight. The verb's column is eight cells, as the targets and the conventions draw it, not the widest verb in sight; a test with an exhaustive match over the calls' actions holds it. In the feed beyond the task's list: a refused submission and every change of a finding; left out: a node reaching a wait, a task's moves and checks by a session, a refused stop, a run tool's refusals, a service lost, `Degraded`, a scope derived. Row `run.work.T26` is carried by rewording the chronicle's own line once (`criteria_checked`): a red check reads `{task} post: {…} · red · {…}`, the word alone as the proof's pill says it, where it read `· red: {…}`, in the feed and in `yunta status --follow` alike; one test quoted the old words and no golden or doc did. The lead first took the targets' form for a slip and changed eighteen of their rows, then read the conventions' own decision and restored them from the phase's commit: no target moved. The smell counters read a test file under `src` not named `tests.rs` as production code: a trap is recorded.

**Fails first:** `crates/render/tests/feed_words.rs` (new): over the logs in `crates/replay/tests/fixtures/logs/` and the design run's kept JSONL under `crates/testkit-ui/fixtures/designed/`, each read by path as `crates/replay/tests/logs_from_runs.rs` reads its logs (`yunta-render` cannot reach `yunta-testkit-ui`, which would cycle through `yunta-ui`), every moment a feed lists says in its row the words `chronicle::logged` says for it after its time and its subject (the spec's "same words"). Red: there is no feed.

**Acceptance criteria:**
- [x] A row is one row at any width; the newest is strong on the 16-color layer.
- [x] The inventory's rows for the feed's words are carried here.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render --test feed_words`, `cargo test -p yunta-render chronicle`

**Dependencies:** Task 63

**Files likely touched:** `crates/render/src/chronicle/feed.rs` (new), `crates/render/src/chronicle/{mod.rs,tests.rs}`, `crates/render/tests/feed_words.rs` (new)

**Estimated scope:** M

### Task 66: what a node hands over, a task's proof and its scope

**Description:**
- Values in `render::blocks`, each handing its blocks as `FailureDetail` does, in the design's words (the inventory's rows of P1 and P2):
  - **`HandsOver`**: each artifact the node's workflow declares, with its standing: not yet, handed over and accepted, or handed over and unread, with its why (what Phase 1's `Document::Unread` carries), so a document that fails today's shape is said unreadable in its place;
  - **`TaskProof`**: the task's title, from the tasks document, and its attempt; `proved`, red or green, by its latest post-phase check; `by`, its criterion's command; `keeps`, what the run holds every task to;
  - **`TaskScope`**: its scope a path a row, marked where a session of the task wrote it, and the paths granted, `granted by you`.
- Each reads Phase 1's ledger facts. Names said free in `crates/`.
- Knowledge: `blocks/mod.rs`'s `//!` lists them beside a failure's detail.

**Found in its turn:** `HandsOver`, `TaskProof` and `TaskScope` in `render::blocks`, each reading the ledgers and handing its blocks, with nine goldens written by hand first from the targets' rows. A document counts as handed over only from the node's latest start, which gives a node sent back its `not yet`. `proved` falls back to the task's check before the work where no check after it stands yet, as the target `board` shows `red` before any work (`TaskRecord::checked_before`); `by` leaves the guards out and `keeps` names what the baseline holds green (`lint`), as the target says. A document that cannot be read is said with the reader's own lines, its path from the run's folder and its count, then each error, under the artifact's row marked `✗`, as the target `card-unread` says them: nothing in render worded them, so `HandsOver` prints the why it is handed and takes a `Look` to wrap it; the engine's reader of a shown document names a failing one by its view (`artifacts/spec/spec.yaml`) where it named its absolute path, as it names every other artifact in a diagnostic. Proposed, for the person: `granted`, alone, for a path a rule or the evidence of a red criterion granted, where a person's reads `granted by you`. `task_mark` moved into `task_proof.rs`, its one owner.

**Fails first:** `crates/render/src/blocks/tests.rs`, and goldens in `crates/render/goldens/blocks/` written by hand from the targets' rows: a task red, by its command, with two of three paths written; a node that handed one document over accepted and one unread, the unread one with its why. Red: none of them exists.

**Acceptance criteria:**
- [x] Every word they say is render's, from the inventory; no sentence stands in `yunta-ui` for them.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render blocks`

**Dependencies:** Phase 1's ledgers; Checkpoint A (the inventory's rows of P1 and P2)

**Files likely touched:** `crates/render/src/blocks/hands_over.rs` (new), `crates/render/src/blocks/task_proof.rs` (new), `crates/render/src/blocks/task_scope.rs` (new), `crates/render/src/blocks/{mod.rs,tests.rs}`, `crates/render/goldens/blocks/`

**Estimated scope:** M

### Task 67: the node's printed page, composed of the parts

**Description:**
- **`page::Node`** composes, in order: its headline, what it said or what failed it, its failure whole, its fields, what it hands over, for a loop each task's proof and scope, its feed's last twenty rows under their heading with how many stand above, the documents it produced, what to type next. The count of rows is a `const` with its `///` reason: what reads at once under the parts above it.
- **`OpenedRun::node_page`** (`crates/api/src/run/page.rs`) folds the run's events with `chronicled` and keeps the node's moments; the tasks' titles come from the run's tasks document.
- `yunta status <run> --node <id>` prints them. Goldens in `crates/render/goldens/node/` (new): a session at work, a loop with a task red and its scope, a node finished, a node failed, a node waiting; blessed, and their diff reviewed at Checkpoint B.
- The CLI's tests that read a node's page pass, or follow where they read an order the parts move: `gate_review_cmd.rs`, `plan_implies_cmd.rs`, `three_questions.rs`, `status_nodes_cmd.rs`, `command_output.rs`.
- Knowledge: `node.rs`'s `//!` (the node says in print what its pane shows).

**Found in its turn:** `page::Node` composes its parts in the listed order, the feed's last twenty rows under their heading (`FEED_ROWS`), the pane's parts keeping the pane's layout (a heading and its rows at one column) and the page's own facts their indent; `node_page` folds the run once with `chronicled`, frames the state itself, reads the tasks' titles from the latest tasks document, and takes a loop's tasks from every registered task in that document's order, tasks being registered under `plan`. Fifteen new goldens in `crates/render/goldens/node/` (a session at work, a loop, a node finished, failed, waiting, at three environments); no other golden moved and the run's page prints as at the base. A document a node handed over that cannot be read is said in its place on the node's page, which printed nothing and failed whole at the base: the follower's reading is `Document::read`, called by both, with `yunta_engine::produced_shown` and `beside_its_plan` public for it. `yunta status <run>`, the run's own page, still fails whole on such a log. The count of calls is taken from the moments, the ledger clearing it when an attempt ends. One of the engine's tests, `memo_across_wakes`'s wake whose commands run elsewhere, failed once beside another build and passes alone: added to the open work on tests under load.

**Fails first:** `crates/cli/tests/status_node_cmd.rs`: on the design's run cut where the target `board` stands (`yunta_testkit_ui::cut`), the loop node's page prints its task's proof and scope and its feed's last rows. Red: the page prints none of them.

**Acceptance criteria:**
- [x] The run's page prints as at the base: its goldens unchanged.
- [x] `pages.sh`'s captures differ from the base in the node pages alone.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render page::node`, `cargo test -p yunta-api --test run_page`, `cargo test -p yunta --test status_node_cmd`
- [x] `YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine`, then the goldens' diff read.

**Dependencies:** Task 65, Task 66

**Files likely touched:** `crates/render/src/page/node.rs`, `crates/render/src/page/node/tests.rs` (new), `crates/render/goldens/node/` (new), `crates/api/src/run/page.rs`, `crates/api/tests/run_page.rs`, `crates/cli/tests/status_node_cmd.rs`

**Estimated scope:** M

### Task 68: the meter in render's words, on the attached run's region

**Description:**
- **`watched::spent(frame: &RunFrame) -> Meter`**: the meter in render's words, as Surco's `Meter` (Task 64): the run's tokens, one bar a node, its time at work, its time waiting on the person where it is not zero. It draws nothing itself: a surface draws it through `Terminal::meter` with its look, and Phase 3's run screen sets its `Screen.meter` from it.
- **The attached run's region draws the run's gist (`state::gist`, the row a run's screen draws under its ribbon) and then the meter as its last two rows**, the meter through `Terminal::meter` with the region's look, where it drew `watched::footer`, and so does `status --follow` at a terminal. The footer's word on whether the run needs the person stays so, in the gist's words. `watched::footer` stays only for the application's run screen, which Phase 3 replaces.
- The region's tests and the CLI's that read the footer's words read the meter's: `crates/cli/src/surface/region/tests.rs`'s `the_footer_stays_the_last_row_as_nodes_come_and_go` and its quiet run; `crates/cli/tests/run_surface.rs`'s `the_estimate_appears_after_three_runs`. What the footer said and neither the gist nor the meter says (the counter, the usual time) is listed for Checkpoint B, each difference a row of the inventory.
- **Surco's reference** follows this phase's components, in its existing sections, each with its owner in the code: `Feed` (Task 63), the meter row (Task 64) and the screen's title (Task 58).
- Knowledge: `watched/mod.rs`'s `//!` (the meter is named by what it says).

**Found in its turn:** `watched::spent(frame) -> Meter` and `watched::gist(frame) -> Option<Line>` (the words of `state::gist` as they are, in the needs-you tone while the run needs the person, muted otherwise); the attached region ends with the gist and then the meter, drawn through `Terminal::meter`, its last two rows saying what rows 6 and 34 of the target `work` say, from the first column; where a run has no gist the region ends with the meter alone. The footer's counter and its usual time leave the region, and `Rows` keeps no footer, the application's run screen calling `watched::footer` itself until Phase 3. Through a pipe the follower prints as at the base. No golden of the suite moved; tests that waited on the footer's words wait on the meter's, and the estimate of three past runs is read from the closing block. A meter of nothing spent draws the lowest bar a node, as the targets' `0 tokens ▁ · 45s at work`, where Surco drew `░`: the meter asks for the lowest step itself and `bars::sparkline` keeps `░` for a history with nothing to scale against, its one other caller; five goldens `spent-nothing.*` written by hand. `README.md` and render's crate map say what the region ends with. A doc link Task 63 wrote, to a name that is both a function and a macro, failed the strict doc build of `yunta-surco`: corrected.

**Fails first:** `crates/cli/src/surface/region/tests.rs`: a region's last two rows are the run's gist and the meter, its tokens, a bar a node and the time at work. Red: its last row is the footer.

**Acceptance criteria:**
- [x] `status --follow` through a pipe prints as at the base: its captures equal.
- [x] The region's last two rows at a terminal's width say what the gist's and the meter's rows of the target `work` say (its rows 6 and 34), from the same cut.
- [x] Surco's reference names `Feed`, the meter row and the screen's title, each with its owner.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render watched`, `cargo test -p yunta --bin yunta surface::region`, `cargo test -p yunta --test run_surface`, `cargo test -p yunta --test status_follow`

**Dependencies:** Task 64; Task 58 (the screen's title, for Surco's reference)

**Files likely touched:** `crates/render/src/watched/{mod.rs,tests.rs}`, `crates/cli/src/surface/{region.rs,region/tests.rs}`, `crates/cli/tests/run_surface.rs`, and Surco's reference, where Task 160 updates it

**Estimated scope:** M

### Group check: what a node says in print
- [x] The suite's groups green; the goldens that changed are Surco's catalog and every-block, and the new ones of the feed, the meter, render's blocks and the node page; `pages.sh`'s captures differ from the base in the node pages and the attached run's last two rows alone.

**Found in its check:** the gate's short steps pass, the strict doc build among them. 322 binaries and 5,040 tests, none failing: the base's by name but twelve this phase and Phase 1 moved, replaced or renamed, and 187 new. 699 goldens: seventeen changed (Surco's catalog and every-block, eleven, by the feed's and the meter's entries; and the six frames of `tui` that Tasks 51 and 60 moved), none gone, and 452 new (the 408 targets, the node's fifteen, the feed's ten, the meter's ten, the blocks' nine). `prints.sh`'s 159 views differ from the base in three, `node-park` at its three environments, each gaining the node's feed (`what it is doing · 0 calls`, `running: attempt 1`, `failed: exit 1`); the followers are captured through a pipe and equal the base's. Nine more views differed by the installed Claude Code CLI's version alone, which the machine updated between captures (2.1.325 to 2.1.326): `normalize.py` masks it now, with its reason.

### Task 69: the field moves at a pseudo-terminal

**Description:**
- **`crates/cli/tests/application/field.rs`** (new), bare `yunta` open on the field and no key pressed, each run moved by another process:
  - a run started detached appears under what runs;
  - held by its word, then let go to its gate, it moves under needs you, and the tally and the attention row say so;
  - answered with `yunta resolve-gate`, it leaves needs you;
  - stopped with `yunta cancel`, it moves under what is stopped;
  - finished, it moves among the closed runs;
  - its engine killed, it moves under stalled, and the log holds no event past the kill.
- Each assertion is an order: the row stands before the next word is given or the next process runs.

**Found in its turn:** four tests in `crates/cli/tests/application/field.rs`, each step an order, the run held by words on bash nodes as `calling.rs` holds one: a run started detached comes under what runs, parked it moves under needs you with the tally and the attention row, answered it leaves, finished its row goes and the headline counts it closed; cancelled it moves under what is stopped; its engine killed it moves under stalled with the log's head unmoved. Each was red with the performer made to drop the run's news and its joining (`the run started never came under what runs`, and its like), the performer put back byte for byte. Fifty stressed runs and two hundred beside eight busy loops, none failing. The field at a glance lists no closed row and no key may be pressed, so "among the closed" is held as the row leaving the open groups and the headline's count. Seen and left: a run just created is counted as running in the headline and left out of the tally.

**Fails first:** each test seen red with the performer's `Input::FollowNews` dropped for the run: the field never moves.

**Acceptance criteria:**
- [x] The application never hosts a run: each run's engine is another process's (`engine_of`).
- [x] Criterion 1 holds.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test application field`
- [x] `python3 .plans/yunta-tui/tools/stress_binary.py` on `field`, no failure.

**Dependencies:** Group check: gone

**Files likely touched:** `crates/cli/tests/application.rs`, `crates/cli/tests/application/field.rs` (new)

**Estimated scope:** S

### Task 70: a run's screen moves at a pseudo-terminal

**Description:**
- **`crates/cli/tests/application/watching.rs`**, the run's screen open and no key pressed:
  - a run parked on its gate, answered from a shell with `yunta resolve-gate`, goes on: its headline leaves needs you and the next node works;
  - a run stopped, then resumed from a shell with `yunta resume`, is read on to its end;
  - a node picked, the run's stats opened and closed while the run moves: the rows stand where the run went meanwhile.
- Each assertion is an order, the run held by its word between two of them. This task runs beside Task 69: disjoint files.

**Found in its turn:** three tests in `crates/cli/tests/application/watching.rs`: a gate answered from a shell with `yunta resolve-gate` leaves needs you and is read to its end; a run cancelled then resumed from a shell reads stopped, running, finished; with a node picked and the stats opened and closed while the run moves, the rows stand where the run went (`second running for`, `1 of 2 nodes done`, then `2 of 2`). Each was red with the performer made to drop the run's news, where the three tests already there went red too. Fifty stressed runs and two hundred beside eight busy loops, none failing. No engine or held node was left.

**Fails first:** each seen red with the performer's `Input::FollowNews` dropped for the run: the screen stands where it was opened.

**Acceptance criteria:**
- [x] Criterion 2 holds as far as the run's screen shows it today, and criterion 4 holds at a terminal as the walk holds it.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test application watching`
- [x] `python3 .plans/yunta-tui/tools/stress_binary.py` on `watching`, no failure.

**Dependencies:** Group check: gone

**Files likely touched:** `crates/cli/tests/application/watching.rs`

**Estimated scope:** S

### Task 71: what the phase weighs

**Description:**
- **The processor at rest**: `at_rest.py` (Phase 1's), the release build open for a minute on this machine's field with nothing moving, the median of three, against the base's; and the same at rest on a run's screen.
- **Real runs**: three fragua runs, as Phase 1's base ran three, with the application open on the field beside each (`follow_in_app.py` made to stand on the field), their time between nodes from `yunta stats`, against the base's three.
- **The walk**: its states, palettes, helps and time at the phase's end, against `walk-fed.txt`'s and the base's.
- **The release size** by crate, against the base's.
- All of it in `.plans/yunta-tui/proof/live-screens/checkpoint-b.txt`.

**Found in its turn:** in `proof/live-screens/checkpoint-b.txt`. The release, `yunta-b`, weighs 31,211,712 bytes, 611,776 over the base's (2.0%) and 165,360 over Checkpoint A's, `yunta_ui` growing most since. Three real runs of fragua with the application open on the field beside each, every one through to `ship`, answered `abort` and closed: `01M7K5SS21ATAGFKDDBCWQ60CN` (127 s, between 0.540 s, 1.29M tokens), `01M7K5YQ9MCS7CFPQPRKTT94Y5` (102 s, 0.537 s, 1.20M), `01M7K62EGEJSMYKME8J16ZA4HY` (168 s, 0.529 s, 1.04M). The third stands 7.5 ms under the base's lowest (0.536 to 0.555): the fastest of the nine runs measured, its difference in its first step and in the stretch after the abort, not a slowing. With no key pressed, each run's row stood under what runs with its count of nodes climbing, moved under needs you at `ship` with the tally and the row above the keys naming it, and left the open groups once closed (`real-b-<n>/field-moves.txt`, `app-at-ship.txt`, `app-closed.txt`). The processor at rest takes 0.403% of a core on the field against the base's 0.100% and Checkpoint A's 0.104%, and 0.496% on a run's screen against the base's 0.155% measured the same way: the application looks at the log ten times a second (`Following::LOOKS_EVERY`), 20.2 wakeups a second against 2.2, the looks and `PRAGMA data_version` being what a sample shows busy; under the bound of 2%, said for the user. The walk: 63,842 states, 41 palettes, 72 helps in 87.5 s, and 6,683 screens at each size in 84.6 s, against the base's 22,388 in 19.3 s and 22.0 s. `at_rest.py` takes where it rests, `follow_in_app.py` stands on the field, and `wakeups.py` and `stretches.py` are new, all outside git. No process was left.

**Fails first:** none: measurements.

**Acceptance criteria:**
- [x] Time between nodes inside the base's spread; a processor at rest over 2% of one core is reported with its cause.

**Verification:**
- [x] Manual check: `checkpoint-b.txt` read whole.

**Dependencies:** Task 69, Task 70, Group check: what a node says in print

**Files likely touched:** `.plans/yunta-tui/proof/live-screens/checkpoint-b.txt` and its tools (outside git)

**Estimated scope:** S

### Group check: at a pseudo-terminal, and measured
- [x] The suite's groups green; each new pty test green under `stress_binary.py`; `checkpoint-b.txt` written.

**Found in its check:** run as the whole gate on the phase's tree, every step of the `ci` job passing: 322 results and 5,050 tests in its test step, none failing or aborted. The new tests at a pseudo-terminal passed fifty stressed runs and two hundred beside busy loops each (Tasks 59, 69, 70). `checkpoint-b.txt` is written (Task 71).

### Checkpoint B: the application follows
- [x] The user opens the application on their own field and watches it move with no key: a run started, answered and stopped from another terminal; a run's screen through a picked node, its stats and back.
- [x] `tui`'s fifteen frames equal the base's but the two `help.*` rows of `f`; criteria 1 and 4 hold, and 2 as far as today's screen shows it; the walk is bounded, its states and time beside the base's; runs keep their time between nodes with the application open; the processor at rest and the release size stand beside the base's.
- [x] The goldens' diff is reviewed and what they now print approved: the two `help.*` frames without the run's `f`, the only base frames that change before their target; the node's printed page; the attached run's meter with what left its last row; Surco's catalog with the feed and the meter.
- [x] The phase commits, with `crates/ui/`, `crates/testkit-ui/`, `crates/testkit/src/events.rs`, `crates/tui/`, `crates/cli/`, `crates/render/`, `crates/surco/`, `crates/api/`, `docs/guide.md`, each file staged by name, after every step of the `ci` job with `CARGO_INCREMENTAL=0`; then `cargo clean`.

**At the checkpoint (2026-11-16):** the user was given the phase's binary to open on their own field and a page with every frame, print and golden that moved beside the base's, the choices taken and the measures, and approved with no note. Beyond the two `help.*` frames the list foresaw, four frames moved in the ages their rows say (Task 51), and the CLI changed where the list's later rulings say: a red check's words in the chronicle, a promoted run followed to its successor, an unreadable document said in its place on a node's page and named by its path in the run.


## Phase 3: a run's screen

Its groups, each green before the next: Surco's pieces of a run's screen; the components the design redraws; render's phrases of a run's screen; the scene and the lead; a run's frame and its keys; what a node does; where a run stands; over a run; decisions in place; the proof.

**Decided in this list, beyond the plan:**
- **A run's frame and its keys land before its scenes** (Task 92, Task 93, Task 94), not after the decisions as the plan's bullets list them: every target carries the tally, the attention row and the legend, and a legend never names a key before the key does what it says. The keys of the findings, a card, a round and the desk land with them (Task 100, Task 113 to Task 118).
- **Each scene lands with its goldens at four sizes, in its own task**, as the plan's "the scenes land family by family, each with its goldens"; Task 123 holds the whole set. A scene's task moves (`git mv`) its approved targets from `crates/tui/goldens/targets/` into `crates/tui/goldens/screens/`, unchanged, under Phase 1's names (`<screen>.<environment>.txt`, the environments `unicode-120` (120×36), `unicode-80` (80×24), `ascii-120` (120×36) and `unicode-60` (60×14)), and builds each state from that target's cut in `yunta_testkit_ui::cut::TARGET_CUTS`. A state between two targets (findings posted one by one) is asserted in `crates/ui/tests/scenes.rs`, never a golden of its own.
- **`Scene` is wired into the run's stage when it lands** (Task 91), each arm drawing what Phase 2 drew until its family's task replaces it; Task 119 deletes the page once no arm draws it. A `Scene` nobody reads would not pass `clippy -D warnings`.
- **The base's `tui` frames go in the task that first changes what they draw, with their builders in `crates/testkit-ui/src/screens.rs`**, never edited: `run`, `palette` and `help` in Task 76 (the side region's sizes change the run under them at 80 columns), `card` in Task 80, `desk` in Task 82, `confirmation` in Task 84; a frame whose cut split a number would go in Task 73, and none does at the base. Until its target lands, a screen is held by the walk's layout checks. No golden passes through a frame nobody approved.
- **The box is one component, redrawn once** (Task 84): its edges, title and measure change for the palette, help and a confirmation alike, so `confirmation.*` goes here and the confirmations' targets (`close-run`, `field-close`) land in Phase 4, which redraws what a confirmation holds (the plan's "a confirmation's box").
- **A line asked of a person is drawn in the design's box wherever it is asked** (Task 79): on a card, at the desk's foot, in a round's typed answer and in a `TypedAnswer`, since the box belongs to the row a person types on (`TypedRow`); Phase 4 gives `TypedAnswer` only its placeholder and its hint. A list narrowed by typing is not a line asked: it keeps the design's row, `type to filter` after the pointer (Task 81).
- **The field in perspective enters Surco here** (Task 78), not in Phase 4 as the plan's section 6 lists it: the receipt (spec element P9, "the harvested field under it", `design/receipt.wide.txt` rows 17-33) is its first user, and Task 108 lays out a region that ends with it. Phase 4 reuses both. The name in block letters stays Phase 4's.
- **The six behaviours "a golden cannot hold" are tested red first in the task that makes each** (Task 103 no spinner, Task 105 the forge read once, Task 106 `esc` on a child, Task 107 no key answers a run asked elsewhere, Task 109 a successor only while the run leads); the sixth, a repository with no run opening on the first request, is held on followed news by Phase 2's test of where the application first stands. Task 120 holds what holds and only evidence moving.
- **One composer of the tally and the attention row**, `FollowedRuns::framing` (Task 92), which every stage that draws the design's frame calls, Phase 4's among them; where a line is typed, `a` types, so the attention row names no key (`Answer::Unkeyed`, Task 73).
- **For the user's approval: a number picks on a card in the shared `DecisionCard`, so the CLI's card answers numbers too** (Task 113). There is one prompt model, so a prompt answers alike on every interface: a key is added and none changed, where the spec's out-of-scope line keeps the CLI's inline prompts' keys. **The arrows over a card** are taken by the run's stage for what is read above it before the card sees them; the card keeps its arrows for the CLI, whose keys stay.
- **For the user's approval: an option's words are typed at the desk's foot in the CLI's desk too** (Task 118), where the spec's out-of-scope line keeps the CLI's inline prompts' flow. `PlanDesk` is the one prompt model of a desk, and the spec puts the line at the desk's foot; the keys are the CLI's (`enter` sends, `esc` returns to the options), and the guide's sentence "goes back to the card with its field open" changes with it.
- **The pull request's link is shown through the request a page is shown by** (Task 104): `Want::Show`, `Request::ShowPage` and `Input::Unshown` take `Address` (new), `Page(PathBuf)` or `Link(String)`, since a URL held as a `PathBuf` is a type that admits a lie. The forge is read by `Operation::ReadPullRequest` through `OpenedRun::pull_request` (new in `yunta-api`), which polls the gate's pull request with the run's forge (`Forge::poll`, `crates/core/src/port/forge.rs:207`).
- **The receipt's scene writes nothing**: it reads the receipt Phase 1 derives from the ledgers of the run in view, and verifies the event chain once with `Operation::Evidence(Verify)`; `WriteReceipt`, which writes `receipt.md`, is not asked.
- **Criterion 12 holds at Checkpoint A; after it, the receipt differs from the base only by the design's phrases, each a row of the inventory.** The rows the inventory rewords to the design's (`1 of 1 green, red before the work`, `intact`) are the same facts `yunta receipt`, `receipt.md` and the pull request's body print (`crates/engine/goldens/receipt/finished.md` holds `each command below` and `hash-linked`), so they change here, each keeping after the design's words, past the separator, what the inventory leaves to the printed page (`intact · 342 events, hash-linked, replayable`), blessed in Task 88 and approved at Checkpoint C.
- **`NextStep::Follow` opens the run's whole chronicle** (Task 110), which is what `yunta status <run> --follow` prints; `State::opened_on` opens a run under a step by `NextStep::Status`.
- **The side regions' widths**: a side at the left stands 31 cells, one at the right 39 (the field's glance, as its target draws it: its edge in column 79, measured in Task 76), each with a gutter of 3, from 102 columns. Phase 4 gives a new run's facts their own width where its target asks.
- **A run's numbers and the graph's headline are reworded with their drawing** (Task 85), in the files that redraw touches, rather than with render's other phrases.
- **A tally counted with nothing in it says `nothing needs you`** (Task 75): `Screen.tally` becomes `Tally` (new in Surco), `Uncounted` or `Counted(Vec<Count>)`, so a screen that counts nothing is told apart from one whose runs need nothing.
- **Names added**, each grepped free in `crates/`: Surco's `Noted`, `Tally`, `SideAt`, `Line::pointed`, `Horizon`, `FindingList`, `FindingRow`, `Answer::Unkeyed`, `Screen.{noted,anywhere,meter}`, `Region.edge`; render's `blocks::{NodeTitle,FindingsAt,WhereItStopped,FenceRefused}` and `watched::{stretch_names,needing}`; `yunta-ui`'s `InPlace`, `ChronicleStage`, `Address`, `Standpoint`, `FollowedRuns::framing`, `Operation::ReadPullRequest`, `Outcome::PullRequest`; `yunta-api`'s `OpenedRun::pull_request` and `PullRequestPolled`.

**Needs from other phases:**
- Phase 1: the design's run kept under `crates/testkit-ui/fixtures/designed/`, and `yunta_testkit_ui::small_runs`, built in-process: a cap, a loop cap, a departure, a run that wakes alone, a pull request, a child, no room, orphans, a stop, a stall, a run asked elsewhere, a failure, a promotion, a broken log, a gate answered at the console, a pause whose escalation carries no menu, a run created, a loop with two tasks at work at once (their calls naming their sessions), a run in a mode that leaves a node out, a plan gate answered `adjust` (which sends `plan` back), a round whose first answers are refused (`AnswersRefused`) and then answered, and a gate whose shown document fails today's shape.
- Phase 1: `yunta_testkit_ui::cut::TARGET_CUTS`, the committed table of each target's cut (by the target's name: its runs and seqs, registry, node pointed at, keys pressed), with `Cut::of_target`, which loads one run or several at their moments into a fresh store; a cut for every target this list lands, `node-needs-you`'s with its node pointed by `j` then `k`, which Task 101 reaches before `esc` puts a prompt aside in Task 117.
- Phase 1: every target of a run's screens approved, at the four sizes, in `crates/tui/goldens/targets/` under the names this list lands (Task 35 to Task 38, with `work-moved`, `board-moved`, `failure-recorded`, `scope-recorded`, `round-aside`, `desk-proof`, `desk-decided`, `desk-change`, `card-unread`, `receipt-palette` and `palette-aside`); the inventory, with its `element` and `survey` columns; `targets_check.py`, which takes the directory it checks; the base's goldens, captures and walk counts; the probe (`frame`, `whole`) in `proof/live-screens/probe/`; `MockStep::AwaitsWord` and `Hold`'s named words; `Awaited` keyed by its pause, `RunNews::AwaitedUnread { why }` and `RunNews::Remark`; the ledgers' facts (a task's criteria and their results, its scope, the paths its sessions wrote, a session's task); the receipt derived from the ledgers; `RunState::chronicled`.
- Phase 2: `FollowedRuns` (`crates/ui/src/followed_runs.rs`) and `RunInView` (moments by node, documents handed over, what sessions said, `awaited`, the why of a pause whose ask could not be read, a remark not yet said) read by every stage; `State::followed()` naming the run under every stage over it; the followed field under every stage, which the tally counts; the run's `f` and `follows` gone; `Outcome::Followed` gone; a promoted run's successor as the run in view; the project's prices for the numbers; the walk and `drive` fed what is followed; the performer's channel from its surface; a repository with no run opening on the first request, held on followed news.
- Phase 2: render's parts of a node's pane in `render::blocks` (what it hands over, a task's proof, its scope) and a feed's rows in `render::chronicle`, in the design's words; Surco's `Feed` block and `Meter`, with `watched::spent(frame: &RunFrame) -> Meter`; `page::Node` composed of them; `watched::footer` kept only for the application's run screen, which Task 95 deletes; Surco's reference updated for Phase 2's pieces.
- Phase 4: the field's `Screen.noted` composed from the field's `said`, where Task 94 hands the note a run's `q` and `esc` leave; `render::screen::over_every_stage` deleted once every stage composes `Screen.anywhere`; the confirmations' targets (`close-run`, `field-close`) in place of the deleted `confirmation.*`; the first request and the setup drawing `Horizon` through Task 108's layout, and `TypedAnswer`'s placeholder and hint over Task 79's box; `n` opening the new run's screen (here it opens the start stage as it stands).

### Task 72: the note, in the attention row's place

**Description:**
- **`Noted`** (new), in `crates/surco/src/components/screen.rs`: what an act came to, said once until the next key: a mark and a sentence (`mark: Mark`, `said: String`). Not `Note`, a muted paragraph inside a document. `Screen.noted: Option<Noted>`, with the builder `Screen::noted`.
- **`Terminal::noted(&Noted) -> Line`**, in `surface/terminal/screen.rs`: the mark in its tone, the sentence plain, untinted, cut at the row's end with the ellipsis.
- Surco's catalog (`components/catalog/on_screen.rs`) shows the note beside the attention row. No interface draws it yet: the run's frame does (Task 92).
- Knowledge: `Screen`'s `///` on `noted` says it stands in the attention row's place until the next key, and news leave it.

**Found in its turn:** `Noted { mark, said }`, `Screen.noted` and `Terminal::noted`: the mark in its tone and the sentence plain, cut with the ellipsis, in the attention row's place. The catalog is a document, so the note is also a block, `Block::Noted`, which every surface says; Markdown's and HTML's screens say `Screen.noted` where they say the attention row (HTML as a status paragraph). Goldens written by hand first: `screen/noted.*` at three environments from the targets' rows, `markdown/screen-noted.md`, `html/screen-noted.html`; the catalog's seven gain the note's row and nothing else.

**Fails first:** `crates/surco/src/surface/terminal/screen/tests.rs`: a note at 60 cells is one row, its done mark first and its sentence cut with the ellipsis; it does not compile before `Noted` exists.

**Acceptance criteria:**
- [x] Goldens of the note at 60 and 80 cells, ASCII and Unicode, plain and 16-color, in `crates/surco/goldens/screen/`, reviewed in the diff.
- [x] Every base golden is unchanged but the catalog's, which gains the note.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco screen`, `cargo test -p yunta-surco catalog`

**Dependencies:** None. Runs beside Task 77 and Task 78, which touch other files.

**Files likely touched:** `crates/surco/src/components/screen.rs`, `crates/surco/src/surface/terminal/{screen.rs,screen/tests.rs}`, `crates/surco/src/components/catalog/on_screen.rs`, `crates/surco/goldens/screen/`, `crates/surco/goldens/components/`

**Estimated scope:** S

### Task 73: the attention row's cases, and a cut that never splits a number

**Description:**
- **The attention row's cases**, as `Attention::lines` (`crates/surco/src/surface/terminal/attention.rs`) draws them on a screen: a run's own decision with no subject (`◆ needs you: <what it asks> · a go there`), another run's with its handle (`◆ Q78GRR needs you: …`), each keeping its key whole at the row's end. `Answer::Elsewhere` stays for the CLI; a screen never names a run asked at its own terminal, which Task 92 holds.
- **The attention row with no key**: `Answer::Unkeyed` (new, `crates/surco/src/components/attention.rs`), with `Attention::unkeyed`: what needs the person, said where a line is typed and `a` types, so no key answers it there; drawn as the mark, the subject and what it asks, cut at the row's end. Not `Elsewhere`, answered at another terminal. Markdown and HTML say it as they say `Elsewhere` (every match on `Answer`, the compiler names them).
- **A cut never splits a number**, the spec's "Sizes": `width::truncate` (`crates/surco/src/width/mod.rs:278`) backs a cut that would land inside a run of digits off to before it, so `exits 101` cut short reads `exits …`, never `exits 1…` (the defect of `design/field.narrow.txt` row 23). Every cut in Surco goes through it.
- **A `tui` frame among the goldens whose cut split a number is deleted here**, with its builder in `crates/testkit-ui/src/screens.rs`, never blessed. None does at the base: no digit stands before an ellipsis in `crates/tui/goldens/screens/`.
- Knowledge: `truncate`'s `///` says why a number is kept whole; `Answer::Unkeyed`'s `///` says why no key stands there; the catalog's attention rows show the three cases.

**Found in its turn:** `Answer::Unkeyed` and `Attention::unkeyed`, which Markdown and HTML say as they say `Elsewhere`; the catalog's attention rows show three cases. The rule on cuts is owned once, in `width::truncate`, which the feed and the attention row call: a cut never falls inside a word that holds a digit (a count, a date, a time, a handle) and never right after a digit; it falls before that word, which goes whole with what is left out, as the conventions say. A run of digits was not enough: a date read `until 2026-01-...`. The targets and the conventions set two spaces before the separator of `a go there`, where the task wrote one: the targets govern. Goldens that moved: `screen/rows.*` and the catalog's seven by the separator and the new rows; `catalog.unicode-60` from `4 tasks in 3…` to `4 tasks in…`; `every-block.unicode-color-100` from `reported at src/lib.rs…` to `reported at…` (the word is `src/lib.rs:3:5`); and render's `status/waiting.{dig,full}.ascii-80`, a waiting row of `yunta status` that read `hit a limit until 2026-01-01...` and reads `hit a limit until...`, for the person to see at Checkpoint C. `crates/tui/goldens/screens/` did not move; the feed's goldens stand. The targets name a run's own decision in the attention row by its handle, which Task 92 composes. `middle_cut`, `render/src/published.rs` and `adapters/src/forge/mod.rs` keep cuts of their own. The catalog's `KINDS` still leaves out `rows`.

**Fails first:** `crates/surco/src/width/tests.rs`: `truncate("exits 101 now", 9, …)` keeps no digit of `101`, red at the base, which cuts to `exits 1…`; `crates/surco/src/surface/terminal/attention/tests.rs`: the design's attention row at 80 cells keeps `· a go there` and cuts what it asks before `101`; an unkeyed attention with a handle names no key and keeps its handle at 60 cells, which does not compile at the base.

**Acceptance criteria:**
- [x] Goldens of the three cases at 60 and 80 cells, ASCII and Unicode, plain and 16-color, and on Markdown and HTML, reviewed in the diff.
- [x] Every base golden whose cut split a number changes there and nowhere else, each named in the diff's review; every other golden is unchanged, and `crates/tui/goldens/screens/` with it.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco -- width attention catalog`, `cargo test -p yunta-tui --test screens`
- [x] `YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine`, then the goldens' diff reviewed.

**Dependencies:** None. Runs beside Task 77 and Task 78; after Task 72 where both touch the catalog (`components/catalog/on_screen.rs`).

**Files likely touched:** `crates/surco/src/width/{mod.rs,tests.rs}`, `crates/surco/src/components/{attention.rs,catalog/on_screen.rs}`, `crates/surco/src/surface/terminal/{attention.rs,attention/tests.rs}`, `crates/surco/src/surface/{markdown/screen.rs,html/screen.rs}`, `crates/surco/goldens/{screen,markdown,html,components}/` (every match on `Answer`, which the compiler names, moves with the new arm)

**Estimated scope:** M

### Task 74: the legend's far keys, and a region title's far words

**Description:**
- **`Screen.anywhere: Legend`** (new): the keys a person presses anywhere they are not typing, the palette and help. Not `legend`, the keys of what is on this screen. `Terminal::legend(&Legend, &Legend) -> Line` draws them at the row's far end from a row of 98 cells (a terminal of 100 columns inside its margins, the design's threshold; a numeric `const` with its `///` reason), and below that after the screen's keys, where the overflow rule keeps the last one, `? keys`, as the base does (`surface/terminal/legend.rs:43-77`). An empty `anywhere` draws nothing there, as on a screen where a line is typed.
- **`Region.edge: Option<String>`** (new): the words at the far end of a region's title row: who works a node, its task, its check, its assignee. `Region.title` becomes its parts, drawn as the desk's title is: the first strong, the rest muted after the separator. `Terminal::title` draws the parts and the edge, cutting the title first and keeping the edge whole, as the trail keeps its tally.
- `yunta-tui`'s `drawn.rs`, the caller of `Terminal::legend` and `Terminal::title`, follows the new forms, drawing what it drew (every caller, the compiler names them).
- Knowledge: `Screen`'s `//!` gains the far keys; the catalog shows a legend with its far keys and a titled region with its edge.

**Found in its turn:** `Screen.anywhere`, `Terminal::legend(&Legend, &Legend)` with `FAR_KEYS_FROM = 98`, `Region.title` as parts with `Region.edge`, and `Terminal::title`; Markdown says the edge in the heading and the far keys after the screen's, HTML in spans of their own. The targets govern over the task's text: under 98 cells only `? keys` follows the screen's keys, `/ anything` left out (`work.unicode-80`); from 98, screen keys that do not fit beside the far pair end in ` · …` and drop their last key (`desk.ascii-120`); the edge is drawn muted, as the desk's, a card's and a pane's are. The desk's `title_parts` moved into `terminal/screen.rs`, one owner for the desk's title and a region's. Over a box no far keys stand. Goldens written by hand: `screen/legend.*`, `screen/title.*`, `markdown/screen-far.md`, `html/screen-far.html`; no existing golden moved. The lead's ruling on the task's catalog line: the legend and a region's title are parts of a screen, held by the screen's goldens, and take no block of their own in the catalog, whose goldens the task keeps unchanged.

**Fails first:** `crates/surco/src/surface/terminal/legend/tests.rs`: at 120 cells `/ anything · ? keys` ends the row; at 80 the far pair follows the keys and the overflow keeps `? keys` last. `crates/surco/src/surface/terminal/screen/tests.rs`: a title with an edge at 60 cells cuts the title and keeps the edge. Neither compiles at the base.

**Acceptance criteria:**
- [x] Goldens of a legend with its far keys at 60, 80 and 120 cells, and of a titled region with its edge, ASCII and Unicode, plain and 16-color, reviewed in the diff.
- [x] `crates/tui/goldens/screens/` and every other base golden are unchanged: no stage composes `anywhere` or an edge yet.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco -- legend screen`, `cargo test -p yunta-tui --test screens --test walk`

**Dependencies:** Task 72 (both touch `components/screen.rs`)

**Files likely touched:** `crates/surco/src/components/screen.rs`, `crates/surco/src/surface/terminal/{legend.rs,legend/tests.rs,screen.rs,screen/tests.rs}`, `crates/tui/src/drawn.rs`, `crates/surco/goldens/screen/` (every caller of the changed signatures)

**Estimated scope:** M

### Task 75: a tally with nothing in it

**Description:**
- **`Tally`** (new, `crates/surco/src/components/screen.rs`), `Uncounted` or `Counted(Vec<Count>)`, in place of `Screen.tally: Vec<Count>`: a screen that counted its runs and found none needing a look says `nothing needs you` (Surco's word, `words::NOTHING_NEEDS_YOU`) at the trail's far end; one that counts nothing says nothing there. Not `Count`, how many runs stand at one word.
- `Terminal::trail` (`surface/terminal/screen.rs:17`) takes `&Tally`; `yunta-tui`'s `drawn.rs` and the field's stage, the one stage that sets a tally (`crates/ui/src/stage/field.rs:119`), follow (every caller, the compiler names them). The field sets `Counted`: a field whose runs need nothing says `nothing needs you`, as the design's field does; every base frame counts a run on its gate, so none changes.
- Knowledge: `Screen`'s `//!` gains the tally's two states; the catalog shows a counted empty tally.

**Found in its turn:** `Tally::{Uncounted, Counted}` in place of the list, `Terminal::trail(&[String], &Tally)`: a tally counted with nothing in it says `nothing needs you`, muted, and Markdown and HTML say it in their place line; the field sets `Counted` and stays `Uncounted` before its first reading. The field still counts the stopped and the running, Phase 4's to change, so a field of running runs alone reads `● n running`. Goldens written by hand: `screen/tally.*`, `markdown/screen-nothing-needs-you.md`, `html/screen-nothing-needs-you.html`; no existing golden moved. As for Task 74, an empty tally is a part of a screen and takes no block in the catalog. The walk's counts stand.

**Fails first:** `crates/surco/src/surface/terminal/screen/tests.rs`: a counted empty tally says `nothing needs you` at the trail's far end, and an uncounted one says nothing; it does not compile at the base, where a tally is a list.

**Acceptance criteria:**
- [x] Goldens of the trail with each tally at 60 and 80 cells, ASCII and Unicode, plain and 16-color, reviewed in the diff.
- [x] `crates/tui/goldens/screens/` and every other base golden are unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco screen`, `cargo test -p yunta-ui --test field`, `cargo test -p yunta-tui --test screens --test walk`

**Dependencies:** Task 74 (both touch `components/screen.rs`, `surface/terminal/screen.rs` and `drawn.rs`)

**Files likely touched:** `crates/surco/src/components/screen.rs`, `crates/surco/src/surface/terminal/{screen.rs,screen/tests.rs}`, `crates/tui/src/drawn.rs`, `crates/ui/src/stage/field.rs`, `crates/surco/goldens/screen/`

**Estimated scope:** S

### Task 76: a side region at either edge, giving way below 102 columns

**Description:**
- **`SideAt`** (new), `Left` or `Right`, on `Side` (`crates/surco/src/components/screen.rs:65-70`): which edge of the screen the region beside the main one stands at. `Screen::side` takes it.
- **`yunta-tui`'s layout** (`crates/tui/src/layout.rs`): a side region stands from a terminal of 102 columns (a numeric `const` with its reason: the design's screen of 100 cells inside its margins), at the left 31 cells and at the right 40, each with the gutter of 3 the edge draws; below 102 the main region takes the width. `SIDE`, `LEAST_MAIN` and their test copies in `crates/tui/tests/walk.rs` give way to the new rule. Wider than 120 the side keeps its width and the main region takes the rest.
- The run's stage passes `SideAt::Left` for its nodes.
- The base's frames `run`, `palette` and `help` go from `crates/tui/goldens/screens/` with their entries in `crates/tui/tests/screens.rs` and their builders (`run`, `palette`, `help`) in `crates/testkit-ui/src/screens.rs`, whose `//!` follows: their run loses its nodes at 80 columns, and their targets land with Task 98 and Task 112.
- Knowledge: `layout.rs`'s `//!` says the side's two edges, its widths and where it gives way.

**Found in its turn:** `SideAt::{Left, Right}` on `Side` and `Screen::side(region, at)`; the layout's `SIDE_FROM = 102`, `SIDE_AT_LEFT = 31` and `SIDE_AT_RIGHT = 39`, the side standing at a fixed width from 102 columns and the main region taking every cell below. The right side is 39 cells, not the 40 the list gave: on `field.unicode-120` the edge stands in column 79 and the glance in columns 81 to 119, and the targets govern. Gone, with their builders and their assertions: `run`, `palette` and `help` at `ascii-80` and `unicode-60` (six files), held by the walk's layout checks until their targets land (Task 98, Task 112); `field`, `card`, `desk`, `confirmation` and `room-needed` pass unchanged. The walk draws at 80 and 60 columns, both under 102, so it never lays out a side: only the layout's tests prove one at either edge until the walk draws the design's own size (written into Task 122). Markdown and HTML say the side region first at either edge. `crates/ui/tests/run_stage.rs` keeps a copy of the old side's width, 22, in a helper its test no longer depends on. The walk's counts stand.

**Fails first:** `crates/tui/src/layout/tests.rs`: a side at the right stands at the right edge; a side stands at 102 columns and gives way at 101; at 140 the side keeps 31 cells. Each is red at the base, where the side is 22 cells at the left and stands from 65.

**Acceptance criteria:**
- [x] The walk lays every state out with the side's edge whole on every row it spans, at both its sizes.
- [x] `field`, `card`, `desk`, `confirmation` and `room-needed` frames are unchanged; `run`, `palette` and `help` are gone with their builders, each named for the task that lands its target.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --lib layout`, `cargo test -p yunta-tui --test screens --test walk`, `cargo test -p yunta-surco screen`, `cargo test -p yunta-testkit-ui`

**Dependencies:** Task 75 (both touch `components/screen.rs` and `drawn.rs`)

**Files likely touched:** `crates/surco/src/components/screen.rs`, `crates/tui/src/{layout.rs,layout/tests.rs,drawn.rs}`, `crates/tui/tests/{screens.rs,walk.rs}`, `crates/tui/goldens/screens/`, `crates/ui/src/stage/run.rs`, `crates/testkit-ui/src/screens.rs` (every caller of `Screen::side` and of the deleted builders, the compiler names them)

**Estimated scope:** M

### Task 77: the selection fill of a pointed row

**Description:**
- **`Line::pointed(on: bool, glyphs) -> Line`** (new, `crates/surco/src/line.rs`): one owner for how a row a pointer moves along stands apart: led by the pointer in the needs-you tone, or by the cells it takes when not pointed at, and, when pointed at, filled with the selection to its region's edge (`Fill::Selection`, which a terminal tints to its area's edge).
- A card's choices (`surface/terminal/card.rs:304-337`) and the rail's stops (`surface/terminal/desk/rail.rs:173-190`), which each draw it their own way today, draw through it. The nodes list (Task 95), the findings (Task 83) and the field's runs (Phase 4) draw through it too.
- Knowledge: `Line::pointed`'s `///`; the catalog's choice list shows a pointed row filled.

**Found in its turn:** `Line::pointed(on, glyphs)`, the opening of a row a pointer moves along, drawn once for a card's choices and the rail's stops; a wrapped line of a pointed option takes its opening's fill, and the rail's own `POINTER` is gone. Every golden of a card, a choice list and a desk holds byte for byte on every layer. One golden added, `components/pointed.unicode-color-100.txt`: the 16-color layer paints no fill, so it shows the pointer and the strong name, the fill being held by the unit test and the truecolor goldens already there.

**Fails first:** `crates/surco/src/line/tests.rs`: a pointed row leads with the pointer and is filled with the selection; one not pointed at keeps the pointer's cells and no fill.

**Acceptance criteria:**
- [x] Every base golden of a card, a choice list and a desk is unchanged on every layer: the two drawings it replaces drew the same.
- [x] A golden of a pointed row and its neighbours on the 16-color layer is reviewed in the diff.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco -- line card desk`

**Dependencies:** None. Runs beside Task 72, Task 73 and Task 78.

**Files likely touched:** `crates/surco/src/{line.rs,line/tests.rs}`, `crates/surco/src/surface/terminal/{card.rs,desk/rail.rs}`, `crates/surco/goldens/components/`

**Estimated scope:** S

### Task 78: the field in perspective

**Description:**
- **`Horizon`** (new, `crates/surco/src/components/horizon.rs`): furrows running to a horizon, drawn in braille cells, in a tone the caller names (`Tone::Done` under a receipt, the harvested field). It says nothing: a layer without braille, ASCII, draws nothing in its place, and a surface that draws no picture (Markdown, HTML) leaves it out, losing no fact. Not `Furrow`, a run's nodes as strokes.
- `Block::Horizon` (new), and `Terminal`'s drawing of it (`surface/terminal/horizon.rs`, new) at a region's width and the rows it is given, as `design/receipt.wide.txt` rows 17-33 and `design/empty.wide.txt` draw it; `surface/markdown` and `surface/html` leave it out.
- The catalog shows it.
- Knowledge: the component's `//!` says it is drawing alone and why ASCII leaves it out.

**Found in its turn:** `Horizon { tone }`, `Block::Horizon` and `Terminal::horizon(&Horizon, rows)`, a port of the spike's canvas (nineteen furrows, the same clipping and the same dots), drawing nothing in ASCII or under three rows, left out of Markdown and HTML; `Glyphs::dots` owns the braille cells. It equals the targets cell for cell at each region they give it: `receipt` at 84×16 and 78×5, `reported` at 84×12, `first-request` at 118×20 and 78×10, `setup` at 118×8, `setup-unreachable` at 118×11, `setup-changing` at 118×4, and every braille cell outside the box of `receipt-palette`. The task asks goldens at 60 and 80 cells and no target draws the field at 60 columns, so the goldens stand at the targets' regions, written by hand from their rows; the catalog draws it at 60 and 100. The least it needs is three rows, as the conventions say; a document, which gives no rows to fill, draws it at those three. Four catalog goldens gain its three rows and a blank one; the ASCII, Markdown and HTML catalogs hold no trace of it.

**Fails first:** `crates/surco/src/surface/terminal/horizon.rs`'s tests: at 40 by 8 cells in Unicode every row holds braille and its rows widen toward the bottom; in ASCII it draws no row.

**Acceptance criteria:**
- [x] Goldens at 60 and 80 cells in Unicode, plain and 16-color, and its absence in ASCII, reviewed in the diff; the Markdown and HTML catalogs hold no trace of it.
- [x] Every other base golden is unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco -- horizon catalog`

**Dependencies:** None. Runs beside Task 72, Task 73 and Task 77.

**Files likely touched:** `crates/surco/src/components/horizon.rs` (new), `crates/surco/src/surface/terminal/horizon.rs` (new), `crates/surco/src/doc.rs`, `crates/surco/src/components/catalog/on_screen.rs`, `crates/surco/goldens/horizon/` (new)

**Estimated scope:** M

### Group check: Surco's pieces of a run's screen
- [x] `python3 .plans/yunta-tui/tools/suite.py` for Surco's, the terminal application's and `yunta-ui`'s groups green; every golden not named by Task 72 to Task 78 equals the base's digest; the walk's counts equal the base's.

**Found in its check:** run on the whole suite, the rule on cuts reaching everything Surco prints: 322 binaries and 5,078 tests, none failing; the gate's short steps pass, the strict doc build among them. The walk's counts equal Phase 2's: 63,842 states, 41 palettes, 72 helps, 6,683 screens at each size. 728 goldens against the base's digests: twenty changed, each named by a task (the catalog and every-block, eleven, by Tasks 63, 64, 72, 73 and 78; `screen/rows`, three, by Task 73's separator; render's `status/waiting.{dig,full}.ascii-80` by Task 73's cut; `tui`'s `field` and `confirmation`, four, by Task 51); six gone, `run`, `palette` and `help` by Task 76; 487 new.

Task 79 to Task 85 each redraw one of Surco's components and bless what draws it; they run one at a time, since each blesses the same golden directories.

### Task 79: the row a person types on, in the design's box

**Description:**
- **The typed row in the design's box** (`design/empty.wide.txt` rows 7-9): three rows at the region's width, the top edge, the words between the uprights with a cell inside each, the bottom edge, in `Shape::Round`'s corners from `glyphs.rs` (`.` and `'` in ASCII); the cursor stands on the middle row, and as much of the words around it as the box holds stays in sight. The box stands where the prompt `> ` stood at a terminal; Markdown and HTML draw the prompt and the words as they do.
- `TypedRow`'s terminal drawing (`crates/surco/src/surface/terminal/typed.rs`) owns it, and every line asked of a person draws through it: a `TypedAnswer` (`surface/terminal/asked.rs:21`), a card's `CardBody::Saying` (`surface/terminal/card.rs:189`) and a round's typed answer (`surface/terminal/card/round.rs:274`); the desk's foot takes it in Task 80. `Card::typed_room` and `typed_room_in_round` count the box's cells. A pane holds its cursor inside the box, counted from the pane's top.
- A list narrowed by typing is not a line asked: it keeps its own row (Task 81).
- Blessed and reviewed: `crates/surco/goldens/{typed_answer,card,components}/`; the CLI's captures of `yunta init -i`'s lines asked and of a card's note at `yunta resolve-gate`, by `prompts.py`; `crates/cli/tests/console_interaction/{init.rs,card.rs}` where they read the typed row (`> repo` at `init.rs:157`).
- Knowledge: `typed.rs`'s `//!` says the box is the one drawing of a line asked, and that a list narrowed by typing is not one.

**Found in its turn:** `TypedRow::drawn` draws the design's box, three rows with round corners and muted edges, the words strong one cell in and the cursor on the middle row; a typed answer, a card's words and a round's answer draw through it, and `fitted` is private to it. The CLI's prompts take it with it: `init -i`, `resolve-gate` and a round differ from the base in the boxed rows and the cursor one row higher (captures in `proof/live-screens/task-79/prompts/`). Goldens that moved, all by the box: Surco's `typed_answer/named`, `card/saying`, `card/round` and the catalog's typed rows, on the five terminal layers; no Markdown, HTML, render, engine or `tui` golden moved. Three tests beyond the list read the typed rows and follow: `console_interaction/round.rs`, `application/starting.rs`, `ui/src/prompt/round/tests.rs`. The walk's counts stand.

**Fails first:** `crates/surco/src/surface/terminal/typed/tests.rs`: a typed row at 40 cells draws three rows, round corners above and below and the words between the uprights; words longer than the box keep the cursor's side in sight; in ASCII the corners are `.` and `'`. `crates/surco/src/surface/terminal/pane/tests.rs`: a typed answer under a paragraph is held with its cursor inside the box. Red at the base, which draws `> ` and the words on one row.

**Acceptance criteria:**
- [x] Goldens of a typed answer, a card that asks for words and a round's typed answer at 60 and 80 cells, ASCII and Unicode, plain and 16-color, reviewed in the diff; the box's rows equal `design/empty.wide.txt` rows 7-9 for the same words at the same width.
- [x] `prompts.py`'s captures differ from the base's only in the typed rows, each a row of the inventory; the inventory's rows for the line a person types on are closed.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco -- typed asked card pane catalog`, `cargo test -p yunta --test console_interaction`
- [x] `YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine`, then the goldens' diff reviewed.

**Dependencies:** Group check: Surco's pieces of a run's screen

**Files likely touched:** `crates/surco/src/surface/terminal/{typed.rs,typed/tests.rs,asked.rs,card.rs,card/round.rs,pane/tests.rs}`, `crates/cli/tests/console_interaction/{init.rs,card.rs}`, `crates/surco/goldens/{typed_answer,card,components}/` (every drawing of a typed row moves at once: the compiler names each caller of `TypedRow::fitted`, which gives way to the boxed row)

**Estimated scope:** M

### Task 80: the card with its options, and a line typed at the desk's foot

**Description:**
- **The card** (`crates/surco/src/surface/terminal/card.rs`) as the design draws it (`design/{scope,failure,ship}.wide.txt` rows 27-34, `sweeps/gap-decisions.md` rows S7-S10, F8-F9, H8-H9): the question under `◆` with the card's edge at its row's far end, what stands under the question wrapped, a blank row, then the options; the route under them (`then …`) as the inventory leaves it; a card whose keys stand in a screen's legend draws no foot of keys, and a card printed by the CLI keeps its keys as its foot.
- **A line typed at the desk's foot**: `FootRow` (`crates/surco/src/components/desk.rs:73-79`) gains the typed line, Task 79's boxed row, which stands in place of the options with what is asked of the words, as Phase 1's target `desk-words` draws it. A card's typed line (`CardBody::Saying`) is Task 79's already.
- The base's frames `card.*` go from `crates/tui/goldens/screens/`, with their entry in `crates/tui/tests/screens.rs` and their builder `card` in `crates/testkit-ui/src/screens.rs`: they draw the base's card; its targets land with Task 113.
- Blessed and reviewed: `crates/surco/goldens/{card,pane,components}/`; the CLI's card at `yunta resolve-gate` and a gate of an attached run, captured by Phase 1's `prompts.py`; `crates/cli/tests/console_interaction/card.rs` where it reads the card's rows.

**Found in its turn:** a card with no keys draws no foot and no route; short of rows, nothing takes an option: the blank rows and the route go first, the question is cut to one row with its edge whole, the reason on its last row, then each option to one row, then the list counts what it hides, the keys last. `FootRow::Saying` draws the option's row, then the box with what is asked faint inside it, then a refusal, on every surface; `TypedRow::drawn` takes a placeholder for it. The base's `card.*` frames are gone with their builder. Goldens: `card/scope.*` at the four sizes written from the targets' card rows (at 120 columns the old drawing already matched: the design's card is the base's without keys or route), `desk/saying.{md,html}`, and `pane/held-card`, three, by the new order of giving way; three tests that held "the list gives way first" follow the design's order. The card's own typed line still draws the chosen option, what is asked, then the box, where the target `card-words` keeps every option with the ask faint inside the box: Task 118's. The options' drawing moved to `card/options.rs` for the line counters. The walk's counts stand.

**Fails first:** `crates/surco/src/surface/terminal/card/tests.rs`: a card with no keys draws no foot and ends on its last option; a desk whose foot types words draws the asked words and the boxed typed row in place of its options, its cursor on that row. Both red at the base.

**Acceptance criteria:**
- [x] The card's goldens equal the design's card rows of `design/scope.wide.txt` for the same data, at 120 cells, every difference one the inventory names.
- [x] `prompts.py`'s captures differ from the base's only in the card's rows the inventory names.
- [x] The inventory's rows for the card's component are closed.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco -- card desk pane`, `cargo test -p yunta --test console_interaction`, `cargo test -p yunta-tui --test screens`
- [x] `YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine`, then the goldens' diff reviewed.

**Dependencies:** Task 79 (Task 77's fill and Task 79's boxed row both stand on the card)

**Files likely touched:** `crates/surco/src/surface/terminal/{card.rs,card/tests.rs,desk.rs}`, `crates/surco/src/components/desk.rs`, `crates/surco/goldens/card/`, `crates/tui/goldens/screens/`, `crates/tui/tests/screens.rs`, `crates/testkit-ui/src/screens.rs` (the deleted builder's callers, the compiler names them)

**Estimated scope:** M

### Task 81: the round, and a list of choices

**Description:**
- **The round** (`crates/surco/src/surface/terminal/card/round.rs`) as `design/ask.wide.txt` rows 10-18 and `ask+enter.wide.txt` draw it (`sweeps/gap-decisions.md` A6-A13): the card's question and edge, a blank row, then each question, the one being answered under `◆` with its choices indented under it, those to come under `◌` wrapped under their text, an answered one folded to one row with its answer; a refused answer under its question; the keys out of the card when a screen holds them. A question answered in words types on Task 79's boxed row.
- **A list of choices** (`surface/terminal/asked.rs`, `ChoiceList`) as the palette and the round draw it (`design/work+palette.wide.txt` rows 9-19): what is typed on its first row after the pointer, `type to filter` faint where nothing is, a blank row, then the rows; a list that hides rows says how many above and how many below it; its keys out of it when a screen holds them. A list narrowed by typing is not a line asked, so it draws no box.
- Blessed and reviewed: `crates/surco/goldens/{card,choice_list,components}/`; the CLI's round (`yunta resolve-gate` on questions) and its lists (`yunta init -i`'s adapters), captured by `prompts.py`; `crates/cli/tests/console_interaction/{round.rs,init.rs}` where they read the rows.

**Found in its turn:** the list of choices is drawn as the palette's and the round's targets draw it: a first row with a muted pointer, then what was typed strong or `type to filter` faint, a blank row, the names with the descriptions three cells past the widest name and cut at the row's end, and the rows it hides counted `… n above` and `… n under` at the names' column through `Line::hidden`, which the feed's counts use too; a list with no keys draws no row of them. `narrowed` and the words `ANSWERED` and `TO_ANSWER` are gone. A round's list has no blank row; short of rows, the blank rows go first, then the questions above and under fold into the counts, then the card's question, the question being answered takes one row, `type to filter` goes, and the list's window narrows last. A scrolled list follows the row on the selection before any row that starts with the pointer, or the palette followed its filter row. Goldens: `card/ask*` at the four sizes (twelve) and `choice_list/palette.unicode-{80,60}` written from the targets; `choice_list/shapes` and the catalog on the five terminal layers blessed; no render, engine, `tui` or CLI golden moved, and the CLI's `init -i`, `new -i` and round differ from Task 80's captures in these rows alone. Two target rows are cut one or two cells short of what `width::truncate` gives at their width (`round-answered.unicode-80` row 11, `round-answered.ascii-120` row 12): the goldens keep the code's cut, and Task 115 settles the targets. The target `document-shape` sets the filter's pointer in the rows' column where the round's and the palette's set it one cell right: followed the two. `new-run`'s and `workflows`' lists are not narrowed by typing and need a list with no filter row, Phase 4's.

**Fails first:** `crates/surco/src/components/choice_list/tests.rs`: a list scrolled past its first row says how many stand above it, red at the base, which says only `N more options` below; `card/round` tests: the question being answered draws its choices indented under it as the design does.

**Acceptance criteria:**
- [x] The round's golden equals `design/ask.wide.txt`'s card rows for the same data at 120 cells, every difference one the inventory names.
- [x] `prompts.py`'s captures differ from the base's only where the inventory names.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco -- card choice_list`, `cargo test -p yunta --test console_interaction`
- [x] `YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine`, then the goldens' diff reviewed.

**Dependencies:** Task 80

**Files likely touched:** `crates/surco/src/surface/terminal/{card/round.rs,asked.rs}`, `crates/surco/src/components/choice_list/tests.rs`, `crates/surco/goldens/{card,choice_list}/`, `crates/cli/tests/console_interaction/round.rs`

**Estimated scope:** M

### Task 82: the desk's rail

**Description:**
- **The rail** as `design/plan.wide.txt` rows 7-16 draws it (`sweeps/gap-decisions.md` P5-P9): the change and the numbered tasks in the one untitled group; `what it says` with its tallies (`decided for you`, `out of scope`, `risks`), each count right-aligned and its key at the far end; `what weighs on it` with its marked sentences. Render composes the groups (`crates/render/src/page/desk.rs:231-312`): the groups the design draws no row for (`order`, `implies`, `impact`) leave the rail as the inventory says, to the pane or the printed page; Surco's rail (`surface/terminal/desk/rail.rs`) draws a group's title and a weighed sentence wrapped under its mark.
- The pane's title and its edge for the change (`1 task`) and for a task's proof (`<task>, its proof` with its criteria counted) as the design draws them.
- The base's frames `desk.*` go from `crates/tui/goldens/screens/`, with their entry in `crates/tui/tests/screens.rs` and their builder `desk` in `crates/testkit-ui/src/screens.rs`; their targets land with Task 116.
- Blessed and reviewed: `crates/surco/goldens/desk/`, `crates/render/goldens/document/desk-*`, the CLI's desk at `yunta resolve-gate` (`r`) captured by `prompts.py`, `crates/cli/tests/plan_desk.rs` where it reads the rail.

**Found in its turn:** render draws every stop of the desk's rail in one untitled group, then `what it says` (`decided for you` with `d`, `out of scope`, `risks`), then `what weighs on it`; order, implies and impact left the rail, and with them Surco's `RailRow::{Pair, Order}` on every surface and render's `short_names` and `impact_paths`. A pane's title says `N tasks` on the change and `<task>, its proof` with its count of criteria. The base's `desk.*` frames are gone with their builder. Goldens: `document/desk-designed-rail.txt`, the target's rows 7 to 16, met exactly; render's `desk-{choices,proof,task,the-change}`, Surco's `desk/plan.*`, `desk/saying.{md,html}`, `pane/held-desk` and the catalog's desk rows blessed. The count beside `decided for you` still leaves out the decisions that restate a round's answers, where the desk's targets count all three: Task 116's. The walk's counts stand.

**Fails first:** `crates/render/src/tests/desk.rs`: the rail of the design's plan holds, in order, the untitled group of the change and its task, `what it says` with three tallies, and `what weighs on it`; red at the base, whose groups are `steps`, untitled tallies and `left out`.

**Acceptance criteria:**
- [x] The desk's golden at 120 cells draws the rail rows of `design/plan.wide.txt` for the design's plan.
- [x] The CLI's desk differs from the base's only in the rail and pane rows the inventory names.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render desk`, `cargo test -p yunta-surco desk`, `cargo test -p yunta --test plan_desk`, `cargo test -p yunta-tui --test screens`
- [x] `YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine`, then the goldens' diff reviewed.

**Dependencies:** Task 81

**Files likely touched:** `crates/render/src/page/desk.rs`, `crates/render/src/tests/desk.rs`, `crates/surco/src/surface/terminal/desk/rail.rs`, `crates/surco/goldens/desk/`, `crates/render/goldens/document/`, `crates/tui/goldens/screens/`, `crates/tui/tests/screens.rs`, `crates/testkit-ui/src/screens.rs`

**Estimated scope:** M

### Task 83: a finding's row, and a checklist's row

**Description:**
- **`FindingList`** and **`FindingRow`** (new, `crates/surco/src/components/finding_list.rs`), with `Block::Findings`: one finding a row, its mark (its severity's, or done once answered), its severity's word in a column as wide as the widest, its title cut at the row's end; the pointed row through `Line::pointed`. A terminal draws it (`surface/terminal/finding_list.rs`, new) as `design/review.wide.txt` row 10 does; Markdown and HTML as a list, with every fact. Render lists its findings through it in Task 88.
- **A checklist's row** stands as the base draws it: the design's row (`design/receipt.wide.txt` rows 8-12) is Surco's (`surface/terminal/checklist.rs`) for the same data, a mark, the subject in the widest subject's column and what it found after it (`sweeps/gap-review-receipt.md`: "same mapping"); the rows that differ are render's, in Task 88. A golden of the design's five checks at 120 cells holds it; where the inventory finds a difference in the drawing, it is made here.

**Found in its turn:** `FindingList` and `FindingRow`, with `Block::Findings`: a row is the pointer (`Line::pointed`), the mark, done once answered, the severity's word muted in the widest word's column, and the title cut at the row's end; an answer stands muted under its title, so an answered finding keeps its row. Markdown and HTML say it as a list. Goldens written by hand, `finding_list/{posted,answered}` at 84, 78 and 58 cells, the first two equal to the targets' rows; the checklist's golden `checklist/receipt.{unicode,ascii}-120` equals the design's rows and passed at once, the base drawing that row already. The catalog's seven gain the list's rows alone. The findings' page takes `FindingList` in Task 88. A checklist's pointer is drawn by its own margin, not `Line::pointed`, and the pages' targets at 60×14 cut a checklist's rows where it wraps: Phase 4's.

**Fails first:** `crates/surco/src/components/finding_list.rs`'s tests: an answered finding draws the done mark and keeps its severity's word; the words line up in one column; the pointed row is filled. They do not compile at the base.

**Acceptance criteria:**
- [x] Goldens of a finding list at 60 and 80 cells, ASCII and Unicode, plain and 16-color, and on Markdown and HTML, reviewed.
- [x] The golden of the design's five checks equals `design/receipt.wide.txt` rows 8-12 for the same data.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco -- finding checklist`

**Dependencies:** Task 82

**Files likely touched:** `crates/surco/src/components/finding_list.rs` (new), `crates/surco/src/surface/terminal/finding_list.rs` (new), `crates/surco/src/doc.rs`, `crates/surco/src/surface/{markdown/mod.rs,html/blocks.rs}`, `crates/surco/goldens/`

**Estimated scope:** M

### Task 84: the boxes over a screen: the palette's with its three columns, and the help's

**Description:**
- **The box** (`crates/surco/src/surface/terminal/boxed.rs`) as `design/work+palette.wide.txt` and `work+help.wide.txt` draw it: rounded corners (`Shape::Round`'s), its title in its top edge between spaces, a cell inside each upright, no keys inside it (they stand in the screen's legend while it is open). How wide a terminal stands it is Task 96's.
- **The palette's three columns**: `ChoiceRow` gains `key: Option<String>`; a list whose rows carry keys lays them out as what each does, its key, then its command in the muted tone, the key and the command standing from a terminal of 100 columns (the design's `wide`, where Task 96's box stands 84 cells and holds the three columns), and only what it does below.
- **The help's rows**: a key in a column of 7 cells and what it does after it, a tagline above them.
- The base's frames `confirmation.*` go from `crates/tui/goldens/screens/`, with their entry in `crates/tui/tests/screens.rs` and their builder `confirmation` in `crates/testkit-ui/src/screens.rs`: the box is one component, and the confirmations' targets (`close-run`, `field-close`) land in Phase 4.
- Blessed and reviewed: `crates/surco/goldens/{boxed,components}/`.

**Found in its turn:** a box has round corners, its words a cell in from its left upright, and keeps no cell inside its right one (the targets cut `help.unicode-60` row 5 and `setup-confirm.unicode-60` row 8 at the upright); a palette's list fills every cell between the uprights, its pointer in that cell and the selection reaching both. `ChoiceRow.key` gives the palette its three columns (what it does in 40 cells, the key in 6, the command muted) from 82 inner cells. `KeyList` is help's rows, built by `render::screen::help`, giving way as the targets do and still scrolling; its key column is 7 cells, or the widest key and two where a key leaves no cell (`pgup pgdn` makes it 11), a rule that gives both help targets at the four sizes. The base's `confirmation.*` frames are gone with their builder, a check of the walk holding every box between the rule and the legend. Only Surco's goldens moved: `boxed/{help,scrolled}`, the new `boxed/palette`, and the catalog's seven by help's rows. Left for the tasks that land each: the three columns show from 82 inner cells, so a box of 84 under 100 columns shows them against the spec's "from 100" (Task 96); the palette's keys still draw inside its box, its list carrying render's legend (Task 112); help has no tagline yet (Task 86). The walk's counts stand, in 154 s and 121 s beside other builds.

**Fails first:** `crates/surco/src/surface/terminal/boxed/tests.rs`: a box's corners are the round ones and its last row the bottom edge; a palette's row in the box a terminal of 100 columns stands holds what it does, its key and its command in that order, and in the box of a terminal of 80, what it does alone. Red at the base.

**Acceptance criteria:**
- [x] The boxed goldens at 60, 80 and 120 cells, ASCII and Unicode, plain and 16-color, reviewed.
- [x] The walk lays out every palette, help and confirmation it opens with the box between the rule and the legend.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco -- boxed choice_list`, `cargo test -p yunta-tui --test screens --test walk`
- [x] `YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine`, then the goldens' diff reviewed.

**Dependencies:** Task 83

**Files likely touched:** `crates/surco/src/surface/terminal/{boxed.rs,boxed/tests.rs,card.rs}`, `crates/surco/src/components/card.rs`, `crates/surco/goldens/boxed/`, `crates/tui/goldens/screens/`, `crates/tui/tests/screens.rs`, `crates/testkit-ui/src/screens.rs`

**Estimated scope:** M

### Task 85: a run's numbers and its graph: the bars of where time went and of who spent it, the graph, and their words

**Description:**
- **Where its time went** (`crates/render/src/page/stats/time.rs`) as `design/work+s.wide.txt` rows 14-16: the heading `where its time went`, the bar across the region's whole width, then its legend, each part's mark in its tone, its name and its time, `·` between, wrapped.
- **Who spent it** (`page/stats/run.rs:300-330`) as rows 18-23: the heading `who spent it`, each role's name in a column, its bar on a track of fixed cells against the largest (a numeric `const` with its reason), then its tokens.
- **A run's numbers' words** (`page/stats/{run.rs,facts.rs}`): the sentence under the ribbon with its period, each labelled row as the design says it, each a row of the inventory.
- **The graph** (`crates/render/src/page/graph.rs`, `crates/render/src/graph.rs`) as `design/work+g.wide.txt`: the headline `<workflow> · the nodes it runs, in order` with its mode at the far end; a node's box holds its id alone on a run's graph too; the parallel groups and the reroute edges the design draws no box for stand as the inventory says.
- The words are reworded here with the drawing, in the files it touches, rather than in Task 88.
- Blessed and reviewed: `crates/render/goldens/{stats,graph}/`, `yunta stats` and `yunta graph` captured by Phase 1's `prints.sh`; `crates/cli/tests/{stats_cmd.rs,graph_cmd.rs}` where they read those rows.

**Found in its turn:** a run's numbers put the facts first (`task cost` reading `…, 1 task done`), then `where its time went`, a bar across the whole width with a legend whose parts open with `●`, then `who spent it`, a role a row, the most first, its name in 14 cells, a bar of 28 on a track of 30 (`SPENT_BAR`) and its tokens muted; what the printed page adds, the rails and the nodes, comes after. The graph's page is headed `<workflow> · the nodes it runs, in order`, the mode at the far end or `as written`, every box holding its id alone; `Graph.run` became `mode`, and `graph::drawn` names nodes by their place and leaves out an edge a longer chain already makes. Every figure of the design's run equals its target's: 65% at work, 37% of rework, 96% cached, the tokens and every role. `yunta graph` prints Mermaid or DOT, never the page, and keeps its `--run` labels; `yunta stats` changes as said (`stats/run.*` blessed: the comma, `who spent it` above the nodes; `graph/{as-written,chained,of-a-run}.*`: the headline, the labels gone); `stats/designed.*` and `graph/designed.*` are new, the first written by hand from the targets' rows. The design's graph never drew its diagram: `@` in an id defeats Surco's parser, and the frozen workflow carries the edges a node reads along, which the page's diagram leaves out. The targets `graph` and `workflow-graph` hold the recording's diagram, a chain, where the workflow fans `implement` out to three nodes: the code's rows equal the target's once trimmed, seventeen columns to the right, centred over the real fan-out; Task 111 settles the targets against the workflow's own diagram. The graph no longer moves with a node's news. At 58 cells the nodes' row `waiting on you` wraps, as at the base; `nodes:` and `rails:` keep the printed page's style. The walk's counts stand.

**Fails first:** `crates/render/src/page/stats/run/tests.rs`: the headings read `where its time went` and `who spent it`; the time bar takes the line's whole width and each part of its legend opens with a mark; the runners' bars stand in one column. `crates/render/src/page/graph/tests.rs`: a run's graph is headed by the design's headline and labels each node by its id. Red at the base.

**Acceptance criteria:**
- [x] The stats and graph goldens differ from the base's only in rows the inventory names; `prints.sh`'s captures of `yunta stats` and `yunta graph` likewise.
- [x] The inventory's rows for the two bars, a run's numbers, the graph's drawing and its headline are closed.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render -- stats graph`, `cargo test -p yunta --test stats_cmd --test graph_cmd`
- [x] `YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine`, then the goldens' diff reviewed.

**Dependencies:** Task 84

**Files likely touched:** `crates/render/src/page/stats/{time.rs,run.rs,facts.rs}`, `crates/render/src/page/graph.rs`, `crates/render/src/graph.rs`, `crates/render/goldens/`

**Estimated scope:** M

### Group check: the components the design redraws
- [x] The suite's groups green; every golden changed is one Task 79 to Task 85 names, its diff reviewed; Phase 1's captures differ from the base's only where those tasks name; the walk's counts equal the base's.

**Found in its turn:** run twice: the first run met a disk under the pool's floor (19 GB free where 24.7 GB stays free), where every group from `engine-4` on failed on `NoRoom` and `crates/cli/tests/session_ends.rs` waited three hours on a session that never started; the trap on a full disk says so now. With room: the gate's short steps pass; the suite's groups hold 5,150 tests, none failing, none aborted; the walk reaches 63,842 states, 41 palettes and 72 helps, and draws 6,683 screens at each size, in 88 s and 87 s. Against the base's digests 69 goldens changed, each one a task of this phase or the last names, 12 are gone (the base's `card`, `confirmation`, `desk`, `help`, `palette` and `run` frames at their two sizes) and 529 are new. Of 159 captured views 11 differ from the base's, each where a task names: the node's printed page (its feed), the cut that no longer ends inside a number (`needs` at 120 columns: `until…`), the typed line in its box (`init -i` twice, `resolve-gate`, the round), the list of choices with its filter row (`new -i`), and a run's screen under 102 columns with no side, as the design draws it at 80 (`review` at a terminal, twice).

### Task 86: the keys' words, and what a screen notes: the legends, the far pair, help's rows, the palette's rows and the sentence for an answer recorded

**Description:**
- **Render's words for every key a run's screen and what stands over it says** (`crates/render/src/screen.rs`, `screen/run.rs`), each a row of the inventory: a run's legend by its scene (`j k node · f follow it · s stats · g graph · q field, it keeps going`; the findings' `j k finding`; a card's, a round's, the desk's, a confirming desk's), the numbers' and the graph's, the palette's and help's; the far pair `anywhere()` (new) with `/ anything · ? keys`. `run_legend` takes what the scene offers and whether the run leads, and says no key a scene does not take.
- **Help's rows**: the tagline and the keys every screen answers to, in the design's words (`design/work+help.wide.txt` rows 9-19), then the keys of the screen under it that its legend leaves out.
- **The palette's rows**: each act's words as the palette says them (`stop it and everything under it`, `what it cost, and where its time went`, `start a new run`), owned once in `command.rs`. A next step for an act the palette words says the palette's words on every page that offers it (`command::effect`, the `next` of `yunta receipt`, of `receipt.md` and of the pull request's body, the pages of `yunta init` and `yunta new`): `write the receipt`, `verify its evidence`, `stop it and everything under it`, `close it for good`, `continue it from its log`, `start a new run`, `check adapters, git, forge, packs`; a next step for an act no palette words keeps its words. Blessed: `crates/render/goldens/{status,open,closing}/`, `crates/engine/goldens/{receipt,forge}/`, and `docs/` where `docs_sync` holds what they quote.
- **The sentence for an answer recorded** (new in `screen/run.rs`), one per kind of option: `Recorded: grant. The session goes on with the wider scope.`, `Recorded: retry. A fresh attempt starts from the run's tree.`, `Recorded: approve. On to \`pr\`.`; `Nothing recorded. The run stays waiting on these questions.`; what leaving a run says (`The run keeps going: no engine lives in this window.`, that it waits, that it ended); a decision answered elsewhere, and by whom, in the log's words.
- `over_every_stage` stays for the stages Phase 4 moves; the run's stage and those over it take `anywhere()` from Task 92.

**Found in its turn:** render owns the keys' words and what a screen notes: `screen::anywhere()` (`/ anything · ? keys`); `run_legend` from what the scene offers (`RunKeys`, `Along`, `Quit`, `Leaving`) and whether the run leads; the legends of a card, a round, the desk and a confirming desk; help's tagline and `help_over_a_run`; `palette_legend`; `recorded`, `NOTHING_RECORDED`, `answered_elsewhere` in the log's words; the titles `what do you want to do?` and `keys`. `command::does` and `effect` say the palette's words, one const each, and the pages of the receipt, `init`, `new`, cancel, the gate's pull request and a refusal call them: `write the receipt`, `verify its evidence`, `close it for good`, `check adapters, git, forge, packs`, `follow it as it moves`. The targets `palette` and `field-palette` word two acts that show one command, not one act twice: over the field `follow it as it moves` is what `enter` and the command do; over a run `the run's whole chronicle` is the row of the act Task 110 lands. No target holds the task's example legend with both `f` and `g`: with `f follow it` the legend leaves `g` out, as `node-working` draws it. The sentences no target gives (a recorded answer for each option the design does not show, a gate that finishes or goes back, leaving a run that waits or that ended) were each held to the engine's code, eight of them corrected by it (a rejected gate fails and claims no reroute; a promotion closes the run and a new one goes on; a cap is passed until the run next stops), and are the lead's to show the person. Goldens, the next steps' words alone: render's `closing/` (9), `open/` (5), `status/` (36), `initialized/` (9), `gate/pull-request.md`; the engine's `receipt/{finished,failed,no-baseline}.md`, `forge/{pull-request,gate}.md`; no line of `docs/` quotes them. The run's stage, forced by the new signature, says `j k node`, its acts, `s stats`, `g graph` and `q quit` until Task 94 gives `q` the field; the pages over a run drop `q leaves yunta`; stats keeps `f every row` until Task 111. The CLI's prompts keep their own legends, so a card's keys are worded twice until Tasks 113 to 118. The walk opens 51 helps where it opened 72, one for each distinct legend now that a run's legend names no node; its 63,842 states and 41 palettes stand. `a_promoted_runs_graph_moves_onto_its_successor` (`crates/ui/tests/stats_graph.rs`) failed once beside the batch on `surfaced.is_none()` and passed alone: stressed apart, before the group's check.

**Fails first:** `crates/render/src/tests/screen.rs`: the legend of a run at work while it leads reads the design's row; help's first row is the tagline; `anywhere()` holds `/` and `?` with the design's words; the sentence recorded for a grant is the design's. Red at the base.

**Acceptance criteria:**
- [x] Every word of these rows is in render and nowhere else (`cargo xtask smells --check`).
- [x] The goldens and captures that change are the ones the inventory names; `crates/ui/tests/walk.rs`'s `MUST_REACH` and `crates/cli/tests/application.rs`'s awaited words follow the words they quote.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render -- screen command`, `cargo test -p yunta-ui --test walk --test palette`, `cargo test -p yunta --test docs_sync --test application`
- [x] `YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine`, then the goldens' diff reviewed.

**Dependencies:** Group check: the components the design redraws

**Files likely touched:** `crates/render/src/{screen.rs,screen/run.rs,command.rs}`, `crates/render/src/tests/screen.rs`, `crates/render/goldens/status/`, `docs/guide.md`

**Estimated scope:** M

### Task 86a: a question answered at the engine's terminal lands with its answer

**Description:** an engine that asks at its own terminal writes the question and its answer once the answer is in hand, and wrote them as two commits (`gate_waiting`, then `gate_resolved`). A reader that looks between the two finds a gate standing with no answer: the frame says the run needs the person, and the application calls them for a question already answered. Found by a test of Phase 2 that fails under contention (`a_promoted_runs_graph_moves_onto_its_successor`, 19 runs of 400, each on `Some(CallThePerson)`); the run that turned was the test's own promoted run, at the batch that ended on `gate_waiting`.
- **The pair is one commit.** `Store::append_together_at` (and `AsyncStorage::append_together`) writes several drafts in one transaction, each row and its run's head through the one `append_row` every append takes; `RunLog::record_decided` drafts the question and the answer and appends them together; `RunCtx::emit_decided` is what the engine's sites call.
- **Every site that asks at the terminal** takes it: an escalation, an internal gate, a budget's cap, a console gate, a loop's escalation.
- A reader of the log, in this process or another, finds the question and its answer or neither; a crash between the two no longer leaves a question standing that was answered.
- Knowledge: the `//!` of `crates/storage/src/store.rs` (events appended together are seen all or none) and of `crates/engine/src/run_log.rs`; the test's `Reached by:` names the five sites.

**Found in its turn:** the cause was not in the fold, the follower or the frame, which read the log truly: the log held the question alone for an instant. Nothing outside the test's bench was read. A first form made every `emit`'s future larger (2,208 to 2,840 bytes) and overflowed the stack of `follow_build`'s `a_promoted_run_hands_over_the_checkout_it_moved_to`; the storage's future is boxed, and `emit` takes 2,144 bytes, `emit_decided` 2,288. Stressed with the same command, 16 processes of 25 runs: 19 of 400 before, none of 400 after, and none of 400 for the test's sibling on stats. The branch `feature/agentic-flows` changes four of the five sites (`internal_gate.rs` most of all): the person chose to keep the correction here, and its patch stands apart in the plan's `fixes/` for whoever joins the branches. Left: while the scripted answer is given the engine's registry shows the run asking at its terminal for under a millisecond; a look that lands there calls the person by the rule, and the test's helper would refuse it (none in 960 runs); the helper says so once it accepts that call. The words' socket of a run (`/tmp/yunta-<uid>/<run id>.sock`) is shared by processes that give two runs one fixed id, as a stress of one test does; it carries what sessions said and moves no standing.

**Fails first:** `crates/engine/tests/asking.rs`, `a_question_answered_at_the_terminal_lands_with_its_answer`: the log read after every commit, through the engine's observer, never ends on a question with no answer. Red: one reading ends `["asking_opened", "gate_waiting"]`.

**Acceptance criteria:**
- [x] No reading of the log between two commits holds a question answered at the terminal without its answer.
- [x] The promoted run's tests pass under contention: none of 400.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-engine`, `cargo test -p yunta-storage`, `cargo test -p yunta-ui` (every target but `walk`), `cargo test -p yunta --test parked_runs`
- [x] `python3 .plans/yunta-tui/tools/stress_binary.py <stats_graph's binary> 16 25 <folder> a_promoted_runs_graph_moves_onto_its_successor`

**Dependencies:** Task 86

**Files likely touched:** `crates/storage/src/{store.rs,async_storage.rs}`, `crates/engine/src/run_log.rs`, `crates/engine/src/run/{ctx.rs,escalation.rs,internal_gate.rs,budget.rs,console_gate.rs,loop_exec/escalate.rs}`, `crates/engine/tests/asking.rs`

**Estimated scope:** M

### Task 87: a card's question and each option with what it does, and a decision's gist

**Description:**
- **A card's question** for each menu (`crates/render/src/claim.rs`, `escalation.rs`): `` `tests` failed. What now? ``, `It asks to touch <path>`, the gate's own message, as the inventory says; its edge (`prompt/card.rs:23`).
- **Each option's name and what it does** (`crates/render/src/offers.rs`, `blocks/failure.rs`, `blocks/decision.rs`): `grant  the task may also touch this path, and goes on`, `retry  Starts a fresh attempt…`, `approve  On to \`pr\`: opens the pull request on <forge>`. The words an older log stored for an option still win (`offers.rs:30-46`), so `crates/render/tests/older_words.rs` holds unchanged.
- **A decision's gist** (`crates/render/src/state.rs:148`): `a task asks to touch a path outside its scope`, `a task departs from what the plan decided`, `regression: …`, the ship's sentence, a round's `node \`grill\` asks three questions: nothing is recorded until every one is answered`.
- Blessed: the CLI's card and the run's page that print the same (`crates/render/goldens/{status,gate,document}/`, `prompts.py`'s captures), and `crates/cli/tests/parked_runs/cap.rs`, `crates/engine/tests/failed_node_retry.rs` where they read the words.

**Found in its turn:** render owns a card's question (`claim::question`, `escalation::question`) and its edge, the node's or the run's handle where there is none; each option's words on both menus (grant, deny, retry, the loop's retry, the budget's continue, continue-work, accept, send-back), the departure's facts `the plan decided` and `the session says instead`, and the gists of a scope claim, a round, a red check and a departure; `TokensSpent` names the run by its handle. The gist of a scope claim is one, the design's `a task asks to touch a path outside its scope`: the targets `node-needs-you` and `still-unread` stand at the cut of `scope` and `card-words` and kept the base's gist from the dump on their gist rows, which the task that lands them corrects. `abort` reads `Pause here: nothing runs; this decision stays open` on every menu, the departure's too; the `departure` target's row dropped `Pause here:` and is corrected the same way. The ship's approve, which needs the forge and the repository, is Task 87a. No golden moved: the fixtures' logs store their options' words, and stored words still win; `older_words.rs` changed in the two checks that quote words derived for lines that stored none, its stored-word checks holding. The round's question `{node} asks` and its handle are Task 115's; the desk keeps the claim as its question. The sentences no target gives, each true of the engine's code, are the lead's to show the person: the plural forms of grant, deny, accept and send-back for several tasks, continue-work's, the retry that waits on something outside the tree, the questions and gists that count tasks, a round's `asks 1 question`.

**Fails first:** `crates/render/src/tests/offers.rs` and `cards.rs`: the scope's grant reads the design's name and words; a failure's question reads `` `tests` failed. What now? ``; the scope's gist is the design's. Red at the base.

**Acceptance criteria:**
- [x] Every one of these phrases has one owner in render, and the CLI prints it where it prints the same thing.
- [x] The inventory's rows for a card's words and a decision's gist are closed; the captures differ only where they name.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render -- offers cards claims retry`, `cargo test -p yunta-render --test older_words`, `cargo test -p yunta --test parked_runs`
- [x] `YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine`, then the goldens' diff reviewed.

**Dependencies:** Task 86

**Files likely touched:** `crates/render/src/{offers.rs,claim.rs,state.rs}`, `crates/render/src/blocks/{failure.rs,decision.rs}`, `crates/render/src/tests/offers.rs`

**Estimated scope:** M

### Task 87a: an option says what lies ahead of it: the pull request a node opens, and on which repository

**Description:** the ship's approve reads ``approve  On to `pr`: opens the pull request on github eulke/yunta`` in the design, and render knew of an option only the node it leads to. Found by Task 87.
- **`yunta_core::ahead`** (new): `Ahead` says what a node an option may lead to does that its name does not say (`OpensPullRequest { repo }`, `LeftOut(Because)`), and `NodesAhead::of` reads it for every node from the run's frozen workflow, its config's forge and the nodes the run leaves out. The log holds none of it, and no event gains a field.
- **It travels in what a decision asks**: `DecisionAsked`, filled where the engine frames an asking, then `WaitingDecision`, `DecisionPut`, render's `escalation::Decision`, the run's page and the CLI's closing block, so the application's card and the CLI's say the same.
- **Render's words read it**: `option_words`, `decision_options` and the sentence recorded for an approve (``On to `pr`.``). An option that leads to a node of another kind says ``On to `<node>` `` and nothing it does not know; one that leads past a node the run leaves out says so.

**Found in its turn:** a `pull_request` node with no forge declared is left out of the run or refuses it before it starts, so it never stands ahead of a decision as one that opens anything: the option then says ``Resolve the gate: `pr` is not in this run: the project declares no `forge` ``, and its recorded sentence ``The gate finishes; `pr` is not in this run.`` No golden moved. A program's JSON of a decision reads no workflow and says nothing of what lies ahead: whether it should is the person's to say, since every interface is meant to hold the same facts. `ask_human` in the engine and the recorded sentence's helper in render were split to stay under fifty lines; core's crate map names `ahead`.

**Fails first:** `crates/core/tests/ahead.rs`: a workflow whose gate leads to a `pull_request` node, under a config that declares a GitHub repository, reads `OpensPullRequest` with it; one left out for lack of a forge reads `LeftOut`. Render's `tests/offers.rs`: the ship's approve reads the target's row. Red: ``On to `pr`: resolves this gate, and settles every finding it shows``.

**Acceptance criteria:**
- [x] The ship's approve reads the design's row in the application's card and the CLI's.
- [x] No event of the log gains a field.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-core --test ahead`, `cargo test -p yunta-render -- offers`, `cargo test -p yunta-engine`, `cargo test -p yunta-api`, `cargo test -p yunta-ui` (every target but `walk`), the CLI's groups

**Dependencies:** Task 87

**Files likely touched:** `crates/core/src/{ahead.rs,lib.rs,port/person.rs}`, `crates/core/tests/ahead.rs`, `crates/engine/src/run/asking.rs`, `crates/api/src/{answers.rs,run/page.rs}`, `crates/ui/src/prompt/{decision_put.rs,decision_card.rs}`, `crates/render/src/{offers.rs,escalation/decision.rs,document/decision.rs,screen/run.rs}`, `crates/cli/src/{commands/drive.rs,surface/closing/mod.rs}`

**Estimated scope:** M

### Task 88: the findings, the checklist and the receipt

**Description:**
- **The findings** (`crates/render/src/page/findings.rs`): the heading `findings · 0 of 1 answered · review, review-alt, conform` (answered of posted, then the nodes that posted); a row's words; the answer as `fixed: <why>` under a done mark. `page::Findings::Run` (`:65`) lists its findings through Surco's `FindingList` (Task 83), then each one's place, detail and answer.
- **The checklist and the receipt** (`page/receipt.rs`): the design's five rows (`1 of 1 green, red before the work`, `cargo test --workspace still passes`, `6 files, 2 of them granted by you`, `4 findings, 4 answered`, `intact`), then `cost`, `changed` and `pull request` under them as the receipt's pane lays them out, and the gist `every criterion green, every finding answered, the log's chain intact`, which the receipt's scene draws under its ribbon, from the ledgers Phase 1 made. A printed receipt (`yunta receipt`, `receipt.md`, the pull request's body) takes the same rows in the same order, its `run` row staying first and its headline keeping `receipt of`; it prints no gist; and it keeps on each row, after the design's words and the separator, each fact the inventory leaves to the printed page (`intact · 342 events, hash-linked, replayable`; the cost's split in and out; the baseline's regressions, comparisons, suite and hash; the review's runners); the criteria row counts the task's criteria and says `red before the work` where the task's check before the work (`TaskRecord::checked_before`, Task 41c) has each of them red, the guards counted apart (`2 guards green`, after the separator on a printed receipt); the ship's line `4 findings, 4 answered · 1 task, 1 criterion green`. The checklists of doctor and verify (`page/checklist.rs`) draw through the same row.
- Blessed and reviewed: `crates/render/goldens/{document,closing}/`, `crates/engine/goldens/{receipt,forge}/` (the receipt and the pull request's body), `crates/cli/tests/receipt_cmd.rs` where it reads them; `docs/guide.md` where `docs_sync` holds its quotes.

**Found in its turn:** the findings' page of a run carries the design's heading (answered of posted, then the nodes that posted) and its findings through Surco's `FindingList`, `fixed: <why>` under an answered row, then each one's place, detail and answer. The receipt's rows say the design's words in the pane's order on the screen and in print (`Told::{OnScreen, Printed}`, `gist`, `verdicts`), a printed receipt keeping after the separator each fact the inventory leaves to it. Core's `Receipt` gains what the rows count (the criteria red before the work, the guards, the grants, the findings, the pull request), derived in replay; it publishes no schema, and `cargo xtask schema` moves nothing. The printed baseline row follows its print target, the suite's command in a code span, where the screen sets it plain: one phrase on two surfaces (`` `make test` still passes · 0 regressions across 2 comparisons … ``, where it read `0 regressions across 2 comparisons …`). The tests stand in `page/{findings,receipt}/tests.rs`: aligned rows under `src/tests/` read as production to the smell counters. Goldens, the rows' order and words alone: the engine's `receipt/{finished,failed,no-baseline}.md` and `forge/pull-request.md`, each equal to its print target. Phrases no target gives, for the lead to show: the guards' counts, the cost with no cost per task, `{p} of {n} criteria green`, the findings' heading's tail by severity and `settled`, and ``fixed: {why} · by `{node}` ``. The walk's counts stand: 63,842 states, 41 palettes, 51 helps, 6,683 screens at each size.

**Fails first:** `crates/render/src/tests/findings.rs`: the heading of one finding posted by three reviewers, none answered, reads the design's, and its rows are a `FindingList`; the receipt's criteria row reads `1 of 1 green, red before the work`. Red at the base.

**Acceptance criteria:**
- [x] The inventory's rows for findings, the checklist and the receipt are closed.
- [x] `yunta receipt`, `receipt.md` and the pull request's body differ from the base only in the rows the inventory rewords (each keeping after the design's words what the inventory leaves to the printed page), in the pane's order of rows, and in their next steps' words, listed for Checkpoint C.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render -- findings receipt`, `cargo test -p yunta --test receipt_cmd --test docs_sync`, `cargo test -p yunta-engine --test receipt`
- [x] `YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine`, then the goldens' diff reviewed.

**Dependencies:** Task 87

**Files likely touched:** `crates/render/src/page/{findings.rs,receipt.rs}`, `crates/render/src/tests/findings.rs`, `crates/render/goldens/`, `crates/engine/goldens/`

**Estimated scope:** M

### Group check: render's phrases of a run's screen
- [x] The suite's groups green; every golden and capture that changed against the base is one Task 86 to Task 88 names, each reviewed; `cargo xtask smells --check` and `cargo xtask docs --check` pass.

**Found in its turn:** the gate's short steps pass, the schema's among them; the suite's groups hold 5,147 tests, none failing, none aborted; the walk reaches 63,842 states, 41 palettes and 51 helps, and draws 6,683 screens at each size (110 s and 119 s under a load of 19 and 10). Against the base's digests 141 goldens changed, each one a task of this phase or the last names, 12 are gone and 529 are new. Of 159 captured views 67 differ from the base's and 61 from the last check's, every line one of this group's: the next steps in the palette's words (`close it for good`, `write the receipt`, `verify its evidence`, `check adapters, git, forge, packs`, `start a new run`, `follow it as it moves`), in the pages and in a program's JSON (`why`); retry's words and the retry that waits on the outside (`label`, `tradeoff` in JSON too); a card's edge, the node alone; a round's gist (`asks 1 question`, JSON's `said`); the receipt's rows in the pane's order (`scope  no file touched`, `event chain  intact · 8 events, hash-linked, replayable`); a run's legend (`j k node`). `yunta receipt --json` gains fields, none lost: the criteria's `tasks`, `proving`, `proved`, `red_before`, `guards`, `guards_green`, the scope's `granted`, and `findings` with `posted` and `answered`.

### Task 89: `ScrolledTo` in `Scroll`

**Description:**
- **`ScrolledTo`** (new, `crates/ui/src/scroll.rs`), `End` or `Row(usize)`: where a person scrolled a region to. `Scroll` keeps it in place of `top`; `sight()` maps `End` onto `Sight::at_the_end()` and `Row` onto `Sight::from_line`. A region that grows stays at its end while it is there.
- Keys: `End`, and a key that reaches the last row the surface laid out, go to `End`; any other key that scrolls goes to a row. `Scroll::at_the_end()` makes a region that opens at its end (a feed); `Default` opens at its first row (a page), as every stage scrolls at the base.
- The desk's pane (`prompt/plan_desk.rs:270`), the one caller of `top()`, reads its row.
- Knowledge: `scroll.rs`'s `//!` says the two places a region stands at.

**Found in its turn:** `ScrolledTo { End, Row(usize) }` is what `Scroll` keeps in place of `top`: `sight()` maps `End` onto `Sight::at_the_end()` and a row onto `Sight::from_line`; `End`, and any key that reaches the last first line laid out, go to `End`, every other key to a row; a key does nothing on a region the last surface showed whole. `top()` is gone, the desk reading `sight().top`. Two held asserts of `crates/ui/tests/scroll.rs` compared `sight.top` with the `most` the base kept, and now say the first row shown: the same behavior, read from what the region keeps. `Scroll::at_the_end()` is not made: nothing opens a region at its end until a pane holds a feed, and it would be dead code; Task 97 makes it. The run's stage takes the arrows for its nodes, so the new test steps back with `PageUp`. The walk reaches 53,686 states by this task alone, where it reached 63,842: a page at its end is one state however long it grew, where each line `End` had reached was a state of its own.

**Fails first:** `crates/ui/tests/scroll.rs`: a region at its end that grows between two surfaces' `Laid` is still at its end; `PageDown` onto the last row goes to its end, and `Up` from there to a row. Red at the base, where `End` goes to a `most` the growth left behind (`scroll.rs:70-82`).

**Acceptance criteria:**
- [x] Every stage that scrolls a page scrolls it as before: the tests `crates/ui/tests/scroll.rs` held before this task pass unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test scroll`, `cargo test -p yunta-ui --lib prompt`

**Dependencies:** Group check: render's phrases of a run's screen

**Files likely touched:** `crates/ui/src/scroll.rs`, `crates/ui/src/prompt/plan_desk.rs`, `crates/ui/tests/scroll.rs`

**Estimated scope:** S

### Task 90: `PointedBy`, and who leads a run's screen

**Description:**
- **`PointedBy<T>`** (new, `crates/ui/src/stage/run/lead.rs`), `TheRun(Option<T>)` or `ThePerson(T)`: who moves a pointer, kept for the node pointer and for the finding pointer, each kept by the node or the finding it stands on, never by a position. Not the base's `pointed: Option<NodeId>`, which it replaces.
- **The pointer's rule**, while the run leads: on the node the run waits on a person at, when one waits; else on the first node at work in the workflow's declared order, kept there while it works; else on the last node that ran. A pause or an end that names no node leaves it where it was, and the scene shows the run's own (Task 91). The finding pointer: on the first finding with no answer, else on the last.
- **`RunStage::leads()`**, derived and never stored: both pointers are the run's and the pane stands at its end (`ScrolledTo::End`). `j k` take the node pointer onto the node moved to; scrolling the pane back takes the pane; `f` gives all three back. While the run does not lead, its news move neither pointer nor the pane. What the person took is kept under any stage over the run; leaving to the field gives it back.
- The trail names the run and the node pointed at, always (`design/work+j.wide.txt` row 1).
- **`yunta_testkit_ui::screens::run`** (new module, where Task 76 deleted the base's `run` builder): the application opened on a target's cut, taken by the target's name from `yunta_testkit_ui::cut::TARGET_CUTS` (its runs and seqs, registry, node pointed at and keys pressed), loaded through `Cut::of_target` and driven as Phase 2's helpers drive. Every later test of a scene builds its state from a target's name.
- Knowledge: `lead.rs`'s `//!` says who leads and how it is given back; `run.rs`'s `//!` drops the base's following.

**Found in its turn:** `crates/ui/src/stage/run/lead.rs` holds `PointedBy<T>` (`TheRun`, `ThePerson`), the node pointer's rule, the finding pointer's (the first with no answer, else the last) and `RunStage::leads()`. `j`, `k` and `enter` take the node pointer, so a picked page and the trail agree; `f` gives both pointers back and drops a picked page; the pointers are placed again as the run's news fold and on coming back from the field. The trail names the pointed node always, and `run_legend` takes `leads()`. One case the rule did not say: a run that waits on a person where no node says so itself points at the node its pause names (the menuless escalation of `standstill` names `approve` in its claim alone). The pane is not in `leads()` yet: the pane is still Phase 2's page, which opens at its head, and the CLI's tests at a terminal wait for that head at 80×24; opened at its end it would hide it. The pane joins `leads()` when it holds a feed (Task 97): it opens at its end, scrolled back it is the person's, and `f` gives the three back. The test kit's `screens::run::{opened, RunScreen}` opens the application on a target's cut by the target's name and presses its pointed node and keys (`pressed`, `went_on`, `Cut::goes_on`); it does not yet apply a cut's `asked` or `goes_on_to`, which the first task that needs them adds. A probe held the trails of the 41 targets of a run's screen to the rule: all match but `receipt` and `reported`, whose trail names the receipt's scene (Task 108). Tests that held the trail ending at the handle, or the pointer on the first node, follow the rule: `scroll.rs`, `decision_stage.rs`, `followed.rs`, the walk's run-page key, the CLI's `interface/{start,run}.rs`; `page.rs`'s `followed_on` is `page_turned`. No golden moved. The walk: 55,574 states (63,842 before the group; Task 89 alone 53,686, this task adds 1,888), 41 palettes, 59 helps (51: the eight with `f follow it`), 5,460 screens at each size (6,683), in 75 s and 71 s.

**Fails first:** `crates/ui/tests/lead.rs` (new), on the cut of `findings` (the design's run, three reviewers at work): the node pointer stands on `review@reviewer` and stays there as `conform` moves; `j` takes it to the next node and the next event leaves it there; `f` brings it back to the node at work; the trail names the pointed node. Red at the base, where the pointer falls to the first node.

**Acceptance criteria:**
- [x] A test for each case of the rule: a node waiting on the person, the first at work kept while it works, the last that ran, and a finding pointer on the first unanswered.
- [x] `crates/ui/tests/run_stage.rs` passes with the pointer's moves it holds rewritten to the rule.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test lead --test run_stage`

**Dependencies:** Task 89

**Files likely touched:** `crates/ui/src/stage/run.rs`, `crates/ui/src/stage/run/lead.rs` (new), `crates/ui/tests/lead.rs` (new), `crates/ui/tests/run_stage.rs`, `crates/testkit-ui/src/screens.rs`, `crates/testkit-ui/src/screens/run.rs` (new)

**Estimated scope:** M

### Task 91: `Scene`, made with no wildcard arm

**Description:**
- **`Scene<'r>`** (new, `crates/ui/src/stage/run/scene.rs`), the spec's enum: `Work`, `Board`, `Findings`, `Round`, `Desk`, `Card(&WaitingDecision, Grounds)`, `PullRequest`, `Standstill(Standstill)`, `AskedElsewhere`, `Receipt`, `Ended`, `Node`. **`Grounds`** (new): what a card's decision rests on, drawn above it: what a gate shows with the change, a failure's output, a scope's refused write, a departure, what a cap spent. Not `Evidence`, a criterion's proof; not render's `Gate::AboveTheCard`. **`Standstill`** (new): why a run stands with nothing to answer, a pause or no process driving it; not `Standing::Stopped`, a person's stop.
- **`fn scene(run: &RunInView, frame: &RunFrame, pointed: &NodeId, leads: bool) -> Scene`**: `match` alone over `RunPhase`, `WaitingOn`, `PauseReason`, `Menu` (from the escalation the engine rebuilt, `awaited`, never the log's), the pointed node's `Standing` and its `NodeKind` (read from the manifest `RunInView` keeps, since `NodeFrame.kind` is a `&'static str`, `crates/replay/src/frame/node.rs:30`), with no wildcard arm; it takes `RunFrame::actions`, `awaits_decision`, `PauseReason`'s own questions and `desk::plan_among` as given and decides only what stands where. A prompt stands only while the run leads. `#![deny(clippy::wildcard_enum_match_arm)]` on the module.
- **The run's stage draws by scene** from this task on: each arm draws what Phase 2 drew (the run's page, or the pointed node's) until its family's task replaces it.
- Knowledge: `scene.rs`'s `//!` holds the rule that every closed set has a screen, the lint and the test that hold it, and that no `if let` or `matches!` decides a scene.

**Found in its turn:** `scene(run, frame, pointed, leads)` in `crates/ui/src/stage/run/scene.rs` says which scene stands, every closed set it reads matched by name (`RunPhase`, the run's and the node's `Standing`, `WaitingOn`, `PauseReason`, `Menu`, `NodeKind`, `Failure`, `TaskCause`) under `#![deny(clippy::wildcard_enum_match_arm)]`: a wildcard added there fails clippy (`wildcard match will also match any future added variants`), seen once. It takes the pointed node's frame, so no case stands for a node the run does not hold; the menu is the awaited decision's, and only one read for the run's present pause. The run's page draws through a `match` on the scene in `page.rs`, each arm drawing what the stage drew; `State::scene()` lets a test read it. The table of scenes and four targets disagreed, and the targets govern, the spec's four rows brought to them: a pause not yet read, an ask that could not be read and an answer awaiting its resume draw as their node at work draws, the board where it is a loop (`still-unread`, `still-answered`); a gate that shows a plan is the desk where the change it counted holds no file and a card with the change where it holds files, the plan one `r` away (`RunInView::change_counted_at`: `approve-plan` counted none, `ship` seven); what a pointed node shows does not depend on who points, so the node a failed, promoted or broken run's end names shows the end and any other node its own scene (`lead::node_led`); `ChildPaused` is a standstill of the parent, the child's screen being the application following the child. `promoted` with the person leading is not asserted yet: the stage still moves onto the successor, which Task 109 changes; `broken` and `failed` hold the rule. A scope owed, a capped loop and a departure owed behind a failure's menu give the card their grounds. Two rules written twice have one owner each: `Failure::departures_owed()` in core, read by render's gist and claim; `RunInView::awaited_decision()` and `awaited_round()`, read by the stage's offers and the scene. The test kit applies a cut's `asked: NotYet` (`RunScreen::handed`) and presses a pointed node with `j` so the person holds the pointer. No golden moved; the walk's counts stand.

**Fails first:** `crates/ui/src/stage/run/scene/tests.rs` (new): a scene for every `Standing` of the pointed node (`Standing::ALL`), each the arm the spec's table names; `crates/ui/tests/scenes.rs` (new): the cut of each target of a run's screens (the design's run at `round`, `work`, `board`, `desk`, `scope`, `findings`, `failure`, `ship` and `receipt`, and each small run's) makes its row's scene. They do not compile at the base.

**Acceptance criteria:**
- [x] A wildcard arm added in `scene.rs` fails `cargo clippy` (seen once, then taken out).
- [x] Every screen draws as after Phase 2: `crates/ui/tests/run_stage.rs` and the walk pass unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --lib scene`, `cargo test -p yunta-ui --test scenes --test run_stage --test walk`
- [x] `cargo clippy -p yunta-ui --all-targets -- -D warnings`

**Dependencies:** Task 90

**Files likely touched:** `crates/ui/src/stage/run.rs`, `crates/ui/src/stage/run/scene.rs` (new), `crates/ui/src/stage/run/scene/tests.rs` (new), `crates/ui/tests/scenes.rs` (new), `crates/testkit-ui/src/screens/run.rs` (new) in Task 90

**Estimated scope:** M

### Group check: the scene and the lead
- [x] The suite's groups green; the walk's states counted against the base's, its time recorded; every golden equal to the base's or named by an earlier task.

**Found in its turn:** the gate's short steps pass; the suite's groups hold 5,170 tests, none failing, none aborted; the walk reaches 55,574 states, 41 palettes and 59 helps, and draws 5,460 screens at each size, in 77 s and 72 s (63,842 states and 6,683 screens at the last group: a page at its end is one state however long it grew, Task 89). The same goldens differ from the base's as at the last check: 141 changed, 12 gone, 529 new, none by this group. Of 159 captured views two differ from the last check's, `review` at a terminal twice, in the trail alone, which names the pointed node (`yunta › <handle> › approve-plan`, Task 90).

### Task 92: a run's screen in its frame: the tally, the attention row, the legend and the note

**Description:**
- The run's stage composes the design's frame (spec elements C1 to C5, E1) through **one composer of the tally and the attention row**, `FollowedRuns::framing(&self, at: Standpoint, now) -> (Tally, Option<Attention>)` (new, in Phase 2's `crates/ui/src/followed_runs.rs`), which the stats and graph stages (Task 111) and Phase 4's stages call too. **`Standpoint`** (new, beside it): the run whose screen the person is on, if any, whether that run's own decision stands in sight, and whether a line is typed there. Not `FieldReach`, how far the field reaches.
  - **the tally** from the followed field: how many runs need the person and how many stalled, a run asked at its own terminal among them; `Tally::Counted` with none says `nothing needs you`;
  - **the attention row**, tinted: on a run's screen whose own decision is not in sight (the run does not lead, or a stage stands over it), that decision with `a go there`; elsewhere the run that waited longest on the person among those answered here, by its handle; never a run asked at its own terminal. **Where a line is typed**, `a` types, so the row names no key (Task 73's `Answer::Unkeyed`): here under a palette over the run. **Where the run's own decision stands in sight** (a round, a card or the desk, from Task 113 on), no attention row stands, a line typed there or not, as the design leaves that row empty; each passes the same `Standpoint`;
  - **the legend** of the scene, in Task 86's words, with `anywhere()` beside it, left out where a line is typed;
  - **the note**, `Screen.noted`, which a refused or failed act sets, standing until the next key; news leave it. **A remark** (Phase 1's `RunNews::Remark`, a manifest's unknown keys, which Phase 2's fold keeps for the run in view) is said once as the note, the first time the run's screen draws after it came.
- `yunta-tui`'s `drawn.rs` draws the note in the attention row's place, the attention otherwise, and the legend with its far keys.
- `crates/render/src/watched/mod.rs`'s attention row stays the CLI region's; the application's comes from render's words for it (`watched::needing`, new), which `framing` calls.
- Knowledge: `followed_runs.rs`'s `//!` says that every stage frames itself through `framing`, and why a typed line takes the key away.

**Found in its turn:** `FollowedRuns::framing(Standpoint, now)` (`crates/ui/src/followed_runs/framing.rs`) hands the tally and the attention row: the tally counts the field's open runs that need the person and that stalled; the attention row comes from render's `watched::needing` (the handle, the gist, `a go there`, no key where a line is typed). The run's stage frames itself in `stage/run/frame.rs`: tally, attention, the scene's legend with `anywhere()`, and the note (`Notice`: a refused act under `✓`, a failed one under `✗`, a remark under its weight's mark, `Weight::mark`); `drawn.rs` draws the note in the attention row's place. The targets ask a third case of where the run's own decision stands, which the list's text gave as yes or no: `ItsDecision::{InSight, InItsPane, OutOfSight}`, since on `still-unread` and `standstill` the run leads, its decision is the pane's and not shown yet, and the row names another run. `node-needs-you` is held to the design's gist and not to its stale row; `still-unread`'s attention row names another run and needs no correction, its gist row alone does. Remarks are said one a key and never take the place of a note that stands. The test kit's cuts had no project configuration, so a run's screen stood over the set-up and followed no field: `cut.rs` writes one where none came. The walk opened help only where `?` stood in the legend, which now stands among the far keys: it reads them too. Left for the tasks that follow: `/` and `?` never reach the stage, so a note stays under the palette or help (Task 93); the layout keeps the row above the legend only while something stands in it, where the conventions keep it at every size (Task 96); a box can cover the attention row at 24 rows, and the walk skips screens with a box (Task 112); `card-unread` draws no attention row, its scene being work until Task 103 counts its decision as in sight. No golden moved; the CLI's region stands. The walk: 72,623 states (55,574 before: the tally and the attention row say the field under the run, so states it merged are told apart; with those rows left off it reaches 55,562), 41 palettes, 60 helps, 8,051 screens at each size (5,460), in 113 s and 109 s; its bound is raised to 95,000 by the person's standing rule.

**Fails first:** `crates/ui/tests/frame.rs` (new): with the cut of `failure` and the home's other runs followed, the tally counts them; pointing another node (the run not leading) puts the decision in the attention row with `a go there`; a run asked at its terminal is counted and never named; the palette open over the run leaves the attention row with no key; a refused act's note stands through a piece of news and goes at the next key; a remark kept for the run is the note once, and gone after the next key. Red at the base.

**Acceptance criteria:**
- [x] The walk's layout checks hold the attention row above the legend on every state, with the note in its place where one stands.
- [x] No stage but the run's composes the frame yet; stats, graph and the field draw as after Phase 2.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test frame --test run_stage --test stats_graph`, `cargo test -p yunta-tui --test walk`

**Dependencies:** Group check: the scene and the lead

**Files likely touched:** `crates/ui/src/stage/run.rs`, `crates/ui/src/followed_runs.rs` (Phase 2's), `crates/tui/src/drawn.rs`, `crates/render/src/watched/mod.rs`, `crates/ui/tests/frame.rs` (new)

**Estimated scope:** M

### Task 93: a run's keys, and the keys whose meaning changes

**Description:**
- The run's stage takes the spec's keys for a run: `j k` along the nodes; the arrows, the wheel, `PgUp`, `PgDn`, `Home`, `End` and `space` through the pane; `f` gives the lead back; `s` and `g`; `r` and `x` where the scene offers them; `enter` nothing beyond a prompt; `n` a new run where no prompt stands (the start stage, until Phase 4); `a` to what the attention row names: on this run's own decision, its prompt (the decision stage until Task 117), else that run's screen in place of this one. `esc` and `q` are Task 94's.
- Gone: a run's `p` and `w` (`acts.rs:73-82`). Kept out of the legend and listed in help and the palette: `c`, `e`, `v`, `o`.
- `docs/guide.md`'s "The application at a terminal" says a run's keys as they are.
- Knowledge: `acts.rs`'s `//!` names the keys a run binds and the ones it leaves to help and the palette.

**Found in its turn:** `j k` move along the nodes and show the pointed node's page, which `enter` did; the arrows, `PgUp`, `PgDn`, `Home`, `End` and `space` scroll the pane and leave the pointer where it is; `f` gives the lead back; `a` goes where the attention row points (`FollowedRuns::named_in_attention`, by framing's rule): another run's screen in place of this one, or, on the run's own decision, the lead given back and the decision's stage, until its prompt stands in place; `n` opens a new run's start over the run where no prompt stands; `r` and `x` act where `RunFrame::actions` offers them; `enter`, `p` and `w` do nothing. The legend names `r` and `x` in render's words, `n` on the receipt alone (`Scene::names_a_new_run`), and leaves `c`, `e`, `v`, `o` to the palette. Any key takes a note away. One row stands against the targets for now: the run's legend is led by the shared `↑↓ scroll · pgup pgdn by the page` where its pane scrolls, which the targets do not draw; without it the interface's walk, which lays nothing out, takes a scrolled state for the one it left and its check that pages scroll fails. The task that lays a feed out drops that row and makes the walk lay out each state it reaches. Help is built from the legend, so `c e v o` and the scroll keys are not in help until the palette and help land over a run; `w` opened a parked parent's child, which nothing reaches until the child's task; `x` shows on `no-room`, `orphans` and `still-answered`, where the targets draw none, until the standstill's task; where the attention row names nothing and the run waits on a decision not yet read, `a` opens the decision's stage. `docs/guide.md` says a run's keys as they are. The CLI's tests that pressed `q` on a run (`watching`, `deciding`, `review`, `starting`) go through one helper that asserts the note where the run's standing is settled.

**Fails first:** `crates/ui/tests/run_stage.rs`: on a run at work the arrows scroll the pane and leave the pointer on its node; `p` does nothing; `a` with another run named goes to its screen in place of this one. Red at the base, where the arrows move the pointer.

**Acceptance criteria:**
- [x] Each key of the spec's row for a run, and each of its list of keys whose meaning changes that a run takes, has a test, `esc` and `q` in Task 94.
- [x] `crates/cli/tests/application/watching.rs` passes with the keys it presses rewritten to these.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test run_stage --test palette --test walk`, `cargo test -p yunta --test application -- watching`, `cargo test -p yunta --test docs_sync`

**Dependencies:** Task 92

**Files likely touched:** `crates/ui/src/stage/run.rs`, `crates/ui/src/stage/run/acts.rs`, `crates/ui/tests/run_stage.rs`, `crates/cli/tests/application/watching.rs`, `docs/guide.md`

**Estimated scope:** M

### Task 94: `q` and `esc` leave a run for the field, with the note they leave

**Description:**
- **`q` and `esc` go to the field** from a run with no prompt standing, with a note that says truly whether the run keeps going, waits or ended, in Task 86's sentences (`The run keeps going: no engine lives in this window.`). The state goes back down to the stage at the bottom of what stands under the run.
- **Where the first request stands at the bottom** (a repository that had no run when the application opened, and a run started from it since), the field, made from the kept field, takes its place there: `q` always ends on the field, never on the first request.
- **The note goes to the field's `said`** (`crates/ui/src/stage/field.rs:50`), which the field draws today where it says what an act said; Phase 4 moves it into the attention row's place.
- Knowledge: `state.rs`'s `//!` says that leaving a run ends on the field, which takes the first request's place at the bottom.

**Found in its turn:** `q` and `esc` leave a run for the field: `Want::ToTheField(said)`, and `State::down_to_the_field` drops to the stage at the bottom, keeps a field that stands there, and lets any other, the first request among them, give way to the kept field; the note goes to the field's `said`. `Framed::leaving()` picks the sentence: a run that ended, one an engine drives (`The run keeps going: no engine lives in this window.`), else one that waits. The legend says `q field`, with `it keeps going` where it does; render's `Quit` is gone, one variant being left, and `unread_run_legend` says `q field` before the run is read; Ctrl-C still leaves the application. `esc` goes to the field even where a prompt stands, until prompts stand in place. `runs_left.rs` passes unchanged: its `q`s are pressed on the evidence's stage, which still leaves the application until Phase 4. The walk: 85,806 states (72,623 before: the arrows scroll a line at a time, `j k` read each node's page, `n` opens the start over every run, `q` and `esc` land on a field under one of three notes), 38 palettes (41) and 55 helps (60), the row of `enter` for each node and `c e v o` having left the legends; 7,118 screens at each size (8,051). The interface's walk took 170 s, its test 189 s, begun on an idle machine: past the three minutes at which the lead stops for the person; the layout's took 104 s.

**Fails first:** `crates/ui/tests/run_stage.rs`: `q` on a run at work goes to the field, whose note reads `The run keeps going: no engine lives in this window.`; on a run waiting on the person the note says it waits. `crates/ui/tests/state.rs`: on the first request, a run started from it and its screen opened over it, `q` stands the field alone at the bottom. Red at the base, where `q` leaves the application.

**Acceptance criteria:**
- [x] `crates/cli/tests/application/runs_left.rs` passes with the keys it presses rewritten to these.
- [x] The note stands on the field for each of the three ends (keeps going, waits, ended), each a test.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test run_stage --test state --test field --test walk`, `cargo test -p yunta --test application -- runs_left`

**Dependencies:** Task 93

**Files likely touched:** `crates/ui/src/state.rs`, `crates/ui/src/stage/field.rs`, `crates/ui/src/stage/run/acts.rs`, `crates/ui/tests/{run_stage.rs,state.rs}`, `crates/cli/tests/application/runs_left.rs`

**Estimated scope:** M

### Task 94a: the note a run leaves is true of a run started a moment ago

**Description:** `q` pressed on a run this window started, before its engine's registry was read, left the note that the run waits beside a field that said it runs. Found by a stress of one test of the application at a terminal (`starting::a_workflow_started…`: 5 runs of 60, 9 of 120). The legend and the note follow the conventions: `it keeps going` where an engine drives the run, `q field` where none does or it ended.
- `Framed::leaving()` matches by name, with no wildcard arm: a closed run has ended, a cancelled one that closed among them; a run whose registry reads an engine alive keeps going; a run whose registry reads unrecorded keeps going only where this window handed it to a detached engine itself; every other run waits. The legend's `q` and the note read that one decision.
- `FollowedRuns.handed_on` keeps the runs this window started, from the start's outcome until the first reading of the run's engine that is not `Unrecorded`, of the run in view or of the field.

**Found in its turn:** the lead first ruled that the note says what the run's standing says, without reading the conventions; `created` and `still-answered` draw `q field` on a run that stands created or running with no engine, and the ruling was withdrawn. An absent registry reads unrecorded at any age and a run stalls only on a registry that names a dead process, so a detached child that dies before it writes one leaves a run that reads as going for good: open work, in `.agents/open.md`. Stressed with the same commands: none of 60 and none of 120. The walk's counts stand; it starts no run.

**Fails first:** `crates/ui/tests/run_stage/leaving.rs`: on the cuts of `created` and `still-answered` the legend reads `q field` and `q` leaves the waiting note; a run that closed cancelled has ended. `crates/ui/tests/state.rs`: a run started from the first request keeps going while its registry reads unrecorded, and waits once its engine is read dead.

**Acceptance criteria:**
- [x] The legend's `q` and the note agree on every run, and say what the conventions say.
- [x] `starting::a_workflow_started…` passes under contention: none of 120.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test run_stage --test state`, `cargo test -p yunta --test application`

**Dependencies:** Task 94

**Files likely touched:** `crates/ui/src/stage/run/acts.rs`, `crates/ui/src/stage/run.rs`, `crates/ui/src/followed_runs.rs`, `crates/ui/src/state.rs`, `crates/ui/tests/run_stage/leaving.rs`, `crates/ui/tests/state.rs`

**Estimated scope:** S

### Group check: a run's frame and its keys
- [x] The suite's groups green, the application's pty tests among them; the walk's counts against the last group check's.

**Found in its turn:** run on the profile that optimises five packages in a debug build. The suite's groups hold 5,193 tests: one failed in its batch and passed three times alone (`context_command_timeout_leaves_no_process_alive` of `crates/engine/tests/process.rs`, the open work on a command's group still seen running), none aborted; the application's tests at a terminal pass. The walk reaches 85,806 states, 38 palettes and 55 helps, and draws 7,118 screens at each size, its tests in 90 s and 87 s (175 s and 141 s before the profile). The same goldens differ from the base's as at the last check. The release binary weighs 31,343,968 bytes. Of the gate's short steps one fails, and not by this tree: `cargo deny` reads a new advisory, RUSTSEC-2026-0543, `chrono` unmaintained, every version; it is the person's to rule before the phase's commit. The captures' scripts pressed `q` once to leave a run, which now goes to the field: `prompts.py` and `screens.py` press it again there. Captured again, 68 of 159 views differ from the base and three from the last check: `review` at a terminal twice, which ends on the field under the note `The run has ended: leaving it changes nothing.` or with the run among those that need the person, and the bare field's ages, an effect of capturing it apart.

### Task 95: what stands across a run's screen, its nodes beside it, and its meter

**Description:**
- **Across** (`Screen.about`): the headline (H1, `Run::headline`, `crates/render/src/page/run/mod.rs:147`), the ribbon with its names (H2), the gist (H3, `state::gist`), from the run in view at each draw; the base's rows of attention, working nodes and counters leave the application, and the CLI's region keeps them.
- **The ribbon's names**, by question 3's rule: **`watched::stretch_names(ids: &[&str]) -> Vec<String>`** (new, `crates/render/src/watched/mod.rs`), over a workflow's node ids in declared order, names each stretch by the part of its id before `@`, else its first word split at `-`, where no stretch before it in declared order has that name; else its last word under the same condition; else its whole id. `watched::ribbon` (`:340`) names its stretches through it, and Phase 4 calls it for a mode with no run. Fragua's reads `grill brief plan spec approve implement review alt conform fix lint fix-lint tests ship pr`; where every name does not fit, the ribbon draws strokes alone.
- **The nodes list** (`watched::nodes`, `:400`): a node a row (N1): the pointer, the spinner's frame for a node at work (N3, by its events) or its mark, its name in a column of 19 cells, how long it took right-aligned in 7 for a node that ended; a loop's tasks under it, `└─` and each task's mark (N2); the mark of a node sent back (E16); `f follow what it is doing` under the nodes while the run does not lead (E3); the pointed row through `Line::pointed` (E2). Numeric widths carry their reasons.
- **The meter** (C6): `Screen.meter: Option<Meter>` (new field), which the run's stage sets from `watched::spent(frame) -> Meter` (Phase 2's): tokens, one bar a node, time at work, and time waiting on a person where it is not zero. Task 96 lays it out.
- **Deleted**: `watched::footer` with its words (`footer_words`), the row of `watched::rows` that carried it and their tests in `watched/tests.rs`: the meter replaces its last caller, the application's run screen (`crates/ui/src/stage/run.rs:167`).

**Found in its turn:** render's `watched::headline` says a run's headline across its screen (`for` while it is open, `in` once it closed, through `page::stood_open`, which `Run::headline` shares), the ribbon is named through `watched::stretch_names` (fragua's fifteen), and the gist stands on a row of its own. `watched::nodes` lives in `watched/nodes.rs`: the pointed row through `Line::pointed`, one spinner frame for every node at work, names in 19 cells, how long a node took in 7, a loop's tasks on `└─`, `↻` for a node a gate sent back, `f follow what it is doing` while the person leads. `Screen.meter` is set from `watched::spent`, and Markdown and HTML say it too; `watched::footer` is gone. The targets governed three points of the task's text: how long a node took shows for a node that finished or was rerouted, none for one that failed; a task's mark is `●`, not the spinner; the ribbon's strokes alone are an even `floor(w ÷ n) − 1` cells, which turned a Surco test that held them to the page's furrow. The node a run waits on a person at draws `◆`, as `scope`, `still-unread`, `node-needs-you`, `card-words` and `failure` draw it: the rule that names it has one owner, replay's `RunFrame::waits_on_a_person_at()` with `WaitingOn::names()`, read by the list and by the pointer's rule in `lead.rs`, whose own copy is gone. `Place::Task.nth` went, nothing reading it. The CLI's live region and every printed page draw as they did; no golden moved, `screen-meter` for Markdown and HTML being new and written by hand.

**Fails first:** `crates/render/src/watched/tests.rs`: `stretch_names` over fragua's node ids, with no run, reads the rule's fifteen names, and the ribbon of a run of it the same; the nodes of the design's run at `board` draw `implement`'s task under it with `└─` and the spinner on `implement`; `review@reviewer` and `review@reviewer-alt` keep their names whole in the column. Red at the base.

**Acceptance criteria:**
- [x] The CLI's live region draws as after Phase 2 (`crates/cli/tests/run_surface.rs`, `live_motion.rs` unchanged); `git grep -n "fn footer" crates/render/src/watched` finds nothing.
- [x] The inventory's rows H1 to H3, N1 to N3, E2, E3 and E16 are closed in render's tests; the run's screen holds its meter at every scene.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render watched`, `cargo test -p yunta-ui --test run_stage`, `cargo test -p yunta --test run_surface --test live_motion`, `cargo test -p yunta-surco screen`

**Dependencies:** Group check: a run's frame and its keys

**Files likely touched:** `crates/render/src/watched/{mod.rs,tests.rs}`, `crates/render/src/page/run/mod.rs`, `crates/ui/src/stage/run.rs`, `crates/surco/src/components/screen.rs`

**Estimated scope:** M

### Task 96: a run's screen at its sizes

**Description:**
- **The meter's row**: `yunta-tui` draws `Screen.meter` across the inner width on the row over the attention row's.
- **The box over a screen** stands as wide as what it draws needs, measured as `drawn::holds` measures its rows, up to 84 cells (`BOX_MOST`, its `///` reason the palette's three columns at 120 columns), and never wider than the terminal less its insides.
- **The sizes** (`crates/tui/src/layout.rs`, `drawn.rs`): the main region's title on the main column's first row, the side starting on the same row (`design/work.wide.txt` row 8); on a run's screen the rows from the top are the trail, the rule, what stands across, a blank row, the regions, the meter, the attention row and the legend, as the targets lay them out; short of rows, what stands across gives way before the regions keep their fewest; taller, the regions take the rows. The walk's copies of the rules (`crates/tui/tests/walk.rs:39-56`) follow.
- **The four environments** of the targets join `crates/tui/tests/screens.rs`, each drawing a state that `screens::run` builds from a target's name.
- Knowledge: `layout.rs`'s `//!` gains the meter's row and the title row of the main column.

**Found in its turn:** the layout puts the meter on the row above the attention row, and keeps the row above the legend on every screen; a title is the first row of its own column; short of rows the blank row under what stands across goes first, then those rows (`LEAST_REGIONS`); a box is as wide as what it draws needs, 84 at most (`BOX_MOST`). Surco's `Terminal::across` drops the ribbon's names before anything else, `Terminal::trail` the tally's words before it cuts the trail (`◆ 1 · ▲ 1`, as `findings` draws it at 60 columns), and a headline whose edge is said elsewhere lets its handle go before it is cut (`board-tasks`, `orphans`); the conventions give no order past the handle, and the code takes the facts from the last, then the subject. The pane's place below 102 columns: the desk's pane says where it stands among the places (`2 of 2 · step 1 of 1`). `crates/tui/tests/screens.rs` draws the four environments and holds eleven targets of the design's run everywhere but the pane and the legend; the walk's copies of the layout's rules follow. Left to the tasks that land each: the boxes' widths against their targets (help 68 for 70, a confirmation 84 for 64) and the palette's three columns from 88 columns; the desk pane's title, the task's title where the targets show its id; the receipt's trail and gist; `f follow…` after the feed is scrolled back. A stress of one test of the application at a terminal (`starting::a_workflow_started…`, failed once in a batch) found a defect of Task 94: 5 runs of 60 and 9 of 120 end on a note that says the run waits beside a field that says it runs, `q` having landed before the run's engine read alive; corrected apart. The walk's counts stand: 85,806 states, 38 palettes, 55 helps, 7,118 screens at each size, its tests in 97 s and 87 s.

**Fails first:** `crates/tui/src/layout/tests.rs`: a screen holding a meter lays it out on the row over the attention row; a main title stands on the main column's first row while the side's first row is a node; a box is never wider than 84 cells. Red at the base, where the title row spans both regions and a box stops at 60.

**Acceptance criteria:**
- [x] The walk's layout checks pass at both its sizes, the meter's row among them.
- [x] `crates/tui/tests/screens.rs` draws a state at 120×36, 80×24 and 60×14.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --lib layout`, `cargo test -p yunta-tui --test walk --test screens`

**Dependencies:** Task 95

**Files likely touched:** `crates/tui/src/{layout.rs,layout/tests.rs,drawn.rs}`, `crates/tui/tests/{walk.rs,screens.rs}`

**Estimated scope:** M

### Task 97: a feed's window, its counts and its keys

**Description:**
- **A region holding a feed is laid out as its feed** (`crates/surco/src/surface/terminal/pane.rs`), as a region holding a desk is laid out as its pane: the blocks above it stand still and give way from their top before the feed does; the feed's window takes the rows they leave; its newest row never gives way; `Laid` is counted from the feed's first row (its window's start plus the rows above it), so a place stays on the rows in sight whatever grows above or under.
- **The window** (new, `crates/ui/src/stage/run/feed.rs`): the run's stage hands render only the moments around the place, sized by the last `Laid` with a margin, from `RunInView`'s moments of the node; `ScrolledTo` holds the place; the feed says how many rows stand above its window and how many under it, a number that grows as more arrive; `End`, `f` or scrolling to its last row go with the end again; only the feed scrolls in a pane that ends with one.
- **Same words**: each row says what `status --follow` prints for the same moment after its time and its subject; held where Phase 2's tests do not hold it already.
- The screen a feed scrolled back draws, its counts and `f follow what it is doing` under the nodes, is the target `feed-back`, which lands with the pane it stands in (Task 98).
- Knowledge: `pane.rs`'s `//!` gains the feed's rule; `ScreenLaid`'s `///` says a feed's region is laid out as its feed.
- **The pane that holds a feed is the run's while it leads** (from Tasks 89 and 90): `Scroll::at_the_end()` opens it at its end, so its newest moment is in sight and stays there as the feed grows; scrolled back it is the person's, and the run's news move it no more; `RunStage::leads()` is both pointers the run's and the pane at its end, and `f` gives the three back.

**Found in its turn:** a region whose document ends with a `Feed` is laid out as its feed: the blocks above stand still and give way from their top only where the window would keep fewer than three rows (`FEED_LEAST`); `Laid` is counted from the whole feed's first row, and a page is the window's rows less one, so a page key moves by the rows between the two counts. The run's stage hands render a page and a margin of 64 rows on each side (`MARGIN`) from `RunInView`'s moments of the node (`stage/run/feed.rs`), and keeps two places, the page's and the feed's, the feed's opening at its end; keys and layouts go to the one the pane shows. `RunStage::leads()` is both pointers the run's and the feed at its end; `f`, `j k` and a successor put the feed back at its end; scrolled back it is the person's and news leave it where it stands, the counts above and under growing. A pane ends with its feed in the work, board and findings scenes and a node's own (`Scene::ends_with_its_feed`), unless a node's page is picked; there the feed stands under the run's page where the run's chronicle stood, with no heading yet. The legend's `↑↓ scroll · pgup pgdn by the page` is gone, as the targets draw it: the interface's walk reads the place a pane stands at (`State::pane()`) and needs no layout for it. Surco's goldens `pane/feed-at-its-end` and `pane/feed-scrolled-back`, six files written by hand, equalled the code at once, the first at 60 columns equal to `work`'s pane at 60×14; nothing was blessed. The CLI's tests at a terminal pass unchanged, `FINISHED` standing in the headline across the screen. Clippy's bound on a large variant boxed the stage's `turned`. The cut of `feed-back` presses `up` three times with no layout between, which does nothing: the test kit gains `RunScreen::laid`, and the task that lands that target lays out after each key at each size; the order in which blanks and the scope give way at 60×14 is the scenes' too. The walk: 66,694 states (85,806 before: only the feed scrolls where the whole page scrolled a line at a time, 21,422 fewer; a run read before its following came adds 2,310), 38 palettes, 49 helps (55, six legends having differed by the scroll row alone), 5,289 screens at each size (7,118), its tests in 89 s and 80 s.

**Fails first:** `crates/surco/src/surface/terminal/pane/tests.rs`: a page with a feed under three blocks, in rows too few for all, keeps the feed's newest row last and cuts the blocks from their top; `crates/ui/src/stage/run/feed/tests.rs` (new): a node of ten thousand moments hands render no more rows than the last `Laid`'s page and its margin; scrolled back by the arrows, ten moments more leave the rows in sight where they stand and raise the count under them by ten.

**Acceptance criteria:**
- [x] Goldens of a held feed in a pane at 60 and 80 cells, ASCII and Unicode, plain and 16-color, reviewed in `crates/surco/goldens/pane/`.
- [x] A feed's place, rows above and rows under are asserted at its start, its middle and its end.
- [x] A pane on a feed opens at its end and follows it; scrolled back it stays where the person left it as the feed grows; `f` brings it back to its end with both pointers, and `leads()` says so.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco pane`, `cargo test -p yunta-ui --lib feed`, `cargo test -p yunta-ui --test scroll`

**Dependencies:** Task 96

**Files likely touched:** `crates/surco/src/surface/terminal/{pane.rs,pane/tests.rs}`, `crates/surco/src/components/screen.rs`, `crates/ui/src/stage/run/feed.rs` (new), `crates/ui/src/stage/run/feed/tests.rs` (new), `crates/surco/goldens/pane/`

**Estimated scope:** M

### Task 98: the work scene

**Description:**
- The rows "the node runs a session" (what it hands over, each artifact with its standing; what it says; its feed), "the node runs a command or a check" (what it runs; its result and the tail of the output kept, once it has them; its feed) and "created, no node started" (what runs first), composed by `crates/ui/src/stage/run/pane.rs` (new) from render's parts, holding no sentence.
- **The pane's title and its far words** (P0): `blocks::NodeTitle` (new, `crates/render/src/blocks/node_title.rs`): the node and its kind, and at the far end who works it (`planner on codex/gpt-6-luna`), its task, its check or its assignee, as `Region`'s title and edge.
- The feed's heading counts the open attempt's calls; `session opened` stands before the first.
- The panes that end with the node's feed are the work, board and findings scenes and a node's own; a card's, a round's, the desk's, a standstill's, the receipt's and the ended scenes show none, as the frames draw them, each held by its own task's target.
- Targets, moved (`git mv`) from `crates/tui/goldens/targets/` at the four sizes: `work`, `work-moved` (one event on), `work-command` (a check at work), `created`, and `feed-back` (the work scene's feed scrolled back by the arrows, its counts above and under, `f follow what it is doing` under the nodes). Cuts: the design's run at `work`, one event on, while `tests` runs, at its creation, and at `work` scrolled back.
- `crates/cli/tests/interface/run.rs` reads the node's feed rows where it read the chronicle under the page, against the same piped follower's lines.

**Found in its turn:** the work scene composes its own pane (`crates/ui/src/stage/run/pane.rs`) from render's parts, where it showed the run's page: the title, the node and its kind with who works it at the far end (`blocks::NodeTitle`), what the node runs (`blocks::NodeCommand`) or what it hands over, and the feed last under a heading that counts the open attempt's calls (`chronicle::open_attempt_calls`, shared with the node's printed page). A pane with a title leaves a blank row under it, and in five rows or fewer drops every blank row. Landed, moved beside the goldens and held whole at the four sizes: `work`, `work-moved`, `work-command` and `created`. `created` at 60 columns needed the conventions' rule on the meter: at the shortest look the sparkline gives way with the look and not with the width, as the ribbon's names do; the layout owns that decision (`Areas.short_of_rows`, true where the gap under what stands across gave way) and the screen draws the meter's figures alone there (`Terminal::meter_figures`), the meter as a component keeping its bars where they fit. `feed-back` draws the board's pane, a loop with its task, so it lands with the board's scene and stays among the targets, held around its pane and by its legend with a layout before each `up`. `what it says` is drawn nowhere, as the notes ask; a command's result and the tail of its output are not drawn, no state of the work scene holding them. A node with no moment draws an empty feed with no heading: falling back to the page's scroll there made the walk reach 86,674 states. The lead's tools read a landed target beside the goldens. The walk: 64,940 states (66,694 before, the run's page having left the work pane), 38 palettes, 49 helps, 5,189 screens at each size, its tests in 92 s and 82 s.

**Fails first:** `crates/tui/tests/screens.rs`: `work` at `unicode-120` differs from its target (the run's page stands there); `crates/ui/tests/scenes.rs`: the work pane's last row is the feed's newest call.

**Acceptance criteria:**
- [x] The five targets equal at the four sizes.
- [x] The inventory's rows of elements P0 and P1 are closed.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --test screens`, `cargo test -p yunta-ui --test scenes`, `cargo test -p yunta-render blocks`, `cargo test -p yunta --test interface run`

**Dependencies:** Task 97

**Files likely touched:** `crates/ui/src/stage/run/pane.rs` (new), `crates/render/src/blocks/node_title.rs` (new), `crates/tui/goldens/{targets,screens}/`, `crates/testkit-ui/src/screens/run.rs` (new) in Task 90, `crates/cli/tests/interface/run.rs`

**Estimated scope:** M

### Task 99: the board scene

**Description:**
- The row "the node runs a loop": each task at work with its spinner, title (from the tasks document handed over) and attempt; how it is proved, `proved` red or green, `by` its command, `keeps` what the run holds every task to; `its scope`, a path a row, marked where a session of the task wrote it, `granted by you` beside a grant; then the node's one feed under every task.
- With several tasks at work, each feed row names its task after its verb, and each scope marks the paths its own sessions wrote; on a log without a call's session a row names no task, and a path is marked only where its task is the node's one open task or its scope check ran.
- A scope's paths give way before the feed.
- Targets, moved (`git mv`) from `crates/tui/goldens/targets/`: `board`, `board-moved` (one event on), `board-tasks` (two tasks at work). Cuts: the design's run at `board` and one event on; `yunta_testkit_ui::small_runs`' loop with two tasks at work at once, their calls naming their sessions, held by `awaits_word`.
- **`feed-back`** (from Task 98, whose text set it in the work scene): its target draws the board's pane, the loop `implement` with its task, its feed scrolled back by the arrows with its counts above and under and `f follow what it is doing` under the nodes. It lands here, moved with the board's targets, a layout laid before each key at each size (`screens::run::opened_laid_out`).

**Found in its turn:** the board's scene composes its pane (`pane/board.rs`): each task at work, running or blocked, in the order they were registered, with its title from the tasks' document, its proof and its scope, over the loop's one feed, which stands one step in with its heading. A region carries abridged forms of what stands above its feed (Surco's `pane/fed.rs`), and the pane draws the first that leaves the feed's window its three rows: no scopes, then no blank row under a task's row, then the tasks' rows alone; a pane that stands close keeps the last whole. `TaskProof::turning` sets the spinner on a task's row. Landed and held whole at the four sizes: `board`, `board-moved`, `board-tasks` and `feed-back`. A task's title comes from a document the follower hands over, which came after `State::caught_up` said so: it waits for those documents too (`yunta_api::Document::handed_over_in`). A blocked task counts as at work, and `still-unread` draws one. At 80×24 the tasks' rows alone are tried before blocks give way from their top, a state no target reaches. `Scene::composes()` says which pane a scene composes, in place of a yes or no. The walk's counts stand, its recordings holding no loop.

**Fails first:** `crates/tui/tests/screens.rs`: `board` differs from its target; `crates/ui/tests/scenes.rs`: with two tasks at work each row of the feed names its task, and a scope cut by too few rows keeps the feed's newest row.

**Acceptance criteria:**
- [x] The three targets equal at the four sizes; the inventory's rows of elements N2 and P2 are closed.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --test screens`, `cargo test -p yunta-ui --test scenes`

**Dependencies:** Task 98

**Files likely touched:** `crates/ui/src/stage/run/pane.rs` (new) in Task 98, `crates/tui/goldens/{targets,screens}/`, `crates/testkit-ui/src/screens/run.rs` (new) in Task 90, `crates/ui/tests/scenes.rs` (new) in Task 91

**Estimated scope:** M

### Task 100: the findings scene

**Description:**
- The row "findings posted, and the node reviews or fixes": how many are answered and which nodes posted; one finding a row through `FindingList`, the finding pointer on its row; the pointed finding: who posted it, where, what, and its answer once it has one; then the node's feed. An answered finding keeps its row with a done mark and its answer under it. No title row.
- **`blocks::FindingsAt`** (new, `crates/render/src/blocks/findings_at.rs`): the run's findings as the pane shows them, at the one pointed at.
- **Its keys**: `j k` along the findings (the finding pointer becomes the person's); `tab` hands `j k` to the nodes and back, and the legend says which they move; the arrows through the pane.
- Targets, moved (`git mv`) from `crates/tui/goldens/targets/`: `findings` (three reviewers at once) and `findings-answered` (the pointed finding with its answer), and no other. Cuts: the design's run at `findings` and where the pointed finding is answered. The states between them, one, three and four findings posted, are asserted in `crates/ui/tests/scenes.rs`.

**Found in its turn:** render's `blocks::FindingsAt` is the findings' pane: its heading, the list with its pointer, and the pointed finding, its node in 21 cells, its place, its words wrapped, its answer under a done mark. The pane is titled by that heading, one part cut at the row's edge, as the targets set it on the pane's first row; short of rows the pointed finding gives way first, then the list. `j k` move the finding pointer, `tab` hands them to the nodes and back, and the legend says which they move (`lead.rs`). Landed and held whole at the four sizes: `findings` and `findings-answered`. The design's run posts no finding after one is answered, so the pointer staying as findings change is held as more are answered (seq 229), and posts at seq 191, 196 and 199. The walk's counts stand, its recordings holding no finding: the walk's own task brings the design's run into its home. The lead's tools hold: 19 screens and 76 frames of the family, and the inventory's cover.

**Fails first:** `crates/tui/tests/screens.rs`: `findings` differs from its target; `crates/ui/tests/scenes.rs`: as the design's run posts its first, third and fourth finding, the heading counts them answered of posted, each a row, and the finding pointer stays on the first with no answer; `tab` then `j` moves the node pointer and the legend says `j k node`; the finding pointer stays on an answered finding the person pointed at as more are posted.

**Acceptance criteria:**
- [x] The two targets equal at the four sizes; the inventory's row of element P7 is closed.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --test screens`, `cargo test -p yunta-ui --test scenes --test lead`, `cargo test -p yunta-render blocks`

**Dependencies:** Task 99

**Files likely touched:** `crates/render/src/blocks/findings_at.rs` (new), `crates/ui/src/stage/run/pane.rs` (new) in Task 98, `crates/ui/src/stage/run/lead.rs` (new) in Task 90, `crates/tui/goldens/{targets,screens}/`, `crates/testkit-ui/src/screens/run.rs` (new) in Task 90

**Estimated scope:** M

### Task 101: a node's own scene

**Description:**
- The row "a node pointed at while the run does not lead", by its standing: at work, as its work, board or findings scene; needing the person, what it waits on and its feed, the prompt one `a` or `f` away; ended, what it did (its findings, for a node that posted or answered them), its outcome's summary (`stands`, `took`, `tokens`) and its feed whole; sent back, as ended, under the nodes list's mark for it (E16); never ran, what it runs (`It has not run yet.`); skipped, why the mode leaves it out (E4).
- `page::Node`, which `yunta status <run> --node` prints, says the same fields: blessed where its goldens change, `crates/cli/tests/status_node_cmd.rs` where it reads them.
- The `Node` arm no longer reads the node's page (`ReadNodePage`), which Task 119 deletes.
- Targets, moved (`git mv`) from `crates/tui/goldens/targets/`: `node-working`, `node-needs-you`, `node-ended`, `node-never-ran`, `node-skipped`, `node-sent-back`. `node-needs-you` lands here alone; Task 117 holds that `esc` reaches the same frame. Cuts: the design's run at `work` with a node at work pointed by the person, with `brief` pointed (ended) and with `spec` pointed (never ran); the design's run at `scope` with `implement` pointed by `j` then `k` (needing the person); `yunta_testkit_ui::small_runs`' run in a mode that leaves a node out, and its run whose plan gate is answered `adjust`, which sends `plan` back.

**Found in its turn:** a node's own scene composes its pane (`pane/node.rs`) from render's `blocks::HowItStands`: `It has not run yet.`, the mode that leaves it out, `what it waits on` in the log's words, or `stands`, `took` and `tokens` with its summary, over a feed that lists every call of every attempt. The `Node` arm reads the node's page no more; `j k` still ask for it, the `Ended` arm drawing it. Landed and held whole at the four sizes: `node-working`, `node-ended`, `node-never-ran`, `node-skipped`, `node-sent-back` and `node-needs-you`, the last with its gist rows corrected to the design's gist by the lead's ruling, each row listed before and after for the person. Two rules the targets follow and the code did not: a word that ends on a pane's last cell is not broken (Surco's `row_end`, now as `wrap_unbroken`), and a node's own scene asks nothing, so its legend names no `r` or `x` (`Scene::names_the_runs_acts`). A node that never ran names the runner its run resolved for its role (`blocks::runner_of`, read by the title and the printed page). A node that ended having posted or answered findings stands as the findings' scene: no target holds it, a test does. The printed page says the same three fields and keeps what it printed: its tokens row says the design's words, then the split in and out it said before (`HowItStands::printed_fields`); its headline reads the standing the nodes' list reads (`watched::listed_as`), where a node a gate sent back read `failed` above a row that said `rerouted`. Goldens, render's `node/` alone: `stands` added, `took` and `tokens` before `kind`, the tokens' row as said, `never-ran` new with its runner. One test of `crates/engine/tests/process.rs` failed in a batch and passed alone and 120 times of 120 under stress: the open work on a command's group. The walk: 56,029 states (64,940 before, the node's scene no longer changing as its page arrives), 36 palettes, 47 helps, 4,718 screens at each size, its tests in 75 s and 80 s.

**Fails first:** `crates/tui/tests/screens.rs`: `node-never-ran` and `node-ended` differ from their targets; `crates/ui/tests/scenes.rs`: a finished node's feed lists every call of every attempt, each attempt named where it opens; the node sent back is pointed with its mark.

**Acceptance criteria:**
- [x] The six targets equal at the four sizes; the inventory's rows of elements E4 and E16 are closed.
- [x] `yunta status <run> --node` differs from the base only in the fields the inventory names.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --test screens`, `cargo test -p yunta-ui --test scenes`, `cargo test -p yunta --test status_node_cmd`
- [x] `YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine`, then the goldens' diff reviewed.

**Dependencies:** Task 100

**Files likely touched:** `crates/ui/src/stage/run/pane.rs` (new) in Task 98, `crates/render/src/page/node.rs`, `crates/tui/goldens/{targets,screens}/`, `crates/testkit-ui/src/screens/run.rs` (new) in Task 90, `crates/cli/tests/status_node_cmd.rs`

**Estimated scope:** M

### Group check: what a node does
- [x] The suite's groups green; every target of Task 98 to Task 101 equal at the four sizes; the CLI's captures differ from the base only where earlier tasks name.

**Found in its turn:** the gate's short steps pass; the suite's groups hold 5,248 tests, none failing, none aborted; the sixteen targets of Tasks 98 to 101 stand beside the goldens and equal their screens at the four sizes, and the lead's tools hold (19 screens and 76 frames of the family; the inventory's cover). The walk reaches 56,029 states, 36 palettes and 47 helps, and draws 4,718 screens at each size, its tests in 61 s and 59 s. Against the base's digests the goldens that differ are those of the last check and this group's own: the landed targets under `screens/`, Surco's `pane/feed-*`, `screen-meter` for Markdown and HTML, render's `node/never-ran`. The release binary weighs 31,410,192 bytes. Of 159 captured views 67 differ from the base and four from the last check: the printed page of a node at its three sizes, by the fields Task 101 names (`stands`, the order of `took` and `tokens`, the tokens' words before their split), and the bare field, whose ages the last check had captured apart.

Task 102 to Task 110 each add one family to the pane's composer and to `screens/run.rs`, but Task 104, which changes a type under the pull request's family; they run one at a time.

### Task 102: the standstill, in its kinds

**Description:**
- `Scene::Standstill` for the rows: a pause whose escalation carries no menu (the pause in the log's words; that no option answers it here; `r`, `x`); `NoRoom` (what is free against what is needed; `u` opens upkeep over the run, which collects once confirmed; then `r`); `UncertainOrphans` and `Blocked` (what is left half done and what to check before going on; `r`); `Cancelled`, `CancelledAfterCrash`, `GateAborted`, `Recorded` (where it stopped and why, in the log's words; `r`, `x`); stalled (where it stopped: no process drives it; `r`, `x`).
- **`blocks::WhereItStopped`** (new, `crates/render/src/blocks/where_it_stopped.rs`): a standstill's facts, from `pause::said` and its kin. Not `blocks::stop`, why a task stopped.
- The run's `r` resumes and `x` closes once confirmed where the scene offers them, and `u` opens upkeep over the run on no room.
- Targets, moved (`git mv`) from `crates/tui/goldens/targets/`: `standstill` (the pause no option answers here), `no-room`, `orphans` (and blocked), `stopped` (cancelled, after a crash, a gate aborted, a sentence recorded), `stalled`. Cuts: `yunta_testkit_ui::small_runs`' runs for each, the pause with no menu among them.

**Found in its turn:** a standstill says its kind (`Standstill::{Unanswered, NoRoom, HalfDone, Stopped, NoProcess}`) and composes its pane (`pane/standstill.rs`) from render's `blocks::WhereItStopped`: what it waits on, then what the disk has free and needs, what was left half done with its branch, or where it stopped and why, its labels in a column two cells past the widest. `Scene::named_acts` decides the acts a legend names: `x` stays out on `no-room` and `orphans`; `u` opens upkeep over a run with no room; `r` reads `continue it where it stopped`. A region with no feed draws the first abridged form that fits (Surco's `pane/fitted.rs`), so `what it waits on` gives way whole at 60×14. Landed and held whole at the four sizes: `standstill`, `no-room`, `orphans`, `stopped`, `stalled`. The target `standstill` draws `◌ approve` where the code drew `◆`: the list marks by standing, and a node the run waits at is marked `◆` only once its log started it. `docs/guide.md` says what `u` does on a run with no room; under no isolation the `branch` row shows the base branch alone.

**Fails first:** `crates/tui/tests/screens.rs`: each standstill's target differs; `crates/ui/tests/scenes.rs`: on no room `u` opens upkeep over the run and `esc` comes back to it.

**Acceptance criteria:**
- [x] The five targets equal at the four sizes; the legend of each offers exactly the keys its row names.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --test screens`, `cargo test -p yunta-ui --test scenes --test upkeep_schema`, `cargo test -p yunta-render blocks`

**Dependencies:** Group check: what a node does

**Files likely touched:** `crates/render/src/blocks/where_it_stopped.rs` (new), `crates/ui/src/stage/run/acts.rs`, `crates/ui/src/stage/run/pane.rs` (new) in Task 98, `crates/tui/goldens/{targets,screens}/`, `crates/testkit-ui/src/screens/run.rs` (new) in Task 90

**Estimated scope:** M

### Task 103: work that stands still: a run that wakes alone, an answer awaiting its resume, a pause not yet read

**Description:**
- `Scene::Work`, standing still, for the rows: `AwaitingReset` (as work, with when it goes on alone; no tint, no spinner); an answer recorded with its resume not yet on the log (the answer as recorded; that it goes on when an engine takes it; `r` where no engine does); a pause on the log whose ask is not read yet (as work, with no card yet).
- **A pause whose ask could not be read** (Phase 1's `RunNews::AwaitedUnread { why }`, its why kept in `RunInView`): as work, with that why shown in place of the card, the spec's "unreadable in place", and no option; the next reading of the pause stands its card there.
- Targets, moved (`git mv`) from `crates/tui/goldens/targets/`: `wakes-alone`, `still-answered`, `still-unread`, `card-unread`. Cuts: `yunta_testkit_ui::small_runs`' run that wakes alone; the design's run between the scope's answer and its wake; the design's run with a pause on the log and its ask not yet read; `small_runs`' gate whose shown document fails today's shape.

**Found in its turn:** render's `watched::still` says the line of work that stands still: `it goes on alone at 01:00 UTC`, and `✓ <answer> · the run goes on when an engine takes it up`, the answer from replay's `answer_awaiting_its_resume()`. Where no engine drives the run, the work, board and findings scenes name no act but the resume an answer awaits. An ask that could not be read stands as a card with no options at the pane's foot, ending on its reason, in the feed's place; its decision counts as in sight, so no attention row stands, and `a` answers nothing there; before a decision is read `a` goes nowhere, as the spec says. A page that could not be read is said in the page's place and not as a note. Landed and held whole at the four sizes: `wakes-alone`, `still-answered`, `still-unread` (its gist row corrected to the design's gist by the lead's ruling, listed before and after for the person) and `card-unread`, three slips of whose writer the lead ruled corrected in the target: the ribbon names `approve-plan` as `approve`, by the one rule on a stretch's name; its card asks the message's question, `◆ Approve?`, as `ship` and `card` do; at 60×14 its reason stands whole, wrapped, the conventions cutting a reason only to give way. The walk: 55,879 states (56,029 before, by `a`'s rule), 36 palettes, 47 helps, 4,718 screens at each size, its tests in 78 s and 67 s.

**Fails first:** `crates/ui/tests/scenes.rs`, a behaviour a golden cannot hold: a run that wakes alone draws no spinner frame on any node, on a moving look, through three events; a pause whose `awaited` is not read yet shows work and no card; a pause whose ask could not be read shows its why where the card stands, and no key answers it; an answer awaiting its resume offers `r` only where no engine drives the run.

**Acceptance criteria:**
- [x] The four targets equal at the four sizes.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --test screens`, `cargo test -p yunta-ui --test scenes`

**Dependencies:** Task 102

**Files likely touched:** `crates/ui/src/stage/run/pane.rs` (new) in Task 98, `crates/ui/src/stage/run/scene.rs` (new) in Task 91, `crates/render/src/watched/mod.rs`, `crates/tui/goldens/{targets,screens}/`, `crates/testkit-ui/src/screens/run.rs` (new) in Task 90

**Estimated scope:** S

### Task 104: what a surface is asked to show, a page or a link

**Description:**
- **`Address`** (new, `crates/ui/src/request.rs`), `Page(PathBuf)` or `Link(String)`: what the surface is asked to show. `Want::Show` (`crates/ui/src/stage.rs:119`), `Request::ShowPage` (`request.rs:18`) and `Input::Unshown` (`input.rs:30`) take it in place of a `PathBuf`, since a URL held as a path is a type that admits a lie. Not `NextStep::Open`, the command that shows a run's page.
- `yunta-tui`'s `show_page` (`crates/tui/src/application.rs:56`) takes `&Address`, and the CLI hands it the system's opener (`crates/cli/src/browser.rs`), which opens a page's path or a link alike; `yunta open` hands it a page.
- The evidence stage's pages are `Address::Page` (`crates/ui/src/stage/evidence.rs:155,217`), and the walk hands them back unshown as before (`crates/testkit-ui/src/walk.rs:358`).
- Knowledge: `request.rs`'s `///` on `Address`; `browser.rs`'s `//!` says it opens a link as it opens a page.

**Found in its turn:** `Address { Page(PathBuf), Link(String) }` in `crates/ui/src/request.rs` is what a surface is asked to show; `Want::Show`, `Request::ShowPage`, `Input::Unshown` and `Stage::unshown` carry it, `yunta-tui`'s `show_page` takes it, and the CLI's `browser::open` hands a page's path or a link to the system's opener alike. The evidence's stage and `yunta open` hand a page; the walk hands pages back unshown as before. Its guard passed before and after (`evidence`, the application's own tests, `open_cmd`), the acceptance's search finds nothing, and no golden was touched. `crates/cli/src/commands/open.rs` changed too, the compiler naming it.

**Fails first:** none: a type in place of another, with no new behaviour until the pull request shows a link (Task 105). Its guard: `crates/ui/tests/evidence.rs` and `crates/tui/src/application/tests.rs` pass with `Address::Page` where they named a path, and `crates/cli/tests/open_cmd.rs` passes unchanged.

**Acceptance criteria:**
- [x] `git grep -n "ShowPage(PathBuf)\|Unshown(PathBuf)\|Show(std::path::PathBuf)" crates` finds nothing.
- [x] Every golden and capture equals the last group check's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test evidence --test walk`, `cargo test -p yunta-tui --lib application`, `cargo test -p yunta --test open_cmd`

**Dependencies:** Task 103

**Files likely touched:** `crates/ui/src/{request.rs,input.rs,stage.rs,state.rs,performer.rs,stage/evidence.rs}`, `crates/tui/src/{application.rs,application/tests.rs}`, `crates/cli/src/{browser.rs,application.rs}`, `crates/testkit-ui/src/walk.rs`, `crates/ui/tests/evidence.rs` (a mechanical change of type: every caller of `ShowPage`, `Unshown` and `Want::Show`, the compiler names them)

**Estimated scope:** M

### Task 105: the pull request

**Description:**
- `Scene::PullRequest` for `ExternalGate`, `Menu::Published` and `PullRequestClosed`: the link and how it stands, read from the forge once when the scene opens; `o` opens the pull request, as `Address::Link` (Task 104), and the run's page is in the palette here.
- **`OpenedRun::pull_request`** (new, `crates/api/src/run/reading.rs`) polls the gate's pull request with the run's forge and hands back `PullRequestPolled` (new): its link and `ReviewOutcome`. **`Operation::ReadPullRequest { run }`** and `Outcome::PullRequest` (new) carry it.
- Target, moved (`git mv`) from `crates/tui/goldens/targets/`: `pull-request`. Cut: `yunta_testkit_ui::small_runs`' run with a pull request, its forge a fake answering `Pending`.

**Found in its turn:** the pull request's scene reads the forge once for each pause, as the run's news, a key or a return to the run opens it, never on a draw (`stage/run/pull_request.rs`): `OpenedRun::pull_request` hands `PullRequestPolled`, the pull request with what the forge answered or why it gave none, the whole poll, a review deciding only the commit it reviewed; `Operation::ReadPullRequest` and `Outcome::PullRequest` carry it. The pane is render's `blocks::PullRequestReview`; the legend names `o` alone, which asks the surface for `Address::Link` with the log's link; the run's page stays in the palette, an act's key being optional now. A link that could not be opened is noted as a failed act with the link (`page::unopened`, which a page's own note uses too). The forge needed a seam for tests: `Workspace.forges`, every interface handing the declared forge. Landed and held whole at the four sizes: `pull-request`. Its row at 60×14 cut a link outside its number, which the one rule on a cut refused, having been made wider than the conventions at Task 73 (no cut inside a word that holds a digit): the rule is narrowed to their words, never right after a digit and never inside a number, a number being digits with the marks that join them, so a link, a path or a handle is cut anywhere outside its numbers and a date, a time, a decimal and a duration still go whole. One golden moved by it, Surco's `every-block.unicode-color-100` row 14, back to what the branch's base held; no landed target and no golden of render, the engine or the CLI moved. `yunta doctor` said of a forge whose client could not be built that its token was not set, which was false: it says the token is set and what reaching the forge failed on (`ForgeAnswer::Unready`). The engine reads a pull request's reference through core's one reader. The sentences on how a pull request stands, each true of the forge's answer, are the lead's to show the person. `crates/ui/src/stage.rs` stands at 496 lines. The walk's counts stand; it reaches no pull request.

**Fails first:** `crates/ui/tests/scenes.rs`, a behaviour a golden cannot hold: the forge is read once when the scene opens, and not again on a piece of news of the same pause; `o` asks the surface to show `Address::Link` with the pull request's link.

**Acceptance criteria:**
- [x] The target equals at the four sizes; the walk records the reading and its outcome.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api pull_request`, `cargo test -p yunta-ui --test scenes --test evidence`, `cargo test -p yunta-tui --test screens`

**Dependencies:** Task 104

**Files likely touched:** `crates/api/src/run/reading.rs`, `crates/ui/src/{operation.rs,outcome.rs}`, `crates/ui/src/performer/run.rs`, `crates/ui/src/stage/run/{acts.rs,pane.rs}` (`pane.rs` new in Task 98), `crates/tui/goldens/{targets,screens}/`

**Estimated scope:** M

### Task 106: a child

**Description:**
- The row `ChildPaused`: while the run leads, the child becomes the run read whole, shown in its place (`State::followed()` names it); the trail leads up to its parent; with no prompt standing, `esc` goes up to the parent, whose node pointer becomes the person's on the node that bore the child; a prompt of the child's is put aside first, as any prompt is.
- Target, moved (`git mv`) from `crates/tui/goldens/targets/`: `child`. Cut: `yunta_testkit_ui::small_runs`' run whose child paused.

**Found in its turn:** while a run leads and is parked on a child, its stage moves onto the child (`crates/ui/src/stage/run/child.rs`: `Parent`, `onto_its_child`, `up_to_its_parent`, `up_where_a_parent_moved`), so `State::followed()` names the child under every stage opened over it; the trail reads the parent, the node that bore the child, the child, then its node; `esc` goes up to the parent, its node pointer the person's on that node, and `f` shows the child again; where the field shows the parent no longer parked on that child, the parent takes the screen back. Which child a run is parked on has one owner, replay's `WaitingOn::child_run`. `docs/guide.md` says how a child is shown. The target `child` does not land here: its pane and its legend draw the child's card in place (`j k choose · enter answer · esc leave it waiting`), which the task that lands a card in place draws, and its attention row names another run where every card's target leaves that row blank, a slip of its writer the lead ruled corrected when it lands; it is held everywhere but those rows, and the list's Task 113 carries it. The walk's home holds no child: the walk's own task brings the small runs into it and holds that `followed()` names the child. The walk's check follows the run the trail names last.

**Fails first:** `crates/ui/tests/scenes.rs`, a behaviour a golden cannot hold: `esc` on the child goes up to its parent, and `followed()` names the parent then; the trail reads parent, node, child.

**Acceptance criteria:**
- [x] The target equals at the four sizes; the walk holds that `followed()` names the child under a stage opened over it.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test scenes --test walk`, `cargo test -p yunta-tui --test screens`

**Dependencies:** Task 105

**Files likely touched:** `crates/ui/src/stage/run.rs`, `crates/ui/src/state.rs`, `crates/tui/goldens/{targets,screens}/`, `crates/testkit-ui/src/screens/run.rs` (new) in Task 90, `crates/ui/tests/scenes.rs` (new) in Task 91

**Estimated scope:** M

### Task 107: a run asked elsewhere

**Description:**
- `Scene::AskedElsewhere` for a question open at another terminal: the node it asks about, since when, and the process that asks; nothing here answers it. It is read only.
- Target, moved (`git mv`) from `crates/tui/goldens/targets/`: `asked-elsewhere`. Cut: `yunta_testkit_ui::small_runs`' run with a registry whose engine asks at its terminal.

**Found in its turn:** a run asked at its own terminal composes its pane (`pane/asked_elsewhere.rs`) from render's `blocks::AskedAtItsTerminal`: the node it asks about, the process that asks, since when, and that nothing here answers it; `NOTHING_HERE_ANSWERS` and `answer it there` are written once in render. No key on the scene answers it, `a`, `enter`, a number and the palette's rows among them, and the run's log is the same after each: that held before this task too. Landed and held whole at the four sizes: `asked-elsewhere`, its title row corrected by the lead's ruling, a slip of its writer: it drew `ship · gate` with nothing at the far end, where the conventions give a gate's title `assignee {…}`, the run's gate declares `assignee: lead`, and seven other gate targets draw it; the row is listed before and after for the person. The target holds `{pid}` for the test's own process; at 60×14 the gist row's cut depends on how many digits the pid has, so the test holds that row to the conventions' cut for the pid in hand.

**Fails first:** `crates/ui/tests/scenes.rs`, a behaviour a golden cannot hold: no key on the scene answers it, `a`, `enter`, a number and the palette's rows among them, and the run's log is unchanged after each.

**Acceptance criteria:**
- [x] The target equals at the four sizes.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test scenes`, `cargo test -p yunta-tui --test screens`

**Dependencies:** Task 106

**Files likely touched:** `crates/ui/src/stage/run/pane.rs` (new) in Task 98, `crates/tui/goldens/{targets,screens}/`, `crates/testkit-ui/src/screens/run.rs` (new) in Task 90, `crates/ui/tests/scenes.rs` (new) in Task 91

**Estimated scope:** S

### Task 108: the receipt

**Description:**
- `Scene::Receipt` for a run finished or reported: the checklist (criteria, baseline, scope, review, event chain), `cost`, `changed`, `pull request`; under a report, the findings that block; the harvested field under it (`Horizon` in the done tone); no title row.
- **A region that ends with a horizon** (`crates/surco/src/surface/terminal/pane.rs`) lays out its blocks first, then the horizon in the rows they leave; given fewer rows than the horizon's least, it draws none and the blocks stand where they stood. The first request and the setup reuse this layout in Phase 4.
- The receipt comes from Phase 1's receipt of the run in view's ledgers; the event chain is verified once, by `Operation::Evidence(Verify)`, when the scene first shows; nothing is written.
- Targets, moved (`git mv`) from `crates/tui/goldens/targets/`: `receipt`, `reported`. Cuts: the design's run at its end; `yunta_testkit_ui::small_runs`' finished run that reports blocking findings.
- Knowledge: `pane.rs`'s `//!` says how a region that ends with a horizon is laid out.

**Found in its turn:** a pane whose document ends with a horizon draws its blocks, a blank row, then the field in every row left; with too few rows the field goes, and where the blocks still do not fit their blank rows go before they scroll. The receipt's scene composes its pane: the checks, `cost`, `changed` and `pull request`, under a report the findings that block (`blocks::FindingsThatBlock`), then the field; its trail names `receipt` and its gist is the receipt's. The event chain is verified once for a run and nothing is written: no `receipt.md` stands after it. A reading asked once as a scene opens, and asked again where a stage opened over the run let it go before an answer, has one owner, `crates/ui/src/stage/run/read_once.rs` (`ReadOnce<K, T>`), which the receipt's verification and the pull request's poll both use: the poll had the same defect and never asked again. One rule in replay, `RunState::findings_that_block`, feeds the frame's count, the receipt's count and the pane's list. Landed and held whole at the four sizes: `receipt` and `reported`, row 11 of each at 60×14 corrected by the lead's ruling, a slip of their writer: the row that counts hidden lines stood two cells in, where it is the region's own row at its first cell, as `evidence-receipt`, `graph`, `setup-audit` and `setup-confirm` set it. Surco's goldens `pane/receipt.{unicode,ascii}-{80,60}` are new, written by hand, the 80-column one from the target's rows. `docs/guide.md` says nothing yet of the receipt's screen.

**Fails first:** `crates/surco/src/surface/terminal/pane/tests.rs`: three blocks and a horizon given more rows than the blocks take draw the blocks, then the horizon in every row left; given fewer than the horizon's least, the blocks alone. `crates/ui/tests/scenes.rs`: showing the receipt asks one verification and no receipt's writing, and no `receipt.md` stands in the run's directory after it; `crates/tui/tests/screens.rs`: `receipt` differs from its target.

**Acceptance criteria:**
- [x] The two targets equal at the four sizes; the inventory's row of element P9 is closed.
- [x] Goldens of a pane ending with a horizon at 60 and 80 cells, Unicode and ASCII, reviewed in `crates/surco/goldens/pane/`.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco pane`, `cargo test -p yunta-ui --test scenes --test evidence`, `cargo test -p yunta-tui --test screens`

**Dependencies:** Task 107

**Files likely touched:** `crates/surco/src/surface/terminal/{pane.rs,pane/tests.rs}`, `crates/surco/goldens/pane/`, `crates/ui/src/stage/run/pane.rs` (new) in Task 98, `crates/ui/src/stage/run.rs`, `crates/tui/goldens/{targets,screens}/`, `crates/testkit-ui/src/screens/run.rs` (new) in Task 90

**Estimated scope:** M

### Task 109: the ends: failed, promoted, broken

**Description:**
- `Scene::Ended` for a run failed (the failure's evidence, `blocks::FailureDetail`), promoted (its successor), broken (where the log stops making sense).
- **The successor takes the screen while the run leads**: `State::followed()` names the successor then, and the run's own ended scene otherwise, where `f` gives the lead back and the successor follows; Phase 2's unconditional move to the successor narrows to this.
- Targets, moved (`git mv`) from `crates/tui/goldens/targets/`: `failed`, `promoted`, `broken`. Cuts: `yunta_testkit_ui::small_runs`' runs that failed, were promoted and broke.

**Found in its turn:** the ended scene composes its pane from render's `HowItFailed`, `ItsSuccessor` and `WhereItsLogBreaks`. A successor takes the screen only while the run leads: with a pointer the person's, the promotion leaves the screen on the run, which keeps its successor beside itself, `State::followed()` naming the run, and `f` hands the screen over; `promoted` with the person leading is asserted now. The tally counts by the field's groups, which set a broken run among those that need the person. Landed and held whole at the four sizes: `failed`, `broken` and `promoted`, rows 4 and 5 of the last corrected by the lead's ruling, a slip of its writer: the ribbon named `fix-lint` whole, where the one rule on a stretch's name gives `fix` when no stretch before it is named so. The cut of `promoted` stands at seq 8 and goes on to 9 after `j k`, so the person holds the screen as the successor comes. Two sentences no target gives are the lead's to show the person: a successor that never came (`none came in the time one takes to be created`) and a broken log that names no node (`it reads up to seq {n}`). The walk: 56,144 states (55,879 before, a finished run's screen standing once with its verification under way and once answered), 36 palettes, 47 helps, 4,702 screens at each size, its tests in 61 s and 59 s.

**Fails first:** `crates/ui/tests/scenes.rs`, a behaviour a golden cannot hold: with the node pointer the person's, the promotion leaves the screen on the run and `followed()` on it; `f` then names the successor.

**Acceptance criteria:**
- [x] The three targets equal at the four sizes.
- [x] `crates/cli/tests/application/watching.rs`' promotion followed onto its successor passes unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test scenes --test walk`, `cargo test -p yunta-tui --test screens`, `cargo test -p yunta --test application watching`

**Dependencies:** Task 108

**Files likely touched:** `crates/ui/src/stage/run.rs`, `crates/ui/src/stage/run/pane.rs` (new) in Task 98, `crates/tui/goldens/{targets,screens}/`, `crates/testkit-ui/src/screens/run.rs` (new) in Task 90, `crates/ui/tests/scenes.rs` (new) in Task 91

**Estimated scope:** M

### Task 110: the run's whole chronicle, from the palette

**Description:**
- **`ChronicleStage`** (new, `crates/ui/src/stage/chronicle.rs`), `Stage::Chronicle`: the run's whole chronicle as a feed of its own over the run, every moment of every node from `RunInView`, its newest in sight, through Task 97's window; the arrows scroll it, `esc` goes back.
- `Stage::opened_by(NextStep::Follow)` opens it, the command that prints the same (`yunta status <run> --follow`); the run's palette offers it; `State::opened_on` opens a run under a step by `NextStep::Status`.
- `crates/cli/tests/interface/run.rs` holds the chronicle's rows to the piped follower's lines, as it held the base's chronicle under the page.
- Target, moved (`git mv`) from `crates/tui/goldens/targets/`: `chronicle`. Cut: the design's run at `findings`, the chronicle opened from the palette.

**Found in its turn:** `ChronicleStage` (`crates/ui/src/stage/chronicle.rs`, `Stage::Chronicle`) is the run's whole chronicle as a feed of its own over the run: every moment of every node, opened at its end and kept there as it grows, scrolled by the arrows, `End` going with the end again, `esc` going back, `n` a new run, in the run's frame. `NextStep::Follow` opens it; the field's `enter`, `a`, a run just started and `State::opened_on` open a run's screen by `NextStep::status`. The run's palette offers `the run's whole chronicle`, an act carrying its own words (`Act::saying`). Render gains `chronicle::{whole, whole_heading}` and the stage's words; Surco's `Feed.subjects` lays out the subjects' column where the rows in sight are known; `Scroll::window` and its margin serve both feeds. `stage.rs` is split into `act.rs` and `want.rs`. Landed and held whole at the four sizes: `chronicle`. Its cut stands at seq 104 with `implement` at work, as the kit's table has it, where the task's text said at the findings; it types `chronicle` into the palette, so the palette narrows by what a row does as well as by its command (`Picker::found_by_labels_too`). `a` acts on the chronicle, the numbers and the graph as their attention row names it, by the one rule the run's stage follows (`FollowedRuns::attended`, `Want::BackToItsDecision`, `Want::in_place`): the list had left it to the task that gives `a` to every stage, and a row may not name a key that does nothing. `q` still leaves the application from these three pages, and no legend of theirs names it. The walk walks on from no palette row, so the chronicle adds no state.

**Fails first:** `crates/ui/tests/chronicle.rs` (new): the palette's chronicle row opens it with the newest moment on its last row; ten more moments keep it at its end; `End` after scrolling back returns there. It does not compile at the base.

**Acceptance criteria:**
- [x] The target equals at the four sizes; the parity test passes against the piped follower.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test chronicle --test state`, `cargo test -p yunta --test interface run`, `cargo test -p yunta-tui --test screens`

**Dependencies:** Task 109

**Files likely touched:** `crates/ui/src/stage/chronicle.rs` (new), `crates/ui/src/{stage.rs,state.rs}`, `crates/ui/tests/chronicle.rs` (new), `crates/cli/tests/interface/run.rs`, `crates/tui/goldens/{targets,screens}/`

**Estimated scope:** M

### Group check: where a run stands
- [x] The suite's groups green; every target of Task 102 to Task 110 equal; the walk's counts and time against the last group check's.

**Found in its turn:** run for Tasks 102 to 109, the chronicle's target being the next group's. The gate's short steps pass; the suite's groups hold 5,290 tests, none failing, none aborted; thirty-two targets stand beside the goldens and equal their screens at the four sizes, and the lead's tools hold (`run` 19 screens, `stands` 13, `decisions` 22; the inventory's cover). The walk reaches 56,144 states, 36 palettes and 47 helps, and draws 4,702 screens at each size, its tests in 78 s and 90 s beside another session's builds. Against the base's digests the goldens that differ are those of the last check, the targets that landed and Surco's `pane/receipt`. The release binary weighs 31,476,368 bytes. Of 159 captured views 67 differ from the base and none from the last check. The capture of `review` at a terminal sent its keys on a clock and, beside other builds, pressed `q` before the screen it was meant for stood: it waits for each screen now, and three captures in a row are equal.

### Task 111: a run's numbers and its graph

**Description:**
- The stats and graph stages over a run as `design/work+s.wide.txt` and `work+g.wide.txt` draw them: the sentence under the ribbon, each labelled row, where its time went with its bar and what each mark means, who spent it with a bar a role; the graph's headline and its nodes. Both read the run kept, so they move with it (Phase 2).
- Their frame through `FollowedRuns::framing` (Task 92), the run's decision out of sight under them, and their legends (`esc back · n new run`, the graph's scroll keys) as their targets say; `n` starts a new run there.
- Targets, moved (`git mv`) from `crates/tui/goldens/targets/`: `stats`, `graph`. Cut: the design's run at `work`, with `s` and with `g` pressed.

**Found in its turn:** render's `StatsTold { OnScreen, Printed(Depth) }` says what a run's numbers tell on the screen, leaving the cost, the answers, what is unread, the checks a red criterion decided, the rails and the nodes to the printed page. The numbers and the graph stand in the run's frame (`over_the_run.rs`), one legend for the pages in that frame, `n` starting a new run, `f` gone; their trails read `Q78GRR stats` and `fragua graph`. The conventions give every blank row of a page way at 60×14: `Terminal::pane_short_of_rows`, and the layout marks a screen with nothing across it as short of rows on the least terminal. Landed and held whole at the four sizes: `stats` and `graph`, the diagram rows of the last redrawn by the lead's ruling from the workflow's own diagram (rows 5 to 34 at 120 columns and in ASCII, 5 to 22 at 80, 4 to 12 at 60), every other row as approved: the chain stands eighteen columns to the right, centred over the fan-out from `implement`; the count row follows (`… 34 more lines`, `… 46` at 80); at 60 columns the diagram, 64 cells wide, is written as its chains and no count row stands. The rows before and after are kept for the person. The interface's walk could no longer scroll a run's numbers, no run of its home spending enough for them to pass its screen: the entry is out of `MUST_SCROLL`, `crates/ui/tests/scroll.rs` holding the numbers to scroll on a run that spends enough, and the walk's own task holds it again with the design's run in the home. The walk: 55,478 states (56,144 before: the frame over the two pages adds 340, `f` gone takes 1,006), 36 palettes, 45 helps, 4,560 screens at each size, its tests in 59 s each.

**Fails first:** `crates/tui/tests/screens.rs`: `stats` and `graph` differ from their targets; `crates/ui/tests/stats_graph.rs`: stats over a run waiting on the person names its decision in the attention row with `a go there`.

**Acceptance criteria:**
- [x] Both targets equal at the four sizes; the inventory's rows for a run's numbers and graph screens are closed.
- [x] `crates/ui/tests/stats_graph.rs` passes with its words rewritten to render's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --test screens`, `cargo test -p yunta-ui --test stats_graph`

**Dependencies:** Group check: where a run stands

**Files likely touched:** `crates/ui/src/stage/{stats.rs,graph.rs}`, `crates/tui/goldens/{targets,screens}/`, `crates/testkit-ui/src/screens/run.rs` (new) in Task 90, `crates/ui/tests/stats_graph.rs`

**Estimated scope:** S

### Task 112: the palette and the help

**Description:**
- **The palette** over a run (`crates/ui/src/dialog/palette.rs`): every act the code offers there, the ones the design does not name among them, each row what it does, its key and the command that does the same; then what every stage offers (`Stage::always`): a new run, the runs of this repository, the workflows, doctor, upkeep, as the targets order them; `type to filter` until something is typed. `Alternative` carries the key.
- **The help** (`dialog/help.rs`): render's tagline and the keys every screen answers to, then the keys of the screen under it the legend leaves out (`c`, `e`, `v`, `o` on a run).
- Their legends stand in the screen's legend row while they are open.
- Targets, moved (`git mv`) from `crates/tui/goldens/targets/`: `palette`, `palette-narrowed` (typing narrows it), `help`, `receipt-palette` (the palette over the receipt). The palette over a prompt put aside (`palette-aside`) lands with the putting aside, in Task 117. Cuts: the design's run at `work` with `/`, with `/` and words typed, and with `?` pressed; the design's run at its end with `/` pressed.

**Found in its turn:** the palette over a run names each row by what it does, with its key (`Alternative.key`) and the command that does the same muted; words typed narrow by what a row does, the narrowing by command gone as the targets ask; no key stands inside the box, its legend standing in the screen's legend row. A run's rows stand in the targets' order (`s`, `g`, `o`, the chronicle, `n`), then what every stage offers (`Stage::always`): the runs of this repository, the workflows, doctor, `yunta gc` and the schema; a new run is no row of every stage, `n` opening one only where a stage takes it. Help over a run is `help_over_a_run`: the tagline, the keys every screen answers to, then every keyed act of the screen under it, `c`, `e`, `v` and `o` among them. A box stays above the meter's row (`BOX_LEAVES`) and keeps two cells past its widest row, help measuring 70 as its targets draw it; Surco's `Terminal::boxed` reads `Terminal::wide()` (`WIDE_FROM`, the legend's far keys folded into it), so the three columns show from 100 columns. Landed and held whole at the four sizes: `palette`, `palette-narrowed`, `help` and `receipt-palette`, one cell of the last at 60×14 corrected by the lead's ruling: left of the box the screen under it shows the `…` of the receipt's count row, which the lead had ruled to the region's first cell. Verification on a finished or reported run is decided where a run's acts are, in replay's `RunFrame::actions` (`Receipt`, then `Verify`; `Effect::Verifies`), where the stage had added it for itself: `yunta status`, the closing page and the run's page for a browser gain the next step `yunta verify <run>   verify its evidence`, one line in 26 of render's goldens. On the findings help says of `tab`: `hand j k to the nodes and back`. The walk checks the attention row and the meter under a box; it reaches 55,578 states (55,478 before, `v` opening the verification on the home's finished runs), 36 palettes, 45 helps, 4,588 screens at each size, its tests in about 75 to 90 s beside other builds.

**Fails first:** `crates/tui/tests/screens.rs`: the four targets differ; `crates/ui/tests/palette.rs`: a run's palette row for stats names `s` and `yunta stats <run>`; words typed narrow the rows and stand where `type to filter` stood; help over a run lists `c` with what it does.

**Acceptance criteria:**
- [x] The four targets equal at the four sizes; the inventory's row of element C7 is closed.
- [x] Every row of a palette the walk meets does what its key does (`crates/ui/tests/walk/dialogs.rs`).

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --test screens`, `cargo test -p yunta-ui --test palette --test dialogs --test walk`

**Dependencies:** Task 111

**Files likely touched:** `crates/ui/src/dialog/{palette.rs,help.rs}`, `crates/ui/src/prompt/picker.rs`, `crates/ui/src/stage.rs`, `crates/tui/goldens/{targets,screens}/`, `crates/ui/tests/palette.rs`

**Estimated scope:** M

### Group check: over a run
- [x] The suite's groups green; every target of Task 111 and Task 112 equal; the palettes and helps the walk opens counted against the base's.

**Found in its turn:** the gate's short steps pass; the suite's groups hold 5,305 tests, none failing, none aborted; thirty-nine targets stand beside the goldens and equal their screens at the four sizes, the chronicle's among them, and the lead's tools hold the four families and the inventory's cover. The walk reaches 55,578 states, takes 36 palettes row by row and opens 45 helps (41 and 72 at the base, whose legends named each node), and draws 4,588 screens at each size, its tests in 129 s and 101 s beside another session's builds at a load of 27 and 37. Against the base's digests the goldens that differ are those of the last check and the targets that landed, two of which stand at keys the base's deleted frames held. The release binary weighs 31,509,424 bytes. Of 159 captured views 67 differ from the base and ten from the last check, each by one line: the next step `yunta verify <run>   verify its evidence` on a finished run's pages and in a program's JSON of them.

### Task 113: the card in place, on a gate that shows no plan

**Description:**
- **`InPlace`** (new, `crates/ui/src/stage/run/in_place.rs`): the prompt a run's screen holds, a decision (`Deciding`) or a round (`Round`), with the pause it belongs to as `awaited` names it. Not the decision stage, which Task 119 deletes; not a dialog. The run's stage builds it once for its pause.
- `Scene::Card` with **`Grounds::Shown`**: what the gate shows, as the gate's page draws it above a card; the change by file with its bars; the findings and criteria in a line (P8); the card at the pane's foot under it, by itself while the run leads.
- **A card's keys**: `j k` and `tab` choose; a number picks (`DecisionCard` gains it, in the CLI's card too); `enter` answers, the choice on the run's log through `Operation::Decide`; `n` adds a note; `r` reads the plan it offers; the arrows and the wheel move what stands above it and never its choice. Evidence above it gives way from its top and never overlaps it; its options never give way.
- An answer recorded is said in a note, in Task 86's sentence for that option.
- Targets, moved (`git mv`) from `crates/tui/goldens/targets/`: `ship`, `card-recorded` (the note the ship's answer leaves), `card` (a gate with no plan, answered at the console). Cuts: the design's run at `ship` and after its answer; `yunta_testkit_ui::small_runs`' console gate.
- **`child`** (from Task 106): its target draws the child's card in place, with the card's legend, and could not land before a card stands in place. It lands here, moved with this task's targets, its attention row corrected first by the lead's ruling: the row is blank where the run's own decision stands in sight, as `ship`, `card`, `scope`, `failure` and `budget` draw it, and the target names another run there; the row is reported before and after for the person.

**Found in its turn:** `InPlace` (`crates/ui/src/stage/run/in_place.rs`) is the prompt a run's screen holds for one pause: the pause, a `Deciding` with its card in front, the place of what stands above the card, which opens at its end, and how its answer stands. `Scene::Card` with `Grounds::Shown` composes its pane (`pane/card.rs`): the title; for a child, the run that bore it; render's `Gate::OverTheCardInPlace`, the documents the gate shows but a plan and the run's findings, the change by file, then the line of findings and criteria; the card at the pane's foot, with no about and no keys of its own. On the card `j k`, `tab` and a number choose; `enter` records the choice on the run's log through `Operation::Decide`, once, and the note says the sentence recorded for that option (`Notice::Recorded`); `n` adds a note; `r` opens the plan the card offers; the arrows and the page keys scroll what stands above and leave the choice and `leads()` as they were. Surco sets a card at its pane's foot (`pane/card_at_the_foot.rs`, `Card::least`): what stands above takes every row the options leave, gives way from its top and scrolls; a line typed on a card keeps its box where rows are short; `pane/held-card` blessed, the sentence giving way before the options. The shared card takes a number (`DecisionCard`, `ListKey`): on the CLI's card `2` sets the pointer on the second option, where a number did nothing; no capture of the CLI moves, and the CLI keeps its own legend at a terminal. Landed and held whole at the four sizes: `ship`, `card-recorded`, `card` and `child`. Three rows were slips the lead ruled corrected in the targets, each listed before and after for the person: `child`'s attention row, blank where its card stands in sight; `child`'s legend, which left out `n add a note` where the seven other card targets name it; `card`'s row of the change, set as `yunta status` and its own dump draw the one component. On `card`'s rows of the document the gate shows the target governed and the code follows: above a card a text document stands at the pane's body column, its words right under its title (`page::shown::text_over_a_card`), the gate's printed page keeping its own layout. `a` still opens the decision's stage over a card in place, and the prompt in place has no round yet; a decision recorded here does not mark the run as handed on. The walk: 55,585 states (55,578 before), 34 palettes, 46 helps, 4,452 screens at each size, its tests in about 75 s beside other builds.

**Fails first:** `crates/ui/tests/in_place.rs` (new): on the design's run at `ship` the card stands with no key pressed; the arrows over the card scroll its evidence, leave its choice where it was, and `leads()` stays true; `2` points `abort`; `enter` records it on the log and the note says the design's sentence. `crates/ui/src/prompt/decision_card/tests.rs`: a number picks its option.

**Acceptance criteria:**
- [x] The three targets equal at the four sizes; the inventory's row of element P8 is closed.
- [x] `crates/cli/tests/console_interaction/card.rs` passes, a number picking there too.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test in_place`, `cargo test -p yunta-ui --lib decision_card`, `cargo test -p yunta-tui --test screens`, `cargo test -p yunta --test console_interaction`

**Dependencies:** Group check: over a run

**Files likely touched:** `crates/ui/src/stage/run/in_place.rs` (new), `crates/ui/src/stage/run.rs`, `crates/ui/src/prompt/decision_card.rs`, `crates/ui/tests/in_place.rs` (new), `crates/tui/goldens/{targets,screens}/`

**Estimated scope:** M

### Task 114: what a card rests on: a failure, a scope, a departure, a cap

**Description:**
- `Grounds` for the other menus, each drawn above its card:
  - **a failure** (`Menu::Failure`, `NodeFailed`, an escalation of a failed node): the failing output's tail; `kind`, `attempt`, `took` (P6);
  - **a scope** (`Menu::Scope`): the write the fence refused, where one was (`✗ the fence refused a write: <path>`), from the node's ledger's refused writes; the path and why in the card (P5). **`blocks::FenceRefused`** (new, `crates/render/src/blocks/fence_refused.rs`) says it;
  - **a departure** (`Menu::Departure`): what the plan decided, then what the session says instead; the options the engine offers;
  - **a cap** (`Menu::Budget`, `BudgetExhausted`, `Menu::LoopCap`, `LoopOverrun`): what is spent against its cap.
- Targets, moved (`git mv`) from `crates/tui/goldens/targets/`: `failure`, `failure-recorded`, `scope`, `scope-recorded`, `departure`, `budget`, `loop-cap`. Cuts: the design's run at `failure` and `scope` and after each answer; `yunta_testkit_ui::small_runs`' departure, token cap and loop cap.

**Found in its turn:** every card composes its pane with its grounds (`ComposedPane::Card(Grounds)`, `pane/card.rs`) and holds its prompt in place. A failure: render's `HowItFailed` quotes a regression's tail as evidence, its abridged forms letting `attempt` go first, then the tail from its top, counted in the gutter (Surco's `Evidence.kept`), `kind` and `took` staying. A scope: `blocks::FenceRefused`, the task's reason in the card (`escalation::why_it_asks`), `abort` left to `esc` as the table and the targets ask. A departure and a cap: `blocks::Departures`, whose `because` gives way first, and `blocks::SpentAgainstItsCap`. A scope's or a departure's card names its one task at the title's far end (`NodeTitle::about_the_task`); where a card's least cannot stand under the title, the card takes the title's row (`Terminal::keeps_its_title`). A card stands whole but at the shortest look, which the layout hands down (`Room.shortest`): above that, room is made over it by the blank row between the grounds and the card, then the grounds' abridged forms, then the blank row under the pane's title, as `failure` and `departure` draw at 80×24; the code had set the card at its least wherever the grounds wanted rows. A labelled row is one Surco block (`Block::Labelled`), which the cap's block and `WhereItStopped` draw through: its labels stand in a column two cells past the widest of the rows that stand, and a row gives way only where none of its value can stand, as `budget` shows at 60×14. Landed and held whole at the four sizes: `scope`, `failure`, `failure-recorded`, `scope-recorded`, `departure`, `budget`, `loop-cap`. The recorded two hold the recording's facts, `chosen by mcp`: the test kit plays the recording's own events from the answer on where a cut's log goes on, in place of the performer's write. Target rows the lead ruled slips and corrected, each listed before and after for the person: `departure`'s `abort`, with `Pause here:`; `budget`'s two rows at 120 and 80 columns and in ASCII, one cell past the column's rule; at 60×14 the blank row both `budget` and `loop-cap` set under the title, and the row `loop-cap` let go where it fits. An answer given on a real screen reads `chosen by unverified:<user>`, the log's name for an answer nobody claimed: for the person to see. On a loop failed at its cap, `iterations already run this invocation` reads the cap, the one figure that failure carries. The cut of `node-needs-you` presses its keys before the ask is read, until the task that puts a prompt aside cuts it with `esc`. The walk's counts stand, no state it reaches holding a card on these grounds.

**Fails first:** `crates/tui/tests/screens.rs`: each target differs; `crates/ui/tests/in_place.rs`: the scope's grant recorded on the log with the note `Recorded: grant. The session goes on with the wider scope.`; the failure's card keeps its two options at 60×14 with its tail cut from the top.

**Acceptance criteria:**
- [x] The seven targets equal at the four sizes; the inventory's rows of elements P5 and P6 are closed.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test in_place --test scenes`, `cargo test -p yunta-tui --test screens`, `cargo test -p yunta-render blocks`

**Dependencies:** Task 113

**Files likely touched:** `crates/ui/src/stage/run/scene.rs` (new) in Task 91, `crates/ui/src/stage/run/pane.rs` (new) in Task 98, `crates/render/src/blocks/fence_refused.rs` (new), `crates/tui/goldens/{targets,screens}/`, `crates/testkit-ui/src/screens/run.rs` (new) in Task 90

**Estimated scope:** M

### Task 115: the round in place

**Description:**
- `Scene::Round` for `Questions` and `AnswersRefused`: the request above (`you asked`); each question in turn; a refused answer under its question; nothing recorded until the last, then `Operation::Answer` with the surface's channel. A round takes the nodes list and the pane, with no title row and no feed.
- **Its keys**: a line typed on Task 79's boxed row, or a choice picked with the arrows and narrowed by typing; `enter` answers; `shift-tab` the question before; `esc` puts it aside with nothing recorded and the note `Nothing recorded. The run stays waiting on these questions.` No attention row stands while the round does, a line typed or not (Task 92's `Standpoint`).
- Targets, moved (`git mv`) from `crates/tui/goldens/targets/`: `round`, `round-answered`, `round-refused`, `round-aside` (the note `esc` leaves). Cuts: the design's run at `round`, after its first answer and after `esc`; `yunta_testkit_ui::small_runs`' round whose first answers are refused (`AnswersRefused`) and then answered.

**Found in its turn:** `InPlace` holds a decision or a round. The round stands under a `you asked` row (`render::prompt::round::you_asked`) and takes the nodes' list and the pane (`ComposedPane::Round`, `pane/round.rs`, `Scene::lists_its_nodes`), drawn from its top and 110 cells wide at most (`ROUND_MOST`). Every key but `esc` types into it; nothing is recorded until the last answer, then one `Operation::Answer` records them all and the note says so (render's `answers_recorded`, true of the engine's code: ``Recorded: 3 answers. `grill` finishes with them, and the run goes on.``, with its forms for one answer and for none); `esc` puts the round aside, the person pointing at the node, nothing recorded, the note saying it (`Notice::NothingRecorded`). A refusal stands under its question wherever the round is asked: `WaitingQuestions` carries the report, read from the run's own pause, so `yunta resolve-gate` shows it as the application does; the engine's own terminal is handed none. A typed screen's legend that overflows keeps its first keys and ` · …` (`Screen.typed`), as `new-run-refused` and `setup-changing` draw too. A node's feed lists its questions asked and their answer, as the follower prints each. Landed and held whole at the four sizes: `round`, `round-refused`, `round-aside` and `round-answered`, two rows of the last corrected by the lead's ruling, slips of a hand count one and two cells short of the region's last cell, where every other cut row of these targets ends (`unicode-80` row 11, `ascii-120` row 12), listed before and after for the person. What the CLI prints now: the round's question `◆ ask asks`, a refusal without its question's name, and on `yunta status --node` for a node waiting on questions its feed's row `needs you: asked 3 questions…` (render's `node/waiting` blessed, that row alone). `docs/guide.md` says what `esc` does on a round. A round whose answer is recorded no longer counts as waiting on the person in what the state keeps of the run in view. The round at a terminal keeps its own legend. The walk's counts stand, its home following no run parked on a round.

**Fails first:** `crates/ui/tests/in_place.rs`: on the design's run at `round` the round stands with no key pressed; two answers leave the log without an answer; the third records all three; `shift-tab` from the second returns to the first with its answer kept; on the refused round, the refusal stands under its question and the next answers are recorded.

**Acceptance criteria:**
- [x] The four targets equal at the four sizes; the inventory's row of element P3 is closed.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test in_place`, `cargo test -p yunta-tui --test screens`

**Dependencies:** Task 114

**Files likely touched:** `crates/ui/src/stage/run/in_place.rs` (new) in Task 113, `crates/ui/src/stage/run/pane.rs` (new) in Task 98, `crates/ui/src/stage/run.rs`, `crates/tui/goldens/{targets,screens}/`, `crates/testkit-ui/src/screens/run.rs` (new) in Task 90

**Estimated scope:** M

### Task 116: the desk in place

**Description:**
- `Scene::Desk` for a gate that shows a plan: the plan's desk as the design draws it, taking everything between the rule and the attention row, the gate's options at its foot, the confirmation row once a number is pressed (P4).
- **Its keys**: a number, then `enter`; `j k` and `tab` from task to task; the arrows and the wheel through what is read; `p` its proof; `d` what is decided; `esc` puts it aside.
- Targets, moved (`git mv`) from `crates/tui/goldens/targets/`: `desk`, `desk-confirm` (`1` pressed), `desk-proof` (`p`), `desk-decided` (`d`), `desk-change` (the change's pane). Cut: the design's run at `desk`, with each key pressed.

**Found in its turn:** The plan's desk stands in place on the run's screen: it takes everything between the rule and the attention row and shows nothing of the run across, beside or under it (`ComposedPane::Desk`, `pane/desk.rs`, `Scene::heads_itself`); it opens on the plan's first task; a number then `enter` records the option once through `Operation::Decide` and the note says so (``Recorded: approve. On to `implement`.``); `esc` puts it aside with nothing recorded; at the shortest look Surco draws it without its own blank rows. `composes()` returns the pane for every scene, so `acts`, `lead`, `receipt`, `pull_request` and `page` follow. By the targets, and for the CLI's desk at a terminal too: the desk's places are the plan's own (a shown document such as the brief is no place); the head names the workflow without its mode (`WaitingDecision.workflow`) and counts the files the plan names, each once, from scope paths, changes and the spec's test files (`1 task, 6 files`); a task's pane is titled by its id; a task read at its desk says neither `changes` nor `tested by`, and its proof reads `proves` and `runs` (the printed plan keeps every row it had); `d` shows every decision of the plan whole, question, choice and why, and counts them all; `j k` walk the places; the head's far words give way before its title loses a part. Landed and held whole at the four sizes: `desk`, `desk-confirm`, `desk-proof`, `desk-decided`, `desk-change`. Two rows of the targets corrected by the lead's ruling, listed before and after for the person: `new · 7 lines` reads `7 lines` (`desk`, `desk-confirm`, `desk-words`, at 120 columns and in ASCII), the cut's plan holding no shape and its log no reading of the tree, so inventory row `decisions.plan.P10` is carried by no target (`none: D20`); and a continued line of code hangs under its own indent as Surco continues every line (`desk-proof` row 30 at 120 columns and in ASCII, and its count at 60 columns, `… 35 more lines`). Render's and Surco's desk fixtures hand the head the workflow alone, as its one caller does (`fragua`, `reviewed-plan`), their goldens' head row blessed. Left as it is for the person to rule at the checkpoint: the printed plan counts as `decided for you` the decisions that restate no answer of the person and lists the others under `you answered`, where the desk counts and shows every one. Left to the tasks after it: `esc` on a card still leaves for the field; `2` then `enter` on the desk turns to the card's typed line; `review` takes three `esc` to reach the field. The walk's counts stand: no state its home reaches stands as the desk until the design's run is brought there.

**Fails first:** `crates/ui/tests/in_place.rs`: on the design's run at `desk` the desk stands with no key pressed; `1` then `enter` records `approve` on the log; `Down` moves what is read and leaves the task pointed at.

**Acceptance criteria:**
- [x] The five targets equal at the four sizes; the inventory's row of element P4 is closed.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test in_place`, `cargo test -p yunta-tui --test screens`

**Dependencies:** Task 115

**Files likely touched:** `crates/ui/src/stage/run/in_place.rs` (new) in Task 113, `crates/ui/src/stage/run/pane.rs` (new) in Task 98, `crates/ui/src/prompt/plan_desk.rs`, `crates/tui/goldens/{targets,screens}/`, `crates/testkit-ui/src/screens/run.rs` (new) in Task 90

**Estimated scope:** M

### Task 117: a prompt put aside and brought back, and an answer from elsewhere

**Description:**
- **`esc` puts a prompt aside**: the node pointer becomes the person's on its node, whose own scene shows what it waits on and its feed; `a` or `f` brings the prompt back as they left it; `esc` again, or `q`, leaves the run for the field with a note that says the run waits.
- **A prompt belongs to its pause**: news of the same pause never rebuild it, so what the person typed or picked stays.
- **An answer from elsewhere** takes the prompt away, even under a person typing on it: a note says it is answered and by whom, in the log's words, and what was typed goes with the pause.
- `a` on a run whose own decision the attention row names brings its prompt, where Task 93 opened the decision stage.
- **The palette over a prompt put aside**: `/` on a round put aside opens the palette over its node's own scene, its row that answers what the run asks among the rest.
- Target, moved (`git mv`) from `crates/tui/goldens/targets/`: `palette-aside`. Cut: the design's run at `round` with `esc` then `/` pressed. `node-needs-you`, landed in Task 101, is held here as `esc` reaches it: the design's run at `scope` with `esc` pressed draws that golden.

**Found in its turn:** `esc` on a prompt in place (a card, a round, the desk) puts it aside wherever the prompt itself does not take `esc` (a typed line or a pending confirmation still does): the person takes the node pointer on the node the run waits at, which shows its own scene; the prompt stays held for its pause; only a round leaves a note; `esc` again, or `q`, goes to the field with the note that the run waits; on a child the first `esc` puts the card aside and the second goes up to the parent. `a` or `f` brings the prompt back as it was left, from the run and from the pages over it (`attending_its_decision`); `a` no longer opens the decision's stage. When the pause goes, the held prompt goes with what was typed on it and `in_place/gone.rs` writes the note from the log: ``Answered elsewhere: `deny` chosen by lead.`` for a decision, `Answered elsewhere: by lead via mcp.` or `Answered elsewhere: via tty.` for a round (render's `chronicle::who_answered`, shared with the chronicle's row so the two cannot drift; a round the workflow lets go is never parked on, so it has no form here); our own answer whose news lands before its outcome reads as recorded; a pause of another run going writes nothing (`AnswerablePause::run()`). A defect found and fixed at its root: a run answered in place was not marked as handed on, so until its engine wrote its registry the legend read `r continue it from its log … q field`, `q` said the run stays waiting and `r` could hand it to a second detached engine; `Outcome::handed_on()` marks it for a start, a resume, a decision and a round's answers, and the legend reads `q field, it keeps going`. The lead's ruling on keys: going on and closing are bound exactly where a scene's legend names them (`keyed(step, named)` in `acts.rs`, from `Scene::named_acts`; a test walks one cut a scene), so `r` and `x` no longer act on a node's own scene, a pull request, a card, a round or the desk, and `x` no longer acts on a run short of room or with orphans; each stays a row of the palette, which takes it with no key (`Want::taking`). `/` and `?` take the run's note away as any key does. The lead's ruling on the task's first test: half a note holds through news of its pause and the pick through the trip, since `esc` on a typed line returns to the options, the card's rule. Landed and held whole at the four sizes: `palette-aside`; `node-needs-you` is drawn by `esc` on the scope's card, its cut in the notes following. The walk reaches a round in place on the asking run's screen; it counts 47,920 states, 37 palettes, 46 helps and 4,023 screens at each size, its tests 29 s and 42 s. `docs/guide.md` says how a prompt is put aside and brought back, and what an answer from elsewhere does. Nothing a command prints off a terminal moves. Left to the task that deletes the decision's stage: `decision_stage.rs` and that stage's round, which the application no longer reaches.

**Fails first:** `crates/ui/tests/in_place.rs`: half a note typed on the scope's card survives a piece of news of the same pause and a trip to `brief` and back with `a`; a `resolve-gate` from the API takes the card away with the note naming who answered. `crates/tui/tests/screens.rs`: `esc` on the scope's card draws `node-needs-you`, red while `esc` leaves the run.

**Acceptance criteria:**
- [x] `palette-aside` equals its target, and `esc` on the scope's card equals `node-needs-you`, at the four sizes; each of the spec's bullets "`esc` puts it aside" and "A prompt belongs to its pause" has a test.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test in_place --test lead`, `cargo test -p yunta-tui --test screens`

**Dependencies:** Task 116

**Files likely touched:** `crates/ui/src/stage/run/in_place.rs` (new) in Task 113, `crates/ui/src/stage/run/lead.rs` (new) in Task 90, `crates/ui/src/stage/run.rs`, `crates/ui/tests/in_place.rs` (new) in Task 113, `crates/tui/goldens/{targets,screens}/`, `crates/tui/tests/screens.rs`

**Estimated scope:** M

### Task 118: an option that asks for words

**Description:**
- An option that asks for words opens its typed line in place, in Task 79's box: at the card's foot, or at the desk's foot in place of its options (Task 80's drawing); `enter` records the words with the option; `esc` returns to the options. Empty words where the option needs them are refused under the line, as at the base. No attention row stands while the card or the desk does, the line typed or not.
- `Deciding` and `PlanDesk` keep the desk in front while the words are typed for an option picked there, in the CLI's desk too; `docs/guide.md`'s sentence on it follows.
- Targets, moved (`git mv`) from `crates/tui/goldens/targets/`: `card-words`, `desk-words`. Cuts: the design's run at `scope` with its option that asks for words picked; at `desk` with `2` then `enter` pressed.

**Found in its turn:** An option that asks for words opens its typed line in place: at the desk's foot in place of its options, the option's row staying over the box and what is asked faint in it until a key is typed (`2` then `enter`; legend `enter send it · esc go back`; no attention row); on a card every option stays in sight with the box under them (legend `enter record it · esc go back` for a note). `esc` returns to the options and drops the words; an empty `enter` where the option needs words is refused under the line (`✗ this option needs an answer`, the sentence the base's card says, owned once by `Offered::said_with`); words then `enter` record the option with them, once. `PlanDesk` holds how it is being answered (`Answering::{Open, Confirming, Saying}`) and the card's options with what each asks (`decision_card::Offered`); `Left::Chose(PickedOption)` replaces `Left::Picked`, and `DecisionCard::pick` is gone. Surco's `CardBody::Saying` carries every option; render gains `prompt::desk::saying` and one `typed_row`; `Block::Desk` is boxed. Landed and held whole at the four sizes: `card-words` (its cut the note, `n`, since neither option of the scope's card asks for words) and `desk-words` (its cut `2` then `enter`, the notes following); no row contested. With no target left beside the run's screen, the test that held a pane everywhere but its surroundings is gone from `screens.rs`. The CLI's desk at a terminal changes on purpose, for the person to approve: `2`, `enter` keeps the desk and types at its foot, where it closed the desk and typed on the card; the CLI's card keeps every option in sight over its typed line, where it showed the chosen one alone. Surco's `card/saying.*` goldens blessed, the option rows alone. Nothing a command prints off a terminal moves. The walk's counts stand: 47,920 states, 37 palettes, 46 helps, 4,023 screens at each size. `docs/guide.md` says the desk keeps the words at its foot. Found and given a task of their own (118a): the note of an option sent back with words is not yet the design's sentence, and the CLI's prompts word their keys in the third person where the screens say the act.

**Fails first:** `crates/ui/tests/in_place.rs`: at the desk `2` then `enter` opens the line at the desk's foot; words typed and `enter` record `adjust` with them on the log; `esc` brings the options back. `crates/ui/src/prompt/deciding/tests.rs`: the desk stays in front.

**Acceptance criteria:**
- [x] The two targets equal at the four sizes; `crates/cli/tests/plan_desk.rs` passes with the line at the desk's foot.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test in_place`, `cargo test -p yunta-ui --lib -- deciding plan_desk`, `cargo test -p yunta --test plan_desk --test docs_sync`, `cargo test -p yunta-tui --test screens`

**Dependencies:** Task 117

**Files likely touched:** `crates/ui/src/prompt/{deciding.rs,plan_desk.rs}`, `crates/ui/src/stage/run/in_place.rs` (new) in Task 113, `crates/cli/tests/plan_desk.rs`, `docs/guide.md`, `crates/tui/goldens/{targets,screens}/`

**Estimated scope:** M

### Task 118a: a prompt at a terminal words its keys as the screens do

**Description:**
- The CLI's prompts at a terminal (the card, its typed line, the round, the desk and its confirmation, and every other prompt `render::prompt` words) name each key's act in the words the screens' legends use: `enter answer`, `enter confirm`, `enter send it`, `enter record it`, `esc go back`, `shift-tab go back`, `esc leave it waiting`, `esc leave it for later`, `n add a note`, `r read the plan`, the arrows `choose`. Today they say `answers`, `chooses`, `confirms`, `sends it`, `records it`, `goes back`, `leaves the run waiting`, `leaves it for later`, `adds a note`, `reads the plan`, `move`: one act worded twice.
- One owner for each act's words: a screen's legend and a prompt's legend that name the same act read them from the same item of render. A prompt keeps a key of its own only where a terminal needs one (the arrows where a screen takes `j k`), and that entry still says the act in the screen's words.
- An act no landed screen names takes the words its approved target gives it (the targets still in `crates/tui/goldens/targets/`), or, where no target names it, the same form: the act named as an order, never in the third person.
- The note an option sent back with words leaves is the design's: ``Sent back to `plan` with your words: "split the second task".``, on the screen; an option that leads back with no words keeps ``Recorded: adjust. Back to `plan`; the gate asks again once it has run.``

**Found in its turn:** Render's `prompt::PromptAct` says each act of a prompt once, as an order (`choose`, `answer`, `confirm`, `send it`, `record it`, `add a note`, `read the plan`, `go back`, `leave it waiting`, `leave it for later`), and one builder makes each legend for the run's screen and for the terminal alike (`card::choosing_legend(ChosenWith::{Letters, Arrows}, …)`, `card::saying_legend`, `round::legend`, `desk::legend`, `prompt::confirming_legend`); the screen's own copies are gone (`card_legend`, `saying_legend`, `round_legend`, `desk_legend`, `confirming_legend`, `confirmation_legend`, `GO_BACK`, `LEAVE_IT_WAITING`, `READ_THE_PLAN`). `screen::recorded` takes the words said with an option: one that leads back with words leaves the design's note, ``Sent back to `plan` with your words: "split the second task".``; with none it reads as before. The lead's rulings: the acceptance search still finds `answers` three times outside any legend (a label `yunta stats` prints, its test, a test's hash input), left as they are; `esc keeps the detected default` was today's phrase carried to the screen in the third person, so the code says `esc keep the detected default`, the target `setup-changing` is corrected in that one word at 120 columns, in ASCII and at 80, and inventory row `starting.setup.T04` follows (listed before and after for the person); `new -i` says `esc take the first`; `type to filter` is said by the list's filter row and leaves the lists' legends. Keys under a prompt fold as a legend folds, between two keys and never inside one, each key strong and what it does muted: the desk's foot (`desk/foot.rs`), the card's and the round's (`card/foot.rs`) and a list's (`asked.rs`) lay them out through one function, `Legend::folded`; before, a joined string was wrapped as words, and at 80 columns `yunta resolve-gate`'s card ended a row `esc leave it` with `waiting` under it (it did at the base too, in the old words). Surco's fixtures and catalog hand every prompt the words the product says, 52 goldens blessed in their legend rows alone (the catalog at 60 columns one row shorter, a list's legend fitting one row). What a prompt shows at a terminal, before and after, is kept for the person under `checkpoint-c/task-118a/`. The confirmation over the field and the application's pickers read the general words until the tasks that land their targets word each act. Nothing a command prints off a terminal moves; no landed screen moves. The walk's counts stand: 47,920 states, 37 palettes, 46 helps, 4,023 screens at each size.

**Fails first:** `crates/render/src/tests/`: each prompt's legend equals the legend of the screen that asks the same thing, entry for entry but the keys a terminal owns; the note of `adjust` recorded with words reads the design's sentence. `crates/cli/tests/console_interaction/`: the card's, the round's and the desk's legend rows read the screens' words.

**Acceptance criteria:**
- [x] `git grep -n '"sends it"\|"goes back"\|"confirms"\|"chooses"\|"answers"\|"records it"\|"adds a note"\|"reads the plan"' crates/render/src crates/ui/src crates/cli/src` finds nothing.
- [x] Every legend row a prompt shows at a terminal is listed before and after for the person.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render`, `cargo test -p yunta-ui`, `cargo test -p yunta --test console_interaction --test plan_desk --test gate_review_cmd --test interface --test docs_sync`

**Dependencies:** Task 118

**Files likely touched:** `crates/render/src/prompt.rs`, `crates/render/src/prompt/{card,round,desk}.rs`, `crates/render/src/screen.rs`, `crates/render/src/screen/run.rs`, `crates/cli/tests/console_interaction/`, `docs/guide.md`

**Estimated scope:** S

### Task 119: `Stage::Decision` and the run's page go; `review` opens the run's screen on the plan

**Description:**
- **Deleted, with every caller** (the compiler names them): `Stage::Decision` and `crates/ui/src/stage/decision.rs`; `Operation::{ReadDecision,ReadQuestions,ReadPage,ReadNodePage}` and `Outcome::{Decision,Questions,Page,NodePage}`, with their performers in `performer/run.rs` and their arms in every stage's `answered`; the run stage's page; render's `screen::DECISION`. The API's `waiting_decision`, `waiting_questions`, `page` and `node_page` stay for the CLI.
- **`Stage::opened_by`** maps `ResolveGate` and `Review` to `Stage::Run(RunStage::opened_on(run, Opens))`, on the card or on the plan; `State::opened_on` opens `[Run]` alone over the field.
- **`yunta review <run>`** at a terminal opens the run's screen on the plan; `esc` puts it aside on the run, `esc` again goes to the field and `q` leaves the run for the field. `crates/cli/tests/application/review.rs` says so. `README.md`'s row and the guide's review paragraph follow.
- `crates/ui/tests/decision_stage.rs` goes: its three tests move to `in_place.rs` under what they hold. `crates/testkit-ui/src/{recorded.rs,screens.rs}` and `crates/ui/tests/walk.rs`' `MUST_REACH` follow.

**Found in its turn:** Gone, with every caller: `Stage::Decision` and `stage/decision.rs`; `Operation::{ReadPage, ReadNodePage, ReadQuestions, ReadDecision}` and `Outcome::{Page, NodePage, Questions, Decision}` with their performers and arms; the run stage's page (`run/page.rs`, its fields and `RunStage::{opening, shown, went_on, node_page_read, reads_a_page}`); render's `screen::DECISION`; the test kit's recordings of the four readings. The API's `waiting_decision`, `waiting_questions`, `page` and `node_page` stay, the CLI's. `Stage::opened_by` maps `status` and `resolve-gate` to the run's screen on its card and `review` to its plan (`RunStage::opened_on(run, Opens)`); the opening is spent on the pause it opened on (`Option<Opens>`, taken by the first prompt built), so a later gate of the run stands as its own scene says. `yunta review <run>` at a terminal opens the run's screen on the plan's first task, under the run's trail and frame: `esc` puts the plan aside, `esc` again goes to the field, `q` goes to the field at once (three `esc` before, under a trail `› decision`); off a terminal it prints what it printed (one run captured with both binaries: 51 lines alike but the run's id). Beyond the task's text, each needed by it: the scene follows where the plan stands, in front of its card or behind it, as the held prompt says; `r` on a card turns the card's own prompt to the plan's desk in place, `esc go back` returning to the card as left; the desk stays in front while its answer is in flight and after it lands, where the card showed for a moment. Help over a prompt in place says the prompt's keys, by one rule in render's `help_over_a_run`: the prompt's keys first, in its own words (its legend, and the keys its legend leaves to help: on the desk `j k next place`, `shift-tab place before`, `pgup pgdn by the page`; on a card `tab shift-tab` and its numbers `choose`), a run's row giving way where the prompt names any of its keys, and only `ctrl-c` staying while a line is typed; before, help over a card said `n` starts a new run. `crates/ui/tests/decision_stage.rs` is gone, its four tests moved under what they hold in `in_place/{aside,desk,round}.rs`; two tests of the deleted stage and of the page's reading are deleted with it. `README.md`'s row, the guide's review paragraph and `yunta review --help` follow. The walk no longer reaches the decision stage's screens and reaches in their place the run's screen opened on the plan, the plan put aside, the plan read from a card, help over the desk and the desk with its answer in flight: 44,541 states, 37 palettes, 51 helps, 3,897 screens at each size, its tests 46 s to 59 s. No landed screen moves. Left as they are, named nowhere: `home`, `end` and `space` on the desk, and `N` and `R` on a card.

**Fails first:** none for the deletion; its guard is `in_place.rs`, the walk and the application's pty tests passing. `crates/cli/tests/application/review.rs`: `review` opens on the plan with the run's trail and no `decision` place, red at the base.

**Acceptance criteria:**
- [x] `git grep -n "Stage::Decision\|ReadPage\|ReadNodePage\|ReadDecision\|ReadQuestions" crates` finds nothing.
- [x] Off a terminal `review` prints what the base prints.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui`, `cargo test -p yunta --test application -- review deciding`, `cargo test -p yunta --test docs_sync`, `cargo test -p yunta-tui --test walk`

**Dependencies:** Task 118

**Files likely touched:** `crates/ui/src/{stage.rs,state.rs,operation.rs,outcome.rs}`, `crates/ui/src/stage/decision.rs`, `crates/ui/src/performer/run.rs`, `crates/cli/src/commands/review.rs`, `crates/cli/tests/application/review.rs`, `crates/ui/tests/decision_stage.rs`, `README.md` (a mechanical deletion that touches every caller)

**Estimated scope:** M

### Group check: decisions in place
- [x] The suite's groups green; every target of Task 113 to Task 118 equal; `prompts.py`'s captures differ from the base's only where the inventory names; the walk's counts and time recorded.

**Found in its turn:** the gate's short steps pass (fmt, clippy on the workspace, the smell counters, the agent docs); the suite's twelve groups hold 5,376 tests, none failing, none aborted (3,227 outside the engine and the CLI, 1,495 the engine's, 654 the CLI's). Sixty targets stand beside the goldens and equal their screens at the four sizes; none of this phase's is left in `targets/`, and the lead's tools hold every family (102 screens, 408 frames), the inventory's cover and the page's notes (no difference unexplained). The walk reaches 44,541 states, takes 37 palettes row by row and opens 51 helps, and draws 3,897 screens at each size; its tests take 55 s and 52 s at a load of 11 to 12 from the other session's builds. The release binary weighs 31,526,048 bytes. Of the 159 captured views, 5 differ from the last check's, each by a task of this group: the prompts of `init -i` (two views) and `new -i` by their legends' words; `resolve-gate`'s card by its legend and by the options that stay in sight over its typed line; the round by its question (`ask asks`) and its legend. `review` at a terminal draws alike once its bytes end. Logs: `.plans/yunta-tui/proof/live-screens/check-decisions/`, captures `check-decisions-prints/`.

### Task 120: what holds, and only evidence moves

**Description:**
- The spec's "What holds" and "Only evidence moves", each a test in `crates/ui/tests/holding.rs` (new), through `State` alone, on the design's run fed one batch of news at a time:
  - with a feed scrolled back by the arrows, calls let go one at a time leave the rows in sight where they are and raise the count under them;
  - a card appearing while the person points another node changes no key's meaning, and the attention row names it;
  - a half typed answer survives news of the same pause;
  - an open palette keeps its pointed row through news;
  - with no event, two drawings a second apart differ only in their times, and every time and age on screen is true to the second.

**Found in its turn:** `crates/ui/tests/holding.rs` holds the five rules through `State` alone on the design's run, fed one event at a time as a following that holds the run hands news (`RunScreen::followed_on_to`, new in the test kit; `Cut::goes_on` returns the events it loads): a feed scrolled back keeps its rows in sight and raises the count under them a call at a time; a decision that comes while the person points another node changes no key's meaning, the attention row naming it and `a` bringing its card; half a note typed on a card stays as news of its pause land; an open palette keeps its pointed row; with no event two drawings a second apart differ only in their times, each equal to one worked out from the log (`holding/times.rs`). Each was seen red with its rule broken in the code (the feed's place kept by its distance from the end; a prompt standing whoever leads; the held prompt dropped on news; the palette built again; the spinner turned by the clock), the source put back as it began. A sixth, `holding/one_following.rs`, holds why a prompt is never rebuilt by a run read whole again: the application makes one following for its life and reads a run's screen whole once, every page over the run keeping the same run followed; the paths that could read it whole are listed in the test's doc with its `Reached by:`. The lead's note: a cut that stops between the two events of an answer (the gate's and its resolution's, one commit) is a state the application never sees, so tests of a pause's news use the gate's own event. The five pass 50 runs at once beside eight busy loops. A trap recorded in `.agents/traps.md`: a source put back with its old time is not built again.

**Fails first:** each test seen red with its rule broken for the run (the feed's place kept by position; the prompt rebuilt from news; the palette rebuilt), then the rule put back.

**Acceptance criteria:**
- [x] Five tests, one for each bullet; the second drawing's differences are times alone.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test holding`

**Dependencies:** Group check: decisions in place

**Files likely touched:** `crates/ui/tests/holding.rs` (new), `crates/testkit-ui/src/screens/run.rs` (new) in Task 90

**Estimated scope:** S

### Task 121: at a pseudo-terminal: a held session's feed, a card answered in place, an answer from a shell

**Description:**
- **`crates/cli/tests/application/feed.rs`** (new): bare `yunta` at a pty on a run whose session is held between its calls by `MockStep::AwaitsWord`; each call let go by its word stands as the last row of the node's feed, the pane's last row, before the next word is given; scrolled back with the arrows, the rows in sight stay as more calls are let go and the count under them rises; the pane stays on that node when the run moves on; `End` returns to the newest.
- **`crates/cli/tests/application/deciding.rs`**: a card appears by itself as the run parks, with no key pressed, and is answered there, the choice on the log; a run parked on a person and answered from a shell goes on with no key pressed: the card goes, a note says who answered, the scene becomes work.
- Each asserts an order, never a duration, and runs under `stress_binary.py` (in `.plans/yunta-tui/tools/`) before the phase's commit.

**Found in its turn:** Three tests at a pseudo-terminal, each waiting for words on the screen and sending a key only after the screen it acts on is seen. `application/feed.rs`: on a run whose session is held between its calls, each call let go stands as the pane's last row before the next word is given; scrolled back with the arrows, three more calls leave the rows in sight as they are and raise the count under them by one each; the pane stays on that node as the run moves on; `End` returns to the newest. `application/deciding.rs`: a card stands by itself as the run parks, with no key pressed, and its answer is on the log; a gate answered from a shell (`yunta resolve-gate <id> go --by lead`, another process) takes its card away with no key pressed, the note reading ``Answered elsewhere: `go` chosen by lead.`` as the log says, and the run works on. Each was seen red with its rule broken (news putting a scrolled feed back at its end; the card not standing by itself; the note not written), the source put back. Each passes 50 runs, five workers beside eight busy loops on eight cores, none failing: `.plans/yunta-tui/proof/live-screens/phase-3-stress.txt`. The engine of the feed's test is `yunta run` in a process of its own, a run on the mock adapter never being handed to a detached engine. The stress tool keeps its own busy loops now (`--busy <n>`).

**Fails first:** each seen red with the run stage's lead broken for the run (the pane following the end while scrolled back; the card not standing by itself), then the rule put back.

**Acceptance criteria:**
- [x] The spec's criteria 2 and 3 hold at a pty.
- [x] Each test passes 50 runs under `stress_binary.py` beside busy loops, the counts written in `.plans/yunta-tui/proof/live-screens/`.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test application -- feed deciding`
- [x] `python3 .plans/yunta-tui/tools/stress_binary.py` over the two files' tests.

**Dependencies:** Task 120

**Files likely touched:** `crates/cli/tests/application.rs`, `crates/cli/tests/application/feed.rs` (new), `crates/cli/tests/application/deciding.rs`

**Estimated scope:** M

### Task 122: the walk over every scene

**Description:**
- The walk's home (`crates/testkit-ui/src/home.rs`) holds the design's run at its moments and the small runs, loaded through `Cut` from `TARGET_CUTS`; `Recordings` (`recorded.rs`) records what is followed and each reading a scene asks (the pull request, a verification) for them.
- `crates/ui/tests/walk.rs`' `MUST_REACH` gains a screen for every scene of the spec's table, and holds that `State::followed()` names the run under every stage opened over it; `crates/tui/tests/walk.rs` lays every state out at its two sizes.
- The walk's states and time are measured against the last group check's, and the suite's against CI's 45 minutes.

- **The walk draws the design's own size.** Its two sizes, 80 and 60 columns, both stand under the 102 a side region needs, so since Task 76 it lays out no side. It draws every state at `ascii-120` (120×36) in place of `ascii-80`, and at `unicode-60` as before: the side's edge whole on every row it spans, every region inside its rectangle. 80×24 is held by each scene's goldens. Its states and time are written beside `walk-fed.txt`'s.
- **A child in the walk's home** (from Task 106): with the small runs in the home, the walk holds that `State::followed()` names the child under every stage opened over a parent parked on it, and reaches the child's screen.
- **`MUST_SCROLL` holds a run's numbers again** (from Task 111): the screen leaves most of a run's numbers to the printed page, so no run of the walk's home spent enough for them to pass its screen and the entry `("stats", "a run's stats")` was taken out; with the design's run in the home the walk scrolls them, and the entry stands again.

**Found in its turn:** The walk's home holds, each loaded through `Cut` from `TARGET_CUTS`: the design's run at five moments (the board at 99, the scope's pause at 118, the findings at 200, the failure at 254, the receipt at 276), twenty small runs in one cut, the child in a cut of its own, a command at work and an answer recorded with no resume on the smallest run that stands as each (`FAVLTY` at 2 and at 4), a round picked from a list on a run of its own (`CH00SE`), and the promoted run. A piece of a run's news is recorded under the head the run stood at, so one run stands at several moments. `MUST_REACH` names every scene of the spec's table (80 entries, the new ones seen red before the home held their runs); `MUST_SCROLL` holds a run's numbers again. The terminal's walk draws every state at `ascii-120` (120×36) and `unicode-60`. The lead's rulings: ten moments of the design's run alone took each walk's test to four and a half minutes, so a scene is carried by the smallest run that stands as it; `MOST_STATES` is 130,000, a third over the 97,626 the walk reaches. What a stage is to its run is a type, `ItsRun::{Shows, WroteFor}` (`crates/ui/src/stage/its_run.rs`): the walk holds that a stage that shows a run as it moves names the run the state follows, and that a stage holding what was written for a run (the receipt, a verification, the page for a browser) names that run still, a successor taking the run's place under it (its own test in `scenes/successor.rs`). The child: the trail names the child under every stage opened over a parent parked on it. Defects the walk found, fixed at their root: a run's numbers were never scrolled by the walk because their page stood outside `State::pane()` (`state/followed.rs`, `stage/stats.rs`); a list picked by typing showed no cursor: it stands on the list's filter row, at its first cell while nothing is typed and after the words otherwise (`filter_cursor` in Surco's `asked.rs`), for the application's lists, the palette, a round's question picked from a list and the CLI's lists alike, the text of each unchanged; where a filter row gave way for room with nothing typed no cursor shows, and a filter in force brings its row back (held by a test; `round-answered` at 60×14 stands as approved). The walk's cursor rule reads the frame (`crates/tui/tests/walk/cursor.rs`): a typed line places a cursor, a list places one exactly where its filter row stands, nothing else shows one. Measured where the walk's time goes (a sample of 20 s): six tenths in each state's fingerprint (the screen 27%, of which a run's frame derived again 17%; its Markdown 21%), a seventh in taking the input, a state's clone 0.2%. Cut without changing what the walk tells apart: an input's label built only for a state kept, what a state follows hashed directly (`Hash` on `yunta_api::Followed` and `FieldReach`), the design's places made once and only when opened; the counts did not move. Not made: Markdown kept by its screen (Surco's `Screen` holds no equality, a meter's bars being floats), and a run's frame derived once a head, which changes the product. On a quiet machine the two walk tests take 192 s and 188 s (walk 153 s and 148 s, user CPU 719 s each): some ten seconds past the three minutes the person set, theirs to rule at the checkpoint. Left as found: the small runs are built one after another (27 s), both walk binaries build the same home, `Menu::Console` stands in no home (the API reads no decision of a console gate; its screen is a standstill). Two tests failed once under load beside this work and pass alone, given to another agent: ui's `in_place::round::nothing_is_recorded_until_the_last_answer_which_records_every_one` and the CLI's `application` `deciding::a_round_answered_…`.

**Fails first:** `MUST_REACH`'s new entries seen red before the home holds the fixtures.

**Acceptance criteria:**
- [x] Every scene of the spec's table is reached; the walk stays under its bound (`MOST_STATES`), its count and time written in `.plans/yunta-tui/proof/live-screens/`.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test walk`, `cargo test -p yunta-tui --test walk`

**Dependencies:** Task 121

**Files likely touched:** `crates/testkit-ui/src/{home.rs,recorded.rs}`, `crates/ui/tests/walk.rs`, `crates/tui/tests/walk.rs`

**Estimated scope:** M

### Task 123: every scene at its four sizes, the inventory counted, the feed's probe, and Surco's reference

**Description:**
- `crates/tui/tests/screens.rs` holds every target of a run's screens and of what stands over a run at the four sizes; none of the base's `run`, `card`, `desk`, `palette`, `help` or `confirmation` frames remains, nor their builders in `crates/testkit-ui/src/screens.rs`; `crates/tui/goldens/targets/` holds no target this list lands.
- `python3 .plans/yunta-tui/tools/targets_check.py crates/tui/goldens/screens` finds no row of a run's screens differing; the rows the CLI prints of the same things are listed with their goldens and captures for Checkpoint C.
- `pages.sh`'s and `prompts.py`'s captures against the base's: each difference one the inventory names, listed in `.plans/yunta-tui/proof/live-screens/phase-3-prints.txt`.
- **The probe's `feed`** (new, in `.plans/yunta-tui/proof/live-screens/probe/`): a run whose one node made ten thousand calls, its screen drawn at 200×60 with that node pointed and its feed at its end, the median of 2,000 drawings, as the probe's `frame` measures; its number written in `.plans/yunta-tui/proof/live-screens/phase-3-feed.txt`, which Phase 4 measures again.
- **Surco's reference**, at the path Task 160 names (its sources outside git at `~/.claude/plans/surco-v3/tools/reference/`, republished to its artifact), edited in place, not appended, for the pieces this phase changed or added, each with its owner in the code: `Noted`, `Tally`, `SideAt`, `Line::pointed`, `Horizon`, `FindingList`, `Answer::Unkeyed`, the legend's far keys, a region's edge, the typed row's box, and the redrawn card, round, list of choices, rail, boxes and bars.
- **The guide says a run's screen whole** (from Tasks 108 and 109): `docs/guide.md`'s "The application at a terminal" says what a finished run's screen shows and that showing it writes nothing, and that a promoted run's successor takes the screen while the run leads, `f` handing it over otherwise; `docs_sync` holds what it quotes.

**Found in its turn:** `LANDED` holds 60 screens whole at the four sizes, `feed-back` and `asked-elsewhere` by tests of their own; the 251 files under `crates/tui/goldens/screens/` are each read by a test, none of the base's `run`, `card`, `desk`, `palette`, `help` or `confirmation` frames or builders remains, and `targets/` holds the 40 screens of the next phase. The lead's tools hold a run's screens (62 screens, 248 frames; no row differing), the inventory's cover (66 components, 298 phrases, 332 rows only today's screen says) and the page's notes. Against the base, 67 of 159 captured views differ, every differing line named by a task or a ruling of this phase and none unnamed (`.plans/yunta-tui/proof/live-screens/phase-3-prints.txt`); against the base's digests 162 goldens changed, 540 are new and 8 are gone. The release binary weighs 31,542,560 bytes. The probe gains `feed`: a node of 10,000 calls pointed, its feed at its end, at 200×60 draws in 0.35 ms (median of 2,000; 0.28 ms with 100 calls: some 7 ns a call grow with the node's calls though only the rows in sight are drawn); `parts`, `frame` and `dump` no longer build against this tree and stand out of the probe's build with the reason. `docs/guide.md`'s "The application at a terminal" is one text of five paragraphs: keys and the palette, what a run's screen shows, a run's keys, ended runs and a successor, prompts in place. Surco's reference is rebuilt from this tree and republished (its sources point at this worktree; every capture retaken from this phase's binary): it had been built before the application existed, so it gains the application whole (a screen's parts, how a pane is laid out, the boxes, how a screen gives way, how screens are held), a family for what is answered in place, the one rule on a cut and the legends' words, each piece with its owner; 43 blocks where it said 28. Three real runs with this phase's binary reach `ship` in 126 to 136 s, the engine's time between nodes 0.553 to 0.672 s, the last figure one stall of a tenth of a second at one node's start on a busy machine, the seven other gaps equal to the earlier runs' to the millisecond. Found while the reference was captured, and given a fix of its own: text in a box over a screen keeps the background of the cells under it.

**Fails first:** none: a count, a measurement and the reference. Their guard is the screens test and the checker.

**Acceptance criteria:**
- [x] The checker counts zero differing rows for a run's screens; the screens test passes at the four sizes.
- [x] The feed's median is written beside the probe's `frame` on the base, and Surco's reference names each piece above with its owner.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --test screens`
- [x] Manual check: `python3 .plans/yunta-tui/tools/targets_check.py crates/tui/goldens/screens`, and the captures' list read against the inventory.

**Dependencies:** Task 122

**Files likely touched:** `crates/tui/tests/screens.rs`, `crates/tui/goldens/screens/`, `.plans/yunta-tui/proof/live-screens/` (outside git), `.plans/yunta-tui/proof/live-screens/probe/` (outside git), `~/.claude/plans/surco-v3/tools/reference/` (outside git)

**Estimated scope:** M

### Group check: the proof
- [x] The gate (`sh .plans/yunta-tui/tools/gate.sh`) green, one build at a time with `CARGO_INCREMENTAL=0`; every golden and capture against the base differs only where this phase's tasks name; the walk's counts and time recorded.

**Found in its turn:** the gate's twelve steps pass on the final tree (`.plans/yunta-tui/proof/live-screens/gate-c-final/`): 329 test binaries, 5,399 tests, none failing, none aborted; its test step took 2,123 s on a machine loaded by another session, the tests' own times summing to 1,424 s (832 s at Checkpoint B's gate). A first run of the gate on the phase's tree (`gate-c/`) had failed the strict documentation build alone: two redundant link targets and six links from the test kit's public docs to private items, fixed. Found while Surco's reference was captured and fixed with its tests: a box over a screen kept the ground, color and weight of the cells under it. Against the base, 67 of 159 captured views differ, every differing line named by a task or a ruling (`phase-3-prints.txt`). The walk is one test that holds the interface's rules and the terminal's over 97,626 states, 114 palettes, 63 helps and 10,950 screens at each of `ascii-120` and `unicode-60`, in about 205 s on a quiet machine (280 s in this gate); the screens test draws on every core.

### Checkpoint C: a run's screen
- [ ] The user tries a real fragua run in the application in the sandbox: followed on its screen, every decision answered in place, one answered from a shell; and reviews what the CLI's prompts and pages now print (the goldens' diff and `phase-3-prints.txt`), the receipt's rewordings among them, and Surco's reference.
- [x] The spec's criteria 2, 3, 5, 6 for a run's scenes, 7, 8 and 11, each with its evidence; no row of the inventory differs on a run's screens; what the CLI prints differs from the base only where the inventory names; the walk's states and time and the suite's time are measured against CI's limit; the feed's probe recorded.
- [x] The phase commits after every step of the `ci` job passes (the gate, `CARGO_INCREMENTAL=0`, one build at a time), with `crates/{surco,render,ui,tui,testkit-ui,api,cli}/` (the targets moved out of `crates/tui/goldens/targets/` among them) and the goldens of `crates/{surco,render,engine,tui}/goldens/` that changed, `docs/guide.md` and `README.md`, staged by name; then `cargo clean`.

**Decided at the checkpoint (2026-11-21):** the person read the checkpoint's page, wrote their decisions on it and said to go on; whether they made the trial run is theirs to say, so its box stays open. Their decisions: one exploration for both walks, now (done, with the screens test drawn on every core; a run's frame derived once for a head of its log is Phase 4's first task); the printed plan follows the desk on `decided for you` (done: every decision of the plan under it in the plan's order, one that restates an answer keeping the question asked and the answer beside it; one owner for the count, `page::plan::change::how_many_decided`); the `chrono` advisory ignored in `deny.toml` with its reason and its replacement recorded as open work (done; the edit made by a script the person asked for); the three changes to the CLI's prompts approved; the lead's rulings and the corrected rows confirmed. Told to the person on the page: CI's `test-release` job failed once on the Phase 2 commit in a test of the base (`crates/core/tests/process_mark.rs`), which this module does not touch.

## Phase 4: the field, a new run, a setup, and the proof

Its groups, each green before the next: Surco's pieces of these screens; render's phrases of these screens; the field; the first request; a new run; a setup; every page the design draws no frame for; every key; the docs; the proof.

**Decided by the owner on 2026-11-21, to save about an hour a group:** half of the groups left take a light check, the others the whole gate. Light (the field; the first request; every key; the docs): the gate's short steps (fmt, clippy, the build, the schema, smells, docs, the strict doc build), the whole tests of every package the group touched, then the release build, the captures and their comparison, and the goldens' digests. Whole gate (a new run; a setup; every page) and again before Checkpoint D. Where a light group's check line says "the suite's groups green", it means the touched packages' tests. `.plans/yunta-tui/proof/live-screens/tasks/group_check.sh` runs either (`LIGHT="<packages>"` for the light one) and lists what the tests left running before and after.

**Decided by the owner on 2026-11-22, for what is left of the phase, to cut repeated work without lowering the bar:** (1) a task's agent runs what proves its task (its own tests, the landed screens, the CLI's tests at a terminal where a screen changes, the walk once, the static checks; the engine, core, replay or the adapters whole when touched) and the group's check runs every suite once, where each agent ran every touched package whole and the check ran them again; (2) the walk writes its table of states by stage at each run (`YUNTA_WALK_TABLE`), so a count that moves is explained by comparing two tables (`tasks/walk_diff.py`) and not by further walks; a stage that moves with no cause in the change still stops for the lead; (3) the lead sends a task back only for what blocks the next one and gathers every other ruling into one pass of fixes at the end of each group (`tasks/fixes-<group>.md`). Unchanged: the test written first, every landed frame held cell for cell, the walk, the whole gate of the setup's and the pages' groups and before the checkpoint, the real runs and the review.

**Decided in this list, beyond the plan:**
- **A pack is fetched from a directory inside its repository** (Task 147): a source `<repository>//<directory>[@ref]`, which `yunta pack add` takes too since it calls the same fetch. Evidence: `clone_pack` clones a whole repository and `read_manifest` reads its root (`crates/api/src/packs/vendor.rs:111-128`, `:155-165`), while fragua's manifest stands at `packs/fragua/pack.yaml`; the source the spec's question 8 names cannot be fetched at the base.
- **The setup fetches its pack as it opens, beside detecting the repository, and proposes the runners the pack's workflows name** (Task 149 to Task 151). Evidence: `ProposedRunners::of` reads only the repository's catalog (`crates/api/src/setup/init.rs:49-60`), so a setup that installed fragua after writing would declare none of fragua's runners and its first run would stop on them. Confirmed, what it leaves equals `yunta pack add <source> --yes` then `yunta init -i` with the same answers; the setup also reads who git commits as (`Workspace::committer`, `crates/api/src/committer.rs`) for its `commits as` row.
- **The pack's source is named once, in `yunta-api`** (`SetupPack::of_this_binary`, Task 149). Tests point the Yunta repository's URL at a local repository through git's own `url.<base>.insteadOf` (`GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_0`, `GIT_CONFIG_VALUE_0`), handed to git through the workspace's `subprocess_vars` in process (`crates/api/src/workspace.rs:59-63`) and through the binary's environment at a pty. The product holds no test hook.
- **`enter` on the setup's write row shows the pack's audit and its confirmation:** there `enter` writes the setup and installs the pack, and `esc` writes the setup without it (decision 5's "declined"). An unreachable pack is said on its row, and the write row then writes the setup alone. What was written then stands on its own page (`setup-written`), with what the installed pack still needs, and `enter` goes on from it to the first request (Task 152, Task 153).
- **A role's row holds every candidate the machine answers for, in order,** as the design draws it (`claude-code/… then codex/…`, `design/setup.wide.txt` rows 19-22). The configuration holds a list of candidates for each runner (`crates/core/src/config/mod.rs:74`), and `init -i` asks one adapter and one model a role today (`crates/cli/src/commands/init.rs:104-120`), keeping one `RunnerCandidate` (`crates/api/src/setup/init.rs:120`). So the declaration grows to the list, and `yunta init -i` writes the same list with the same answers: the adapter a person picks stands first, the others the machine answers for follow in the order it found them, each with the model typed for it or none (Task 148, Task 151). What `init` writes unasked changes with it, and its goldens are blessed there.
- **A line asked of a person is one `TypedAnswer`, typed on Phase 3's boxed row** (`Task 79`). The first request, a new run's rows and a setup's typed rows take it, with the placeholder and the hint the design draws (`design/empty.wide.txt`, `compose.wide.txt`), which `TypedAnswer` gains (Task 124). The CLI's line prompts set neither, so what they print does not change here. No second block draws a line to type in.
- **A confirmation's box is the overlay with the question as its title:** under it, what the act does, then `SameAtAShell` (the commands that do the same, muted, with no heading and no gloss). Every confirmation of the application takes it. The CLI draws no confirmation, so only the application's goldens change (Task 127, Task 132).
- **On the workflows and their pages, the stage's own keys from the base win over the everywhere row** (`n` a new workflow and a new pack; `a` an audit), since the spec's row for the workflows keeps "the keys it has at the base". There a new run is `enter` on a workflow, and the attention row says what needs the person with no key, as on a screen being typed into (Task 155, Task 158).
- **One way into a new run:** `n`, the palette's `start a new run`, the first request's `enter` and the workflows' `enter` all take `NextStep::Run` (`crates/core/src/step.rs:56`). `Stage::opened_by` maps it to the new run, blank or with its workflow chosen (Task 145).
- **`esc` on a new run keeps what was typed:** the request goes back into the first request's line when the new run was opened from there. Otherwise the state keeps the one new run left unstarted, and the next `n` opens it as it was left, until a run starts. Reason: the stage under it (the field, the workflows) holds no request (Task 142).
- **A new run reads its panel's facts once:** the catalog and doctor's findings at a glance (`Doctor { sessions: false }`) as it opens, and a workflow's history once when that workflow is first chosen. These are machine facts, read once as the machine's screens are (question 12) (Task 144).
- **Admission decides what is missing.** The new run checks nothing admission checks; a refusal is said under the row it names (a missing input), else above the row that says what `enter` does (Task 145).
- **The field's screen draws `FieldRows`:** its first row and its groups. `yunta list --runs` keeps `Field`, and both draw rows through one `groups()`, so the printed page changes only where a row's or a group's words do. Whatever the inventory leaves to the printed page (headline, footer, next steps) stays only on `Field`, its next steps in the palette's words where the palette words the act (Task 130, Task 137).
- **The glance's acts are worked out once for each head** of a kept run (`RunFrame::actions` with the decision the engine rebuilds), never at a draw: `actions` needs the decision, and rebuilding one is a walk of the state (`crates/api/src/field.rs:198-205`) (Task 138).
- **The signpost takes the design's frame with its own target, and its palette offers the machine's field** (`NextStep::ListRuns { reach: Machine }` opens the field at that reach). That is how a person "reaches the machine's field" from outside a repository, as the spec's last line of "The elements" says, and until they do the application follows nothing (Task 157).
- **A walk check holds that each key a stage takes is listed by its help or its legend** (Task 159), so a key left out of the legend cannot also be left out of help.
- **A target whose wide frames show the glance is held at its narrow sizes first** (Task 137, Task 138): `field` and `field-unreadable` draw the glance of the run pointed at from 102 columns, which Task 138 builds, so Task 137 holds their `unicode-80` and `unicode-60` frames, where no glance stands, and Task 138 their `unicode-120` and `ascii-120` frames.

**Needs from other phases:**
- Phase 1: the approved targets of every screen this phase draws, at the four sizes, in `crates/tui/goldens/targets/`; the task that draws a screen moves (`git mv`) its targets from `goldens/targets/` into `goldens/screens/`:
  - the field: `field`, `field-unreadable`, `field-glance-stalled`, `field-glance-stopped`, `field-marked`, `field-close`, `field-closed`, `field-machine`, `field-note`, `field-resumed`, `field-palette`, `field-help`; and over a run, `close-run`;
  - starting: `first-request`, `new-run`, `new-run-inputs`, `new-run-mode`, `new-run-refused`;
  - the setup: `setup`, `setup-changing` (a role's adapter changed and its model being typed), `setup-audit`, `setup-confirm`, `setup-written` (what was written and what the installed pack still needs, from which `enter` goes on to the first request), `setup-unreachable`;
  - every page the design draws no frame for: `machine`, `upkeep`, `workflows`, `workflow-checked`, `workflow-history`, `workflow-graph`, `cases`, `new-workflow`, `packs`, `pack-audited`, `document-shape`, `evidence-receipt`, `evidence-verified`, `evidence-page`, `signpost`. `room-needed` stands only below 60×14, so it has no target and its base frame is the one kept.
- Phase 1: the inventory with its rows for these screens and for what each says today and the design does not; `targets_check.py`, which takes the directory it checks (`crates/tui/goldens/screens` here).
- Phase 1: the fixtures: the design's run, kept; the small runs and three finished runs of one workflow (`small_runs::past`), built in process by `yunta_testkit_ui::small_runs`. And `yunta_testkit_ui::cut`: `TARGET_CUTS`, the committed table from a target's name to its cut (the runs and their seqs, the registry, the node pointed at, the keys pressed), and `Cut`, which loads several runs at their moments into one store, as the field needs.
- Phase 1: the base's measurements and tools in `.plans/yunta-tui/proof/live-screens/`: the suite by name, every golden's digest, `pages.sh`'s captures, the release size by crate, three real fragua runs, `at_rest.py` (the processor at rest), the probe's `frame` (a drawing's time), and `follow_and_answer.py`, copied from `proof/tui/`. The follower's tests of one statement a look where nothing moved (Task 13, Task 17), and its rule in `.agents/architecture.md` (Task 21). The draft pull request.
- Phase 2: the kept field in `FollowedRuns`: each run's state, manifest and engine by head; the store's path; whether the field is this repository's. Also `followed()` naming the field at a reach and refollowing when it changes, the field's pointer kept by run, where the application first stands (a repository with no run on the first request, held in `crates/ui/tests/state.rs`), the first request standing until the person leaves it, and the repository's reading carrying the project's prices (Task 137 adds the repository's path to it where it does not).
- Phase 2: each field run's `FollowedRun::menu`, worked out once for each head. Task 138 keeps beside it the decision `RunFrame::actions` takes, by head, and derives the flag from it.
- Phase 2: `drive` and the interface tests taking followed news until the kept heads equal the log's; the performer made with its channel.
- Phase 3, Surco: the side region at either edge giving way below 102 columns; the selection fill; the legend's far keys from 100 columns (`Screen.anywhere`); the note (`Screen.noted`); the region title's far words; the attention row's cases, one with no key among them (`Answer::Unkeyed`), which a screen where a line is typed takes, unless the run's own decision stands there, where no attention row stands (Task 73, Task 92); the list of choices with a name column and a description column; the row a person types on, in the design's box (`Task 79`); `Horizon`, and the layout of a region that ends with one (Task 78, Task 108); the palette's and help's boxes (corners, title, width); the checklist's row.
- Phase 3, render: `watched::stretch_names` over a workflow's node ids, the ribbon's names by question 3's rule, callable for a mode with no run (Task 95); a stage's keys beyond its legend listed by help, for a run's screen (Task 112).
- Phase 3, `yunta-ui`: one composer of the tally and the attention row, `FollowedRuns::framing` in `crates/ui/src/followed_runs.rs`, which each stage calls (Task 92); `a` going where the attention row names; `q` going to the field, taking the place of a first request that stands at the bottom (Task 93); the field's `said` taking the note a run's `q` and `esc` leave (Task 94); the four sizes in `crates/tui/tests/screens.rs` (Task 96); a run's screens built from `yunta_testkit_ui::cut` by a target's name (Task 90); the probe's `feed`, a drawing at 200×60 of a node of ten thousand calls (Task 123); `tui`'s `run`, `card`, `desk`, `palette`, `help` and `confirmation` frames deleted with their builders (Task 76, Task 80, Task 82, Task 84); Surco's reference brought up to Phase 3's pieces at the path Task 160 names (Task 123).
- Phase 3: where it delivers any of these another way, the task that uses it says the difference in its turn and builds what it lacks.

### Task 123b: a run's frame is derived once for a head of its log

**Description:**
- The person's decision at Checkpoint C. A run's frame (`RunFrame::of`) is derived again at every draw and every key: about 27% of the walk's work (a sample of 20 s) and the part of a drawing that grows with a node's calls (the probe's `feed`: 0.35 ms a drawing at 10,000 calls against 0.28 ms at 100, some 7 ns a call, though only the rows in sight are drawn).
- What a frame says of a log's head is derived once for that head and kept with the run followed; what moves with the clock (an age, how long a node has worked, how long a run has waited) is worked out apart at each draw from what was kept, so a draw with no news folds no event.
- One owner for the kept frame, replaced when the run's head moves; no reader of a frame derives its own. Not a cache keyed by a clock: nothing that moves with time is kept.

**Found in its turn:** A frame no longer holds anything that moves with the clock: `RunFrame::of` takes no `now`, and what the clock adds (a run's elapsed, its time by share, how long since its last event, how long its engine has asked; a node's time at work, its last event's age, a person's time) is `RunTimes` and `NodeTimes` (`crates/replay/src/frame/times.rs`), worked out at each draw from the node records alone, never the log or a node's calls; `stats` shares its two time rules with it. The one owner of a followed run's frame is `yunta_api::FollowedRun` (`crates/api/src/follow/followed.rs`): its fields are private, it keeps the frame derived where it reads the run and derives it again only when the run's head moves or its engine's reading changes, a successor replacing the whole run; `times(now)` and `row(now)` give what the clock adds. Every reader of the application reads it: the run's stage borrows the kept frame and carries the times, a field's row holds the frame and its times, render's functions that print a time take `&RunTimes`, `status --follow` reads the kept frame; the commands that print once derive a frame and its times directly. What a frame reads beside the log is the engine's registry reading, and the history only a printed page sets. Found by sampling the probe: the part of a drawing that grew with a node's calls was not the frame but the node's feed, listed again whole at every draw, and its open attempt's calls counted again; the run in view now keeps each node's listed moments and its count as they arrive, and the feed's window reads the moments in sight. Held by `crates/ui/tests/holding/derived.rs` (a run read whole derives its frame once, a draw with no news derives none, a piece of news one more; red before: five derivations where one was due) with `holding/times.rs` unchanged. Measured: the probe's drawing of a node of 10,000 calls 0.31 ms where it took 0.38 ms, the same as a node of 100 now; the walk's test 118 s where it took 205 s (the walk 79 s for 166 s, user CPU 427 s for 964 s), its counts unmoved (97,626 states, 114 palettes, 63 helps, 10,950 screens at each size); no golden moved. The run path is not reached: nothing in the engine reads a frame. Left as found, given a task of its own: the painter of a run attached to its terminal replays the whole log at every redraw.

**Fails first:** `crates/ui/tests/holding.rs` or `crates/replay/tests/`: two draws of a run with no news between them fold the run's events once (count the folds through a probe the test owns, never a timer); a draw a second later says every time a second on, as `holding/times.rs` holds today.

**Acceptance criteria:**
- [x] The probe's `feed` draws a node of 10,000 calls within a few hundredths of a millisecond of a node of 100; its figures written beside `phase-3-feed.txt`'s.
- [x] The walk's counts do not move (97,626 states, 114 palettes, 63 helps, 10,950 screens at each size) and its time is written beside `walk-fed.txt`'s last entry; every screen golden unmoved.
- [x] Real runs before and after if the change reaches replay on the run path; said if it does not.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-replay -p yunta-ui -p yunta-tui`, the strict documentation build of the packages touched

**Dependencies:** Checkpoint C

**Files likely touched:** `crates/replay/src/frame/mod.rs`, `crates/ui/src/followed_runs.rs`, `crates/ui/src/followed_runs/framing.rs`, `.plans/yunta-tui/proof/live-screens/probe/src/feed.rs` (outside git)

**Estimated scope:** M

### Task 123c: the painter of an attached run folds only what is new

**Description:**
- Found by Task 123b. `yunta run` at a terminal redraws its region once a second and on every event, and each redraw replays the run's whole log (the CLI's foreground painter): a cost that grows with the run inside the process that drives it.
- The painter keeps the run folded as the follower does (`RunState::fold` over what is new, the frame derived when the head moves, the clock's part at each redraw through `RunTimes`), through the same owner the application reads where it can, so a redraw with no news folds nothing.
- Nothing the region shows or the closing prints moves.

**Found in its turn:** The painter of a run attached to its terminal replayed the whole log, from its own copy in memory, at every redraw (once a second, after every batch of news and on every change of the curtain), and listed the whole chronicle again at every batch: 1.8 ms a redraw with no news and 4.0 ms a piece of news on a run of 10,000 events, on the one thread that drives the run. It now keeps the run folded through the owner the application reads (`yunta_api::FollowedRun`, held by `surface/fold.rs`' `Folded` with the events that wait above a hole): a redraw with no news reads no event and derives no frame (under a microsecond), a piece of news folds only itself (0.57 ms at 10,000 events, almost all of it the frame's copy of the open attempt's calls). It uses the followed run alone and not the whole follower, which would poll the log, read the registry and listen on the run's socket inside the process that already hands every append. A hole that outlives a beat is filled by reading only past the head (`EventLog::events_after`). `SurfaceEnv.prior` is gone, read by nothing. A behavior that changed, held by a test of its own: lines owed for a promoted run while a prompt holds the terminal go out when the prompt ends, each under its own run, before its successor's, where they were dropped. Held by `painter/tests.rs`: a redraw with no news reads no event and a piece of news only itself (red on the base: 2,000 events read for two redraws); a painter born for a run just created reads no store to begin. Nothing the region shows or the closing prints moves; no golden moved. On the run path, measured: the engine makes the same appends and awaits; each pass of the painter between them is shorter. Real runs: three with the change (0.626, 0.867 and 0.659 s between nodes) and two pairs before and after one after the other (0.855 and 0.599 s before; 0.745 and 0.765 s after), all beside another session's builds at a load of 4 to 25, so the totals move with the load; gap by gap, seven of the eight equal the earlier runs' to the millisecond wherever the load let them, and the first, from the run's creation to its first node, is the one that holds the start of the detached engine's process. The lead read that first gap as a cost of the change (0.23 to 0.29 s in four of five runs after it against 0.13 to 0.15 s before) and was wrong: with stand-in adapters, the two builds taken in turn, it stands at 113 and 114 ms on a quiet machine and at 226 and 235 ms beside spinning loops for either build (the agent's measure), and at 115 and 117 ms over ten rounds beside the other session's load (the lead's own). The harness is kept at `.plans/yunta-tui/proof/live-screens/task-123c/`. Left as found, for the person at the checkpoint: a frame copies every call of a node's open attempt for the one reader that counts each action over the attempt, so a piece of news costs in proportion to those calls (0.65 ms at 10,000); a tally kept in the node's ledger as calls are folded would close it.

**Fails first:** `crates/cli/tests/run_surface.rs` or the painter's own tests: a run of many events redrawn with no news reads no event (count the events read through a probe the test owns); red at the base.

**Acceptance criteria:**
- [x] A redraw with no news reads no event; every golden and capture of the attached run unmoved.
- [x] Real runs before and after, since the painter lives in the process that drives the run: the engine's time between nodes written beside `real-c-*.txt`'s.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test run_surface --test live_motion --test console_interaction`, the strict documentation build of the packages touched

**Dependencies:** Task 123b

**Files likely touched:** `crates/cli/src/surface/`, `crates/cli/tests/run_surface.rs`

**Estimated scope:** S

### Task 124: the typed answer, in the design's box

**Description:**
- **`surco::components::TypedAnswer`** (`components/typed_answer.rs`) gains what the design draws around the request (`design/empty.wide.txt`, `compose.wide.txt`): `placeholder: Option<Line>`, which stands faint inside the box while nothing is typed, and `hint: Option<Line>`, which stands muted under the box. The box itself is the row a person types on, which Phase 3 draws in the design's box (`Task 79`, `TypedRow` in `surface/terminal/typed.rs`); this task draws no box of its own.
- The terminal draws them in `surface/terminal/asked.rs`: what is asked, the boxed row with the placeholder in it while nothing is typed and the cursor on the first cell inside the box, then the hint where there is one. Markdown and HTML say the placeholder where nothing is typed and the hint after the typed words.
- The first request and a new run's request set them (Task 141, Task 142). The CLI's line prompts (`render::prompt::typed_answer`) set neither, so what `prompts.py` captures does not change. Every other construction of a `TypedAnswer` takes none, the compiler naming each.
- `components/typed_answer.rs`'s `//!` says what the placeholder and the hint are for. The catalog's typed answer shows both, so the catalog's goldens are blessed.

**Found in its turn:** `TypedAnswer` gains `placeholder` and `hint`, each `Option<String>` (the component sets their tones, faint and muted, as the card's `asks` and a row's `said` are strings): the terminal draws the question, the box with the placeholder faint in it while nothing is typed and the cursor on the cell where typing lands, then the hint wrapped under itself at the look's width; Markdown writes the placeholder as a quoted line and the hint as a paragraph, HTML alike. `TypedRow::is_empty` is the one rule for "nothing typed", read by the box, Markdown and HTML. Phase 3 had built the box and the placeholder faint inside it for the card and the desk; this adds the two fields, the hint and the two other surfaces' forms. `render::prompt::typed_answer` sets neither field, so every line the CLI asks draws as before (the six captured prompts unchanged). Held against the design's own rows (`empty.wide.txt` at 72 cells, `compose.wide.txt` at 71). Goldens: `typed_answer/requested.*` new in the five environments, Markdown and HTML; the catalog's typed answer is the request with both. For the tasks that draw the screens: `setup-changing`'s model box carries its label in its top edge and is 32 cells wide, narrower than its hint wraps at, which `TypedAnswer` does not draw yet; on `new-run` at 60 columns the question and the hint give way and the box stays, so the screen leaves them out.

**Fails first:** `crates/surco/src/components/typed_answer/tests.rs`:
- the placeholder shows, faint, only while nothing is typed, and the cursor stands on the first cell inside the box;
- a hint stands under the box, muted, and none where there is none.

Red: `TypedAnswer` has no placeholder and no hint.

**Acceptance criteria:**
- [x] `crates/surco/goldens/typed_answer/` gains a typed answer with a placeholder and a hint (ASCII-80, Unicode-60, the 16-color layer that holds its faint placeholder, Markdown, HTML), reviewed in the diff; its rows equal `design/empty.wide.txt`'s for the same words at 120 cells. The goldens with neither are unchanged.
- [x] `prompts.py`'s captures equal Phase 3's.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco -- typed_answer asked pane catalog`, `cargo test -p yunta --test console_interaction`
- [x] `YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine`, then the goldens' diff reviewed.

**Dependencies:** Phase 3's boxed row (`Task 79`) and Checkpoint C.

**Files likely touched:** `crates/surco/src/components/typed_answer.rs`, `crates/surco/src/components/typed_answer/tests.rs`, `crates/surco/src/surface/terminal/asked.rs`, `crates/surco/src/surface/terminal/pane/tests.rs`, `crates/surco/src/surface/markdown/asked.rs`, `crates/surco/src/surface/html/asked.rs`, `crates/render/src/prompt.rs`, `crates/surco/goldens/{typed_answer,components}/`

**Estimated scope:** M. Two fields added to a component reach every construction of it, which the compiler names.

### Task 125: a row of words with one chosen and marked

**Description:**
- **`surco::components::WordChoice { words: Vec<String>, chosen: usize, focused: bool }`**, in `components/word_choice.rs` (new): words in one row, apart, the chosen one marked (the pointer before it, and strong) and the rest muted. `///`: "Not `ChoiceList`, a list a pointer moves along, row after row."
- `Block::WordChoice`, drawn in `surface/terminal/word_choice.rs` (new). A row too narrow for every word keeps the chosen one in sight, with the ellipsis on the side that lost words. Markdown names the chosen word.
- The spec's "The mode chosen on a new run is marked, not only filled" holds on every layer: the mark is a glyph, not a fill.

**Found in its turn:** `WordChoice { words, chosen, focused }` and `Block::WordChoice`: on a terminal a blank cell, then each word after a pointer cell and a cell, so no word moves when the choice does; the chosen word strong with the pointer before it, the rest muted; the mark is the glyph on every layer and nothing is filled. `focused` says whether the keys act on the row: the pointer then takes a moved pointer's tone, muted otherwise, the text the same. A row too narrow starts at the earliest word that leaves the chosen one room, an ellipsis on each side that lost words, a chosen word wider than the row cut at its end. It draws the targets' rows (` ▸ quick   standard   full`; the adapters' row, which fits 60 columns' 58 cells exactly; the setup's candidates). Markdown writes `quick · ▸ **standard** · full`, HTML the same with the others muted. Goldens `word_choice/{modes,adapters}.*` in the five environments, Markdown and HTML; the catalog holds 42 kinds. Beside the task's files: the row's Markdown and HTML writers stand with the other screen pieces (`markdown/screen.rs`, `html/screen.rs`); a diagram's fence moved into `markdown/code.rs` and the horizon says how it stands in a document, two functions the new arm had pushed past fifty lines.

**Fails first:** `surface/terminal/word_choice/tests.rs` (new):
- the chosen word carries the pointer in Unicode and in ASCII, so a plain layer still says which is chosen;
- at 30 cells, a row of six words keeps the fifth, when it is chosen, in sight.

**Acceptance criteria:**
- [x] Goldens `crates/surco/goldens/word_choice/` at ASCII-80, Unicode-60 and the 16-color layer (strong against muted), reviewed; the catalog's goldens blessed with the new kind.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco word_choice`, `cargo test -p yunta-surco catalog`

**Dependencies:** Task 124 (both bless the catalog's goldens)

**Files likely touched:** `crates/surco/src/components/{mod.rs,word_choice.rs (new)}`, `crates/surco/src/doc.rs`, `crates/surco/src/surface/terminal/{mod.rs,word_choice.rs (new),word_choice/tests.rs (new)}`, `crates/surco/src/surface/markdown/mod.rs`, `crates/surco/src/surface/html/blocks.rs`, `crates/surco/src/components/catalog/kinds.rs`, `crates/surco/goldens/{word_choice (new),components}/`

**Estimated scope:** M. A new block kind: each surface matches every block, so the compiler names every surface this touches.

### Task 126: the name in block letters

**Description:**
- **`Block::Wordmark`** (`components/wordmark.rs`, new): the name `yunta` in block letters, two rows, centred in its region.
- It is drawing alone. ASCII draws nothing in its place, and no layer loses a fact: the trail names `yunta`. The letters' two rows are glyphs, written in `glyphs.rs`. Markdown and HTML leave it out.
- The field in perspective under it is Phase 3's `Horizon`, laid out by Phase 3's rule for a region that ends with a horizon (Task 108). The first request and the setup reuse both as they are; this task changes no layout.
- `///` on the block: "drawing alone: a layer without it loses no fact".

**Found in its turn:** `Block::Wordmark`, a unit kind drawn by `Terminal::wordmark()` from the two rows `Glyphs::wordmark()` writes: strong, each centred in its region, the odd cell at the left; nothing in fewer than 20 cells; Markdown and HTML leave it out, since it is drawing alone. It draws the first request's rows at 118, 78 and 58 cells and the target's own at 120. The lead's ruling: in ASCII the code follows the targets, `yunta` on the first row and the second blank, as `first-request.ascii-120`, the setup's three ASCII frames and the design's own ASCII frame draw it, where the task's text said nothing stands. The catalog holds 43 kinds; goldens `wordmark/name.*` at four environments. The glyph set's policy moved to `glyphs/policy.rs` (the file passed 500 lines), re-exported by its module as Surco's modules do.

**Fails first:** `surface/terminal/wordmark/tests.rs` (new):
- in Unicode, the wordmark is two rows centred at 120 and at 60 cells;
- in ASCII it draws nothing.

**Acceptance criteria:**
- [x] Goldens `crates/surco/goldens/wordmark/` at ASCII-80, Unicode-60 and Unicode-120, reviewed; the catalog's goldens blessed.
- [x] `cargo xtask smells --check` holds: no glyph outside `glyphs.rs`.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco -- wordmark catalog`
- [x] `cargo xtask smells --check`

**Dependencies:** Task 125

**Files likely touched:** `crates/surco/src/components/{mod.rs,wordmark.rs (new)}`, `crates/surco/src/doc.rs`, `crates/surco/src/glyphs.rs`, `crates/surco/src/surface/terminal/{mod.rs,wordmark.rs (new),wordmark/tests.rs (new)}`, `crates/surco/src/surface/markdown/mod.rs`, `crates/surco/src/surface/html/blocks.rs`, `crates/surco/src/components/catalog/kinds.rs`, `crates/surco/goldens/wordmark/ (new)`

**Estimated scope:** M A block is one step across every surface: each surface matches every block, so the compiler names them.

### Task 127: a confirmation's box

**Description:**
- **`surco::components::SameAtAShell(Vec<String>)`**, in `components/next.rs`: the commands that do at a shell what an act here does, each on its row, muted, with no heading and no gloss. `///`: "Not `Next`, the steps a page suggests with what each does."
- **`yunta_ui::dialog::Confirmation::overlay`** draws the design's box from what a confirmation holds: what it asks, a blank row, then `SameAtAShell` in place of `Next` with its gloss, and the confirmation's legend. `render::screen::DOES_THE_SAME` goes with the gloss it was. `Confirmation::new` and its callers stay as they are; the question as the box's title and each act's words are Task 132's.
- The box's width is the confirmation's, as the targets draw it, through Phase 3's rule for an overlay's width (`crates/tui/src/layout.rs`).
- Every confirmation the application asks takes the box: the field's and a run's cancel and close, a pack's removal and install, doctor's sessions, upkeep's collection, and the setup's pack.

**Found in its turn:** `SameAtAShell(Vec<String>)` and `Block::SameAtAShell`: each command on a row of its own, muted, with no heading and no gloss; Markdown writes a list of code items, HTML copy buttons. `Overlay` gains `confirms` and `Overlay::confirming()`: a confirmation's box holds its words one cell inside each upright, a blank row at its foot that gives way first, and stands 64 cells wide (`layout::CONFIRMATION`; 56 at 60 columns). `Confirmation::overlay` builds what it asks as a line at the box's first cell, a blank row, then the commands; render's `screen::DOES_THE_SAME` is gone. Every confirmation the application asks draws so: before, a box of some 58 cells titled `confirm` with the sentence three cells in, a `next` heading and the command glossed `does the same at a shell`; its title and its legend (`enter confirm · esc go back`) stay until each confirmation's own words are given. The lead's rulings on a command laid in a box (`surface/terminal/next.rs`, the one place): it folds only between two of its words, the row it leaves ending ` \`; a word that does not fit the words' width stands whole where the one cell before the upright lets it (`Width::lending`, `Width::reach`), which draws `setup-confirm.unicode-60`'s row 8 as its target has it, where the code parted the link inside its version; a word wider even than that is cut by the one cut rule, and only where a row cannot pass its edge (`Width::cuts()`: a region's width), since on a printed page a command's word stands whole past the page's width and the terminal wraps it, as before (the lead's first ruling held the page to the cut and was wrong). Goldens `same_at_a_shell/{closing,installing}.*` and `boxed/confirmation.*`; the catalog holds 44 kinds. The walk finds a confirmation by its title until its phrases land (`Overlay.confirms` can tell it then); its counts stand: 97,626 states, 114 palettes, 63 helps, 10,950 screens at each size, the test 128 s.

**Fails first:**
- `crates/surco/src/surface/terminal/next/tests.rs` (new): `SameAtAShell` draws each command on its row with no `next` heading and no gloss.
- `crates/ui/tests/dialogs.rs`: a confirmation's overlay names each command on its own row and says no `does the same at a shell`.

Both are red before the change: `Next` draws its heading and gloss.

**Acceptance criteria:**
- [x] Goldens `crates/surco/goldens/same_at_a_shell/`, reviewed.
- [x] No golden in `crates/tui/goldens/screens/` changes: no target Phase 3 landed draws a confirmation.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco next`, `cargo test -p yunta-ui --test dialogs`, `cargo test -p yunta-ui --test walk`, `cargo test -p yunta-tui --test screens`

**Dependencies:** Phase 3's Checkpoint C. Its files are disjoint from Task 124's; it shares `doc.rs` and the surfaces' matches with Task 125, Task 126 and Task 128, so it runs after Task 126.

**Files likely touched:** `crates/surco/src/components/next.rs`, `crates/surco/src/surface/terminal/{next.rs,next/tests.rs (new)}`, `crates/surco/src/doc.rs`, `crates/surco/src/surface/markdown/mod.rs`, `crates/surco/src/surface/html/blocks.rs`, `crates/surco/src/components/catalog/kinds.rs`, `crates/ui/src/dialog/confirmation.rs`, `crates/render/src/screen.rs`, `crates/ui/tests/dialogs.rs`, `crates/surco/goldens/same_at_a_shell (new)`

**Estimated scope:** M. A new block kind: each surface matches every block, so the compiler names every surface this touches.

### Task 128: the glance's rows

**Description:**
- **`surco::components::AtAGlance { facts: Vec<(&'static str, String)>, acts: Option<(String, Vec<Binding>)> }`**, in `components/at_a_glance.rs` (new): a run seen at a glance beside a list. Each fact has its label in a column as wide as the widest label and its value wrapped under itself. Then the acts it offers under their heading, one key a row, the key in a column as wide as the widest key. `///`: "Not `Fields`, a page's facts in a 12-cell label column; not `Depth::Glance`, how much a page says."
- `Block::AtAGlance`, drawn in `surface/terminal/at_a_glance.rs` (new). A value wraps between words and never cuts a number. An act's row is cut at its edge with the ellipsis, its key kept whole.

**Found in its turn:** `AtAGlance { facts, acts }` and `Block::AtAGlance`: on a terminal the facts two cells in, the label muted in a column of twelve cells and one more, the value after it, wrapped between words under its own first cell (a word wider than the row cut by the one rule); a fact with no value is no row; then a blank row, the acts' heading muted, and a key a row, the key strong and whole, what it does cut at the edge. Markdown writes the facts as items and the keys under their heading; HTML a list of fields and key buttons; both share one writer of keys with `KeyList`, and the terminal shares its key column. The lead's rulings where the targets and the task's text disagreed, for the targets: the label column is twelve cells at least (every target puts the value at one column whether the widest label has seven cells or eight) and widens only for a wider label; the key column is help's (seven cells, or the widest key and two). Held against the field's own glance at 39 cells (`field.unicode-120` rows 7 to 20 from column 81). Goldens `at_a_glance/{asking,waiting}.*`; the catalog holds 45 kinds.

**Fails first:** `surface/terminal/at_a_glance/tests.rs` (new):
- at 40 cells, `27M in / 78.4k out (26M cached)` wraps under its value's column with no figure split;
- the acts' keys align, `enter` the widest;
- a fact with no value is no row.

**Acceptance criteria:**
- [x] Goldens `crates/surco/goldens/at_a_glance/` at ASCII-80, Unicode-60 and the 16-color layer (labels muted, keys strong), reviewed; the catalog's goldens blessed.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco at_a_glance`, `cargo test -p yunta-surco catalog`

**Dependencies:** Task 126

**Files likely touched:** `crates/surco/src/components/{mod.rs,at_a_glance.rs (new)}`, `crates/surco/src/doc.rs`, `crates/surco/src/surface/terminal/{mod.rs,at_a_glance.rs (new),at_a_glance/tests.rs (new)}`, `crates/surco/src/surface/{markdown/mod.rs,html/blocks.rs}`, `crates/surco/src/components/catalog/kinds.rs`, `crates/surco/goldens/at_a_glance (new)`

**Estimated scope:** M. A new block kind, as Task 125. Each surface matches every block, so the compiler names every surface this touches.

### Task 129: a run a row

**Description:**
- **`surface/terminal/runs.rs`** draws a run's furrow at one cell a node, in a column of 16 cells that starts at the same cell on every row. An over-full furrow gives each cell the most urgent word it covers, as `Furrow` does. A short row cuts what holds the run before it drops the furrow. Its `//!` says the rule; `RunLine`'s `///` says the column is fixed.
- **`RunLine`** names the mode alone where the field draws it. `render::page::field`'s row says the mode and, at the far edge, the age, a stalled or stopped row among them. The command that moves a run on is the glance's (Task 131) and the printed page's next steps.
- What `yunta list --runs` prints changes with it: its rows, as the inventory's rows F3 say.
- **`tui`'s base frame `field` goes**, since this task first changes what it draws: `crates/tui/goldens/screens/field.{ascii-80,unicode-60}.txt`, its builder `yunta_testkit_ui::screens::field` and its entries in `crates/tui/tests/screens.rs` and `crates/testkit-ui/tests/screens.rs`. `crates/ui/tests/state.rs` compares with `screens::walked_to(&[])`, which the builder was. Until Task 137 lands the field's target, the walk's layout checks hold the field.

**Found in its turn:** A run's row reads its handle, what names its kind, its furrow, what holds it and, at the far end, its age; every field row ends in its age, and the commands the stalled and stopped rows ended with stand among the page's next steps, in the words of what each does (`FieldRow::step()`). The lead's rulings: the furrow is the targets' (sixteen cells, two a node at most, a gap between strokes), where the task's text said a cell a node; and what a column holds depends on where the row is drawn, which the surface tells by its width (`Width::cuts()`): in a region of a screen the mode alone in ten cells and the furrow in sixteen, as the targets draw them, the glance naming the workflow of the run pointed at; on a printed page the workflow with its mode (`RunLine::name()`, the one place it is worded), since a page has no glance and a list that does not say which workflow a run is of has lost a fact, in a column as wide as the widest name (a quarter of the line at most), and the furrow in a column as wide as the page's widest furrow, sixteen at most, so what holds a run keeps the cells; `runs::LinedUp` lines the handle, the name, the furrow and the edge up across every list of a document. On a short row what holds the run gives way to its floor of sixteen cells before the furrow gives up its column. What `yunta list --runs` and `yunta status needs` print, against the last phase: the name column as it was, the furrow two cells a node with its gaps (`━━ ┄` for `━━┄┄`), what holds a run a cell or two longer at 100 and 60 columns, every row ending in its age with the stalled and stopped runs' commands among the next steps. The base's two `field` frames and their builder are gone; the field's four targets stay in `targets/` for the tasks that land them; `crates/testkit-ui/tests/screens.rs`, whose one test drew the field, is gone, and tui's own test of a terminal short of room draws from the walk's home. A test of the CLI that Task 127 had left failing (it waited for words the confirmation's box now folds) waits on the question that opens the confirmation, through render's constant; 100 runs under load, none failing. Markdown's table and HTML's cell name the workflow with its mode. The walk's counts stand (97,626 states; a region's row is what it was).

**Fails first:**
- `crates/surco/src/surface/terminal/runs/tests.rs`: two runs of 15 and 4 nodes draw furrows that start at one cell and take 15 and 4 of 16 cells; at 60 cells the furrow stands and what holds the run is cut.
- `crates/render/src/page/field/tests.rs`: a stalled row ends in its age.

Both are red at the base: two cells a node capped at a fifth of the width, and a command at the edge.

**Acceptance criteria:**
- [x] `crates/render/goldens/field/` and Surco's runs goldens blessed, their diff reviewed, each difference one the inventory's rows F3 name.
- [x] Every CLI test that reads a `list --runs` row (`run_inbox_cmd.rs`, `stalled_runs.rs`, `parked_runs.rs`, `close_cmd.rs`, `surface_parity.rs`, `three_questions.rs`) passes, each assertion it changes named in the task's commit.
- [x] `field.*`, `screens::field` and their entries are gone, and the walk lays the field out at both its sizes.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco runs`, `cargo test -p yunta-render field`, `cargo test -p yunta --test run_inbox_cmd --test stalled_runs --test parked_runs --test close_cmd --test surface_parity --test three_questions`, `cargo test -p yunta-tui --test screens --test walk`, `cargo test -p yunta-testkit-ui --test screens`, `cargo test -p yunta-ui --test state`

**Dependencies:** Phase 3's Checkpoint C. Disjoint from Task 124 to Task 128, so it can run beside them.

**Files likely touched:** `crates/surco/src/components/runs.rs`, `crates/surco/src/surface/terminal/{runs.rs,runs/tests.rs}`, `crates/render/src/page/{field.rs,field/row.rs,field/tests.rs}`, `crates/render/goldens/field/`, `crates/testkit-ui/src/screens.rs`, `crates/tui/tests/screens.rs`, `crates/testkit-ui/tests/screens.rs`, `crates/ui/tests/state.rs`, `crates/tui/goldens/screens/`

**Estimated scope:** M. The base frame goes with its builder and every caller of it, which the compiler names.

### Group check: Surco's pieces
- [x] `python3 .plans/yunta-tui/tools/suite.py` for Surco, render and the CLI green; `goldens.py` against Phase 3's digests: only the new pieces, the typed answer's, the catalog's, the runs' and field's goldens and `same_at_a_shell`'s changed; in `crates/tui/goldens/screens/` only `field.*` went, and nothing else changed; `pages.sh` against Phase 3's captures: only `list --runs`' rows differ.

**Found in its turn:** the gate's twelve steps pass (`.plans/yunta-tui/proof/live-screens/check-surco-4/`): 328 test binaries, 5,450 tests, none failing, none aborted; its test step took 2,059 s beside another session's builds, the tests' own times summing to 1,220 s. The catalog holds 45 kinds of block, six of them this group's (`WordChoice`, `Wordmark`, `SameAtAShell`, `AtAGlance`, and the typed answer and the run's row redrawn). The walk's counts stand: 97,626 states, 114 palettes, 63 helps, 10,950 screens at each size. The release binary weighs 31,575,632 bytes. Of the 159 captured views, 12 differ from the last phase's (`check-surco-4-prints/` against `phase-3-prints/`), each by Task 129's row: the five of `yunta list --runs` and the five of `yunta status needs` (the furrow two cells a node with its gaps, the stalled and stopped rows ending in their age with their commands among the next steps, the workflow's name kept), and the two screens that show the field (`screen-bare`, `screen-review-esc`), whose row reads the mode alone and the furrow in sixteen cells, as the targets draw it. Under 102 columns the field has no glance, so the row's `default` is all it says of a run's workflow there, as the approved targets at 80 and 60 columns draw it.


### Task 130: the field's phrases

**Description:**
- **`yunta_render::page::FieldRows`**, in `page/field.rs`: the field as a screen lists it.
  - The first row (F1, `› what do you want to build?  · n`).
  - Then render's groups: `asking now`, `needs you, oldest first`, `stalled`, `stopped`, `running`, and `closed` as deep as `depth` asks.
  - Then `unreadable`, each run with why (F2), the pointer and the marks.
  - No headline and no next steps. `///`: "Not `Field`, the page `yunta list --runs` prints."
- `Field` and `FieldRows` draw their rows through one `groups()`. What the inventory leaves to the printed page (its headline, its footer, its next steps) stays on `Field`, its next steps in the palette's words where the palette words the act (`this repository's runs, the closed too`).
- **`render::screen::field`:**
  - the field's legend in the design's words (`j k move · enter open · n new run · x close · q quit`);
  - the trail's place, the repository's path as `surco::paths::shown` gives it;
  - the note `r` leaves on a run with nothing to resume (`Nothing to resume: this one waits on a decision. Enter reads it.`);
  - the words of every key out of the legend, which help says (`space`, `r`, `d`, `e`, `a`, `c`, `w`, `m`).
- **`render::command::does`:** the field's palette rows in the design's words (`start a new run`, `every run of this repository`, `the workflows you can run`, `check adapters, git, forge, packs`, `remove orphaned checkouts`). Those it shares with a run's palette are Phase 3's words already.
- The inventory's rows for the field's phrases close here, each owned once in render.

**Found in its turn:** `page::FieldRows` is the field as a screen lists it (its first row, `› what do you want to build?  · n`; its groups; the unreadable runs, a row each with why), and `Field`, the printed page, draws its groups through it and keeps its headline, footer and next steps. A group's word has one owner (`FieldGroup::word()`): `closed` for `closed, newest first`, `unreadable` for `could not be read`. A closed run's row says its gist. The field's legend is the design's five keys (`j k move · enter open · n new run · x close · q quit`), where it named twelve; render gains, for the task that wires the field's keys, help over the field (`help_over_the_field(machine, closed)`, the sixteen rows of `field-help`, `d` and `e` saying the turned-back act) and the trail's place (`field_place`). `command::does` says `go to what needs you`, `this repository's runs, the closed too` and `every run on this machine`, each literal elsewhere now reading it. Where a target said more than the task's text, the target governs: help's `s what it cost` and `x close the marked, or the pointed, once confirmed`; the palette's rows for `a`, `e` and the closed too. What commands print: the list's next step `lists the closed runs too` → `this repository's runs, the closed too`; with `--full`, the two headings and a closed row's gist (the JSON's `said` alike). The lead's ruling on the walk: two fields that differ in what they are asked to show drew alike once the legend stopped saying it and were merged (97,626 states fell to 96,368); the fingerprint carries the depth of the field the person is at (`State::field_depth()`, hashed directly; its reach was already in what a state follows), and help over the field is told apart by its depth and reach, so the walk reaches 97,626 states, 114 palettes and 63 helps again; it draws 10,174 screens at each size where it drew 10,950, the 776 gone having differed only by the old legend's words. Left to the tasks that wire the field: until then its legend names `n new run` while `n` opens the run that waits, and `space`, `r`, `c`, `a`, `d`, `e`, `w` and `m` act with no row naming them.

**Fails first:** `crates/render/src/page/field/tests.rs`:
- `FieldRows` over a field with a run asking, two needing the person, one stalled, one unreadable lists the first row, then the groups in order, then `unreadable` with each run's why, with no headline and no `next`;
- the legend says the design's five keys.

The first is red until `FieldRows` exists; the second says today's words.

**Acceptance criteria:**
- [x] Each of the inventory's rows for the field's phrases stands in render once (`grep` finds each phrase once in `crates/render/src`, or once in `crates/surco/src/words.rs` for a state word).
- [x] `crates/render/goldens/field/` blessed only where the inventory changes a group's words; `yunta list --runs`' capture differs only there.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render field`, `cargo test -p yunta-render screen`
- [x] `python3 .plans/yunta-tui/tools/targets_check.py crates/tui/goldens/screens` passes: the targets already landed still hold their rows.

**Dependencies:** Group check: Surco's pieces

**Files likely touched:** `crates/render/src/page/{field.rs,field/unreadable.rs,field/tests.rs}`, `crates/render/src/screen/field.rs`, `crates/render/src/command.rs`, `crates/render/goldens/field/`

**Estimated scope:** M

### Task 131: the glance's phrases

**Description:**
- **`yunta_render::page::RunAtAGlance`**, in `page/field/glance.rs` (new): the run the field points at, at a glance. Its parts:
  - its headline (mark, workflow and mode, standing, handle at the far edge);
  - its furrow drawn apart, without names;
  - its gist (`state::gist`);
  - `AtAGlance`'s facts: `progress`, `tokens`, `time`, `slowest`, `branch` for a run at work or ended, and `waiting` for one that waits;
  - `what you can do`, one act a row.
- `///`: "Not `Run`, the page `yunta status` prints."
- The facts' rows share their words with `page::run::facts`. What a row says on both is said once, in `facts.rs`, so `yunta status` says it the same way.
- **The acts' words** are `command::effect`'s: the palette's where the palette words the act (Task 86), the glance's where only the glance does (`continue it where it stopped`), and today's where neither does (`waits for the reset, then drives on`, `hands it back once that is settled`, `reads its plan, and answers it there`). `read the decision`, `answer what it asks` and `what it cost` are the words of the glance's own keys, `enter` and `s`, which no effect says. `yunta status`' next steps say the same words.
- The inventory's rows F4 close here.

**Found in its turn:** `page::RunAtAGlance` (`page/field/glance.rs`): a run read (its headline, furrow and gist, then `AtAGlance` with `progress`, `tokens`, `time`, `slowest` and `branch` for a run at work or ended, or `waiting` and `branch` for one that waits) or a run that could not be read; `what_you_can_do()` gives the acts alone where the glance gives way. `GlanceAct` says each act; the keys stay the caller's. Every fact's label and value is said once (`page/run/facts.rs`), and `yunta status`, a run's screen and the watched region read them: a value of several parts takes `, ` between them, as the glance's targets say it, so status, the closing and `open` print `…, 1 reroute`, `X at work, Y waiting on you` and `a, b, c` where they joined them with ` · ` (60 goldens of render, those rows alone). An act is said as an order in a next step too: `continue it where it stopped` for `continues it where it stopped`, in `yunta cancel`'s page, the JSON's `next[].why` and MCP. Where a target governs: the glance says `r hand it back`, as a standstill's legend does, while `yunta status` keeps `hands it back once that is settled`, as its approved print has it; the asking run's gist is the row's, without the pid.

**Fails first:** `crates/render/src/page/field/glance/tests.rs` (new):
- a run that needs the person, a stalled run, a stopped run and a finished run each have the facts and acts their targets carry;
- the stalled one's acts read `r continue it from its log` and `x close it for good`.

Red until the page exists.

**Acceptance criteria:**
- [x] `crates/render/goldens/status/` and `closing/` blessed where an effect's words change, their diff reviewed, each difference one the inventory's rows name.
- [x] `pages.sh`' `yunta status` captures differ only in those words.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render glance`, `cargo test -p yunta-render status`, `cargo test -p yunta --test status_page_cmd --test status_cmd`

**Dependencies:** Task 130 (both change `command.rs`)

**Files likely touched:** `crates/render/src/page/{field.rs,field/glance.rs (new),field/glance/tests.rs (new),run/facts.rs}`, `crates/render/src/command.rs`, `crates/render/goldens/{field,status,closing}/`

**Estimated scope:** M

### Task 132: a confirmation's phrases

**Description:**
- **`render::screen`** words every confirmation the application asks, each with:
  - its question (`close EZRPE9?`; several named by `surco::text::listed`);
  - what the act does, in the design's sentence for closing (`It closes as cancelled, by you, and stops waiting on a person. Its branch and worktree stay until gc removes them.`);
  - its legend (`enter close it · esc keep it`).
- Each act gets its own: closing and cancelling runs, removing a pack, installing one (S4), doctor's sessions, upkeep's collection. `screen::closing` and `cancelling` become these.
- **`yunta_ui::dialog::Confirmation::new`** takes the question, the sentence, the commands, the legend and the work, and `overlay` titles Task 127's box with the question (`close EZRPE9?`) and puts the act's own legend under it. `render::screen::CONFIRM` and `confirmation_legend` go with the title and legend they were. Every caller passes its act's words: the field's and a run's cancel and close, a pack's removal and install, doctor's sessions, upkeep's collection.
- **`page::Closed`**, what `yunta close` prints, says what closing leaves in the same words as the confirmation's second sentence, through one function in `screen/field.rs`. So `yunta close` changes with it.
- **The targets `close-run` and `cancel-run` land** (the confirmations to close and to stop, over a run's screen): moved (`git mv`) from `crates/tui/goldens/targets/` into `crates/tui/goldens/screens/`, held at the four sizes by `crates/tui/tests/screens.rs`, its state from `yunta_testkit_ui::cut` by its name, as Phase 3's screens take theirs (Task 90).
- The inventory's row E5 and the confirmation rows of S4 close here.

**Found in its turn:** Every confirmation of the application says its own question as its box's title, its own sentence, the commands that do the same, and its own legend. The acts confirmed are one closed set in render, `screen::Confirming` (`Close`, `Stop`, `RemovePack`, `InstallPack`, `OpenSessions`, `Collect`), each giving its question, sentence and legend; the legends' words are `PromptAct`'s (`close it`, `stop it`, `remove it`, `install it`, `open them`, `collect it`, `keep it`); `Confirmation::new` takes the one value. `prompt::confirming_legend()` stays for the plan's desk alone. Landed and held whole at the four sizes: `close-run` and `cancel-run` (62 in `LANDED`). The lead's ruling on what a close confirms: the design's second sentence, `Its branch and worktree stay until gc removes them.`, is not what a close does (it gives the run's checkouts back to the project and keeps its branch: `crates/api/src/ending.rs`), and a screen never says what the engine does not do, an approved target included; the confirmation says `Its branch stays until gc removes it; its checkouts go back to the project.` after the design's first sentence, agreeing in number with the runs it closes (`screen::what_a_close_does`: `They close… Their branches stay…`), and the eight frames that carried the old sentence are corrected (`close-run`'s four, written from what the code draws: the box a row taller at 120 and 80 columns, its top row where it was; `field-close`'s four, derived by the same rules for the task that draws it), each row listed before and after for the person; the inventory's row marks the sentence's second half as made up. `yunta close` prints what it printed (`closed by X: its checkouts went back to the project, and its branch stays until \`yunta gc\` removes it.`): what happened and what will happen are two sentences. The walk tells a confirmation by `Overlay.confirms`, where it read the title `confirm`, and fails when one is skipped (seen with the field's close skipped in place); its counts stand: 97,626 states, 114 palettes, 63 helps, 10,174 screens at each size. Words no target gives, for the person: a pack removed (`remove P?`, `Its files and its lock entry go from this project.`, `enter remove it · esc keep it`), a pack with executors installed (`install P?`, `enter install it · esc go back`), sessions opened (`open a session per binding?`, `It spends a prompt each.`, `enter open them · esc go back`), what a dry run found collected (`collect what the dry run found?`, `What it lists is removed for good.`, `enter collect it · esc keep it`). Left to their tasks: `field-close`'s note line; the setup's confirmation, with its own sentence and `esc write without it`.

**Fails first:**
- `crates/render/src/tests/screen.rs`: closing two runs asks `close EZRPE9 and M7WQY4?`, says the design's sentence and the legend `enter close it · esc keep it`; `Closed` ends with the same second sentence. Red: today `close X for good? A closed run is not resumed.`.
- `crates/ui/tests/dialogs.rs`: closing a run titles its box with the question. Red: the title is `confirm`.
- `crates/tui/tests/screens.rs` with `close-run` and `cancel-run` against their targets, red until the box holds these words.

**Acceptance criteria:**
- [x] `close-run` and `cancel-run` equal their targets at the four sizes.
- [x] `yunta close`' capture in `pages.sh` and the CLI's `close_cmd.rs` say the shared sentence, their diff reviewed.
- [x] No `CONFIRM` is left in `crates/` (`grep`); the walk's check of a confirmation finds it by what `Confirmation` holds.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render screen`, `cargo test -p yunta --test close_cmd`, `cargo test -p yunta-ui --test dialogs --test walk`, `cargo test -p yunta-tui --test screens`

**Dependencies:** Task 127; after Task 130, since both word `screen/field.rs`. Disjoint from Task 131.

**Files likely touched:** `crates/render/src/screen.rs`, `crates/render/src/screen/{field.rs,catalog.rs,machine.rs}`, `crates/render/src/page/closed.rs`, `crates/render/src/tests/screen.rs`, `crates/ui/src/dialog/confirmation.rs`, `crates/ui/src/stage/{field.rs,doctor.rs,upkeep.rs}`, `crates/ui/src/stage/run/acts.rs`, `crates/ui/src/stage/catalog/{packs.rs,packs/install.rs}`, `crates/ui/tests/{dialogs.rs,walk/dialogs.rs}`, `crates/tui/tests/screens.rs`, `crates/tui/goldens/screens/`

**Estimated scope:** M. `Confirmation::new` changes with every caller, which takes its act's words; the compiler names each.

### Task 133: the first request's phrases

**Description:**
- **`yunta_render::page::FirstRequest`** (new, `page/first_request.rs`): the question (`What do you want to build?`), the placeholder its box shows while nothing is typed, then each workflow the catalog offers with what it does, a row each. `///`: "Not `NewRun`, the screen a request goes on to."
- Its legend's words say only keys that do not type (the spec corrects the design's first-request legend), and no far pair, since a line is typed.
- The inventory's rows S1 close here.

**Found in its turn:** `page::FirstRequest { before, after, catalog }`: the question over its box with its placeholder, then each workflow that reads with what it does, in the catalog's order. The question has one owner (`FirstRequest::QUESTION`), which the field's first row says in lower case. The legend is `type what you want to build · enter new run · esc quit` (`screen::first_request_legend`, true of today's keys); `screen::first_run_asks()` is gone. The page is drawn by the task that lays the first request's screen out, which also places the name in block letters, the horizon and the workflows' sixteen-cell column.

**Fails first:** `crates/render/src/page/first_request/tests.rs` (new):
- `FirstRequest` lists each workflow with its description, in the catalog's order;
- its legend names no printable key.

Red until the page exists.

**Acceptance criteria:**
- [x] Each of the inventory's rows S1 stands in render once (`grep`).

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render -- first_request`

**Dependencies:** Group check: Surco's pieces. Disjoint from Task 130 to Task 132.

**Files likely touched:** `crates/render/src/page/{first_request.rs (new),first_request/tests.rs (new)}`, `crates/render/src/page.rs`

**Estimated scope:** S

### Task 134: a new run's phrases

**Description:**
- **`yunta_render::page::NewRun`** (new, `page/new_run.rs`): a new run's main region.
  - The request's hint (`this becomes the brief`). Its second clause names the chosen workflow and stands only where a node of the chosen mode asks questions (`Workflow::ask`), so no phrase says what the workflow does not do.
  - The headings `workflow`, `mode` and `what runs`.
  - `◆ it stops for you at …`, naming the gates of the mode.
  - A row's label for each other input (from `input_asks`).
  - The adapter row's first word (`AS_THE_RUNNERS_SAY`).
  - `///`: "Not `RunRequest`, what the rows build."
- `screen::start` loses the sequence's questions (`WHICH_WORKFLOW`, `WHICH_MODE`, `WHICH_ADAPTER`, `asked_for`) with Task 142, which deletes their last caller. The new run's legends by focus come in the design's words. The trail's place is `new run`.
- The inventory's rows S2 of the main region close here.

**Found in its turn:** `page::NewRun { workflow, mode }` holds a new run's words: the request (the first request's question and box, shared, with its hint under the box), the hint, the line that says where it stops for the person (`◆ it stops for you at approve-plan and ship`, each gate of the mode), an input's label and box with its description under it, and the headings `workflow`, `mode`, `adapter`, `what runs`. `screen::new_run_legend(NewRunRow)` says each row's own key first (`type the request`, `↑↓ workflow`, `←→ mode`, `←→ adapter`), then `tab next row · enter start · esc back`; Surco gains the left and right arrows as a legend's key. The trail's place is `new run`, where it read `start`. The lead's rulings: fragua's own words are reworded as decided when the targets were written (`packs/fragua`: its description `the full cycle: questions, plan, tests, work, review, PR`, its `idea` input `this becomes the brief`; `yunta list` prints them), the design's run recorded again (one field of its kept log, the manifest's hash; no landed screen moved) and the commit `setup-written` shows following the pack (`9fe5b41307ae` for `9ffe7a8d1d1a`, at the four sizes); and the hint's second clause is said only where it is true: `{workflow} asks before it decides for you` where every node of the mode that asks always asks, `{workflow} asks only what it cannot decide for you` where one asks only what is required (the real runs of this module in quick mode were asked nothing), nothing where no node asks, so `new-run` and `new-run-mode`, drawn with quick chosen, are corrected in that row at 120 and 80 columns and in ASCII, listed before and after for the person. Where a target governs over the task's text: the hint's first clause is the request input's own description (`new-run-inputs` shows run-tasks' `What the tasks build`). Left to the task that draws a new run: the sequence's old questions go with their last caller; at 60 columns the question and the hint give way.

**Fails first:** `crates/render/src/page/new_run/tests.rs` (new):
- `NewRun` for a workflow whose mode has gates at `approve-plan` and `ship` says `it stops for you at approve-plan and ship`;
- the hint's second clause stands for a mode with a node that asks, and not for one without.

Red until the page exists.

**Acceptance criteria:**
- [x] Each of the inventory's rows S2 for the main region and the legends stands in render once (`grep`).

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render -- new_run screen`

**Dependencies:** Task 133 (both add a page to `page.rs`)

**Files likely touched:** `crates/render/src/page/{new_run.rs (new),new_run/tests.rs (new)}`, `crates/render/src/page.rs`, `crates/render/src/screen/start.rs`

**Estimated scope:** S

### Task 135: what a new run usually takes, and what the machine answers for

**Description:**
- **`yunta_render::page::WhatItTakes`** (new, `page/new_run/takes.rs`): the panel. `what it usually takes`, then, from three past runs on (`history::MIN_SAMPLES`), `N past runs`, `median … tokens, p90 …`, `median wall-clock …` and the sparkline of the past runs' tokens. Then what the machine answers for: doctor's findings for each adapter, git, the forge and the checkouts, each in the words `yunta doctor` says it. `///`: "Not `spend::estimation`, the catalog's one line of it."
- `spend::estimation` says its three parts once. The catalog's one line joins them, so `yunta list` changes where a part's words do.
- Where the inventory gives a doctor finding the design's words (`forge  github eulke/yunta`), `yunta doctor` says them too.
- The inventory's rows S2 of the panel close here.

**Found in its turn:** `page::WhatItTakes { past, machine }`: under `what it usually takes`, how many past runs it reads and, once enough of them stand, `median 2.17M tokens, p90 27.1M`, `median wall-clock 1h07m` and a sparkline of each run's tokens, oldest first; then what this machine answers for, a row a check, from doctor's own findings (`doctor::glance`: an adapter by its version, git by `commits as {name}`, the checkouts by `builds {size} of a {budget} budget`, and anything that does not hold in doctor's words whole, with its fix once). A reading still missing says nothing. Every figure is one the product holds: the past runs' own logs through `spend` (whose parts are said once and joined for `yunta list`, unchanged), and doctor's reading. Render owns what the pool keeps (`page::doctor::PoolKept`), the engine's `Display` of it gone. The lead's ruling: who git commits as is carried as a name and an address apart (`engine::git::Identity`, api's `Committer::Named`), render parsing nothing; `yunta doctor`'s row keeps `commits as {name} <{address}>` and the panel says the name. What a command prints: `yunta doctor --full` and its JSON say `github {repo}` for a forge in reach, where they said `github {repo}: reachable`. Sentences no target gives: the heading alone while history is unread; `healthy` for an adapter that reports no version.

**Fails first:** `crates/render/src/page/new_run/tests.rs`:
- `WhatItTakes` with two past runs says no estimate, and with three says the three rows;
- the forge's finding reads `forge  github eulke/yunta`.

Red until the panel exists; the second says today's words.

**Acceptance criteria:**
- [x] `crates/render/goldens/catalog/` and `crates/render/src/page/doctor/tests.rs` changed only where the inventory moves their words, their diff reviewed; `pages.sh`' `yunta list` and `yunta doctor` captures differ only there.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render -- new_run catalog spend doctor`, `cargo test -p yunta --test command_output`

**Dependencies:** Task 134 (the panel stands under `new_run`)

**Files likely touched:** `crates/render/src/page/{new_run.rs,new_run/takes.rs (new),new_run/tests.rs,catalog.rs}`, `crates/render/src/spend.rs`, `crates/render/src/page/doctor/{finding.rs,tests.rs}`, `crates/render/goldens/catalog/`

**Estimated scope:** M. A phrase moved into `spend` changes with every caller that says it; the compiler names them.

### Task 136: a setup's phrases

**Description:**
- **`page::Setup`** becomes the setup's screen as the design draws it:
  - the trail's place `set up`;
  - the sentence (`This repository has never used yunta. Here is what it found. Nothing is written until you say so.`);
  - `what this repository is`, with rows `language`, `suite`, `lint`, `forge` (`github …, from origin`), `base`, `commits as`, and `workflow` (`yunta/fragua, to install as a pack`, or why it cannot be reached);
  - `who does the work`, one role a row;
  - the write row (`write .yunta/config.yaml and install yunta/fragua · then yunta asks what to build`, or without the install where the pack cannot be reached);
  - the legends (`j k step · enter looks right · q leave, nothing written`; `enter write it` on the write row).
- **`render::prompt`'s `PROJECT_NAME`, `BASE_BRANCH`, `model_of` and `runs_on`** take the design's word where the inventory gives one (`base`). `init -i` asks with the same words, so its prompts change with them.
- **`page::Initialized`**, what `init` prints, says a fact the setup also says in the setup's words.
- The inventory's rows S3 and S4's audit rows close here.

**Found in its turn:** `page::Setup` is the setup as the design draws it: the sentence, `what this repository is` (name, language, suite, lint, forge, base, commits as, workflow; a fact not found has no row), `who does the work` (a role a row, its first candidate, then `then …`), and the write row (`Setup::writes()`, with `· then yunta asks what to build` where the row has room). One owner for each word the setup, `init -i` and `init`'s page share (`PROJECT_NAME` is `name`, `BASE_BRANCH` is `base`, `prompt::MODEL`, `Setup::LANGUAGE`, `from_origin()`, `Setup::COMMITS_AS`, which doctor says too). Legends: `setup_legend()` (`j k step · enter write it · q leave, nothing written`), `setup_audit_legend()`, `written_legend()` (`enter on to what to build · q quit`), today's questions' legend renamed `answering_legend()`; the trail's place is `set up`. No target landed; the frames of `setup` and `setup-unreachable` are held in their rows and legends. What commands print: `init -i` asks `name (enter keeps …)`, `base (enter keeps …)` and `model (enter with none leaves it to declare later)`, where it asked `project name`, `base branch` and ``the model `implementer` runs``; `init` prints `language` where it printed `ecosystem`, and `github R, from origin`. Where the targets govern over the task's text: `enter looks right` is retired, `name` is the first row, there are five roles. The lead's rulings: the `language` row says a language, never the ecosystem's name (`Ecosystem.language`, set beside each detection: node says `typescript` where a `tsconfig.json` stands at the root, else `javascript`; the ecosystem's name, read by nothing after it, is gone), so `init` prints `language javascript` where it printed `ecosystem node`; a role with no model keeps its `✓`, made true by Tasks 147a and 147b (a candidate may leave its model to its adapter), asked of the owner; `Nothing else is touched.` is not true and is reworded when its screens are drawn (Tasks 152, 153: `Nothing outside .yunta/ is touched but .gitignore.`); `This repository has never used yunta.` and the tally are the field's to say, and a setup follows no field, so Task 149a gives the setup its repository's field. The proof's `prompts.py` waits on the new words. The walk's counts stand: 97,626 states, 114 palettes, 63 helps, 10,174 screens at each size.

**Fails first:** `crates/render/src/tests/setup.rs`:
- the setup of a Rust repository with a forge, a committer, a reachable pack and four roles says each row its target carries;
- the same with the pack unreachable says why on the `workflow` row and drops `and install …` from the write row.

Red: today's page says `to write` and has no such rows.

**Acceptance criteria:**
- [x] `crates/render/goldens/{setup,initialized}/` blessed, their diff reviewed.
- [x] `crates/cli/tests/console_interaction/init.rs` reads the new words where they changed, `init`'s flow unchanged.
- [x] `prompts.py` against the base differs only in those words.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render -- setup initialized`, `cargo test -p yunta --test console_interaction init`, `cargo test -p yunta --test init_new_cmd`

**Dependencies:** Group check: Surco's pieces. Disjoint from Task 130 to Task 133.

**Files likely touched:** `crates/render/src/page/{setup.rs,initialized.rs}`, `crates/render/src/prompt.rs`, `crates/render/src/screen/setup.rs`, `crates/render/src/tests/{setup.rs,initialized.rs}`, `crates/render/goldens/{setup,initialized}/`, `crates/cli/tests/console_interaction/init.rs`

**Estimated scope:** M A phrase reworded moves with every caller and every test that says it; the compiler names the first, the goldens the second.

### Group check: render's phrases
- [x] The suite's groups green. `pages.sh` against the base: each difference is `list --runs`, `status`, `close`, `list`, `doctor` or `init`'s, and one an inventory row names. `goldens.py`: the changed goldens are those Task 130 to Task 136 named. `python3 .plans/yunta-tui/tools/targets_check.py crates/tui/goldens/screens` passes, `close-run` among the targets it holds.

**Found in its turn:** the gate's steps pass but two tests of its `test` step (`.plans/yunta-tui/proof/live-screens/check-phrases-4/`, `after-the-gate.txt`): `status_nodes_cmd` waited on `1 of 1 node done · 1 left out`, which the page says with `, ` since the glance's phrases, and reads the new words now; one test of `session_ends`, the base's and unchanged, ran out of its 30 s wait for a stub CLI at a load of 17 to 21 beside another session's builds, passes alone in 4.7 s, never failed in 24 runs beside 12 busy loops and failed 2 of 30 whole-binary runs five at once beside 16 (a different test each time on the same wait): the machine's load, told to the owner as an observation. The release binary weighs 31,592,176 bytes (16,544 more than the last group's). Against the last group's captures (`check-surco-4-prints/`) 11 of 159 views differ, each by a row this group's tasks name: `init`'s `language` for `ecosystem`; `init -i`'s `name`, `base` and `model`; `yunta list --runs --full   this repository's runs, the closed too` for `lists the closed runs too` on `list` and on the field's next steps; the field's legend, the design's five keys. `doctor`, `close`, `status` and the catalog do not move in the captures (their cut holds no forge in reach, no several parts in one value, no fragua). 137 goldens differ from the last commit, every one a family this phase's tasks named (render's `status`, `closing`, `open`, `field`, `setup`, `initialized`; Surco's new pieces; the 14 target frames corrected by the lead's rulings; `close-run` and `cancel-run` landed); the digests are written for the next group (`digests-phrases-4.txt`, 839 goldens). `targets_check.py` holds the landed screens, `close-run` among them. The probe's `whole` did not build since the followed run kept its state to itself (`read.state()`); it builds again. The walk passes in the gate, its counts as Task 136's turn read them: 97,626 states, 114 palettes, 63 helps, 10,174 screens at each size.

### Task 137: the field's screen

**Description:**
- **`FieldStage::screen`** draws, from `FollowedRuns`' kept field:
  - `FieldRows` in the main region, the pointed row filled to the region's edge (E2, Phase 3's fill);
  - the trail `yunta › <the repository's path>` (C1);
  - the tally (C2) and the attention row (C5), through Phase 3's one composer of both, `FollowedRuns::framing` (Task 92): the run that has waited longest on the person among those answered here, what it asks, `a go there`. A run asked at its own terminal is counted in the tally and listed under `asking now`, never named in the attention row.
  - **the note**, `Screen.noted`, composed from the field's `said`: what an act here came to, and the note a run's `q` or `esc` left there (Task 94), in the attention row's place until the next key; news leave it;
  - the legend Task 130 words, with `Screen.anywhere` beside it.
- The repository's path comes with the repository's reading as the application opens. Where Phase 2's reading does not carry it, `yunta_api::Repository::SetUp` carries the workspace's directory.
- An unreadable run stands under `unreadable` with why; the rest of the field goes on.
- **The targets `field` and `field-unreadable` land:** moved (`git mv`) from `crates/tui/goldens/targets/` into `crates/tui/goldens/screens/`, each state from `yunta_testkit_ui::cut` by its name: the design's run and the small runs loaded at their moments into one store, with a run whose manifest is gone. `crates/tui/tests/screens.rs` holds them at `unicode-80` and `unicode-60`, where no glance stands; Task 138 holds their 120-column frames.
- **The field's drawing is recorded**, the drawing Task 138 is held to: the probe's `frame` pointed at a field of 200 waiting runs at 200×60, the median of 2,000 and its spread, in `.plans/yunta-tui/proof/live-screens/phase-4/field-draw.txt`.
- `FieldStage`'s `//!` says what the screen draws and where each part comes from.

**Found in its turn:** `FieldStage::screen` draws `page::FieldRows` from the field the follower keeps: the trail `yunta › <the repository's path>` (the home said as `~`), or `this machine` where the field is not this repository's (`screen::field_place`); the tally and the attention row from `FollowedRuns::framing` with no run to stand on, so a run asked at its own terminal is counted and never named; the legend the design's five keys. What the field has just said is `Screen.noted` (`✓` a note or a refusal, `✗` a failure): news leave it and the next key clears it. `Repository::{SetUp, NeverSetUp}` carry the repository's directory. Surco lays a region that lists runs under headings and follows a pointer as its lists (`pane/listed.rs`): one cell inside each edge (rows of 74, 76 and 56 cells at the three widths), the blank rows between lists giving way first, then a window that opens at the earliest heading keeping the pointer, `… n above` and `… n under` at the marks' column. The pointed row takes the pointer of a run that needs the person and the selection's fill. `s` on the field opens the pointed run's screen with its numbers over it (`s what it cost` is an act the approved frame offers; over the field itself it would name a run on the trail that the screen does not follow). Render's `Field` lost `pointed`, `marked` and `listed()`. Landed: `field` and `field-unreadable` at 80 and 60 columns in this task (the base's own `field.unicode-60` replaced). A field with no run shows its first row alone, where the base said `no runs yet` and where runs are kept. Drawn in about 1.4 ms with 200 waiting runs at 200x60 (`phase-4/field-draw.txt`). Not true until its task: `a go there` on the attention row (`a` still turns the reach), `n new run` (it still opens the run that waits).
 The walk reaches 100,873 states for 97,626, 114 palettes, 63 helps and 10,479 screens a size for 10,174 (its test 155 s at a load of 9 to 18); 387 of the 3,247 are `s` on the field (measured by unbinding it); the rest were not attributed: each part of the new screen put back to its old form one at a time, and the trail's place and `held` taken out of the fingerprint, left the count where it is, two instrumented walks give the states by class (`phase-4/walk-classes.txt`, every class of the field populated, no sign of a fall under the rise), and the tree before these tasks was not kept to walk again. From here on the lead keeps the tree before each task that can move the walk (`tasks/snapshot.sh`), so a count is compared with its base. The engine's suite whole: 1,495 pass and one test of process-group supervision, untouched by these tasks, failed once at a load of 11 to 13 (alone 3 of 3; 0 of 200 beside 8 busy loops); what the three engine files gained is a reader beside the run loop (`awaited_escalation`, `InternalGate::unread`), and nothing a run calls changed.

**Fails first:**
- `crates/ui/tests/field.rs`: with a run asking at its terminal and two waiting on a decision, the attention row names the older of the two and never the asking one; the trail names the repository's path; a run's `q` leaves its note in the attention row's place, through a piece of news, until the next key.
- `crates/tui/tests/screens.rs` with `field` and `field-unreadable` at `unicode-80` and `unicode-60` against their targets.

Both are red before the screen changes.

**Acceptance criteria:**
- [x] `field` and `field-unreadable` equal their targets at `unicode-80` and `unicode-60`. `crates/tui/goldens/screens/field.ascii-80.txt` is gone and `field.unicode-60.txt` is the target's.
- [x] The inventory's rows C1, C2, C5, E2, F1, F2 and F3 of the field differ in none.
- [x] `phase-4/field-draw.txt` holds the field's drawing.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test field`, `cargo test -p yunta-tui --test screens`, `cargo test -p yunta-tui --test walk`
- [x] `python3 .plans/yunta-tui/tools/targets_check.py crates/tui/goldens/screens`

**Dependencies:** Group check: render's phrases

**Files likely touched:** `crates/ui/src/stage/field.rs`, `crates/api/src/field.rs`, `crates/tui/tests/screens.rs`, `crates/tui/goldens/screens/`, `crates/ui/tests/field.rs`, `.plans/yunta-tui/proof/live-screens/phase-4/` (outside git)

**Estimated scope:** M

### Task 138: the glance at the right

**Description:**
- The field's side region holds the run pointed at as `RunAtAGlance` draws it. It stands at the right edge from 102 columns and gives way below, through Phase 3's side region (F4).
- **Its acts are `RunFrame::actions`'**, worked out once for each head of the kept run with the decision the engine rebuilt for it, and kept beside its state, never worked out at a draw. Each act takes the field's key for its step (`enter` opens, `r` resumes, `x` closes, `s` its numbers), through the one table that maps a step to its key (`Act::by`).
- Phase 2 keeps each field run's `FollowedRun::menu`; the kept field run keeps the decision by head beside it, and the flag is derived from it.
- **The glance's targets land:** `field` and `field-unreadable` at `unicode-120` and `ascii-120`, which carry the glance of the run pointed at (the one that needs the person, in `field`); `field-glance-stalled` and `field-glance-stopped` at the four sizes, moved (`git mv`) into `crates/tui/goldens/screens/`, each state from `yunta_testkit_ui::cut` by its name.

**Found in its turn:** The field's side region holds `RunAtAGlance` for the pointed run, read or unread with why; below 102 columns the side gives way and what a person acts on from it stands under the list (`Side.instead`, tui's `Terminal::pane_holding`). `FollowedRun` keeps the run's `decision`, rebuilt once a head, and its `actions`, worked out when its frame is derived: neither a drawing nor news of the same head works them out again (the test counts it). The decision comes from the engine's `awaited_escalation(manifest, state)`, which reads no document (a gate answers with what it shows as the ledger holds it, its question its author's words); `menu` follows it, so a gate whose shown artifact the run does not hold offers no menu on the field. The glance's acts name the field's keys (`r` resume, `c` stop, `x` close, `enter read the decision` where one waits, `s what it cost`). Surco: a headline with no word draws its mark and subject alone; a gist parts a path too long after its last `/` that fits. Landed, held whole at the four sizes: `field`, `field-unreadable`, `field-glance-stalled`, `field-glance-stopped` (66 in `LANDED`; the test opens a run's or a field's target). No row contested. Left to the tasks that give the field its marks and keys: `field-marked`, `field-close`, `field-closed`, `field-machine`, `field-note`, `field-resumed`, `field-palette`, `field-help`; and that the pointer now stands on an unreadable run, on which `x`, `r`, `c` and `enter` still act though its glance offers nothing.

**Fails first:**
- `crates/ui/tests/field.rs`: the pointer on a stalled run offers `r` and `x`, and on a run asked at its terminal offers no `x`; news of the same head work out no act again (a counter on the kept run's acts).
- `crates/tui/tests/screens.rs` with the targets above.

Red until the glance exists.

**Acceptance criteria:**
- [x] `field` and `field-unreadable` equal their targets at the four sizes, and `field-glance-stalled` and `field-glance-stopped` at the four sizes. At 80×24 no glance stands, and the main region takes the width.
- [x] The probe's `frame`, pointed at the field of 200 waiting runs at 200×60 as Task 137 recorded it, takes what Task 137 recorded, within its spread.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test field`, `cargo test -p yunta-tui --test screens`
- [x] Manual check: the drawing beside Task 137's in `phase-4/field-draw.txt`

**Dependencies:** Task 137

**Files likely touched:** `crates/ui/src/stage/field.rs`, `crates/ui/src/followed_runs.rs` (Phase 2's, new in that phase), `crates/ui/tests/field.rs`, `crates/tui/tests/screens.rs`, `crates/tui/goldens/screens/`, `.plans/yunta-tui/proof/live-screens/phase-4/` (outside git)

**Estimated scope:** M

### Task 139: marking and closing several after one confirmation that names each

**Description:**
- `space` marks the pointed run and puts it back; the marks draw as their target draws them.
- `x` closes the marked runs, or the pointed one where none is marked, and only where the run's acts include closing. One confirmation, in Task 127's box with Task 132's words, names each run. `c` cancels the pointed run through its own confirmation.
- **A confirmation holds what it named.** News that move rows or add runs leave its runs and its pointed row as they were (spec "What holds"), and `enter` closes exactly the runs it named. What refused or failed is said in the note.
- **The targets `field-marked` and `field-close` land** (the marks, and the confirmation that closes several): moved (`git mv`) into `crates/tui/goldens/screens/`, each state from `yunta_testkit_ui::cut` by its name. No base frame goes here: `confirmation.*` went in Phase 3.
- The field's `//!` says the rule.

**Found in its turn:** `space` marks the pointed run and puts it back, its mark `◉` (`+` in ASCII). `x` closes the marked runs, or the pointed one where none is marked, and only those whose kept acts hold a close: one confirmation names in its question the runs it closes and, under its sentence, each marked run it leaves open with why (`Q78GRR is not closed: an engine is driving the run.`); where it would close none, `x` does nothing. `c` asks its own confirmation, only where the run's acts hold a stop. A confirmation holds what it was asked: news that move the rows leave its runs alone and `enter` closes exactly those (the test fails when the close is rebuilt by place). The words are render's (`Confirming::Close { runs, left }`, `LeftOpen`, `WhyLeftOpen`); the engine's refusal of a close is one function, `close_refusal(state, engine)`, which `close_run` refuses by and the field reads through `FollowedRun::close_refused()`; the field's keys act through one table (`glance::key_of`). `field-close`'s corrected frames come out as the lead derived them (the box at rows 10 to 18, 6 to 14 and 3 to 11). Not landed: `field-marked` and `field-close` are held cell for cell everywhere but one row in ASCII (row 7: the frame folds the middle dot of the session's own recorded words to `;`, the code draws the words as recorded), which waits on the owner's answer in Task 140a. The walk: 97,368 states and 10,096 screens a size for 100,873 and 10,479, each state measured to its cause (`c` only where a stop is offered: 2,965 fewer; `x` only where a close is offered: 558 fewer; the two overlap by 18).

**Fails first:** `crates/ui/tests/field.rs`, `a_confirmation_closes_the_runs_it_named_while_rows_move`:
- two runs are marked and `x` is pressed;
- followed news move a third run into `needs you` above them;
- `enter` asks `Operation::Close` of exactly the two.

Red for its cause if the confirmation is rebuilt from the field at `enter`, which the test proves by doing so for its red.

**Acceptance criteria:**
- [x] `field-marked` and `field-close` equal their targets at the four sizes.
- [x] `crates/cli/tests/interface/field.rs` closes two picked runs after one confirmation through this build, as it does today, with its words updated.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test field --test dialogs`, `cargo test -p yunta --test interface field`, `cargo test -p yunta-tui --test screens`

**Dependencies:** Task 138

**Files likely touched:** `crates/ui/src/stage/field.rs`, `crates/ui/tests/{field.rs,dialogs.rs}`, `crates/cli/tests/interface/field.rs`, `crates/tui/tests/screens.rs`, `crates/tui/goldens/screens/`

**Estimated scope:** M

### Task 140: the field's keys

**Description:**
- `j k` and the arrows move the pointer, `enter` opens, and `q` leaves (the field reads nothing beside its list).
- **`d`** lists the closed too, and again hides them.
- **`e`** reaches the whole machine and back. `followed()` names the field at that reach, so the follower refollows; the trail and the groups say so as the target draws it. This is the key `a` was at the base.
- **`a`** goes where the attention row names, through Phase 3's way: that run's screen.
- **`r`** resumes where the pointed run's acts include it, and leaves the note of what the resume did, in `render::start`'s words (`{…} resumed, driving forward independently.`, the target `field-resumed`), where the base's field says nothing; elsewhere it starts nothing and leaves the note Task 130 words (the target `field-note`), where the base hands a resume to a detached process that the run's decision stops again.
- **`esc`** does nothing on the field.
- `c`, `w` and `m` keep what they do, out of the legend.
- **`FieldStage::offered`** says the palette's rows: one an act, the field's out-of-legend keys among them, `e` as `yunta list --runs --all`.
- **The field says the keys it takes beyond its legend** (`space`, `r`, `d`, `e`, `a`, `c`, `w`, `m`, in Task 130's words), which help lists in Phase 3's box, as Phase 3 made a run's screen say its own.
- **The targets `field-closed`, `field-machine`, `field-note`, `field-resumed`, `field-palette` and `field-help` land**, moved (`git mv`) into `crates/tui/goldens/screens/`, each state from `yunta_testkit_ui::cut` by its name.
- The spec's "Keys" rows for the field close here, except `n`, which Task 145 gives the new run.

**Found in its turn:** `e` reaches the whole machine and back; `a` opens over the field the run the attention row names, by the row's own rule; `esc` does nothing and `q` and ctrl-c leave; `r` resumes where the run's acts hold a resume and notes `C0NS0E resumed, driving forward independently.` (`render::start::resumed_note`), notes `Nothing to resume: …` on a run that waits on a decision, and does nothing on any other run with no resume (the task's text said the note stands elsewhere too: its words would not be true there); `enter` and `s` act only on a run the field reads. Help over the field is render's `help_over_the_field(machine, closed)`; the palette lists its rows in the target's order, and `start a new run` from it opens today's start over the field. Landed: `field-closed`, `field-machine`, `field-resumed`, `field-help` (70 in `LANDED`). Not landed: `field-note` (the same ASCII row as Task 139's two, Task 140a) and `field-palette` (row 14 names `n` for `start a new run`, true once `n` starts one: Task 145); `crates/tui/tests/screens/held_but_a_row.rs` holds the four everywhere but those rows until they land. Help and the legend say `n` starts a new run, true from Task 145. The walk: 96,097 states, 114 palettes, 63 helps, 10,122 screens a size (`r` asks no resume where none is offered: 2,037 fewer, no screen; its note on a run waiting on a decision: 766 more states, 26 more screens; `a`, `e`, `esc`, help and the palette move no count), its test 131 to 157 s at a load of 7 to 11; `MUST_REACH` holds 83. The engine's suite whole passes (1,496). `docs/guide.md` says the field's keys. For the tasks of every key: at the machine's reach with the closed shown, help names `yunta list --runs` for both `d` and `e`.

**Fails first:**
- `crates/ui/tests/field.rs`: `e` asks `followed()` for the field at the machine's reach; `a` opens the run the attention row names; `esc` asks nothing; `r` on a run waiting on a decision leaves the note and asks no work; `r` on a run it resumes leaves the note of the resume. Each is red at the base, where `a` toggles the reach and `esc` leaves.
- `crates/ui/tests/palette.rs`: help over the field lists `c`, `w` and `m`. Red: the field's help is its legend.

**Acceptance criteria:**
- [x] `field-closed`, `field-machine`, `field-note`, `field-resumed`, `field-palette` and `field-help` equal their targets at the four sizes.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test field --test palette`, `cargo test -p yunta-tui --test screens`, `cargo test -p yunta-ui --test walk`

**Dependencies:** Task 139

**Files likely touched:** `crates/ui/src/stage/field.rs`, `crates/ui/tests/{field.rs,palette.rs}`, `crates/tui/tests/screens.rs`, `crates/tui/goldens/screens/`

**Estimated scope:** M

### Task 140a: an ASCII screen holds ASCII alone

Added in Phase 4 by Task 139's finding (2026-11-21). Five approved frames in ASCII fold words that are not Surco's
own: a session's `Not logged in · Please` reads `; Please` (`field-marked`, `field-close`, `field-note`, row 7), and
the pack's own `—` reads `--` (`setup-audit`, `setup-confirm`, row 18); the code draws the words as recorded.
Surco's ASCII glyphs are for an environment that names no UTF-8 locale, where a character outside ASCII is drawn
as several cells of noise and a row of the full-screen application no longer ends at its width.

**Decided by the owner on 2026-11-22:** the full-screen application folds what it draws under the ASCII glyphs;
a printed page does not (a log or a pipe is often read later where UTF-8 shows, and a page never loses a
character of what was recorded). A screen never breaks its grid; a page never loses a character.

**Description:**
- Under the ASCII glyphs, every text a screen of the application draws is folded to ASCII before it is measured, recorded words included:
  - a character Surco already has an ASCII form for takes that form (`·` as `;`, `—` as `--`, `…` as `...`, the dashes, the curved quotes, the arrows);
  - a Latin letter with a mark takes its base letter (`diseño` reads `diseno`, `más` reads `mas`), from a table in Surco, with no new dependency;
  - any other character outside ASCII is drawn `?`.
- One owner in Surco (beside `Glyphs`), applied where the application's screen takes a text, never where a page is printed to a stream; a document (JSON) and Markdown and HTML are never folded. What `yunta` prints at a shell under the ASCII glyphs does not change.
- The five frames land as approved, held whole; `held_but_a_row.rs` goes with its last row.

**Found in its turn:** Under the ASCII glyphs the full-screen application folds every text it draws to ASCII before anything is measured, recorded words included; a printed page, Markdown and HTML keep every character. The code tells a screen from a page by its entry and its type: `yunta_tui::drawn` is the one place a `Screen` is laid out on a terminal, and under `Glyphs::Ascii` it lays out `screen.in_ascii()`; a page is a `Doc` drawn by a surface, and none calls it. Where the task's text gave way: no single place of Surco's terminal surface takes a screen's text (each component measures its own), so the fold is a value's transformation over the whole `Screen` (`InAscii`, implemented for every component field by field, so a field left out does not compile). The character's rule is one owner beside `Glyphs` (`crates/surco/src/glyphs/ascii.rs`): Surco's own forms read from `Glyphs` itself (`·` as `;`, `…` as `...`, `›` as `::`, `→` as `->`, `—` as `--`), `–` as `-`, curved quotes straight, every letter of the Latin-1 Supplement and Latin Extended-A from a written table (`ñ` as `n`, `ß` as `ss`, `æ` as `ae`), and any other character one `?` (a wide character takes one cell for two, a combining mark one for none). Red: with the fold returning its text as recorded, nine tests failed (`Not logged in · Please` for `; Please`; `diseño — más`); the page's test passes throughout. Landed as approved, no frame edited: `field-marked`, `field-close`, `field-note`, `setup-audit`, `setup-confirm` (85 in `LANDED`); `held_but_a_row.rs` is gone; a test holds that every landed ASCII frame is ASCII alone, and the walk holds it over every state drawn at `ascii-120`. Nothing a command prints changed (no golden moved). No class of the walk moved (64,810 states, 114 palettes, 68 helps, 9,183 screens a size). Gathered for the next group's pass of fixes: the window's title is still written as recorded under the ASCII glyphs; in a quoted line a caret keeps its recorded column, so it stands off its fault by the cells a longer form adds before it.

**Fails first:** Surco: a screen's line holding `Not logged in · Please`, drawn with the ASCII glyphs, holds ASCII alone and reads `Not logged in ; Please`; `diseño — más` reads `diseno -- mas`; a character with no form reads `?`; the same line printed as a page keeps its characters. Red: the words are drawn as recorded.

**Acceptance criteria:**
- [x] `field-marked`, `field-close`, `field-note`, `setup-audit` and `setup-confirm` moved to `screens/` and in `LANDED`; `crates/tui/tests/screens/held_but_a_row.rs` is gone.
- [x] Every landed frame of the ASCII environment holds ASCII alone (a test over `crates/tui/goldens/screens/*.ascii-*`).
- [x] No golden of a printed page moves: the captures against the last group's differ in no view.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-surco --lib`, `cargo test -p yunta-tui --test screens`, the walk once with its table

**Dependencies:** Task 153; the owner's answer (given).

**Files likely touched:** `crates/surco/src/glyphs.rs` (or a module beside it), `crates/surco/src/surface/terminal/`, `crates/tui/src/drawn.rs`, `crates/tui/tests/screens.rs`, `crates/tui/tests/screens/held_but_a_row.rs`, `crates/tui/goldens/screens/`

**Estimated scope:** S

### Group check: the field
- [x] The suite's groups green, the walk's count and time against Phase 3's. Every target of the field equals at the four sizes (`field`, `field-unreadable`, `field-glance-stalled`, `field-glance-stopped`, `field-marked`, `field-close`, `field-closed`, `field-machine`, `field-note`, `field-resumed`, `field-palette`, `field-help`), with `close-run`; `tui`'s `field` frames are gone. `python3 .plans/yunta-tui/tools/targets_check.py crates/tui/goldens/screens` passes: no row of the field or its confirmation differs.

**Found in its turn:** a light check, as the owner decided for this group (`.plans/yunta-tui/proof/live-screens/check-the-field-4/`), on a build directory emptied first (it had reached 63 GB with 47 GB free). The short steps pass; the eight touched packages' tests hold 241 binaries and 3,977 tests, one failing: the engine's `process.rs::context_command_timeout_leaves_no_process_alive`, the base's and untouched by this phase, at a load of 49 to 73 beside three other sessions' builds; its binary alone passes and the test never failed in 60 runs beside 8 busy loops at a load of 75 (`after-the-tests.txt`): told to the owner as an observation. The release binary weighs 31,658,304 bytes (66,128 more than the last group's). Against the last group's captures 3 of 159 views differ, the three that draw the field in the full-screen application (the new screen: the trail with the repository's path, the tally's words giving way to it at 80 columns as the conventions say, no headline row); every printed page reads as before. The screen captures waited on the old headline's words and wait on the field's first row now (`screens.py`). The goldens: none changed, 32 moved from `targets/` to `screens/` (the eight screens landed in this group); digests written (`digests-the-field-4.txt`). The walk as Task 140 left it: 96,097 states, 114 palettes, 63 helps, 10,122 screens a size. No process of the worktree's tests was left running, before or after. Still held everywhere but a row: `field-marked`, `field-close`, `field-note` (Task 140a, on the owner's answer) and `field-palette` (Task 145).

### Task 141: the first request

**Description:**
- **`FirstRunStage`** draws one stage, from `FirstRequest`:
  - the wordmark;
  - `What do you want to build?`;
  - the typed answer in its box, with its placeholder;
  - each workflow with what it does;
  - the horizon under them;
  - the trail `yunta › <the repository's path>`; the tally and the attention row through Phase 3's one composer, `FollowedRuns::framing` (Task 92); and its legend (S1), with no far pair, since a line is typed.
- **Keys:** printable keys type. `enter` goes on with what is typed: to today's start until Task 145 opens the new run with it. `esc` goes to the field where there is one, else leaves.
- **The screen stays under a typist.** A run started elsewhere while words are typed shows in the tally, and the screen and what is typed stay until the person leaves it. The stage stands down only by the person's key, as Phase 2 decided.
- **The attention row says what needs the person with no key**, since a line is typed: Phase 3's case with no key (`Answer::Unkeyed`, Task 73).
- **The target `first-request` lands**, moved (`git mv`) into `crates/tui/goldens/screens/`, its state from `yunta_testkit_ui::cut` by its name: a repository set up with no run (the home's `set_up`).
- The stage's `//!` says what stands and what moves it.

**Found in its turn:** The first request draws the name in block letters, the question over its box with its placeholder, and each workflow a row (its name muted in sixteen cells, then its own description), in a column of 72 cells, centred, the field in perspective in the rows left under it; from 27 rows of pane the words stand two rows lower, and at 60x14 the horizon alone gives way. The trail is `yunta › <the repository's path>`; the tally and the attention row come from `FollowedRuns::framing` as under a typist, so the attention row names what needs the person with no key. The legend is `type what you want to build · enter new run · esc quit`, its last key `esc field` where the field holds a run (render's word for that place; no target draws it). Printable keys type; `enter` goes on to today's start once the catalog is read (through the old questions until the task that opens the new run with it); `esc` goes to the field where it holds a run, else leaves; news never replace the stage. Surco gains a region's measure (`Region::measured`, `pane/measured.rs`): a column of a given width, centred, with a horizon under it and a typed line that keeps its cursor over it. Landed: `first-request` at the four sizes (71 in `LANDED`), no row contested. Where the task's text gave way: the test at a pseudo-terminal starts a run that parks on a gate (`yunta run`, which ends when it parks) in place of one held running, since the tally counts the runs that need the person or stalled and a running one never shows in it; it waits on the tally's mark and count, and stressed 50 times with ten workers it never failed, twice. The kit opens a target that stands in a place of the home (`HomePlace::made_alone`, `screens::home::opened`). The walk: 96,287 states, 114 palettes, 63 helps, 10,175 screens a size, every state of the rise measured to its cause (`esc` going to the field 178, the tally and attention row 10, the trail's path 2); with each part put back it lands on its base. `docs/guide.md` says the first request's keys. For later tasks: the setup's 92-cell column cannot take the measure as it stands (its sentence is wider and it must not stand lower); Surco's reference gains the region's measure.

**Fails first:**
- `crates/ui/tests/first_run.rs`: `abc` typed, then followed news of a run started elsewhere: the stage stays, `abc` stays, and the tally says the run. Red if the stage gives way to the field on news, which the test proves by doing so for its red.
- `crates/cli/tests/application/starting.rs`, `the_first_request_stays_under_a_typist_while_a_run_starts_elsewhere`: words typed at a pty; a run started from a shell with `yunta run --detach`, held by `Hold`; the tally shows it before the next key; the typed words and the question stand; `esc` then goes to the field. An order, no duration.

**Acceptance criteria:**
- [x] `first-request` equals its target at the four sizes, and the inventory's rows S1 differ in none.
- [x] A repository set up with no run opens on it: Phase 2's test in `crates/ui/tests/state.rs`, an empty field's first news standing the state on the first request, still green.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test first_run --test state`, `cargo test -p yunta-tui --test screens`, `cargo test -p yunta --test application starting`
- [x] The new pty test 50 times under `stress_binary.py` with ten workers: no failure.

**Dependencies:** Group check: the field

**Files likely touched:** `crates/ui/src/stage/first_run.rs`, `crates/ui/tests/first_run.rs`, `crates/cli/tests/application/starting.rs`, `crates/tui/tests/screens.rs`, `crates/tui/goldens/screens/`

**Estimated scope:** M

### Group check: the first request
- [x] The suite's groups green; the walk reaches the first request with its new words; `first-request` equals its target.

**Found in its turn:** a light check (`.plans/yunta-tui/proof/live-screens/check-first-request-4/`): the short steps pass; the six touched packages' tests hold 104 binaries and 2,357 tests, none failing; the release binary weighs 31,658,320 bytes (16 more). Against the field's captures no view of 159 differs: the captures' repository holds runs, so no command or screen of theirs stands on the first request, which its own tests and its landed target hold. One view differed by the clock alone, a run's age on its row in the field read `1m` in the slower capture and seconds in this one: the comparison masks a row's age across the minute now (`normalize.py`), and the two earlier comparisons keep their counts under it (3 and 11). The goldens: none changed, 4 moved (`first-request`); digests written (`digests-first-request-4.txt`). No process of the worktree's tests left running. The walk as Task 141 left it: 96,287 states, 114 palettes, 63 helps, 10,175 screens a size.

### Task 142: a new run's stage holds every row

**Description:**
- **`StartStage`** in `crates/ui/src/stage/start.rs` becomes one screen with a row each for:
  - the request (a `TypedAnswer`, with the hint `NewRun` words under its box);
  - each other input the workflow declares (a `TypedAnswer`, its label asked);
  - the workflow (Phase 3's list of choices);
  - the mode (a `WordChoice`, only where the workflow declares modes);
  - the adapter (a `WordChoice`: as the runners say, then each built adapter).
- **The focus** moves with `tab` and `shift-tab`. On a typed row printable keys type and `←→` move in the line; on the workflows `↑↓` move the pointer; on the mode and the adapter `←→` change the chosen word.
- **What the rows hold is the `RunRequest`** `yunta run` builds from the same choices. The request stands in the input the workflow reads it from. The mode and the adapter open on what `yunta run` takes when none is named.
- **`esc`** goes back. The request goes into the first request's line when the new run was opened there; otherwise the state keeps this one new run left unstarted, as the next `n` will find it, until a run starts.
- The sequence goes: `Asking`, `next_question`, `asked_again`, `left`, the pickers of the mode and the adapter, `screen::start`'s questions, and `crates/ui/tests/first_run.rs`' `the_choices_ask_for_the_run_yunta_run_asks_for_with_them`, which moves into `crates/ui/tests/new_run.rs` under what it now holds.
- The stage's `//!` says the rows, the focus and what it builds.

**Found in its turn:** A new run is one screen with a row each for the request (the first request's question over its box, its hint under it), the workflow (a list, the pointer on the one chosen), each other input the workflow declares, the mode (where the workflow declares modes) and the adapter (`as the runners in config.yaml say`, then each adapter the binary builds). `tab` and `shift-tab` move the focus and wrap; a typed row types; `↑↓` move along the workflows; `←→` change the mode or the adapter; the legend follows the row in focus and the trail reads `yunta › new run`. It is a typed screen on every row (`Stage::typed()`): no printable key opens the palette or help there, as its targets' legends name no far pair; `Stage::typing()` still says a line is open. The rows build the `RunRequest` `yunta run` builds from the same choices: the request in the input marked `request: true`, the mode opening on the workflow's first (what admission falls back to), the adapter on none (each runner's own); another workflow keeps what was typed for each input by name. `esc` puts the request back in the first request's line when the new run was opened there, and otherwise the state keeps the one new run left unstarted (`State.unstarted`, opened again by `Want::NewRun`) until a run starts. Surco gains a form (`Form`, `FormRow`, `FormValue::{Typed, Listed, Words}`), which stands whole and gives way in a stated order, then keeps the focused row in sight in a window (`… n above`, `… n under`); Markdown and HTML draw it whole; it is in the catalog. The sequence's questions are gone from render (`WHICH_*`, `STARTING`, `asked_for`, `input_asks`). No target landed and no row contested; the rows the screen already equals in `new-run`, `new-run-mode` and `new-run-inputs` are told in its report, the rest waiting on the panel and the order of the workflows. A sentence no target gives: `{workflow} takes no request`, under the box of a workflow that declares no request input. Until the next tasks: a refusal reads in the note row, not under the row it names; `enter` asks today's admission. The lead's rulings: the workflows are listed by how often each ran in this repository, as the approved targets draw, from the count the catalog's own reading of each history already holds (built with the panel); a kept new run is not in the walk's fingerprint (the screen a kept run opens on is the screen the same words typed reach, so every screen is walked; in it, every state after an `esc` would be walked twice); a kept new run opened again takes the catalog it is opened with (built with the ways into a new run). The walk: 72,805 states for 96,287, 114 palettes, 63 helps, 9,827 screens a size; by place, on the tree kept before the task and on this one, the new run holds 12,987 states for 36,469 (the sequence's filtered pickers gone) and every other place the same count, 59,818 in all.

**Fails first:** `crates/ui/tests/new_run.rs` (new):
- a workflow with a request input and one other input, modes and two adapters: `tab` to each row, words typed, `←→` on the mode, `enter`; the stage asks `Operation::Start` with the `RunRequest` that `yunta run greet --input who=… --mode … --adapter …` builds, compared field by field;
- `esc` from the first request's new run puts the edited request back in its line.

Red: the stage asks one question at a time.

**Acceptance criteria:**
- [x] No `Asking`, picker of a mode or picker of an adapter is left in `crates/ui/src/stage/start.rs` (`grep`).
- [x] The walk's count and time measured against Task 141's, each difference named.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test new_run --test first_run --test walk`

**Dependencies:** Group check: the first request

**Files likely touched:** `crates/ui/src/stage/start.rs`, `crates/ui/src/state.rs`, `crates/ui/src/stage/first_run.rs`, `crates/render/src/screen/start.rs`, `crates/ui/tests/{new_run.rs (new),first_run.rs}`

**Estimated scope:** M

### Task 143: what runs and where it stops

**Description:**
- Under the rows, `what runs`: the chosen workflow's nodes in the chosen mode (`Workflow::iter_nodes_under` with the mode's `include`), named by `watched::stretch_names` over their ids (Phase 3's, the ribbon's names by question 3's rule, Task 95) over the furrow drawn apart. Then `◆ it stops for you at …`, naming the mode's gates.
- It moves with the workflow and the mode chosen and with nothing else. `NewRun` draws it, and the stage hands it the workflow and the mode.

**Found in its turn:** Under the rows the form's foot says `what runs`: a stroke for each node the workflow declares, drawn as a field row draws a run's furrow (`┄` where the chosen mode runs it, `╌` where it leaves it out, two cells a node at most), the names of the nodes it runs as the ribbon names them, and `◆ it stops for you at …` with each gate the mode reaches (`replay::modes::reached`). It changes with the workflow and the mode alone. Render's `NewRun::what_runs(glyphs)` gives the foot (`FormFoot { heading, strokes, said }`). Held against fragua as its pack declares it: quick names ten nodes, leaves out five and stops at `ship`; standard and full run all fifteen and stop at `approve-plan` and `ship`. The rows equal `new-run` rows 21 to 24 at 120 columns and in ASCII, and `new-run-inputs` rows 24 to 26. The walk's counts stand (72,805 states, 9,827 screens a size).

**Fails first:** `crates/ui/tests/new_run.rs`: choosing the mode with no gate drops the `it stops for you` row, and choosing back brings it. Red until the block is drawn.

**Acceptance criteria:**
- [x] For fragua's three modes, `what runs` names the nodes each mode's `include` lists and only those, the names those `yunta graph yunta/fragua` draws for them (`crates/render/src/page/new_run/tests.rs`).

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test new_run`, `cargo test -p yunta-render -- new_run`

**Dependencies:** Task 142

**Files likely touched:** `crates/ui/src/stage/start.rs`, `crates/render/src/page/{new_run.rs,new_run/tests.rs}`, `crates/ui/tests/new_run.rs`

**Estimated scope:** S

### Task 144: the panel at the right

**Description:**
- **Opening**, the new run asks for the catalog and for doctor's findings at a glance (`Operation::Doctor { sessions: false }`). When a workflow is first chosen it asks for its history (`Operation::ReadHistory`), once per workflow; each answer is kept.
- **The side region**, at the right from 102 columns, holds `WhatItTakes`: what the workflow usually takes, from three past runs on, and what the machine answers for. Below 102 columns it gives way, as its target draws.
- A machine fact read when the screen opens is never read again on a timer.
- The tally and the attention row come through Phase 3's one composer, `FollowedRuns::framing` (Task 92).
- **The target `new-run` lands** (the boxed request and its hint, `workflow`, `mode`, `what runs`, and the panel), moved (`git mv`) into `crates/tui/goldens/screens/`, its state from `yunta_testkit_ui::cut` by its name: the repository holding Phase 1's three past runs.

**Found in its turn:** A new run asks for the catalog and for doctor's findings at a glance once, as it opens (`Doctor { sessions: false }`, which opens no session; one to two seconds on this machine): the screen draws at once and the machine's rows fill in when the reading lands. The panel stands at the right from 102 columns, 44 cells wide (a side region that names its measure stands that wide), its heading level with the question; below 102 columns what stands in the way of the chosen workflow's run takes its place, doctor's rows that do not hold and bear on that workflow (`Finding::bears_on`, `doctor::in_the_way`: the forge for fragua, which admission refuses without it): under the form at 80 columns, a row of the form's window at 60 (it counts in `… 2 under`). The lead's ruling built: the workflows are listed by how often each ran here, most first, then in the catalog's order; the catalog's entry keeps the past runs its one reading holds (`ReadableWorkflow.past`, with `usually()` and `past_runs()`), and the order and the panel's figures both come from it, so the stage asks for no history of its own where the task's text asked for one a workflow (the lead accepts: one reading); `yunta list` and `list_workflows` print as before. `n` on the field opens the new run in this task, since the target's cut presses it. A defect found and fixed where it starts: a refusal or a failure of other work reset a start still under way, so the started run was dropped and a second `enter` could start another; it is said in the note now and the start stays waited on. The kit: `HomePlace::made_alone(WithFragua)`; the place reaches its forge as a real workspace does (its token unset, as the target says) and fixes its pool's budget (`98.9G`), which the target's figure took from this machine's disk. Landed: `new-run` at the four sizes, no row contested. The walk: 72,016 states (the new run 16,856, elsewhere 55,160), 114 palettes, 63 helps, 8,930 screens a size, each cause measured apart with the base reproduced: doctor read as it opens 3,204 more, the panel 1,764 more, `n` on the field 2,816 more, the start no longer reset 8,573 fewer.

**Fails first:**
- `crates/ui/tests/new_run.rs`: opening asks the catalog and doctor once each, and choosing a workflow twice asks its history once; with the three finished runs of one workflow, its panel says `3 past runs` and the median. Red until the reads are asked.
- `crates/tui/tests/screens.rs` with `new-run` against its target.

**Acceptance criteria:**
- [x] The walk records each read the panel asks, by its kind (`crates/testkit-ui/src/recorded.rs`).
- [x] `new-run` equals its target at the four sizes.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test new_run --test walk`, `cargo test -p yunta-tui --test screens`

**Dependencies:** Task 143

**Files likely touched:** `crates/ui/src/stage/start.rs`, `crates/ui/tests/new_run.rs`, `crates/testkit-ui/src/recorded.rs`, `crates/tui/tests/screens.rs`, `crates/tui/goldens/screens/`

**Estimated scope:** M

### Task 145: a run started from it, and every way to it

**Description:**
- **`enter`** asks admission with every row. A refusal is said in place: under the row it names (a missing input), the focus moved there, else above the row that says what `enter` does. Nothing is asked again and what was typed stays. `Started` puts the run's screen in its place.
- **`Stage::opened_by(&NextStep::Run { workflow })`** opens the new run: blank, or with the workflow chosen.
  - The field's `n` takes it (the run waiting longest went to `a` in Task 140).
  - So do the palette's `start a new run` and the first request's `enter`, with what was typed.
  - So does the workflows' `enter`, with the workflow chosen: `StartStage::picked` goes.
- **`n` on any stage not typing where no prompt stands** is Task 158's. Here it is the field's.
- **The target `new-run-refused` lands** (an admission refused, said in place), moved (`git mv`) into `crates/tui/goldens/screens/`, its state from `yunta_testkit_ui::cut` by its name.

**Found in its turn:** Every way into a new run goes through `Stage::opened_by(&NextStep::Run { workflow })`: the field's `n` and its palette's row, `n` on a run, its chronicle, its numbers and its graph, and `enter` on a workflow that reads; the first request's `enter` opens it with the request in its line (`StartStage::asked`). The new run opens at once and reads its catalog and doctor itself (`Want::NewRun`, `StartStage::{new, picked}` and each stage's own wait on the catalog are gone); its workflow row stands once the catalog lands. The lead's ruling built: a kept new run opened again reads the catalog again and keeps its choices by name, a workflow that is gone falling back to the first. `enter` asks admission with every row; a refusal that names an input stands under that row with the focus moved there (`FormRow.refusal`), any other in the note; nothing is asked again and what was typed stays. Landed: `new-run-refused` at the four sizes and `field-palette` (74 in `LANDED`; `held_but_a_row.rs` holds three targets). Two tests at a pseudo-terminal wait on `what runs` before `enter` (they raced the catalog's reading; 20 of 20 beside 8 busy loops). Nothing a command prints changed. The walk: 58,322 states (the new run 18,552, elsewhere 39,770), 114 palettes, 63 helps, 8,896 screens a size, on the tree before the task stood up beside this one: the states of other stages waiting on a catalog for a new run fall from 15,706 to 309, the new run reading its own adds 1,798, the kept run's second reading none; 102 fewer and 7 more were not traced. The walk records no refused start, so the refusal in place is held by its target and its tests, not walked. `n` on other stages than these is the task of every key's.

**Fails first:**
- `crates/ui/tests/new_run.rs`: a workflow whose required input is left empty; `enter` says admission's refusal under that input's row and keeps the request.
- `crates/ui/tests/field.rs`: `n` opens the new run with nothing chosen.

Both are red: the refusal asks the questions again, and `n` follows a waiting run.

**Acceptance criteria:**
- [x] `crates/cli/tests/interface/start.rs`: an input left empty is refused as `yunta run` refuses it, said in place. The run started at the first request is followed to its end through this build.
- [x] `new-run-refused` equals its target at the four sizes.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test new_run --test field --test catalog --test palette`, `cargo test -p yunta --test interface start`, `cargo test -p yunta-tui --test screens`

**Dependencies:** Task 144

**Files likely touched:** `crates/ui/src/stage.rs`, `crates/ui/src/stage/{start.rs,field.rs,first_run.rs,catalog.rs}`, `crates/ui/tests/{new_run.rs,field.rs,catalog.rs}`, `crates/cli/tests/interface/start.rs`, `crates/tui/tests/screens.rs`, `crates/tui/goldens/screens/`

**Estimated scope:** M Every way into a new run changes in one step: the compiler names every caller of the stage that goes.

### Task 146: a new run's goldens, and starting at a terminal

**Description:**
- **The targets `new-run-inputs`** (a row for each other input the workflow declares, and the adapter's) **and `new-run-mode`** (the focus on the mode, the chosen one marked) **land**, moved (`git mv`) into `crates/tui/goldens/screens/`, each state from `yunta_testkit_ui::cut` by its name: the repository holding the three past runs.
- **`crates/cli/tests/application/starting.rs`**, bare `yunta` at a pty:
  - from the first request: words typed, `enter`, the new run with the request in its line, `enter`; the run starts detached and is followed;
  - from the field: `n`, a workflow and a mode chosen, `enter`;
  - from the workflows: `enter` on one, then `enter`.
- Each run is listed by `yunta list` and goes on once the application leaves.

**Found in its turn:** `new-run-inputs` and `new-run-mode` land at the four sizes with no change to the product (76 in `LANDED`), no row contested. Three tests at a pseudo-terminal start a real run, one for each way in: from the first request (the request typed, `enter`, `enter`), from the field (`n`, the request, then a workflow, a mode and an adapter that are not the first, `enter`), from the workflows (`w`, a row, `enter`, the request, `enter`). Each sees the run listed and followed as an engine of its own, reads back what was chosen (the workflow and the mode from `yunta status --json`, the request and the adapter from the frozen manifest, which no command prints), leaves through the field, sees the run still running, lets it go and sees it finish. No run is left behind: the test's `Hold` is dropped before its terminal, which ends the held node; with a failure forced after the start nothing was left running. Red with the stage's start dropped: `no run is listed but those before`. Where the task's text gave way: the held workflow's request is typed between the two `enter`s from the workflows; the field's test chooses an adapter too, named by its checkout's runners. Each test 50 times with ten workers, and again beside 8 busy loops: none failed. The lead's ruling on its finding: the adapter's row offers only what a run here can be pinned to, the adapters the project's runners name and this binary builds (admission's own set), where it offered every adapter the binary builds and admission refused one no runner names. The set reaches the stage with the catalog's reading (`Catalog.pinnable`, from the one owner admission reads), and a kept new run opened again keeps its adapter only while the runners still name it. The home's set-up place declares its runners on both adapters, as a repository that is set up does (the lead's ruling: with none the row held one word and the walk stopped walking the adapter's choices; no landed frame moved). The walk: 57,794 states, 114 palettes, 63 helps, 8,880 screens a size; the 528 states and 16 screens under the earlier count are an adapter chosen before the catalog landed, which the row no longer offers.

**Fails first:** each pty test red with the stage's `Operation::Start` dropped for the run: no run is listed; `crates/tui/tests/screens.rs` with `new-run-inputs` and `new-run-mode` against their targets.

**Acceptance criteria:**
- [x] `new-run-inputs` and `new-run-mode` equal their targets at the four sizes; the inventory's rows S2 differ in none.
- [x] Each new pty test passes 50 times under `stress_binary.py` with ten workers.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --test screens`, `cargo test -p yunta --test application starting`
- [x] `python3 .plans/yunta-tui/tools/targets_check.py crates/tui/goldens/screens`

**Dependencies:** Task 145

**Files likely touched:** `crates/cli/tests/application/starting.rs`, `crates/tui/tests/screens.rs`, `crates/tui/goldens/screens/`

**Estimated scope:** M

### Group check: a new run
- [x] The suite's groups green. A run started from one screen leaves what `yunta run` leaves with the same choices (`new_run.rs` and the interface test). The walk's count and time against Phase 3's, each difference named. `pages.sh` unchanged since the group check of render's phrases.

**Found in its turn:** the whole gate (`.plans/yunta-tui/proof/live-screens/check-a-new-run-4/`). Its first try stopped at the dependency check on an advisory published since the last gate, RUSTSEC-2026-0592 (a stack overflow in `url` 2.5.8, under `reqwest` and `rmcp`): `Cargo.lock` takes the advisory's fix, `url` 2.5.9 and with it `form_urlencoded` 1.2.3, nothing else (`first-try.txt`). Its second try passes every step but one test of 329 binaries and 5,539 tests: this phase's own test of the first request under a typist looked for the attention row's words at once after the tally counted the run, and the two land a step apart by design (a run is counted as soon as its gate is in the log, named once the application has read that the engine that asked at its own terminal is gone); it waits on the row now and never failed in 50 runs with ten workers beside 12 busy loops (`after-the-gate.txt`). The release binary weighs 31,707,920 bytes (49,600 more than the last group's). Against the first request's captures no view of 159 differs: no printed page changed, and the captures open no new run, which its targets and its tests at a pseudo-terminal hold. The goldens: 7 changed, each Surco's catalog of components gaining the form's sample; 14 new, the form's own; 20 moved from `targets/` to `screens/` (`new-run`, `new-run-refused`, `new-run-inputs`, `new-run-mode`, `field-palette`); digests written (`digests-a-new-run-4.txt`, 853 goldens). No process of the worktree's tests left running, before or after. The walk as Task 146 left it: 57,794 states, 114 palettes, 63 helps, 8,880 screens a size.

### Task 147: a pack fetched from a directory inside its repository

**Description:**
- **`Packs::fetch`** takes `<repository>//<directory>[@ref]`: the repository cloned, the ref checked out, then the manifest read, the audit made and the tree hashed and vendored at `<directory>`. `vendor::split_source_and_ref` splits the directory off the source.
- The lock records the source with its directory, so `yunta pack update` fetches it again from there.
- **`yunta pack add`** takes the form, since it calls the same fetch.
- `vendor.rs`' `//!` says the form; `docs/packs.md` adds it to what `pack add` accepts.

**Found in its turn:** `yunta pack add` and `Packs::fetch` take `<repository>//<directory>[@ref]` (`crates/api/src/packs/source.rs`, `PackSource`, `Cloned`): the repository is cloned and its ref checked out, and the pack's directory alone is read, audited, hashed and vendored; the lock's `source` keeps the form, so `yunta pack update` fetches from the same directory again. A source is held inside its clone, each refusal a variant of its own with render's words (`PackRefusal::SourceDirectory`): an absolute directory and one that climbs with `..` are refused before anything is cloned, any symbolic link on the way to the directory is refused (vendoring follows none), and a directory the repository does not hold is said so. Red: the whole source went to git (`repository '…/monorepo//packs/demo' does not exist`), nothing having read the form. What `yunta pack add` prints for such a source: the inventory and `✓ pack acme/review-pack · installed · 1.0.0 · <sha>`, where it printed git's error; each refusal in the product's words, where git's stood; `--help` names the form. Every test fetches from a repository the test makes.

**Fails first:** `crates/api/tests/packs.rs`, `a_pack_at_a_directory_of_its_repository_installs_as_one_at_a_root_does`: a local repository holding the pack at `packs/demo` installs with the same manifest, audit and vendored tree as the same pack at a repository's root, and its lock entry names the directory. Red: the fetch reads the root's `pack.yaml`.

**Acceptance criteria:**
- [x] `crates/cli/tests/pack_cmd.rs`: `yunta pack add <local>//packs/demo` installs, and `pack update` to a later commit moves it.
- [x] Every other pack test passes unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api --test packs`, `cargo test -p yunta --test pack_cmd --test pack_provenance_cmd --test factory_packs_cmd`

**Dependencies:** Group check: a new run. The group runs in order: each task reads what the one before it adds.

**Files likely touched:** `crates/api/src/packs.rs`, `crates/api/src/packs/{vendor.rs,fetched.rs}`, `crates/api/tests/packs.rs`, `crates/cli/tests/pack_cmd.rs`, `docs/packs.md`

**Estimated scope:** M

### Task 147a: a candidate may leave its model to its adapter

Added in Phase 4 by the lead's ruling on Task 136's finding (2026-11-21): the approved setup draws each role
with its adapters alone and a `✓`, goes from its write to the first request, and Task 148 writes candidates with no
model; the configuration's candidate requires a model, so a role answered with none is written commented, does no
work, and the first request is refused. The owner was asked on 2026-11-21 and may rule otherwise before this
task starts.

**Description:**
- **`RunnerCandidate.model`** (`crates/core/src/config/sections.rs`) becomes optional. A candidate that names no model leaves it to its adapter's CLI, as an absent `effort` already does: the runner is declared, a run starts on it, and its session is opened with no model named (`SessionRequest.model` already takes none, and the ledger already hears the model a session reports).
- **The run path** (`crates/engine/src/run/session_plan.rs`) hands the candidate's model as it is. Whatever reads a model to decide something (pricing, a capability by model) reads the model the session reported.
- **The receipt** (`crates/replay/src/receipt.rs`) names the model the session ran, heard from it where the candidate named none.
- The configuration's schema is written again (`cargo xtask schema`, `crates/core/schemas/`). `docs/reference.md` says the key is optional and what its absence means.
- Knowledge: the field's `///` says what an absent model means; nothing else.

**Found in its turn:** A runner's candidate that names no `model:` is a declaration like any other (`RunnerCandidate.model: Option<ModelName>`, left out when not named): a run starts on it, its session opens with no model named (neither adapter passes `--model`), and the node's ledger holds the model the session reports. The receipt's `runners[].model` is the model the candidate names, else the first its node's sessions reported, else the key is left out. What decides by a model reads none of the candidate's: pricing averages the `pricing:` entries, estimates are a workflow's, runner resolution picks by adapter and pin, and `yunta stats` lists the models the sessions reported. The event log: `runner_resolved`'s `chosen` holds no `model` for such a candidate, and the frozen config `- adapter: claude-code`; a log written before reads as before and the kept recording of the design's run is the same byte for byte; a binary of the base does not read a run that leaves the field out (it fails as a corrupt payload, and the manifest's config does not parse), which the events module's `//!` now says: a field every log holds may become optional. The schema's diff is the one key: `model` leaves `required` and admits null (`config.json`, `events.json`). Red: core's configuration with no model failed on `missing field 'model'`; the engine's run on such a runner failed to parse its config; the receipt held no model where the session had reported one. What a real run on each adapter must show, for the proof's group: no `--model` in the CLI's arguments, `runner_resolved.chosen` with no `model`, the session's own model on `agent_session_opened` (Codex's stream names none, so its rail's summary fills it), `yunta stats` and the receipt naming it, the node's title `planner on claude-code` until the session reports. The suites whole: core 859, replay 211, adapters 332, api 132, engine 1,497, none failing.

**Fails first:**
- core: a configuration whose candidate names an adapter and no model reads, and is written back without one. Red: the model is required.
- engine: a run whose runner's one candidate names no model opens its session with no model named, and its node's ledger holds the model the session reports (the mock adapter says one). Red: it does not compile; the candidate has a model.

**Acceptance criteria:**
- [x] A configuration and a log written at the base read and run as before; the kept recording of the design's run is byte for byte the same.
- [x] The schema's diff is the one key's optionality.
- [x] A real run with a role that names no model, on each adapter, stands in the proof's group: its receipt and its stats name the model the session reported.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-core config`, `cargo test -p yunta-engine`, `cargo test -p yunta-replay receipt`
- [x] `cargo xtask schema`, its diff reviewed.

**Dependencies:** Task 136. Before Task 147b and Task 148.

**Files likely touched:** `crates/core/src/config/sections.rs`, `crates/core/schemas/`, `crates/engine/src/run/session_plan.rs`, `crates/replay/src/receipt.rs`, `docs/reference.md`

**Estimated scope:** M

### Task 147b: a role with no model is declared, and said as it is

**Description:**
- **Every sentence that names a runner's candidate** says what it knows: the adapter alone where the candidate names no model (`planner on claude-code`), with the model where it names one or the session has reported it. The node's title (`crates/render/src/blocks/node_title.rs`), the chronicle (`chronicle/words.rs`), the watched lines (`watched/mod.rs`), doctor's session row (`page/doctor/session.rs`) and what `init` prints (`page/initialized.rs`) take it from one owner of that phrase.
- **`ProjectDraft::answered`** (`crates/api/src/setup/init.rs`) declares a role answered with an adapter and no model; a role no adapter answers for is still written commented. Doctor's session check (`crates/api/src/machine/doctor/session.rs`) opens its session as the run would.
- **The words**: the hint under the model's box and `init -i`'s prompt say `enter with none leaves it to the adapter`, where they said it was left to declare later (`render::prompt`). The target `setup-changing` carries the old hint: its row is corrected by the lead's ruling at the four sizes and listed before and after for the owner.

**Found in its turn:** One owner says a runner's candidate (`render::runner::{candidate, on}`): `claude-code` or `claude-code/opus`, `planner on claude-code` until a model is named or the session has reported one; the node's title and page, the chronicle, the watched row, doctor's session label, the setup's role rows and what `init` prints go through it (doctor's row names the binding as the config's line does, so a reader finds it there). `ProjectDraft::answered` declares every role answered with an adapter, with its model where one was typed; a role nothing answered for, or a machine with no adapter, is still written commented. The hint under the model's box and `init -i`'s prompt say `enter with none leaves it to the adapter`. The target `setup-changing` is corrected in that row by the lead's ruling (120 columns and ASCII row 27, 80 columns row 20: `enter with none leaves it to declare later` to `enter with none leaves it to the adapter`; 60 columns rows 9 and 10: the hint on one row for two, the freed row going to the window when the screen is drawn); the inventory's row and the notes follow. What commands print and write: `init -i` with a model left empty writes `runners: implementer: - adapter: claude-code` where it wrote the role commented, its page gains `runners      implementer on claude-code` and loses the block that told how to declare one; with every model named it writes the same and only the prompt's words change; with no adapter on the machine nothing changes; `init` unasked is unchanged here (the next task makes it write what the setup proposes, so that leaving every question and `init` unasked write the same again). No golden moved; one case added (`initialized/declared-on-its-adapter`). Two tests that held "every question left writes what `init` unasked writes" compare with `init -i` left at every question until then. For the next task: the advice to declare a runner still shows `model: <model>` (`render::declare`, doctor, check, `init`'s runner step, the commented proposal), where the adapter alone is now enough. The walk's counts stand: 57,794 states, 114 palettes, 63 helps, 8,880 screens a size.

**Fails first:** `crates/api/tests/` (the target that holds `ProjectDraft::answered`): a role answered with an adapter and no model is written declared, and the configuration reads back with that runner. render: a candidate with no model reads `planner on claude-code`. Red: the role is written commented.

**Acceptance criteria:**
- [x] `yunta init -i` answered with no model writes a configuration a run starts on; the prompt's words before and after listed.
- [x] The goldens that say a runner's candidate are blessed and their diff reviewed: none moves where a model is named.
- [x] `setup-changing`'s corrected row holds at the four sizes; `targets_check.py` and `targets_page.py --check starting` pass once the lead's notes follow.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api setup`, `cargo test -p yunta-render`, `cargo test -p yunta --test console_interaction init`, `cargo test -p yunta --test init_new_cmd`

**Dependencies:** Task 147a.

**Files likely touched:** `crates/api/src/setup/init.rs`, `crates/api/src/machine/doctor/session.rs`, `crates/render/src/prompt.rs`, `crates/render/src/blocks/node_title.rs` and the four files named above, `crates/tui/goldens/targets/setup-changing.*`

**Estimated scope:** M

### Task 148: a role declared with its candidates, in order

**Description:**
- **`RunnerDeclaration::Declared`** (`crates/api/src/setup/init.rs:120`) keeps a role's list of `RunnerCandidate`s where it keeps one: the adapter picked first, with the model typed for it, then every other adapter the machine answers for, in the order it found them, each with no model. `ProposedRunners::of` proposes that list. What is written is the configuration's own list for the runner (`crates/core/src/config/mod.rs:74`), which already holds candidates in order.
- **`yunta init -i` writes the same list** with the same answers (`crates/cli/src/commands/init.rs:104-120`): it asks what it asks today, and the candidates after the first follow by themselves. `yunta init` unasked writes them too.
- The page that says what a setup writes names each role's candidates in order, in the design's words (`then`), which Task 136 owns.
- `setup/init.rs`'s `//!` says a role is declared with every candidate the machine answers for, and why: a run goes on with the next when the first cannot be reached.

**Found in its turn:** A role is declared with its candidates in order: the adapter picked, or the first that answered where none was picked, with the model typed for it, then every other adapter the machine answers for, each with no model (`ProjectDraft::answered`; `RunnerDeclaration::Declared` holds every proposed role in the order proposed, `Proposed` stands only where no adapter answers). `yunta init` unasked, `yunta init -i` left at every question and every question left in the application write the same bytes, and the tests hold it again; `init -i` writes byte for byte what the API's draft writes, on one adapter and on two. Where the task's text was not true of the code: a run does not go on with the next candidate when the first cannot be reached (resolution takes the first candidate this binary builds, and an adapter that is not healthy refuses the run); the `//!` says the true reason for the order: a run pinned to an adapter takes the role's candidate on it, and a binary that builds no adapter of the first takes the next. The lead's rulings built: the advice to declare a runner names the adapter alone (`- { adapter: claude-code }`; `render::declare` owns it; doctor, check, `init`'s runner step, the commented proposal and Surco's catalog sample follow); where no workflow names a runner the proposal is its own variant (`ProposedRunners::StandIn`) and the write makes that role the default runner too (`defaults.runner`), which the page says once (`implementer (the default) on claude-code`), so a workflow added later whose node names no runner checks clean, where the role declared alone ran nothing; the page's cases no input of `init` produces are gone (`partly-declared`) or reshaped into one it does (`proposed`, where no adapter answers), and the page takes its runners as declared or as proposed, never some of each. What commands print and write: `init` unasked with an adapter writes `runners: implementer: - adapter: claude-code` (and `- adapter: codex` after it with two) and `defaults: runner: implementer`, where it wrote both commented and told how to declare one; with no adapter the commented proposal reads `- { adapter: <adapter> }` and carries the default commented; `init -i` gains the further candidates and the default; doctor's and check's advice lose `model: <model>`; a repository whose workflows name their runners writes the same config, and its page lists them in the config's order. Today's setup stage shows each role with the candidates the write holds (rows 18 to 22 of `setup` at 120 columns already equal). The walk's counts stand: 57,794 states, 114 palettes, 63 helps, 8,880 screens a size. Left for the module's review: render's page still admits an empty list, a role with no candidate and a default that is not one of its roles, which its one builder (`InitializedProject::page`) never hands it.

**Fails first:** `crates/api/tests/setup.rs` (or the test target that holds `ProjectDraft::answered` today, named in its turn): on a machine answering for two adapters, a role declared with the second picked writes both candidates, the picked one first; with one adapter answering, one candidate, as at the base. Red: the declaration keeps one.

**Acceptance criteria:**
- [x] `yunta init -i` with the same answers writes the same bytes the API's draft writes, on one adapter and on two.
- [x] `crates/render/goldens/{setup,initialized}/` and `init`'s captures are blessed and their diff reviewed: each differs only by the candidates after the first.
- [x] A configuration written at the base still reads and runs as it did: one candidate is a list of one.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api setup`, `cargo test -p yunta --test console_interaction`, `cargo test -p yunta-render setup`
- [x] `YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine`, then the goldens' diff reviewed.

**Dependencies:** None.

**Files likely touched:** `crates/api/src/setup/init.rs`, `crates/cli/src/commands/init.rs`, `crates/render/src/page/setup.rs`, `crates/render/goldens/{setup,initialized}/`, `crates/cli/tests/console_interaction/init.rs`

**Estimated scope:** M

### Task 149: the pack a setup installs, and the runners it proposes

**Description:**
- **`yunta_api::SetupPack { pack: PackRef, source: String }`**, in `crates/api/src/setup/pack.rs` (new): the pack a setup installs. `SetupPack::of_this_binary()` is `yunta/fragua` from the Yunta repository's `packs/fragua` at the binary's version tag (`v` and `CARGO_PKG_VERSION`). The repository's URL is written once, here. `///`: "Not a pack a person names: the one a setup offers."
- **`DetectedProject::proposing(&mut self, pack: &FetchedPack)`**: the roles proposed become those of the repository's workflows and the pack's, in the order the catalog lists them once the pack is installed. So `yunta init -i` after `yunta pack add` proposes the same roles.
- `yunta init` itself asks what it asks today; what it writes for a role is Task 148's list.
- `yunta_testkit` gains the reference pack as a local repository: `packs/fragua` copied and committed under the binary's version tag. It also gains the environment that points the Yunta repository's URL at it (`url.<local>.insteadOf`), handed to git through `Env::subprocess_vars` or a process's environment.

**Found in its turn:** `yunta_api::SetupPack::of_this_binary()` names the pack a setup installs, `yunta/fragua`, and its source, built in one place from the package's own repository and version: `{repository}//packs/fragua@v{version}` (a release binary and a development binary of one version ask for the same tag; a version not yet tagged is out of reach). `DetectedProject::proposing(&FetchedPack)` proposes the roles in the order the catalog lists them once the pack is installed, reading the fetched copy (`Workspace::catalog_workflows_installing`, one rule for naming a pack's workflows) and writing nothing: a repository with no workflow proposes fragua's five roles (`planner`, `executor`, `reviewer`, `reviewer-alt`, `mechanical`; the task's text said four), equal to what its twin proposes once the pack is installed; one with a workflow of its own proposes its roles first. Where the pack cannot be reached the reason is `Packs::fetch`'s own (offline: git cannot resolve the host; no such tag: git's checkout fails), nothing is written, and the proposal stays the stand-in. Red: the import did not resolve; with `proposing` doing nothing, `StandIn` stood where the five roles were due. Every test fetches from a repository it makes. `yunta init` prints and writes as before. The walk's counts stand (57,794). For the next task: the row that says why the pack is out of reach is handed git's own words (the error's first line names the command and a temporary directory, where the target draws `fatal: …`); a detection that also fetched would make the rows wait on the network; `Packs::fetch` refuses a pack already vendored in a repository with no config.

**Fails first:** `crates/api/tests/setup_pack.rs` (new):
- a repository with no workflow, its draft proposing with the local fragua fetched, proposes fragua's four roles in the order `init` proposes them once the pack is installed;
- `SetupPack::of_this_binary()` names the binary's version.

Red until the items exist.

**Acceptance criteria:**
- [x] `crates/api/tests/public_items.rs` passes: no new public item names a type below the API.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api --test setup_pack --test public_items`

**Dependencies:** Task 147, Task 148

**Files likely touched:** `crates/api/src/{setup.rs,setup/pack.rs (new),setup/init.rs,lib.rs}`, `crates/api/tests/setup_pack.rs (new)`, `crates/testkit/src/{lib.rs,reference_pack.rs (new)}`

**Estimated scope:** M Two of its paths are its test and the testkit's stand-in for the pack; the compiler names the rest.

### Task 149a: a setup follows its repository's field

Added in Phase 4 by the lead's ruling on Task 136's finding (2026-11-21): a setup's screen says two things that
are the field's to say, the tally at its top right (`nothing needs you`) and `This repository has never used
yunta.`, and while a setup stands the follower follows no field, so neither is known to be true (a checkout of a
branch without the config, in a repository whose runs wait on the person).

**Description:**
- **`Stage::field_reach`** (`crates/ui/src/stage.rs`) gives `FieldReach::Here` for a setup, as it does for the first request: the follower keeps this repository's field while the setup stands, so the tally and the attention row say what its runs need, and a run that turns to need the person calls them there as on any screen.
- **The sentence's first clause** follows what the field holds, as a type that names the two states (never a bare `bool`): `This repository has never used yunta.` where the field holds no run, `This checkout holds no yunta setup.` where it holds one; the rest stands (`Here is what it found. Nothing is written until you say so.`). Until the field's first news the sentence is not drawn, as the rows under it wait for the detection: no sentence is shown and then taken back.
- `Workspace::repository` reads no runs, as it says: the fact is the field's.

**Found in its turn:** A setup follows its repository's field (`Stage::field_reach` gives `FieldReach::Here` for it): its tally and its attention row come from `FollowedRuns::framing`, the row naming no key since `a` is not bound there, and the first clause of its sentence follows what the field holds (`RepositoryPast::{NeverUsed, Used}`, `Setup::sentence(past)`): `This repository has never used yunta.` where it holds no run, `This checkout holds no yunta setup.` where it holds one, read or not; no sentence is drawn before the field's first news, and a field kept from another reach is never read as this repository's. `Project::repository` reads no runs, as its doc now says (`NeverSetUp`: never set up as far as the checkout says). Red: a store holding a waiting run showed the design's sentence and no tally. What bare `yunta` shows on a setup: the tally on the trail's row (`nothing needs you`, `◆ 1 needs you`), an attention row where a run waits, the first clause by the field. The frames of `setup` and `setup-unreachable` equal rows 1 and 2 now at the four sizes; none moved. The walk: 64,533 states for 57,794, 114 palettes, 63 helps, 9,066 screens a size for 8,880; with the setup's following switched off it gives the base exactly, and by the stage the person is at the rise is the setup itself (32 to 128), a run's screen opened in the place never set up (3,203), its numbers (390), a workflow's graph (69), a new run opened from those (2,661) and the field reached from them (320), every other stage standing. The kit's recordings follow the field over a setup and keep those places' field news out of the walk's field (`FieldNews::Dropped`).

**Fails first:** `crates/ui/tests/setup.rs`: a setup opened in a repository whose store holds a run of it says `This checkout holds no yunta setup.` and counts that run in its tally; in one with none it says the design's sentence. Red: the setup follows no field; the sentence is one const.

**Acceptance criteria:**
- [x] The landed and unlanded frames of the setup hold unchanged (their cut holds no run).
- [x] The walk's counts are told before and after, and any change explained by the states the setup gains.
- [x] A run of this repository that waits on the person is named on the setup's attention row (a test at the stage).

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test setup`, `cargo test -p yunta-render setup`, `cargo test -p yunta-tui --test walk`

**Dependencies:** Task 136. Before Task 150.

**Files likely touched:** `crates/ui/src/stage.rs`, `crates/ui/src/stage/setup.rs`, `crates/render/src/page/setup.rs`, `crates/ui/tests/setup.rs`, `crates/render/src/tests/setup.rs`

**Estimated scope:** S

### Task 150: the setup's work

**Description:**
- **`Operation::DetectProject`** detects the repository, reads who git commits as and fetches `SetupPack::of_this_binary()`. The draft proposes the pack's roles where it was fetched, and the project and the pack are held together under its ticket (`Found::Setup { project, pack }` in `performer/held.rs`, in place of `Found::Project`).
- **`Outcome::Detected`** carries the draft, the ticket, the committer, and the pack as fetched (its reference and audit, what a person confirms) or why it could not be.
- **`Operation::WriteSetup { held, setup, pack: InstallsThePack }`**, `InstallsThePack::{Yes, No}` (new), writes the setup. Where the pack is confirmed it then installs the held copy, confirmed, as `yunta pack add --yes` does, and reads what the installed pack needs (`installed.needs`). The held pack is let go otherwise.
- The walk's recording of the setup (`crates/testkit-ui/src/recorded.rs`' `set_up`) performs both answers. The home's `to_set_up` and `clis` places reach the local reference pack through the workspace's `subprocess_vars`.
- `performer.rs`' `//!` and `held.rs`' say what the setup holds.

**Found in its turn:** The setup's work is three pieces: `DetectProject` detects the repository and reads who git commits as (`Outcome::Detected { draft, held, committer }`); `FetchSetupPack`, asked beside it as the setup opens (the lead's ruling: a person waits for the slower of the two, not their sum), answers `Fetched { pack, audit, proposed }`, `Installed` or `OutOfReach { why }` (git's own words, through `Failure::in_its_own_words()`); `WriteSetup { held, setup, pack: InstallsThePack::{Yes, No} }` installs the pack confirmed and then writes the config (a failed install leaves the repository as it was: the task's text had the other order), or lets the pack go. The two readings join in the performer's `Held`, the one place both copies live, whichever lands first; the roles come from the fetched copy (`FetchedPack::proposed()`). The lead's ruling built: a pack the repository already vendors needs no network (`SetupPack::fetched` looks locally first, `Packs::vendors`), so offline with fragua vendored the row says `yunta/fragua, installed as a pack` and the write row `write .yunta/config.yaml` alone. Red: the stage's test did not compile (no such operations); the fetch was not asked until the detection landed; a vendored pack with its source out of reach read as out of reach. Held: the committer, fragua's audit and its five roles; the write with the pack and without, each equal to its twin's config, lock and vendored tree; out of reach says `fatal: … does not appear to be a git repository` and writes the setup alone. What bare `yunta` shows in a repository never set up: a `commits as` row and a `workflow` row once the fetch lands, fragua's five roles where it was fetched, and writing installs it; `init -i` is unchanged. A sentence no target gives: `yunta/fragua, installed as a pack`.

**Fails first:** `crates/ui/tests/setup.rs`:
- `DetectProject` in a repository whose git points the Yunta URL at the local fragua answers with the committer, fragua's audit and fragua's roles;
- `WriteSetup` with `Yes` leaves `.yunta/config.yaml`, `yunta.lock` and `.yunta/packs/yunta/fragua` equal, byte for byte, to a twin repository's after `Packs::fetch` and `install` then `DetectedProject::write` with the same answers;
- with `No`, the twin's without the pack.

Red until the operations carry the pack.

**Acceptance criteria:**
- [x] With the Yunta URL pointing at nothing, `Detected` says why the pack cannot be reached, and `WriteSetup` writes the setup alone.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test setup --test walk`

**Dependencies:** Task 149

**Files likely touched:** `crates/ui/src/{operation.rs,outcome.rs,performer.rs,performer/held.rs,performer/pack.rs}`, `crates/ui/tests/setup.rs`, `crates/testkit-ui/src/{recorded.rs,home.rs,sandbox.rs}`

**Estimated scope:** M An operation's shape changes with every place that performs or records it; the compiler names them.

### Task 151: a setup's rows changed in place

**Description:**
- **`SetupStage`** becomes one screen of rows (S3):
  - what the repository is, each fact a row; the base branch and the project's name are rows that change;
  - who does the work, one role a row with its candidates in order, as the design draws them (`claude-code/… then codex/…`): the first is the adapter picked, the first answering by default, with the model typed, none by default; the others the machine answers for follow;
  - the write row.
- **The pointer starts on the write row.** `j k` move along the rows that change.
- **`enter` on a row changes it in place.** On a role it opens a `WordChoice` of the adapters this machine answers for (`←→`, `enter` takes it), then the model's typed line in place. On the name or the base it opens the typed line. `esc` gives the row back as it was. `enter` on the write row writes (Task 152 puts the pack's audit before it).
- **`q` leaves with nothing written.**
- The wordmark and the horizon stand as on the first request. The trail is `set up`; the tally (`nothing needs you` where nothing does) and the attention row come through Phase 3's one composer, `FollowedRuns::framing` (Task 92); the legend goes by the row pointed at.
- The sequence goes: `Asking`, `Answered`, `next_runner`, `model_asked`, `typed`, and today's legends `setup_legend` and `written_legend`.
- **The candidates after the first** stand on the row from 102 columns, as the spec's sizes say, and are written with it. `enter` on a role picks which adapter stands first; the order of the rest is the machine's.
- The stage's `//!` says the rows and what writing asks.

**Found in its turn:** The setup is one screen of rows, on a new Surco component (`Survey`): the pointer starts on the row that writes; `j`, `k` and the arrows step along the rows that change; `enter` on the name or the base opens a line typed in place (an empty one keeps the value), on a role a choice of adapters (`←→`, `enter take it`) and then the model's box; `esc` gives the row back; `enter` on the write row writes; `q` and ctrl-c leave with nothing written; `a` goes to the run the attention row names, as the spec binds it wherever no line is typed. The window keeps the act's row and the pointed row in sight (`… n above`, `… n under`) and gives way in order: the blank row under the sentence, the name in block letters, then the window. Surco's measure is extended, not laid out a second way (`Measure { cells, lowered }`, `Width::in_a_pane`, `across`): the sentence stands whole, centred over the 92-cell column. Today's questions one at a time are gone with `answering_legend`. Landed, held whole at the four sizes: `setup`, `setup-unreachable`, `setup-changing` (79 in `LANDED`). The lead's rulings on `setup-changing` at 60x14: its rows written from what the code draws (the freed row of the shorter hint goes to the window: `… 10 above` and `reviewer` shown), and its legend whole (` ←→ adapter · enter take it · esc keep the detected default`: the frame's cut came from the notes' `keeps`, a cell longer). The stage was written before its test; with opening in place taken out the test fails on "the base opens a line typed in place". Sentences no target gives: `j k step · enter change it · q leave, nothing written`; `type the name · enter take it · esc keep the detected default` (and for the base and the model); `esc keep it as it was`; ``enter with none keeps `yunta` `` and the `name` and `base` box titles; `… n under`. `/` and `?` open on a setup's rows. The walk: 64,806 states, 114 palettes, 67 helps, 9,177 screens a size; by stage the setup holds 132 on its rows and 52 with a row open, `a` on a setup adds 217 across the stages it reaches, and every other stage stands; `MUST_REACH` holds 88. For the next tasks: `enter` installs fragua without its audit yet (the audit is held in the fetch's answer); what the installed pack needs is carried by `Initialized.installed`.

**Fails first:** `crates/ui/tests/setup.rs`, `the_rows_changed_in_place_make_the_setup_init_writes`: a base typed in place, one role's adapter changed and a model typed, then `enter` on the write row. The setup asked equals what `ProjectDraft::answered` makes with the same answers, `init -i`'s. Red: the stage asks in sequence.

**Acceptance criteria:**
- [x] `escape_through_every_question_writes_what_init_writes_unasked` becomes `enter_at_once_writes_what_init_writes_unasked`: the pointer starts on the write row, and one `enter` (with the pack declined) writes what `init` writes unasked.
- [x] `a_setup_left_before_it_is_written_writes_nothing` holds with `q`.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test setup --test walk`

**Dependencies:** Task 150

**Files likely touched:** `crates/ui/src/stage/setup.rs`, `crates/render/src/screen/setup.rs`, `crates/ui/tests/setup.rs`

**Estimated scope:** M

### Task 152: the pack's audit and its confirmation

**Description:**
- `enter` on the write row, where the pack was fetched, shows its audit as `yunta pack add` shows it before it installs (S4, render's `page::pack::Inventory`). Over it stands a confirmation in Task 127's box: the pack named, what installing does, `yunta pack add <source>` that does the same, and the legend Task 132 words.
- **Confirmed:** `WriteSetup` with `Yes`. **`esc`:** `WriteSetup` with `No`, which writes the setup without it (decision 5). An unreachable pack has no audit, and the write row writes the setup alone.
- **Written:** a page says what was written and what the installed pack still needs, as `setup-written` draws it. `enter` goes on from it to the first request ("then yunta asks what to build").

**Found in its turn:** `enter` on the write row, where fragua was fetched, shows its audit (the trail `set up › yunta/fragua`, the arrows scroll it, `esc` gives the rows back); `enter` on the audit stands the confirmation, a variant of its own in render's closed set (`Confirming::SetUpWith { pack, writes }`), which names `yunta pack add <the source this binary asks for>` and `yunta init`; `enter` there writes the setup with the pack, `esc` writes it without (`esc write without it`), as `init` writes with the same answers. The confirmation holds through followed news (a run that comes to need the person is counted, the box stands, `enter` still writes). The lead's ruling built: the box says `It writes .yunta/config.yaml and installs yunta/fragua. Nothing outside .yunta/ is touched but .gitignore.`, render working out what is touched outside `.yunta/` from the paths written, and the rows of `setup-confirm` that carried `Nothing else is touched.` are corrected (120 columns and ASCII row 12, 80 columns row 8; at 60 columns rows 5 to 10, the sentence taking a row more and the box's last blank row giving way). Surco: a page leaves its empty furrow to the screen's rule (`Region::of_a_page`). Red: with the audit taken out of the write row, six tests saw `enter` write at once. Not landed: `setup-audit` and `setup-confirm` are held everywhere but one row in ASCII (row 18: the pack's own `—`, which the frames fold to `--`: Task 140a, on the owner's answer). What bare `yunta` shows: `enter` on the write row showed nothing between it and the write; it shows the audit, then the confirmation. The walk gains the box over the audit (4 states); `MUST_REACH` holds 91.

**Fails first:** `crates/ui/tests/setup.rs`:
- `enter` on the write row shows fragua's audit and a confirmation, and asks no work;
- `enter` then asks `WriteSetup` with `Yes`, `esc` with `No`.

Red until the audit stands before the write.

**Acceptance criteria:**
- [x] Holding the confirmation through followed news leaves it, as Task 139's holds.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test setup --test dialogs --test walk`

**Dependencies:** Task 151

**Files likely touched:** `crates/ui/src/stage/setup.rs`, `crates/ui/tests/setup.rs`

**Estimated scope:** S

### Task 153: a setup's goldens, and setting up at a terminal

**Description:**
- **The targets of the setup land**, moved (`git mv`) into `crates/tui/goldens/screens/`: `setup` (as it opens), `setup-changing` (a role's adapter changed and its model being typed), `setup-audit` (the pack's audit), `setup-confirm` (its confirmation), `setup-written` (the page that says what was written and what the installed pack still needs) and `setup-unreachable`. Each state comes from `yunta_testkit_ui::cut` by its name: the home's never-set-up repository with the `claude` stub answering and its git pointing the Yunta URL at the local reference pack (Task 149), or at nothing for `setup-unreachable`. `TARGET_CUTS`' entries for the setup gain that here.
- **`crates/cli/tests/application/repository.rs`**, `a_repository_never_set_up_opens_on_its_setup_and_writes_what_init_writes_with_fragua_installed`, replacing the base's setup test. Bare `yunta` at a pty, with `GIT_CONFIG_*` pointing the Yunta URL at the local fragua:
  - one role's model typed in place;
  - `enter` on the write row, the audit, `enter`.
  - What stands equals a twin repository's after `yunta pack add <source> --yes` and `yunta init -i` with the same answers: config, lock, vendored tree.
  - The first request follows.

**Found in its turn:** The written page is the pack's installed page with a note under it (`✓ Wrote .yunta/config.yaml and installed yunta/fragua. Nothing outside .yunta/ was touched but .gitignore.`, or `… was touched.` where the write did not touch `.gitignore`), `PackInstalled::page` the one owner of that page for the catalog and the setup; without the pack it is `init`'s page with the note (a page no target draws); `enter` goes on to the first request. The lead's ruling built in the frames: `setup-written` row 35 at 120 columns and in ASCII, row 23 at 80 (`Nothing else was touched.` to `Nothing outside .yunta/ was touched but .gitignore.`). The test at a pseudo-terminal sets a repository up from bare `yunta`: its config, its lock and fragua's tree equal a twin's after `yunta pack add --yes` and `yunta init -i`, and the first request follows; red with the pack's install dropped (no lock written). Stressed 50 times with ten workers, and again beside 8 busy loops: none failed. The walk's table (step before these tasks): the walk writes its states by class where `YUNTA_WALK_TABLE` names a file (`crates/testkit-ui/src/walk/classes.rs`), nothing of what it tells apart changed; against the table before these tasks one class moved (`Setup` under a confirmation, 4 more) with one help and six screens more: 64,810 states, 114 palettes, 68 helps, 9,183 screens a size. For later tasks: the audit's page is not scrolled by the walk (its scroll is not among the panes the walk scrolls); the pages' tasks use `Region::of_a_page`.
 The group's one pass of fixes: the three frames at 60x14 are corrected from what the code draws, the conventions and the approved `pack-audited.unicode-60` letting a page's blank rows go there (`setup-audit` rows 4 to 12 and its count of lines, 497 for 514; `setup-confirm` row 12, the same count; `setup-written` rows 4 to 7), and `setup-written` lands (80 in `LANDED`); the confirmation names `yunta init -i` where a row was changed (`NextStep::Init { interactive }`), since `yunta init` writes the same only where none was; no class of the walk moved.

**Fails first:** the pty test red with `InstallsThePack::Yes` treated as `No` for the run: the lock is missing. `crates/tui/tests/screens.rs` with the six targets above, red until their cuts reach the reference pack.

**Acceptance criteria:**
- [x] `setup`, `setup-changing`, `setup-audit`, `setup-confirm`, `setup-written` and `setup-unreachable` equal their targets at the four sizes; the inventory's rows S3 and S4 differ in none.
- [x] `crates/cli/tests/console_interaction/init.rs` passes: `init`'s own flow is unchanged.
- [x] The new pty test passes 50 times under `stress_binary.py` with ten workers.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --test screens`, `cargo test -p yunta --test application repository`, `cargo test -p yunta --test console_interaction init`
- [x] `python3 .plans/yunta-tui/tools/targets_check.py crates/tui/goldens/screens`

**Dependencies:** Task 152

**Files likely touched:** `crates/cli/tests/application/repository.rs`, `crates/testkit-ui/src/cut.rs` (Task 25's, new in that phase), `crates/tui/tests/screens.rs`, `crates/tui/goldens/screens/`

**Estimated scope:** M

### Group check: a setup
- [x] The suite's groups green. A setup written from one screen leaves what `yunta init -i` leaves with the same answers, with `yunta/fragua` installed as `yunta pack add` installs it, at a pty. `pages.sh`: `pack add`'s captures equal the base.

**Found in its turn:** the whole gate, every step passing at its first try (`.plans/yunta-tui/proof/live-screens/check-a-setup-4/`): 330 binaries, 5,590 tests, none failing, none aborted. The release binary weighs 31,757,568 bytes (49,648 more than the last group's). Against the new run's captures 4 of 159 views differ, each by this group: `init` declares its runner (`runners      implementer (the default) on claude-code, then codex`) where it told how to declare one; `init -i`'s prompt says `enter with none leaves it to the adapter` (two views); and `yunta new -i` right after `init` says `✓ shaped.yaml · written · lint-fix · can run here` where it said `▲ … 1 error` (its node named no runner and the config declared no default: the stand-in role is the default runner now). The goldens: 28 changed (what `init` prints, the setup's page, Surco's catalog sample of the advice, the five frames corrected by the lead's rulings), 9 new and 3 gone among what `init` prints (the cases an input produces), 16 moved from `targets/` to `screens/` (`setup`, `setup-unreachable`, `setup-changing`, `setup-written`); digests written (`digests-a-setup-4.txt`, 859 goldens). No process of the worktree's tests left running. The walk as the group left it: 64,810 states, 114 palettes, 68 helps, 9,183 screens a size (its table: `walk-tables/after-setup-fixes.txt`). Still held everywhere but a row: `field-marked`, `field-close`, `field-note`, `setup-audit`, `setup-confirm`, which Task 140a lands (the owner's decision of 2026-11-22: the application folds what it draws under the ASCII glyphs; a printed page does not).

### Task 154: the machine and upkeep

**Description:**
- **`DoctorStage`** (the machine) and **`UpkeepStage`** take the design's frame and components as their targets design them: the trail; the tally and the attention row through Phase 3's one composer, `FollowedRuns::framing` (Task 92); the legend with `Screen.anywhere` beside it; the findings as Phase 3's checklist rows; and every confirmation in Task 127's box.
- The facts each shows today stand, held by `crates/ui/tests/doctor.rs` and `upkeep_schema.rs` as they are. Each is read when opened and again on its key, never on a timer (question 12).
- Their legends and trail words are `render::screen::machine`'s, in the design's form where the inventory gives one.
- **The targets `machine` and `upkeep` land**, moved (`git mv`) into `crates/tui/goldens/screens/`, each state from `yunta_testkit_ui::cut` by its name.

**Found in its turn:** The machine and upkeep draw in the frame: the trails `yunta › machine` and `yunta › machine › upkeep`, the tally and the attention row of the field the page was opened over (`FollowedRuns::framing`), `a` going where the row names (`stage/over_the_field.rs`, which the setup shares), each page through `Region::of_a_page`; `Doctor::on_screen` heads the page `this machine` and leaves the next steps to the palette, `Collected::on_screen` heads it `upkeep`. The kit's `Worked` place holds a config (its pool's budget `98.9G`, `1G` kept free) and git alone on its `PATH`, since a C compiler there made doctor add a figure of the machine. The lead's rulings on the rows held: the code right for the advice's row (`- { adapter: <adapter> }`) and the subject's column (the widest subject plus two); the target right for a checklist that stands close under its folded row (`Checklist::close_under_its_fold()`; what `yunta doctor` prints is unchanged) and for the least terminal, where a labelled value and a checklist's row are cut to their row and not wrapped, built as one rule of Surco (`Values::OnTheirRow`, a page drawn close), which the conventions say and the code did not hold; with it the landed `setup-audit` and `setup-confirm` at 60 columns are corrected again (their `runners` row cut, the count 496). Landed: `machine` at the four sizes. Not landed: `upkeep`, held everywhere but the row that says the disk's free space (the engine's `CheckoutPool::report` reads the disk itself: the reading is given to it by its caller in the group's pass of fixes, so the kit's place fixes it). What a screen says: the trail `machine` for `doctor`, the headlines `this machine` and `upkeep`, the legends `f every check · s a session per binding · esc back` and `c collect it · esc back`; `yunta doctor` and `yunta gc` print as before. `u` and `q` leave the machine's legend for the tasks of every key.

**Fails first:** `crates/tui/tests/screens.rs` with `machine` and `upkeep` added against their targets, red until the stages draw the frame.

**Acceptance criteria:**
- [x] Both equal their targets at the four sizes; `doctor.rs` and `upkeep_schema.rs` pass unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --test screens`, `cargo test -p yunta-ui --test doctor --test upkeep_schema`, `cargo test -p yunta --test application machine`

**Dependencies:** Group check: a setup. Task 154, Task 155 and Task 157 touch disjoint stages and can run at once, sharing only one entry each in `crates/tui/tests/screens.rs`; Task 156 follows Task 155.

**Files likely touched:** `crates/ui/src/stage/{doctor.rs,upkeep.rs}`, `crates/render/src/screen/machine.rs`, `crates/tui/tests/screens.rs`, `crates/tui/goldens/screens/`

**Estimated scope:** M

### Task 155: the workflows with their checks, a workflow's history and its graph

**Description:**
- **`CatalogStage`'s listing, checking and checked**, **`StatsStage::of_workflow`** and **`GraphStage::of_workflow`** take the design's frame and components as their targets design them.
  - The listing is Phase 3's list of choices, with each workflow's estimation.
  - The check's findings are checklist rows.
  - The history is drawn as a run's numbers are (Phase 3).
  - The graph is drawn as a run's graph is.
  - The tally and the attention row come through Phase 3's one composer, `FollowedRuns::framing` (Task 92); the legend has `Screen.anywhere` beside it.
- `enter` on a workflow opens the new run (Task 145).
- The workflows' own keys from the base stand, `n` among them (decided above).
- The facts stand, held by `crates/ui/tests/catalog.rs` and `stats_graph.rs`.
- **The targets `workflows`, `workflow-checked`, `workflow-history` and `workflow-graph` land**, moved (`git mv`) into `crates/tui/goldens/screens/`, each state from `yunta_testkit_ui::cut` by its name.

**Found in its turn:** The workflows stand as a list under `the workflows you can run`, most-run first (`Catalog::listed`, which a new run shares), with a panel of 44 cells at the right for the pointed one (its facts and `what you can do`; under 102 columns its content stands under the list) and the legend `j k move · enter new run · n new workflow · t cases · p packs · s shapes · esc back`; a workflow's check, its history and its graph draw as pages with their trails (`<name> check`, `<name> history`, `<name> graph`); `n` opens a new run from each and `a` attends on every catalog page but the packs; the history's and the graph's pages say where they are scrolled to, so the walk scrolls them. The lead's rulings on the rows held: the code right for the meter's row given to the regions, the bar's last eighth rounded, no bar where its track does not fit and the name's column of who spent it, and the graph's diagram written from the one layout; the least terminal's rule cutting the history's advice row. One language: `History::document` is the one page of a workflow's history, on the screen and at `yunta stats --workflow`, which prints `◌ fragua · 3 runs`, its sentence, `task cost` with its sparkline and `by mode`, where it printed lines of its own; one fact goes, the latest cost's exact figure (`2170000.0`, now `2.17M` as every token figure). Landed: `workflow-checked`, `workflows`, `workflow-history`, `workflow-graph` at the four sizes (90 in `LANDED`). The kit opens a cut on the workflows (`Opens::OnTheWorkflows`), as `yunta list` opens the application there: a repository set up with no run offers no key to its pages from the first request, which is the owner's to decide. The walk: 84,248 states, 114 palettes, 66 helps, 10,945 screens a size for 64,810, 68 and 9,183: by its table the pages over the field rise (each now shows the tally and the attention row, so each piece of field news tells a state apart), the history and the graph scroll, `a` on these pages reaches a run's screen, and two helps go with the legends that became one; the rulings moved no class.

**Fails first:** `crates/tui/tests/screens.rs` with `workflows`, `workflow-checked`, `workflow-history` and `workflow-graph` added against their targets, red until drawn.

**Acceptance criteria:**
- [x] Each equals its target at the four sizes; `catalog.rs` and `stats_graph.rs` pass with only the trail's and legend's words changed.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --test screens`, `cargo test -p yunta-ui --test catalog --test stats_graph`, `cargo test -p yunta --test application repository`

**Dependencies:** Group check: a setup

**Files likely touched:** `crates/ui/src/stage/{catalog.rs,catalog/page.rs,stats.rs,graph.rs}`, `crates/render/src/screen/catalog.rs`, `crates/tui/tests/screens.rs`, `crates/tui/goldens/screens/`

**Estimated scope:** M

### Task 156: cases, a new workflow, and the packs

**Description:**
- **The cases** (`catalog/cases.rs`), **a new workflow** (`catalog/new_workflow.rs`) and **the packs** (`catalog/packs.rs`, `packs/install.rs`: the listing, an audit, a removal, a new pack, an install, an update) take the design's frame and components as their targets design them.
  - A line typed is a `TypedAnswer`, on Phase 3's boxed row.
  - A shape is chosen in a list of choices.
  - Confirmations stand in Task 127's box.
  - The tally and the attention row come through Phase 3's one composer, `FollowedRuns::framing` (Task 92); the legend has `Screen.anywhere` beside it.
- The packs keep `a`, `n`, `x`, `i` and `u`.
- The facts stand, held by `crates/ui/tests/catalog.rs` and `packs.rs`.
- **The targets `cases`, `new-workflow`, `packs` and `pack-audited` land**, moved (`git mv`) into `crates/tui/goldens/screens/`, each state from `yunta_testkit_ui::cut` by its name.

**Found in its turn:** The cases stand under their tally once every case ran (`✓ cases · 1 case, 0 failed`, the rows in one aligned list; while they run, the rows with `running the cases` under them; `render::page::CasesOnScreen`); a new workflow is a typed screen (the trail `new workflow`, the legend `type the name · enter choose its shape · esc back`); the packs' listing and every page the packs show stand as pages (`Region::of_a_page`), the listing's legend `j k move · a audit it · u update it · x remove it · i install one · n new pack · esc back`, the audit's, a written pack's, an installed and an updated one's `esc back · n new run` with `n` opening a new run, a fetched one's `enter install it · esc back`. A pack's audit says `tests` among what the pack declares, failed cases after it, on the screen and at `yunta pack audit`, where it stood at the page's foot. Surco: a page that moves a pointer along its rows stands close at the least terminal; a line asked alone opens on a blank row, which gives way first. The kit builds the `Packed` place alone and reads a frame's `{commit}` as the commit the place made fresh. Red: with the four targets added the screens test failed (the cases' rows over their tally). Landed in its turn: `cases`, `packs`. Held, then ruled by the lead in the group's pass of fixes: `new-workflow` (the target right: its box is the design's box of a question, 71 cells at 120 columns, where the code spanned the region) and `pack-audited` (the code right: one component with the landed `setup-audit`).

**Fails first:** `crates/tui/tests/screens.rs` with `cases`, `new-workflow`, `packs` and `pack-audited` added against their targets, red until drawn.

**Acceptance criteria:**
- [x] Each equals its target at the four sizes; `packs.rs` and `catalog.rs` pass with only the words the inventory changes.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --test screens`, `cargo test -p yunta-ui --test catalog --test packs`, `cargo test -p yunta --test application repository`

**Dependencies:** Task 155 (both touch `catalog.rs`' screen)

**Files likely touched:** `crates/ui/src/stage/catalog/{cases.rs,new_workflow.rs,packs.rs,packs/install.rs}`, `crates/render/src/screen/catalog.rs`, `crates/tui/tests/screens.rs`, `crates/tui/goldens/screens/`

**Estimated scope:** M

### Task 157: a document's shape, a run's evidence, and the signpost

**Description:**
- **`SchemaStage`**, **`EvidenceStage`** (a receipt, a verification, a page for a browser) and **`signpost`** take the design's frame and components as their targets design them: the tally and the attention row through Phase 3's one composer, `FollowedRuns::framing` (Task 92), the legend with `Screen.anywhere` beside it.
- **The signpost's palette offers the machine's field.** `Stage::opened_by(&NextStep::ListRuns { reach: Machine })` opens the field at that reach, and outside a repository `followed()` names nothing until it is taken.
- The facts stand, held by `crates/ui/tests/upkeep_schema.rs` and `evidence.rs`.
- **The targets `document-shape`, `evidence-receipt`, `evidence-verified`, `evidence-page` and `signpost` land**, moved (`git mv`) into `crates/tui/goldens/screens/`, each state from `yunta_testkit_ui::cut` by its name.

**Found in its turn:** A document's shape stands under the trail `workflows › schema`: the question, the documents' list picked by typing (every printable key types into its filter; the legend `↑↓ move · enter its shape · esc back`), and a picked shape as a page (`esc back · n new run`). A run's evidence stands in the run's frame with its tally and attention row, the trails `Q78GRR receipt`, `Q78GRR verification`, `Q78GRR page`, `n` and `a` bound: `Receipt::on_screen` says its rows in the run screen's words with the detail the printed receipt keeps, its criteria each on a row (`Checklist::on_their_rows()`: a command cut at the line's end, not wrapped), no next step (the palette offers it); `Opened::on_screen` heads `✓ the run's page for a browser · written`. The signpost stands under `yunta` alone with no tally (`Signpost::on_screen`), its legend `e every run on this machine · q quit`: `e`, and the palette's first row, open the field at the machine's reach, and the application follows nothing until then. Red: the signpost's first row was `yunta list --runs`' own list; the screens test failed twenty times. Landed in its turn: `evidence-page`, `signpost` (94 in `LANDED`). Held, then ruled by the lead in the group's pass of fixes: the code right for `document-shape`'s filter a cell in, `evidence-receipt`'s wrap, blank row, cut and meter's row, and `evidence-verified`'s column. What commands print: nothing but `yunta pack audit`'s order (Task 156). The walk: 88,223 states, 115 palettes, 60 helps, 12,468 screens a size for 84,248, 114, 66 and 10,945; by its table the evidence's pages rise (the tally, the attention row, `n` and `a`), the shapes' list rises (every key types), the signpost's `e` reaches the machine's field and what stands over it, six helps go with legends become one; the first request, the field and the catalog's pages hold fewer states (4, 25 and 128), which the agent stopped on and the group's pass of fixes proves state by state against the tree before. Left to the tasks of every key: `q` still leaves the application from these pages; the packs' other pages name no key for `a`; the running cases keep `esc stops them · q leaves yunta`; the new workflow's shape list has no legend; the signpost's palette offers rows that cannot work outside a repository; whether a page's legend names its scroll keys.
 The group's one pass of fixes (the lead's rulings and what was gathered): every held target lands, 100 in `LANDED` and none left under `targets/`: `new-workflow` (a line asked on a page stands in a measure of 71 cells from the page's left edge, `QUESTION_CELLS`, named once beside the panel's 44: 71 cells at 120 and 80 columns, the region's 58 at 60), `pack-audited`, `document-shape`, `evidence-receipt` and `evidence-verified` (frames corrected from the code, 58 rows, listed in `phase-4/frames-corrected-pages-fixes.txt`), and `upkeep` (the disk is a seam like the clock: `yunta_core::disk::{Disk, SystemDisk}`, which the pool's report takes from its caller and the kit fixes at 74.2 GB free; the room checks keep their own reading; the engine's suite whole passes, 1,498). Under the ASCII glyphs the window's title is folded as the screen is, and a caret stands under its fault after the fold while `path:line:col` keeps the recorded numbers. The walk's falls are proven state by state against the tree before these tasks (`phase-4/walk-falls-156-157.txt`): 457 states went by route (a screen that does not name its place is now met first from the signpost's `e`, and the same structure is walked from there: 428 gained) and 144 by a page grown shorter (the new pack's written page lost its faint line, so its second scroll position is gone); none went without a cause. The walk: 88,223 states, 115 palettes, 60 helps, 12,468 screens a size; the pass moved no class.

**Fails first:**
- `crates/ui/tests/state.rs`: outside a repository the application follows nothing, and the palette's machine row then follows the machine's field.
- `crates/tui/tests/screens.rs` with `document-shape`, `evidence-receipt`, `evidence-verified`, `evidence-page` and `signpost` against their targets.

Both are red before the change.

**Acceptance criteria:**
- [x] Each equals its target at the four sizes.
- [x] The inventory's rows for every page the design draws no frame for differ in none.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-tui --test screens`, `cargo test -p yunta-ui --test state --test evidence --test upkeep_schema`, `cargo test -p yunta --test application -- opening runs_left`
- [x] `python3 .plans/yunta-tui/tools/targets_check.py crates/tui/goldens/screens`

**Dependencies:** Group check: a setup

**Files likely touched:** `crates/ui/src/stage/{schema.rs,evidence.rs,signpost.rs}`, `crates/ui/src/stage.rs`, `crates/render/src/screen/{machine.rs,run.rs}`, `crates/ui/tests/state.rs`, `crates/tui/tests/screens.rs`, `crates/tui/goldens/screens/`

**Estimated scope:** M Three stages take the frame alike; every caller of what they shared is named by the compiler.

### Group check: every page
- [x] The suite's groups green. Every page of the application stands in a frame with a target. `crates/tui/goldens/screens/` holds only targets, and `room-needed.unicode-59.txt`, the one base frame kept, since it stands only below 60×14; `crates/tui/goldens/targets/` is empty. `python3 .plans/yunta-tui/tools/targets_check.py crates/tui/goldens/screens` passes. `goldens.py` names each changed golden by its task.

**Found in its turn:** the whole gate (`.plans/yunta-tui/proof/live-screens/check-every-page-4/`): every step passes but two tests of 331 binaries and 5,616, both of `yunta-ui` and both an expectation the pages' tasks left behind, found here as the owner's sharing of the checks means them to be: each expected the trail's place of a run's page without the run's handle (`receipt` for `Q78GRR receipt`, the handle alone for `PR0M0T page`); they read the place as it is and their targets pass (`after-the-gate.txt`). The release binary weighs 31,823,712 bytes (66,144 more than the last group's). Against the setup's captures 4 of 159 views differ, each by this group: `yunta stats --workflow` prints the page the screen draws (three views: `◌ agent-demo · 2 runs`, its furrow and sentence, where it printed `workflow \`agent-demo\`: 2 runs`), and bare `yunta` outside a repository draws the signpost's screen. `yunta doctor`, `yunta gc`, `yunta list`, `yunta check`, `yunta graph`, `yunta test`, `yunta new`, `yunta schema`, `yunta receipt`, `yunta verify` print as before. The goldens: none changed in place; 80 moved from `targets/` to `screens/` (the twenty screens landed in this group, the frames corrected by the lead's rulings among them); digests written (`digests-every-page-4.txt`, 859 goldens). **Every target is landed: 100 screens held cell for cell at the four sizes, none left under `targets/`.** No process of the worktree's tests left running. The walk as the group left it: 88,223 states, 115 palettes, 60 helps, 12,468 screens a size (its table: `walk-tables/after-pages-fixes.txt`).

### Task 158: `n`, `a` and `q` on every stage

**Description:**
- **Everywhere nothing is typed and no prompt stands:**
  - `n` opens a new run (`NextStep::Run`), except where the stage binds `n` from the base (the workflows, the packs);
  - `a` goes where the attention row names, except where the stage binds `a` (the packs' audit), where the attention row names no key;
  - `q` goes to the field from every page.
- The setup and the signpost leave with `q`; the field leaves; a line typed takes `q` and `n` as letters.
- The spec's "everywhere" row closes here.

**Found in its turn:** Wherever nothing is typed and no prompt or box stands: `q` on a page goes down, `n` opens a new run, `a` goes where the attention row names; the field, the setup and the signpost leave with `q`, ctrl-c leaves everywhere. The lead's rulings: `q` goes down and never builds (`Want::ToTheField` goes to the nearest field under the stage, else back to the stage at the bottom, which comes back into view; nothing drops the stage at the bottom: the setup's `a` opens the run over the setup, and the palette's runs row goes down by the same rule); `n` is offered only where a run can be asked for (`FollowedRuns::can_ask_for_a_run()`, the one owner: a repository set up; elsewhere no legend, help, palette row nor the field's first row names it, the workflows' `enter new run` and their panel's row among them); a page that comes back into view reads again what it let go when a stage opened over it (`State::opened_over` keeps those readings with the stage under, `back_in_view` asks them again), a defect older than the phase that left upkeep's dry run, a workflow's check, the cases, the packs' readings, the evidence being written, a history, a graph and the setup's detection reading for ever; and a defect it exposed, the setup taking any refusal as its own, is fixed. The new workflow's shape list is a typed screen; the running cases' legend is `esc stops them`. Red: the walk's check failed on `q` (signpost, `e`, `w`, `q`); with the field's legend broken, on a new run named where none can be asked for; upkeep come back to did not read. The walk's rule that a screen saying it reads waits on that reading (render's one predicate, `screen::says_a_reading`) found no state before the fix, since the walk keeps a state a screen and never comes back to a page left mid-reading: the state's test holds that case. The agent stopped once, rightly, on the field's own total falling by 308 with nothing removed: a field built at the bottom in the signpost's place, a route the first ruling removes; after it the field holds more states than before the task (7,210 for 5,995). The walk after the task: 81,462 states, 129 palettes, 68 helps, 13,681 screens a size, by its table: the new run's stage 12,717 fewer (no new run outside a repository nor over a setup), the catalog's pages 2,038 fewer (the shape list typed), the pages, the field and a run's screen more (outside a repository they no longer name a new run, so they no longer read as the same screens inside one), the setup 900 more (`a` and `q` come back to it). No landed frame moved.

**Fails first:** `crates/ui/tests/walk.rs` gains two checks on every input handed:
- `n` on a state not typing, with no prompt standing and no stage key for it, opens the new run;
- `q` from any page but the field, the setup and the signpost asks the field.

Red at the base and through Task 157, where `q` leaves from the workflows, doctor, upkeep and the pages.

**Acceptance criteria:**
- [x] The walk holds both over every state it reaches.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test walk --test state`

**Dependencies:** Group check: every page

**Files likely touched:** `crates/ui/src/{state.rs,stage.rs}`, `crates/ui/src/stage/{catalog.rs,doctor.rs,upkeep.rs,evidence.rs,stats.rs,graph.rs,schema.rs}`, `crates/ui/tests/walk.rs`

**Estimated scope:** M. Each stage's `pressed` changes alike; the walk names a stage that does not. It touches every caller of the three keys: each stage that presses them.

### Task 159: help and the palette say every key, and the walk's required lists

**Description:**
- **Help lists each stage's keys:** the legend's and those out of it (the field's, which Task 140 says; a run's `e`, `v`, `o`, `c`; the machine's `u`; the new run's and the setup's), in Phase 3's box. Each stage says the keys it takes beyond its legend, as Phase 3 made a run's screen and Task 140 the field say them.
- **The palette's rows** are every act the stage offers.
- **`render::screen::over_every_stage` goes** (`crates/render/src/screen.rs:94`), with its call in `crates/ui/src/state.rs`: every stage composes `Screen.anywhere` before this task (Phase 3's stages and this phase's, Task 137 to Task 157), and help says the far keys from it. Every caller goes with it; the compiler names them.
- **The walk's required lists** become what these screens say:
  - `MUST_REACH` (the field's groups and glance, its confirmation, the first request, a new run's rows and panel, the setup's rows, audit and written page, every page's);
  - `MUST_OFFER` (`yunta run` from the field, `yunta pack add` from the setup's confirmation);
  - `MUST_SCROLL`.
- **`crates/tui/tests/walk.rs`** lays out every new screen at both sizes, the side region at the right among its rules.

**Found in its turn:** Help lists each stage's keys through render's `StageKeys`: its legend's, those beyond it and those every stage answers to (`q` reads `go to the field` or `go back to where yunta opened`; the machine's `u`, the workflows' `c g h`, the setup's audit's `q`, the signpost's `esc`); `over_every_stage` is gone with every caller. The palette's rows are every act the stage offers: every page offers `a` and `n` by their keys, the setup's audit offers `yunta pack add`, and `Stage::always(followed)` offers only what works where the application stands (the signpost's palette holds `e`, `yunta doctor` and `yunta schema`). One rule for a page's scroll keys, owned by `PageRead` and `screen::page_legend`: a page whose content runs past the design's own 120x36 screen names `↑↓ scroll` at every size (the chronicle, a graph, the setup's audit), any other names none and help says the keys; the legend that changed once a page was laid out is gone; `State::pane` reports every page, so the walk scrolls them, the setup's audit among them. The new workflow's shape list has its legend (`↑↓ move · enter write it · esc take the first`); the field's `d` and `e` name `--all` and `--full`. Red: the walk's `help_taken`, extended to every key a state takes, failed first on the signpost's `esc` and listed the machine's `u` and `q`, `a`, `n` on every page. The walk's lists: `MUST_REACH` 96, `MUST_OFFER` 13 (`yunta run` from the field, `yunta pack add` from the setup's audit), `MUST_SCROLL` with the setup's audit. The walk on the task's final tree: 81,686 states, 278 palettes, 259 helps, 12,667 screens a size, 74 s (its test 113 s), set against Phase 3's in `phase-4/walk.txt`; no bound rose. Sentences no target gives: `go to the field`, `go back to where yunta opened`, `reading the run to draw its graph`, `esc back` on a workflow being checked and a pack being audited or fetched, `esc stops them`, `enter back` in help. Gathered for the group's pass of fixes: the walk gains a refused start (984 states measured); outside a repository the field's `e` still offered this repository's runs; `d` at the machine's reach names no command.

**Fails first:** `crates/ui/tests/walk/dialogs.rs`' `help_taken` checks that every key whose press changes a state is said by that state's legend or help. Red on a stage whose help is still its legend and leaves out a key it takes beyond it, such as the machine's `u`.

**Acceptance criteria:**
- [x] Both walks pass, with their count of states, palettes and helps, and their time, set against Phase 3's in `.plans/yunta-tui/proof/live-screens/phase-4/walk.txt`. No bound rises; a bound that would rise stops for the user.
- [x] No `over_every_stage` is left in `crates/` (`grep`).

**Verification:**
- [x] Tests pass: `cargo test -p yunta-ui --test walk --test palette --test dialogs`, `cargo test -p yunta-tui --test walk`, `cargo test -p yunta-render screen`

**Dependencies:** Task 158

**Files likely touched:** `crates/ui/src/{stage.rs,state.rs,dialog/help.rs}`, `crates/render/src/screen.rs`, `crates/ui/tests/{walk.rs,walk/dialogs.rs}`, `crates/tui/tests/walk.rs`

**Estimated scope:** M. `over_every_stage` goes with every caller, which the compiler names.

### Group check: every key
- [x] The suite's groups green. The spec's "Keys" table, row by row, against `crates/ui/tests` and the walk's checks: each key held by a test, in `phase-4/keys.txt`.

**Found in its turn:** a light check (`.plans/yunta-tui/proof/live-screens/check-every-key-4/`), after the group's one pass of fixes (the walk reaches a refused start, said under its row; outside a repository the field's `e` offers nothing; `d` and `e` at the machine's reach name the commands that list the same, `yunta list --runs --all --full` among them, which the CLI takes: `Reach::MachineWithClosed`). The short steps pass; the six touched packages' tests hold 159 binaries and 2,594 tests, none failing. The release binary weighs 31,840,240 bytes (16,528 more than the last group's). Against the pages' captures no view of 159 differs and no golden changed: the keys change what a key does and what help and the palette say, which no capture opens and the walk and the stages' tests hold. No process of the worktree's tests left running. The walk as the group left it: 73,202 states, 271 palettes, 255 helps, 12,257 screens a size, 70 s (its table: `walk-tables/after-keys-fixes.txt`; set against Phase 3's close in `phase-4/walk.txt`); `MUST_REACH` holds 97, `MUST_OFFER` 13. Gathered for the docs' pass of fixes: `yunta list --runs --all` names this repository's `--full` as its next step, where `--all --full` now exists.

### Task 160: the docs

**Description:**
- **`README.md`:**
  - the application's paragraph (`:143-146`): the field, a new run, a setup that installs fragua;
  - `list --runs`' row (`:182`): the mode alone, the age at the edge;
  - `pack add`'s row: the directory form.
- **`docs/guide.md`:**
  - "The application at a terminal" (`:683-705`): the screens and every key as the spec's table says, the keys out of the legend in help and the palette, and `q` going to the field;
  - the paragraphs on deciding (`:800-820`) where keys changed.
- **`docs/packs.md`:** the directory form (Task 147).
- **`.agents/architecture.md`:** the follower's rule Phase 1 wrote (Task 21) is read against the application as it stands: the application and `status --follow` follow through the one `Following`, held by `two_followers_change_nothing`. It is rewritten only where it is false, within the cap (6,144 bytes); a cap that would rise stops for the user.
- **`CAPABILITY-MAP.md`:** the `live-screens` row says what the module delivered and what it added to Surco, render and `yunta-ui`.
- **Surco's reference**, whose sources stand outside git at `~/.claude/plans/surco-v3/tools/reference/` (`capture.py` takes real captures, `build.py` writes the page; the paths at the top of each script pointed at this worktree first), republished to its artifact (`https://claude.ai/artifact/XgLAT2CUJBeT41waGWzSjU`). The sections of the components this phase changed or added are edited in place, not appended: the typed answer's placeholder and hint, the row of words, the wordmark, `SameAtAShell`, `AtAGlance`, the run's row.

**Found in its turn:** The docs say the application, a runner's configuration, the packs and the ASCII glyphs as the tree has them. `docs/guide.md`'s "The application at a terminal" is one text again: where bare `yunta` opens, the frame every screen shares, the field with its keys, `n` only in a repository that is set up, the first request, a new run, a setup with its pack's audit, confirmation and written page, the pages (`esc` back, `q` down to the field or to where yunta opened). `docs/compatibility.md` gains the glyphs: what picks the set, that the application folds every text under ASCII and what a character becomes, that a printed page, Markdown and HTML are never folded. `docs/adapters.md`: the order of a runner's candidates is not a fallback (it serves a run pinned to an adapter and a binary that builds none of the earlier ones), `model:` may be left out, what `init` declares and the default runner. `docs/packs.md`: the source form `<repository>//<directory>[@ref]`, the pack a setup installs and from where, the audit's `tests` row. `README.md`: the application's paragraph, `list --runs`' row, `stats --workflow`'s page. `CAPABILITY-MAP.md`: the module's row. What a doc said that was false, corrected: outside a repository the application "says what to run instead" (it offers `e`); on a page `q` "leaves" (it goes down) and `esc` does something on the field (it does nothing); `n` starts a run wherever no prompt stands (it needs a repository set up); a run's row carries "the command that moves it" (the age stands at its edge, the commands under the groups); a runner "falls back" to its next candidate (a run does not); `model` shown as required. `.agents/architecture.md` (6,140 of 6,144 bytes) and `.agents/open.md` (8,182 of 8,192) are unchanged: no line of them is false of the tree, the follower's rule among them. Surco's reference is rebuilt from real captures of this tree and republished (version 12): `Gist`, `Fields`, `Runs`, `Checklist` and `TypedAnswer` edited in place; `AtAGlance`, `Wordmark`, `SameAtAShell`, `WordChoice`, `Form`, `Survey` new; the application's screens, a region's measure, a page in a region and a screen in ASCII added (`checkpoint-d/surco-reference-changes.md`). `docs_sync` 9 and `doc_claims` 5 pass. For the owner: a product rule proposed for `.agents/architecture.md` (`A screen offers only what works where it stands…`, 132 bytes, held by the walk) does not fit under the file's cap with 4 bytes free. Gathered for the group's pass of fixes: `yunta list --runs --all`'s next step, and `yunta doctor --session`'s label `(planner fallback)`, which promises what a run does not do.

**Fails first:** none: the docs. `docs_sync.rs` holds the README's command table and every output the docs quote, and `doc_claims.rs` its claims.

**Acceptance criteria:**
- [x] `cargo xtask docs --check` passes with every cap unchanged; `wc -c .agents/architecture.md` is at most 6,144, and the follower's rule is unchanged unless a line of it was false, that line named in the commit.
- [x] The republished reference shows each component above as a real capture of this tree.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test docs_sync --test doc_claims`; `cargo xtask docs --check`; `cargo xtask smells --check`

**Dependencies:** Group check: every key

**Files likely touched:** `README.md`, `docs/guide.md`, `docs/packs.md`, `.agents/architecture.md` (only where the follower's rule is false), `CAPABILITY-MAP.md`, `~/.claude/plans/surco-v3/tools/reference/` (outside git)

**Estimated scope:** S

### Group check: the docs
- [x] `docs_sync`, `doc_claims`, `docs --check` and `smells --check` pass; only docs changed since the group check before.

### Task 161: every golden, every phrase, every command, against the base

**Description:**
- **Every golden equal to its target.** `crates/tui/goldens/screens/` equals the approved targets byte for byte, and `tui`'s fifteen frames are gone but `room-needed`, the one base frame kept, since it stands only below 60×14.
- **No row of the inventory differs.** `python3 .plans/yunta-tui/tools/targets_check.py crates/tui/goldens/screens` passes for every family: zero rows differ on any screen.
- **The captures of every command** (`pages.sh`, `normalize.py`) set against the base: each difference one the inventory names.
- **The suite by name** against the base (`suite_diff.py`, `moved_tests.py`): moved, renamed, new, and none gone without its reason.
- **Every golden's digest** (`goldens.py`): each change named by the task that made it.
- **Every pty test this module added** run under `stress_binary.py`.
- All of it goes in `.plans/yunta-tui/proof/live-screens/after.txt`.

**Found in its turn:** All of it is in `.plans/yunta-tui/proof/live-screens/after.txt`, measured on the final tree after the review's fixes. The screens: 102 screens in four environments (100 names in `LANDED`), `targets_check.py` `all hold`, `targets_page.py` 0 findings, `inventory_cover.py --all` clean; `crates/tui/goldens/targets/` is empty and of the base's frames only `room-needed` stays. A landed frame equals its approved target but in 311 rows the lead corrected by ruling, each found cell by cell against the targets as committed and listed for the person under its reason, none unexplained. The captures: 159 views; 36 differ from the last group check, each one a named change of the review's fixes (the receipt's version, the close's sentence, a step's words from their one owner, `init`'s runners row, the open reach's words); 96 differ from the base, none only on one side. The suite by name: 4,863 at the base, 5,676 now; 9 moved, 897 new, 84 gone, each with where it lived and what holds its behaviour now (`after-gone-tests.txt`: 16 renamed, 56 rewritten, 12 removed with what they tested, in Phase 2's and Phase 3's commits; one of those was the estimate of a followed run's time in the base's footer, told to the person). The goldens: 861; 116 changed and 2 new since the last group check, each by a named fix; 180 changed, 15 gone and 629 new against the base. The pty tests: the CLI's `application` binary (44 tests) run whole 50 times from 5 workers beside 8 busy loops, 0 failures.

**Fails first:** none: a measurement.

**Acceptance criteria:**
- [x] Every difference from the base is one this list or an earlier phase's names.

**Verification:**
- [x] Manual check: `after.txt` read whole.

**Dependencies:** Group check: the docs

**Files likely touched:** `.plans/yunta-tui/proof/live-screens/` (outside git)

**Estimated scope:** S

### Task 162: three real runs followed and answered in the application

**Description:**
- Three fragua runs from the final tree's release binary, in the sandbox, on the CLI and model of the base's runs. Each is opened from bare `yunta` on the field and followed in place. Every gate is answered on its card or at its desk where it appears, by the driver (`follow_and_answer.py`, which Phase 1 copies from `proof/tui/`, pointed at decisions in place), and each run is stopped before its `pr` node.
- Each run's time between nodes and tokens, from `yunta stats <run>`, set beside the base's three and Phase 2's.

**Found in its turn:** Three fragua runs of the final tree's release binary, each opened from the field of bare `yunta` with `enter`, followed in place, and its gate `ship` answered `abort` on the card that stood by itself on the run's screen (`j`, `enter`); each log holds `gate_resolved ship by unverified:sandbox`, and each run was closed. Time between nodes: 0.561, 0.571 and 0.542 s (working 144.7, 143.8, 139.7 s; 0.99M, 1.20M, 1.13M tokens). Two stand over the base's three of 2026-11-15 (0.536 to 0.555 s) by 6 and 16 ms, so the base's binary and the final one were run again the same hour, alternating, with no application beside: base 0.572 and 0.576 s, final 0.562 and 0.567 s. The final tree's time between nodes is the base's or under it measured the same hour; the machine moved since the base was measured. The application followed each run 1.0 s after it opened and spent 0.71 to 0.77 s of processor over the run. The driver (`follow_and_answer.py`) and the at-rest script waited for words the base's field said: both now read this phase's field, card and keys (the first three starts ended before any gate, each run cancelled and closed). A role with no model on each adapter (`real_run_no_model.py`): on Claude Code `runner_resolved.chosen` holds the adapter alone and the sessions reported the model this machine's Claude Code picks; on Codex `chosen` holds the adapter alone, Codex took the model its own configuration names, the account refused it, and the run waited with `retry` and `abort` saying so in Codex's words: told to the person. The watch of the CLIs' arguments was lost with that script's early end; each adapter adds `--model` only where the request names one. Numbers and records: `after.txt`, `real-final-*.txt`, `real-control-*.txt`, `real-final-no-model.txt`.

**Fails first:** none: a measurement.

**Acceptance criteria:**
- [x] `between_secs` inside the spread of the base's three (criterion 14); every gate answered in the application is on its run's log, by the pty's user.

**Verification:**
- [x] Manual check: the runs' numbers in `after.txt`

**Dependencies:** Task 161

**Files likely touched:** `.plans/yunta-tui/proof/live-screens/` (outside git)

**Estimated scope:** S

### Task 163: what the module weighs

**Description:**
- From the final tree's release build, all in `after.txt`:
  - a drawing of a run with a feed of ten thousand rows at 200×60, the median of two thousand (the probe's `feed`, which Task 123 builds);
  - the application's processor time, open a minute on the machine's field with nothing moving (`at_rest.py`, Phase 1's, given `e`, the key that reaches the machine's field since Task 140, where the base took `a`);
  - the release binary's size by crate against the base's (`tools/size_by_crate.py`).
- Criterion 13's "one pragma a look" is held by the follower's own tests on the final tree: a look at a store where nothing moved costs one statement (`crates/api/src/follow/tests.rs`, Task 13), and so does a look over a field of three runs held (`crates/api/tests/following/field.rs`, Task 17).

**Found in its turn:** From the final tree's release build, all in `after.txt`. A drawing of a run whose node made 10,000 calls, at 200x60: median 0.312 and 0.318 ms whole, 0.272 and 0.280 ms where nothing changes (Phase 3: 0.349 and 0.304); 0.316 ms at 1,000 calls and 0.320 at 100, so a drawing no longer costs more as the feed grows. At rest for a minute, median of three: 0.346% of one core on the machine's field (key `e`) and 0.345% on the screen of a run parked on its gate (base 0.100% and 0.155%; Checkpoint B 0.403% and 0.496%). The release binary: 31,906,400 bytes (30.4 MiB), +4.27% over the base's; with symbols, text +1,278,924 bytes against the base (`core` +202k, `yunta_ui` +201k, Surco +170k, `serde_path_to_error` +141k, `yunta_api` +126k, `yunta_core` +126k, `yunta_render` +120k) and +356,652 against Checkpoint C's; no new dependency. One statement a look: the follower's two tests pass in the gate's suite. What following costs, for the review's two carried findings: a real run of about 125 events followed for 150 s costs 0.5% of one core, and a run of 10,008 events is read, folded and pointed at in 1.8 to 1.9 s.

**Fails first:** none: measurements.

**Acceptance criteria:**
- [x] Criterion 13: a drawing under 2 ms; under 2% of one core; Task 13's and Task 17's tests of one statement a look pass. Criterion 14: the binary under 40 MiB.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-api follow::`, `cargo test -p yunta-api --test following`
- [x] Manual check: the numbers in `after.txt`

**Dependencies:** Task 162

**Files likely touched:** `.plans/yunta-tui/proof/live-screens/` (outside git)

**Estimated scope:** XS

### Task 164: the review

**Description:**
- The module's diff from `4315c1da`, read on four lenses by readers of their own, given the code and not this list's conclusions:
  - Surco's new and redrawn pieces, and its catalog;
  - render's phrases and pages, one owner each, against the inventory;
  - `yunta-ui`: the follower's data in the state, the scenes and the lead, decisions in place, the field, a new run, a setup, every key, and the walk;
  - the API (the follower, the fold, the ledgers, the packs' directory, the setup's pack), the binary, and every test the module added or changed.
- Each finding is checked against the code and the base and recorded here with its decision. Each fix is red first. A finding that changes a target stops for the user.

**Found in its turn:** Four readers, one lens each, read the module's diff from `4315c1da` and ran nothing; sixty-two findings, each checked in the code before it was fixed and recorded with its decision and what was built in `.plans/yunta-tui/proof/live-screens/phase-4/review-findings.md` (the readers' reports beside it). By lens, each finding with its decision: render: R1 fix, listed; R2 fix; R3 fix, listed; R4 fix, listed; R5 fix, listed; R6 fix, listed; R7 fix; R8 fix; R9 fix; R10 fix; R11 fix, listed; R12 fix; R13 fix; R14 fix; R15 fix, listed; R16 fix; R17 fix; R18 fix; R19 fix. the application: A1 fix; A2 fix; A3 fix; A4 fix; A5 fix; A6 fix; A7 fix; A8 fix; A9 decline; A10 fix; A11 fix; A12 fix. under the application: U1 fix; U2 fix; U3 fix; U4 fix; U5 fix; U6 fix; U7 carry, measured; U8 carry, measured; U9 fix; U10 fix; U11 fix; U12 fix; U13 fix; U14 fix; U15 fix. Surco: S1 fix; S2 fix; S3 fix; S4 fix; S5 fix; S6 fix; S7 fix; S8 fix; S9 fix; S10 fix; S11 fix; S12 fix; S13 fix; S14 fix; S15 fix; S16 fix. The fixes were built in four lanes, one after another in the one tree (under the application, the application, Surco, render), each fix red first where it has behavior; a finding that changed an approved frame's words was ruled by the lead and each row is listed before and after for the person (`checkpoint-d/to-show.md`): the close's sentence (eight frames) and the open reach's words (twenty-three). Left for the person, not built: the mood of what a command does, the walk near its bound, a gate whose shown artifact is not held treated as asking nothing by `resolve-gate`, a release build's abort on a panic, the follower's copies of a state, the counters only tests read. After the fixes the gate on the final tree passes whole (`proof/live-screens/check-final/`): every step of the `ci` job, 331 test binaries, 5,679 tests, none failing, none aborted; the walk holds 114,469 states of 130,000 with no class moved by the last two lanes.

**Fails first:** none: a review; each fix red first.

**Acceptance criteria:**
- [x] Every finding recorded here with its decision; the suite's groups green after the fixes, none aborted (an exit 101 with no failure read for it).

**Verification:**
- [x] Tests pass: the suite's groups; fmt, clippy, `smells --check`, `docs --check`

**Dependencies:** Task 163

**Files likely touched:** as the findings say

**Estimated scope:** M

### Checkpoint D: the field, a new run, a setup, and the proof
- [x] The user reviews the spec's fifteen criteria, each with its evidence in `after.txt` (1, 4 from Phase 2; 2, 3, 5, 7, 8, 11 from Phase 3; 6 at every screen; 9, 10 here: Task 139, Task 141, Task 146, Task 153; 12 as it held at Checkpoint A, and since then `yunta status --follow` and the receipt differing from the base only by the design's phrases and components, each difference a row of the inventory approved at Checkpoints B and C; 13, 14 from Task 162 and Task 163; 15 from Task 160 and CI). The user tries the field, a new run and a setup on their own machine, and reads what the CLI's pages and prompts now print (the captures' differences).
- [x] Equal to the base: every difference in the captures, the suite and the goldens is one a task named. Measured: time between nodes, the drawing, the processor at rest, the size by crate.
- [x] The phase commits once every step of the `ci` job passes (`sh .plans/yunta-tui/tools/gate.sh`, `CARGO_INCREMENTAL=0`, one build at a time). Paths are staged by name: `crates/{surco,render,ui,tui,api,testkit,testkit-ui,cli}/…` as this phase changed them, `crates/tui/goldens/{targets,screens}/` (the targets moved), the blessed goldens, `README.md`, `docs/guide.md`, `docs/packs.md`, `.agents/architecture.md` where its rule changed, `CAPABILITY-MAP.md`. Then `cargo clean`.

### Task 165: the pull request

**Description:**
- The draft pull request's body says the module whole. It is pushed, and CI runs on Linux and macOS. The user decides when it leaves draft.

**Fails first:** none.

**Acceptance criteria:**
- [ ] CI green on Linux and macOS.

**Verification:**
- [ ] Manual check: `gh pr checks <the pull request>`, every job passing.

**Dependencies:** Checkpoint D

**Files likely touched:** the pull request's body

**Estimated scope:** XS
