---
number: D192
title: "A session that asked for scope continues after the answer, in the checkout it saw, on its own work"
status: accepted
revises: [D189, D191]
revised_by: []
---

# D192 — A session that asked for scope continues after the answer, in the checkout it saw, on its own work

## Context

A loop task asked for `crates/cli/Cargo.toml`, wrote what its scope allowed,
and was blocked for a person's decision. The person granted it 44 minutes
later. The task went back to `pending` with nothing carried: a new unit cut
from the run's tree, a new session on the brief, and every file of the first
attempt written again. The first session spent 562k input and 11.5k output
tokens; the second spent 543k and 12.2k to add one file. D189 gave a node the
same answer after a grant — a fresh attempt — and D191 let the attempt after
an engine grant open a fresh session too.

The session that asked knows what it wanted to write and why. One thing
changed for it: the answer. Both adapters resume a session by its id, and
build the resumed process's fence from the new request, so a resumed session
can be held to the widened scope.

A prompt that tells the agent "you were granted X" is not enough on its own.
If the fence, the session's tools and the close read the scope from anywhere
else, the agent is told it may write and is refused when it does.

## Decision

1. **The answer to a scope request continues the session that asked.** A
   person's grant or denial for a task, a person's grant for a node, and an
   engine grant in `rules` mode each pick the session back up instead of
   opening a fresh one.
2. **In the checkout it saw, holding its work.** A task reopens naming that
   session (`resumes` on its `pending`), and its cycle reopens the unit whose
   branch holds the work the attempt left, putting it back as uncommitted
   changes on the tree the unit began from. A node reopens the unit its last
   attempt worked in. A session's conversation names the paths of its own
   checkout; a different one would contradict it.
3. **The work is judged before any session opens.** When the answer alone
   closes the task, no session opens.
4. **The session learns the answer from the engine, and its scope from the
   tools.** The resumed session is told what was granted or refused, and
   which tool reads its scope and what still keeps it from closing. The
   scope is not copied into the message: the fence, the tools and the close
   all read it from the grants on the log.
5. **The log says which session continues which.** `agent_session_opened`
   names the task a loop's session works (`task_id`) and the session it
   resumes (`continues`).
6. **When it cannot continue, the work still does.** An adapter that declares
   no resume, a CLI that refuses the session, or a checkout that is gone opens
   a fresh session on the brief — over the same work when the checkout is
   there, or with the work carried into a fresh unit when it is not — and
   the log records a `capability_degraded` (`resume_session` → fresh
   session).

## Rationale

A second session is justified when something it depends on changed, and it
earns its cost when it has something the first did not (D191). After an
answer, the first session has everything the second would have except the
answer itself, which one message carries. Continuing it spends that message
and the turns the remaining work needs, instead of re-exploring and
re-writing.

## Rejected alternatives

**Keeping the session open while a person decides.** The run parks, its
process may exit, and the decision may come from another process hours
later.

**A fresh session over the carried work.** It keeps the work but not what the
session knew about it, and re-reads what it wrote. It is the fallback, not
the rule.

**Resuming in a fresh unit with the work carried in.** The conversation's
paths would name a checkout that is not the one the session works in.

**Continuing after `retry` and `continue-work` too.** Those follow a red
attempt, not an answer: nothing the session asked for changed.
