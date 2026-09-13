#!/bin/sh
# Fake `codex` binary for the codex adapter tests — no network, no
# API cost, deterministic. Mirrors claude_code_stub.sh exactly; see that
# file's own comment for the rationale each knob shares.
#
# - `--version`: replies like the real CLI and exits.
# - $CODEX_STUB_STDIN_FILE, if set: everything read from stdin — the prompt.
# - $CODEX_STUB_ARGS_FILE, if set: every argv entry, one per line.
# - $CODEX_STUB_ENV_FILE, if set: the child's whole environment — lets a
#   test assert that secret material reached the process the one way it
#   may (the environment) and not the one it may not (argv).
# - $CODEX_STUB_CHILD_PID_FILE, if set: spawns a background blocker of
#   its own and records its pid — a grandchild kill() must also reach.
# - refuses an option of the parent `exec` command that follows the
#   `resume` subcommand, the way the real CLI's parser does: a
#   diagnostic on stderr, exit 2, no session.
# - streams the JSON lines from $CODEX_STUB_LINES_FILE if set, else from
#   .codex-stub-lines.jsonl relative to the working directory.
# - (if $CODEX_STUB_HANG is set) ignores SIGINT and blocks afterwards.

if [ -n "$CODEX_STUB_ARGS_FILE" ]; then
  printf '%s\n' "$@" > "$CODEX_STUB_ARGS_FILE"
fi

if [ -n "$CODEX_STUB_ENV_FILE" ]; then
  env > "$CODEX_STUB_ENV_FILE"
fi

if [ -n "$CODEX_STUB_STDIN_FILE" ]; then
  cat > "$CODEX_STUB_STDIN_FILE"
fi

if [ "$1" = "--version" ]; then
  echo "codex-cli 0.47.0"
  exit 0
fi

# `exec resume` takes a session id, `--last`, `--all` and `--image`;
# `exec`'s own options belong to the parent command and are not
# `global`, so the real CLI reads one of them after the subcommand as an
# unexpected argument and exits before opening a session. Only `--json`
# and `-c/--config` are `global` and may sit on either side. The stub
# stands in for that parser, so it refuses what clap refuses.
seen_resume=""
for arg in "$@"; do
  if [ -z "$seen_resume" ]; then
    if [ "$arg" = "resume" ]; then
      seen_resume=1
    fi
    continue
  fi
  case "$arg" in
    --last|--all|--image|--json|--config) ;;
    --*)
      echo "error: unexpected argument '$arg' found" >&2
      exit 2
      ;;
  esac
done

if [ -n "$CODEX_STUB_CHILD_PID_FILE" ]; then
  tail -f /dev/null &
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
  tail -f /dev/null
fi

exit "${CODEX_STUB_EXIT:-0}"
