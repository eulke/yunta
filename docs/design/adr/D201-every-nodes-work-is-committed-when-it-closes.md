---
number: D201
title: "Every node's work is committed when it closes"
status: accepted
revises: [D184]
revised_by: []
---

# D201 — Every node's work is committed when it closes

## Context

A node that declares `scope:` works in a checkout of its own and lands its work
as commits. A node that declares nothing works in the run's own tree and, until
now, left its work there uncommitted: "between two nodes of a run nobody
commits". Three things followed.

- Fragua's `fix-findings` has no scope. What it fixed never reached the branch
  the `pr` node pushes, and `cleanup: worktree` removed the worktree with it.
- A loop's task checkouts open from the run's `HEAD`, so a task never saw what
  a node before the loop left, while the baseline, the scope audit and every
  check of the tree did. "The run's tree" meant two things.
- A node that reached a task only through uncommitted leftovers could pass in
  its own checkout and be refused at integration, over and over.

## Decision

1. **A node that works in the run's own tree has its work committed on the
   run's branch when it closes**, finished or failed. A failed attempt's partial
   work is committed as that attempt's, so the next attempt or a corrective node
   starts from it and nothing later is blamed for it.
2. **The commit is the tree the log names.** The engine captures the tree —
   `HEAD` plus what lies in it uncommitted, less what git ignores — commits it on
   top of `HEAD` and moves the branch by compare-and-swap. No git hook runs: a
   hook that reformats or refuses could make the branch disagree with the log.
3. **Nodes working in the tree at once commit together.** Two of them cannot tell
   their work apart, so neither commits while another still works; the last to
   close commits, naming the others. A `parallel` group is running while its
   children are, so it commits what they left when it closes.
4. **A run with `isolation: none` commits nothing.** It works in a person's own
   checkout, and their branch is left as it was found.
5. **`node_finished` and `node_failed` name the commit they made**, when they made
   one.

The run's `HEAD` and its tree are then the same content, less what git ignores:
a task, the criteria cache, the handover probe, a successor run and the pull
request all read the same work.

## Rejected alternatives

**Committing the leftovers only when a loop starts.** It would make tasks see
them and leave `fix-findings`' work off the branch.

**Letting task checkouts open from a snapshot of uncommitted work.** Every
integration would have to verify against a second snapshot and replay only the
task's own commits, a task editing a leftover could never land, and the work
would still never reach the branch.

**Running the repository's hooks.** A hook is the person's; its failure would
fail a node whose work was already verified by the workflow's own checks.

## Consequences

Anything a node writes that git does not ignore is committed and pushed with the
branch — build output without a `.gitignore` entry, a `.env`. The guide says so.
A machine without a git identity cannot commit, which scoped nodes already
needed.
