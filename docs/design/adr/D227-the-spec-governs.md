---
number: D227
title: "The spec governs: a plan that cannot be proven as written is refused when handed over, and no gate offers to go on with one"
status: accepted
revises: [D194, D225]
revised_by: []
---

# D227 — The spec governs: a plan that cannot be proven as written is refused when handed over, and no gate offers to go on with one

## Context

A planner handed its plan over sixteen times and was refused fifteen. Twelve of
those refusals said that a criterion already passed before any work. The planner
had tried `cargo test` filters that match no test, and such a filter exits 0.
The refusal did not say what the command printed, so the planner never saw that
zero tests had run. It got past the rule by adding `&& grep -q '<name>' <file>`,
reading a file the task itself changes.

The spec that followed wrote two test files that each held `assert!(true)`, and
none of its tests ran them. Each of its tests was the plan's own criterion
again. The plan was accepted, and so was the spec. The gate drew the plan as
"held to 2 tests from its spec" and asked "Plan and its tests registered.
Approve?". Approving that plan approves work that passes once a name is written,
whatever the code does.

## Decision

1. **What keeps a plan from being proven is data.** A plan shown with its spec
   reads as a list of flaws:
   - a criterion that passes once a name is written in a file its task changes;
   - a test of the spec that runs none of the files the spec wrote;
   - a file the spec wrote that none of its tests runs;
   - a change the plan names on a test the spec wrote;
   - a criterion that runs the test the spec gives another task.

   The engine computes the list, and every surface draws it from the same data.
2. **What can be told at handover is refused there.**
   - A plan is held to these rules: `criterion-checks-presence`,
     `shared-criterion`, `uses-its-own-shape`, `task-writes-its-test` (only when
     the workflow writes a spec), `changes-a-spec-test` and `unknown-answer`.
   - A spec is held to `unrun-spec-file` and `spec-test-runs-no-spec-file`.
   - These rules are applied when a document is handed over, never when one is
     read back. The documents of an older run therefore stay readable. The
     contract publishes the rules beside the shape rules.
3. **A refusal for a criterion that passes before the work quotes the
   command.** It carries the tail of what the command printed, and it says that
   a test filter matching no test passes.
4. **An internal gate withholds going on.** When what a gate shows has a flaw,
   the options that take the run past the gate leave the menu. `gate_waiting`
   records them in `withheld`, each with its reason, and the reason is also a
   fact of the record. The options `on:` maps back, and `abort`, stay.
   - An answer that names a withheld option is refused on every surface, and
     the refusal says why.
   - An approval seeded while the run was parked no longer stands, so the gate
     asks again.
5. **A gate on a forge does not publish.**
   - The engine posts one finding per flaw on the plan and fails the gate,
     retryable. Its `on_failure` sends the plan back, as a request for changes
     would.
   - A gate that falls back to the console withholds approval.
   - A gate already published stays as it was.

## Rationale

A person approves a spec as the definition of done, and the run builds against
it. An approval offered on a plan that the spec cannot hold to anything records
a decision with nothing behind it. Refusing at handover costs the planner one
turn. A refusal discovered at the gate would cost a person a decision, and then
another lap.

Some flaws can only be seen once both documents exist. A plan can change a file
that the spec, written after it, makes a test. The gate is the first place that
holds both, so it holds the line there too. It does not hide the options it
withholds: it shows them with their reason, so a person can see why the run
cannot go on and what sends it back.

## Rejected alternatives

**A caution beside an `approve` that still works.** A spec that anyone can
override at the decision governs nothing. A warning that does not change what
can be chosen would only be read after the run had built the wrong thing.

**The rules in `TasksFile::check()` and `SpecFile::check()`.** Every read applies
those checks again. An older run's documents would become unreadable, including
the one this decision comes from.

**Leaving withheld options off the menu silently.** A person who sees no
`approve` and no reason cannot tell a broken gate from a gate that holds the
line.
