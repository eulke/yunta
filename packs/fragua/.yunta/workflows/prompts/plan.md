Read the brief and the surrounding context, then the code the brief touches:
its shapes, conventions and tests are what the plan builds on. The knowledge
in your context holds what earlier plans for this project settled; follow it,
and make a decision of any point where the plan departs from it.

Register a tasks document: one task per independently-verifiable unit of
work. Never mark anything done yourself — that's the engine's call once your
criteria pass. The run holds every task to the test suite it measured before
any work, so keep each task's criteria to what its own change turns green
rather than repeating the suite.

A person reads this plan — before any work starts when the run asks them to
approve it, and beside the finished work before its pull request. Write it so
they can see what will be built and disagree with it:

- `summary`: what the plan changes, in one line.
- `description`: what changes, why, and how you approach it, in Markdown.
  Use a `mermaid` block when a diagram says it better than prose.
- `decisions`: every point the brief left open, closed here — never left to
  whoever implements a task. Each says what was open, what you chose, what
  you did not choose, and why. A person answers these in seconds; they are
  what they are most likely to change.
- `shapes`: every type, interface, schema, signature or file format the plan
  creates or changes, declared once and whole, in the file it lives in, and
  owned by the one task that builds it — whose scope covers that file. A
  task that builds on a shape another task owns lists it under `uses` and
  waits for its owner. If a later task needs the shape to grow, the owner
  builds it whole; no other task may change its file.
- `design`: how the parts fit together, in prose and code examples, when the
  shapes alone do not say it.
- `risks` and `out_of_scope`, when there are any.
- For every task:
  - `description`: what it does and why; name the shapes it touches rather
    than repeating them.
  - `changes`: every place it changes, a file or a file and what in it
    (`src/theme.rs::Theme`), and what changes there — each inside its scope.
  - `outcome`: what a person will observe once it is done. For a change
    someone sees — output, a screen, a message — say what they see before
    and after.
  - `invariants`: what the code it touches already promises and it must
    keep. Read that code first: its comments and tests say what it holds to.
- For every criterion, what passing it `proves`, in words. A criterion runs
  the behavior — a test, not a check that a file exists or a line appears.
  It must run today and fail: a test filter that matches nothing usually
  passes, and a script that is not there yet may not run at all, which the
  engine refuses — name a test that does not exist yet through the
  project's own test runner, and whoever writes the task's tests writes it.
  The files a task's tests live in are inside its scope and outside its
  `changes`.

Tasks run at the same time unless one depends on another, and two such tasks
may not share a path: a file both need belongs to one task, and the other
depends on it. Each task is its own session: split only where the parts can
be verified apart.
