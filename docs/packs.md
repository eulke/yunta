# Packs

A pack is a distributable, versioned bundle of workflows, skills, knowledge
and docs — never something that extends the engine itself, only content it
already knows how to run. A pack **declares** the roles it needs (a role name
plus a permissions ceiling, e.g. `reviewer` at `read-only`) and its own
permissions ceiling (`declares:` in `pack.yaml`) — never a concrete adapter,
model or secret; the installing team resolves those roles against its own
`runners:`, so the same pack runs unedited on a team that's all Claude Code
and one that's all Codex.

Nothing in the [workflow guide](guide.md) requires a pack — packs are a
distribution mechanism for sharing workflows and knowledge across projects,
not a prerequisite for writing one.

This repo ships two example packs at [`packs/`](../packs/) — `yunta/starter`
(two minimal workflows: a one-node `fix` and a fan-out `review`) and
`yunta/fragua` (the full reference pipeline: grill, a verified tasks document,
lint→fix, a baseline check, multi-runner review, PR). Both install and remove
like any third-party pack; the engine treats them no differently, and
they're worth reading as concrete, working examples of everything below.

## Installing and using a pack

```bash
yunta pack add github.com/acme/review-pack@v1.2.0
yunta pack list
yunta pack update acme/review-pack v1.3.0
yunta pack remove acme/review-pack
```

`add` clones the ref, vendors it to `.yunta/packs/<publisher>/<name>/`
(checked into the repo alongside the team's own code — a pack's contents are
versioned with your history, not fetched fresh on every checkout), and
records `{ref, commit, content hash}` in `.yunta/yunta.lock`. Nothing updates
itself: `update` always names an exact target ref. `list` re-hashes what's
actually vendored on disk against the lock and says so if they've drifted —
an offline, no-network way to confirm the vendoring hasn't been tampered with
or gone stale.

Once installed, a pack's workflows and skills are addressable by
`publisher/name` — the name after the slash is the workflow or skill's own
file basename (as declared in `pack.yaml`'s `contents:`), not the pack's own
`name` field, so a publisher's installed packs share one flat namespace:

```bash
yunta run acme/review
yunta check acme/review
```

and the same form works inside a workflow (`use: acme/qa-review`) and a
node's `skills:` list (`skills: [acme/review-rubric]`). Resolution always
tries the repo's own `.yunta/workflows/` first — a repo file at the same
`publisher/name` path always wins over the pack: a local workflow with the
same name shadows the one from the pack. `yunta list` reflects the same
two-layer view and the same shadowing.

Composition is scoped to a pack's own contents: a workflow shipped inside a
pack may freely `use:` another workflow from the *same* pack, but reaching
into a different pack, or back out to the repo, is rejected by `check` —
cross-pack composition is out of scope for v1.

A pack is code from someone else, and it can be audited by reading it:
`yunta pack audit acme/review-pack` prints a full static inventory of every
workflow it ships — every `bash`/hook/loop command, every context source and
exactly what it points at, permissions and required agent per node, `mcp`
servers reached, executors flagged as code, and each workflow's **complete,
untrimmed prompt text**. It's inventory, never a verdict: nothing here flags
content as "suspicious" — that would be trivially evadible and would only
give false confidence. It also reports whether the pack ships its own tests
under `.yunta/tests/` (same format `yunta test` uses) and whether they pass.
`add` prints the same inventory before anything is vendored — nothing lands
in your repo unseen — and runs nothing of the pack on its own: the pack's
cases run only with `--run-tests`, after the install, so reading the audit
and confirming it never executes what is being audited. A pack that ships
a symlink, or whose `pack.yaml` names a publisher, pack or content path that
would reach outside the pack's own directory, is refused before vendoring.

`declares:` in `pack.yaml` is a ceiling, not a description, and `yunta check`
enforces it as one: a `prompt`/`loop` node inside a pack can never request a
session permission above what that pack's manifest promises — including a
node that declares no `permissions:` of its own, which still falls back to
the engine's own `edit` default and can exceed a `read-only` ceiling just the
same. Exceeding it fails `check` with an error naming the node, the pack and
both the declared and requested level; the same rule follows composition, so
a child workflow reached through `use:` from inside the pack is checked
against that pack's ceiling too. Declaring any `executors:` raises the bar
further, and how far is the installing team's own call:
`permissions.packs.executors` in the config (an org-ceiling setting — lower
layers only narrow it, never re-widen it) decides. `prompt`, the default,
refuses to install or update a pack that ships executable code unless
`--yes` confirms it, after the audit has shown exactly what the executors
are; `deny` refuses outright — no flag overrides a permissions ceiling;
`allow` installs without asking. `permissions.packs.publishers.allow`, when
non-empty, additionally restricts which publishers can be installed or
updated at all — a refusal names the config layer that declares the
restriction. `update` enforces both the same way `add` does: a new ref is
where new executor code first appears.

`requires:` is the mirror image of `declares:` — a floor the *installing*
team's own config must clear, not a ceiling the pack promises. `yunta doctor`
validates every installed pack's `requires:` against the merged config
alongside its usual adapter health check — see [adapters](adapters.md#yunta-doctor).
Nothing here blocks `pack add` or `check` — a pack can be installed and
configured for later, same as an adapter that isn't set up yet doesn't stop
`yunta init`.

Starting a run from a pack's own workflow freezes exactly which pack version
produced it — publisher, name, the pack's own semver, and the exact commit
`yunta.lock` recorded, all in the run's manifest from the moment it's
created. `yunta pack update` afterward changes nothing about a run already in
flight: `resume` only ever re-reads that manifest, never the vendored pack on
disk again, the same immutability every other frozen field (`workflow`,
prompts, config) already has.

## Creating a pack

A pack is a directory with a `pack.yaml` manifest at its root, plus whatever
workflows, skills, knowledge and docs it lists. `yunta pack new
<publisher>/<name>` scaffolds one at `./<name>` — a manifest, one verified
`example` workflow, a self-test config and case, and a README — then checks
and runs it, so the scaffold passes from the first command. Or copy the
shape of [`packs/starter`](../packs/starter) (the smaller of the two
examples) and adjust:

```
my-pack/
  pack.yaml
  README.md
  .yunta/
    workflows/
      review.yaml
    tests/
      review.yaml
      fixtures/
        review.yaml
```

### `pack.yaml`

```yaml
name: review-pack
publisher: acme
version: 1.0.0
description: "A two-runner review workflow with a written rubric."
license: Apache-2.0
declares:
  permissions: read-only
  network: false
  executors: []
requires:
  runners:
    - name: reviewer
    - name: reviewer-alt
contents:
  workflows:
    - .yunta/workflows/review.yaml
  docs:
    - README.md
```

Field by field:

- **`name`/`publisher`** — the pack's identity, invoked as
  `publisher/name` everywhere (`yunta run acme/review`, `use:
  acme/qa-review`). Kept as two fields because they're independently
  meaningful: `permissions.packs.publishers.allow` matches on `publisher`
  alone, so every pack you publish under the same `publisher` shares one
  trust decision on the installing side.
- **`version`** — semver, for humans to read; nothing in `yunta pack`
  compares versions automatically (`update` always names an exact target
  ref), so this is a label you control, not something the tooling resolves
  against.
- **`declares`** — the ceiling your pack promises never to exceed.
  `permissions` is the highest `read-only|edit|full` any node inside the
  pack will ever request. Set it to the *lowest* value your workflows
  actually need — `check` enforces this as a hard ceiling on every
  `prompt`/`loop` node, including ones that don't set `permissions:`
  explicitly (which still fall back to `edit`). `executors` lists any
  executable code you ship (empty means the pack is 100% declarative YAML
  and prompts, auditable by reading) — a non-empty list makes `add`/`update`
  require confirmation on the installing side by default.
- **`requires`** — the floor the installing team's config must provide:
  `runners` (the runner names your workflows use in `runner:`, optionally
  with the permission profile you expect them resolvable at), `mcp_servers` (names
  your workflows reference under `context: { mcp: ... }`), `commands`
  (binaries your `bash`/hook steps assume are on `PATH`). None of this is
  enforced at install time — `yunta doctor` reports gaps, naming your pack,
  so an installing team can fix them before running anything.
- **`contents`** — every path your pack ships, relative to the pack's own
  root: `workflows`, `skills`, `knowledge` (a pack can be knowledge-only —
  an empty `workflows`/`skills` with a non-empty `knowledge` is a legitimate
  pack, e.g. an org-wide style guide with nothing to run), `docs`.

### What to keep out

- **No adapter, model, or secret anywhere in the pack.** A workflow names
  roles (`runner: reviewer`); the installing team's own `.yunta/config.yaml`
  resolves those. A pack that hardcodes `runner: claude-code` couldn't run on
  a Codex-only team, which defeats the point of shipping a pack at all.
- **No composition outside the pack's own contents.** A workflow inside your
  pack can `use:` another workflow from the same pack; reaching into another
  pack or back out to the installing repo is rejected by `check`.

## Testing a pack before sharing it

Give your pack its own `.yunta/config.yaml` — read only when the pack's own
tests run in isolation, never once it's vendored into someone else's project
(a project only ever resolves its *own* root config, never a nested pack
directory):

```yaml
runners:
  reviewer:
    - { adapter: mock, model: mock-model }
  reviewer-alt:
    - { adapter: mock, model: mock-model }
```

Then write cases under `.yunta/tests/`, same format any repo's own tests use
— see [`packs/starter/.yunta/tests/`](../packs/starter/.yunta/tests) for a
working pair:

```yaml
# .yunta/tests/review.yaml
workflow: review
fixture: fixtures/review.yaml
expect:
  final_state: finished
```

A workflow that declares `modes:` or required `inputs:` gets both from the
case — `mode: standard` and `inputs: { idea: "add dark mode" }` — see
[`packs/fragua/.yunta/tests/`](../packs/fragua/.yunta/tests) for mode-specific
cases.

Every case runs in a fresh, empty repository. A workflow whose nodes read
files (`files:`), take a `path` input or run the project's own toolchain
declares `worktree: <directory>` (relative to the case file): the directory's
contents become the sandbox's initial commit before any session starts, so
a scope diff only ever shows what the sessions changed.

Run them from inside the pack's own directory, or from anywhere with `--dir`:

```bash
cd my-pack
yunta test
yunta test --dir path/to/my-pack
```

This runs entirely against `mock` (see [adapters](adapters.md#mock-not-a-test-helper-a-first-class-adapter))
— no LLM, no network, deterministic — the same way the engine's own test
suite runs. `yunta pack audit` runs these same cases too and reports whether
they pass, so a clean `yunta test` locally is exactly what a consumer sees
with `yunta pack add --run-tests`.

To see the full audit inventory — and exercise the real install path — before
publishing anywhere, `yunta pack add` accepts a local path (it's still a
`git clone` under the hood, so the source just needs to be a git repo, not a
pushed one):

```bash
cd some-scratch-project
yunta pack add ../my-pack
```

## Publishing a pack

There's no pack registry yet (v1 doesn't have one) — a pack's source is
whatever `yunta pack add` can `git clone`: a bare `host/path` shorthand
(`github.com/acme/review-pack`, `https://` is assumed), a full URL, an SSH
shorthand (`git@github.com:acme/review-pack`), or a local path. "Publishing"
today means: push the pack's repo somewhere reachable, and tag a release.

A few conventions worth following, since consumers will `add`/`update`
against exactly what you do here:

- **Tag every release** (`v1.2.0`, matching `pack.yaml`'s own `version`) —
  `update` always names an exact ref, so an untagged pack forces every
  consumer to track a branch instead of a stable point.
- **Bump `pack.yaml`'s `version` in the same commit you tag** — `yunta pack
  audit`/`add` print this version straight from the manifest; a mismatch
  between the tag and the manifest is confusing for anyone reading the
  output.
- **Keep the pack's own repo root *at* the pack root** — `pack.yaml` at the
  top level, exactly the layout `add` clones and vendors verbatim. If a pack
  lives inside a larger monorepo, publish it from a repo (or subtree) whose
  root already looks like `packs/starter/` does in this one.
- **Write the `README.md` you listed in `contents.docs`** — it's the first
  thing a `pack audit` reader (and a human deciding whether to `add` you)
  sees referenced; describe what the pack does and what roles a consumer
  needs to fill, not how it was built.

Consumers then install exactly what you published:

```bash
yunta pack add github.com/acme/review-pack@v1.2.0
```
