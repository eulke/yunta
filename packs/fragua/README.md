# yunta/fragua

The full reference pipeline, end to end: an ambiguity-resolving grill, a
plan registered as a verified task ledger, implementation checked task by
task, a lint→fix cycle, a baseline check, a two-runner review, and a PR.
Open modes throughout (`quick`/`standard`/`full`) and the plan distilled to
knowledge on finish. Installable and removable like any third-party pack —
the engine grants it no special status (D57).

```bash
yunta pack add <source-of-this-pack>
yunta run yunta/build-feature --input idea="add dark mode to the settings page"
```

`--mode quick` skips the two human gates and the multi-runner review for a
fast pass; `standard` is the full human-in-the-loop cycle; `full` runs every
node.

## Attaching the receipt

`yunta receipt <run_id>` can only run once a run is finished (D54) — never
from inside the run that produced it, since the run can't be "finished"
while one of its own nodes is still executing the receipt command. That's
why the `pr` node above doesn't try to attach one itself. The recommended
pattern is a follow-up step, run by whatever drives this pack in CI:

```bash
yunta run yunta/build-feature --input idea="..." --detach
# ... wait for the run to reach a terminal state ...
yunta receipt <run_id>
gh pr comment <pr-number> --body-file <run_dir>/receipt.md
```

Making the receipt a required PR check is a team decision Yunta doesn't
impose (D54) — configure it in your own forge, not in this pack.

## Installing

Your own `runners:` needs `planner`, `executor`, `mechanical`, `reviewer`
and `reviewer-alt` resolvable, and `baseline.suite` configured for the
`tests` node's `baseline_compare` check — `yunta doctor` says so if
something's missing.
