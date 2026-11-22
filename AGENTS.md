# Working in Yunta

Yunta runs YAML workflows: declared criteria, judged by the engine, decide when work is done, and replay derives
every state from the event log.

## Where knowledge lives

A fact has one home. Read it there, and write it there in the commit that changes the behavior, at the first home
below that can hold it; write nothing the code, its tests and its `//!` already say.

1. A closed set (kinds, fields, keys, tools, rules): the type that defines it; never a list in prose.
2. A rule a test, a type or a smell counter can hold: that check, failing with the fix. When code outside its
   owner can break the rule, the test's doc says so in a last line, `Reached by:` and that code's paths.
3. How a module works, the alternative it rejects, how to add to it: its `//!`. One item or one term: its `///`,
   with a "Not <what it is confused with>" sentence where one exists. A trap at one line: a comment there.
4. What belongs to no single type, check or module (how crates fit, a product rule, a non-goal, open work, a trap
   no one line holds): the file `.agents/README.md` names for it.
5. What users do and rely on: `docs/`, `README.md`. Contributor process: `CONTRIBUTING.md`. Why something
   changed: the commit message.

Rewrite in place and delete what became false; git keeps the history. Personal memory and plan files hold no
rule another agent needs. If a sandbox refuses a write under `.agents/`, keep the entry in your report
(`Knowledge: <file>, refused`); never move it to another home. In a Yunta run, `AGENTS.md` changes only
when a person granted it to your task, an `.agents/*.md` file only when your task's scope names it,
and nothing under `.agents/skills/`.

## Commands

- Before you edit a file, run `cargo xtask docs --about <files>`. It prints the module chain down to each file,
  whose `//!` you read first, and the rules, traps, open work and tests recorded for those paths.
- Iterate with `cargo test -p <package> --test <target>`, building incrementally (packages are `yunta-<dir>`;
  `crates/cli` is `yunta`).
- A pull request merges with every job of `.github/workflows/ci.yml` green; CI runs them on it. When to commit,
  and what each commit passes on the way, belongs to the process you work under: a skill, a plan or a run's
  workflow. With none, commit only when a person asks.
- A local run of the whole `ci` job, and only that, runs one build at a time with `CARGO_INCREMENTAL=0` and ends
  with `cargo clean`: its artifacts fill the disk. A test or a check you run by itself builds incrementally and
  cleans nothing.
- Stage paths by name, never `git add -A` or `git stash`: a checkout can hold a person's uncommitted files.
- A changed document type: `cargo xtask schema`, then commit `crates/core/schemas/`. Output meant to change:
  `YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine`, then review the goldens' diff.
- A change on the run path is proven by real runs before and after: a release build, a run started detached and
  stopped before its `pr` node, gates answered with `yunta resolve-gate`, time read from `yunta stats <run>`.

## How to work

- When sources disagree, say how; keep facts, hypotheses and choices apart.
- Let the domain shape the code: types that admit only valid states before branches, and the owner of each
  invariant found by tracing how data enters, persists, is replayed and reaches a caller. Design as if the
  requirement had always existed, reach it in coherent steps, and delete the old path. An abstraction earns its
  place by removing duplicated rules, invalid states or caller effort.
- Evidence must be able to disprove the change: reproduce a bug and explain its cause, show a refactor keeps
  public behavior, measure performance against a baseline. A test asserts an outcome known apart from the code.
  A `yunta/run/*` or `yunta/unit/*` branch is run output, never precedent.

## Rules

- Code, YAML, tests, scripts and CI never cite documentation: no decision numbers, section signs or doc names;
  say the behavior and its reason. Commits and pull requests neither. [smell doc_reference]
- English for documentation, comments, UI text and examples, in the present tense. [smell spanish_prose_lines]
  [smell tense_markers]
- A numeric `const` carries a `///` line saying why it has that value. [smell numeric_const_without_reason]

## Before you finish

- If you changed files: run `cargo xtask docs --about` with no paths, which reads your diff, and keep true each
  rule, trap and note it prints; the tests of each crate you touched, `cargo xtask smells --check` and
  `cargo xtask docs --check` pass; what you learned is in its one home above, or nowhere. A report to a person
  carries the line `Knowledge: <files>` or `Knowledge: none`.
- In a Yunta run (you have the `yunta-run` server's tools) the engine judges the work instead. A fix or a lesson
  outside your task's scope goes in your report, never in a scope request. A plan puts in each task's scope the
  docs that task must keep true, and in its `invariants` the rules and traps its paths meet.
