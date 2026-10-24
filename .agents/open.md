# Open

Work Yunta has not done yet, and questions it has not answered. An entry is deleted by the change that does it.

### A skill a run revises is not the one its later sessions load
Paths: `crates/engine/src/frozen/**`, `crates/adapters/src/mount.rs`, `crates/engine/src/run/session_plan.rs`
A run freezes each skill when it is born and mounts that copy in every session, so a node that rewrites a skill's
source under `.yunta/skills/` changes the next run, not the rest of this one. Using the revision within the run
needs the engine to own it: a revision recorded in the log, frozen and mounted from there, with each adapter
proven to load a mount that changed between sessions.

### A test case asserts where a run ends, never the path it took
Paths: `crates/cli/src/commands/test/**`
A case's `expect` names the final state and the state of each node and task. It cannot say how many times a node
ran or was re-routed, that the log holds a kind so many times, or that something never happened — a lint fixed
in two laps and one fixed in four pass alike. The shape to add: `nodes: {<id>: {runs, reroutes}}` beside the
plain state, `events: [{<kind>: {count}}]` and `never: [{<kind>: {<field>: <value>}}]`, each read off the
replayed log the case already derives.

### A Codex session meets the run's token cap only when it ends
Paths: `crates/adapters/src/codex/mod.rs`, `crates/adapters/src/codex/parse.rs`
Codex reports a turn's usage on `turn.completed` alone, so its adapter declares no `mid_session_token_cap`: the
run's token cap ends a Codex session only once it completes, where a Claude Code session is cut as it passes the
cap. Every adapter offers the same features, so this is a gap to close, which needs a usage reading Codex gives
while its turn runs.
