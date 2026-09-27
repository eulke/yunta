---
number: D189
title: "A node that fails on its scope names the paths, and a person may widen the node by exactly those from the failure's menu"
status: accepted
revises: []
revised_by: []
---

# D189 — A node that fails on its scope names the paths, and a person may widen the node by exactly those from the failure's menu

## Context

A lint failure was re-routed to a corrective node declared with
`scope: ["**/*.rs"]`. The cause of most of the errors was a `[lib]` target
that an earlier task had added to a `Cargo.toml`. The corrective node found
it and changed the manifest, and the scope audit failed the node: "1 file(s)
outside the declared globs". The node has no `on_failure` of its own, so the
run asked a person, with `retry` or `abort` as the only answers.

Neither answer could change the outcome. `retry` starts a fresh attempt
from the same tree under the same scope, and the fix it needs is still
outside that scope. `abort` pauses, and a resume asks the same question.
Scope expansion existed only for a loop's tasks. The person could not
even read which file the node had written: the paths were on
`scope_checked`, and the failure carried a count.

## Decision

1. **A scope failure is the list of paths.** `node_failed` carries
   `outside_scope`, and every surface renders the paths from it.
2. **The failure's menu offers `grant`.** When a node fails on its scope,
   the menu is `grant`, `retry`, `abort`. `grant` widens the node's scope
   by exactly the paths it wrote outside, and runs the node again in a
   fresh attempt. The attempt's session fence and its close both hold it to
   the widened scope. The grant lasts for the rest of the run.
3. **Only a person grants, and only where one may.** `grant` is not
   offered to a `read-only` node, and not under a permission ceiling of
   `scope_expansion.max_mode: deny`. Nothing else limits it: a node that
   is not a task has no modes, `within` or `max_per_run`.
4. **One decision, one grant, on the log.** The grant is
   `scope_expansion_granted` with no `task_id`, written under the node,
   before the attempt it widens starts. A restart finds it after the
   decision and does not write it again. The live prompt and a decision
   seeded by `resolve_gate` reach the same attempt and the same grant.

## Rationale

A menu should offer the answer that changes the outcome. When the failure
is the scope, the change is the scope, and the paths the node wrote are
the smallest widening that lets its work stand.

A fresh attempt is simpler than accepting the failed attempt's work. A
node's close reads the documents its session handed over while it ran, so
closing without a session would need its own rules for what a document
from an earlier attempt means.

The grant is written by the attempt that consumes the decision, as a
loop's `retry` reopens its tasks. The same decision then works whether a
person answered at the terminal or from another process.

## Rejected alternatives

**Accepting the failed attempt's work under the widened scope.** It saves
a session, but it needs a session-less close for nodes that submit
documents.

**Requiring `scope_expansion:` on the node before offering `grant`.**
The default would keep the dead end this decision removes. The permission
ceiling already lets a layer forbid it.

**Letting `retry` widen the scope.** One option would mean two things
depending on the failure.
