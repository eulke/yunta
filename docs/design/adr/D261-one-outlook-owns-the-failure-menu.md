---
number: D261
title: "One outlook owns the failure menu"
status: accepted
revises: [D188, D189, D191, D197, D242]
revised_by: []
---

# D261 — One outlook owns the failure menu

## Context

A failed node reached a person as a `retryable` flag, a headline and a detail.
The flag said whether another attempt might end differently, but not which
input the attempt waited for, so a menu could offer `retry` where nothing the
run could change would alter the outcome: a red check on a tree no attempt had
touched still offered a retry that would run the same command on the same
answer. A refused session showed only that its task had blocked, with neither
its message nor its cause, and the engine wrote a headline into the menu's
evidence, so the log held prose beside the facts it described.

## Decision

1. **One function says what can change a failure.** `Failure::remedies` reads the
   failure and the facts the log holds beside it: when the failure happened,
   whether the node's tasks hold work a continuation can pick up, whether the run
   may still grant the scope the failure asks for, and whether the run's tree moved
   since the node's last failed attempt. It returns one outlook: a wait until a
   reset, the options a person may choose from, or the reason no option helps.
   It reads nothing else, so a resumed run and a fresh one offer the same menu.

2. **A retry names its input.** Every `retry` carries the precondition it waits
   for: the run's tree, something outside it, or a named input such as a credential,
   a model, the service, a lifted limit, or a command that can run. A retry is
   offered only when that input may have changed since the failed attempt.

3. **A tree cause offers no retry until the tree moves.** A red check, an artifact
   or an unmet criterion offers no `retry` while the log has not seen the run's tree
   move since the failed attempt, and the menu says why. A person's
   change to a parked run's tree reaches the log through the wake that finds it: a
   resume that finds a failure waiting records the tree it finds in
   `run_resumed.tree`, and the menu then offers `retry`.

4. **The engine records ids and facts; render writes the words.** A failure's menu
   stores its remedies, the reason none is offered, and the failure itself as data.
   Option labels, trade-offs, and the words of each precondition are written by the
   render crate from that data, for every surface.

5. **The retryable flag leaves the node's state.** A node's failure no longer
   carries `retryable`. A log written with it reads the value and never acts on it.

## Rationale

A boolean says whether another attempt may end differently. A person needs to
know what has to change first, and the answer depends on the kind of failure, on
the tree, and on what the log already holds. Keeping that answer in one pure
function makes it testable from the log alone, lets every surface offer the same
options, and keeps prose out of the engine. Because the outlook is a function of
recorded facts, a replay of the log gives the menu the live run showed.

## Rejected alternatives

**Keep `retryable` and add a reason string.** A string says what happened but not
which input the retry waits for, so each surface would still have to guess which
retry to offer.

**Let each surface compute its own menu.** The surfaces would drift apart, and each
change to what a failure allows would have to land in every one of them.

**Offer `retry` after every failure and let the attempt find out.** A red check on
an unchanged tree would run the same command for the same answer, and a refused
credential would run again into the same refusal.
