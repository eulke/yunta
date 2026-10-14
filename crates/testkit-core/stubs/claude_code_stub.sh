#!/bin/sh
# Fake `claude` binary for claude_code adapter tests — no network,
# no API cost, deterministic. Real `claude` calls would break CI's
# no-real-LLM rule.
#
# - `--version`: replies like the real CLI and exits.
# - $CLAUDE_STUB_STDIN_FILE, if set: everything read from stdin — the prompt.
# - $CLAUDE_STUB_ARGS_FILE, if set: every argv entry, one per line — lets
#   tests assert the exact CLI invocation the adapter built (permission
#   flags, --model, --resume, ...) without exposing that logic publicly.
# - $CLAUDE_STUB_STDERR_FILE, if set: its contents go to stderr before
#   the session streams anything — what a CLI that refuses its
#   configuration says on its way out. A file rather than the text
#   itself, because a session redacts every declared secret and session
#   token from its stderr tail, and a test asserting on the line needs it
#   unredacted.
# - $CLAUDE_STUB_ENV_FILE, if set: the environment it ran with, one
#   `NAME=value` per line.
# - $CLAUDE_STUB_EXIT, if set: the status to exit with.
# - $CLAUDE_STUB_CHILD_PID_FILE, if set: spawns a background blocker of
#   its own and records its pid — a grandchild the adapter's kill() must
#   also reach, since the whole process tree must die together.
# - streams the JSON lines from $CLAUDE_STUB_LINES_FILE if set, else from
#   .claude-stub-lines.jsonl relative to the working directory the
#   adapter launched it in — real yunta runs don't route test scaffolding
#   through SessionRequest.env (the engine fills that field with the
#   session's shared directories and declared secrets), so a caller that
#   only controls the worktree still has a way to script a session.
# - (if $CLAUDE_STUB_HANG is set) ignores SIGINT and blocks afterwards —
#   exercising the kill fallback a session that ignores interrupt needs.
# - (if $CLAUDE_STUB_LINGER is set) blocks until interrupted, then exits —
#   a session that honours SIGINT.
# - $CLAUDE_CODE_STOP_HOOK_BLOCK_CAP, if set: how many stop turns in a row the
#   stream prints. A stop turn is a line carrying the text the contract suite
#   writes after a refused stop; past the cap those lines are not printed,
#   and the result line still is. Unset means one; zero lifts the cap.

# How SIGINT is met is settled before anything else, so a test that waits
# for the grandchild's pid interrupts a stub that already meets it so.
if [ -n "$CLAUDE_STUB_HANG" ]; then
  trap '' INT
elif [ -n "$CLAUDE_STUB_LINGER" ]; then
  trap 'exit 0' INT
fi

# Each record is written beside its file and moved into place, so a test
# that reads it while the stub runs meets the old record or the new one,
# never a file truncated and not yet rewritten.
if [ -n "$CLAUDE_STUB_ENV_FILE" ]; then
  env > "$CLAUDE_STUB_ENV_FILE.$$" && mv "$CLAUDE_STUB_ENV_FILE.$$" "$CLAUDE_STUB_ENV_FILE"
fi

if [ -n "$CLAUDE_STUB_ARGS_FILE" ]; then
  printf '%s\n' "$@" > "$CLAUDE_STUB_ARGS_FILE.$$" && mv "$CLAUDE_STUB_ARGS_FILE.$$" "$CLAUDE_STUB_ARGS_FILE"
fi

if [ -n "$CLAUDE_STUB_STDIN_FILE" ]; then
  cat > "$CLAUDE_STUB_STDIN_FILE"
fi

if [ "$1" = "--version" ]; then
  echo "2.1.296 (Claude Code)"
  exit 0
fi

if [ -n "$CLAUDE_STUB_STDERR_FILE" ]; then
  cat "$CLAUDE_STUB_STDERR_FILE" >&2
fi

if [ -n "$CLAUDE_STUB_CHILD_PID_FILE" ]; then
  tail -f /dev/null &
  echo $! > "$CLAUDE_STUB_CHILD_PID_FILE"
fi

lines_file="${CLAUDE_STUB_LINES_FILE:-.claude-stub-lines.jsonl}"
if [ -f "$lines_file" ]; then
  # The most stop turns in a row the stream prints; zero prints them all.
  stop_cap="${CLAUDE_CODE_STOP_HOOK_BLOCK_CAP:-1}"
  stop_run=0
  # `|| [ -n "$line" ]` also emits a final line with no trailing newline —
  # POSIX `read` alone drops it, and a stream-json session's last line
  # (its terminal result) is exactly the one that must never go missing.
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
      *"a refused stop, and the work goes on: "*)
        stop_run=$((stop_run + 1))
        if [ "$stop_cap" -ne 0 ] && [ "$stop_run" -gt "$stop_cap" ]; then
          continue
        fi
        ;;
      *)
        stop_run=0
        ;;
    esac
    # `printf '%s\n'` rather than `echo`: a shell whose `echo` reads
    # backslash escapes (dash's does) would split a JSON line carrying
    # `\n` in a string into two lines, and neither half is this protocol.
    printf '%s\n' "$line"
  done < "$lines_file"
fi

if [ -n "$CLAUDE_STUB_HANG" ]; then
  tail -f /dev/null
fi

if [ -n "$CLAUDE_STUB_LINGER" ]; then
  while :; do sleep 1; done
fi

exit "${CLAUDE_STUB_EXIT:-0}"
