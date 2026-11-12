# Open

Work Yunta has not done yet, and questions it has not answered. An entry is deleted by the change that does it.

### A red measurement can still stop a task's pre-check
Paths: `crates/engine/src/task_cycle/**`, `crates/engine/src/run/aside.rs`, `crates/engine/src/run/baseline.rs`, `crates/engine/tests/measure_beside_loop.rs`
A loop starts beside the lineage's measurement, and a task is held to the suite only when the suite passed before
the run changed anything. On macOS's CI runner, `a_red_measurement_holds_no_guard` once saw the task's pre-check
run a red suite (`sleep 2; false`) as a guard and stop the node with `BrokenGuard`, where the run should finish;
run again, it passed. The order between the pre-check and the measurement landing is a race the test does not
pin. Reproduce it under load, many runs beside busy loops, before deciding what the pre-check waits on.

### A skill a run revises is not the one its later sessions load
Paths: `crates/engine/src/frozen/**`, `crates/adapters/src/mount.rs`, `crates/engine/src/run/session_plan.rs`
A run freezes each skill when it is born and mounts that copy in every session, so a node that rewrites a skill's
source under `.yunta/skills/` changes the next run, not the rest of this one. Using the revision within the run
needs the engine to own it: a revision recorded in the log, frozen and mounted from there, with each adapter
proven to load a mount that changed between sessions.

### A test case asserts where a run ends, never the path it took
Paths: `crates/api/src/cases.rs`, `crates/api/src/cases/**`
A case's `expect` names the final state and the state of each node and task. It cannot say how many times a node
ran or was rerouted, that the log holds a kind so many times, or that something never happened — a lint fixed
in two laps and one fixed in four pass alike. The shape to add: `nodes: {<id>: {runs, reroutes}}` beside the
plain state, `events: [{<kind>: {count}}]` and `never: [{<kind>: {<field>: <value>}}]`, each read off the
replayed log the case already derives.

### A Codex session meets the run's token cap only when it ends
Paths: `crates/adapters/src/codex/mod.rs`, `crates/adapters/src/codex/parse.rs`
Codex reports a turn's usage on `turn.completed` alone, so its adapter declares no `mid_session_token_cap`: the
run's token cap ends a Codex session only once it completes, where a Claude Code session is cut as it passes the
cap. Every adapter offers the same features, so this is a gap to close, which needs a usage reading Codex gives
while its turn runs.

### A composed run's stack grows with every level of composition
Paths: `crates/engine/src/run/workflow_exec/**`, `crates/testkit/src/stack.rs`
A run that composes awaits its child's run inside its own, so each level nests in the one above and the stack a
composed run takes grows with its depth; `limits.max_workflow_depth` defaults to 4, and the tests of deep
composition run on a deep stack. Not decided: whether the engine bounds its recursion (boxing each level's await,
or a limit tied to the stack rather than the depth) or the default depth is the guarantee.

### Writes with no engine race a resume
Paths: `crates/engine/src/run/escalation.rs`, `crates/engine/src/answers.rs`, `crates/engine/src/run/close.rs`,
`crates/api/src/ending.rs`
Four commands append to a run's log without its `engine.lock`: `resolve_gate`, `answer_questions`, `close` and the
closing step of `cancel`. Each first reads that the run stopped, and a `resume` that starts right after that read
can write beside them. The window is narrow and SQLite serializes the rows, so nothing has come of it yet. Closing
it means each of them takes a `RunHold` for its write.

### A parent stays parked after its child ends
Paths: `crates/engine/src/run/workflow_exec/**`, `crates/engine/src/run/escalation.rs`
A child answered on its own log ends in an engine of its own, and its parent stays parked on the child's pause
until someone runs `yunta resume <parent>`. The parent's page offers the way to the child and that resume, and
nothing hands the parent back. Going on by itself changes how any pause of a child is taken up, which is why it
waits: who resumes the parent, and what an answer does when the parent has been closed.

### How the token cap is shared among sessions
Paths: `crates/engine/src/run/budget.rs`, `docs/reference.md`
A session may spend the cap over the nodes still owed, and cached tokens count against it. No real fragua node
fits a share of the `2000000` that `docs/reference.md` gives as its example: `grill` was cut at 285734 of 200000
after 28 s, so such a run stops on a cut session long before it has spent the cap, and goes on only past the cap.
Three questions stand: whether a share should be equal across nodes, whether cached tokens should count, and what
that example should be.

### A stub can wait on a child that never reaches its rendezvous
Paths: `crates/cli/tests/session_ends.rs`, `crates/testkit-core/stubs/**`
The test `cancel_after_its_engine_died_leaves_no_command`, in `crates/cli/tests/session_ends.rs`, failed once in
four runs of the CLI's fifth batch: the run's Claude Code stub started its children and waited on one that never
wrote its pid within 30 s. The test alone, its target beside a build or from four workers at once, the target right
after a relink, and the batch itself did not show it again in 128 runs and ten batches. The order of the fifos, the
engine's probe, a sibling's sweep and a slow `python3` are ruled out. The test now names the command it waited on,
whether the stub started it, and what the run left (`detached.log`, `engine.json`, `yunta status`), so the next miss
names its cause.

### The binary carries what the interface's performer drives
Paths: `crates/ui/src/performer.rs`, `crates/ui/src/performer/**`, `crates/api/src/**`, `crates/cli/src/application.rs`
Bare `yunta` links `yunta-ui`'s performer, which drives every API operation, and the release grew 2.1 MB with it,
of which the application's own crates are about 90 KB. The growth is the performer's own work, the engine's and
the API's async paths and the readers of tasks, questions and specs through `serde_path_to_error`. Not known: how
much of it is a second copy of what the CLI's commands already instantiate. A symbol by symbol diff of the two
builds answers that before any change.
