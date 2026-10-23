# Open

Work Yunta has not done yet, and questions it has not answered. An entry is deleted by the change that does it.

### A skill a run revises is not the one its later sessions load
Paths: `crates/engine/src/frozen/**`, `crates/adapters/src/mount.rs`, `crates/engine/src/run/session_plan.rs`
A run freezes each skill when it is born and mounts that copy in every session, so a node that rewrites a skill's
source under `.yunta/skills/` changes the next run, not the rest of this one. Using the revision within the run
needs the engine to own it: a revision recorded in the log, frozen and mounted from there, with each adapter
proven to load a mount that changed between sessions.
