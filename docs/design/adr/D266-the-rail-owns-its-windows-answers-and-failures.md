---
number: D266
title: "The rail owns its windows, its answers and its failures"
status: accepted
revises: [D249, D255, D172]
revised_by: []
---

# D266 — The rail owns its windows, its answers and its failures

## Context

D249 made a session's hooks ask the engine. Its fourth decision set the answer
deadline at 8 seconds from the moment the hook asked, under the 10 seconds D172
gives a write hook. The deadline was not counted from the start of the hook
process, so the process's own startup sat outside it, and the answer was read
against a deadline per read: an engine that sent a byte every few seconds kept
the hook waiting with no limit. D249 named no report route. A call the engine
could not judge, and a hook that gave up on the engine, left nothing on the log
but a debug line.

D255 formatted what a session wrote as it wrote it. Its formatter had no bound,
so a command that never exited held the write's hook until the CLI gave up. A
formatter that could not start was reported to the model and not to the log.
Its time was summed into `stats` as one share of working time, with no
percentile, and the output of a failed formatter was not kept.

Each moment also carried its own reply. The codec encoded a `RailAnswer` by hand
for each CLI, the mock turned a refused write into a failed session, and a stop
hook with a window under a second wrote a timeout of zero. The session of a call
came from the call's body, so a refusal went to whichever session the body
named.

## Decision

1. **One module owns the windows, measured from the start of the hook process.**
   `core::rail::window` holds every window and the margins between them. A write
   hook answers within 7 seconds of its process starting (`WRITE_ASK_MS`), so
   the engine's refusal lands before the CLI's 10-second timeout
   (`WRITE_HOOK_CLI_MS`). A 1-second margin covers the process's startup, which
   the CLI's timeout also counts, and a 1.5-second budget covers the failure
   report. The module asserts that these fit. A hook's timeout is its window in
   whole seconds, rounded up, and never below one second. The stop hook has no
   window of its own: its judgement is bounded by the session budget, or by an
   hour when the budget sets none, as `RunToolsEndpoint::call_timeout` states.

2. **One answer per moment, and one mark on a refusal.** `WriteAnswer`,
   `FormatAnswer` and `StopAnswer` are the answers of the three moments, each
   bound to its moment through `HookAnswer`. The text of a write refusal begins
   with `REFUSAL_MARKER`, and a `WriteRefusal` cannot be built or deserialized
   without that mark or without a reason after it. Each codec implements one
   method per moment, and the reply forms the CLIs share are written once, in
   the adapters' session module.

3. **A hook settles a failure once and reports it with a closed cause.** The
   hook asks the engine for its moment's answer type. When the call fails, the
   hook answers that moment's policy: a write is refused with the cause in the
   refusal, and the other moments proceed. When the failure has a cause the
   engine records, the hook also reports it once, on `/rail/report`. A hook with
   no engine to ask reports nothing, because no engine can record it.

4. **The engine owns each call's session through its listener.** A listener
   serves one session: the one its stream opened last on that listener. The
   listener's token authenticates a call, and the call's body does not name the
   session. Refusals, judgements, formatting results and failures are recorded
   under that session. A call that arrives before the stream opens the session
   waits until it does.

5. **One formatter bound per write, and the output is kept.** The formatters of
   one write share `FORMAT_MS` (5 seconds), counted from the start of the write.
   A formatter still running at the bound is stopped and recorded with exit code
   `-2` and `timed_out`. A formatter that starts after the bound has run out is
   stopped at once. What a formatter printed is stored redacted, and
   `write_formatted` carries its content hash.

6. **One summary per session, with histograms that merge.** A session with a rail
   closes with one `rail_summary`: for each moment, a histogram of the answer
   times, which merges with the histograms of other sessions, and the first model
   the rail heard. `stats` reads the p50 and p95 of each moment from it. No hook
   writes a timing to the log.

## Rejected alternatives

**A deadline on each read.** A sender that paced its bytes could keep a hook past
any window. The window has to bound the whole answer.

**One bound per formatter.** The bounds would add up across a write's formatters,
and the engine could answer after the hook's window had closed. One bound per
write ends inside the window.

**A timing event per call.** Each write and each stop would add an event to the
log for a figure that the session's summary already states at its close, with
the same counts.

**The session named by the body.** The body is the agent's call as its CLI
forwards it; the token is issued by the engine for one listener. Recording a call
under the body's session lets one call put a refusal into another session's log.

**Holding the session on an unjudged stop.** A stop whose judgement cannot run
would hold the session on work nobody can check. The stop proceeds, and the close
still judges the work.

## Consequences

- A write whose answer misses its window is refused, and the log holds a
  `rail_failed` with `timeout`. A write the engine cannot be asked about is
  refused with the cause in the refusal and logs nothing, because no engine
  listens to record it.
- `rail_failed` is an audit kind: no derived state reads it. `rail_summary`
  moves one fact of the derived state, the model of a session whose
  `agent_session_opened` carries no model, and `yunta stats` reads its
  histograms.
- A log written before this decision reads `write_formatted.output` as absent and
  `timed_out` as `false`.
- `RunToolsEndpoint::call_timeout` keeps its signature and its value.
- The payload spec, the contract's event table and the adapter spec state the new
  fields, kinds and windows, and `docs/design/adrs.md` is generated from the
  decision files.
