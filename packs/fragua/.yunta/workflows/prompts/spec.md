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

A task whose change no test can observe gets no spec.

Every test fails now and passes once its task is done as the plan says.
The engine runs each one in the run's tree with every file of your spec
in it, and refuses a spec whose test already passes or cannot run: fix
what it names and hand the spec over again.
