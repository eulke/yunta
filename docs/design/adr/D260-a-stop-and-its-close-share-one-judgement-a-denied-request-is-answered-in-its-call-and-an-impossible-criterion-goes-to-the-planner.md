---
number: D260
title: "A stop and its close share one judgement, a denied request is answered in its call, and an impossible criterion goes to the planner"
status: accepted
revises: [D254, D73]
revised_by: []
---

# D260 — A stop and its close share one judgement, a denied request is answered in its call, and an impossible criterion goes to the planner

## Context

A session's stop and its attempt's close judged the same work along different
lines. The close read the criteria, the scope audit, the request and the documents
a node owes. The stop read the criteria and the scope, but a session that had left
a request file was exempt from the stop check, and the request itself was decided
only when the attempt ended (D254, decisions 2 and 4). A request the project refuses
on its own terms, such as one under mode `deny`, therefore told the session nothing
until the attempt ended. The session could write the refused path and stop with no
`stop_refused` naming it.

The same split caused two more gaps. A task that closed with a request still waiting
blocked its loop with `ScopeOwed`, although no one needed that request any more. A
criterion that no work of the plan could pass had no route to the planner: the only
declaration a session could make was a departure, which waited for a person to accept
it, while the criterion itself belongs to the plan.

## Decision

1. **One judgement decides a session's stop and its close.** `task_cycle::judge`
   reads a session's criteria, its request, its scope audit and, for a node, the
   documents it owes, in one pass. A stop asks it dry (`Settle::Dry`), which reads the
   grant window without moving it. The close asks it on the attempt's own window
   (`Settle::Commit`). Only the commit moves the window, and only a commit's decision
   counts toward `max_per_run`.

2. **A stop is held only for work the close would not take.** A grant or an escalation
   is the close's to decide, so a session whose request the close can grant or escalate
   goes even with red criteria. A request that is only denied does not exempt the
   session. A criterion that cannot run at all, and a criterion declared impossible, do
   not hold the stop either: the close blocks the task on the first, and the planner
   takes the second.

3. **A task's request the project refuses on its own terms is answered in its call.** Mode
   `deny` and a path no person may grant are settled from the request alone, and no work
   the session does changes them. They are settled before any file is read, so the tool's
   reply carries the reason at once, and the log records the request and its refusal at
   that moment. A refusal that depends on the work, such as a proposed criterion that
   already passes, the file bound, or a `rules` request that names no criterion, which a
   red output that locates every path it asks for still grants, is decided when the
   judgement runs on the request, at the stop or at the close. The call and the judgement
   share the one rule for what stands, so a request meets the same answer however it
   reached the engine.

4. **A request joins the pending one under the same criterion.** The request file is
   read and written under one lock per run. A request under the criterion already
   pending adds its paths, even when several arrive at once. A request under another
   criterion is refused with `request_pending`, which names the pending criterion.

5. **A request left when its task closes is withdrawn.** A task that closes while its
   request still waits records `scope_expansion_withdrawn` with the cause `task_closed`.
   Nobody decided it, so it is neither a grant nor a denial, leaves no finding and does
   not count toward the cap. A request goes to a person only when the attempt cycle ends
   without the task closing. Before that, a pending escalation with a red criterion gets
   one fresh session, once per cycle, which may close the task without the request. When
   it does not, the cycle ends and a person decides, and no further session spends the
   run while they do. Their answer picks back up the session the cycle ended in.

6. **A criterion no work of the plan can pass returns to the planner.** A task session
   declares it with `yunta_declare_deviation` and `kind: unpassable`. The engine accepts
   the declaration only for a criterion of the task itself, not one of the suite, that is
   red on the tree the session left, and only with a reason. The first declaration of a
   session is recorded as `criterion_declared_unpassable`; a second is answered and not
   recorded. When a loop has nothing left to run and its blocked tasks hold declarations,
   the node fails with `PlanUnpassable`, which lists each task with its criterion and its
   reason, and `on_failure.goto` routes it to the planner. A repeat of the same task and
   criterion fails as `unchanged`: it is not routed again, and the node waits for a
   person.

## Rationale

The stop and the close must agree on the same work. When they judged it differently, a
session was sent back for a reason its close would not act on, or let go while its close
would hold it.

A refusal the project makes on its own terms needs no checkout and no attempt. Answering
it in the call lets the session act on it while it still can, and the log shows the
refusal at the moment the request was made.

A withdrawn request was never decided, so no one should be asked about it. Escalating it
would ask a person about a request whose task had already closed.

An impossible criterion is a fact about the plan, so the planner rewrites the plan. A
repeat of the same declaration after a rewrite means the planner did not change the
cause, so a person decides what happens next.

## Rejected alternatives

**A separate judgement for the stop.** Two judges of one state of work would drift
apart, and each fix would have to land twice.

**Hold every stop until the request is settled.** A session that cannot do better would
wait until its budget ran out.

**Escalate every request still waiting when an attempt ends, as before.** A task that
closed would still block its loop on a request it no longer needed, and a red criterion
would put a person's decision ahead of a session that could still act.

**Refuse a second request while one waits.** A session that adds paths under one
criterion in two calls would lose the second, and the agent would have to guess what
the first call had already asked for.

**Declare an impossible criterion as a departure.** A departure keeps the task open
until a person accepts it, but the criterion belongs to the plan, which the planner
rewrites.

**Route every repeat of a declaration to the planner again.** The planner would rewrite
the same plan without a change to its cause, and the run would reroute until the budget
ran out.

## Consequences

- Logs written before this decision hold none of `scope_expansion_withdrawn`,
  `criterion_declared_unpassable` and `PlanUnpassable`. Replay moves no ledger for them,
  and no existing kind changes its shape.
- A session learns of a request the project refuses from the tool's reply, and the
  wording of that reply is new with this decision.
- D254's decisions 2 and 4 are revised: a stop no longer waits for an attempt's end to
  decide a request the project refuses on its own terms, and a stop is exempt while the
  close can grant or escalate the request. D73's rule that every request is decided when
  its attempt ends holds for requests the project does not refuse on its own terms.
