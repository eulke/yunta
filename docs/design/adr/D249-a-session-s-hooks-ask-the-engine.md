---
number: D249
title: "A session's hooks ask the engine"
status: revised
revises: [D172]
revised_by: [D266]
---

# D249 — A session's hooks ask the engine

*(Revised by D266: decision 4's answer deadline is 7 seconds from the start of the hook process, not 8 seconds from the ask, and a failure the engine hears of is recorded once in the log, with a closed cause, through the report route `/rail/report`.)*

## Context

D172 put the fence's judge in core and gave each adapter a codec for its
CLI's hook: `yunta fence <adapter>` read the fence from `YUNTA_FENCE` in the
child's environment, judged the write and answered. The engine heard of a
refusal only if the CLI printed it back into its stream, and only an adapter
whose parser recognised the marker reported it. Claude Code does; Codex,
whose stream shows nothing for a refused patch, cannot. A refused write that
never reaches the log is one that the evidence rule (D240) never grants.

The hook was also the only moment Yunta could speak to a running session,
and it could say one thing: whether a write may happen. Formatting what a
session wrote, or keeping a session from stopping on red work, needs the
engine itself at the other end of the hook, with the run's log and the
session's checkout in hand.

## Decision

1. **The hook is a thin client.** `yunta rail <adapter> <event>` reads the
   CLI's call through the adapter's `RailCodec`, sends it to the engine and
   writes the engine's answer back in that CLI's shape. It judges nothing.
2. **The engine answers on the session's own listener.** The listener every
   session already has for its run tools serves a `/rail/<event>` route
   behind the same single-use token. The session's CLI hands its hooks the
   route and the token in `YUNTA_RAIL_URL` and `YUNTA_RAIL_TOKEN`; the fence
   stays in the engine, set on the session's rail before it is dispatched.
3. **A refusal is on the log before the hook hears it.** The engine judges
   each path the call names with `Fence::judge` and records `write_refused`
   for each refused one, whichever adapter opened the session. No adapter
   reads a refusal out of its stream any more.
4. **A hook that cannot ask refuses a write.** No engine in its
   environment, a call it cannot read, a refused credential or no answer
   within 8 s all answer as a refusal. The deadline sits under the 10 s a
   CLI gives its write hook (D172, decision 9), so the CLI never runs out
   of patience first and lets a write through unanswered. The post-check
   diff remains the guarantee.
5. **One event per moment.** `before-write` is the first; each later
   moment of a session the engine answers is one more event of the same
   subcommand and route, never a second channel.

## Rationale

A judgement the engine makes is one it can record, and one it can make with
everything the run knows. Keeping the hook as a client also keeps the
adapters to translation: every CLI with hooks gets the same rail by writing
a codec.

## Rejected alternatives

**Keep judging in the hook and report refusals to the engine afterwards.**
Two paths for one fact, and a hook process that has to reach the engine
anyway.

**Read refusals out of each CLI's stream.** It works only for a CLI that
prints them, which is the gap this closes.
