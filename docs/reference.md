# Reference

Every key of the config and of the workflow language, each with a value a run
accepts. The [workflow guide](guide.md) says how to use them; this page says what
each one is, and the comments in each file say what its value means.

- [The config](#the-config)
- [A workflow](#a-workflow)
- [A workflow built from others](#a-workflow-built-from-others)
- [Nodes the examples above leave out](#nodes-the-examples-above-leave-out)
- [Connecting an MCP client](#connecting-an-mcp-client)
- [Notes](#notes)

## The config

A config layer is one YAML file. `yunta check` and every run read the layers
merged, the most specific winning key by key.

```yaml
# Every key a config layer takes, each with a value. Three layers merge, the
# most specific first: the repo's .yunta/config.yaml (committed, the team's),
# the user's ~/.yunta/config.yaml (credentials, local paths) and the
# organization's /etc/yunta/config.yaml (optional defaults). A key the more
# specific layer sets wins, and a list is replaced whole, never joined.
version: 1

project:
  name: my-repo
  base_branch: main                 # the branch work merges back into
  branch_prefix: yunta/             # what a run's own branch name starts with

forge:                              # where a run opens its pull requests
  github: { repo: acme/web, token_env: GITHUB_TOKEN, remote: origin }
                                    # token_env names the variable holding the token, never
                                    # the token; remote is the git remote a run pushes to

runners:                            # the names nodes ask for, each an ordered list of candidates
  planner:
    - { adapter: claude-code, model: claude-opus-4-8, effort: high }
    - { adapter: codex, model: gpt-5-codex, effort: high }
                                    # effort: low | medium | high | xhigh, how long the model
                                    # reasons; absent leaves it to the CLI, and a node's wins
  executor:
    - { adapter: claude-code, model: claude-sonnet-4-6 }
  reviewer:
    - { adapter: claude-code, model: claude-sonnet-4-6, agent: benito }
  reviewer-alt:
    - { adapter: codex, model: gpt-5-codex }
  mechanical:
    - { adapter: claude-code, model: claude-haiku-4-5 }

adapters:
  claude-code:
    binary: ~/.local/bin/claude     # a local override, kept in the user layer
  codex:
    binary: /usr/local/bin/codex
    adapter_settings: { sandbox: workspace-write }
                                    # what an edit session's commands may write: workspace-write
                                    # (the default), read-only, or danger-full-access, which
                                    # needs the node's network open

defaults:
  runner: executor                  # the runner a node without `runner:` uses
  isolation: worktree               # worktree | none; a workflow node says the same word
  timeout_minutes: 45               # the time a session may run while the host is awake
  max_parallel_nodes: 4             # independent nodes the scheduler runs at once
  on_failure: pause                 # pause | abort | continue, for a node with no re-route
  on_interrupt: restart_node        # restart_node | fail_if_uncertain | resume_session

skills:
  paths: [.yunta/skills, ~/.yunta/skills]   # where skill names resolve, in order
  always: [conventions]             # mounted on every session, before a node's own
  executors:                        # what an `executor` node runs, by name
    - { name: coverage-gate, kind: binary, path: .yunta/bin/coverage-gate }
    - { name: license-scan, kind: binary, path: .yunta/bin/license-scan }

mcp_servers:                        # servers the `mcp` context source can query
  internal-docs: { url: "https://docs.internal.example/mcp", auth_env: DOCS_TOKEN }

commands:                           # what the project runs for each capability a workflow names
  lint: "cargo clippy --workspace -- -D warnings"
  fmt: "cargo fmt"
  format: "rustfmt --edition 2021"

format:                             # which command formats which files as a session writes them
  - { paths: ["**/*.rs"], run: format }   # the first rule that covers a file; it gets the paths

shared_dirs:                        # caches every command and session of a run shares
  PIP_CACHE_DIR: ~/.cache/yunta/pip # exported under the variable, outside the worktree;
                                    # writable by every session that is not read-only

baseline:
  suite: "make test"
  held: [lint]                      # commands measured beside the suite: one that passes on
                                    # the base holds every task as a guard, as the suite does

coverage:
  cmd: "cargo llvm-cov --summary-only"
  threshold: 90

storage:
  path: ~/.yunta/yunta.db           # the run log, SQLite in WAL mode
  retention_days: 90
  pool:                             # what the project's checkouts may keep of the disk
    budget: 60G                     # bytes (80G, 500M, 1.5T) or a share of the disk (20%)
    keep_free: 10%                  # what of the disk always stays free

paths:                              # where state lives; `YUNTA_HOME` overrides both
  runs: ~/.yunta/runs
  worktrees: ~/.yunta/worktrees

permissions:                        # a ceiling: a more specific layer only narrows it
  commands:
    allow: []                       # empty runs whatever is not denied; a list allows only it
    deny: ["curl * | *", "sudo *"]
  paths:
    deny: [".github/**"]            # no run writes these, whatever its scope
  packs:
    executors: prompt               # allow | prompt | deny
    publishers: { allow: [acme] }
  network:
    default: true                   # for a node that does not say; read-only never has it
  scope_expansion: { max_mode: ask }   # rules | ask | deny: the widest a node may ask for

limits:
  max_tokens_per_run: 2000000       # the run's tokens, settled and in flight
  max_loop_iterations: 12
  max_concurrent_runs: 3
  max_workflow_depth: 4
  max_artifact_bytes: 50000000      # a guard against runaway artifacts
  inline_context_bytes: 32000       # past this size, context is mounted by reference
  max_expansion_files: 5            # files a rules-mode expansion may name before it is denied

pricing:                            # optional; without it stats and the receipt count tokens only
  claude-opus-4-8: { cost_per_1k_tokens: 0.015 }
  claude-sonnet-4-6: { cost_per_1k_tokens: 0.003 }

secrets:                            # names of environment variables, never their values
  - GITHUB_TOKEN
```

## A workflow

A workflow lives in `.yunta/workflows/<name>.yaml` and runs as `yunta run <name>`.
This one shows every node kind but `workflow` and `parallel`, which the next one
shows.

```yaml
# A workflow that takes a feature from its brief to a pull request, written to
# show every node kind but `workflow` and `parallel` and every key a node takes.
name: build-feature
description: Complete feature with grill, verified tasks document, multi-runner review and PR
yunta_schema: ">=1 <2"              # the schema versions it is written for; absent, the binary's
inputs:                             # each input has a type, and only that type's fields
  idea:
    type: string
    required: true
    description: "What to build; it becomes the brief"
    min_length: 10
  ticket:
    type: string
    pattern: "^[A-Z]+-[0-9]+$"
    default: WEB-1
  audience:
    type: enum
    values: [internal, public]
    default: internal
  max_tasks:
    type: number
    min: 1
    max: 20
    default: 8
  notes:
    type: path
    default: docs/architecture.md

modes:                              # their names and number are the author's, and their order
                                    # is the ladder a run is promoted along
  quick:
    include: [grill, brief, plan, implement, lint, fix-lint, tests, ship, pr]
    ask: required                   # which question rounds stop the run: always (the default)
                                    # or required; a node's own `ask` wins
  standard:
    include: [grill, brief, plan, approve-plan, implement, lint, fix-lint, tests, coverage,
              licenses, review, fix-findings, findings, ship, pr]
  full: { include: all }

node_defaults:
  hooks:
    before:
      - { run: "cargo fetch", on_failure: warn, timeout_seconds: 120 }
    after:
      - run: "cargo fmt"
  cli_config: isolated              # inherit (the default) | isolated: whether sessions load the
                                    # CLI configuration of the person who runs them; a node may
                                    # say its own

nodes:                              # an id is a letter, then letters, digits, `_` or `-`
  - id: grill
    kind: prompt
    runner: planner
    skills: [grill]
    prompt: |
      Identify the ambiguities in "{{inputs.idea}}" and write the questions
      needed as an artifact; do not converse.
    artifacts:
      produces: [questions]         # a node that asks declares nothing else; it may say
                                    # ask: always | required

  - id: brief
    kind: prompt
    runner: planner
    depends_on: [grill]
    context:
      - artifact: { node: grill, kind: questions }
      - artifact: { node: grill, kind: answers }
      - files: [{ path: "{{inputs.notes}}", optional: true }]   # optional: done without it
    prompt: |
      Write the brief for "{{inputs.idea}}" from the questions and their
      answers, for {{inputs.audience}} readers, sized for at most
      {{inputs.max_tasks}} tasks.
    artifacts:
      produces: [brief.md]

  - id: plan
    kind: prompt
    runner: planner
    effort: xhigh                   # over the effort of whichever candidate resolves
    permissions: read-only
    depends_on: [brief]
    context:
      - artifact: { node: brief, name: brief.md }
      - knowledge: { layers: [repo, user] }
      - files: ["docs/architecture.md"]
      - command: "git log --oneline -20"
      - mcp: { server: internal-docs, query: "{{inputs.idea}}" }
    prompt: { file: prompts/plan.md }   # or the prompt inline, as a string
    artifacts:
      produces: [tasks]

  - id: approve-plan
    kind: gate
    depends_on: [plan]
    assignee: lead
    message: "Plan recorded. Do you approve?"
    options: [approve, adjust]
    on: { adjust: plan }            # asks what to change, and resumes plan's session with it
    shows: [{ node: plan, kind: tasks }]   # the plan approved, bound to its hash

  - id: implement
    kind: loop
    runner: executor
    depends_on: [approve-plan]
    until: all_tasks_complete
    invariant: true
    concurrency: 2                  # tasks at once, with disjoint scopes; the default is 1
    scope_expansion:                # deny when absent
      mode: ask
      max_per_run: 3                # counts only the grants the engine decides
    prompt: |
      Read your task in the tasks document. Implement it within its scope.

  - id: lint
    kind: bash
    invariant: true
    optional: true                  # without commands.lint, the run goes on without this node
    network: false                  # no network for this node; absent, permissions.network decides
    depends_on: [implement]
    run: { command: lint }
    on_failure: { goto: fix-lint, max_reroutes: 1 }

  - id: fix-lint
    kind: prompt
    runner: mechanical
    context: [{ node-output: { node: lint } }]
    prompt: "Fix only the errors in the report."
    scope: run                      # what the run has changed so far

  - id: tests
    kind: check
    builtin: baseline_compare       # fails when something that passed on the base stops passing
    invariant: true
    depends_on: [lint]

  - id: coverage
    kind: check
    builtin: coverage_gate          # runs coverage.cmd, and fails under coverage.threshold
    depends_on: [tests]

  - id: licenses
    kind: executor
    executor: license-scan          # declared under skills.executors
    with: { deny: [AGPL-3.0] }      # handed to it as JSON on stdin; its verdict is JSON on stdout
    timeout_seconds: 600
    depends_on: [coverage]

  - id: review
    kind: prompt
    depends_on: [licenses]
    runners: [reviewer, reviewer-alt]   # one node per runner, before anything runs
    agent: auditor                  # over the agent each candidate names
    permissions: read-only
    prompt: "Audit the changes and report each finding."
    artifacts:
      produces: [findings]

  - id: fix-findings
    kind: prompt
    runner: executor
    depends_on: [review]
    context:
      - run-events: { filter: findings }   # the findings as data, never as prose
      - tasks: {}                   # the tasks document, each task with its status
    prompt: "Fix the blocking findings; document each dismissed one with its reason."

  - id: findings
    kind: check
    builtin: findings_gate          # fails while a finding no proof or person settled
    max_severity: blocking          # stands at this severity or above
    depends_on: [fix-findings]

  - id: ship
    kind: gate
    depends_on: [findings]
    assignee: lead
    message: "Create the PR?"

  - id: pr
    kind: pull_request              # pushes the run's branch and opens the PR on the forge
    depends_on: [ship]
    on_interrupt: fail_if_uncertain # cut off mid-push, it fails as uncertain, never pushes again
    title: "{{inputs.idea}}"
    body: "Closes {{inputs.ticket}}."
    receipt: true                   # the body carries the run's receipt

on_finish:
  - cleanup: worktree
  - distill: [{ node: plan, kind: tasks }]
```

## A workflow built from others

A `workflow` node runs another workflow as a sub-run of its own, and a
`parallel` group runs its children at once.

```yaml
# A workflow built from others: a release of two features, each built by the
# reference workflow, between a design review and a QA pass.
name: release-cycle
inputs:
  rfc:
    type: path
    required: true
    description: "The RFC to review"
  feat_a:
    type: string
    required: true
  feat_b:
    type: string
    required: true
nodes:
  - id: design
    kind: workflow
    use: design-review              # a workflow the catalog holds, by its name
    inputs: { rfc: "{{inputs.rfc}}" }

  - id: approve-design
    kind: gate
    depends_on: [design]
    assignee: architecture

  - id: build
    kind: parallel
    depends_on: [approve-design]
    join: all                       # all | any: done when every child is, or at the first
    nodes:
      - { id: feat-a, kind: workflow, use: build-feature, inputs: { idea: "{{inputs.feat_a}}" } }
      - { id: feat-b, kind: workflow, use: build-feature, inputs: { idea: "{{inputs.feat_b}}" } }

  - id: qa
    kind: workflow
    use: qa-review
    depends_on: [build]

  - id: ship
    kind: gate
    depends_on: [qa]
    assignee: lead
```

## Nodes the examples above leave out

A gate a review on the forge answers, instead of a person at the console:

```yaml
- id: summary
  kind: prompt
  runner: planner
  prompt: "Summarize the design the review settled, for the people who approve it."
  artifacts:
    produces: [design-summary.md]

- id: approve-design
  kind: gate
  depends_on: [summary]
  assignee: architecture
  message: "Approve the design"     # the pull request's title
  external:                         # the artifacts are committed to `branch`, and a pull
    kind: pull_request              # request opens there; approving or merging it lets the
    artifacts: [design-summary.md]  # run go on, and closing it fails the gate
    branch: "review/{{run.branch}}"
```

A child workflow that works in its parent's tree and is born holding one of its
artifacts, and a group whose children share a blackboard:

```yaml
- id: notes
  kind: prompt
  runner: planner
  prompt: "Write the release notes."
  artifacts:
    produces: [notes.md]

- id: qa
  kind: workflow
  use: qa-review
  depends_on: [notes]
  isolation: none                   # worktree (the default) gives the child a tree of its
                                    # own; none has it work in the tree this node works in
  mounts:                           # copied into the child's artifacts when it is born
    - artifact: { node: notes, name: notes.md, as: release-notes.md }

- id: polish
  kind: parallel
  depends_on: [qa]
  join: any                         # done at the first child that succeeds; the rest stop
  coordination: blackboard          # independent (the default) | blackboard: the children post
                                    # to the group's blackboard, read only once the group joins
  nodes:
    - { id: by-planner, kind: prompt, runner: planner, prompt: "Tighten the notes." }
    - { id: by-reviewer, kind: prompt, runner: reviewer, prompt: "Tighten the notes." }
```

## Connecting an MCP client

```json
{ "mcpServers": {
    "yunta": { "command": "yunta", "args": ["mcp"] }
} }
```

A client launches `yunta mcp` as a subprocess and talks to it over stdio. Its
tools, and the `yunta-run` server the engine opens for each session of a run,
are listed in the guide's [MCP](guide.md#mcp) section. So that a client's agent
knows when to reach for them, `yunta init` installs a mechanism skill in the
repository and offers a line for the team's `CLAUDE.md`. The skill never holds
the catalog of workflows: it asks `list_workflows`, which is always current.

## Notes

- **`artifacts.produces`** is a list of plain strings. A string that names a
  kind (the set is closed: `tasks`, `findings` and `questions`) declares that
  document, and any other string is the name of an opaque file. A node produces
  at most one document of each kind, so the kind identifies the document and
  there is no name to choose: declaring a kind twice is refused by `yunta
  check`, and the three kind names are reserved as file names. Declaring a kind
  is all a node needs to receive that document's shape in its context.
  `yunta schema <kind>` prints the shape, and `--json` its JSON Schema.
- **`inputs:`** maps a name to its spec, with `type:` choosing the shape:
  `string`, `number`, `boolean`, `enum`, `path` or `document`. Each type takes
  only its own fields: `default` and `required` for every type, `min_length`
  and `pattern` for a string, `min` and `max` for a number, `values` for an
  enum. A `document` also declares the `kind:` the run reads the file as: the
  run is born holding it as an artifact of its own, with no node that produced
  it, and `{{inputs.<name>}}` renders its identity (`sha256:<hash>`), never its
  path.
- **An artifact reference** (a `context: [{ artifact }]`, an entry of `mounts:`
  or of `on_finish.distill`) names the artifact by what identifies it, exactly
  one of `kind: <kind>` for a document the engine reads and `name: <file>` for
  an opaque one. A mount's `as:` renames an opaque artifact, and the child holds
  it under that name; a document is identified by its kind in every run that
  holds it, so `as:` beside a `kind:` is refused when the workflow is read.
- **Template variables** are what a node may write between `{{ }}` in its
  prompt, its `run:`, its hooks and its `context:` patterns. The set is closed:
  `{{run.dir}}`, `{{run.worktree}}`, `{{run.branch}}`, `{{run.base}}` (the
  commit the run started from, so `git diff {{run.base}}` is everything the run
  changed), `{{run.staging}}` (the root each node's staging sits under),
  `{{node.artifacts}}` (the node's own directory, where it writes what it
  declares), `{{node.id}}`, `{{runner.name}}` when the node declares a runner,
  `{{project.name}}`, `{{project.base_branch}}` and `{{project.branch_prefix}}`
  as `project:` declares them, and one `{{inputs.<name>}}` per declared input.
  A name outside the set is no variable: reading the template refuses it and
  names the ones that exist. A variable of the set that nothing defines where
  it is used fails the node, naming it, and `yunta check` refuses every
  `{{inputs.<name>}}` that `inputs:` does not declare.
- **`skills:` and `context:`** stay apart. `context:` brings data, what to work
  on, through the [context sources](guide.md#context); `skills:` mounts
  instructions and capabilities, how to work, through the adapter's own
  mechanism.
- **`agent:` on a node** takes the place of the agent its runner names, for that
  node. The name resolves to the project's `.yunta/agents/<name>.md`, or to the
  definition a pack declares under `contents.agents` (`publisher/agent`): front
  matter with `name` and `description`, and the body as instructions. The same
  definition runs on every adapter, and a name with no definition fails the node
  before its session opens.
- **`effort:`**, on a runner's candidate or on a node, whose own wins: `low`,
  `medium`, `high` or `xhigh`, the levels every CLI accepts. `claude-code`
  receives it as `--effort` and `codex` as `-c model_reasoning_effort`, a
  resumed session included; a level only one CLI has (`max`, `minimal`) is
  refused when the YAML is read. The level asked for is recorded in the
  `agent_session_opened` event's `effort`.
