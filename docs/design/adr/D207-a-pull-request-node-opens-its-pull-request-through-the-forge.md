---
number: D207
title: "A pull_request node opens its pull request through the forge"
status: accepted
revises: []
revised_by: []
---

# D207 — A `pull_request` node opens its pull request through the forge

## Context

A workflow opened its pull request with a `bash` node:
`git push -u origin {{run.branch}}` and `gh pr create --fill`. That tied every
pack shipping one to the GitHub CLI and to whatever login that CLI held on the
machine, pushed whatever the push command named, and opened a second pull
request whenever the node ran again. The engine already speaks to a forge for
external gates, through a port with a GitHub implementation and a mock.

## Decision

1. **`kind: pull_request`** (`title`, optional `body`, both templates) pushes
   the run's own branch and opens a pull request of it into
   `project.base_branch`, or the branch the run started from when the project
   names none.
2. **The push is git's; the pull request is the forge's.** The engine pushes
   the run's branch to the forge's remote (`forge.github.remote`, `origin` when
   absent) and never forces. The forge port opens the pull request with the
   run's marker in its body, and a later call for the same run and branch
   answers with the open one it finds — a node that runs again never opens a
   second, and a closed or merged pull request is never reused.
3. **The log records it**, as `pull_request_opened { url, number, head, base }`,
   written as soon as the forge answers: a pull request is an effect outside
   the run that exists even when the node fails afterwards.
4. **What it needs is refused before any token is spent.** A missing `forge`
   and a run with no branch of its own (`isolation: none`) are keys the node
   cannot run without — refused by `check`, or the node left out when it is
   optional. A forge whose token variable is not set on the machine refuses
   `yunta run` before the run exists; `yunta doctor` says whether the token
   reaches the repository and may push, and whether the remote is that
   repository.
5. **A test case never reaches a real forge**: `yunta test` stands a mock in for
   the one the config declares, as the mock adapter stands in for every runner.

## Rejected alternatives

**A project command for opening the pull request.** Each project would script
its own call to its forge's CLI, the rerun would open a second pull request,
and nothing could record the pull request as a fact.

**Publishing through the gate path, which commits files over the API.** A pull
request of the run's code needs the run's own commits on the branch, which only
a push carries.

## Consequences

The network use belongs to the engine: a pack whose manifest declares
`network: false` still states the truth about its own commands. A second forge
is another implementation of the port, and no workflow changes for it.
