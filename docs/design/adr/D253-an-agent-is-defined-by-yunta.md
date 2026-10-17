---
number: D253
title: "An agent is defined by Yunta, and every adapter adds its instructions"
status: accepted
revises: [D29]
revised_by: []
---

# D253 — An agent is defined by Yunta, and every adapter adds its instructions

## Context

D29 made `agent:` portable by having each adapter translate the name to its
CLI's own agents and `probe()` check they exist. Only `claude-code` had a
mechanism (`--agent`, over the person's `.claude/agents/`); `codex exec` has
no agent selector, so it declared no `custom_agents` and a workflow naming an
agent could not run on it. The definition lived in each person's CLI
configuration, outside the run's checkout, so two machines could run the
same workflow under different instructions without the log saying so.

A Codex profile bundles a model with instructions, but a model belongs to
one provider: an agent that names one cannot run on the other CLI. Both
CLIs take extra system instructions headless — `claude` with
`--append-system-prompt-file`, `codex` with `-c developer_instructions`.

## Decision

1. **The definition is Yunta's.** `agent: <name>` resolves to
   `.yunta/agents/<name>.md` in the run's checkout; `publisher/agent`, to the
   file a pack declares under `contents.agents`. Its front matter holds
   `name` and `description` only; its body is the instructions.
2. **A model or tools in a definition are refused.** The model is the
   runner's and the tools are the permission profile's.
3. **The engine resolves, the adapter hands over.** `SessionRequest.agent`
   carries the name and the instructions; each adapter adds them to its
   CLI's own, and both declare `custom_agents`.
4. **A name without a definition fails its node before the session opens**,
   naming where it was looked for — the same as a missing skill.

## Rejected alternatives

**A Codex profile per agent.** It ties the agent to a model of one provider
and lives in the person's configuration, outside the checkout.

**Keep each CLI's own agents.** The same workflow would run under different
instructions on each machine and each CLI, and on Codex not at all.
