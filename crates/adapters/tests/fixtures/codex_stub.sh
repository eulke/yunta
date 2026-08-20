#!/bin/sh
# Fake `codex` binary for the codex adapter tests (T7.4) — no network, no
# API cost, deterministic. Mirrors claude_code_stub.sh exactly; see that
# file's own comment for the rationale each knob shares.
#
# - `--version`: replies like the real CLI and exits.
# - $CODEX_STUB_ARGS_FILE, if set: every argv entry, one per line.
# - $CODEX_STUB_CHILD_PID_FILE, if set: spawns a background `sleep` of
#   its own and records its pid — a grandchild kill() must also reach.
# - streams the JSON lines from $CODEX_STUB_LINES_FILE if set, else from
#   .codex-stub-lines.jsonl relative to the working directory.
# - (if $CODEX_STUB_HANG is set) ignores SIGINT and sleeps afterwards.

if [ -n "$CODEX_STUB_ARGS_FILE" ]; then
  printf '%s\n' "$@" > "$CODEX_STUB_ARGS_FILE"
fi

if [ "$1" = "--version" ]; then
  echo "codex-cli 0.47.0"
  exit 0
fi

if [ -n "$CODEX_STUB_CHILD_PID_FILE" ]; then
  sleep 300 &
  echo $! > "$CODEX_STUB_CHILD_PID_FILE"
fi

lines_file="${CODEX_STUB_LINES_FILE:-.codex-stub-lines.jsonl}"
if [ -f "$lines_file" ]; then
  while IFS= read -r line || [ -n "$line" ]; do
    echo "$line"
  done < "$lines_file"
fi

if [ -n "$CODEX_STUB_HANG" ]; then
  trap '' INT
  sleep 300
fi

exit "${CODEX_STUB_EXIT:-0}"
