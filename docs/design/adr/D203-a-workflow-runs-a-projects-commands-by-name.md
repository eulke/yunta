---
number: D203
title: "A workflow runs a project's commands by name"
status: accepted
revises: []
revised_by: []
---

# D203 — A workflow runs a project's commands by name

## Context

A workflow that lints writes the linter into its `bash` node:
`cargo clippy --all-targets -- -D warnings`. For a workflow the repository
wrote for itself that is fine. For a pack it means the pack only runs in one
ecosystem: fragua, the reference pack, could not be used in a TypeScript
repository without copying its workflow and editing the command. The pack
guide already holds that a workflow names roles and the installing project
resolves them — `runner: executor`, `baseline.suite` — but nothing let a
workflow name a command the same way.

## Decision

1. **The config declares what the project runs for each capability**, under
   `commands:` — `{lint: "pnpm lint"}`. It merges key by key across layers.
2. **A `bash` node or a hook step names one**: `run: { command: lint }`. A plain
   string stays a script the workflow wrote. The name is the workflow's; the
   text is the project's.
3. **A project's command runs as the project wrote it.** No template is
   rendered into it — the workflow's templates are the workflow's — and
   `permissions.commands` governs it like any command.
4. **A command the config does not declare is the same fact as any unset key.**
   `ConfigKey::Command` joins the keys a node cannot run without: `check`
   refuses the workflow before the first token, and a run that meets it fails
   the node with the same words and no retry, since its config is frozen.
5. **What a node needs is read off the node alone** (`ConfigKey::needed_by`),
   so a pack's workflows say which commands they need before any project
   reads them. `requires.commands` in a pack manifest, which listed binaries a
   pack starts itself, is renamed `requires.programs`: "command" now means one
   thing.

## Rejected alternatives

**Templating the command into the script (`run: "{{commands.lint}} --fix"`).**
It invites a workflow to add flags that belong to one tool, which is the
coupling this removes.

**A closed vocabulary (`lint`, `test`, `typecheck`).** A pack may need a
capability nobody listed; names are free, like runner names, and `yunta init`
detects the conventional ones.

## Consequences

A pack names capabilities and the installing project declares them; `check`
and `doctor` say which ones a project lacks. The rename of
`requires.commands` breaks a pack manifest that still uses it.
