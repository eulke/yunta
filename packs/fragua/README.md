# yunta/fragua

The full reference pipeline, end to end: an ambiguity-resolving grill, a
plan registered as a verified tasks document, implementation checked task by
task, a lint→fix cycle, a baseline check, a two-runner review, the work held
to the plan a person approved, and a PR.
Open modes throughout (`quick`/`standard`/`full`) and the plan distilled to
knowledge on finish. Installable and removable like any third-party pack —
the engine grants it no special status.

```bash
yunta pack add <source-of-this-pack>
yunta run yunta/fragua --input idea="add dark mode to the settings page"
```

`--mode quick` skips the plan's approval and the multi-runner review for a
fast pass, and still holds the work to its plan before asking you to ship it;
`standard` is the full human-in-the-loop cycle; `full` runs every node.

## Held to its plan

Green criteria say the work passes its tests, not that it is what the plan
says. Before `ship` asks you, `conform` — a read-only `reviewer` — reads the
plan, every departure from it a task's session declared with the answer it
got, and the diff of everything the run changed, and posts a finding for each
difference nobody accepted. `ship` shows you the plan, headed by the
departures you accepted, and those findings; in `standard` and `full`,
`fix-findings` works them first.

## Attaching the receipt

`yunta receipt <run_id>` can only run once a run is finished — never
from inside the run that produced it, since the run can't be "finished"
while one of its own nodes is still executing the receipt command. That's
why the `pr` node doesn't try to attach one itself. The recommended
pattern is a follow-up step, run by whatever drives this pack in CI:

```bash
yunta run yunta/fragua --input idea="..." --detach
# ... wait for the run to reach a terminal state ...
yunta receipt <run_id>
gh pr comment <pr-number> --body-file <run_dir>/receipt.md
```

Making the receipt a required PR check is a team decision Yunta doesn't
impose — configure it in your own forge, not in this pack.

## Installing

Your own `runners:` needs `planner`, `executor`, `mechanical`, `reviewer`
and `reviewer-alt` resolvable. `yunta init` writes most of the rest from
what it detects in your repository, and `yunta pack add` lists whatever is
still missing, with what it detected for it — `yunta doctor` says the same
later.

## What this pack asks of your project

The pack names capabilities; your config says how your project does each.

- **`commands.lint`** (optional): `lint` runs the command your project
  declares for it — `commands: { lint: "pnpm lint" }`, or
  `"cargo clippy --all-targets -- -D warnings"` — and a failing lint goes to
  `fix-lint` once before asking you. A project that declares no `lint` runs
  without both; `yunta status` says why they are not in the run.
- **`baseline.suite`**: the suite `tests` compares against, measured before
  the run's first node.
- **`forge.github`**: `pr` pushes the run's branch to `origin` and opens a
  pull request of it into your base branch, reading the token from the
  variable `token_env` names. A run is refused while the config declares no
  forge.
- **`docs/architecture.md`** (optional, recommended): `plan` reads it as
  context when it is there. Without it the session is told the file is
  absent and the planner explores the code on its own, which costs more
  tokens and plans with less of your intent. The run starts from your last
  commit, so commit the file before `yunta run`.

Nothing in the pack names a path of your repository: `fix-lint` may edit
what the run itself changed (`scope: run`), and a task that needs files
outside its scope asks a person (`scope_expansion.mode: ask`). Paths no
node may ever touch are yours to declare, under `permissions.paths.deny`.
