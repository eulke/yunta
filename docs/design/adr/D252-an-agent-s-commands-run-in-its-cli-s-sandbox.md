---
number: D252
title: "An agent's commands run in its CLI's sandbox, and a node's closed network holds there"
status: accepted
revises: [D105, D119]
revised_by: []
---

# D252 — An agent's commands run in its CLI's sandbox, and a node's closed network holds there

## Context

A Claude Code session under `edit` had no shell at all, and a Codex session
under `edit` had one confined by its sandbox: the same workflow gave an agent
different powers on each CLI. Neither adapter claimed network isolation, so a
node's `network: false` stayed declarative (D105, D119), and Codex's network
was a setting of its own (`adapter_settings.network_access`), closed by
default where Claude Code's was open.

Claude Code's sandbox confines its shell's writes to the working directory
and the directories it was handed, refuses the network unless allowed —
`allowedDomains: ["*"]` opens it, `allowLocalBinding` loopback — and with
`failIfUnavailable` and `allowUnsandboxedCommands: false` it neither runs
unconfined nor lets a command leave it. With `denyWrite` on the working
directory it writes nothing of the checkout. Codex's `read-only`,
`workspace-write` and `danger-full-access` modes confine the same way.

## Decision

1. **Every session has a shell, confined the same way on every CLI.**
   `read_only`: nothing of the checkout is written; `edit`: the checkout and
   the run's roots; `full`: unconfined. `adapter_settings.sandbox` picks the
   `edit` mode, and both adapters read it.
2. **The network is the session's request.** It is the node's `network:`,
   else `permissions.network.default`, else open. Under `edit` it opens or
   closes the commands' network, loopback included; under `read_only` it is
   closed; under `full` it is open. Both adapters declare
   `network_isolation`, and `adapter_settings.network_access` goes.
3. **`network: false` on a `full` node is refused by `check`.** Nothing
   confines a full session's commands on any adapter.
4. **Coverage says what the commands were left**: nothing, the checkout and
   the roots, or everything.

## Rejected alternatives

**Keep the shell out of a Claude Code `edit` session.** Codex cannot work
without one, so parity comes from giving both a confined one.

**A loopback-only level.** Codex's sandbox opens all of the network or none
of it.
