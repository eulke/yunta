---
number: D189
title: "A node that fails on its scope names the paths, and a person may widen the node by exactly those from the failure's menu; a node's session can check its scope and ask first"
status: revised
revises: []
revised_by: [D192]
---

# D189 — A node that fails on its scope names the paths, and a person may widen the node by exactly those from the failure's menu; a node's session can check its scope and ask first

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
5. **A node's session can check its scope and ask first.** The session
   of a node that declares `scope:` gets `yunta_check_scope`, which runs
   the close's own audit and lists what lies outside, and, where a person
   may grant, `yunta_request_scope_expansion`. The close takes the request
   out of the checkout, records it as `scope_expansion_requested` with no
   `task_id`, and fails the node with it (`requested_scope`), so the
   failure's menu offers the same `grant` with the session's reason as
   evidence. A node has no rules to decide a request by; a person always
   does.

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

Telling a session its scope is not enough on its own. A session that
obeys it and cannot ask finishes without the fix, and the dead end moves
to the node whose failure it was correcting, whose re-routes then run
out. Asking turns the need into the same decision a violation reaches,
one attempt earlier and with the reason attached. The session learns its
scope from a tool that runs the close's audit, not from a copy in its
prompt.

## Rejected alternatives

**Accepting the failed attempt's work under the widened scope.** It saves
a session, but it needs a session-less close for nodes that submit
documents.

**Requiring `scope_expansion:` on the node before offering `grant`.**
The default would keep the dead end this decision removes. The permission
ceiling already lets a layer forbid it.

**Letting `retry` widen the scope.** One option would mean two things
depending on the failure.

**Giving node sessions the loop's modes (`rules`, `ask`, `deny`,
`within`, `max_per_run`).** A node has no criteria to pre-check a request
against, and a person already answers the failure's menu.
