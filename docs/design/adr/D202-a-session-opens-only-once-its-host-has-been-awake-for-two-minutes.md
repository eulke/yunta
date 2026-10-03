---
number: D202
title: "A session opens only once its host has been awake for two minutes"
status: accepted
revises: []
revised_by: []
---

# D202 — A session opens only once its host has been awake for two minutes

## Context

A laptop that sleeps does not stay asleep: it wakes for a minute of
maintenance and sleeps again, over and over. A run moves during each of those
wakes. A session it opens in one starts, the host sleeps under it, and it hangs
until the agent CLI's stream timeout or the session's own `timeout_minutes` —
both counted in awake time, so they take several of those short wakes to
fire. The session is lost, and the run looks stuck for hours. Commands and
criteria cost nothing to rerun; a session costs tokens and a timeout.

## Decision

1. **After the host slept, no session opens until the host has been awake two
   minutes** since it last woke. The engine reads its clocks every second while
   it waits and notices a suspension the same way it always does; a second
   suspension starts the count over. A host the run has not seen sleep is
   settled from the start.
2. **Only sessions wait.** Commands, criteria and checks run as soon as they
   are due: they are cheap, and a sleep that cuts one costs a rerun, not a
   session.
3. **A cancellation ends the wait** as it ends a session: nothing opened, so
   there is nothing to interrupt.
4. **The chronicle says so** on the line that reports the suspension, so a person
   watching knows why no session opens yet.

## Rationale

Two minutes is longer than the maintenance wakes a laptop makes and short next
to the sleep they follow. The wait uses the same two clocks as detection, so it
needs nothing from the operating system and a test moves it with the clock.

## Rejected alternatives

**Waiting for a person.** A laptop that wakes because someone opened it should
go on by itself; asking would stall every overnight run twice.

**Pausing the run on every suspension.** The run would have to be resumed by
hand after each maintenance wake, which is the same stall with a step added.

## Consequences

A run that resumes on a host that just woke takes up to two minutes to open its
first session. A wall clock stepped forward by ten seconds or more reads as a
suspension, and costs the same wait.
