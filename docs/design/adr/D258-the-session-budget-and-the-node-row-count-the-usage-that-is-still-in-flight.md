---
number: D258
title: "The session budget and the node row count the usage that is still in flight"
status: accepted
revises: []
revised_by: []
---

# D258 — The session budget and the node row count the usage that is still in flight

## Context

A session reports its usage while it runs, and that usage settles only when the
node's terminal or a round of questions records it. Until then the tokens are
spent all the same; only the bookkeeping lags. D17 evaluates budgets against the
`Usage` in the log, and it does not say which part of that usage counts.

Each surface read a different part. The run-wide budget and the share each
session was handed read the settled figure alone, so a session that opened while
another was spending saw a remaining cap that left the other's usage out. The
frame read the settled figure plus the in-flight usage, the status read the
settled figure alone, and a node's row read only its closed attempts. The same
screen could therefore show two figures for one run: while a node waited on a
question, the closing line read `60 in / 24 out` and the status read
`30 in / 12 out`. A node that was working showed zero, and a run cancelled
while an attempt was open recorded zero for the usage that attempt had reported.

## Decision

1. **A run's spend is one figure with two parts.** The settled part is closed
   work and the usage of asking rounds that settled. The in-flight part is the
   usage a session reported for an attempt that is still open, which no terminal
   or round has settled. The spend a surface shows is the sum of the two.
2. **The run-wide budget counts that sum.** `limits.max_tokens_per_run` is
   compared with the run's spend before each batch of nodes. A session's share of
   the budget is `min(remaining, cap / nodes still owed)`, where `remaining` is
   the cap less the same spend, in-flight usage included.
3. **A node's row shows that node's spend**, in-flight usage included, so the
   rows of a run add up to the run's figure, child runs aside.
4. **A closed run records that sum.** The metrics its `run_finished` carries are
   the spend at the close. Nothing is left in flight once a run is closed, and a
   closed run spends what it recorded.

## Rationale

The budget exists to stop spending, and usage a session has reported is already
spent, whether or not a terminal has written it down. Counting only the settled
part lets sessions that run together each plan against a remaining cap that
leaves out what the others have reported, and it lets the escalation fire only
after the in-flight usage has already gone past the cap.

One sum on every surface keeps the frame, the status, the node rows and the
recorded metrics saying the same thing about one run. A second figure for the
same spend would be a second owner of that fact. The sum is read from the node
ledgers, which fold the log, so no stored running total can drift from the events
it summarises.

The choice is costly to reverse. The budget decides when a run stops to ask a
person, and the metrics a closed run records are read by every later surface, so
the rule for what counts cannot change for one surface alone.

## Rejected alternatives

**Settled usage everywhere, as before.** The frame, the status and the node rows
keep disagreeing during every wait, and the budget keeps letting sessions that
run together spend past the cap.

**In-flight usage in the node row only.** The row would agree with the frame,
but the budget and the session shares would still ignore that usage, so the run
could still spend past its cap.

**In-flight usage in the budget only.** The budget would stop at the right figure,
but the screens would still show two numbers for one run during a wait.

**Settling a round from its own session's messages.** The usage messages carry no
session id, so a round cannot pick out its own session's messages. The round
subtracts the usage it declares from the attempt's in-flight usage, and the usage
other sessions of that attempt reported stays in flight.

**A stored running total updated by each event.** It would be a second owner of
the spend, when the node ledger already folds it from the log.

## Consequences

- A run cancelled while an attempt was open records the usage that attempt had
  reported. Before this decision it recorded only what had settled.
- A run in progress can reach its budget earlier than it did, by the usage its
  sessions have reported and that no terminal has settled, and it pauses on that
  figure.
- The JSON surfaces carry `in_flight` next to the total, so a reader can tell the
  open part of a spend from the settled part.
- Logs already written are read with the same rule. A run that stopped while an
  attempt was open, and is closed after this decision, records the usage that
  attempt had reported. A run closed before it keeps the metrics it recorded.
