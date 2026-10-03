---
number: D199
title: "A run records the time its host was suspended, and its durations leave it out"
status: accepted
revises: []
revised_by: []
---

# D199 — A run records the time its host was suspended, and its durations leave it out

## Context

A run on a laptop stalled for two hours: the machine slept. The agent CLI's
own stream timeout and the session's `timeout_minutes` both count the time the
host is awake — the process's monotonic clock does not advance while it
sleeps — so neither fired while it slept, and each took minutes of awake time
to notice after it woke. During the short maintenance wakes in between, the run
moved a little and stalled again. The log showed two hours with nothing in them,
and every duration the run reported counted the sleep as work.

## Decision

1. **The engine notices a suspension by comparing two clocks.** The clock port
   reads wall time and awake time; awake time is the process's monotonic clock,
   which stands still while the host sleeps on the systems the engine runs on.
   Between two readings, a wall clock that outran awake time by ten seconds or
   more says the host slept for the difference. The engine looks every second,
   and before every event it appends. No operating-system API is involved.
2. **The log records it**, as `host_suspended { slept_ms }`, stamped when the
   engine noticed. It is a fact about the machine, not something an invocation
   did: it does not wake the run.
3. **Every duration the run reports leaves the suspended spans out** — a
   node's time, the time it waited, the run's elapsed, the chronicle's clock.
   Readings of liveness — how long ago the last event was, how long a run has
   been in its state — stay on the wall clock, because they answer when, not
   how long something worked.
4. **Timeouts count awake time**, as they did: a suspended host spends none of a
   session's budget, and the log says why a session outlived its wall-clock
   minutes.

## Rationale

Two clocks the process already has answer the question on every system the
engine runs on, with nothing to install and nothing to subscribe to, and a test
can put a host to sleep by moving one of them.

## Rejected alternatives

**Operating-system sleep notifications.** They differ on every system, need a
subscription the engine would have to keep alive, and say nothing a pair of
clocks does not.

**A boot-time clock for timeouts.** It would spend a session's budget while the
machine sleeps and cut a session the moment the host wakes, before the agent's
own connection has had a chance to recover.

## Consequences

A wall clock stepped forward by ten seconds or more — an NTP correction, a person
setting the time — reads as a suspension. A wall clock set back reads as none.
The detection rests on the process's monotonic clock not counting suspended time;
a platform where it did would stop detecting anything, without failing.
