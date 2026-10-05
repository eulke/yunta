---
number: D244
title: "Whether a round of optional questions stops a run is the workflow's to say, and by default it does"
status: accepted
revises: [D241]
revised_by: []
---

# D244 — Whether a round of optional questions stops a run is the workflow's to say, and by default it does

## Context

D241 let a round whose questions were all optional go on without a person:
each question says what it assumes, and the engine records empty answers in
its place. That suits a workflow that trades a person's word for speed, such
as a quick pass. It does not suit every workflow. An author who put a node
that asks in front of the work counts on a person reading those questions
before anything is built, and D241 decided otherwise for them, from a word —
`required` — that the session writing the questions chose. The author of the
workflow had no say in whether their run stops.

## Decision

1. **A node or a mode says which rounds stop the run.** `ask: always |
   required`, on a node that asks or on a `modes:` entry. With `always`, every
   round that asks something waits for a person. With `required`, only a round
   with a required question waits, and a round of optional ones goes on with
   what each assumes, as D241 decided.
2. **The default is `always`.** A workflow that says nothing waits for a
   person whenever one of its nodes asks something.
3. **The most specific word wins.** A node's `ask` beats its mode's.
4. **`ask` on a node that asks nothing is refused.** `yunta check` names the
   node, since the key would say nothing there.
5. **Fragua's quick mode says `required`.** Its standard and full modes keep
   the default. The grill's prompt no longer tells the session what stops the
   run, because the engine decides that. It still asks each optional question
   to say what it assumes.

What an optional question assumes, and how the node after it reads that,
stays as D241 decided.

## Rationale

Whether to wait for a person is a property of the workflow's course, not of a
question's wording. The same question is worth a stop in a deliberate run and
not in a quick one, and the author who chose the course is the one who knows
which run this is. The default waits because a stop costs time, while going on
without a person can build what nobody asked for.

## Rejected alternatives

**Keeping `required` as the default.** A workflow written before D241 expected
a stop at every round. Its runs would still go on without one, and its author
would have to learn a new key just to get back what they had.

**A flag on `yunta run`.** The course belongs to the workflow. A flag would
make the same workflow stop or not depending on who launched it.
