# Working in Yunta

Yunta runs YAML workflows: declared criteria, judged by the engine, decide when work is done, and replay derives
every state from the event log. This file is the map: where to read before a change and where to write what you
learn. How the parts fit is in `.agents/architecture.md`.

## Where to read

Before you change a file, open the module file of the directory that holds it, then the file: before
`crates/engine/src/worktree/pool.rs`, open `crates/engine/src/worktree/mod.rs` (a directory with no `mod.rs` has its
`<dir>.rs` beside it). Its `//!` says how the module works, what it keeps and what else a change touches. A crate
root's `//!` maps its crate. When you know a change by its subject and not yet by its file, start from this table:

| You are changing | Open |
|---|---|
| event kinds, payload fields | `crates/core/src/events/mod.rs` |
| replay, the log store | `crates/engine/src/replay.rs`, `crates/storage/src/lib.rs` |
| workflow keys, node kinds, config keys | `crates/core/src/workflow/mod.rs`, `crates/core/src/config/mod.rs` |
| document shapes | `crates/core/src/shape/mod.rs` |
| the run loop, gates, child workflows | `crates/engine/src/run/mod.rs` |
| tasks, criteria, the memo | `crates/engine/src/task_cycle/mod.rs` |
| scope, scope expansion | `crates/engine/src/scope.rs`, `crates/engine/src/scope_expansion.rs` |
| the fence, the rail | `crates/core/src/fence.rs`, `crates/core/src/rail/mod.rs` |
| protected paths, confinement | `crates/core/src/protected.rs`, `crates/core/src/confinement.rs` |
| run tools, the control plane | `crates/engine/src/run_tools/mod.rs`, `crates/cli/src/commands/mcp.rs` |
| the port, capabilities | `crates/core/src/port/mod.rs`, `crates/core/src/capabilities.rs` |
| adapters | `crates/adapters/src/lib.rs` |
| Surco, a surface | `crates/render/src/lib.rs` |
| smell counters, the docs checker, CI | `xtask/src/main.rs`, `.github/workflows/ci.yml` |

Before you edit a file, run `cargo xtask docs --about <files>`; before you finish, run it with no paths (your
diff). It prints the module chain and the rules, traps and open work recorded for those paths. Where
`cargo xtask` cannot build (a run's planner, spec writer or reviewer; plan mode), read `.agents/traps.md` and the
product rules in `.agents/architecture.md` whole, and run `git grep -n 'Reached by:' -- crates xtask`.

| Before you | Read |
|---|---|
| plan or review a change, or change what a session is told | `.agents/architecture.md` |
| add a pause, a gate, per-run work, text for a person, an adapter feature, a crate edge | `.agents/architecture.md` |
| propose a mechanism, a config key or a command | `.agents/choices.md` |
| take on a gap or an unanswered question | `.agents/open.md` |
| write a test, or work where `cargo xtask` cannot build | `.agents/traps.md` |

## Where knowledge goes

Write a fact once, at the first home below that can hold it, in the commit that changes the behavior; write
nothing the code, its tests and its `//!` already say.

1. A closed set (kinds, fields, keys, tools, rules): the type that defines it; never a list in prose.
2. A rule a test, a type or a smell counter can hold: that check, failing with the fix. When code outside its
   owner can break the rule, the test's doc says so in a last line, `Reached by:` and that code's paths.
3. How a module works, the alternative it rejects, how to add to it: its `//!`. One item or one term: its `///`,
   with a "Not <what it is confused with>" sentence where one exists. A trap at one line: a comment there.
4. How crates fit, a product rule (one line, tagged with what holds it, with `Paths:` when only review does), or
   a guarantee no single module owns: `.agents/architecture.md`.
5. An absence or a non-goal: `.agents/choices.md`. Open work: `.agents/open.md`, deleted when done.
6. A mistake that cost a cycle, with no check and no single line to hold it: `.agents/traps.md`, with the paths
   where it bites. The change that adds its check deletes it.
7. What users do and rely on: `docs/`, `README.md`. Contributor process: `CONTRIBUTING.md`. Why something
   changed: the commit message.

Rewrite in place and delete what became false; git keeps the history. Personal memory and plan files hold no
rule another agent needs. If a sandbox refuses a write under `.agents/`, keep the entry in your report
(`Knowledge: <file>, refused`); never move it to another home. In a Yunta run, `AGENTS.md` and `CLAUDE.md`
change only when a person granted them to your task, an `.agents/*.md` file only when your task's scope names it,
and nothing under `.agents/skills/`.

## Commands

- Iterate with `cargo test -p <package> --test <target>`, building incrementally (packages are `yunta-<dir>`;
  `crates/cli` is `yunta`).
- Commit only when you are asked to. Before a commit, run every step of the `ci` job in
  `.github/workflows/ci.yml`, one build at a time with `CARGO_INCREMENTAL=0`; a plan committed in phases
  commits each whole phase with them green, then runs `cargo clean`.
- Stage paths by name, never `git add -A` or `git stash`: a checkout can hold a person's uncommitted files.
- A changed document type: `cargo xtask schema`, then commit `crates/core/schemas/`. Output meant to change:
  `YUNTA_BLESS=1 cargo test -p yunta -p yunta-engine -p yunta-render`, then review the goldens' diff.
- A change on the run path is proven by real runs before and after: a release build, a run started detached and
  stopped before its `pr` node, gates answered with `yunta resolve-gate`, time read from `yunta stats <run>`.
- A release is a pushed `vX.Y.Z` tag.

## How to work

- Frame a task as a change in observable behavior: what happens now, what should happen, and which assumption
  might be wrong. Read nearby code and tests, and an external API's current documentation. When sources
  disagree, say how; keep facts, hypotheses and choices apart.
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

- If you changed files: run `cargo xtask docs --about` and keep true each rule, trap and note it prints; the
  tests of each crate you touched, `cargo xtask smells --check` and `cargo xtask docs --check` pass; what you
  learned is in its one home above, or nowhere. A report to a person ends with `Knowledge: <files>` or
  `Knowledge: none`.
- In a Yunta run (you have the `yunta-run` server's tools) the engine judges the work instead. A fix or a lesson
  outside your task's scope goes in your report, never in a scope request. A plan puts in each task's scope the
  docs that task must keep true, and in its `invariants` the rules and traps its paths meet.
