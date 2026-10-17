---
number: D193
title: "What a run's commands print belongs to the run, never to the terminal"
status: accepted
revises: []
revised_by: []
---

# D193 — What a run's commands print belongs to the run, never to the terminal

## Context

A run of a workflow whose criteria are `cargo` commands showed `Compiling…`
and the test harness's output interleaved with the live view. Three kinds of
command inherited the engine's streams: a task's criteria, in the pre-check
and the post-check (including the ones `yunta_check_task` runs and the suite
a `baseline_compare` runs through the same cache); a node's hooks; and the
pre-check of the criterion a scope request proposes. Everything else the
engine runs already collected its output: `bash` nodes, `command:` sources,
`git`, the baseline measurement, executors and the agents' CLIs.

What those commands printed was also lost. `criteria_checked` recorded exit
codes only, so a person deciding about a blocked task read `cargo test still
exits 101` and nothing about why, and the session that called
`yunta_check_task` got the same exit code while the output went to the
engine's terminal instead of to the agent that asked.

## Decision

1. **No command the engine runs writes to the terminal.** A child's stream is
   collected or discarded; there is no way to hand it the engine's own. The
   terminal belongs to the run's display.
2. **What a criterion or a hook prints is kept in the run's object store**,
   redacted with the same secrets the log redacts, and its event names the
   object (`output`). The criteria cache keeps what a red answer printed,
   so a check that reuses it still says why: a session's
   `yunta_check_task` and the close that follows it on the same tree are
   one run of the command. A green answer reused names nothing.
3. **The event carries the tail of a command that failed**: the last 20
   lines (`tail`). A command that passed carries none.
4. **Every reader of the log gets the reason with the verdict.** The
   chronicle names each red criterion with its last line. A blocked task's
   cause quotes the last line each red criterion printed, and that cause is
   what the node's failure, the decision and `yunta status` read. A node
   whose hook failed, and a `baseline_compare` that finds a regression,
   quote the command's tail, as a `bash` node's failure already does.
5. **The session's tools answer with the tail**: `yunta_task` for every check
   the log holds, `yunta_check_task` for the check it just ran.
6. **What only an exit code is recorded for is discarded**: the pre-check of a
   proposed criterion.
7. **The live view says what a quiet task is doing.** A running task no
   session has opened for since it started running is shown as checking its
   criteria.

## Rationale

A command's output is evidence of the verdict its exit code states. The
terminal was the one place it went, and the one place nobody can read after
the fact: not the person deciding hours later, not the session that asked,
not a second process reading the log. The tail on the log reaches every
reader the verdict already reaches; the whole output in `objects/` keeps the
log bounded while nothing is thrown away.

## Rejected alternatives

**Streaming the output live under a flag.** It brings the interleaving back
for whoever turns it on, and what it streams is still not kept. What the run
keeps can be read at any time.

**The whole output on the event.** A build log of a thousand lines would
reach every surface that quotes the log, and the receipt. The last lines are
the ones that say why, which is the rule `bash` nodes already follow.

**Files next to the node, like `node-output`.** A second store for content the
run keeps by hash. `objects/` is the run's one store, and what a context
source resolved already lives there.
