---
number: D241
title: "Only a required question stops a run, and an optional one says what it assumes"
status: accepted
revises: [D173]
revised_by: []
---

# D241 — Only a required question stops a run, and an optional one says what it assumes

## Context

D173 made a node that asks wait between `questions_asked` and
`questions_answered`, whatever it asked. A question's `required` only decided
whether an answer could be left out once a person answered; it never decided
whether the run waited. So a round of questions that were all optional — ones
whose author said the work can go on without them — paused the run until a
person came back, and that person found nothing they had to decide. When they
did not come back soon, the run sat idle with nothing blocking it.

## Decision

1. **An optional question says what it assumes.** `Question` gains `assumes`:
   what the work takes as the answer when nobody gives one. A question with
   `required: false` and no `assumes` is refused when it is handed over.
2. **A round with nothing required does not wait.** When every question a node
   hands over is optional, the run records `questions_asked`, then the engine's
   own empty answers and a `questions_answered` whose channel is `assumed`, and
   the node finishes in the same close. The questions stay on the log and in the
   artifact, each with what it assumes.
3. **A round with a required question waits, as before.** A person answers
   what they want of the optional ones at the same time.
4. **What comes after reads the assumptions.** A node that reads the questions
   and their answers finds each unanswered question with its `assumes`, and
   treats it as an open point the plan closes.

## Rationale

A person is reached only for what the engine cannot decide on its own. A
question whose author already wrote the answer the work can live with is not
one of those: stopping for it costs the person a trip and the run its time, and
leaves the decision where it was.

## Rejected alternatives

**Asking the optional questions without pausing, when a console is there.** It
would make the run's course depend on who happens to be watching, and the
answer would arrive after the next node already started.

**Filling each answer with its assumption.** A `choice` or `boolean` answer has
to be one of its values; an assumption is prose. The answers stay empty and the
assumption stays with its question.
