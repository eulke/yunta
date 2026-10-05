---
number: D242
title: "A session cut off from its service resumes once the service answers"
status: accepted
revises: [D191]
revised_by: []
---

# D242 — A session cut off from its service resumes once the service answers

## Context

A session whose CLI loses the network fails with a message like `API Error:
Can't reach the API server — check your internet or DNS (ENOTFOUND)`. The
engine read it as any other failure: the node failed, the run paused on a gate
offering `retry` and `abort`, and a `retry` opened a fresh session that redid
from the start what the lost one had already done — millions of cached tokens
and the whole conversation thrown away. Nothing about the work was wrong, and
nothing a person could decide would have helped: the run only had to wait for
the network, as D202 has it wait for a host that just woke.

## Decision

1. **The cause is typed at the port.** An adapter reports a failure as
   `retryable`, `final` or `unreachable`, read from narrow markers —
   `ENOTFOUND`, `EAI_AGAIN`, `ECONNREFUSED`, `ENETUNREACH`, `ETIMEDOUT`,
   "Can't reach the API server". A stream that broke after the service
   answered stays the attempt's.
2. **The engine waits for the service.** It records `service_unreachable`,
   then probes the endpoint the session would use — its proxy, its base URL or
   the provider's host, through `Adapter::reachable` — with growing pauses.
   An adapter that cannot locate its service waits one pause, and the resumed
   session is the probe. Once the service answers, the engine also waits for
   the host to settle (D202), and records `service_reachable` with how long it
   waited.
3. **The same session goes on.** It is resumed with what is left of its time
   and its tokens, told that the connection dropped and nothing was lost. This
   is not another attempt: D191's rule — another session only when something
   changed — is about attempts, and here the session itself continues.
4. **The wait is bounded and cancellable.** A cancellation ends it at once;
   past 30 minutes of the host's awake time, or with an adapter that cannot
   resume sessions, the session fails as it would have before.
5. **`stats` counts it apart.** Time a session spends waiting for its service,
   with nothing else at work, is `offline`, never `working`.

## Rationale

A person reached for a dropped network has nothing to decide, and a fresh
session pays again for everything the lost one did. Waiting is what a person
would do; the engine can do it unattended, and keep what the session built.

## Rejected alternatives

**Retrying with a fresh session after a fixed delay.** It loses the
conversation and pays for its context again, and a fixed delay is either too
short for a real outage or too long for a blip.

**Waiting without a bound.** A service that is gone would hold the run open
forever with nobody told.
