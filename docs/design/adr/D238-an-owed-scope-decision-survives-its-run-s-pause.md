---
number: D238
title: "A scope decision owed to a person survives the run's pause, and a refused tool says why"
status: accepted
revises: [D73, D189]
revised_by: []
---

# D238 — A scope decision owed to a person survives the run's pause, and a refused tool says why

## Context

A task's request for scope in `ask` mode reached a person only while one was
there to answer. With nobody there, the loop failed at once with a sentence, the
run paused, and the gate it paused on offered `continue-work`, `retry` and
`abort`: none of them answered the request, which lived only in memory and was
lost. Every other task of the loop stopped with it, though nothing they did
depended on the answer. And when the engine refused a session's call to one of
its tools — a second request while the first waited, arguments it could not read —
the log said only that the call failed, so nobody could tell why.

## Decision

1. **The task waits, the loop goes on.** A request nobody is there to decide keeps
   its task blocked and is owed; the loop runs every task that does not depend on
   it.
2. **The loop ends on what it owes.** Once nothing else can run, the loop fails with
   `owed`: each task's request, with the paths it asked for and the reason its
   session gave.
3. **The failure's answer decides them.** Its menu offers `grant`, which grants
   every request and reopens each task on the work its last attempt left. Any other
   answer denies them, each with the finding a denial becomes, and the tasks go on
   within what they declared.
4. **A refused call says why.** The server that refused a call records
   `run_tool_refused` with the tool and a closed reason.

## Rationale

A decision the engine cannot make is the person's, and the run has to keep asking
it until it is answered — from the place a person answers paused runs. Waiting
only the task that asked keeps the rest of the loop's work moving meanwhile.

## Rejected alternatives

**Failing the loop as soon as a request is owed.** Stops independent work for a
question about one task.
