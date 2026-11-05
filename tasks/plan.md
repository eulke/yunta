# Implementation Plan: `parked-answers`

Spec: `SPEC-parked-answers.md`, approved on 2026-11-05. Status: **approved on 2026-11-05**. Task list: `tasks/todo.md`.
Base: `feature/yunta-tui` at `b213d605`, on `main` at `34790ab8`, which had not moved when this was written.

## Overview

A run stopped by the token cap gets a decision every interface can read and answer. Two stops get one: the run's own stop on the cap, whose decision belongs to the run and not to a node, and a session cut at its share, whose node offers `continue` in place of a retry that cannot help. An answer goes through the path every gate already uses, and it lifts the cap for the one engine that takes it. The hand-off that `run-guard` broke is fixed first, because everything here goes through it. No event kind, no field in the log and no dependency are added.

## Architecture Decisions

**1. Who waits for a leaving engine reads the run's pause, not the tail of the log.**
- `contention_for` and `refuse_if_driven` read `state.run.paused()`, which only `run_resumed` ends, instead of `tail_paused`, which an answer makes false.
- `tail_paused` keeps its one other reader: `resolve_gate` refuses a second answer with it, and that behavior is right.

**2. An answer asks `refuse_if_driven` before it is recorded.**
- In `record` in `resolve_gate.rs`, which the CLI and MCP share, and in `tool_answer_questions`.
- A live holder of a parked run passes, because it is leaving. What no waiting can clear refuses, with the sentence `run-guard` already writes.
- On a moving run, `resolve-gate` now says who drives it instead of the vaguer "isn't parked at a pause". A finished run has no lock and keeps its sentence, so `answering.rs` stays as it is.

**3. A decision belongs to a node or to the run, as a type.**
- `DecisionOn::{Node(NodeId), Run}` is what `current_escalation` and `awaits_decision` return beside the escalation.
- The compiler walks every surface: `Decided`, `Asked`, `DecisionJson`, the closing block and `yunta test`, whose case matches only `Node` and so keeps stopping at the cap, as the spec settled.
- Rejected: a second function for the run's decision. A new surface calling the old one would miss the cap with no warning.
- Rejected: `Option<NodeId>`, where `None` already means "no decision" a few lines away.

**4. `RunState` keeps the run's id from the log it folds.**
- The claim of the menu names the run, and `current_escalation` is given no id. `StoredEvent` carries it, so replay reads it there, and it stays out of the state's equality, like the clock.
- Rejected: an `id` parameter, which touches about twenty test call sites for no promise. Rejected: reading the last component of `run_dir`, which makes a path a fact.

**5. The run's menu is rebuilt from its pause.**
- In `current_escalation`, ahead of the schedule's decision: a `BudgetExhausted` pause with no answer waiting is `(DecisionOn::Run, over_budget_escalation(..))`.
- `over_budget_escalation` takes the run id instead of `RunCtx`, so the live path and the rebuilt one build the same menu.

**6. The answer to the run's stop lives in `GateLedger`, and its window is the pause.**
- Replay ignores a gate event with no node today. `GateLedger` keeps the latest `gate_resolved` with no node and its sequence.
- `RunState::cap_answer` counts it only when `run.paused()` is the cap and the answer stands after that pause. `paused()` already drops a pause with a `run_resumed` after it, so the window closes when an engine takes the answer, and a `continue` given earlier at a terminal never qualifies.
- `answer_awaits_its_resume` asks this while the run stands on the cap, and node answers otherwise.

**7. An engine takes the run's answer once, at its wake.**
- In `start`, in `exec.rs`, with the state read before `wake::resume` writes `run_resumed`. `continue` sets `budget_lifted`. `abort` pauses again with the same reason and returns the report, as a run whose log already ended does, and asks nobody.
- The logic lives in `budget.rs`, which owns the cap, because `steps.rs` is at 492 lines.

**8. A session cut at its share offers `continue`.**
- `Failure::TokenBudget` gets `Remedy { option: Continue, precondition: None }`. `Remedy`'s shape does not change, so the schemas do not either.
- `ReservedOption::runs_again` gains `Continue`, so `retry_chosen` runs the node through `Decision::Execute` like any other choice. Its other readers are `reopen.rs` and `steps.rs:353`, both of which only act after a failure on a node's latest choice, so the loop cap's live `continue` reaches neither in a way that matters. A test holds it: `authorizing_continue_lifts_the_iteration_cap_for_this_invocation`.
- The lift has two sites, one answer each: at the wake, for a node still `Failed` whose chosen option is `Continue`, and in `failure_escalation`, for an answer given at a terminal. Once the attempt starts the node is not `Failed`, so a later engine does not lift. An engine that dies before starting it has spent nothing, and the answer still stands.

**9. A stop on the cap does not close a node's answer.**
- `RunLedger::last_paused_at` skips a `BudgetExhausted` pause, and it has one reader, `unconsumed`. This is what ends the alternation the spec describes: `retry` stops on the cap, `continue` wakes an engine, and the `retry` still stands, so that engine runs the attempt.
- Its doc says why, in the ledger.

**10. A loop's `retry` waits for nothing, and says so.**
- `Failure::LoopCapped` gets a `Retry` with no precondition. `Precondition::CapLifted` stays, because logs hold it, and the enum's own `///` says so, since a `///` on a unit variant reshapes the schema.
- `retry_words` cannot say this from a precondition, so the words of a retry on a capped loop are chosen from the menu's failure.
- `Remedy`'s `///` says a `retry` always names its input, which stops being true. Its words and the enum's change, so `events.json` is regenerated: two descriptions move and its shape does not.
- `PAST_THE_CAP` goes away, and `exceeded_loop_cap` and `budget` stop saying it.

**11. `schema_version` moves to 9, once.**
- The constant in `json/mod.rs`, the two tests that write the number, and `docs/compatibility.md`, which says what moved it.

**12. The parent of a stopped child offers the command that reaches it.**
- In `after_for`, for a `NeedsYou` frame whose pause is `ChildPaused`: `advice::status` of the child, named for the reader. This shows on any child's pause, since the page is the same one.

## Dependency Graph

```
hand-off (1, 2)
   │
   ├── DecisionOn through every surface, with no change of behavior (3)
   │      │
   │      └── the run's id (4) ── the run's menu (5) ── its answer (6) ── the engine takes it (7)
   │                                                                          │
   │                                                                          ├── a cut session (8)
   │                                                                          └── a stop that keeps a node's answer (9)
   │
   └── sentences (10), stamp (11), the parent's command (12) ── documentation ── proof
```

Every step is sequential. The hand-off, the type and the engine's answer touch the same files, and the proof needs all of them, so nothing here runs in parallel.

## Phases

### Phase 0: The evidence of the base
The results are already kept beside the scripts in `.plans/yunta-tui/proof/parked-answers/`: `before.txt`, `details-before.txt`, `loop-before.txt`, `share-before.txt`, `handoff-probe.txt` and `real-cap-before.txt`. Their binary, built from the code at `b213d605`, is kept there as `yunta-before`. Nothing runs again.

### Phase 1: An answer reaches an engine (1, 2)
The first thing fixed, because each later phase answers through it. It fails first with a gate answered beside a live holder, leaving the run reading `running`.
**Checkpoint A:** `probe_handoff.sh` shows the run going on, and the `guarding` tests and `mcp_flow` pass.

### Phase 2: The run's own stop (3, 4, 5, 6, 7)
Vertical in three steps. First the type, through every surface, with the suite unchanged, which separates a refactor from a behavior. Then the menu shows and an answer is recorded. Then an engine takes it.
**Checkpoint B:** `repro.sh` scenario A goes past the cap, off a terminal. `abort` stays parked. An engine killed after taking the answer leaves the next one stopped on the cap. `budget_authorization_is_per_invocation_a_resume_asks_again` passes.

### Phase 3: A cut session (8, 9)
It fails first with `continue` refused as an option off the menu. The test for the alternation goes here, with its answers counted.
**Checkpoint C:** `probe_share.sh` moves on with one answer in each shape: cut at its share, and failed otherwise with the cap spent.

### Phase 4: Every interface (11, 12)
MCP with a real client, the parity test, a terminal taking a waiting answer without asking, the parent's command and the stamp.
**Checkpoint D:** the whole suite against the base's count, with no binary aborted.

### Phase 5: What it says (10)
Both caps, the loop's `retry`, and the refusal that stops naming a budget cap. Goldens are blessed only here, and their diff is read before it is kept.
**Checkpoint E:** I read the pages at a terminal and with `--json`, and the user sees how they read.

### Phase 6: Documentation
`docs/compatibility.md`, `docs/troubleshooting.md` and the rows of `README.md`. `.agents/architecture.md` gets the rule "One answer pays for one engine". `.agents/open.md` gets the parent left parked and how the cap is shared. `CAPABILITY-MAP.md` is corrected: a loop's cap is not raised. The `//!` of `budget.rs`, `status/decision.rs` and the docs of `json/mod.rs` stop saying this pause has no menu.

### Phase 7: Proof and close
A release build; `repro.sh`, `probe_share.sh`, `probe_loop.sh` and `probe_handoff.sh` against their results at the base; the real run with its time read from the log; every step of the `ci` job.
**Checkpoint F:** the user reviews the whole diff and the before and after. **No commit until the user approves.** Then one commit, a push to the open draft, and a look at CI on Linux.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| An answer authorizes a second engine | High: it spends money nobody approved | The window reads `paused()`, and the lift lives in memory. Tests kill an engine with `kill -9` in both shapes, and `budget_authorization...` keeps its assertions |
| `runs_again` gaining `Continue` reaches a path I did not read | Medium | Decision 8 names its three readers, and `run_gates_limits` and `reopen.rs`'s tests stay green with no change |
| Keeping a node's answer across the cap's stop changes `unconsumed` for every answer | Medium | Only a `BudgetExhausted` pause is skipped. `failed_node_retry`, `retry_asks_again` and the replay properties stay green. A base log in the probe's state is read in a test |
| `answer_awaits_its_resume` reads a kept answer as "running" on a run standing on the cap | High: the page would hide the decision | Decision 6 asks the cap's answer while on the cap, with a test that reads the page in that state |
| An older log | High: `A log an earlier binary wrote still replays` | `CapLifted` and a `retry` on `TokenBudget` stay readable, with tests in `older_words`. A menu that no longer offers an older `retry` asks again |
| The JSON stamp moves under a reader | Low | Said in `docs/compatibility.md`. The next modules move it only with a reason of their own |
| The real run spends about 3.4M tokens and shows nothing new | Low | It is the same request as the base, and its first stop is already measured. Nothing is published, because the forge names a repository that does not exist |
| Lifting the cap leaves a detached engine with no limit | Medium, and accepted by the spec | The tradeoff says it on the menu, and `yunta cancel` stops it. A session still has its timeout |
| The repository's size checks | Low | `steps.rs` (492) and `resolve_gate.rs` (488) take one line each at most. New logic goes in `budget.rs`, and new tests in new files under `parked_runs/` |
| Goldens hide a change nobody meant | Medium | Blessed in one phase, with the diff read before it is kept |

## Open Questions

None.
