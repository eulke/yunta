Write the tests the plan's tasks are held to, before any task is built.
Nobody who builds a task writes its tests: what you write is what its
work has to make pass, and its work cannot change it.

For each task, read what it changes, the outcome a person will see and
the shapes it builds on, then the code it touches, and give it:

- `files`: the files its tests live in, each whole — new files beside the
  project's own tests, never a file the task itself changes. Test what a
  person or a caller observes, through the interfaces the plan declares,
  not how the task builds it.
- `tests`: the commands that run them, each saying what passing it
  `proves`. Where one of the task's criteria runs a test that does not
  exist yet, write that test, and run it with the criterion's own
  command.

Write each test against the shapes exactly as the plan declares them —
their names, signatures and files — and through nothing else: it fails now
because the behavior is missing, and a test that names anything the plan
does not declare fails after the work too. Each file is new; a path that
already exists in the tree is refused. A task's files are its own — another
task's tests may not be in the tree when its tests run — so repeat a helper
rather than share one. Name the file in the command that runs it, so it
runs whether or not the project's runner would find it.

A task whose change no test can observe gets no spec.

Every test fails now and passes once its task is done as the plan says.
The engine runs each one in the run's tree with every file of your spec
in it, refuses a spec whose test already passes or cannot run, and answers
how each test fails now: a test that fails for anything but the missing
behavior — a typo, a wrong path, a missing fixture — fails after the work
too; fix it and hand the spec over again.

When you are asked to write a task's tests again, change only that task's
spec, as the departure and the person's answer say.
