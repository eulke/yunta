---
number: D231
title: "A run's units take their checkouts from a pool of its own, and keep what git ignores"
status: accepted
revises: [D65]
revised_by: []
---

# D231 — A run's units take their checkouts from a pool of its own, and keep what git ignores

## Context

D65 gave every task of a batch a worktree of its own, cut from the base commit,
and every attempt of every unit got another. A loop of ten tasks made ten
checkouts and left them all behind. A project whose criteria build something —
a test suite that compiles first — built it from nothing in every one of them,
because a new checkout has none of the output an earlier one produced. The
build, not the work, was most of what each task's checks cost.

## Decision

1. **A pool per run.** A run's units take their checkouts from a pool kept
   under the run's directory: as many checkouts as the run ever has units at
   work at once — a loop's concurrency, and the nodes that work in a tree of
   their own beside each other.
2. **A unit takes a free checkout.** It is put back to the commit the unit
   starts from, on a branch of the unit's own; what is not ignored is cleaned
   away, and what git ignores stays. With none free, the pool adds one.
3. **Free means nobody's.** A checkout is free when no unit holds it, its
   commits are all in the commit the next unit starts from — landed, or never
   made — and nothing in it is uncommitted beyond the request a session writes
   for the engine. A blocked task's committed work, or a node's uncommitted work
   a session may continue on, keeps its checkout out of the pool.
4. **Found by branch.** A unit picked back up is found by the branch it worked
   on, wherever its checkout is.
5. **Probes take one too.** A document's hand-over proves its commands in a
   checkout of the pool on the run's tree with no branch, and gives it back
   clean.
6. **The run's close takes them away.** A run that finishes asking to be cleaned
   up removes its checkouts and every unit branch whose work its tree holds;
   `yunta gc` takes the rest when it collects the run.

## Rationale

Isolation while working is what D65 needed, and it holds: two units at work at
once never share a checkout. What it did not need was a new checkout for every
unit — that only threw away the build each one paid for. The free rule makes
reuse safe without tracking owners: a checkout holding anything that is still
somebody's is simply never chosen.

## Rejected alternatives

**A shared build directory across checkouts.** Works for some toolchains through
an environment variable, but it is the project's build configuration, not the
engine's, and concurrent builds into one directory contend.

**Keep a checkout per task and copy the build into the next.** Copies gigabytes
to save what reusing the directory saves for free.
