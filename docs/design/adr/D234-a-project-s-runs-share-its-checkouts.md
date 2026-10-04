---
number: D234
title: "A project's runs share its checkouts, and a build stays at the path that made it"
status: accepted
revises: [D231, D160]
revised_by: []
---

# D234 — A project's runs share its checkouts, and a build stays at the path that made it

## Context

D231 kept a pool of checkouts per run, so a run's units stopped building from
nothing in every checkout. Each new run still did: its pool started empty, and
its first build in every checkout was cold. A project worked around that by
pointing every checkout at one build directory through `shared_dirs`. That broke
the isolation the engine's judgement rests on. A build tool names a workspace
crate's output the same way in any checkout and judges it fresh by time, so a
task compiled against another task's code — its own sources consistent, the
error from the other checkout — and the criteria memo kept that red answer for
its tree. Output also names the directory that built it (`CARGO_MANIFEST_DIR`,
`CARGO_BIN_EXE_*`): a build copied into another checkout reads as fresh and runs
the first checkout's paths. And concurrent builds into one directory wait for
each other.

A build is right only at the path that made it. Warmth has to come from using a
path again, never from sharing or copying what a path built.

## Decision

1. **The pool is the project's.** Every checkout a run's units, probes and
   measurement work in comes from one pool per repository, under the worktrees
   root, and outlives the run. A checkout is handed out again where it is, put
   back to the commit its next user starts from; what git ignores stays.
2. **Free is clean, not held, and nobody's work.** A checkout is nobody's when
   it is on no branch, or its branch's work is in the commit its next user
   starts from. A unit's attempts take back the checkout one of them worked in
   first. Work that has not landed keeps its checkout its unit's, and anything
   uncommitted keeps it somebody's. A checkout whose work landed keeps its
   branch until somebody takes it, so a node a person sends back to its session
   picks it up where it was.
3. **Holding is a lock; owning is a branch.** A process holds a checkout with a
   lock file beside it, taken with the engine's lock protocol, so two processes
   — two runs — never share one. A gone holder's lock is taken back, unless the
   run that held it still has agents at work. A checkout a crash left half added
   is made again. Owning lives in the branch, which outlives every process.
4. **Nearest first.** A unit with no checkout of its own takes the free one whose
   commit differs from where it starts in the fewest paths.
5. **A landed task lets go of its branch.** When a task's work lands on the run's
   tree, its checkout goes back on no branch and the branch is deleted: the run's
   tree holds the work, and a done task is never picked back up. A node keeps its
   branch until its run ends, since a person may send it back to its session. At
   rest, a repository's branches are the runs' own and the work of blocked units
   of runs still open.
6. **A run's end gives its units' checkouts back,** whatever the end: each lets
   go of its unit's branch, and a branch whose work landed goes with it. A
   blocked unit's branch stays until `gc` collects the run.
7. **The pool keeps what it uses.** `gc` keeps as many free checkouts as were busy
   at once in the last 30 days, the most recently used, and takes the rest away.

## Rationale

The same checkout used again is a developer's checkout across branches: git
rewrites the files that differ, the build tool rebuilds what they touch, and
nothing else. Separate paths keep every build right and let concurrent builds
run side by side. The cost is a cold build the first time a checkout is made, and
disk for as many checkouts as the project's runs keep busy at once.

## Rejected alternatives

**A build directory shared across checkouts.** Contaminates builds, serializes
them, and lets a test run another checkout's code. Measured on a two-crate
workspace and seen in a real run.

**Copying a warm build into a new checkout.** The copy reads as fresh and keeps
the old checkout's paths compiled in.

**One checkout per run, its tasks one after another.** Always warm, but gives up
running independent tasks at once, which is worth more than any build.
