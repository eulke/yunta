---
number: D250
title: "A session loads its person's own CLI configuration unless its workflow isolates it, and no session writes an agent's configuration"
status: accepted
revises: [D172]
revised_by: []
---

# D250 — A session loads its person's own CLI configuration unless its workflow isolates it, and no session writes an agent's configuration

## Context

D172 (decision 6) placed each session's fence under its scratch directory and
pointed the CLI's home variable there, so a session would load nothing but
what the run mounts. No adapter did it: every session loaded the person's own
configuration — their settings, extensions, MCP servers and hooks — and the
run's on top. Relocating a CLI's home also moves where it keeps its login, and
a token refreshed in a copy can stop working in the original.

Both CLIs now offer a narrower way: Claude Code loads no setting source and
no MCP server but the ones handed to it with `--setting-sources ""` and
`--strict-mcp-config`, and Codex skips the person's `config.toml` and rules
with `--ignore-user-config` and `--ignore-rules`; both keep their login.
People also differ on whether a run's sessions should carry what they set up
for themselves.

Checkouts now come from a pool that outlives runs. A file a session writes
where its CLI reads configuration — a hook, an MCP server — would be loaded by
the next session that opens in that checkout, and a hook runs outside the
session's sandbox.

## Decision

1. **`cli_config: inherit | isolated`.** A node says whose configuration of
   the agent's CLI its sessions load, over what `node_defaults` says;
   `inherit` is the default. The adapter translates it to its CLI's flags.
   The CLI's home is never relocated, and its login is the person's either
   way.
2. **The run's own hooks load in both.** They come with the session's
   request, not from a configuration file the choice above leaves out.
3. **An agent's configuration is denied to every session.** Each adapter
   names the files of a checkout its CLI loads code-running configuration
   from; the run denies the union of every adapter it can drive to every
   session, every close and every scope request, like
   `permissions.paths.deny`.

## Rejected alternatives

**Relocate the CLI's home with the login linked in.** It isolates, but a
token refresh in the copy can break the original, and it cannot inherit.

**Deny the whole `.claude/` and `.codex/` directories.** It would refuse a
project's own agent definitions and skills, which run nothing on load.
