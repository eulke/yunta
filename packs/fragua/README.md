# yunta/fragua

The full reference pipeline, end to end: an ambiguity-resolving grill, a
plan registered as a verified tasks document, the tests its tasks are held
to written before any is built, implementation checked task by task, a lint→fix cycle, a baseline check, a two-runner review, the work held
to the plan a person approved, and a PR.
Open modes throughout (`quick`/`standard`/`full`) and the plan distilled to
knowledge on finish. Installable and removable like any third-party pack —
the engine grants it no special status.

```bash
yunta pack add <source-of-this-pack>
yunta run yunta/fragua --input idea="add dark mode to the settings page"
```

`--mode quick` skips the plan's approval, its tests, the multi-runner
review and the fixes it asks for, and still holds the work to its plan
before asking you to ship it;
`standard` is the full human-in-the-loop cycle; `full` runs every node.

## Tests before the work

In `standard` and `full`, `spec` writes the tests each task is held to
before any task is built, and `approve-plan` shows them beside the plan:
you approve what the work has to make pass. Each test fails when it is
handed over, a task closes only once its tests pass, and no session of the
run can change them. A test the session building a task believes is wrong
is a departure from the plan, and yours to settle: accepted, `spec` writes
that task's tests again and `approve-plan` asks you about them before the
task goes on. Sending the plan back with `adjust` writes its tests again
before you are asked again.

## Held to its plan

Green criteria say the work passes its tests, not that it is what the plan
says. In `standard` and `full`, two reviewers read the brief and the diff,
and `fix-findings` answers each finding they report — fixed, or declined and
why; a fix whose finding proposes a criterion is proved by it, which settles
the finding. Then, in every mode, `conform` — a read-only `reviewer` — reads
the plan, every departure from it a task's session declared with the answer
it got, and the diff of everything going into the pull request, and posts a
finding for each difference nobody accepted.

`ship` shows you the plan, headed by the departures you accepted, and every
finding the run holds, each with the node that found it and how it was
answered or settled. Going on settles what it showed.

## The receipt

The `pr` node opens its pull request with the run's receipt as its body: the
checks the run held its work to, each criterion with how it exited, and any path
written outside the scope — as the run stands when its last step begins. Once
the run is finished, `yunta receipt <run>` writes it again with how the run
ended, and `yunta verify <run>` checks its evidence.

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
- **`baseline.suite`**: the suite `tests` compares against and every task is
  held to. A run in a checkout of its own measures it in another checkout of
  the commit it started from while `grill`, `brief` and `plan` run; the loop,
  and a gate that shows the plan, wait for it.
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
