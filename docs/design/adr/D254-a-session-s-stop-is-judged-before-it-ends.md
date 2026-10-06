---
number: D254
title: "A session's stop is judged before it ends, once for one state of its work"
status: accepted
revises: []
revised_by: []
---

# D254 — A session's stop is judged before it ends, once for one state of its work

## Context

A session that ended with its work unfinished — a criterion red, a path
outside its scope, a document never handed over — was judged only by its
attempt's close. The close failed the attempt, and the work went to a new
session with none of the context the first one had, or stopped the run for
a person to choose `continue-work`. In the real runs this happened in most
runs, and often for something the same session could have fixed in a turn.

Both CLIs run a `Stop` hook headless before a session ends its turn, and
both read `{"decision":"block","reason":…}` on stdout as "go on, and here
is why"; `claude` caps how often in a row unless
`CLAUDE_CODE_STOP_HOOK_BLOCK_CAP=0`.

## Decision

1. **The rail asks before a stop.** Both adapters install a `Stop` hook
   that runs `yunta rail <adapter> before-stop`; it waits for the engine as
   long as a call that runs the task's checks may take, and a hook that
   cannot ask lets the session stop.
2. **The engine judges the work as the close will.** A task session's work
   is checked as `yunta_check_task` checks it; a node's own session's, by
   the documents it hands over and its scope. When the close would not take
   it, the session is sent back with what stands in the way, and
   `stop_refused` names the state and why before the session hears it.
3. **Once for one state.** A session that stops again with its work
   unchanged is let go, and its close judges it. The state is the tree,
   and for a node's session the verdicts on its documents.
4. **Never when the attempt's end owes the session an answer** — a scope
   request it made, or a refused write a red criterion locates — and never
   once the session is being cut, or when a criterion could not run at all.

## Rejected alternatives

**Judge in the close only.** It hands fixable work to a new session, or to
a person, and repeats the build that new session needs.

**Send a stop back until the work closes.** A session that cannot do
better would be held until its budget ran out.
