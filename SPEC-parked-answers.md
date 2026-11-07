# Spec: `parked-answers`

Module of `CAPABILITY-MAP.md`. Status: **approved on 2026-11-05, reopened by a measurement and approved again the same day**.
Base: `feature/yunta-tui` at `b213d605`. Depends on: nothing. Needed by: the "decide" slice of `tui`.

## Objective

A run stopped by the token cap can be continued from any interface, not only from a terminal attached to the engine that goes on. A run meets the cap in two ways: a session cut at its share of it, which is what a real run meets first, and the run itself on the cap.

### What happens at the base commit

Reproduced with the mock adapter and no terminal, and once with real sessions (`.plans/yunta-tui/proof/parked-answers/`, which stays out of git):

- **A real run meets the cap as a cut session, with a retry that cannot help.** Under `limits.max_tokens_per_run: 2000000`, the example in `docs/reference.md`, a real fragua run stopped after 28 s and 286k tokens with ``node `grill` failed: exceeded max_tokens 200000 (285734 used)``. A session's share is the cap over the nodes still owed, and it counts cached tokens. The only offer is `retry` "once something outside the tree changes", whose next attempt gets the same share, so the way to `continue` is spending the whole cap retry by retry. A terminal offers the same.
- **With a node failed and the cap spent, `retry` does nothing.** `resolve-gate retry` records an answer, the next engine stops on the cap before the node starts, and the attempt number does not move.
- **The run's own stop has no way out.** A run stopped on `limits.max_tokens_per_run` says "no option answers this pause here". `yunta resolve-gate <run> continue` refuses it, and `yunta resume <run>` parks again at once. The node left never starts. `resume_run` over MCP starts that same resume, so an agent gets the same dead end.
- **The iteration cap already has a way out, and its page says otherwise.** A loop stopped on `limits.max_loop_iterations` offers `retry`, and `yunta resolve-gate <run> retry` runs it with its count started over: five chained tasks under a cap of 2 went 2 of 5, 4 of 5, finished, with one `retry` each round and nothing lifted. The page says "once a person lifts the cap on resume", "until a person lifts the cap on resume, it fails the same way" and "resume with an interactive surface to continue past the cap or abort". None of the three is true.
- **An answer can strand a run, since `run-guard`.** Neither case exists on `main`; both come from the commit before this module, and its tests missed them.
  - A gate answered while the engine that parked the run is still leaving: the resume `resolve-gate` starts is refused with `run TAQ9Q3 is already moving`, because `tail_paused` is false once an answer follows the pause, so it never waits. That refusal reaches only `detached.log`. `resolve-gate` has said the answer is resolved, and the run reads `running` with no engine behind it until someone runs `yunta resume`. The window is the moments that engine takes to leave, 6 and 16 ms when measured.
  - A gate answered beside a lock nobody can read: the answer is recorded, the resume refuses in `detached.log`, and the run reads `running` the same way.
- **A pause inside a child is answered on the child.** The parent's page names the child's id and offers no command to reach it. Answered there, the child ends in a resume of its own, and its parent stays parked saying the child is parked until someone resumes the parent. This holds for every pause of a child, not only for the cap.

### Who this is for

- **A person at another terminal or with no terminal**, whose detached run stopped on the cap.
- **An agent over MCP**, which reads `workflow_status` and answers with `resolve_gate`.
- **The application in `tui`**, which offers the same decision on a key. That is why this comes before it.

### What each of them gets

1. As anyone reading a run stopped on the cap, I see what it spent, the two options and what each does, on the same surfaces that show any other decision.
2. As anyone whose session was cut at its share, one answer sends it on past the cap, instead of a retry that fails the same way until the cap is spent.
3. As anyone answering `continue`, the run goes on past the cap in an engine of its own, and I do not have to be attached to it.
4. As anyone answering `abort`, the run stays where it is and the decision stays open.
5. As whoever pays, one answer pays for one engine. An engine that took it and died leaves nothing authorized.
6. As a person at a terminal, a resume with no answer waiting asks me as it does today, and one with an answer waiting takes it without asking again.
7. As a person reading a parent whose child stopped on the cap, the page gives me the command that reaches the child.
8. As a person reading a loop stopped on its cap, what the page says is what `retry` does.
9. As anyone answering any gate, my answer reaches an engine: the resume waits for the engine that is leaving, and what no waiting can clear refuses me before anything is recorded.

### What it says

A run stopped on the cap. The sentence loses its last clause, which stops being true, and the block takes the place of "no option answers this pause here":

```
◆ over-budget (default) · needs you · for 0s                              RHKMPY
  its token budget is spent (100 tokens with `limits.max_tokens_per_run: 100`).

  ◆ decision on the run
  ▎ continue: Continue past the cap
  ▎   Lifts the cap for this invocation only; a later resume will ask again
  ▎   before spending more
  ▎   yunta resolve-gate RHKMPY continue
  ▎ abort: Pause here
  ▎   nothing runs; this decision stays open
  ▎   yunta resolve-gate RHKMPY abort
```

The label and the tradeoff of `continue` are the ones a terminal shows at the base commit.

A session cut at its share. `continue` takes the place of `retry`, on every interface, a terminal included:

```
◆ fragua (quick) · needs you · for 0s                                     4GGEMA
  node `grill` failed: exceeded max_tokens 200000 (285734 used).

  ◆ decision on node `grill`
  ▎ continue: Run it again past the cap (attempt 2)
  ▎   Lifts the cap for this invocation only; a later resume will ask again
  ▎   before spending more
  ▎   yunta resolve-gate 4GGEMA continue
  ▎ abort: Pause here
  ▎   nothing runs; this decision stays open
  ▎   yunta resolve-gate 4GGEMA abort
```

| Case | What it says |
|---|---|
| `resolve-gate <run> continue` | `✓ run RHKMPY · resolved 'continue'` and `a resume in the background applies it.`, as any gate |
| An option off the menu, `retry` on a cut session included | `option 'x' isn't valid here; declared options: continue, abort`, as any gate |
| A node with its `retry` answered, stopped on the cap | The page shows the run's decision, and the node's answer stands: after `continue`, that attempt runs |
| The parent of a child stopped on the cap | Its sentence stays, and `next` gains `yunta status <child>` with "shows what it is waiting on" |
| An answer beside a lock nobody can check | The sentence `run-guard` writes for that lock, exit 1, and the log gains nothing |
| A loop stopped on its cap | ``loop `implement` exceeded `limits.max_loop_iterations` (2)``, with no last clause |
| The `retry` of that loop | `Run it again (attempt 2): its iteration count starts over`, and its tradeoff ends saying it stops again at the cap if tasks remain |

Under `--json`, `decision` is present for the run's stop, with `budget`, `evidence`, `options`, `resolve_with` and `commands`, and with no `node`: the decision is the run's. `waiting_on` stays as it is. For a cut session, `decision.failure.remedies` lists `continue`, with a `null` precondition, where it listed `retry`. Because a reader could take `node` as always present, `schema_version` moves from 8 to 9 and `docs/compatibility.md` says so.

Over MCP, `workflow_status` carries that same document and `resolve_gate` takes the same options. The commands in a document a program reads name the run whole.

### Decisions the plan starts from

- **Every interface behaves the same.** No option depends on who asks: the CLI, MCP and later the application reach one engine function.
- **One path for an answer.** `resolve_gate` records the pair a terminal records for this decision at the base commit, `gate_waiting` and `gate_resolved` with no node, and the caller hands the run to a detached resume. The engine never spends from inside the command that answers.
- **The menu is rebuilt from the pause.** `run_paused` already carries `BudgetExhausted { spent, cap }`, which is all the menu needs, so no process has to remember anything.
- **A session cut at its share offers `continue` and not `retry`.** The next attempt gets the same share, so a plain retry fails the same way, and nothing retries unless an input changed. `continue` lifts the cap for the engine that applies it and runs the node again, whether a person answers at its terminal or from elsewhere.
- **A stop on the cap does not use up a node's answer.** At the base commit any `run_paused` closes it. Here a `retry` that reached the cap would be lost, the run's `continue` would then wake an engine that parks on the failed node without spending, and the two answers would alternate forever. While a run stands on the cap, the run's decision is the one on offer, and the node's answer waits for it.
- **The window of this answer is narrower than a node's.** Repeating a node's `retry` after a crash is harmless, so that answer stands until a node event closes it. Spending is not harmless, so this answer is spent as soon as an engine takes it. It counts only when it stands right after the `run_paused` of the cap with no `run_resumed` after it, which also keeps a `continue` given earlier at a terminal from authorizing a later engine. The lift itself lives in the engine that took the answer, so an attempt that engine started and did not finish restarts in the next engine under its share, and asks again if it is cut. An engine that dies before it starts the attempt has spent nothing, and the answer still stands.

- **A run is parked until a resume says otherwise.** `tail_paused` asks whether the last event was the pause, which an answer makes false. Who waits for a leaving engine reads the run's pause instead, which only `run_resumed` ends.
- **An answer that cannot reach an engine is not recorded.** Whatever records an answer and starts a resume asks the same question `resume_run` asks first, because the resume's own refusal reaches nobody.

### Assumptions

1. Whoever answers, the behavior is the same. An agent over MCP can authorize this spend as it can answer any other gate: it can already start runs that spend. If it passes no `by`, the log records `unverified:<user>`, as for a person.
2. `continue` covers the whole next engine with no new cap, as at a terminal today. `yunta cancel` stops it.
3. A child is answered on the child, by its id. A `continue` there does not lift the parent's own cap, so the parent can stop on its own cap once the child's spend reaches it.
4. The menu is the one a terminal shows: `continue` and `abort`, with the same consequences. `abort` leaves the run parked on the same reason.
5. It shows on every surface that shows a decision: `status`, `status --json`, `list`, `resolve-gate` with and without an option, `yunta open`, the block a run leaves when it stops, and `workflow_status`.
6. No new event kind, no new field in the log and no change to the frozen manifest. The cap stays what it was.
7. The loop cap gets no new `continue`, because `retry` already does it. Its false sentences are corrected here, since this module rewrites the sentence they share. `cap_lifted` stays readable in logs that wrote it.
8. A real run proves it: a fragua run under the `2000000` of the reference, about 3.4M tokens in all, is cut at `grill`, goes on with `resolve-gate continue` and reaches `ship`. Nothing is published. The run measured for `run-guard`, on this same code, is the comparison, so the base is not run again.
9. `tasks/plan.md` and `tasks/todo.md` are replaced for this module. Those of `run-guard` stay in its commit.
10. No new dependency, and no time added to a run that is not stopped.
11. A run parked on a cut session when this binary arrives gets the new menu, and an older `retry` still waiting on its log asks again, which is what a menu that no longer offers an answer already does. A `retry` in a log an older binary wrote still reads.

### Out of scope

- Answering a child from its parent, or a parent that goes on by itself once its child ends. Both change how any pause of a child is taken up. The second is recorded in `.agents/open.md`.
- Raising a frozen cap.
- How the cap is shared among sessions, and whether cached tokens count against it. No real fragua node fits a share of the example in `docs/reference.md`. Recorded in `.agents/open.md`.
- The other pauses with no menu: a scope in ask mode, an unanswered questions document and a forge that does not answer.
- The race between an answer and a resume, recorded in `.agents/open.md` by `run-guard`.
- `yunta test`, settled at approval: see the end of this file.

## Tech Stack

Rust at the version `rust-toolchain.toml` pins, tokio, and the existing event log and escalation types. No new dependency.

## Commands

The work happens in the worktree `/Users/eulke/development/yunta-tui`, on the branch `feature/yunta-tui`.

```sh
# Before editing a file, and before finishing with no paths
cargo xtask docs --about <files>

# Build
cargo build --workspace --locked
cargo build --release --locked -p yunta

# Test
cargo test -p yunta-core --test events
cargo test -p yunta-render
cargo test -p yunta-engine --test parked_answers
cargo test -p yunta-engine --test run_gates_limits
cargo test -p yunta --test parked_runs
cargo test -p yunta --test mcp_flow
cargo test -p yunta --test surface_parity
cargo test --workspace --locked --no-fail-fast

# Output meant to change, then read the goldens' diff
YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine

# Lint and checks
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo xtask smells --check
cargo xtask docs --check
cargo xtask schema --check
```

Before a commit, every step of the `ci` job in `.github/workflows/ci.yml` runs, one build at a time, with `CARGO_INCREMENTAL=0`. After it, `cargo clean`. `cargo xtask smells` never runs without `--check`.

## Project Structure

```
crates/core/src/events/failure/remedy.rs   a cut session offers continue; a loop's retry waits for nothing
crates/core/schemas/events.json            two descriptions: a capped loop's retry names no input
crates/core/src/events/gates/ledger.rs     the answer with no node, and where it stands
crates/core/src/events/run/ledger.rs       the last pause that closes a node's answer
crates/core/src/reserved.rs                continue runs a node again
crates/engine/src/replay.rs                the run's id, from the log it folds
crates/engine/src/replay/decisions.rs      the answer that waits, with no node
crates/engine/src/run/escalation.rs        the run's own menu, rebuilt and answered
crates/engine/src/run/budget.rs, steps.rs  the next engine takes the answer
crates/engine/src/run/exec.rs              what that engine reads before it says it resumed
crates/engine/src/run_hold.rs              a parked run waits, also with an answer on its log
crates/cli/src/commands/resume.rs          the same reading in `refuse_if_driven`
crates/engine/tests/parked_answers.rs      NEW: the answer, at the engine's level
crates/render/src/pause.rs                 the sentences of both caps
crates/render/src/blocks/failure.rs        the words of a loop's retry, and of continue on a cut session
crates/render/src/offers.rs                what lifting the cap costs, said in one place
crates/cli/src/commands/status/decision.rs a decision with no node on a page
crates/cli/src/commands/resolve_gate.rs    the menu at a terminal, and the answer, refused first
crates/cli/src/commands/mcp/tools.rs       `answer_questions`, refused first too
crates/cli/src/commands/advice.rs          the command that reaches a child
crates/cli/src/run_page/, surface/, json/  the block, and the document at version 9
crates/cli/src/error/words.rs              the refusal stops naming a budget cap
crates/cli/tests/parked_runs.rs, parked_runs/   the black box
crates/cli/tests/mcp_flow.rs, mcp_flow/, surface_parity.rs  the same on every interface
docs/compatibility.md, docs/troubleshooting.md, README.md   what whoever uses Yunta relies on
.agents/architecture.md                    one answer pays for one engine
.agents/open.md                            the parent left parked, and how the cap is shared
CAPABILITY-MAP.md                          corrected: a loop's cap is not raised
SPEC-parked-answers.md                     this file
tasks/plan.md, tasks/todo.md               this module's plan and task list
```

## Code Style

The shape, for illustration. The plan decides the names.

```rust
/// Whom a parked run's decision belongs to. Not always a node: the token cap
/// gates the whole invocation, and a menu that had to name a node would lie.
pub enum DecisionOn {
    Node(NodeId),
    Run,
}

impl RunState {
    /// The answer to the token cap the next engine takes: it counts only where it
    /// stands right after the cap's pause, because every engine spends new money.
    /// Not [`RunState::pre_seeded`], whose answer outlives a crash, which is
    /// harmless for a retry and not for a spend.
    pub fn cap_answer(&self) -> Option<&HumanChoice> { /* ... */ }
}
```

The repository's rules hold:

- A printed sentence carries no dash: a colon explains, a semicolon comes before the hint it ends with.
- A command a person is told to type is spelled in `commands/advice`.
- Types that admit only valid states before branches.
- Code, tests, commits and pull requests cite no document.
- English, in the present tense.

## Testing Strategy

`cargo test`, with integration tests under each crate's `tests/` and the helpers of `yunta_testkit`.

| Level | What it proves |
|---|---|
| CLI, black box | Off a terminal, a run stopped on the cap shows both options; `resolve-gate continue` exits 0, the node left starts, the run finishes, and the log gains one pair with no node and its `by` |
| CLI, black box | Sessions past their share, as a real session goes: the menu offers `continue` and `abort`, `retry` is refused, and `continue` runs the node past its share and the run goes on. This fails first, on an option off the menu |
| CLI, black box | A node failed on anything else with the cap spent: `retry` stops on the cap, the page shows the run's decision, and `continue` runs that attempt once. Two answers, and the run moves on |
| CLI, black box | `resolve-gate abort` leaves the run parked on the cap, with the decision open and no node started |
| CLI, black box | An engine killed with `kill -9` after taking the answer leaves nothing authorized: the next resume stops on the cap again, and a cut session's attempt restarts under its share |
| CLI, black box | A second answer before the resume takes the first is refused, and the log gains no second pair |
| CLI, black box | A gate answered while a live holder is on its way out: the resume waits, the holder ends, and the run goes on. This fails first, with the run left reading `running` |
| CLI, black box | A gate answered beside an unreadable lock exits 1, names the file, and the log gains no pair |
| CLI, black box | The parent of a child stopped on the cap offers `yunta status <child>`; `resolve-gate <child> continue` ends the child, and `yunta resume <parent>` takes it from there |
| CLI, black box | `status --json` carries `decision` with `budget`, no `node` and `schema_version: 9`; the commands in it name the run whole |
| MCP | `workflow_status` shows the decision and `resolve_gate` with `continue` gives the same result as the CLI, by a real client over stdio |
| MCP | `resolve_gate` and `answer_questions` beside an unreadable lock return the same refusal, and record nothing |
| Parity | The options, the evidence and the commands are the same on the page, in the document and over MCP |
| Engine | With an answer waiting, an engine takes it and does not ask its surface; with none, it asks and parks as at the base commit. An answer is taken once. A `continue` given earlier at a terminal authorizes no later engine |
| Unit | The sentences of both caps and the words of a loop's `retry`, written by hand. A log holding `cap_lifted` still reads |
| Regression | `budget_authorization_is_per_invocation_a_resume_asks_again`, `run_gates_limits`, `workflow_compose`, `failed_node_retry` and `run_flow` stay green, the first with no change at all |

Rules for these tests:

- Expected values are known apart from the code.
- A test whose subject can spend for no reason asserts what did not start, not only what did.
- `a_pause_with_no_menu_still_says_what_the_run_is_waiting_on` uses the token cap as its example, which stops being one. It moves to another pause with no menu, and keeps its promise.
- `a_loop_past_its_cap_waits_for_a_person_to_lift_the_cap`, `a_retry_that_waits_for_a_capped_loop_says_a_person_lifts_the_cap` and the outlook of `TokenBudget` in `static_outlook.rs` assert what this module corrects, and change with it.
- `schema_version` is written as 8 in `parked_runs/menu.rs` and `status_nodes_cmd.rs`, and moves to 9.
- On a moving run, `resolve-gate` now says who drives it. No test holds the old words for that case: `answering.rs` holds them for a finished run, which keeps them.
- A decision that can belong to the run changes a type a few engine tests read, in `failed_node_retry.rs`, `project_commands.rs`, `deviation/surfaces.rs` and `common/menu.rs`. Those edits change a type and no promise.
- `budget_authorization_is_per_invocation_a_resume_asks_again` keeps its assertions. Its comment says nothing in the log authorizes spend, which stops being true, and that clause changes.
- Goldens change only for the sentences above, and their diff is read before it is kept.

Proof on a release build, with `.plans/yunta-tui/proof/parked-answers/repro.sh`, `probe_share.sh`, `probe_loop.sh` and `probe_handoff.sh`, whose results at the base commit are kept beside them, and with the real run of assumption 8.

## Boundaries

**Always**
- Work in this worktree, and stage paths by name.
- Reproduce before changing, and measure the real run after.
- Keep one path for an answer, for every interface.

**Ask first**
- Add a dependency, change `deny.toml` or touch `.github/workflows/ci.yml`.
- Add or change an event of the log. This spec needs none.
- Change an assertion of `budget_authorization_is_per_invocation_a_resume_asks_again`. If one has to change, the work stops there.
- Change a test that passed at the base commit, other than those this spec names and the goldens of the sentences above.
- Change how a child's pause is taken up.

**Never**
- Let an answer authorize more than one engine.
- Give one interface an option another does not have.
- Spend from inside the command that records the answer.
- Run `cargo xtask smells` without `--check`.

## Success Criteria

1. Off a terminal, a run stopped on the cap shows its spend, `continue` and `abort`, and the command for each, on `status`, `status --json`, `list`, `resolve-gate` with no option, `yunta open` and the block a run leaves when it stops.
2. `yunta resolve-gate <run> continue` exits 0, and the run goes past the cap in a detached engine: the node left starts, and the run ends.
3. A session cut at its share offers `continue` and `abort` on every interface, a terminal included, and refuses `retry`. `continue` runs the node again past its share, and the run goes on.
4. A node with its `retry` answered and the cap spent moves on with one `continue`. The answers do not alternate, and the attempt runs once.
5. `resolve_gate` over MCP gives the results of criteria 2 and 3, and `workflow_status` shows what `status --json` shows.
6. `abort` leaves the run where it stopped with the decision open, and no node starts.
7. One answer pays for one engine. An engine that took it and then parked, or died with `kill -9` after it started spending, leaves the next engine stopped on the cap or under its share. `budget_authorization_is_per_invocation_a_resume_asks_again` keeps its assertions.
8. At a terminal, a resume with no answer waiting asks as at the base commit, and one with an answer waiting takes it without asking.
9. The parent of a child stopped on the cap offers the command that reaches the child, and the child can be answered there.
10. An answer always reaches an engine. A gate answered while its engine is leaving goes on with no one resuming it by hand, and `probe_handoff.sh` shows it. An answer no waiting can help is refused before it is recorded, on the CLI and over MCP.
11. The page of a loop stopped on its cap says nothing false, and `probe_loop.sh` gives the same 2 of 5, 4 of 5, finished. Logs holding `cap_lifted`, or a `retry` on a cut session, still read.
12. `status --json` and every document that shares its stamp carry `schema_version: 9`, and `docs/compatibility.md` says what moved it.
13. A real fragua run under `limits.max_tokens_per_run: 2000000` is cut at `grill`, goes on with `resolve-gate continue` and reaches `ship`. From `gate_resolved` to the next `node_started` takes under 0.5 s, and the whole run's time outside nodes and pauses stays under 1 s (0.656 s on the `run-guard` run).
14. Every step of the `ci` job passes.

## Open Questions

None. Two were settled at approval:

- The hand-off is fixed in this module's commit, because every answer here goes through it and the pull request is an unmerged draft.
- `yunta test` stays out. A test case names its answers by node, so a case that reaches the cap keeps stopping there, as at the base commit: a case is a script of a run and not an interface for deciding, and adding a way to name the run's decision would change the format of test cases.
