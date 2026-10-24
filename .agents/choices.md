# Choices

What Yunta does not do, and why, so a change does not propose it again unaware. An entry goes when its reason
stops holding.

### No workflow unlocks a protected path
Not a key that lets a run write `.claude/`, `.codex/`, `.agents/skills/` or `.mcp.json`. Claude Code refuses
writes under `.claude/` in a headless session and its sandbox never lets a command write there, so the unlock
would hold on one CLI only; a session could edit the declaration that unlocks it; and a permission layer only
narrows the one above it. A flow that iterates a skill keeps its source under `.yunta/skills/`, which nothing
protects: every run freezes the version it starts with.

### No Windows build
The engine's process layer is POSIX: process groups, signals, and telling a live lock from a dead one. A binary
that cannot stop a run's process tree is not published.

### No visual workflow builder
Not an editor that draws a workflow. A developer writes YAML faster, with completion, git and a diff; for anyone
else the hard part is a criterion a command can verify, which no canvas writes. `yunta graph` draws what a workflow
declares, and the YAML stays the only source a run reads: a drawing holding state the YAML did not would break
versioning, packs and the manifest a run freezes.

### No merge into the base branch
Not a merge queue or a lock of Yunta's own. A run pushes its branch and opens a pull request; the forge's merge
queue and branch protection already order concurrent merges and hold the required checks and the permissions, so a
lock Yunta kept would only copy them. `check` warns about a node that pushes to the base branch with no gate before
it.

### No switch that turns verification off
Not a flag, a mode or a config key that skips a task's criteria. A run that can finish without verifying anything
makes its receipt mean nothing. Speed comes from the workflow instead: a smaller one, or a mode that skips
deliberation, which never skips verification.

### No secret manager integration
Not an adapter for 1Password, Vault or a cloud's store. A secret reaches a session only as an environment variable
the config names under `secrets:`, and the manager a team already uses fills the environment before the binary runs
(`op run -- yunta run …`, `vault exec …`). An integration would be one adapter per provider, each with its own
authentication, and an engine that keeps credentials.
