# Troubleshooting

Common ways a workflow, a run, or a pack refuses to do what you expected,
and what each one actually means. Most of these are deliberate refusals, not
bugs — Yunta prefers a named, actionable error over guessing what you meant.

## `yunta check` refuses the workflow

`check` validates statically, before any session opens. The page always
names the node and the exact problem, and a problem about the workflow file is
followed by the line it is about, with carets under the part at fault and what
helps fix it under them:

```text
✗ bad.yaml · 2 errors
┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄

  at           .yunta/workflows/bad.yaml

  error  node `lint`: `on_failure.goto` names `fixx`, and no node carries that
         id
   --> .yunta/workflows/bad.yaml:6:25
    │
  6 │     on_failure: { goto: fixx, max_reroutes: 1 }
    │                         ^^^^
    help: did you mean `fix`?

  error  node `fix`: unknown key `promt` for a `prompt` node
   --> .yunta/workflows/bad.yaml:9:5
    │
  9 │     promt: "Fix what lint reported."
    │     ^^^^^
    help: did you mean `prompt`? · `yunta schema workflow` lists every key

next
  yunta check .yunta/workflows/bad.yaml   again, once they are fixed
```

A name one or two typos away from one the workflow or the config declares — a
key, a kind, a node id, a runner, a command — is suggested in the `help:` under
the problem, in the plain text an agent or a pipe reads as on a terminal. Where a
problem is said on one line — a run tool's reply, a log line, a `yunta doctor`
row — the suggestion follows its sentence after a semicolon, and so does the one
for a key inside a `context:` source, a `files:` entry, an artifact reference or
an `on_finish` step. `yunta schema workflow` lists every key a workflow and each
kind of node read.
`check` reads past a key it does not know, and a kind one typo from a kind there
is, each as the one it most likely meant — a key only when the node does not
write that key already — so every problem in the file is listed at once, in the
order the file has them, and one round of edits fixes them all. `yunta run`
refuses a file with any of them.

- **``node `x` references runner `y`, which `runners:` does not define``** —
  add the runner under `runners:` in `.yunta/config.yaml` (see
  [adapters](adapters.md#configuring-runners)), or fix the typo in the
  node's `runner:`.
- **`cycle in depends_on: ...`** — the path is printed; break the cycle,
  there's no partial-order fallback.
- **``node `x`: `depends_on` names `y`, and no node carries that id``** (or
  `on_failure.goto`, a gate option's `on`) — a typo'd or removed node id.
  Every target must exist in the same workflow.
- **``node `x` references `{{inputs.y}}`, which `inputs:` does not declare``** —
  add `y` under the workflow's own `inputs:`, or fix the template reference.
- **a pack ceiling error naming the node, the pack, and both the declared
  and requested permission level** — a node inside a pack asked for more
  than that pack's `declares.permissions` promises. See
  [packs](packs.md#installing-and-using-a-pack) — the ceiling can't be
  exceeded from the consuming side; it's the pack's own manifest that has to
  change.
- **`node "x": `baseline_compare` has nothing to compare against ...`** (or
  `coverage_gate`, an unregistered executor, a session with no runner) — the
  node's kind cannot run without a key the config leaves unset. A run
  freezes its config, so it would stop there every time: declare the key
  (`baseline.suite`, `coverage`, `skills.executors`, `defaults.runner`)
  and check again.
- **``node `x`: it runs the project's command `lint`, and the config declares
  none``** — the node runs a
  project command by name (`run: { command: lint }`) and `commands:` does
  not name it. Declare what this project runs for it; when the repository
  answers for it, the next line says what was detected there
  (`detected here: declare `commands: { lint: "pnpm lint" }``).
- **`loop "x" works through the run's tasks document, and nothing ... gives it
  one`** / **`... reads the ... of node "y", which does not declare it`** /
  **`... reads the run's ..., and nothing can hold one`** — a read nothing in
  the run can answer, in the named mode when there is one. Declare the
  artifact on the node that writes it, bring it in as a `type: document`
  input, or mount it from the composing workflow.
- **`node "x" reads "path" (a `files:` context source) ... stops there every
  time`** — the commit the run starts from lacks the file and no node that
  runs before the reader can write it. Commit it, or declare the entry
  `optional: true`. When an earlier node might write it, this is a warning
  instead.
- **`node "x": `use: y` fails check — ...`** — a composed workflow its birth
  would refuse, reported before the parent spends anything.
- **`pack "p" requires ...`** — a pack workflow needs a runner, an MCP server
  or a command on `PATH` this project or machine lacks. `yunta doctor` lists
  the same gaps for every installed pack.
- **`yunta_schema: "..."` — ... (this binary speaks schema N)`** — the
  workflow (or an installed pack) declares a schema range your installed
  `yunta` binary doesn't satisfy. Check [compatibility](compatibility.md) for
  what's changed between schema versions.

## `yunta doctor` reports an adapter unhealthy

```text
  ✗ codex      unhealthy: `codex` is not on PATH
```

The diagnostic names the actual problem: a binary missing or not answering, a
version the adapter does not support, or a setting it does not read. Fix that
specific thing and re-run — `doctor` runs the identical probe `yunta run` runs
before spending anything, so a healthy `doctor` means the binary is there and
answers.

It does not mean its credentials hold or a session opens. The probe asks for a
version, which reaches no service and never touches the configuration a run
writes the CLI; a CLI that refuses that
configuration is healthy to a probe and dead to a run. `yunta doctor
--session` opens one real session per binding, asks it to submit an empty
`questions` document, and reports success only after the run accepts that
document and finishes. It costs a prompt per binding. See
[adapters](adapters.md#yunta-doctor).

If `doctor` reports a pack's `requires:` unmet (a role, an `mcp_servers:`
name, or a command not on `PATH`), it names the pack — add what's missing to
your own config, you don't need to touch the pack itself.

## A node failed with "session `<adapter>` exited with code N before any terminal event"

The CLI started and stopped without ever opening a session. The line carries
how its process ended and the last thing it wrote to stderr, which is
normally the whole answer: a configuration key it does not accept, a
credential it could not read, a flag it does not know.

`yunta status <run>` shows the rest of what the CLI said on its way out,
up to its last twenty lines. Values that came from the session's own
environment — the run tools' token among them — read as `[redacted]`.

`yunta doctor --session` reproduces it outside any run, once per binding, so
you can fix the configuration and check it without spending a workflow.

## `doctor --session` opened a session but received no questions document

`session opened, no questions document` means the CLI opened, but the probe
run never accepted its required `yunta_submit_questions` call. The probe
asks for `{"document":{"questions":[]}}`; this empty document needs no
human answer. Check the binding's tool access. For a workflow run that
shows the same symptom, `yunta status <run>` can show a failed
`yunta-run` call. Codex sessions need the per-run server's tool approval;
Yunta supplies that approval when it mounts the endpoint.

## A run reports a failed `yunta-run` call

The live chronicle shows each failed call, while `yunta status <run>` shows
the **last failed call of the attempt** of a node that has not finished, with
the tool name and either `approval_blocked` or `call_failed`. `call_failed`
means the run's tool server received the call and answered it with an error.
`approval_blocked` means the CLI refused the call before it reached that
server, for example at its approval step. The engine tells them apart by
whether the server saw the call, not by the CLI's own words. A node that
finished is past the calls it got wrong on its way. Its row says how its
document was accepted instead, for example "accepted on its 2nd handover
(1 refused)". The event log records every failed call as `run_tool_failed`.
`status --json` exposes the last one on the node as `last_tool_failure`,
with `session_id`, `tool`, and `cause`, whether the node finished or not. A retry starts a
new attempt and clears
that summary. The failed call is diagnostic context: the session may recover
and finish, and a node failure can have another cause. Arguments, responses,
tokens and CLI error text are not stored in this event or summary.

## A node failed with "scope violated: N files outside the declared globs"

The session edited something outside the node's `scope:` globs, and the
failure names each of those paths. This is a hard post-check, not a warning.

When the run pauses on it, the decision offers `grant` and `abort`:

- `grant` widens the node's scope by exactly those paths, for the rest of
  this run, and picks the same session back up in the checkout it worked
  in, with its work — told what was granted — rather than starting over.
  When the adapter cannot resume it, or that checkout is gone, a fresh
  session runs the node again and the log records a `capability_degraded`.
  Choose it when the edit was the right one.
- `abort` pauses the run.

The menu never offers `retry` for a scope failure. The same scope writes the
same paths again, so running the node again would only ask the same question.
`grant` is not offered to a `read-only` node, when a config layer sets
`permissions.scope_expansion.max_mode: deny`, or when those paths reach a path
the project protects. Then the menu offers nothing but `abort`, and the work has
to stay inside `scope:`. If the edit is legitimate for every run, widen `scope:`
in the workflow instead. See [scope and permissions](guide.md#scope-and-permissions).

## A node failed on an artifact it declared

```text
artifacts/plan/tasks.yaml: 2 errors
  task `graph-cmd`: `scope` is empty; every task declares at least one glob, the only paths it may touch
  task `graph-cmd`: `depends_on` names `t9`, which no task in this file declares
```

A node that declares a kind under `artifacts.produces` — `tasks`, `findings` or
`questions` — has to leave behind a document the engine can read. The heading names the file and how many
problems it has; each line below names one problem and the entry it belongs to, in
the document's own words. `yunta schema <document>` prints the shape the document is
read against.

For a `prompt` or `loop` node, the document arrives through a run tool and the
engine takes it into the run. A session that never handed one over closes the
node on a failure of its own, headed by the node rather than by a file:

```text
node `plan`: 1 error
  handed over no tasks document: produce it before the node ends, or stop declaring it here
```

No path is named because none was ever going to be there: such a document is
never a file on its way in, and a file left in the node's directory is not one
either, because no acceptance explains it. The run's log carries every submission
the session made, accepted or refused, under `artifact_submitted`. There is no
second session: a node that produces nothing fails once. Its menu offers `retry`
only once the run has seen the tree change since that attempt, which a
`yunta resume` after your change records, so a retry never repeats the same
attempt on the same tree.

For a `bash`, `check`, `gate` or `executor` node, the command writes the file
itself. Check that it writes the name the node declared, under
`{{node.artifacts}}` — the node's own directory, which the engine empties at the
start of every attempt.

A file that was never written, is empty, is past `limits.max_artifact_bytes`, or
that the filesystem refuses is reported as a failure of the file rather than of the
document, and names which of those it is.

## The engine refused a document or a finding a session offered

```text
The tasks document `tasks.yaml` was not accepted. Fix these and submit again:

  1. task `graph-cmd`: `scope` is empty; every task declares at least one glob, the only paths it may touch

  2. task `graph-cmd`: `depends_on` names `t9`, which no task in this file declares
```

This is the engine answering `yunta_submit_tasks`, `yunta_submit_spec`, `yunta_submit_questions`,
`yunta_post_finding`, `yunta_update_finding` or `yunta_withdraw_finding` inside the
session, with the verdict the node's close reaches. It is not a failure: the
session reads the numbered list, fixes exactly those problems, and calls the tool
again. A correction costs one call, not a session, and there is no limit on how
many times a session tries.

A document that reads into its kind is refused with every rule it breaks, all at
once. A document that does not read into its kind is refused with that one problem
and the path where it sits:

```text
  1. does not parse at `tasks[1].scope`: invalid type: string "src/**", expected a sequence
```

A value of the wrong type stops the read, and the rules only hold over a document
that parsed, so fixing the structure and submitting again is what surfaces them. A
refused finding leaves every other finding the node reported standing; only the one
in that call is rejected.

The refusal also lands in the run's log — `artifact_submitted` with a `refused`
outcome, or `finding_refused`, each carrying the whole report — so how often a run
gets a document wrong is a fact about the run and not something only the session
saw.

## A node's criteria never turn green

`yunta status <run>` shows which criterion is failing, its exit code and
the last line it printed. What a criterion prints never reaches your
terminal; the run keeps it. The last 20 lines of a red criterion are on its
`criteria_checked` event, which is what a task session reads through
`yunta_task`, and the whole output, redacted, is the object under the run's
`objects/` that the event's `output` names.

If the same criterion keeps failing across every reroute
(`on_failure.goto`) up to `max_reroutes`, the run pauses on a gate instead of
looping forever — that's expected, not a hang. Widen what the correction
node is allowed to see (`context:`) or change (`scope:`) before assuming the
criterion itself is wrong.

## `yunta run` refuses because git cannot name who commits

```text
a run commits every node's work, and git cannot name who commits here: …
```

Every node's work becomes a commit on the run's branch, so a repository where
git has no `user.name` and `user.email` — or is told not to guess them, with
`user.useConfigOnly` — would fail the run at its first commit. Set both, globally
or for this repository alone, and run again; `yunta doctor --full` says who git
commits as.

## `isolation: none` refuses to start

```text
`<path>` has uncommitted changes; isolation `none` requires a clean tree: without it, scope-by-diff can't tell the agent's work from what was already there
```

`isolation: none` runs directly against your working tree instead of a
fresh `git worktree`, so it requires the tree to already be clean, and
refuses a second concurrent run against it. Commit or stash first, or switch
to the default `isolation: worktree` if you don't specifically need to run
in place.

## A run looked stuck after the laptop slept

```text
run: host suspended for 1h52m, which durations leave out; new sessions wait until it has been awake 2m00s
```

The run noticed the machine slept and recorded it. None of its durations count
that time, and a session's timeout counts only the time the host is awake. When
the host wakes, the run does not open a new session until the host has been
awake for two minutes, because a laptop that wakes briefly for maintenance and
then sleeps again would leave a session hanging. Commands and checks don't
wait. If nothing moves after those two minutes, the run is waiting on something
else: check `yunta status`.

## A suite takes minutes longer than its tests on macOS

```text
  ▲ first run  a new binary's first run takes 307 ms here, its next 4 ms
    so         macOS assesses each new executable before its first run, and a
               suite pays that for every test binary it links again
    fix        add the terminal that runs yunta under System Settings > Privacy
               & Security > Developer Tools, then open it again
```

`yunta doctor` links a binary nobody ran before and runs it twice. macOS
assesses every executable it has not seen before letting it run, and a Rust
suite links one binary per test target: a run whose tasks change a base crate
links them all again, and pays that assessment once for each of them, outside
any test — on this repository, two hundred binaries and over three minutes per
suite. Add the terminal you run yunta from under System Settings › Privacy &
Security › Developer Tools, open it again, and run `yunta doctor` once more:
processes it starts are no longer assessed, and the first run costs what the
next one does.

## `yunta resume` says a run is already moving

```text
run Q78GRR is already moving: pid 91930 drives it
```

Another engine is driving that run. A run has one engine, because a second would
write the same log and work in the same checkout, so the refusal changes nothing.
`yunta status <run>` follows the run and `yunta cancel <run>` stops it; under
`--json` those commands are in `next`, and an agent calling `resume_run` reads the
same sentence. When that engine is asking at its terminal, the refusal says so
instead: answer it there.

An engine that is gone never holds a run: the next `yunta resume` replaces it,
also when another process got its pid.

Two other refusals name a file, because who holds the run cannot be told from here:

- `is held by pid N, a process this user cannot signal`: that pid runs as another
  user. It may be an engine that user started or, on macOS, where this user cannot
  read when another user's process started, a process that got the pid after the
  engine died.
- `has an engine lock with no readable owner`: the file is corrupted.

In both, if nothing drives the run, delete the file the sentence names and resume
it again.

## A run says it `needs you`

Not stuck — paused on a `gate`, waiting for a human decision, and it
survives the engine restarting. `yunta status <run>` shows what it's
waiting on and the command that chooses each option; `yunta resolve-gate
<run> <option>` answers it from any process, and `yunta resolve-gate <run>`
on a terminal puts the menu to you. See [gates from the
outside](guide.md#gates-from-the-outside).

## A run stopped on its token cap

A run meets `limits.max_tokens_per_run` in two ways. Both say `needs you`, and both
are answered the same way, from anywhere.

``node `grill` failed: exceeded max_tokens 200000 (285734 used)`` is a session cut at
its share. A session may spend the cap over the nodes still owed, cached tokens
included, so this comes long before the run has spent the cap. Its next attempt would
get the same share and fail the same way, so the menu offers `continue`, which runs
the node again past the cap, and no `retry`.

``its token budget is spent (2000431 tokens with `limits.max_tokens_per_run: 2000000`)``
is the run itself on the cap, with nodes still to run. The menu offers `continue`,
which goes on past it. A node whose `retry` you already chose keeps that answer: after
`continue`, that attempt runs.

`yunta status <run>` shows the decision and the command for each option. `yunta
resolve-gate <run> continue` answers it from any process, and `resolve_gate` over
`yunta mcp` does the same. The run goes on in a resume of its own, with nothing
attached to it. `abort` runs nothing and leaves the decision open.

One answer pays for one engine. `continue` lifts the cap for the engine that takes it,
with no new cap, so `yunta cancel <run>` is what stops that engine. The next engine
stands under the cap again: one that starts after a stop or a kill asks again when it
meets the cap. An engine that died before it started the attempt spent nothing, so the
answer still stands. A resume with no answer waiting asks at its terminal, and off a
terminal it stops again.

A parent whose child stopped on the cap has no option of its own to answer. `yunta
status <parent>` names the command that reaches the child. Answer there with `yunta
resolve-gate <child> continue`, and once the child has finished, `yunta resume
<parent>` takes it from there. That answer lifts the child's cap and not the parent's,
so the parent can stop on its own cap once the child's spend reaches it, and asks for
itself.

## A loop stopped on its iteration cap

``node `implement` failed: loop `implement` exceeded `limits.max_loop_iterations` (2)``
offers `retry`. `yunta resolve-gate <run> retry` starts the loop's count over, and it
goes on from the tasks already done. It stops at the cap again if tasks remain, so a
long plan takes one `retry` per round. A person at a terminal is asked before the loop
stops, and `continue` there keeps it iterating for that engine. The limit itself
stays what the run froze when it started: changing it takes a new run.

## A run says it `waiting`

A run says `waiting` when a limit's reset holds it, under the default
`defaults.on_failure: pause`. Its `run_paused` event carries
`"type": "awaiting_reset"`, the node the limit stopped and the time it resets,
and `yunta status <run>` says the run resumes on its own once the limit resets.
Nothing is asked of you, so `yunta run` and `yunta resume` exit 5, the code for
a wait. Exit 3 still means a person has to act.

The wait runs on its own. The detached resume the run started waits for that time
without holding the run's worktree, then continues. A `yunta resume` you run
before that time waits for it the same way, and says on the terminal that it is
waiting. A resume that wakes before the reset pauses again on the same reset. A
limit that names no reset time is not a wait: the run says `needs you`.

## A run says it `stopped`

A person stopped it: `yunta cancel` on the running run, `abort` chosen at a
gate, or a gate's pull request closed on its forge. Nothing is asked of anyone and
nothing is wrong with it: `yunta resume <run>` continues it where it stopped — a
gate asks its decision again, and one whose pull request was closed publishes
again, on the pull request if someone reopened it or on a new one — and
`yunta close <run>` closes it for good. A node whose attempt was open when its run
stopped says `stopped` too, and the resume starts that attempt again. A run that
was closed after a stop keeps the word, and nothing resumes it.

## A node failed and `resume` pauses on the same failure

A node with no `on_failure` reroute that fails leaves the decision to you:
`yunta status <run>` lists `abort`, and lists `retry` only when what the failure
waits on may have changed. A check that judges the tree (a coverage gate or a
baseline comparison), a missing artifact and an unmet criterion wait on the
run's tree: until the run has seen the tree move since the attempt, the menu
offers no `retry`, because the same check on the same tree gives the same
answer. Other failures, such as a credential, a model, a service or the
environment, offer `retry` and name what it waits for.

To make a fix count after a red check, make it in the run's own worktree
(`~/.yunta/worktrees/<run_id>` by default), not in your checkout: the run starts
from the commit it was created on and never sees files you add or change there
afterwards. Then run `yunta resume <run>`. The resume records the tree it finds,
and `yunta status <run>` then lists `retry`. Only then run
`yunta resolve-gate <run> retry`: the node starts a fresh attempt, and nothing
before it runs again. A `retry` chosen before that resume is refused, because the
menu does not offer it yet. A plain `yunta resume` with no choice asks the same
question again rather than spending on a retry nobody chose.

When the failure is a config key the node cannot run without — `baseline.suite`,
`coverage`, an executor's registration, a runner — the menu offers only
`abort`: the run's config was frozen when it was created, so no attempt of it
can go differently. Declare the key in `.yunta/config.yaml` and start a new
run; `yunta check` refuses the workflow until you do.

When a limit names the time it resets, such as an account's usage limit or a
forge's retry-after, the run waits for that time instead of asking you: see
[A run says it `waiting`](#a-run-says-it-waiting). Without a reset time the run
asks you, and `retry` waits for the service to accept requests again.

A loop task that ends an attempt with its criteria red is `blocked` right
away: another session on the same task and tree would have nothing the first
did not. One exception: a scope request still pending under a red criterion
gets one fresh session first, once per cycle, which may close the task without
it. A request the task leaves unanswered when it
closes is withdrawn, and nobody is asked about it. The decision is yours —
`retry` once what it failed on has changed, `continue-work` when its attempt
left work behind, or `grant` when it asked for scope. After an answer to a
task's scope request, granted or denied, the session the task's cycle ended in
picks its work back up where it left it; if that work
already closes the task, no session opens at all. That answer is not the node's own
decision about its failure, so it never runs the node again as a `retry` would.

## A task stopped by a red guard

A `guard` criterion was red before the task did any work. The pre-check ran it on
the tree the task starts from and it failed, so the task is `blocked` before any
session opened. The stop names each guard, and says what the guard is there to
show when the criteria say so:

```text
guard `cargo test -p billing` is already red before any work started: it is there to show that totals match the invoices
```

The menu offers a `retry` that waits for what the guard reads. Once the log has
seen the run's tree move since the node failed (a `yunta resume` after your change
records the move), the `retry` waits for the tree. Until then it waits for something
outside the tree, such as a service coming back or a file you provide.
`yunta status <run>` lists it, and `yunta resolve-gate <run> retry` chooses it.

A `retry` asks the red guard again and keeps what passed: a criterion that passed
on the same tree is not run again. When several stops wait for different inputs,
the menu still offers one `retry`, and its label names each input they still wait
for.

A criterion that already passes before any work offers no `retry` for its task,
whatever else the pre-check found. The stop says the criteria need fixing, not the
task, and only the plan can change them.

## `yunta pack add`/`update` refuses

- **`publisher "x" is not in permissions.packs.publishers.allow`** — your
  config (the layer is named in the error) restricts which publishers can be
  installed. Add the publisher there, or get the pack from an allowed one.
- **`this pack declares N executors ...`** — `permissions.packs.executors`
  is `prompt` (the default) and needs `--yes` after you've reviewed the
  printed audit, or is `deny` and refuses outright regardless of `--yes`.
  See [packs](packs.md#installing-and-using-a-pack).

## `resume` says the run is broken because of an artifact

```text
run is broken: run `01J...` no longer holds the bytes its log accepted for 1
of the 3 artifacts it names: `artifacts/plan/tasks.yaml`: object
`a1b2...` holds content that hashes to `c3d4...`: the bytes under
`objects/` are not the bytes the run accepted
```

Waking a run reads back every artifact its log accepted, because the log
names bytes by their hash and the run keeps them under `objects/`. That
message means one of those objects is gone, or its content no longer hashes
to its own name — somebody edited or replaced a file under `objects/`, or
the filesystem lost part of it. The run stops before doing any further work:
its own history says it holds something it can no longer hand to a node.

What to do:

- **Put the bytes back.** If the run's directory came from a backup or a
  copy, restore `objects/` from it — the object's name *is* its sha256, so
  any copy of the right content is the right object, wherever it comes from.
- **Editing an artifact is not how you change one.** The file under
  `artifacts/` is a view the engine writes and never reads; deleting it is
  harmless, and editing it changes nothing. `objects/` is the run's
  evidence, and nothing outside the engine writes there.
- **If the bytes are gone for good**, the run cannot be resumed — its
  artifacts are part of what it is. Start a new run from the same inputs.

`yunta verify <run>` reports the same check on demand, without resuming.

A run created by a Yunta older than the object store reports instead that it
holds artifacts this binary cannot verify — a `minor` finding, not a break.
That log names files rather than objects; see [compatibility](compatibility.md).

## `resume` says the run is broken because of its worktree

```text
run is broken: run `01J...` works in `/home/me/.yunta/worktrees/01J...`, whose
HEAD `9f1c...` no longer has the run's base commit `4a77...` behind it: every
task, scope check and criterion this run's log records was established against
a tree that this one is not a continuation of. Put it back on the run's own
branch (`git -C /home/me/.yunta/worktrees/01J... checkout yunta/run/01J...`), or
on any commit that still descends from `4a77...` (`git -C
/home/me/.yunta/worktrees/01J... reflog` shows where the run left it). If those
commits are gone for good, start a new run against the tree as it is now.
```

Waking a run asks its worktree two questions, and this is the second one
failing. The run branched from a commit, and everything on its log — each
task marked done, each `scope_checked`, each green criterion — was
established against a tree descending from it. A `git reset --hard` behind
the run's own commits, a rebase, or a checkout of an unrelated branch takes
that commit out of the tree's history, and from then on the state derived
from the log describes a tree that is not there. The run stops before doing
any more work on it.

What to do:

- **Put the tree back.** `git -C <worktree> reflog` shows every commit that
  tree has been on, including where the run left it. `git -C <worktree>
  checkout yunta/run/<run_id>` returns it to the run's own branch; any commit
  that still descends from the base commit works.
- **Under `isolation: none`** the run works directly on your checkout, so a
  `git pull --rebase` or a rebase while the run was paused is the usual way
  this happens. The remedy is the same — the reflog, then back onto a commit
  that still descends from the base.
- **If that history is gone for good**, this run's is too: start a new run
  against the tree as it is now. The work in the tree is not lost by this —
  only the run's claim to have verified it.

**What is *not* a problem: a changed tree.** New commits on top, or
uncommitted edits you made by hand during the pause, are the expected case
and the run resumes over them without a word. The worktree is the work; its
content is never verified against a snapshot. What answers a changed tree is
the criteria memo, which is keyed on a hash of the tree, so the run re-runs
its criteria against what is there instead of trusting a result about a tree
that is gone.

## `resume` cannot find the run's worktree

```text
the run's branch `yunta/run/01J...` has no worktree at
`/home/me/.yunta/worktrees/01J...` (there is nothing at that path); the
run's history and artifacts are intact; bring the checkout back with `git
worktree add /home/me/.yunta/worktrees/01J... yunta/run/01J...`, run from the
repository the run was created in, and resume again
```

The first of the two questions. The run is not broken: its event log and the
objects under `objects/` — everything that is its evidence — are untouched,
and git still holds its branch with every commit the run made. Only the
checkout is missing, and the command in the message brings it back exactly.
Run it from the repository the run was created against (the one whose
`.git/worktrees/` holds the entry), then resume.

If the run has `isolation: none` the message is different — it says the
directory is not a git working tree at all. That run works on the checkout it
was created in rather than on one of its own, so resume it from there.

## Something looks corrupted, or a replay disagrees with what you remember

```bash
yunta verify <run>
```

Checks a run's two mechanical guarantees and reports them apart:

- **the event chain** — every link recomputed from the log as persisted, so
  an altered payload or a deleted, inserted or reordered event is named with
  the seq it begins at;
- **the objects** — every artifact the log accepted, read back and hashed
  against its own name.

The two are independent: a corrupt object leaves the chain intact, and an
altered event leaves the objects alone. Either one failing exits non-zero.
This is the mechanical way to confirm (or rule out) tampering or corruption —
never guess from `status` output alone if you suspect this.

## Still stuck

`yunta <command> --help` is the source of truth for flags — this doc and the
[workflow guide](guide.md) cover behavior, not every flag. If a run's
behavior doesn't match anything here, `yunta status <run>` — and
`yunta status <run> --node <id>` for one node whole — are derived straight
from the event log and are the most reliable place to start.
