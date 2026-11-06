# Tasks: `log-follow`

Spec: `SPEC-log-follow.md`. Plan: `tasks/plan.md`. Status: **approved on 2026-11-05, in implementation**.

**How these run:** one at a time, with `incremental-implementation` and `test-driven-development`. Each task names the test that fails first and why; it is seen failing for that cause before the code that makes it pass. A test that may pass at once says so and says what it guards. Every task leaves the tree compiling and its tests green. `cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`. Before editing a file, `cargo xtask docs --about <files>` runs and what it prints is kept true. No commit until the user approves at Checkpoint E.

## Phase 1: the store

### Task 1: `events_after` on both stores, one row reader

**Description:** The store reads a run from a position. One function turns a row into a `StoredEvent`, and `events_for_run` and `events_after` both use it, so an unknown kind and a corrupt row are handled the same way by both. `store.rs` is at 474 lines, so the reads move to a file of their own.

**Fails first:** `events_after_returns_what_events_for_run_returns_past_the_seq`, in `crates/storage/tests/store/events.rs`, because `events_after` does not exist.

**Acceptance criteria:**
- [x] For every seq of a run of several events, `events_after(run, seq)` equals `events_for_run(run)` with the events at or below `seq` taken out, in order; at the run's last seq, and for a run with no events, it is empty.
- [x] An unknown kind past the seq comes back verbatim, and a corrupt payload past the seq fails the read as `events_for_run` fails it, while one at or below the seq is not read.
- [x] `AsyncStorage::events_after` answers what `Storage::events_after` answers.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-storage`
- [x] `cargo xtask smells --check` passes.

**Dependencies:** None

**Files likely touched:**
- `crates/storage/src/store.rs`, `crates/storage/src/store/reading.rs` (new, where the reads move)
- `crates/storage/src/async_storage.rs`
- `crates/storage/tests/store/events.rs`

**Estimated scope:** M

### Task 2: each run's head, and what its fallback costs

**Description:** One read names every run with an event and its last seq. The first reading, `run_heads` and the events of a run with no head row, which an older binary wrote, took 28 to 39 ms on 1,000 runs of 200 events, since that fallback scans every event. At Checkpoint A the user chose a walk of the events' key, one run to the next, which reads the same heads with no fallback and no cache.

**Fails first:** `each_run_s_head_is_its_last_seq_with_or_without_a_head_row`, in `crates/storage/tests/store/heads.rs`, because the read does not exist. The test deletes one run's `run_heads` row through its own connection, as `chain.rs` does, to stand for a run an older binary wrote.

**Acceptance criteria:**
- [x] Every run with an event appears once, with the seq of its last event, whether or not it has a head row; a purged run does not appear.
- [x] The async store answers the same.
- [x] `heads_cost.py`, on a synthetic store of 1,000 runs of 200 events each, gives the same heads from every reading and times the store's read under 10 ms, with every head row there and with half of them gone. The numbers go to `heads-cost.txt`.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-storage`
- [x] Manual check: `python3 .plans/yunta-tui/proof/log-follow/heads_cost.py`, running the SQL the store runs.

**Dependencies:** Task 1

**Files likely touched:**
- `crates/storage/src/store/reading.rs`, `crates/storage/src/async_storage.rs`, `crates/storage/src/lib.rs` (`RunHead`)
- `crates/storage/tests/store.rs`, `crates/storage/tests/store/heads.rs` (new)

**Estimated scope:** M

### Checkpoint A: after Tasks 1-2
- [x] `cargo test -p yunta-storage` and `cargo xtask smells --check` pass.
- [x] The user sees the fallback's cost and whether it is cached.

## Phase 2: following a run that moves

### Task 3: `--follow`, its conflicts, and a run that is not moving

**Description:** `yunta status` takes `--follow`. On a run whose phase is neither created nor running, it prints the page `yunta status` prints and exits with the code `render::exit` gives its standing. The parser refuses it beside `--json`, `--full`, `--dig` and `--node`.

**Fails first:** `following_a_run_that_is_not_moving_prints_its_page_and_exits_with_its_code`, in `crates/cli/tests/status_follow.rs`, because the parser refuses `--follow` with exit 2.

**Acceptance criteria:**
- [x] On a run parked on a gate, `status <run> --follow` prints what `status <run>` prints and exits 3; on a finished run, it exits 0.
- [x] Each of `--json`, `--full`, `--dig` and `--node` beside `--follow` exits 2, and `status` without `--follow` still exits 0 on the parked run.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test status_follow`

**Dependencies:** None

**Files likely touched:**
- `crates/cli/src/cli/mod.rs` (490 lines: the flag only), `crates/cli/src/cli/dispatch.rs`
- `crates/cli/src/commands/status/mod.rs`, `crates/cli/src/commands/status/follow.rs` (new)
- `crates/cli/tests/status_follow.rs` (new)

**Estimated scope:** M

### Task 4: a surface fed from the log, and the engine read from the registry

**Description:** `Surface` takes its seed from its caller and can be handed events from the log, each waiting for room in the queue. `SurfaceEnv` says where the engine comes from, this process or the run's registry, and the painter's frame reads it. `yunta run` passes this process and its observer, and shows what it shows today. `painter.rs` is at 477 lines, so its tests move to a file of their own.

**Fails first:** `a_painter_reading_the_registry_calls_a_run_with_a_dead_engine_stalled`, in the painter's tests, because the painter always reads the engine as this process. Beside it, `events_handed_to_a_surface_are_never_dropped` hands twice the queue's depth while the painter is stood down and finds every one folded once it comes back.

**Acceptance criteria:**
- [x] A painter whose engine is read from a registry naming a dead process draws the run as stalled, and one asking at its terminal as asking; a painter on this process draws what it draws today.
- [x] Every event handed to a surface reaches the painter, in order, however full the queue is.
- [x] `run_surface`, `run_flow` and the surface's own tests pass unchanged.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --bin yunta surface`
- [x] Tests pass: `cargo test -p yunta --test run_surface --test run_flow`

**Dependencies:** None

**Files likely touched:**
- `crates/cli/src/surface/mod.rs`, `crates/cli/src/surface/feed.rs`
- `crates/cli/src/surface/painter.rs`, `crates/cli/src/surface/painter/tests.rs` (moved)
- `crates/cli/src/commands/drive/watch.rs`

**Estimated scope:** M

### Task 5: the follow loop, at a terminal and off one

**Description:** The follower reads the log once, opens the surface with it, and at the pace of `Pace` hands what came after its last seq to the surface and to its own `RunState`. It reads the registry once a second. When the phase is no longer created or running it takes the surface down, prints the closing block the way `drive.rs` does, and exits with its outcome. The interval is a const with its reason, 100 ms until Task 11 measures it.

**Fails first:** `a_follower_leaves_the_block_and_the_code_the_attached_run_leaves`, in `crates/cli/tests/status_follow.rs`: a run that stops on a gate is driven attached in one pty and followed in another, and the follower's closing block and exit code are compared with the attached run's. It fails because `--follow` on a moving run prints its page and returns at once.

**Acceptance criteria:**
- [x] At a terminal the follower draws the region while the run moves, and its closing block and exit code equal those of the attached run of the same log.
- [x] Off a terminal it prints a line per moment after the one it started on, the first saying why, then the same block and code.
- [x] Each read is one statement, the interval const says why it has its value, and `Pace` spaces the reads.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test status_follow`
- [x] Manual check: a held run followed at a terminal, read by eye.

**Dependencies:** Tasks 1, 3, 4

**Files likely touched:**
- `crates/cli/src/commands/status/follow.rs`
- `crates/cli/src/commands/drive.rs` (what prints the closing block, used from the follower)
- `crates/cli/tests/status_follow.rs`

**Estimated scope:** M

### Checkpoint B: after Tasks 3-5
- [x] `cargo test -p yunta --test status_follow --test run_surface --test run_flow` and `cargo xtask smells --check` pass.
- [ ] The user sees a run followed at a terminal and off one, and the two closing blocks side by side.

## Phase 3: the edges

### Task 6: Ctrl-C

**Description:** The follower watches the invocation's stop token. When it trips, the surface comes down, a note says that following stopped, that the run goes on and which command follows it again, and the exit is `RunExit::Interrupted`. Nothing is sent to the engine.

**Fails first:** `ctrl_c_stops_following_and_the_run_goes_on`, in `crates/cli/tests/status_follow.rs`: a follower in a pty on a held run is interrupted, exits 130 and says so, and the run, once released, finishes. It fails because the loop ignores the token and goes on following.

**Acceptance criteria:**
- [x] Exit 130, and the note says the run goes on and offers `yunta status <handle> --follow`.
- [x] The run's log gains no pause and ends `run_finished` once released, driven by the same engine throughout.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test status_follow`

**Dependencies:** Task 5

**Files likely touched:**
- `crates/cli/src/commands/status/follow.rs`, `crates/cli/src/commands/advice.rs` (`advice::follow`, which Task 9 also uses)
- `crates/cli/tests/status_follow.rs`

**Estimated scope:** S

### Task 7: an engine killed under the follower, and a run asking at its terminal

**Description:** A run whose log says it moves and whose registry names a dead engine is a stall: the follower takes the surface down, prints what `yunta status` prints of the run, and exits 1. An absent registry is not a stall. A run asking at its terminal is running, and the follower shows it asking and goes on.

**Fails first:** `an_engine_killed_under_a_follower_ends_it_as_a_stall`: the engine of a held run is killed with `kill -9`, and the follower should end within 2 s. It fails because the follower waits for a phase that never comes. Beside it, `an_absent_registry_is_not_a_stall`, in the follower's unit tests, and `a_run_asking_at_its_terminal_is_shown_asking_and_followed_on`, which may pass at once since Task 4 reads the engine from the registry; it guards that.

**Acceptance criteria:**
- [x] A killed engine ends the follower within 2 s, with the words `status` uses for a stalled run, exit 1.
- [x] A run handed over with no registry yet does not end the follower.
- [x] A run attached in one pty and asking at its gate is shown asking in the follower's pty; once answered there, the follower goes on to the run's end.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test status_follow`
- [x] Tests pass: `cargo test -p yunta --bin yunta follow`

**Dependencies:** Task 5

**Files likely touched:**
- `crates/cli/src/commands/status/follow.rs`
- `crates/cli/tests/status_follow.rs`
- `crates/cli/src/surface/region.rs`, `crates/cli/src/surface/view/mod.rs`, `crates/cli/src/surface/painter.rs`, and the region's callers in `region/tests.rs` and `scrollback.rs`: the attention row says whom the engine asks, which the region drew only for a parked run, so the guard test was red

**Estimated scope:** M

### Task 8: a promotion, a parent, and two followers that change nothing

**Description:** When the log closes the run as promoted, the follower reads its successor's log from then on, as the painter does. A parent's children are drawn under the nodes that bore them. Any number of followers leave the run as it is with none.

**Fails first:** `a_promotion_is_followed_onto_its_successor`, with the promoting workflow `run_flow/yunta_test.rs` already runs, because the follower ends at the predecessor's close. `a_parent_is_followed_with_its_children_under_their_nodes` and `two_followers_change_nothing` may pass at once; they guard what Task 4 and the rule that a follower only reads already give.

**Acceptance criteria:**
- [x] A promoted run is followed to its successor's end, and the follower ends with the successor's block and code.
- [x] A parent's region shows its open child under the node that bore it.
- [x] With two followers on a held run, `engine.json` and `engine.lock` are byte for byte what they are with none, and the run's log holds the kinds a run with no follower holds.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test status_follow`

**Dependencies:** Tasks 5, 7

**Files likely touched:**
- `crates/cli/src/commands/status/follow.rs`: the successor found by each run's head and its `promoted_from`, the first reader of the heads in the binary
- `crates/cli/src/surface/mod.rs`, `crates/cli/src/surface/painter.rs`: the painter reads the successor's registry once it moves onto it
- `crates/cli/tests/status_follow.rs`, `crates/cli/tests/status_follow/edges.rs`

**Estimated scope:** M

### Checkpoint C: after Tasks 6-8
- [x] `cargo xtask smells --check` passes.
- [x] Manual check: Ctrl-C, a killed engine and a run asking at its terminal, read at a terminal.
- [ ] The user sees the result.

## Phase 4: what it says

### Task 9: `--follow` where a run "follows it"

**Description:** `advice::follow(run)` spells `yunta status <run> --follow`, and the three places that offer `yunta status <run>` as what "follows it" use it: the line `run --detach` starts with, the next step of `resolve-gate`'s answer, and the advice of the refusal of a moving run. Goldens are blessed here only, and their diff is read before it is kept.

**Fails first:** `every_place_that_says_follows_it_offers_follow`, in `crates/cli/tests/status_follow/said.rs`, because each still offers `yunta status <run>`.

**Acceptance criteria:**
- [x] Each of the three offers `yunta status <handle> --follow` to a person, and the refusal's document under `--json` offers it with the whole id.
- [x] The goldens' diff holds only that change. No golden holds it, so none moved and nothing was blessed.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test status_follow`
- [x] Tests pass: `cargo test -p yunta --bin yunta advice`
- [x] Tests pass: `cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine`

**Dependencies:** Task 5

**Files likely touched:**
- `crates/cli/src/commands/advice.rs`, `crates/cli/src/commands/mod.rs`
- `crates/cli/src/commands/resolve_gate.rs`, `crates/cli/src/error.rs`
- `crates/cli/tests/status_follow/said.rs` (new)
- `crates/cli/tests/run_flow/guarding/said.rs`: `a_program_reads_the_refusal_as_one_document_with_the_next_commands` held the refusal's next step as `yunta status <id>`, which this task changes

**Estimated scope:** M

### Task 10: docs, agent docs, and the `//!` this module makes false

**Description:** What a person and a script rely on, and what a contributor needs, each in its one home.

**Acceptance criteria:**
- [x] `docs/compatibility.md` gives the exit codes of `status --follow` and keeps that `status` alone exits 0. `README.md`'s rows of `status` and `run`, and `docs/guide.md` where it speaks of a detached run and of gates from the outside, say how to follow a run.
- [x] `.agents/architecture.md` holds that a follower only reads, citing `[test cli/status_follow]`, the target that holds `two_followers_change_nothing`: the docs checker reads a target's own file, not its modules.
- [x] The `//!` of `surface/mod.rs`, which says the events come from the engine and not the log, and of `commands/status/mod.rs`, say what is true; the spec's file list matches what the module touched.

**Verification:**
- [x] Tests pass: `cargo xtask docs --check`
- [x] Tests pass: `cargo test -p yunta --test docs_sync`
- [x] Manual check: `cargo xtask docs --about` over each file the module touched.

**Dependencies:** Task 9

**Files likely touched:**
- `docs/compatibility.md`, `docs/guide.md`, `README.md`
- `.agents/architecture.md`
- `crates/cli/src/surface/mod.rs`, `crates/cli/src/commands/status/mod.rs`, `SPEC-log-follow.md`

**Estimated scope:** M

### Checkpoint D: after Tasks 9-10
- [x] `cargo test --workspace --locked --no-fail-fast` passes, with no binary aborted, and its count is explained against 264 binaries and 4,394 tests on macOS.
- [x] The user sees the result, and keeps `--follow` in the line attached `yunta run` starts with.

## Phase 5: proof and close

### Task 11: measured on a release build

**Description:** With mock sessions, how long an event takes to reach the follower, and whether a follower adds time to a run. The interval of Task 5 is kept or changed by what this finds, and `repro.sh` runs again.

**Acceptance criteria:**
- [x] `follow_latency.py`: over the events of a mock run followed off a terminal, each line reaches the follower within 250 ms of its event's timestamp.
- [x] `follow_cost.py`: 15 runs each, taking turns with and without a follower, and the median `between_secs` with one is no higher than the highest without.
- [x] `repro.sh` on `bin/yunta-after` follows the held run to its end, beside `before.txt`.

**Verification:**
- [x] Build succeeds: `cargo build --release --locked -p yunta`, installed with `install -m 755` as `.plans/yunta-tui/proof/log-follow/bin/yunta-after`.
- [x] Manual check: `latency-after.txt`, `cost-after.txt` and `after.txt` read against the criteria.

**Dependencies:** Checkpoint D

**Files likely touched:** only `.plans/yunta-tui/proof/log-follow/`

**Estimated scope:** S

### Task 12: the real run

**Description:** fragua, as `run-guard` ran it, with no token cap, started with `--detach` and followed in a pty up to `ship`, about 3.1M tokens. Then `abort` and `close`. Nothing is published.

**Acceptance criteria:**
- [x] The follower's terminal shows the region move node by node, and the follower ends at `ship` with its decision and exit 3.
- [x] `between_secs` stays under 1 s, beside the 0.668 s of the `run-guard` run.

**Verification:**
- [x] Manual check: `real-follow-after.txt`, with what the follower drew and `yunta stats` of the run.

**Dependencies:** Task 11

**Files likely touched:** only `.plans/yunta-tui/proof/log-follow/`

**Estimated scope:** S

### Task 13: every step of the `ci` job

**Description:** The whole job on the final tree, one build at a time and with `CARGO_INCREMENTAL=0`.

**Acceptance criteria:**
- [x] Every step passes.
- [x] No binary aborted: an exit of 101 with "0 failed" is an abort.
- [x] The smells baseline does not rise.

**Verification:**
- [x] Tests pass: `sh .plans/yunta-tui/tools/gate.sh /Users/eulke/development/yunta-tui .plans/yunta-tui/out/gate-log-follow`

**Dependencies:** Task 12

**Files likely touched:** none, unless the job asks for a fix

**Estimated scope:** XS

### Checkpoint E: after Tasks 11-13
- [x] All of the spec's success criteria are met.
- [x] The user reviews the whole diff and the before and after. **No commit until the user approves.**

### Task 14: commit the module

**Description:** One commit on `feature/yunta-tui`, with its code, tests, spec, plan and task list, pushed to the open draft, whose list and test plan are brought up to date.

**Acceptance criteria:**
- [x] `feat(cli): follow a run from another process`, with paths staged by name and no document cited in the message.
- [x] `.plans/` stays out of it, the main checkout is untouched, and `cargo clean` ran.
- [ ] CI on the pull request passes on Linux and macOS, with the new tests seen in the log.

**Verification:**
- [ ] Manual check: `git show --stat HEAD`, `git status` and `gh pr checks 30`.

**Dependencies:** Checkpoint E

**Files likely touched:** none new

**Estimated scope:** XS

## The spec's success criteria

| Criterion | Tasks |
|---|---|
| 1. `events_after` | 1 |
| 2. Each run's head | 2 |
| 3. At a terminal, the region, the block and the code | 4, 5 |
| 4. Off a terminal, the lines | 5 |
| 5. A run that is not moving | 3 |
| 6. Ctrl-C | 6 |
| 7. Following writes nothing | 8 |
| 8. A killed engine ends it as a stall | 7 |
| 9. Asking, promotion, children | 7, 8 |
| 10. `--follow` where a run "follows it" | 9 |
| 11. Latency and cost | 11 |
| 12. The real run | 12 |
| 13. The docs | 10 |
| 14. Every step of the `ci` job | 13 |

## Not in this list

The other modules of `CAPABILITY-MAP.md` have no spec, plan or tasks yet. Each goes through the same flow when this one closes.
