---
number: D229
title: "A process group is killed until it is empty, never stopped; its pipes drain for a bounded moment, and its exit is heard when it happens"
status: accepted
revises: [D185]
revised_by: []
---

# D229 — A process group is killed until it is empty, never stopped; its pipes drain for a bounded moment, and its exit is heard when it happens

## Context

D185 closed a group by stopping it with `SIGSTOP` until two observations agreed
on its members, then killing it. A member that calls `setsid` while the stop is
delivered — a detached daemon, the `gc` or `maintenance` a git command starts —
completes the call with the stop pending and is stopped in a session of its own.
The kill then goes to a group it no longer belongs to, and nothing continues or
kills it: it stays frozen, holding the pipes it inherited. The engine read those
pipes to their end, so the command that started the daemon never answered and the
run hung. Runs on a real repository left such processes behind, and a stress test
that starts members leaving their group while it closes reproduces them.

Two more costs sat on every command. The leader's exit was found by polling
`waitid` every 10ms, and an interrupted agent session was always given its whole
grace before the kill, even when it had already left.

## Decision

1. **Kill until empty.** Closing a group repeats `SIGKILL` to it and looks at its
   members until none can still execute; zombies count as terminated and an empty
   group needs no signal. No member is ever stopped. A child born while one signal
   was delivered gets the next. A member that leaves the group as it closes is
   killed before it leaves, or runs on outside it; it is never frozen. The leader
   stays unreaped until the close ends, as D185 decided.
2. **Bounded drain.** Once the group is closed nothing in it can write: what is
   buffered reads at once, and a pipe still open a moment later — 250ms — is held
   by a process outside the group. Reading stops there and keeps what was read.
   The same holds for an agent session's stderr.
3. **The exit is heard.** A child's exit wakes the waiter through the kernel: a
   pidfd on Linux, `NOTE_EXIT` on a kqueue on macOS, both on the runtime's own
   event loop. Every wake is confirmed with `waitid(WNOWAIT)`, which leaves the
   child waitable. Two facts of the kernel shape the watch. macOS accepts a
   kqueue registration for a process that already exited and never fires it,
   so the child is looked at before the first wait. And the kernel tells of
   the exit once, a moment before `waitid` can see it — under load, long enough
   for the confirmation to answer "not yet" — so after it has told, the waiter
   looks every millisecond until the child shows exited. Where neither signal
   is available the waiter looks every 10ms.
4. **Grace only while leaving.** An interrupted session gets its grace until its
   process exits, then the kill at once; one with no process of its own gets none.

## Rationale

The frozen daemon exists only because something stops processes; without the
stop, the race has no bad outcome. The drain bound turns a daemon's pipe into a
cost of at most a quarter second instead of a hung run, and a command never waits
for a process it does not own. Hearing the exit removes a fixed delay from every
git, criterion and suite the engine runs.

## Rejected alternatives

**Keep the stop and hunt escapees afterwards.** Nothing names them: they left the
group and their session is their own, so finding them means scanning the host for
processes that may not be ours.

**A thread blocked in `waitid` per child.** Portable, but the runtime waits for
every blocking task when it shuts down, so a child still alive at exit would hold
the engine open.

**Turn off the daemons and keep the stop.** The engine's own git already starts no
upkeep, but any command a run executes can detach something; the close has to be
correct for all of them.
