#!/bin/sh
# A `git` for a test's PATH. It is the real git in everything except the
# one invocation the test names — `YUNTA_STUB_GIT_BLOCK_ON` is matched
# against the subcommand and against the first two arguments together,
# so `worktree add` can be held while `worktree prune` runs for real.
# `YUNTA_STUB_GIT_BLOCK_WHEN` names a file that arms the hold, for a
# test that wants the second time a run asks the same question and not
# the first. There the stub publishes its pid and blocks until something
# kills it, which is how a test reaches a git that is still running.
#
# The pid goes out by rename, never by writing the watched path: a
# rename is atomic, so the moment the file exists it holds the whole
# pid and a watcher needs no interval to be sure of that.
if [ -n "$YUNTA_STUB_GIT_BLOCK_ON" ] &&
    { [ "$1" = "$YUNTA_STUB_GIT_BLOCK_ON" ] || [ "$1 $2" = "$YUNTA_STUB_GIT_BLOCK_ON" ]; } &&
    { [ -z "$YUNTA_STUB_GIT_BLOCK_WHEN" ] || [ -f "$YUNTA_STUB_GIT_BLOCK_WHEN" ]; }; then
    echo $$ > "$YUNTA_STUB_GIT_PID.partial"
    mv "$YUNTA_STUB_GIT_PID.partial" "$YUNTA_STUB_GIT_PID"
    exec tail -f /dev/null
fi
exec "$YUNTA_STUB_GIT_REAL" "$@"
