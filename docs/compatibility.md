# Compatibility policy

This is the published guarantee for what changes, and what doesn't, across a
Yunta release. It exists so a team adopting Yunta can answer "will upgrading
break our workflows or our in-flight runs" without reading the changelog
line by line.

## The binary: semver

The `yunta` binary and the `yunta-core`/`yunta-storage`/`yunta-adapters`/
`yunta-engine` library crates follow [semver](https://semver.org/):

- **Patch** (`0.1.x`) — bug fixes, no behavior a workflow author could have
  depended on changes.
- **Minor** (`0.x.0`) — new node kinds, context sources, CLI flags, adapter
  capabilities. Additive; nothing that worked before stops working.
- **Major** (`x.0.0`) — anything that isn't additive: a removed flag, a
  changed default, a stricter `check` that now rejects something it used to
  accept.

Before `1.0.0`, minor bumps may still include breaking changes — the same
convention every pre-1.0 Rust crate uses — but the same three-tier reasoning
applies to decide which digit moves.

## The workflow schema: `yunta_schema`, versioned independently

A workflow's own format is versioned separately from the binary, via the
`yunta_schema:` field (a semver range, e.g. `yunta_schema: ">=1 <2"`) that
`yunta check` validates against the binary's own compiled schema version
(`yunta_core::YUNTA_SCHEMA`). This is deliberate: **upgrading the binary
must never retroactively invalidate a workflow a team already wrote and is
running in production.**

The engine supports the current schema version and the one immediately
before it (**N and N-1**). A schema bump (a new required field, a changed
node-kind shape) ships in a minor or major release alongside that release's
own notes on what changed and how to migrate; workflows on the outgoing
version keep working for one more schema generation before `check` starts
rejecting them, with an error that names the field and the fix.

## In-flight runs are never affected by an upgrade

A run's [manifest](guide.md) — workflow, config, inputs, and resolved
runners — is hashed and frozen at creation. Upgrading the `yunta` binary mid-run, or between `yunta run`
and a later `yunta resume`, changes nothing about how that run's remaining
nodes execute: `resume` replays the run's own event log against its own
frozen manifest, never against whatever the newly-installed binary would
generate today. The only thing a new binary version can change for an
existing run is how `status`/`stats`/`graph` and the live view a `run` draws
*render* information already in the log — never the log's content or the run's
outcome.

Waking a run does verify what it holds: `resume` reads back every artifact
the run's log accepted, from the object the log names it by, and refuses to
go on when one is gone or its bytes no longer hash to their own name. That
check covers a run whose log records artifacts as `artifact_accepted` — the
current form. A log written before a run kept the bytes of its artifacts
under `objects/` records them as `artifact_written`, naming a hash with no
object behind it: those artifacts are counted and reported as ones this
binary cannot check — a `minor` finding on the run and a line in `yunta
verify` — and the run resumes. A newer binary never makes an older run
unresumable by asking of it a guarantee its own format could not give.

Waking a run also checks the worktree it works in, and checks something else
there: that a git working tree is still at the path the manifest froze, and
that the commit the run branched from is still behind that tree's HEAD. The
content of the tree is never checked against a snapshot — the tree is the
work, and changing it between a pause and a resume (new commits, a fix made
by hand, a build left behind) is the system working as intended. A tree whose
history has lost the run's base commit — a `reset --hard` behind the run's
commits, a rebase, another branch checked out — leaves the run broken with a
diagnostic; a missing checkout is an error naming the command that brings it
back, not a broken run. Neither depends on the binary's version: both are
asked of the run's own frozen manifest.

## What every release verifies before it ships

Per the release pipeline's `test` gate (`.github/workflows/release.yml`):

- `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`,
  `cargo fmt --all --check`, `cargo deny check` — all green.
- The factory packs (`packs/starter`, `packs/fragua`) pass `yunta check` and
  their own `.yunta/tests/` cases against the `mock` adapter
  (`yunta test --dir <pack>`).
- Each of the four published binaries (Linux x86_64 and aarch64 as static
  musl binaries, macOS x86_64 and aarch64) installs and runs in a clean
  environment for its target platform, and `yunta doctor` is the first
  command the installer suggests running.

None of this is negotiable on a per-release basis — a release that doesn't
pass all of it doesn't ship.

## Scope globs

A scope pattern — a node's or task's `scope`, an expansion's `within` — reads
with a literal separator: `*` never crosses a `/`, so `src/*.rs` names the
files directly under `src/` and `src/**` names everything beneath it. A
workflow that expects `*` to descend into subdirectories writes `**`.

## Identifiers

Every identifier is checked when it is read, and a value that breaks its
rule is refused with the value, what it was meant to be and the rule:

- A node id, a runner name, a mode name, a task id, a question id, an
  adapter id and an executor name are a letter followed by letters, digits,
  `_` or `-`. A fan-out sibling the manifest expands `runners:` into adds
  `@` and its runner's name; authored YAML never spells that form.
- A model name and an agent name are one printable word without whitespace,
  as the adapter's CLI accepts them.
- A run id, a publisher and a pack name are one path segment: printable,
  without whitespace, `/` or `\`, and not `.` or `..`. A pack reference is
  `publisher/name`. Every run yunta creates — a `yunta run`, a child of a
  `kind: workflow` node, a promotion successor — gets a ULID as its id: 26
  Crockford base32 characters that sort by the instant of birth. The link
  between a parent and its child, and between a run and its promotion
  successor, is recorded in the log (`child_run_created`, `promoted_from`),
  never encoded in the id.
- A finding id is any printable label; a session id is whatever the
  adapter's CLI issued, as long as it is not empty.

A pack manifest names the runners it needs under `requires.runners`. The
`runner_resolved` event names its runner under `runner`; the log reader also
accepts `role`, the field's former name, so a log written under it still
replays. `stats --json` and the JSON receipt name the same value `runner`.

## What this system writes down

Five files outlive the command that wrote them: a run's frozen manifest, the pack
lock, a run's process registry, the isolation lock and the receipt. Each carries
`schema_version`, and each is read the same way.

A file stamped with a schema this binary does not know is refused, naming what it
found and what this binary reads — a manifest interpreted under a shape its own
creator did not write is a run whose history would mean something else. A file
stamped lower, or carrying no version at all because it predates one, reads as it
is: every field this binary needs is either there or optional.

A key this binary does not know is kept rather than dropped, and written back
when the file is rewritten — so an older binary that reads a file a newer one
wrote does not silently delete what the newer one recorded. A reader that wants
to say what it did not understand can name those keys; `yunta status` does.

The receipt is the exception to being read back: nothing reads a `receipt.json`,
because `yunta receipt` derives it from the log every time. Its `schema_version`
is for whoever consumes the file outside yunta, who has no log to derive it from.

## The event log

The engine hands storage a draft — what happened, in which run, for which
node — and storage assigns the event's position (`seq`, from 1) and its
timestamp from the injected clock; no caller invents either.

A binary reads every log a newer binary wrote. An event under a `kind` this
binary does not know is kept as written — its kind, the version it was
written under and every field — and the run is interpreted up to what the
binary understands: replay never breaks on it, `yunta status` and the
receipt name the unknown kinds with their counts, `stats --json` lists them
under `unknown_kinds`, and `events.jsonl` carries the event back out
verbatim (with its `schema_version` beside the envelope fields). An event
under a known kind whose payload is not that kind's shape is corrupt, and
reading the run fails naming its position.

`node_failed` records what failed, not a sentence about it. Its `failure` is
either `outcome:` — one sentence the engine states, for a failure with no
artifact behind it — or `artifacts:`, one entry per declared artifact that did
not close. An entry is one of four: the file itself (`artifact-missing`,
`artifact-empty`, `artifact-oversized` with both numbers, `artifact-unreadable`),
a document nobody handed over (`artifact-undelivered`), which carries the node
that declared it and the identity that node owes, its content — the path, the
kind whose shape it was read against, and every problem that document has — or
an artifact no run holds (`artifact-unheld`), which carries the run that owes it,
the node of that run it was asked of when the reference names one, and the
identity it was asked for. Only the first names a path: a document that arrives
through a submission tool and a node of composition write no file, so their
entries name none. Each content problem names
its subject in the document's own words — ``task `t1`, criterion 1`` — which is
the whole of where it is: a diagnostic carries no line and column, and
`events.json` publishes none. Every surface renders from that value — `status`
and the receipt — so none of them can disagree about the facts, and nothing has
to take a sentence apart to recover them.

A finding's `location` is where it is: a path with the lines of it when the
finder named them (`src/lib.rs`, `src/lib.rs:142`, `src/lib.rs:142-150`). The path
is relative and never climbs out of what it is under, because a findings document
is inherited by a successor run that need not be on this host — an absolute path
is that host's, not the run's. A bare path is in the worktree, which is what every
finding an agent writes is about; the engine's own findings about the run's
bookkeeping carry the prefix `run:` and are relative to the run directory
(`run:scratch/engine.json`). A location that does not read is refused where it is
read, as a `parse` problem at its own key (`findings[0].location`).

A report names the document it is about: its `path`, and its `kind` — one of the
artifact kinds, or `workflow` for the file a run is created from. A workflow is
read the same way every other document is, so a graph that breaks its own rules —
an id declared twice, a reference that reaches nothing, two `parallel` children
that can touch the same files, a mode that leaves the graph unable to run —
reaches a reader as the same report a tasks document does. A diagnostic's subject,
under `of`, names the entry the problem is about; for a workflow that is `node`.

A problem is one of two shapes, under the key `problem`. `parse` carries `message`
and, unless the root itself is at fault, the `path` of the value that stopped the
read (`tasks[1].manual_review`); its stable code is `parse`. `rule` carries a
`code` from a closed set and the `detail` a reader acts on. A node fails on an
artifact with `retryable: false`: there is no second session to instruct, so
nothing about the failure asks for one.

The rules a document can break, which is that closed set: `duplicate-id`,
`empty-title`, `empty-scope`, `no-criteria`, `all-criteria-are-guards`,
`unknown-dependency`, `dependency-cycle`, `overlapping-scope`,
`manual-review-without-justification`, `empty-text`, `empty-detail`,
`unknown-id`, `withdrawn-id`, `empty-reason`, `missing-values`, `missing-answer`,
`mismatched-answer` and `incoherent-mode`. Together with
`parse` and the six an artifact fails under, they are every stable code this
system reports: a receipt counts by one, `status --json` publishes one, and a log
is grepped by one.

A log whose `node_failed` events carry `outcome:` on its own — every log written
before `artifacts:` existed — reads back as exactly that one-sentence failure:
no migration, nothing inferred. That tolerance is the rule for everything the
engine persists and reads again. The other direction is the general rule above:
a payload a binary does not recognize as that kind's shape is corrupt to it.

Because each document travels with its own problems, a count says which document
each came from: the receipt counts a problem by its code together with the kind of
document it was found in, and a failure of the file itself, which has no document,
counts by code alone.

`task_status_changed` carries `commit` on a `done`: the commit the run's tree
stood at once that task's work was in it. No other status carries one, and a log
whose `done` events name none — every log written before the field existed —
reads back as exactly that. The field is what lets another run tell whether its
own tree has the work, so a `done` that names no commit crosses to no run: that
run registers the task and does it again, rather than assuming a tree it cannot
ask about.

An artifact kind is read under its current name and under the one it had. The
tasks document is `tasks`; a log — `artifact_accepted`, `artifact_written`,
`artifact_submitted` — or
a frozen manifest that spells it `task-ledger` reads as `tasks`, and so does a
workflow's `kind: task-ledger` or a `ledger: {}` context source. What the binary
writes is always the current spelling.

## Documents a session hands over, and findings it reports

`artifact_submitted` records a whole document a session offered and what the engine
answered: `name` (the document's view name, `<kind>.yaml` — the node declared the
kind, so the call names nothing), `artifact_kind` (named in full so it does not
collide with the envelope's own `kind`), and `outcome` — either `accepted` with the
`content_hash` of the canonical bytes the run stored, or `refused` with the whole
`report`. Both are kept: how often a run gets a document wrong is a fact about the
run, not something only the session saw.

Findings carry three accepted forms and one refusal. `finding_posted` carries the
whole `finding`; `finding_updated` carries the whole finding again, under the same
id, as its new state; `finding_withdrawn` carries the `id` and the `reason` its node
gave. `finding_refused` carries the `operation` (`post`, `update` or `withdraw`), the
`report`, and the `id` the call named when it named one that parses — the field is
absent otherwise.

Which findings a run holds is the last state of each `(node, id)` pair, minus the
withdrawn ones, in the order each was first posted. A log that carries only
`finding_posted` folds to every finding it posted, in posting order: a fold with no
update and no withdrawal to apply has nothing to change. The same fold ignores a sequence the engine never writes: an
update or a withdrawal for an id its node never posted, or a post on an id it
withdrew. A log that carries one came from somewhere else, and the honest reading of
it is the state it can account for.

`yunta list --runs` orders runs by the timestamp of their first event.

`yunta list --runs` groups runs by what can be done about them — what needs a
person, what is in flight, what has closed, and last the runs whose log or
manifest does not read back. The first three groups are ordered by how long a
run has been where it is, longest first; two runs that have been there equally
long are ordered by run id, which for a minted one is the order they were
created in. A run in the last group has no derived state to have been in, so
that group is ordered by run id alone.

## The JSON surfaces

`stats --json`, `status --json` and `run --json` carry `schema_version: 4`. The
three share one stamp, so all of them carry the new number even though only the
run document changed shape.

`run --json`, `resume --json`, `status --json` and the control plane's
`workflow_status` all emit one document. It is derived from the run's own event
log, so the command that drove a run to its stop and the command that reads that
run afterwards publish the same answer, field for field. `outcome` is the word
every text surface prints for the run — `created`, `running`, `paused`,
`finished`, `failed`, `cancelled`, `promoted`, `broken` — so a reader who greps a
terminal for what `status` said finds the same word in the document. A failed or
broken run carries `reason`; a parked one carries `waiting_on` and, when its
pause reconstructs a menu, `decision`. `yunta run --detach --json` publishes that
same document for the run it just handed off, which its log calls `created` or
`running`: no surface reports an outcome of `detached`, because detaching is
something an invocation did and not a state a run is in.

`stats --json` publishes what a run handed over and what it found beside what it
spent: `submissions` (`{accepted, refused}`) counts every document offered,
`findings` (`{posted, updated, withdrawn, refused}`) counts every finding call the
log carries, and `findings_standing` counts the findings that stand now — the
fold over the whole log, where an update replaces and a withdrawal removes. Each
node row carries its own `submissions` and `findings`, zeroed for a node the log
carries none from.

In `status --json`, `diagnostics` maps a failed node to the artifacts its failure
names — `{"<node>": [{code?, path?, kind?, file?, run?, producer?, artifact?,
diagnostics?}, ...]}`, one entry per artifact. Every field is absent when the
failure has nothing to put there, so no consumer meets an invented path: only a
failure the close opened a file for carries `path`. `code` is the stable name of
what is wrong with the artifact itself — `artifact-missing`, `artifact-undelivered`,
`artifact-unheld` — and is absent for a content failure, whose problems each carry
a code of their own. A content failure carries `kind`, the artifact kind whose
shape the content was read against, and `diagnostics`, every problem that document
has in document order. A file-level failure carries `file` instead, naming what
went wrong with the file itself: never written, empty, past
`limits.max_artifact_bytes`, or refused by the filesystem. A document a node ended
owing carries `producer`, the node that owes it, and `artifact`, the identity it
owes. One no run holds carries `artifact` too, plus `run` — the run that was asked
— and `producer` when the reference named a node of that run. A node whose most
recent failure is a plain message has no entry at all, so what the field shows is
always the state the node is in now.

## The MCP servers

Yunta serves MCP in two places: the per-session tool server the engine starts on
loopback HTTP for one node's session, and the control plane `yunta mcp` serves over
stdio. Both announce every protocol revision the SDK implements — `2024-11-05`
through `2026-07-28` — and both serve all of them from one set of handlers.

Every result either server builds satisfies the newest revision it announces. A
list result carries the cache hints `2026-07-28` makes mandatory (`ttlMs: 0`,
`cacheScope: "private"`) and the `resultType` discriminator; a client of an earlier
revision ignores the fields it does not know, which is what lets one result answer
both eras. `ttlMs: 0` is not a placeholder: a session's tool list is built per
session and per node, so it is stale the moment it is read and a client that caches
must ask again.

The two lifecycles both work. A client of `2026-07-28` sends `server/discover` and
then calls straight away, carrying its protocol version, client info and client
capabilities in each request's `_meta`; a client of an earlier revision opens with
`initialize`, and against the HTTP server carries the `Mcp-Session-Id` it is given
on every later request. Which tools a per-session server lists depends on that
session — its node, its task and the documents it declares — never on the revision
the client speaks.

`run --json` carries `budget_warning` when the declared cap sits under the
workflow's historical p90 — the same sentence that goes to stderr, undecorated,
because how a caution looks is the terminal's word and not the document's.
Absent otherwise, and always absent from `resume --json`: the estimation belongs
to whoever *creates* a run.

`status --json` carries `waiting_on` for a parked run, tagged by `on`:
`{"on": "node", "node", "external_ref"?, "reason"?}` when a node is parked on a
person, `{"on": "run", "reason"}` when the run itself stopped. `summary` says the
same thing inside a sentence that also carries the run's counters; this is the
pause on its own.

A parked run's `decision.evidence` is a list of the facts the engine attached,
each `{label?, value}` — the escalation as the log holds it, not the lines a
reader was shown. A fact that names itself, like a failing command's `exit 1`,
carries no `label`.

In `status --json`, `diagnostics` maps a failed node to the documents its failure
names — `{"<node>": [{path, kind?, diagnostics?, file?}, ...]}`, one entry per
file. `path` is always there. A content failure carries `kind`, the artifact kind
whose shape the file was read against, and `diagnostics`, every problem that
document has in document order. A file-level failure carries `file` instead,
naming what went wrong with the file itself: never written, empty, past
`limits.max_artifact_bytes`, or refused by the filesystem. A node whose most
recent failure is a plain message has no entry at all, so what the field shows is
always the state the node is in now.

In `status --json`, `decision` carries what a parked run is waiting on:
`{node, summary, evidence, options: [{id, label, tradeoff}], external_ref?,
resolve_with}` — the escalation under the same field names the `gate_waiting`
event writes, plus the node it belongs to and the command that answers it with
the option left as `<option>`. Two pauses reconstruct one: a node whose
re-routes are exhausted, and an unresolved internal gate. Every other pause — a
budget cap, a scope expansion, an unanswered questions artifact, an external
gate with no reachable forge — carries no `decision` at all, and `summary` says
what the run is waiting on instead. The `decision` field itself is additive; the
stamp moved to `3` for the shape of `decision.evidence`, described above.

## Message wording

The block that reports what is wrong with a document counts in whole words —
`1 error`, `2 errors`. Every surface that prints it says it the same way: an
interpreted artifact that could not be read, a workflow that fails `yunta check`
(``the workflow fails `yunta check`: 2 errors``), `yunta new`, `yunta pack new`.

`yunta run` and `yunta resume` refuse an unhealthy adapter with ``adapter health
check failed (run `yunta doctor` for detail): 2 errors`` — the advice sits inside
the parenthesis so the count lands directly after the heading.

`yunta run`'s live view needs a terminal, and where there isn't one it says so on
its first line and prints one line per event instead: `live view off (<reason>):
one line per event`, the reason being `stderr is not a terminal`, `TERM=dumb` or
`NO_COLOR is set`. The same shape as the line above — what is off, why in the
parenthesis, what happens instead after the colon. `--quiet` announces nothing,
because it has no view to stand down.

The `document_shape` tool refuses an unknown kind with the same sentence
`yunta schema` prints, byte for byte.

A run tool that refuses something a session offered opens with what was not accepted
and which call to make again, then lists the problems numbered from 1, one per
paragraph. The heading names the document by the noun of its kind — `tasks document`,
`findings artifact`, `questions artifact`:

```
The tasks document `tasks.yaml` was not accepted. Fix these and submit again:
The finding was not accepted. Fix these and post it again:
The finding update was not accepted. Fix these and update it again:
The withdrawal was not accepted. Fix these and withdraw it again:
```

A call the engine cannot read as a call at all is answered by naming what that tool
takes instead:

```
invalid submission — requires `document` (an object), the `tasks` document this node declares: `document` is missing or is not an object
`notes.md` is not an artifact this node declares; it declares `tasks`
node `plan` declares no artifacts, so there is nothing to check
```

A run the binary could only interpret in part counts the same way on both surfaces
that report it: `2 unknown event kinds, interpreted partially: <kind> ×<count>, …`,
and `1 unknown event kind` for one. `yunta status` folds that into its
`·`-separated summary; `yunta stats` gives it a line of its own.

`yunta test` closes a case with what it found: `case <name> ... FAILED: 2 errors`,
`case <name> ... ERROR: 1 error`, and `case <name> ... ok` on its own. The tally
under them counts cases — `4 cases, 1 failed`, `1 case, 1 failed` — and carries no
error count, unlike every other heading that introduces problems: `failed` counts
cases while the lines beneath it count problems, and one failing case contributes
several, so a count there would put two different totals on one line. Each count
stays with what it counts.

`yunta pack add --run-tests` and `yunta pack audit` report that same tally in their
tests section — `tests: 4 cases, 1 failed`, or `tests: 4 cases shipped, not run
(pass --run-tests)` when nothing ran, or `tests: none shipped` when the pack ships
no cases. Failure lines sit two spaces in under it.

`yunta pack audit` prints a node's `prompt:` block even when the prompt file is
empty: a blank line under the heading, an empty block that shows it is empty.

## The schemas as files

`crates/core/schemas/` holds `workflow.json`, `config.json`, `pack.json`,
`tasks.json`, `findings.json`, `questions.json` and `events.json`: the JSON
Schema (draft 2020-12) of a workflow file, a config layer, a pack manifest, the
three artifacts the engine interprets, and one event of the log — the shape of a
line of `events.jsonl`. They are generated from the types that read those
documents: `cargo xtask schema` writes them and CI fails when a committed file
differs from what the types emit, so any change to a format is a visible diff in
the pull request that makes it. They live inside the crate whose types produce
them, which is also the crate that ships them: the binary embeds those exact
files, so `yunta schema <kind> --json` prints the bytes CI checked rather than
deriving a schema of its own at run time. An editor or a validator can use the
files as they are, with or without a checkout.

`tasks.json` is the schema of the tasks document; `yunta schema task-ledger`
still answers with it, as an alias of `yunta schema tasks`, and the JSON Schema
itself lists `tasks` alone as the kind's spelling.

## Platforms

Yunta builds and is published for Linux and macOS. The engine's process
layer — every subprocess in its own process group, interrupted and killed
with its whole tree, lock liveness checked by signal — is POSIX, and
`yunta-engine` refuses to compile for any other target rather than ship a
binary that would leave processes behind on cancellation. Windows becomes a
target when a process layer with the same guarantees exists and its
cancellation tests pass on a Windows runner.

A lock's holder is asked about by signal on every platform, and a holder
that cannot be asked about — a process of another user — keeps its lock.
On Linux the holder's start time, read from `/proc`, also tells a reused
pid from the holder, so a newcomer that got a dead holder's pid cannot keep
its lock. macOS publishes no `/proc`: there liveness alone decides, and a
pid reused while the lock stands keeps it until that process ends.

## What isn't covered here

Individual adapters (CLI integrations like `claude-code`, `codex`) have
their own version compatibility against the coding-agent CLI they wrap; see
`yunta doctor`, which checks the installed binary's version against what the
adapter supports. Pack compatibility (a pack's own `declares:`/`requires:`
against a given Yunta version) is the pack author's responsibility, checked
statically at `pack add`/`check` time — this document covers the engine
itself, not third-party content distributed through it.
