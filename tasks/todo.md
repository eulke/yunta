# Tasks: `parked-answers`

Spec: `SPEC-parked-answers.md`. Plan: `tasks/plan.md`. Status: **approved on 2026-11-05, in implementation**.

**How these run:** one at a time, with `incremental-implementation` and `test-driven-development`. Each task names the test that fails first and why; it is seen failing for that cause before the code that makes it pass. A task with no red test says so and says what proves it instead. Every task leaves the tree compiling and its tests green.

**Commands:** `cargo` here always means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui`, which builds with no incremental state and gives any `yunta` a home of its own. Before editing a file, `cargo xtask docs --about <file>`.

```
1 ─ 2 ─ [A] ─ 3 ─ 4 ─ 5 ─ 6 ─ [B] ─ 7 ─ 8 ─ 9 ─ [C] ─ 10 ─ 11 ─ 12 ─ [D] ─ 13 ─ 14 ─ [E] ─ 15 ─ 16 ─ 17 ─ 18 ─ [F] ─ 19
```

The evidence of the base is already kept in `.plans/yunta-tui/proof/parked-answers/`, with its binary as `bin/yunta-before`, so no task runs it again.

## Phase 1: an answer reaches an engine

### Task 1: The resume an answer starts waits for the engine that is leaving

**Description:** `contention_for` and `refuse_if_driven` read the run's pause, which only `run_resumed` ends, instead of `tail_paused`, which an answer makes false. This repairs what `run-guard` broke.

**Fails first:** `a_gate_answered_while_its_engine_leaves_goes_on`, in a new `parked_runs/handoff.rs`, with a process that ends by itself as the holder. The node after the gate never starts, because the resume is refused and the run stays reading `running`.

**Acceptance criteria:**
- [x] A gate answered beside a live holder goes on once the holder ends, with nobody resuming by hand.
- [x] A run moving with no pause is still refused at once.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test parked_runs handoff`
- [x] Tests pass: `cargo test -p yunta-engine --test run_guard`
- [x] Tests pass: `cargo test -p yunta --test run_flow guarding`

**Dependencies:** None

**Files likely touched:**
- `crates/engine/src/run_hold.rs`
- `crates/cli/src/commands/resume.rs`
- `crates/engine/tests/run_guard.rs`
- `crates/cli/tests/parked_runs.rs`
- `crates/cli/tests/parked_runs/handoff.rs` (new)

**Estimated scope:** M

### Task 2: An answer no engine can take is refused before it is recorded

**Description:** Whatever records an answer and starts a resume asks `refuse_if_driven` first, because the resume's own refusal reaches only `detached.log`.

**Fails first:** `a_gate_answered_beside_an_unreadable_lock_records_nothing`, because `resolve-gate` exits 0 and the log gains the pair.

**Acceptance criteria:**
- [x] `resolve-gate` beside an unreadable lock exits 1, names the file, and the log gains nothing.
- [x] `resolve_gate` and `answer_questions` over MCP return the same refusal with the whole id, and record nothing.
- [x] A finished run keeps its sentence: `answering.rs` does not change. On a moving run, `resolve-gate` says who drives it.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test parked_runs handoff`
- [x] Tests pass: `cargo test -p yunta --test mcp_flow`
- [x] Tests pass: `cargo test -p yunta --test run_flow answering`

**Dependencies:** Task 1

**Files likely touched:**
- `crates/cli/src/commands/resolve_gate.rs`
- `crates/cli/src/commands/mcp/tools.rs`
- `crates/cli/tests/parked_runs/handoff.rs`
- `crates/cli/tests/mcp_flow.rs`
- `crates/cli/tests/run_flow/guarding.rs`, `crates/cli/tests/run_flow/guarding/said.rs` (new: the test of a moving run went in `guarding.rs`, which passed the repository's 500 lines, so what its refusals say moved beside it)

**Estimated scope:** M. It went past five files for that one reason.

### Checkpoint A: after Tasks 1-2
- [x] `probe_handoff.sh` on a debug build: the run goes on in case 1, and `resolve-gate` refuses in case 3.
- [x] `cargo xtask smells --check` passes.
- [x] The user sees the result.

## Phase 2: the run's own stop

### Task 3: A decision belongs to a node or to the run

**Description:** `DecisionOn::{Node, Run}` replaces the `NodeId` that `current_escalation` and `awaits_decision` return. Nothing produces `Run` yet, so this changes no behavior; every surface gets its arm through the compiler, with the words the spec gives.

**Fails first:** no red test: it changes a type and no behavior. The suites below, unchanged in their promises, are the proof.

**Acceptance criteria:**
- [x] Both functions return `DecisionOn`, and `Decided`, `Asked` and the closing block carry it.
- [x] A `Run` decision reads "decision on the run", and `DecisionJson` leaves out `node`.
- [x] `yunta test` matches only `Node`. The edits in engine tests change a type and no promise.

**Verification:**
- [x] Build succeeds: `cargo check --workspace --all-targets`
- [x] Tests pass: `cargo test -p yunta --test parked_runs`
- [x] Tests pass: `cargo test -p yunta-engine --test failed_node_retry --test retry_asks_again --test project_commands`

**Dependencies:** Checkpoint A

**Files likely touched:**
- `crates/engine/src/run/escalation.rs`, `crates/engine/src/lib.rs`
- `crates/cli/src/json/mod.rs`, `crates/cli/src/commands/status/decision_json.rs`, `crates/cli/src/commands/status/decision.rs`, `crates/cli/src/commands/status/mod.rs`
- `crates/cli/src/run_page/mod.rs`, `crates/cli/src/surface/closing/mod.rs`, `crates/cli/src/commands/resolve_gate.rs`, `crates/cli/src/commands/test/case.rs`
- `crates/engine/tests/failed_node_retry.rs`, `project_commands.rs`, `deviation/surfaces.rs`, `common/menu.rs`

**Estimated scope:** L. It passes five files because one type cannot change in part and still compile.

### Task 4: A run stopped on the cap shows its decision

**Description:** `RunState` keeps the run's id from the log. `over_budget_escalation` takes that id instead of `RunCtx`. `current_escalation` rebuilds the menu from a `BudgetExhausted` pause.

**Fails first:** `a_run_stopped_on_the_cap_shows_its_decision`, in a new `parked_runs/cap.rs`, because the page says "no option answers this pause here".

**Acceptance criteria:**
- [x] `status` shows the spend, `continue`, `abort` and the command for each.
- [x] `status --json` carries `decision` with `budget` and no `node`; `list` counts it as answerable.
- [x] `a_pause_with_no_menu_still_says_what_the_run_is_waiting_on` moves to the parent of a paused child, and keeps its promise.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test parked_runs`
- [x] Tests pass: `cargo test -p yunta-engine --test run_gates_limits`

**Dependencies:** Task 3

**Files likely touched:**
- `crates/engine/src/replay.rs`
- `crates/engine/src/run/budget.rs`
- `crates/engine/src/run/escalation.rs`
- `crates/cli/tests/parked_runs.rs`
- `crates/cli/tests/parked_runs/cap.rs` (new)

**Estimated scope:** M

### Task 5: The log can tell an answer is waiting

**Description:** `GateLedger` keeps the latest `gate_resolved` with no node. `RunState::cap_answer` counts it only after the cap's pause and with no `run_resumed` after it. `answer_awaits_its_resume` asks this while the run stands on the cap.

**Fails first:** in a new `engine/tests/parked_answers.rs`, a recorded `continue` that leaves the run still offering the decision, because replay drops a gate event with no node.

**Acceptance criteria:**
- [x] After an answer, the run reads as moving and offers nothing.
- [x] A second answer is refused, and the log gains no second pair.
- [x] A `continue` given earlier at a terminal does not count, nor does one before a newer pause.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-engine --test parked_answers`
- [x] Tests pass: `cargo test -p yunta-core`
- [x] Tests pass: `cargo test -p yunta-engine --test properties`

**Dependencies:** Task 4

**Files likely touched:**
- `crates/core/src/events/gates/ledger.rs`
- `crates/engine/src/replay/refuse.rs`
- `crates/engine/src/replay/decisions.rs`
- `crates/engine/tests/parked_answers.rs` (new)

**Estimated scope:** M

### Task 6: An engine takes that answer once

**Description:** At its wake, an engine reads the answer before `run_resumed` is written: `continue` lifts the cap, and `abort` pauses again with the same reason and asks nobody.

**Fails first:** `continue_from_another_process_goes_past_the_cap`, because the node left never starts and the run parks again.

**Acceptance criteria:**
- [x] `resolve-gate continue` exits 0, the node left starts, and the run ends. `abort` stays parked, with the decision open and no node started.
- [x] An engine killed with `kill -9` after it started spending leaves the next one stopped on the cap.
- [x] With an answer waiting, an engine does not ask its surface; with none, it asks. `budget_authorization_is_per_invocation_a_resume_asks_again` keeps its assertions, and one clause of its comment changes.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test parked_runs cap`
- [x] Tests pass: `cargo test -p yunta-engine --test parked_answers --test run_gates_limits`

**Dependencies:** Task 5

**Files likely touched:**
- `crates/engine/src/run/exec.rs`
- `crates/engine/src/run/budget.rs`
- `crates/engine/tests/parked_answers.rs`
- `crates/engine/tests/run_gates_limits.rs` (a comment)
- `crates/cli/tests/parked_runs/cap.rs`
- `crates/cli/tests/parked_runs.rs` (the parked run takes a workflow, and its fixture gains the session of the node left)

**Estimated scope:** M. It went past five files for that one reason.

### Checkpoint B: after Tasks 3-6
- [x] `repro.sh` scenario A goes past the cap on a debug build.
- [x] `cargo xtask smells --check` passes.
- [x] The user sees the result.

## Phase 3: a cut session

### Task 7: A session cut at its share offers `continue`

**Description:** `Failure::TokenBudget` gets a `Continue` remedy with no precondition, `runs_again` includes `Continue`, and the menu says what it does. This task covers the offer; lifting the cap is the next one.

**Fails first:** `a_cut_session_offers_continue_and_refuses_retry`, with sessions past their share, because the menu offers `retry` and refuses `continue`.

**Acceptance criteria:**
- [x] The menu reads `continue: Run it again past the cap (attempt 2)` and `abort`, on the page and in the document, with a `null` precondition.
- [x] `retry` is refused as an option off the menu.
- [x] `authorizing_continue_lifts_the_iteration_cap_for_this_invocation` stays green with no change, which is what holds the loop's live `continue`.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-core --test events`
- [x] Tests pass: `cargo test -p yunta-render`
- [x] Tests pass: `cargo test -p yunta --test parked_runs cap`
- [x] Tests pass: `cargo test -p yunta-engine --test run_gates_limits --test failed_node_retry`

**Dependencies:** Checkpoint B

**Files likely touched:**
- `crates/core/src/events/failure/remedy.rs`, `crates/core/src/events/failure/remedy/tests/static_outlook.rs`
- `crates/core/src/reserved.rs`
- `crates/render/src/blocks/failure.rs`, `crates/render/src/offers.rs` (the tradeoff has one home)
- `crates/cli/tests/parked_runs.rs` (the parked run takes a fixture), `crates/cli/tests/parked_runs/cap.rs`

**Estimated scope:** L. It went past five files for those two reasons.

### Task 8: `continue` lifts the cap for the engine that applies it

**Description:** The lift has two sites, one answer each: at the wake, for a node still `Failed` whose chosen option is `Continue`, and in `failure_escalation`, for an answer given at a terminal.

**Fails first:** `continue_runs_a_cut_session_past_its_share`, because attempt 2 gets the same share and is cut the same way.

**Acceptance criteria:**
- [x] A cut session goes on past its share on `resolve-gate continue`, and the run ends.
- [x] A terminal's answer does the same.
- [x] An engine killed mid-attempt leaves the next one under its share, and it asks again when cut. One that dies before starting the attempt leaves the answer standing.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test parked_runs cap`
- [x] Tests pass: `cargo test -p yunta-engine --test parked_answers --test session_budget`

**Dependencies:** Task 7

**Files likely touched:**
- `crates/engine/src/run/budget.rs`
- `crates/engine/src/run/steps.rs` (two lines, since the file is at 492)
- `crates/engine/src/replay/decisions.rs` (`past_the_cap_chosen`, where `exec.rs` needed no change of its own)
- `crates/engine/tests/parked_answers.rs`
- `crates/cli/tests/parked_runs/cap.rs`

**Estimated scope:** M

### Task 9: A stop on the cap keeps a node's answer

**Description:** `RunLedger::last_paused_at` skips a `BudgetExhausted` pause, which ends the alternation between `retry` and `continue`.

**Fails first:** `a_retry_that_reached_the_cap_runs_after_continue`, with its answers counted: after `continue` the engine parks on the failed node, because the `retry` was closed by the cap's pause.

**Acceptance criteria:**
- [x] `retry`, then `continue`, and the attempt runs once. Two answers, and the run moves on.
- [x] Any other pause still closes a node's answer.
- [x] While the run stands on the cap, the page shows the run's decision. A log in the base's state, with that `retry` on it, reads and goes on.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-core`
- [x] Tests pass: `cargo test -p yunta --test parked_runs cap`
- [x] Tests pass: `cargo test -p yunta-engine --test parked_answers --test failed_node_retry --test retry_asks_again --test properties`

**Dependencies:** Task 8

**Files likely touched:**
- `crates/core/src/events/run/ledger.rs`, `crates/core/src/events/run/ledger/tests.rs`
- `crates/engine/tests/parked_answers.rs`
- `crates/cli/tests/parked_runs/cap.rs`

**Estimated scope:** S

### Checkpoint C: after Tasks 7-9
- [x] `probe_share.sh` on a debug build moves on with one answer per stop, and the attempt number moves.
- [x] `cargo xtask smells --check` passes.
- [x] The user sees the result.

## Phase 4: every interface

### Task 10: Over MCP, the same as the CLI

**Description:** A real client over stdio reads and answers both stops, and the parity test holds the page, the document and MCP to the same options.

**Fails first:** no red test: Tasks 4 to 9 already pass through the shared path, and this is what proves it. If one of these fails, an interface differs, and that is a defect to fix here.

**Acceptance criteria:**
- [x] `workflow_status` shows both decisions as `status --json` does.
- [x] `resolve_gate` with `continue` gives the CLI's result on both, with the whole id in what it says.
- [x] The options, the evidence and the commands match on the page, in the document and over MCP.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test mcp_flow`
- [x] Tests pass: `cargo test -p yunta --test surface_parity`

**Dependencies:** Checkpoint C

**Files likely touched:**
- `crates/cli/tests/mcp_flow.rs`, `crates/cli/tests/mcp_flow/cap.rs`
- `crates/cli/tests/surface_parity.rs`

**Estimated scope:** S

### Task 11: The parent offers the command that reaches its child

**Description:** `after_for` gains `yunta status <child>` for a frame parked on a child's pause, named for its reader.

**Fails first:** `the_parent_of_a_stopped_child_offers_the_command_that_reaches_it`, because `next` names only the parent.

**Acceptance criteria:**
- [x] The parent's page and document offer the command, by handle for a person and whole for a program.
- [x] `resolve-gate <child> continue` ends the child, and `yunta resume <parent>` takes it from there.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --bin yunta advice`
- [x] Tests pass: `cargo test -p yunta --test parked_runs child`

**Dependencies:** Task 10

**Files likely touched:**
- `crates/cli/src/commands/advice.rs`
- `crates/cli/src/commands/advice/tests.rs`
- `crates/cli/tests/parked_runs.rs`, `crates/cli/tests/parked_runs/child.rs` (`cap.rs` would pass 500 lines)

**Estimated scope:** S

### Task 12: `schema_version` moves to 9

**Description:** One constant, the two tests that write the number, and the lines of `docs/compatibility.md` that say it.

**Fails first:** the two tests with the new number, because the document says 8.

**Acceptance criteria:**
- [x] Every document that shares the stamp carries 9.
- [x] `docs/compatibility.md` says what moved it.

**Verification:**
- [x] Tests pass: `cargo test -p yunta --test parked_runs --test status_nodes_cmd --test command_output`

**Dependencies:** Task 11

**Files likely touched:**
- `crates/cli/src/json/mod.rs`
- `crates/cli/tests/parked_runs/menu.rs`
- `crates/cli/tests/status_nodes_cmd.rs`
- `docs/compatibility.md`

**Estimated scope:** S

### Checkpoint D: after Tasks 10-12
- [x] `cargo test --workspace --locked --no-fail-fast` passes, with no binary aborted, and its count is explained against the base's 263 binaries and 4,352 tests.
- [x] The user sees the result.

## Phase 5: what it says

### Task 13: Both caps say what is true

**Description:** `PAST_THE_CAP` goes away, so neither cap ends on "resume with an interactive surface". The refusal for a pause with no menu stops naming a budget cap. Goldens are blessed here, and their diff is read before it is kept.

**Fails first:** the sentences of both caps, written by hand in `failure_families.rs` and in the tests of `pause.rs`, because the last clause is still there.

**Acceptance criteria:**
- [x] The spec's sentences for both caps read exactly as written there.
- [x] `resolve_gate`'s refusal names no budget cap.
- [x] The goldens' diff holds only these sentences. No golden holds them, so none moved and nothing was blessed.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-render`
- [x] Tests pass: `cargo test -p yunta --bin yunta error::words`
- [x] Tests pass: `cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine`

**Dependencies:** Checkpoint D

**Files likely touched:**
- `crates/render/src/pause.rs`
- `crates/render/src/tests/failure_families.rs`
- `crates/cli/src/error/words.rs`
- `crates/cli/src/error/words/tests.rs`

**Estimated scope:** M

### Task 14: A loop's `retry` says what it does

**Description:** `Failure::LoopCapped` gets a `Retry` with no precondition, and the words of that retry are chosen from the menu's failure. `Precondition::CapLifted` stays readable.

**Fails first:** `a_capped_loop_s_retry_says_its_count_starts_over`, because the label says "once a person lifts the cap on resume".

**Acceptance criteria:**
- [x] The label reads `Run it again (attempt 2): its iteration count starts over`, and its tradeoff ends saying it stops again at the cap if tasks remain.
- [x] Logs holding `cap_lifted`, or a `retry` on a cut session, still read.
- [x] `cargo xtask schema --check` passes, after regenerating `events.json`: two descriptions changed and its shape did not. No `///` goes on a unit variant.

**Verification:**
- [x] Tests pass: `cargo test -p yunta-core --test events`
- [x] Tests pass: `cargo test -p yunta-render`
- [x] Tests pass: `cargo test -p yunta-engine --test older_words --test run_gates_limits`
- [x] Manual check: `probe_loop.sh` on a debug build gives 2 of 5, 4 of 5, finished.

**Dependencies:** Task 13

**Files likely touched:**
- `crates/core/src/events/failure/remedy.rs`, `crates/core/src/events/failure/remedy/tests/static_outlook.rs`, `crates/core/schemas/events.json` (a `retry` is no longer always set on `precondition`, and the field's own `///` said it was)
- `crates/render/src/blocks/failure.rs`, `crates/render/src/tests/retry.rs`
- `crates/engine/tests/older_words.rs`

**Estimated scope:** M

### Checkpoint E: after Tasks 13-14
- [x] `cargo xtask smells --check` passes.
- [x] Manual check: I read both stops and the loop's page for real, at a terminal and with `--json`.
- [x] The user sees how they read.

## Phase 6: documentation

### Task 15: Explain it to whoever uses Yunta

**Description:** What each stop means and how to go on, on every interface, and what a person can rely on from one answer.

**Acceptance criteria:**
- [x] `docs/troubleshooting.md` covers `exceeded max_tokens` and `its token budget is spent`, and its section on waiting for a limit's reset does not change.
- [x] `docs/compatibility.md` says three pauses rebuild a menu, and says `decision` can have no `node`.
- [x] The rows of `yunta resolve-gate` and `yunta mcp` in `README.md` say it.

**Verification:**
- [x] Tests pass: `cargo xtask docs --check`

**Dependencies:** Checkpoint E

**Files likely touched:**
- `docs/troubleshooting.md`
- `docs/compatibility.md`
- `README.md`

**Estimated scope:** S

### Task 16: Record it for whoever works on Yunta

**Description:** The rule, what stays open, and every comment this module made false, each in the one home the repository gives it.

**Acceptance criteria:**
- [x] `.agents/architecture.md` holds "One answer pays for one engine", citing its test.
- [x] `.agents/open.md` holds the parent left parked and how the cap is shared. `CAPABILITY-MAP.md` no longer says a loop's cap is raised.
- [x] The `//!` of `budget.rs` and `status/decision.rs`, and the docs of `json/mod.rs` and `advice.rs`, say what is true.

**Verification:**
- [x] Tests pass: `cargo xtask docs --check`
- [x] Manual check: `cargo xtask docs --about` over each file the module touched, keeping true what it prints.

**Dependencies:** Task 15

**Files likely touched:**
- `.agents/architecture.md`, `.agents/open.md`
- `CAPABILITY-MAP.md`
- `crates/engine/src/run/budget.rs`, `crates/engine/src/run/escalation.rs`, `crates/cli/src/commands/status/decision.rs`, `crates/cli/src/json/mod.rs`, `crates/cli/src/commands/advice.rs`, `crates/cli/src/run_page/mod.rs`, `crates/cli/src/error.rs`, `crates/cli/src/surface/view/mod.rs`
- `SPEC-parked-answers.md`: its file list, to what the module touched

**Estimated scope:** M

## Phase 7: proof and close

### Task 17: Measure on a release build

**Description:** The scripts of the base again, and the real run: fragua under `limits.max_tokens_per_run: 2000000`, cut at `grill`, on with `resolve-gate continue`, up to `ship`, then `abort` and `close`. About 3.4M tokens, and nothing is published.

**Acceptance criteria:**
- [x] `repro.sh`, `probe_share.sh`, `probe_loop.sh` and `probe_handoff.sh` give what the spec says, beside their results at the base.
- [x] The real run reaches `ship` on one answer.
- [x] From `gate_resolved` to the next `node_started` takes under 0.5 s, and the whole run's time outside nodes and pauses stays under 1 s.

**Verification:**
- [x] Build succeeds: `cargo build --release --locked -p yunta`
- [x] Manual check: compare `after.txt`, `share-after.txt`, `loop-after.txt`, `handoff-after.txt` and `real-cap-after.txt` with their pairs.

**Dependencies:** Task 16

**Files likely touched:** only `.plans/yunta-tui/proof/parked-answers/`

**Estimated scope:** S

### Task 18: Pass every step of the `ci` job

**Description:** The whole job on the final tree, one build at a time and with `CARGO_INCREMENTAL=0`.

**Acceptance criteria:**
- [x] Every step passes.
- [x] No binary aborted: an exit of 101 with "0 failed" is an abort.
- [x] The smells baseline does not rise.

**Verification:**
- [x] Tests pass: `sh .plans/yunta-tui/tools/gate.sh /Users/eulke/development/yunta-tui .plans/yunta-tui/out/gate-parked-answers`

**Dependencies:** Task 17

**Files likely touched:** none, unless the job asks for a fix

**Estimated scope:** XS

### Checkpoint F: after Tasks 15-18
- [x] All of the spec's success criteria are met.
- [x] The user reviews the whole diff and the before and after of Task 17. **No commit until the user approves.**

### Task 19: Commit the module

**Description:** One commit on `feature/yunta-tui`, with its code, tests, spec, plan and task list, pushed to the open draft, whose list and test plan are brought up to date.

**Acceptance criteria:**
- [x] `feat(engine): a run stopped by its token cap goes on from any interface`, with paths staged by name and no document cited in the message.
- [x] `.plans/` stays out of it, the main checkout is untouched, and `cargo clean` ran.
- [ ] CI on the pull request passes on Linux and macOS, with the new tests seen in the log.

**Verification:**
- [ ] Manual check: `git show --stat HEAD`, `git status` and `gh pr checks 30`.

**Dependencies:** Checkpoint F

**Files likely touched:** none new

**Estimated scope:** XS

## The spec's success criteria

| Criterion | Tasks |
|---|---|
| 1. A run stopped on the cap shows its decision on every surface | 3, 4, 10 |
| 2. `continue` goes past the cap in a detached engine | 6 |
| 3. A cut session offers `continue`, refuses `retry` and goes on | 7, 8 |
| 4. A `retry` that reached the cap moves on with one `continue` | 9 |
| 5. MCP gives the same results | 10 |
| 6. `abort` leaves the run where it stopped | 6 |
| 7. One answer pays for one engine | 5, 6, 8 |
| 8. A terminal asks with no answer, and takes one that waits | 6, 8 |
| 9. A parent offers the command that reaches its child | 11 |
| 10. An answer always reaches an engine | 1, 2 |
| 11. A loop's page says nothing false, and older logs read | 13, 14 |
| 12. `schema_version: 9` | 3, 12 |
| 13. A real run under the reference's cap reaches `ship` | 17 |
| 14. Every step of the `ci` job | 18 |

## Not in this list

The other modules of `CAPABILITY-MAP.md` have no spec, plan or tasks yet. Each goes through the same flow when this one closes.
