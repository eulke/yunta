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
# - $CLAUDE_STUB_CHILD_PID_FILE, if set: spawns a background `sleep` of
#   its own and records its pid — a grandchild the adapter's kill() must
#   also reach, since the whole process tree must die together.
# - streams the JSON lines from $CLAUDE_STUB_LINES_FILE if set, else from
#   .claude-stub-lines.jsonl relative to the working directory the
#   adapter launched it in — real yunta runs don't route test scaffolding
#   through SessionRequest.env (that field carries secrets only), so a
#   caller that only controls the worktree still has a way to script a
#   session.
# - (if $CLAUDE_STUB_HANG is set) ignores SIGINT and sleeps afterwards —
#   exercising the kill fallback a session that ignores interrupt needs.

if [ -n "$CLAUDE_STUB_ARGS_FILE" ]; then
  printf '%s\n' "$@" > "$CLAUDE_STUB_ARGS_FILE"
fi

if [ -n "$CLAUDE_STUB_STDIN_FILE" ]; then
  cat > "$CLAUDE_STUB_STDIN_FILE"
fi

if [ "$1" = "--version" ]; then
  echo "2.1.235 (Claude Code)"
  exit 0
fi

if [ -n "$CLAUDE_STUB_CHILD_PID_FILE" ]; then
  sleep 300 &
  echo $! > "$CLAUDE_STUB_CHILD_PID_FILE"
fi

lines_file="${CLAUDE_STUB_LINES_FILE:-.claude-stub-lines.jsonl}"
if [ -f "$lines_file" ]; then
  # `|| [ -n "$line" ]` also emits a final line with no trailing newline —
  # POSIX `read` alone drops it, and a stream-json session's last line
  # (its terminal result) is exactly the one that must never go missing.
  while IFS= read -r line || [ -n "$line" ]; do
    echo "$line"
  done < "$lines_file"
fi

if [ -n "$CLAUDE_STUB_HANG" ]; then
  trap '' INT
  sleep 300
fi

exit "${CLAUDE_STUB_EXIT:-0}"
