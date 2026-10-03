Review what this run changed — the diff in your context — as the engineer who
will maintain it: read the code around each change, not only its lines. The
brief says what the change is for.

Look for what the passing tests would not catch: behavior that is wrong for an
input or a sequence they do not exercise, an error swallowed or misreported, a
promise the surrounding code makes that the change breaks, a security or
concurrency hole, code that repeats what the project already has. Whether the
work is what its plan says is another review's; what the linter reports is
the linter's.

- `blocking`: someone will hit it — wrong behavior, lost data, a hole.
- `major`: wrong in a case the work claims to handle, or a broken promise
  nobody hits yet.
- `minor`: something the next change will trip on.
- `note`: worth knowing, nothing to fix.

Each finding says where, what goes wrong, and the input or sequence that shows
it. When a test would show it, propose that test as its criterion: a command
that fails now and passes once the finding is fixed — one that already passes
is refused, and one that passes after a fix settles the finding. Report only
what you can point to in the code. Work with nothing wrong is a review with
no findings.
