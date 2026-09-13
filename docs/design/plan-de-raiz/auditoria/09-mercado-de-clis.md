# Auditoría 09 — El mercado de CLIs y el control de escritura

Ocho CLIs de agentes de código relevados el 2026-09-13 como adapters posibles,
para probar que la cerca (`cerca.md`) escala a cualquiera. Un informe por CLI,
textual, en inglés, tal como lo produjo cada agente investigador con Context7 y
las fuentes oficiales; lo marcado `unverified` no se confirmó. Ninguno se
ejecutó salvo Copilot CLI, que el investigador corrió contra un proveedor
falso.

---

## Gemini CLI (v0.59.0, released 2026-09-08)

Latest release: v0.59.0 (https://github.com/google-gemini/gemini-cli/releases/latest). Source citations are `main` at time of research.

### 1. Headless mode

- **Trigger**: `-p/--prompt "<text>"` — "Run in non-interactive (headless) mode with the given prompt. Appended to input on stdin (if any)." Headless also triggers on a non-TTY. (https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/cli/src/config/config.ts; https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/headless.md)
- **Output**: `-o/--output-format text|json|stream-json`.
  - `json`: one object `{response, stats, error?}`.
  - `stream-json`: JSONL, events `init | message | tool_use | tool_result | error | result`. Types in `packages/core/src/output/types.ts`:
    - `tool_use`: `{type, timestamp, tool_name, tool_id, parameters: Record<string,unknown>}` — yes, name + full args, emitted when the model requests the call, before execution (`nonInteractiveCli.ts`: `tool_name: event.value.name, tool_id: event.value.callId, parameters: event.value.args`).
    - `tool_result`: `{tool_id, status: 'success'|'error', output?, error?: {type, message}}`; `error.type = toolResponse.errorType || 'TOOL_EXECUTION_ERROR'`.
    - `error`: `{severity:'warning'|'error', message}` (non-terminal). `result`: `{status:'success'|'error', error?:{type,message}, stats?}` (terminal).
    (https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/core/src/output/types.ts; https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/cli/src/nonInteractiveCli.ts)
- **Exit codes**: 0 success, 1 general/API error, 42 input error, 53 turn limit (docs/cli/headless.md); 41 auth failure, and fatal errors are also emitted as a `result` event with `status:'error'` on stdout before `process.exit` (packages/cli/src/utils/errors.ts, via Context7).

### 2. Write-control mechanisms

**(a) Hooks** — `hooks.BeforeTool` in `settings.json` (project `.gemini/settings.json` > user `~/.gemini/settings.json` > system; extensions too). Entry: `{matcher: "<regex over tool names>", hooks:[{type:"command", command, name?, timeout?(60000ms)}]}`. MCP tools match as `mcp_<server>_<tool>`. (https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/hooks/index.md; https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/hooks/reference.md)
- **stdin JSON**: `session_id, transcript_path, cwd, hook_event_name, timestamp, tool_name, tool_input, mcp_context, original_request_name`. `tool_input` for `write_file` = `{file_path, content}`, for `replace` = `{file_path, old_string, new_string, instruction, allow_multiple}`, for `run_shell_command` = `{command, description, dir_path, is_background}` (https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/reference/tools.md).
- **env**: `GEMINI_PROJECT_DIR`, `GEMINI_CWD`, `GEMINI_SESSION_ID`, `GEMINI_PLANS_DIR` (https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/core/src/hooks/hookRunner.ts).
- **Blocking**: stdout JSON `{"decision":"deny","reason":"...","systemMessage":"..."}` (alias `"block"`), or exit code 2 with reason on stderr; `hookSpecificOutput.tool_input` can rewrite args; `continue:false` stops the agent loop. Docs say other non-zero codes are non-fatal warnings; hookRunner.ts comment says "All other non-zero exit codes (including 2) are blocking" (exit 1 excepted) — discrepancy, treat as unverified beyond 0/2.
- **What the model sees**: tool error `"Tool execution blocked: ${reason}"` with `ToolErrorType.POLICY_VIOLATION` (`policy_violation`); `continue:false` gives `"Agent execution stopped by hook: ..."` / `stop_execution`. Docs: "This text is sent to the agent as a tool error, allowing it to respond or retry." (https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/core/src/scheduler/hook-utils.ts)
- `hooksConfig.enabled` defaults to `true`; hooks are disabled entirely in an untrusted folder when folder trust is on (off by default). (https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/cli/src/config/settingsSchema.ts; https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/trusted-folders.md)

**(b) Sandbox** — technologies: Docker/Podman, macOS Seatbelt (`sandbox-exec`), gVisor `runsc`, LXC (experimental). Enable with `-s/--sandbox`, `GEMINI_SANDBOX=true|docker|podman|sandbox-exec|runsc|lxc`, or `tools.sandbox` setting. (https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/sandbox.md)
- Docker: cwd mounted read-write at the same absolute path; `SANDBOX_MOUNTS=from:to:opts` (opts default `ro`); `tools.sandboxAllowedPaths` mounted read-only (https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/cli/src/utils/sandbox.ts).
- Seatbelt: profiles `permissive-open` (default), `permissive-proxied`, `restrictive-*`, `strict-*` via `SEATBELT_PROFILE`; `(deny default)` then `file-write*` allowed only for `TARGET_DIR`, `TMP_DIR`, `CACHE_DIR`, `~/.npm`, `~/.cache`, `INCLUDE_DIR_0..4` (from workspace include dirs), tty devices. Custom profile: `~/.gemini/sandbox-macos-<name>.sb`, then project `.gemini/sandbox-macos-<name>.sb` (https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/cli/src/utils/sandbox-macos-permissive-open.sb; sandbox.ts).
- Path granularity: directory subpaths (mounts / `subpath`), no globs. Denied write inside a shell command surfaces as a "Sandbox Expansion Request" confirmation (`ask_user`), which in non-interactive mode is treated as deny; `ToolErrorType.SANDBOX_EXPANSION_REQUIRED` exists. Exact stream representation in headless mode: **unverified** (undocumented).

**(c) Policy engine** — TOML `[[rule]]` with `toolName` (string/array/`*`/`mcp_*`), `mcpName`, `argsPattern` (regex over "the tool's arguments converted to a stable JSON string"), `commandPrefix`/`commandRegex` (shell only), `toolAnnotations`, `subagent`, `decision = allow|deny|ask_user`, `priority 0–999`, `denyMessage`, `modes`, `interactive`, `allowRedirection`. Tiers: default 1.x < extension 2.x < **workspace 3.x (non-functional, issue #18186)** < user 4.x (`~/.gemini/policies`, or `--policy`/`policyPaths` which *replace* that dir) < admin 5.x (`/etc/gemini-cli/policies`, `--admin-policy`/`adminPolicyPaths`). `ask_user` in non-interactive mode = deny. A deny rule without `argsPattern` removes the tool from the model's tool list entirely. (https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/reference/policy-engine.md; https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/core/src/policy/types.ts; https://github.com/google-gemini/gemini-cli/pull/18500)
- Also `[[safety_checker]]` with in-process `allowed-path` checker: inspects args whose key contains `path|directory|file` or is `source|destination`, resolves them and DENYs if outside `cwd + workspaces` (include dirs) — the built-in `write.toml` attaches it to `write_file`/`replace` in `autoEdit` mode. Its reason text is dropped by `policy-engine.ts` (returns bare DENY). (https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/core/src/safety/built-in.ts; https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/core/src/policy/policies/write.toml; https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/core/src/policy/policy-engine.ts)
- Deprecated/coarser: `--allowed-tools` ("DEPRECATED: Use Policy Engine instead", tier 4.3), `tools.core` (built-in allowlist), `tools.exclude`, `tools.allowed`.

**(d) Approval modes** — `--approval-mode default|auto_edit|yolo|plan`, `--yolo`, setting `general.defaultApprovalMode` (`default|auto_edit|plan`); in policy TOML `modes` the spelling is `autoEdit`. Built-in `write.toml`: `write_file`, `replace`, `run_shell_command`, `activate_skill`, `web_fetch` are `ask_user` (interactive) and **deny in non-interactive** at priority 10; `autoEdit` allows `write_file`/`replace`/`web_fetch` at 15 (with the allowed-path checker); `yolo.toml` allows `*` at 998; `plan` is read-only. So `-p` with the default mode cannot write at all; `auto_edit` writes but still denies shell. (https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/core/src/policy/policies/yolo.toml; config.ts)

### 3. Granularity ("only these globs / dirs, plus one dir outside the repo")

Yes, at tool-call level, two ways:

**Policy engine (regex over args JSON)** — closest to glob-per-path; `--policy` file, e.g.:
```toml
# deny every path first, allow the whitelisted prefixes above it
[[rule]]
toolName = ["write_file", "replace"]
decision = "deny"
priority = 100
denyMessage = "Writes are restricted to src/**, docs/** and /tmp/yunta-out/**."

[[rule]]
toolName = ["write_file", "replace"]
argsPattern = '"file_path":"/home/u/repo/(src|docs)/[^"]*"'
decision = "allow"
priority = 200

[[rule]]
toolName = ["write_file", "replace"]
argsPattern = '"file_path":"/tmp/yunta-out/[^"]*"'
decision = "allow"
priority = 200

[[rule]]                      # shell can write anywhere; close it or prefix-allow it
toolName = "run_shell_command"
decision = "deny"
priority = 200
```
Run with `gemini -p ... --approval-mode auto_edit --policy ./session.toml --include-directories /tmp/yunta-out --output-format stream-json`. `--include-directories` is required so the tool-level workspace check and the `allowed-path` checker accept the outside dir. Caveats: `argsPattern` sees the raw model args (relative paths, `..`) whereas the tool resolves paths later, so anchor patterns on absolute paths and keep the deny-all backstop; `commandRegex`/`argsPattern` are regex, not globs.

**BeforeTool hook** — a script matched on `write_file|replace|run_shell_command` receives `tool_input.file_path`/`dir_path`/`command`, can canonicalize and glob-match itself, and returns `{"decision":"deny","reason":...}`. Full path/glob freedom, but only configurable via settings files (see 5).

Sandbox and approval modes are whole-directory (mounts/`subpath`) or whole-tool only.

### 4. Denial signal in the stream

Distinct, machine-detectable: a `tool_result` event with `status:"error"` and `error.type:"policy_violation"`:
- policy: `error.message = "Tool execution denied by policy." + (" " + denyMessage)` (https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/core/src/scheduler/policy.ts)
- hook: `error.message = "Tool execution blocked: <reason>"` (hook-utils.ts)
- hook `continue:false`: `error.type:"stop_execution"`, run ends.
Tool-level out-of-workspace rejection (`config.validatePathAccess` in write-file.ts): a `tool_result` error; exact `error.type`/message string **unverified** (`ToolErrorType.PATH_NOT_IN_WORKSPACE = 'path_not_in_workspace'` exists in tool-error.ts). Sandbox denials: **unverified** as a distinct event; expect a shell-tool error / `sandbox_expansion_required`.

### 5. Per-session injection

- **CLI flags**: `--policy <file|dir>` (replaces user policy dir for the session), `--admin-policy`, `--approval-mode`, `--yolo`, `--include-directories`, `--allowed-mcp-server-names`, `--allowed-tools` (deprecated), `--sandbox`, `--output-format`, `--skip-trust`, `--extensions`, `-m`. No flag to pass inline settings JSON (none found in config.ts — **unverified** that none exists elsewhere).
- **Env**: `GEMINI_CLI_HOME=/tmp/job-123` redirects the whole user-level config/state (`~/.gemini` → `$GEMINI_CLI_HOME/.gemini`, incl. `settings.json` and `policies/`), so the harness can generate a throwaway "user" config with hooks + policies without touching the real one (https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/enterprise.md). Also `GEMINI_SANDBOX`, `SEATBELT_PROFILE`, `SANDBOX_MOUNTS`, `GEMINI_CLI_TRUST_WORKSPACE=true`, `GEMINI_CLI_SYSTEM_SETTINGS_PATH`, `GEMINI_CLI_SYSTEM_DEFAULTS_PATH`.
- **Project file**: `.gemini/settings.json` (hooks, `mcpServers`, `tools.*`, `context.includeDirectories`, `policyPaths`); loaded only if the folder is trusted when folder trust is enabled. Workspace `.gemini/policies/` does not work (#18186).
- Mapping: policy → flag or env-home; hooks → settings file only (project or `GEMINI_CLI_HOME`); sandbox → flag/env; approval mode → flag; extra dirs → flag; MCP → settings file + `--allowed-mcp-server-names`.

### 6. Additional directories

Yes: `--include-directories <dir1,dir2>` / `context.includeDirectories`. It widens the workspace for read/write tools and the `allowed-path` checker (`cwd + workspaces`), and feeds Seatbelt `INCLUDE_DIR_0..4` (max 5 writable extra dirs on macOS). For Docker, extra dirs need `SANDBOX_MOUNTS=/dir:/dir:rw` (`tools.sandboxAllowedPaths` mounts read-only). (docs/reference/configuration.md; sandbox.ts; safety/built-in.ts)

### 7. MCP

- Register: `mcpServers.<name>` in settings (`command/args/env/cwd/url/httpUrl/headers/timeout/trust/includeTools/excludeTools`), or `gemini mcp add --scope project|user ...`; session filter `--allowed-mcp-server-names`; `mcp.allowed`/`mcp.excluded`. No flag for an inline server definition found (**unverified**). (https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/tools/mcp-server.md)
- Permissions per MCP tool: policy rules with `mcpName = "srv"` + `toolName = "tool"` (or `toolName = "mcp_srv_*"`), including `argsPattern` and `[[safety_checker]]` with `allowed-path`; hooks match `mcp_<server>_<tool>`; per-server `includeTools`/`excludeTools`.

### 8. Summary

| Mechanism | Level | Path granularity | Denial visible in stream | Settable per session |
|---|---|---|---|---|
| BeforeTool hook | tool-call gate | any (script inspects `tool_input.file_path`; glob-capable) | yes: `tool_result` `status:error`, `error.type:policy_violation`, msg `Tool execution blocked: <reason>` | settings file only: project `.gemini/settings.json` or `GEMINI_CLI_HOME` user settings |
| Policy engine `[[rule]]` + `argsPattern` | tool-call gate | regex over args JSON (prefix/regex, not glob) | yes: `policy_violation`, `Tool execution denied by policy. <denyMessage>` | `--policy file.toml` (also `policyPaths` setting, `GEMINI_CLI_HOME`) |
| `allowed-path` safety checker (built-in in `auto_edit`) | tool-call gate | dir roots = cwd + `--include-directories` | yes: `policy_violation`, bare `Tool execution denied by policy.` (reason dropped) | `--approval-mode auto_edit` + `--include-directories` |
| Tool workspace check (`validatePathAccess`) | tool-call gate (inside tool) | dir roots = workspace dirs | `tool_result` error; exact type/message unverified | `--include-directories` |
| Sandbox (Docker/Podman/Seatbelt/runsc) | filesystem sandbox | dir roots (mounts / `subpath`), ≤5 include dirs on Seatbelt | unverified (shell error / `sandbox_expansion_required`; interactive prompt otherwise) | `--sandbox`, `GEMINI_SANDBOX`, `SANDBOX_MOUNTS`, `SEATBELT_PROFILE` |
| Approval mode / `--allowed-tools` / `tools.core` | tool-call gate, whole tool | none | policy_violation (headless deny rules) | `--approval-mode`, `--yolo`, `--allowed-tools` (deprecated) |

Practical note for the harness: `run_shell_command` bypasses every path-level tool gate (it writes via the shell), so a write constraint needs either a deny/prefix policy on shell, a hook on it, or a real sandbox; and the built-in non-interactive rules already deny shell and writes unless `auto_edit`/`yolo` is set.

---

## GitHub Copilot CLI (v1.0.83)

All evidence is in hand. Sources used: official docs on docs.github.com (fetched via Context7, since docs.github.com is egress-blocked here), the repo changelog (`raw.githubusercontent.com/github/copilot-cli/main/changelog.md`, version headings cited), and **direct verification against the real binary**: I installed `@github/copilot@1.0.83` (platform package `@github/copilot-linux-x64@1.0.83`, released 2026-09-04), read `copilot help`, `copilot help permissions`, `copilot help sandbox`, `copilot help config`, the bundled `schemas/session-events.schema.json`, and ran `copilot -p --output-format json` end-to-end offline against a fake OpenAI-compatible BYOK provider that scripts the model's tool calls. Items marked **[verified locally]** are those runs; **[docs]** cites the page; **unverified** is stated where applicable.

### 1. Headless mode

- **Flags** [verified locally, `copilot help`]: `-p, --prompt <text>` — "Execute a prompt in non-interactive mode (exits after completion)". Prompt can also be **piped on stdin with no `-p`** (`copilot --output-format json <<< "…"` ran a full turn, verified). Related: `-s, --silent` (only the agent response), `--no-ask-user` (disables the `ask_user` tool), `--stream on|off`, `-C <dir>`, `--session-id`, `--resume`, `--continue`, `--model`, `--autopilot`/`--mode autopilot`, `--max-autopilot-continues`, `--attachment`, `--share[=path]`, `--usage-output-file <file>`, `--no-auto-update`, `--log-level`, `--no-color`. Docs: https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference and https://docs.github.com/en/copilot/how-tos/copilot-cli/automate-copilot-cli/run-cli-programmatically.
- **Machine-readable output**: `--output-format json` → "JSONL, one JSON object per line" (help; added in 0.0.422, changelog 2026-03-05). **Event shapes** [verified locally]: each line is the SDK session-event envelope `{"type","data","id","timestamp","parentId","ephemeral"?}` documented at https://docs.github.com/en/copilot/how-tos/copilot-sdk/features/streaming-events (full list of ~130 types in the bundled `schemas/session-events.schema.json`). Observed in `-p` runs: `session.skills_loaded`, `session.mcp_servers_loaded`, `session.tools_updated`, `user.message`, `assistant.turn_start`, `model.call_start/finished`, `assistant.message_start/_delta/message`, `assistant.tool_call_delta`, `tool.execution_start`, `tool.execution_partial_result`, `tool.execution_complete`, `session.info`, `assistant.turn_end`, `assistant.idle`, plus a final non-envelope line `{"type":"result","timestamp","sessionId","exitCode","usage":{...,"codeChanges":{"linesAdded","linesRemoved","filesModified":[...]}}}`.
- **Per-tool-call events: yes.** `tool.execution_start.data` = `{toolCallId, toolName, arguments, turnId, model}`; for MCP tools it adds `mcpServerName`, `mcpToolName`, `toolTitle`; for `bash` it adds `shellToolInfo: {possiblePaths:[…], hasWriteFileRedirection}` [verified locally]. `tool.execution_complete.data` = `{toolCallId, success, result?:{content, detailedContent, contents?}, error?:{message, code}, toolTelemetry}`. Built-in tool names sent to the model [verified]: `bash, read_bash, stop_bash, list_bash, view, create, edit, grep, glob, task, skill, sql, …`; `create` takes `{path, file_text}`, `edit` takes `{path, old_str, new_str}` (absolute paths).
- **Exit code / result**: the `result` line carries `exitCode`; process exit is 0 after a completed turn **even when every tool call was denied** [verified]; nonzero on auth/backend failure (exit 1 with the error on stderr, verified) — changelog 0.0.354 "Exit with nonzero code when `-p` mode fails due to LLM backend errors", 1.0.71 "Exit non-interactive prompt runs with a failure code when a prompt is blocked before responding". In autopilot mode the agent's `task_complete` tool accepts an optional exit code (Context7 `/github/copilot-cli` tools reference). No documented table of numeric codes beyond 0/nonzero (unverified).

### 2. Write-control mechanisms

**(a) Hooks — `preToolUse`** (docs: https://docs.github.com/en/copilot/reference/hooks-reference)
- Config file `{ "version": 1, "hooks": { "preToolUse": [ { "type": "command", "bash": "...", "powershell": "...", "matcher": "regex", "cwd": "...", "env": {...}, "timeoutSec": N } ] } }` (also `"type":"http"` with `url`). Locations (docs, hooks-reference "Hooks locations"): policy `/etc/github-copilot/policy.d/*.json` (root-owned), repo `.github/hooks/*.json`, user `$COPILOT_HOME/hooks/*.json`, inline `hooks` key in `.github/copilot/settings.json` / `.github/copilot/settings.local.json` / `.claude/settings.json` / user `settings.json`, plugin `hooks.json`.
- **Stdin** [verified locally]: `{"sessionId","timestamp","cwd","toolName":"create","toolArgs":{"path":"…","file_text":"…"}}` (PascalCase `PreToolUse` variant gets snake_case `tool_name`/`tool_input`, docs). Env of the hook process [verified]: `COPILOT_CLI=1`, `COPILOT_HOME`, `COPILOT_PROJECT_DIR`, `CLAUDE_PROJECT_DIR`, `COPILOT_MODEL`, plus your `env` map. `matcher` is a full-match regex on tool name (`"create|edit"` verified; changelog 1.0.41 fix).
- **Blocking**: stdout `{"permissionDecision":"deny","permissionDecisionReason":"…"}` (reason required on deny) or **exit code 2** (changelog 1.0.70); a hook error also denies (1.0.57, "fail-closed" per docs); a timeout lets the call proceed (changelog 1.0.67 "Allow tool calls to continue when hooks time out"). `"ask"` in `-p` is a denial [verified]. `modifiedArgs` can rewrite arguments.
- **What the model sees** [verified]: the tool result is the error text `Denied by preToolUse hook: <reason>` (`ask` → `Denied by preToolUse hook (unable to ask user for confirmation): <reason>`; exit 2 → `Denied by preToolUse hook: hook exited with code 2`). Docs add: interactive denial with user feedback appends `Denied by user via preToolUse hook prompt: <reason>. The user provided the following feedback: <feedback>`.
- Caveats: repo-level `.github/hooks` are **not loaded under `-p` in an untrusted cwd** unless `GITHUB_COPILOT_PROMPT_MODE_REPO_HOOKS=true` [verified; changelog 1.0.40/1.0.49]; even pre-listing the cwd in `settings.json.trustedFolders` did not load them in my run [verified]. User-level hooks under `$COPILOT_HOME/hooks/` and inline `hooks` in `$COPILOT_HOME/settings.json` load unconditionally [verified]. Open issue #3874 (hook denial ignored, reported from the VS Code extension, not CLI) — https://github.com/github/copilot-cli/issues/3874; hooks in sub-agents fixed in 1.0.49 (changelog).

**(b) Sandbox** (`copilot help sandbox`; docs https://docs.github.com/en/copilot/concepts/agents/copilot-cli/understanding-local-sandboxing, https://docs.github.com/en/copilot/how-tos/cloud-and-local-sandboxes/configuring-local-sandbox-settings)
- Technology: Microsoft Execution Containers (MXC): macOS Seatbelt (`sandbox-exec`), Linux **bubblewrap** ≥0.5.0 plus `slirp4netns`, util-linux ≥2.35, iptables/ip6tables(-restore), `/dev/net/tun`; Windows ProcessContainer. Experimental: `/sandbox` only registered with `--experimental`; `--sandbox`/`--no-sandbox` flags for one session (changelog 1.0.70, "useful with -p"); persisted key `sandbox.enabled`.
- Filesystem policy is deny-by-default with three levels, keys `sandbox.userPolicy.filesystem.readwritePaths` / `readonlyPaths` / `deniedPaths` (help sandbox) — **absolute paths, whole subtree, "Wildcards are not supported"** (docs, configuring-local-sandbox-settings). Default grants: cwd, PATH dirs, temp dir, user profile, dev-tool caches (`allowDevToolAccess`). Enforcement: OS-level for shell/MCP/LSP child processes; the CLI's own `create`/`edit`/`view` tools "check the same levels in software, without an operating-system backstop" (docs) / "best-effort" (help). `deniedPaths` has no effect on Windows (docs).
- Denied write: "Commands that try to step outside the policy fail" (help); exact error text **unverified** (no bwrap here — with `--sandbox` and bwrap missing the bash tool returned `<error: GenericFailure, Sandboxing is enabled but not supported on this platform…>` with `success:true`, i.e. sandbox failures surface as shell output, not `code:"denied"` [verified]).

**(c) Tool allow/deny lists and policy engine** (`copilot help permissions`; docs https://docs.github.com/en/copilot/how-tos/copilot-cli/use-copilot-cli/allowing-tools)
- Visibility filters: `--available-tools=a,b` / `--excluded-tools=a,b` decide which tools the model sees.
- Permission rules: `--allow-tool=PATTERN`, `--deny-tool=PATTERN`, `--allow-all-tools` (env `COPILOT_ALLOW_ALL=true`); "Denial rules always take precedence over allow rules, even --allow-all-tools". Pattern grammar `kind(argument)`: `shell(cmd)` / `shell(git:*)` (prefix on command stem; git/gh matched at first-level subcommand), `write(path?)` ("tools that create and modify files, except shell tool invocations"; relative path "matches by trailing path components"; absolute path scopes one location), `<mcp-server>(tool?)`, `url(...)`, plus `read`, `memory` (programmatic reference). Malformed patterns are rejected (1.0.71).
- **Path verification** (separate layer): "By default, file access is restricted to paths within the current working directory and its subdirectories, plus the system temporary directory"; `--allow-all-paths` disables it; `--disallow-temp-dir` removes the temp grant; `--add-dir <dir>` extends it. Applies to `create`/`edit`/`view` **and** to shell commands via parsed `possiblePaths` (redirections, `tee`/`cp` targets were all denied outside cwd even with `--allow-all-tools`) [verified]. Docs disclaim: "Scoping of permissions is heuristic and GitHub does not guarantee that all files outside trusted directories will be protected" (about-copilot-cli).
- Glob-capable `permissions: {deny/ask/allow: ["Edit(/src/**)", "Write(...)", "Read(...)", "Shell(...)", "Domain(...)"]}` rules exist **only in enterprise managed settings** (`/etc/github-copilot/managed-settings.json`, root-owned, or MDM/server) — https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-config-dir-reference "Managed permission rules". Putting the same key in user `$COPILOT_HOME/settings.json` or repo `.github/copilot/settings.json` had **no effect** [verified].
- `permissions-config.json` (auto-managed, per-location saved approvals: `locations[path].tool_approvals[{kind:"commands",commandIdentifiers:["git:*"]}]`, `allowed_directories`) "doesn't support deny rules" (docs).

**(d) Approval/permission modes** (`copilot help config` → `defaultPermissionMode`; `/permissions`): `manual` (writes/commands prompted, reads auto), `assisted` (LLM safety judge; `--assisted-approval`, experimental), `allow-all` (`--allow-all`/`--yolo` = `--allow-all-tools --allow-all-paths --allow-all-urls`); enterprise `permissions.disableBypassPermissionsMode: "disable" | "allow-auto-only"` suppresses them. In `-p` there is no prompt: anything not pre-approved is denied with `Permission denied and could not request permission from user` [verified].

### 3. Granularity

**Yes — via `--allow-tool='write(<pattern>)'` + `--add-dir`, at file-glob granularity** [verified locally, v1.0.83; note the docs comment "no glob support yet" is stale — `*` and `**` work]:
```
copilot -p "$PROMPT" --output-format json --no-ask-user \
  --allow-tool='write(src/**)' --allow-tool='write(*.md)' \
  --allow-tool='write(/abs/extra-dir/**)' --add-dir /abs/extra-dir \
  --deny-tool='write(secret/**)' \
  --excluded-tools='bash,read_bash,stop_bash,list_bash'   # or --deny-tool=shell
```
Observed semantics: `write(src/*)` matches `src/a.txt` but not `src/deep/c.txt`; `write(src/**)` matches both; `write(*.md)` matches `notes.md` and `src/x.md`; a bare directory `write(src)` / `write(/abs/src)` matches **nothing** (rules are file patterns matched against trailing components); absolute globs work; `--deny-tool='write(secret/**)'` blocks nested files while `write(secret/*)` does not; writes outside cwd need `--add-dir` (or `--allow-all-paths`) in addition to a matching `write()` rule; `/tmp` is writable by default unless `--disallow-temp-dir`. Two holes you must close yourself: (1) `write()` rules do not govern **shell** — with `--allow-tool=shell` a `python3 -c "open(...)"` inside cwd wrote a file that `write(src/**)` would have blocked (redirection-syntax writes are caught by path parsing, arbitrary program writes are not) [verified]; (2) the sandbox (if enabled) is directory-root granularity only, no globs. A `preToolUse` hook can implement any glob logic (it sees `toolArgs.path` for `create`/`edit` and `toolArgs.command` for `bash`) and is the only mechanism that covers the shell tool by content.

### 4. Denial signal

Distinct, machine-detectable [verified]: `{"type":"tool.execution_complete","data":{"toolCallId":…,"success":false,"error":{"message":"…","code":"denied"}}}`. Messages: `Permission denied and could not request permission from user` (no allow rule, or path outside cwd/`--add-dir`, or MCP tool without rule); `Permission to run this tool was denied due to the following rules: \`write(secret/**)\`` (deny rule, names the rule); `Denied by preToolUse hook: <reason>` (hook). Shell denials additionally carry `toolTelemetry.properties.shell_error_category:"permission_denied"`. **No `permission.requested`/`permission.completed`/`hook.start`/`hook.end` events appear in the `-p` JSONL stream** (they exist in the SDK/schema — `permission.completed.result.kind` ∈ `approved | denied-by-rules | denied-interactively-by-user | denied-no-approval-rule-and-could-not-request-from-user | denied-by-permission-request-hook | denied-by-content-exclusion-policy` — but were not emitted in any of ~35 runs). Process exit stays 0 and `result.codeChanges.filesModified` lists only successful writes. Sandbox denials are **not** `code:"denied"` (they are shell stderr/exit status).

### 5. Injection (per session, without touching the user's global config)

- `COPILOT_HOME=<dir>` (docs, `copilot help environment`) relocates the whole config dir: `settings.json` (incl. inline `hooks`, `sandbox.*`, `trustedFolders`, `disabledMcpServers`), `hooks/*.json`, `mcp-config.json`, `permissions-config.json`, logs, session state. `--config-dir` is the deprecated alias (changelog 1.0.40).
- Flags: `--allow-tool/--deny-tool/--allow-all-*`, `--available-tools/--excluded-tools`, `--add-dir`, `--disallow-temp-dir`, `--sandbox/--no-sandbox`, `--additional-mcp-config '<json>'|@file` (repeatable, later overrides earlier; changelog 0.0.343), `--disable-builtin-mcps`, `--disable-mcp-server`, `--enable-mcp-server`, `--model`, `--secret-env-vars`, `-C`.
- Env: `COPILOT_ALLOW_ALL`, `COPILOT_MODEL`, `COPILOT_GITHUB_TOKEN`/`GH_TOKEN`/`GITHUB_TOKEN`, `GITHUB_COPILOT_PROMPT_MODE_REPO_HOOKS`, `GITHUB_COPILOT_PROMPT_MODE_WORKSPACE_MCP`, `COPILOT_TASK_WAIT_TIMEOUT_SECONDS` (changelog 1.0.71), BYOK `COPILOT_PROVIDER_*`, `COPILOT_OFFLINE`.
- Per-project files: `.github/hooks/*.json`, `.github/copilot/settings.json` / `settings.local.json` (only whitelisted keys: `hooks`, `deniedUrls`, `disabledMcpServers`, `disabledSkills`, `model`, … — **no** `permissions`/`sandbox`; docs "Repository settings" table), `.mcp.json`, `.github/mcp.json`. All repo-level sources are gated on folder trust (docs; verified for hooks).
- Not injectable per session: glob `permissions.*` rules and policy hooks (managed/root-owned only).

### 6. Additional directories / trust

- `--add-dir <directory>` (`copilot help`): "Allow file access to a directory and load its .github/skills and .github/agents as trusted configuration (can be used multiple times)"; relative paths resolve against the session cwd (changelog 1.0.83). Grants read **and write** path access (write still needs a `write` permission rule) [verified]. Interactive equivalent `/add-dir`, persisted in `permissions-config.json.locations[..].allowed_directories`.
- Trusted directories (docs https://docs.github.com/en/copilot/concepts/agents/copilot-cli/about-copilot-cli#trusted-directories): interactive launch asks to trust cwd (session or persistent, stored in `settings.json.trustedFolders`); trust gates loading of repo hooks, workspace MCP, repo settings, custom instructions. Under `-p` no prompt is shown and the run proceeds in an untrusted cwd [verified], with repo-level hooks/MCP skipped unless the env vars above are set (docs add-mcp-servers "prompt mode cannot show an interactive trust prompt"). Trust is not itself a write boundary — that is the cwd/temp/`--add-dir` path check plus rules.

### 7. MCP

- Per-session registration: `--additional-mcp-config '{"mcpServers":{"name":{"type":"local","command":"…","args":[…],"tools":["*"]}}}'` or `@file` [verified: server connected, tool exposed to the model as `<server>-<tool>`, e.g. `fakemcp-echo_tool`]. Also `$COPILOT_HOME/mcp-config.json`, `.mcp.json`; `tools:[…]` in the server entry filters exposed tools (docs add-mcp-servers); `--disable-builtin-mcps` removes the bundled GitHub MCP server.
- Permissions: `--allow-tool='server'` (all tools) / `--allow-tool='server(tool)'`, `--deny-tool='server(tool)'` (wins over `--allow-all-tools`) [verified: without a rule → `code:"denied"`; with `fakemcp(echo_tool)` → success; deny under `--allow-all-tools` → "denied due to the following rules"]. Events carry `mcpServerName`/`mcpToolName`. Enterprise `allowedMcpServers`/`deniedMcpServers` exist in managed settings (docs enterprise-managed-settings).

### 8. Summary

| Mechanism | Level | Path granularity | Denial visible in stream | Settable per session |
|---|---|---|---|---|
| `--allow-tool='write(glob)'` / `--deny-tool='write(glob)'` | tool-call gate (built-in `create`/`edit` only; not shell) | glob (`*`, `**`, `*.md`, absolute or trailing-component; no bare-dir match) | yes: `tool.execution_complete` `success:false`, `error.code:"denied"`, message names the rule | flags (`--allow-tool`/`--deny-tool`), `COPILOT_ALLOW_ALL` |
| cwd/temp/`--add-dir` path verification | tool-call gate (built-in file tools + shell `possiblePaths` heuristic) | dir roots (cwd, temp, each `--add-dir`) | yes: same event, message "Permission denied and could not request permission from user" | `--add-dir`, `--allow-all-paths`, `--disallow-temp-dir`, `-C` |
| `preToolUse` hook | tool-call gate (all tools incl. shell/MCP) | anything the script decides (glob, content) | yes: same event, message `Denied by preToolUse hook: <reason>`; no `hook.*` events in `-p` stream | `$COPILOT_HOME/hooks/*.json` or inline `hooks` in `$COPILOT_HOME/settings.json`; repo `.github/hooks` needs `GITHUB_COPILOT_PROMPT_MODE_REPO_HOOKS=true` |
| Local sandbox (MXC/bwrap) | filesystem sandbox (OS-enforced for shell/MCP/LSP; software-only for built-in file tools) | dir roots (`readwritePaths`/`readonlyPaths`/`deniedPaths`, absolute, subtree, no wildcards) | no distinct event: shell failure text/exit status inside a `success:true` result; exact message unverified | `--sandbox` + `sandbox.*` keys in `$COPILOT_HOME/settings.json`; needs `--experimental` and bwrap+netns tooling |
| Managed `permissions.{allow,ask,deny}` (`Edit(/src/**)`…) | tool-call gate | glob | yes (rule denial message; `denied-by-rules`) | no — root-owned `/etc/github-copilot/managed-settings.json` / MDM only; ignored in user/repo settings |
| `--available-tools` / `--excluded-tools` | tool visibility (model never sees tool) | none | n/a (tool absent) | flags |

Recommended harness stack for "may write only these globs, plus one extra dir": `--excluded-tools` (or `--deny-tool=shell`) to remove the shell, `--allow-tool='write(<globs>)'` + `--add-dir <extra>` + `--allow-tool='write(<extra>/**)'`, a `preToolUse` hook under `COPILOT_HOME` as the second, content-aware gate, and detect `error.code == "denied"` on `tool.execution_complete`. Verified on v1.0.83 (2026-09-04); rule semantics are declared subject to change by GitHub ("It is expected that these permissions will be extended in the very near future to support wildcard matching", `copilot help permissions`).

---

## Cursor CLI (`agent`, alias `cursor-agent`)

Sourcing note: cursor.com / docs.cursor.com / forum.cursor.com are blocked by this session's egress proxy, so official-doc text comes through Context7's index of cursor.com (`/websites/cursor_cli`, `/websites/cursor`, `/cursor/cookbook`), with page URLs as cited by the index. Third-party pages I could reach are marked as such. Anything I could not confirm is marked **unverified**.

### 1. Headless mode

- **Flag**: `-p` / `--print` runs non-interactively and "provides access to all tools, including write and shell capabilities" (https://cursor.com/docs/cli/reference/parameters). The prompt is a positional string argument (`agent -p "..."`) (https://cursor.com/docs/cli/headless, https://cursor.com/docs/cli/using).
- **Prompt from stdin**: **unverified**. No official example of `cat x | agent -p` surfaced in the docs index; only the positional argument form is documented. Marketing text says the CLI can be "piped into other tools" (https://www.learncursor.dev/guides/cursor-cli, third-party, unreachable to verify).
- **`--force` / `--yolo`**: the headless page says "Without `--force`, changes are only proposed, not applied" and `--yolo` is an alias of `--force` (https://cursor.com/docs/cli/headless, https://cursor.com/docs/cli/reference/parameters). This contradicts the `using` page ("In non-interactive mode, Cursor has full write access", https://cursor.com/docs/cli/using) and the parameters page quoted above. Treat as: `-p --force` is the documented way to guarantee writes are applied.
- **Output format**: `--output-format text|json|stream-json`, only with `--print` (https://cursor.com/docs/cli/reference/parameters).
  - `json`: one object on success: `{"type":"result","subtype":"success","is_error":false,"duration_ms":…,"duration_api_ms":…,"result":"<full assistant text>","session_id":"<uuid>","request_id":"…"}`; "does not emit deltas or tool events"; on failure "exits with a non-zero status code and writes an error to stderr, without emitting a JSON object" (https://cursor.com/docs/cli/reference/output-format).
  - `stream-json`: NDJSON, one event per line. Event types documented: `system` (`subtype:"init"`, fields `apiKeySource`, `cwd`, `session_id`, `model`, `permissionMode`), `user`, `assistant`, `tool_call` (`subtype:"started"|"completed"`), `result`. `thinking` events are suppressed in print mode. `--stream-partial-output` adds text deltas (https://cursor.com/docs/cli/reference/output-format).
  - **Per-tool-call events with name + args: yes.** `tool_call` events carry `call_id` and a `tool_call` object keyed by tool kind, e.g. `{"readToolCall":{"args":{"path":"README.md"}}}` and `{"writeToolCall":{"args":{"path":"summary.txt","fileText":"…","toolCallId":"…"}}}`; the `completed` event adds `result.success` (for write: `{"path":"/abs/path","linesCreated":19,"fileSize":942}`) (https://cursor.com/docs/cli/reference/output-format). Other kinds seen in the wild by a third-party wrapper: `shellToolCall`, `editToolCall`, `listToolCall`, `searchToolCall` (https://github.com/veictry/cursor-cli, third-party). Official docs only show `readToolCall` and `writeToolCall` shapes; other shapes are **unverified**.
- **Exit code / result**: success ends with the terminal `result` event (`is_error:false`) and exit 0; "On failure, the process exits with a non-zero code and the stream may end early without a terminal event" (https://cursor.com/docs/cli/reference/output-format). No specific non-zero code table is documented.

### 2. Write-control mechanisms

**(a) Hooks** (https://cursor.com/docs/hooks)
- Config file: `~/.cursor/hooks.json` (user) or `<project>/.cursor/hooks.json` (project); `{"version":1,"hooks":{"<event>":[{"command":"…","matcher":"…","timeout":N,"failClosed":true}]}}`. Command hooks get JSON on stdin and return JSON on stdout; exit `0` = use JSON, exit `2` = block (same as `permission:"deny"`), other codes fail-open unless `failClosed:true`.
- Events relevant to writes: there is **no dedicated `beforeFileEdit`/`beforeWrite` hook**. The pre-write gate is the generic `preToolUse`, which "fires for all tool types (Shell, Read, Write, MCP, Task, etc.)" and can be filtered with `"matcher": "Shell|Read|Write"`. `afterFileEdit` is post-hoc only (input `{"file_path":"<absolute path>","edits":[{"old_string":…,"new_string":…}]}`). `beforeReadFile` gates reads (input includes `file_path`, `content`, common fields; output `permission` + `user_message`). `beforeShellExecution` input `{"command","cwd","sandbox"}`; `beforeMCPExecution` input `{"tool_name","tool_input","mcp_server_name",…}`; both output `{"permission":"allow"|"deny"|"ask","user_message","agent_message"}`.
- `preToolUse` stdin: common fields (`conversation_id`, `generation_id`, `hook_event_name`, `workspace_roots`, `cwd`, `transcript_path`, …) plus `tool_name` (e.g. `"Shell"`), `tool_input` (object), `tool_use_id`. The exact `tool_input` shape for the Write/Edit tool is not documented — **unverified** (the stream-json `writeToolCall.args` has `path`/`fileText`, which is the likely shape but not stated for hooks).
- Deny: return `{"permission":"deny","user_message":"…","agent_message":"…"}` (or Claude Code format `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"…"}}`, accepted per https://cursor.com/docs/reference/third-party-hooks and the April 2026 CLI changelog). `"ask"` is "accepted by the schema but not enforced for `preToolUse`". `updated_input` can rewrite the tool input.
- What the model sees: `agent_message` is "Message fed back to the agent when the action is denied" (https://cursor.com/docs/hooks). A third-party guide claims the CLI's `preToolUse` handler reads only `user_message` and ignores `agent_message` (https://ntorres.dev/blog/cursor-hooks-json-guide, unreachable, **unverified**); safest is to put the reason in both fields.
- CLI support: CLI changelog says hooks are supported (Jan 2026: session start/end/stop; April 2026: "Hooks fire reliably … Claude Code-format hook responses are accepted", https://cursor.com/docs/cli/changelog). A January 2026 forum thread reported the CLI emitting only `beforeShellExecution`/`afterShellExecution` (https://forum.cursor.com/t/cursor-cli-doesnt-send-all-events-defined-in-hooks/148316); a later bug report shows users running `preToolUse`/`beforeReadFile` with `failClosed:true` in the CLI, with a Windows-specific failure (https://forum.cursor.com/t/hooks-not-firing-cannot-have-guardrails/168407) and another showing `preToolUse` firing in the CLI for all tools except `AskQuestion` (https://forum.cursor.com/t/cursor-cli-askquestion-tool-skips-pretooluse-and-posttooluse-hooks/161836). There is no official per-event CLI support matrix (only a cloud-agent matrix exists); treat `preToolUse` in the CLI as "works per user reports, not officially tabulated".

**(b) Sandbox** (https://cursor.com/docs/reference/sandbox, https://cursor.com/docs/sdk/typescript, https://cursor.com/docs/cli/reference/parameters)
- Technology: `bubblewrap` on Linux, `seatbelt` (sandbox-exec) on macOS.
- Scope: it constrains **shell tool calls and shell-spawned processes**, not the agent's own file tools: "constrains every shell tool call and shell-spawned process … Writes are limited to the working directory, temp directories, and paths you allow in sandbox.json. Reads are not restricted to the workspace." Network denied by default.
- Enable: `--sandbox enabled|disabled` flag, or `agent sandbox enable|disable|reset`; `agent sandbox run <cmd> --allow-paths … --readonly-paths … --blocked-patterns … --network` runs one command sandboxed. Feb 2026 changelog: sandbox policy files are global or per-project, and "the CLI provid[es] immediate feedback if sandboxing is required but unavailable" (https://cursor.com/docs/cli/changelog).
- Policy file `sandbox.json` at `~/.cursor/sandbox.json` (lower priority) or `<workspace>/.cursor/sandbox.json` (higher); fields `type` (`workspace_readwrite` default, `workspace_readonly`, `insecure_none`), `additionalReadwritePaths`, `additionalReadonlyPaths` (only under `workspace_readwrite`), `networkPolicy`, `enableSharedBuildCache`. Protected paths are always write-blocked regardless of config (`.cursor` config files, `.claude`, `.vscode`, `.git/hooks`, `.git/config`, `.cursorignore`; `.cursor/rules|commands|worktrees|skills|agents` stay writable).
- Can it restrict writes to specific paths? Only at the level of "workspace root + temp + extra roots" / "read-only workspace" / gitignore-style `--blocked-patterns` for `sandbox run`. There is no "writable only under `src/foo/**`" in `sandbox.json`; and it does not cover the Write/Edit tool.

**(c) Permissions config** (https://cursor.com/docs/cli/reference/permissions, https://cursor.com/docs/cli/reference/configuration)
- Files: `~/.cursor/cli-config.json` (global; `CURSOR_CONFIG_DIR` or `XDG_CONFIG_HOME` override the directory) and `<project>/.cursor/cli.json` (project; "Project-level configuration is limited to permissions").
- Shape: `{"permissions":{"allow":[…],"deny":[…]}}` with tokens `Shell(cmd)` / `Shell(cmd:args-glob)`, `Read(pathOrGlob)`, `Write(pathOrGlob)`, `WebFetch(host)`, `Mcp(server:tool)`.
- Globs: `**`, `*`, `?` supported; "Relative paths are scoped to the current workspace, while absolute paths can target files outside the project. Deny rules always take precedence over allow rules." Example tokens: `Write(src/**)`, `Write(docs/**/*)`, `Write(**/*.key)`, `Write(**/.env*)`, `Read(src/**/*.ts)`.
- Precedence between files: docs say deny > allow; a forum bug report states the project-level `.cursor/cli.json` takes precedence over the global file (https://forum.cursor.com/t/cli-permission-allowlist-written-to-global-config-instead-of-project-level-cursor-cli-json/160343, unreachable; **unverified**). Whether the lists are merged or replaced is not documented.
- The always-write-blocked list for CLI permissions (`.cursor/*.json`, `.vscode/**`, `.git/hooks/**`, `.cursorignore`) came only from a search snippet; the official statement I can cite is the sandbox reference's protected-paths list above — **partly unverified** for the non-sandbox path.

**(d) Modes** (https://cursor.com/docs/cli/reference/parameters, https://cursor.com/docs/cli/using, https://cursor.com/docs/cli/acp, https://cursor.com/docs/agent/security/run-modes)
- `--mode plan` (`--plan`) and `--mode ask`: read-only behaviour; default mode = full tool access.
- `--force` / `--yolo`: apply edits without confirmation in print mode.
- Run modes (Allowlist / Auto-review / Run Everything) exist in the CLI: `approvalMode` in `cli-config.json` (`allowlist` | `unrestricted`, plus `auto-review` since June 2026 with `--auto-review` flag). Auto-review routes Shell/MCP/Fetch through a classifier; in headless runs "a call the classifier blocks is denied rather than escalated, and the agent gets the block reason" (https://cursor.com/docs/sdk/typescript). The classifier "is best-effort convenience, not a security boundary".
- `--trust` trusts the workspace without prompting; `--approve-mcps` auto-approves MCP servers.

### 3. Granularity

**Yes, via permissions config only.** `Write(glob)` supports arbitrary globs, relative globs are workspace-scoped, absolute paths reach outside the repo, and deny wins. Sketch:

```json
// <project>/.cursor/cli.json  (or ~/.cursor/cli-config.json)
{
  "permissions": {
    "allow": [
      "Write(src/engine/**)",
      "Write(tests/**/*.rs)",
      "Write(/abs/path/outside/repo/**)"
    ],
    "deny": [
      "Write(**/*.key)",
      "Write(**/.env*)"
    ]
  }
}
```
Caveats: (1) whether `allow` is an exclusive allowlist for `Write` (i.e. unlisted paths are refused rather than prompted) in `-p --force` mode is not stated in the docs — **unverified**; the headless docs describe `--force` as "permits the agent to make direct file changes without requiring user confirmation" (https://cursor.com/docs/cli/headless) and a third-party user reports "commands off the allow list fail silently" in print mode (https://github.com/adamk33n3r/cursor-local-remote/issues/27). (2) `Write()` gates the agent's file tools; a shell command (`echo > file`) is governed by `Shell()` rules and, if enabled, the sandbox — which is only dir-root granular. (3) Hooks (`preToolUse` matcher `Write`) give arbitrary granularity in your own script, but no path expression in `hooks.json` itself. Sandbox: dir roots only.

### 4. Denial signal in the stream

**Not documented.** The output-format reference only shows `tool_call` `completed` with `result.success`; no `error`/`rejected` variant, no `permission_denied` event, and no field on `result` other than `is_error` is documented (https://cursor.com/docs/cli/reference/output-format). The stream-json `system.init` event carries `permissionMode`. Third-party evidence says print mode has "no permission round-trip" and off-allowlist calls "fail silently" (https://github.com/adamk33n3r/cursor-local-remote/issues/27). For hook denials the agent is fed `agent_message`, so the refusal is visible only as the assistant's subsequent prose unless the `completed` event carries a non-`success` result — **unverified**. Practical detection: correlate `writeToolCall` `started` events (which include the target `path`) with the presence/absence of a `completed`+`success` for the same `call_id`, and have your hook log denials out-of-band.

### 5. Injection for one session

- Config directory: `CURSOR_CONFIG_DIR` (or `XDG_CONFIG_HOME`) env var points the CLI at a private directory containing `cli-config.json` (permissions, `approvalMode`), and — by location convention — `hooks.json`, `sandbox.json`, `mcp.json` under that dir. Only `cli-config.json` is explicitly documented to honour the env var (https://cursor.com/docs/cli/reference/configuration); whether `hooks.json`/`sandbox.json`/`mcp.json` follow `CURSOR_CONFIG_DIR` is **unverified**.
- Per-project files inside the workspace: `.cursor/cli.json` (permissions only), `.cursor/hooks.json`, `.cursor/sandbox.json`, `.cursor/mcp.json`, `.cursor/permissions.json` (Auto-review allowlists/instructions). These touch the repo tree, not the user's global config; `--worktree` puts the run in a fresh worktree under `~/.cursor/worktrees/` (https://cursor.com/docs/cli/using).
- CLI flags: `--force`/`--yolo`, `--mode`, `--sandbox enabled|disabled`, `--auto-review`, `--approve-mcps`, `--trust`, `--workspace <dir>`, `--model`, `--api-key`/`CURSOR_API_KEY`, `--output-format`, `--resume`/`--continue` (https://cursor.com/docs/cli/reference/parameters). No flag takes inline permission JSON or an inline hook — **no `--permissions`/`--hooks-json` equivalent found**.
- Hooks are "file-based only … a project policy boundary, not a per-run knob" (https://cursor.com/docs/sdk/python).

### 6. Additional directories

**No `--add-dir` equivalent.** `--workspace <dir>` sets a single root (https://cursor.com/docs/cli/reference/parameters); multiple workspaces is an open feature request (https://forum.cursor.com/t/support-multiple-workspace-paths-in-cursor-agent-cli/170291). Access outside the root is expressed instead by absolute-path `Read(/abs/**)`/`Write(/abs/**)` permission tokens (https://cursor.com/docs/cli/reference/permissions) and, for sandboxed shell, `sandbox.json` `additionalReadwritePaths`/`additionalReadonlyPaths` or `agent sandbox run --allow-paths` (https://cursor.com/docs/reference/sandbox).

### 7. MCP

- Registration: `.cursor/mcp.json` at project or user level; "The CLI follows the same configuration precedence as the editor (project → global → nested), automatically finding configurations from parent directories" (https://cursor.com/docs/cli/mcp). Management: `agent mcp list|list-tools <id>|login <id>|enable <id>|disable <id>`. `--approve-mcps` skips approval prompts; April 2026: global servers can be auto-approved while "project-level servers maintain strict approval requirements" (https://cursor.com/docs/cli/changelog). No per-session `--mcp-config` flag was found — **unverified/none**.
- Permissions: `Mcp(server:tool)` tokens with wildcards — `Mcp(datadog:*)`, `Mcp(*:search)`, `Mcp(*:*)` — in `allow`/`deny` (https://cursor.com/docs/cli/reference/permissions). `beforeMCPExecution` hook can also deny per call with `tool_name`/`tool_input`/`mcp_server_name` on stdin (https://cursor.com/docs/hooks).

### 8. Summary table

| Mechanism | Level | Path granularity | Denial visible in stream | Settable per session |
|---|---|---|---|---|
| `permissions.allow/deny` (`Write(glob)`, `Read(glob)`, `Shell()`, `Mcp()`) in `cli-config.json` / `.cursor/cli.json` | Tool-call gate (agent file/shell/MCP tools) | Glob (`**`,`*`,`?`), relative = workspace, absolute = outside repo; deny > allow | No documented event; third-party reports silent failure; **unverified** | Yes: `CURSOR_CONFIG_DIR` → private `cli-config.json`, or project `.cursor/cli.json` |
| Hooks `preToolUse` (matcher `Write`), `beforeReadFile`, `beforeShellExecution`, `beforeMCPExecution` | Tool-call gate (script decides) | Arbitrary (your script); no path syntax in `hooks.json` | Only via `agent_message`/`user_message` fed to the agent → prose; no distinct event documented | File-based only: `.cursor/hooks.json` in workspace or `~/.cursor/hooks.json` (env-var relocation **unverified**) |
| Sandbox (`--sandbox enabled`, `sandbox.json`, `agent sandbox run`) | Filesystem sandbox for shell subprocesses only (bubblewrap/seatbelt) | Dir roots: workspace rw/ro + `additionalReadwritePaths`/`additionalReadonlyPaths`; `--blocked-patterns` only on `sandbox run` | Shell command fails inside sandbox; surfaces as shell tool output, not a dedicated event (**unverified**) | Flag `--sandbox`, project `.cursor/sandbox.json` |
| Modes `--mode plan|ask` | Tool-level (read-only) | None | n/a | Flag |
| `--force`/`--yolo`, `approvalMode`, `--auto-review` | Approval policy, not a path constraint (classifier is "not a security boundary") | None | Auto-review: agent "gets the block reason" (prose) | Flag / `cli-config.json` |
| `--add-dir` equivalent | None (`--workspace` is single-root) | — | — | Absolute-path `Write(/abs/**)` + sandbox extra paths instead |

Sources: [Headless](https://cursor.com/docs/cli/headless), [Output format](https://cursor.com/docs/cli/reference/output-format), [Parameters](https://cursor.com/docs/cli/reference/parameters), [Permissions](https://cursor.com/docs/cli/reference/permissions), [Configuration](https://cursor.com/docs/cli/reference/configuration), [Using](https://cursor.com/docs/cli/using), [MCP](https://cursor.com/docs/cli/mcp), [ACP](https://cursor.com/docs/cli/acp), [CLI changelog](https://cursor.com/docs/cli/changelog), [Hooks](https://cursor.com/docs/hooks), [Third-party hooks](https://cursor.com/docs/reference/third-party-hooks), [sandbox.json](https://cursor.com/docs/reference/sandbox), [permissions.json](https://cursor.com/docs/reference/permissions), [Run modes](https://cursor.com/docs/agent/security/run-modes), [TypeScript SDK](https://cursor.com/docs/sdk/typescript), [Python SDK](https://cursor.com/docs/sdk/python), [GitHub Actions](https://cursor.com/docs/cli/github-actions), [cursor-local-remote #27](https://github.com/adamk33n3r/cursor-local-remote/issues/27), [veictry/cursor-cli](https://github.com/veictry/cursor-cli), [endorlabs/cursor-hook-examples](https://github.com/endorlabs/cursor-hook-examples), [forum: CLI hooks events](https://forum.cursor.com/t/cursor-cli-doesnt-send-all-events-defined-in-hooks/148316), [forum: hooks not firing](https://forum.cursor.com/t/hooks-not-firing-cannot-have-guardrails/168407), [forum: AskQuestion skips preToolUse](https://forum.cursor.com/t/cursor-cli-askquestion-tool-skips-pretooluse-and-posttooluse-hooks/161836), [forum: cli.json precedence](https://forum.cursor.com/t/cli-permission-allowlist-written-to-global-config-instead-of-project-level-cursor-cli-json/160343), [forum: multiple --workspace](https://forum.cursor.com/t/support-multiple-workspace-paths-in-cursor-agent-cli/170291).

---

## OpenCode (v1.18.30, `dev` @ df23b7f, 2026-09-13)

Sources: opencode.ai itself is blocked by this session's egress proxy, so every doc claim below cites the docs' source file in the repo (`packages/web/src/content/docs/*.mdx` on github.com/anomalyco/opencode, which is what opencode.ai/docs renders) plus the corresponding opencode.ai URL, and code claims cite the file in the same checkout. Context7 (`/anomalyco/opencode`) was used first and agreed with the raw files. Nothing was executed at runtime; behavior described from code is marked "(code)".

### 1. Headless mode

**`opencode run [message..]`** (docs: `cli.mdx` §run → https://opencode.ai/docs/cli/#run; code: `packages/opencode/src/cli/cmd/run.ts`)

- Prompt: positional args joined with spaces; **stdin is read when not a TTY** and concatenated after the args (`resolveRunInput`: `value + "\n" + piped`) (code, run.ts). Files: `--file/-f` (repeatable).
- Flags (docs table): `--command`, `--continue/-c`, `--session/-s`, `--fork`, `--share`, `--model/-m provider/model`, `--agent`, `--format default|json`, `--file`, `--title`, `--attach <url>`, `--password`, `--username`, `--dir`, `--port`, `--variant`, `--thinking`, `--auto` ("Auto-approve permissions that are not explicitly denied"). Hidden aliases of `--auto`: `--yolo`, `--dangerously-skip-permissions` (code, run.ts builder). Global flags: `--print-logs`, `--log-level` (cli.mdx L670–671).
- **`--format json`** = "raw JSON events", one JSON object per line on stdout (code, `emit()` in run.ts): `{ "type", "timestamp", "sessionID", ...data }`. Emitted types (code): `tool_use` (`{part: ToolPart}`, emitted only when `part.state.status` is `completed` or `error`), `step_start`, `step_finish` (`{part}`; step-finish carries `cost`, `tokens`), `text` (`{part}`, on completed text parts), `reasoning` (only with `--thinking`), `error` (`{error}`). **Per-tool-call events do include tool name and arguments**: `ToolPart` has `tool` (name) and `state.input` (args), plus `state.output`/`state.metadata` on completion and `state.error` (string) on error (`packages/schema/src/v1/session.ts` `ToolStateCompleted`/`ToolStateError`). No `pending`/`running` transitions are emitted in JSON mode, and **`permission.asked` is not emitted** (see §4).
- Non-interactive default (code, run.ts): session is created with a ruleset denying `question`, `plan_enter`, `plan_exit`; any `permission.asked` event is auto-replied `reject` (or `once` with `--auto`) and a line `permission requested: <perm> (<patterns>); auto-rejecting` is printed via `UI.println` → **stderr** (`cli/ui.ts` `println` writes to stderr).
- Exit code (code): the loop ends on `session.status` `idle`; `process.exitCode = 1` if a `session.error` event was seen for the session, if the prompt/command HTTP call returned an error, or if the event loop threw; otherwise 0. A denied/rejected tool call alone does **not** change the exit code. The docs do not document exit codes (cli.mdx has no exit-code section) — this is code-only.
- **`opencode serve`** (docs: `server.mdx` → https://opencode.ai/docs/server/): HTTP API with `GET /event` (SSE, "First event is `server.connected`, then bus events") and `GET /global/event`; `POST /session` (docs list body `{ parentID?, title? }` but the SDK/schema also accept `agent`, `model`, `metadata`, **`permission: PermissionRuleset`**, `workspaceID` — `packages/sdk/js/src/v2/gen/types.gen.ts` `SessionCreateData`, `packages/opencode/src/session/session.ts` `CreateInput`); `POST /session/:id/message` (sync, returns `{info, parts}`), `POST /session/:id/prompt_async` (204), `POST /session/:id/permissions/:permissionID` body `{ response: once|always|reject, remember? }`. Basic auth via `OPENCODE_SERVER_PASSWORD`. `opencode run --attach http://host:port` reuses a server. Also `opencode acp` (ACP over stdin/stdout nd-JSON, cli.mdx §acp) — not investigated further.

### 2. Write-control mechanisms

**(a) Plugins/hooks** (docs: `plugins.mdx` → https://opencode.ai/docs/plugins/; types: `packages/plugin/src/index.ts`)
- Loaded from `.opencode/plugins/*.{js,ts}` (project), `~/.config/opencode/plugins/` (global), or the config `plugin` array (npm spec, `./local-plugin.ts` relative to the declaring config, or `file:///abs/plugin.js` — `packages/core/src/plugin/skill/customize-opencode.md`). Load order: global config → project config → global dir → project dir; "all hooks run in sequence".
- `"tool.execute.before"(input: { tool, sessionID, callID }, output: { args })` — fires for every tool (built-in, plugin, MCP) before permission evaluation (code: `session/tools.ts`; the MCP wrapper is explicitly `tool.execute.before → ctx.ask → execute → tool.execute.after`). Denial = **throw** (docs example: `throw new Error("Do not read .env files")`). The thrown error propagates as an AI-SDK `tool-error`; `processor.ts` `failToolCall` sets the part to `state.status="error"`, `state.error = errorMessage(error)` — so **the model sees the thrown message verbatim as the tool result error**, and continues the turn. `output.args` is mutable, so a plugin can also rewrite the path/command instead of denying.
- `"permission.ask"(input: Permission, output: { status: "ask"|"deny"|"allow" })` is declared in the `Hooks` type (plugin/src/index.ts L261) and listed in the built-in customize skill, but in this checkout **no code triggers it**: the only `plugin.trigger(...)` sites are `tool.execute.before/after`, `tool.definition`, `chat.*`, `shell.env`, `command.execute.before`, `experimental.*` (`packages/opencode/src/permission/index.ts` never calls the plugin service). Whether a released build wires it is **unverified**; do not rely on it.
- Plugins receive `{ project, client (SDK), $ (Bun shell), directory, worktree, serverUrl }`.

**(b) Sandbox** — none. No OS-level sandbox exists: grep for bwrap/seatbelt/sandbox-exec/landlock/firejail/nsjail in `packages/opencode/src` and `packages/core/src` returns nothing; the word "sandbox" in `project/project.ts` means extra project directories (`sandboxes`), and in `tool/code-mode.ts` refers to the `@opencode-ai/codemode` tool namespace. All enforcement is a tool-call gate in-process; the `bash` tool runs a real shell with the user's env (`tool/shell.ts`).

**(c) `permission` config** (docs: `permissions.mdx` → https://opencode.ai/docs/permissions/; `agents.mdx` §Permissions; code: `permission/index.ts`, `agent/agent.ts`)
- Actions `"allow" | "ask" | "deny"`. Top level may be a string (`"permission": "allow"`) or an object keyed by permission name; `"*"` sets all. Keys: `read, edit, glob, grep, list, bash, task, external_directory, todowrite, webfetch, websearch, lsp, skill, question, doom_loop`. `edit` gates `write`, `edit`, `apply_patch` (agents.mdx table). Keys are themselves wildcard-matched against tool names, so `"mymcp_*": "deny"` works.
- Object form: `"edit": { "*": "deny", "packages/web/src/content/docs/*.mdx": "allow" }` (verbatim docs example). Allowed for `read, edit, glob, grep, list, bash, task, external_directory, lsp, skill`; `todowrite, question, webfetch, websearch, doom_loop` are flat only (agents.mdx).
- Matching (docs §Wildcards + code `core/util/wildcard.ts`): `*` → `.*` (matches across `/`, so `**` ≡ `*`), `?` → `.`, everything else literal; anchored `^…$`; a trailing `" *"` also matches the bare command. `~/`, `~`, `$HOME/` at pattern start expand to the home dir (`expand()` in permission/index.ts).
- Precedence (docs + code `evaluate()`): rules are flattened in insertion order — defaults, then global `permission`, then agent `permission`, then the session ruleset — and `findLast` wins: **last matching rule wins**, so put `"*"` first. Unmatched → `ask`. Defaults (code, agent.ts): `"*": allow`, `doom_loop: ask`, `external_directory: {"*": ask, <tmp/skills/references dirs>/*: allow}`, `read: {"*": allow, "*.env": ask, "*.env.*": ask, "*.env.example": allow}` (docs say `.env` is `deny`; code says `ask` — minor docs/code drift).
- **What is matched for `edit`** (code, `tool/edit.ts`, `write.ts`, `apply_patch.ts`): the pattern is `path.relative(instance.worktree, absoluteFilePath)` — i.e. the path relative to the **git worktree root** (not cwd), e.g. `src/a.rs`; a file outside the worktree yields `../x/y.md`. The docs example `"edit": {"~/projects/personal/**": "deny"}` would be compared against a `../…` relative string, so an absolute/home pattern on `edit` does not appear to match in this code (**unverified at runtime**; treat as a discrepancy to test).
- `bash` matches the parsed command text (`git status --porcelain`), one rule per simple command; `read` matches worktree-relative path; `external_directory` matches `<dir>/*` absolute globs (see §6).
- `OPENCODE_PERMISSION` env var: "Inlined json permissions config" (cli.mdx L691), deep-merged over `result.permission` after all files (code, config.ts L559).

**(d) Agents** (docs: `agents.mdx` → https://opencode.ai/docs/agents/): built-ins `build` (all tools), `plan` (code: `edit: {"*": deny, ".opencode/plans/*.md": allow, …}`, `bash` etc.; docs say edits/bash "set to ask" — docs and code differ, plan is `deny` in code), subagents `general`, `explore`, `scout`. Custom agents via `agent.<name>` in JSON or `.opencode/agents/<name>.md` frontmatter, with `permission` (same object syntax), `mode: primary|subagent|all`, `model`, `prompt`, `steps`. Select at run time with `opencode run --agent <name>` (must be `primary`/`all`; a `subagent` falls back to default with a warning — code). Subagent sessions spawned by `task` inherit only the parent **session** ruleset's `deny` and `external_directory` rules, not the parent agent's rules (`agent/subagent-permissions.ts`).

### 3. Granularity

Yes — file-path granularity via `permission.edit` object rules (worktree-relative globs) plus `permission.external_directory` (absolute dir globs) for a directory outside the repo. Whole-tool `deny` is also available but not required. Snippet (syntax from permissions.mdx; the `edit` pattern for the outside dir uses the worktree-relative form the code actually matches — if `/work/repo` is the worktree and `/work/out` the extra dir, `path.relative` gives `../out/…`):

```json
{
  "$schema": "https://opencode.ai/config.json",
  "permission": {
    "edit": {
      "*": "deny",
      "src/*": "allow",
      "docs/*.md": "allow",
      "../out/*": "allow"
    },
    "external_directory": {
      "*": "deny",
      "/work/out/*": "allow"
    },
    "bash": "deny",
    "question": "deny"
  }
}
```

Caveats: (1) `bash` is not path-gated by `edit` — a shell command can write anywhere, so a write-constrained run must `deny` bash or allowlist read-only prefixes (`"bash": {"*": "deny", "git status *": "allow"}`); `external_directory` only catches bash paths for a fixed command list (`rm, cp, mv, mkdir, touch, cd, …`, `tool/shell.ts` `FILES`). (2) Same for MCP tools that write files. (3) `*` crosses `/`, so `src/*` covers the whole subtree. (4) The `edit` rule for the outside dir in the snippet is my derivation from code, not a documented example — verify empirically.

### 4. Denial signal

- **Config `deny`** (code): `Permission.ask` fails with `PermissionDeniedError`, message: `The user has specified a rule which prevents you from using this specific tool call. Here are some of the relevant rules [<json of matching rules>]` (`core/v1/permission.ts`). The tool part becomes `state.status = "error"`, `state.error = <that message>`, `state.input = <args>`. In `--format json` this arrives as a `{"type":"tool_use", "part": {"tool":"write", "state":{"status":"error","error":"The user has specified a rule…","input":{…}}}}` line; on SSE as `message.part.updated` with the same `ToolPart`. No `permission.asked` event fires for `deny` (evaluated before publish).
- **`ask` auto-rejected by `run`**: `PermissionRejectedError` → `state.error = "The user rejected permission to use this specific tool call."`; a `permission.asked` (`{id, sessionID, permission, patterns, metadata, always, tool:{messageID,callID}}`, `schema/v1/permission.ts`) and `permission.replied` (`{sessionID, requestID, reply}`) event go to the SSE bus, but `run --format json` does **not** emit them to stdout (only the stderr prose line).
- **Plugin throw**: `state.error = <thrown message>`; nothing else distinguishes it.
- So: there is a structured field (`part.state.status === "error"` + `part.tool`, `part.state.input`) but **the reason is prose only**; the harness must string-match the fixed messages above (or use a plugin that throws a sentinel like `YUNTA_DENIED:<path>`) to tell a policy denial from an ordinary tool failure. A denial does not set a non-zero exit code.

### 5. Injection per session

Docs: `config.mdx` §Precedence → https://opencode.ai/docs/config/ (also `customize-opencode.md`, `flag/flag.ts`, `config/config.ts`):

- Merge order (later overrides conflicting keys): remote `.well-known/opencode` → global `~/.config/opencode/opencode.json` → **`OPENCODE_CONFIG=<file>`** → project `opencode.json`/`.jsonc` (walks up to worktree) → `.opencode/` dirs and **`OPENCODE_CONFIG_DIR`** (agents/commands/plugins/`opencode.json` inside it) → **`OPENCODE_CONFIG_CONTENT='<json>'`** → managed `/etc/opencode/` (Linux) → macOS MDM. **`OPENCODE_PERMISSION='<json>'`** merges over `permission` after all of these (code). `OPENCODE_DISABLE_PROJECT_CONFIG=1` skips project config; `OPENCODE_PURE=1` skips external plugins; `OPENCODE_DISABLE_DEFAULT_PLUGINS=1` (customize-opencode.md).
- Because merging is deep and last-wins, `OPENCODE_CONFIG_CONTENT` or `OPENCODE_PERMISSION` reliably override the user's global/project `permission`, `agent`, `mcp`, `plugin`, `tools` — everything in `opencode.json` — without touching their files. Plugins can be injected by `"plugin": ["file:///abs/path/hook.ts"]` in that inline JSON, or by pointing `OPENCODE_CONFIG_DIR` at a harness-owned dir with `plugins/`. Session-level: `POST /session` `permission` ruleset (`[{permission, pattern, action}]`) via `serve`; `run` sets it internally but exposes no flag for it. Config is read once at startup (customize-opencode.md).
- Note (code, config.ts L264): when any of `OPENCODE_CONFIG`/`_DIR`/`_CONTENT` is set, OpenCode does not seed a global config file.

### 6. Additional directories

No `--add-dir`. `--dir` only sets the working directory (`chdir`, or remote path with `--attach`). "Inside" is defined by `containsPath` (`project/instance-context.ts`): under `instance.directory` (cwd) **or** the git worktree root (for non-git dirs worktree is `/` and only cwd counts). Anything else triggers an `external_directory` ask with pattern `<parent-dir>/*` (`tool/external-directory.ts`; bash: `tool/shell.ts` for the `FILES` command list). Allow via `"external_directory": {"/abs/dir/*": "allow"}` (`~/` expands); the docs note allowed dirs "inherit the same defaults as the current workspace", so pair with `edit`/`read` rules. `references` config also auto-allows dirs through the boundary (customize-opencode.md).

### 7. MCP

Yes: `mcp.<name>` with `type: "local"` (`command: [...]`, `environment`, `cwd`, `enabled`, `timeout`) or `"remote"` (`url`, `headers`, `oauth`) — `mcp-servers.mdx` → https://opencode.ai/docs/mcp-servers/; injectable via `OPENCODE_CONFIG_CONTENT`. Tools are named `<server>_<tool>`; disable with `tools: {"<server>_*": false}` (legacy) or `permission: {"<server>_*": "deny", "<server>_search": "allow"}` (agents.mdx note); MCP tool permission checks use pattern `"*"` only (`session/tools.ts`), so per-tool allow/deny but no argument-level granularity for MCP tools. `tool.execute.before` fires for MCP tools too.

### 8. Summary

| Mechanism | Level | Path granularity | Denial visible in stream | Settable per session |
|---|---|---|---|---|
| `permission.edit` object rules | tool-call gate (`write`/`edit`/`apply_patch`) | glob on worktree-relative path (`*` crosses `/`) | Yes: `tool_use`/`message.part.updated` with `state.status:"error"`, `state.error` = fixed "user has specified a rule…" prose | `OPENCODE_CONFIG_CONTENT`, `OPENCODE_PERMISSION`, `OPENCODE_CONFIG`, per-agent + `--agent`, `POST /session {permission}` |
| `permission.external_directory` | tool-call gate (any path-taking tool incl. some bash cmds) | absolute dir roots `/dir/*`, `~/` expansion | Same as above (or "user rejected" if `ask` auto-rejected) | same env/config; also session ruleset |
| `permission.bash` / whole-tool `deny` | tool-call gate | command-text patterns; no file paths | Same | same |
| `tool.execute.before` plugin throw | tool-call gate (all tools incl. MCP) | arbitrary (plugin sees `args.filePath`/`command`) | Yes, `state.error` = thrown message (harness can embed a sentinel) | `plugin: ["file:///…"]` in inline config, or `OPENCODE_CONFIG_DIR` |
| `permission.ask` plugin hook | declared type only; not triggered in this checkout | — | — | unverified |
| Sandbox | none | none | — | — |
| Agents (`build`/`plan`/custom) | bundles of the rules above | as `permission` | as above | `--agent`, agent defined in inline config |

Key risk for the orchestrator: enforcement is entirely in-process and prose-signalled; `bash` and MCP writes bypass `edit` path rules, and the `permission.ask` hook and the docs' absolute-path `edit` example are not backed by code in this snapshot.

---

## Aider (0.86.2, 2026-02-12)

**Version verified against:** PyPI latest `aider-chat 0.86.2` (uploaded 2026-02-12, https://pypi.org/pypi/aider-chat/json); `main` branch is `0.86.3.dev` (https://raw.githubusercontent.com/Aider-AI/aider/main/aider/__init__.py). Code quotes below are from `main` unless noted; where `main` differs from the `v0.86.2` tag I say so. aider.chat is blocked by this session's egress proxy, so doc citations use the doc sources in the repo (`aider/website/docs/...`), which are what aider.chat renders, plus Context7 (`/websites/aider_chat`, `/aider-ai/aider`).

### 1. Headless mode

**Flags** (`aider/website/docs/scripting.md`, `aider/args.py`, `aider/website/docs/config/options.md`):

| Flag | Env var | Behavior |
|---|---|---|
| `--message/--msg/-m COMMAND` | `AIDER_MESSAGE` | "Specify a single message to send the LLM, process reply then exit (disables chat mode)" |
| `--message-file/-f FILE` | `AIDER_MESSAGE_FILE` | same, message read from file |
| `--yes-always` | `AIDER_YES_ALWAYS` | "Always say yes to every confirmation" (renamed from `--yes` in v0.49.0, HISTORY.md) |
| `--auto-commits/--no-auto-commits` | `AIDER_AUTO_COMMITS` | default True |
| `--dry-run/--no-dry-run` | `AIDER_DRY_RUN` | "Perform a dry run without modifying files"; `io.write_text` returns early when `dry_run` (`aider/io.py`), output says `Did not apply edit to {path} (--dry-run)` (`aider/coders/base_coder.py` `apply_updates`) |
| `--no-pretty`, `--no-stream`, `--no-fancy-input` | `AIDER_PRETTY`, `AIDER_STREAM`, `AIDER_FANCY_INPUT` | plain text output |
| `--llm-history-file FILE` | `AIDER_LLM_HISTORY_FILE` | "Log the conversation with the LLM to this file" (raw prompts/responses) |
| `--chat-history-file FILE` | `AIDER_CHAT_HISTORY_FILE` | markdown transcript; every `confirm_ask` appends `> {question} {answer}` to it (`io.py` `confirm_ask` → `append_chat_history`) |

**Machine-readable output: none.** There is no JSON/structured output mode; output is text via `io.tool_output/tool_error/tool_warning`. The only durable artifacts are the chat-history markdown, the LLM history log, and git commits.

**Exit code:** `--message` path is `coder.run(with_message=args.message)` then `return` (None) → `sys.exit(status)` → 0 (`aider/main.py` L1126-1134, L1273-1274). `return 1` only for setup failures: message file not found / IO error (L1141-1148), repo sanity check failure, invalid `--lint-cmd`, model/key problems, bad config. Edit-application failures (`apply_updates` catches `ValueError` → prints "The LLM did not conform to the edit format." and sets `reflected_message`) and LLM-send exceptions (`send_message` `except Exception` → `tool_error` + `event("message_send_exception")` + `return`, base_coder.py ~L1506) do **not** change the exit code. So: exit code 0 ≠ success.

**Python API** (`docs/scripting.md`): `Coder.create(main_model=Model(...), fnames=[...], io=InputOutput(yes=True))`, then `coder.run("instruction")`. Doc caveat verbatim: "The python scripting API is not officially supported or documented, and could change in future releases without providing backwards compatibility." `Coder.create`/`__init__` accept `read_only_fnames`, `dry_run`, `auto_commits`, `dirty_commits`, `suggest_shell_commands`, `auto_lint`, `auto_test`, `lint_cmds`, `test_cmd`, `map_tokens`, `aider_commit_hashes`, etc. (base_coder.py `__init__` ~L300-420). State readable after `run()` (base_coder.py): `coder.aider_edited_files` (set of rel paths, accumulated via `.update(edited)` L1588), `coder.last_aider_commit_hash`, `coder.aider_commit_hashes`, `coder.last_aider_commit_message`, `coder.lint_outcome`, `coder.test_outcome` (L1602/L1618), `coder.abs_fnames` (grows when files are auto-added), `coder.num_malformed_responses`, `coder.reflected_message`. `main(argv, return_coder=True)` also returns the Coder and forces `yes_always=True` if unset (main.py L546-547).

### 2. Write-control mechanisms

Aider has **no tool-call gate and no filesystem sandbox**; the LLM emits SEARCH/REPLACE blocks in text, and `Coder.prepare_to_edit` → `allowed_to_edit(path)` decides per path (base_coder.py ~L2196-2240). The full decision logic:

1. `full_path in self.abs_fnames` → allowed.
2. `repo.git_ignored_file(path)` → `tool_warning("Skipping edits to {path} that matches gitignore spec.")`, refused.
3. File does not exist → `confirm_ask("Create new file?", subject=path)`; if yes: `touch`, `git add` (when `auto_commits`), added to `abs_fnames`, allowed. If no: `tool_output("Skipping edits to {path}")`.
4. File exists but not in chat → `confirm_ask("Allow edits to file that has not been added to the chat?", subject=path)`; yes → added to `abs_fnames` and allowed; no → `"Skipping edits to {path}"`.

**(a) New files / `--yes-always`:** With `--yes-always`, `InputOutput.confirm_ask` returns `"y"` for every question except those with `explicit_yes_required=True` (`io.py`: `if self.yes is True: res = "n" if explicit_yes_required else "y"`). Neither "Create new file?" nor "Allow edits to file that has not been added to the chat?" sets `explicit_yes_required`, so **under `--yes-always` the LLM can create any file and edit any existing non-gitignored file at any path it names**, chat list notwithstanding. Additionally, `check_for_file_mentions` scans each reply for repo-file mentions and `confirm_ask("Add file to the chat?", ..., allow_never=True)` → auto-yes → file added and a reflection round is triggered (base_coder.py `check_for_file_mentions`, L1566). The system prompt tells the model "You can create new files without asking!" and "Only create *SEARCH/REPLACE* blocks for files that the user has added to the chat!" (`aider/coders/editblock_prompts.py` L17, L146) — prompt-level only. A further hazard: if a SEARCH block fails to match in its named file, `EditBlockCoder.apply_edits` retries it against **every other file in the chat** and applies it to whichever matches (`editblock_coder.py` L55-65, referencing issue #2258).

**(b) `--read FILE` / `/read-only`:** "specify a read-only file (can be used multiple times)" (`args.py`; env `AIDER_READ`). Read-only files are sent as context and excluded from mention-based auto-add (`get_addable_relative_files` subtracts them). They may live outside the repo (HISTORY v0.59.0: "Add read-only files to the chat context with `/read` and `--read`, including from outside the git repo"). **But `allowed_to_edit` never consults `abs_read_only_fnames`** (grep: the only references are context-building/listing; the edit path at L2196-2240 checks only `abs_fnames` and `git_ignored_file`). So if the LLM emits an edit block for an existing read-only file, it hits branch 4 and `--yes-always` allows it. Derived from code reading; not exercised at runtime.

**(c) Hook/plugin before an edit:** none in `main`. PRs #4485 "Add 'pre' and 'post' handlers" and #4488 were closed unmerged (Nov 16, 2025; https://github.com/Aider-AI/aider/pulls?q=is%3Apr+MCP listing). The only interception points are subclassing `Coder`/`EditBlockCoder` (override `allowed_to_edit`/`apply_edits`) via the unsupported Python API, or supplying a custom `io` object whose `confirm_ask` answers per `(question, subject)`.

**(d) Sandbox:** none. `io.write_text` opens the path directly; paths are `Path(self.root) / path` (`abs_root_path`), so `../` escapes are not checked in `allowed_to_edit`.

**(e) Shell commands:** The LLM can propose shell blocks (` ```bash ` etc. in `find_original_update_blocks`, editblock_coder.py). `run_shell_commands` → `handle_shell_commands` asks `confirm_ask("Run shell command?", explicit_yes_required=True, ...)`; because `explicit_yes_required=True`, **`--yes-always` answers "n"** — suggested commands are not executed headlessly. `--no-suggest-shell-commands` (`AIDER_SUGGEST_SHELL_COMMANDS`, default True) both drops the prompt and skips execution (`run_shell_commands` returns "" when unset; HISTORY v0.50.0: "controls both prompting for and offering to execute shell commands"). `/run`, `/test` are user commands (`commands.py` `cmd_run`, `cmd_test`), not LLM-invocable. Automatic execution that *does* happen: `--auto-lint` (default **True**, `AIDER_AUTO_LINT`) runs the built-in or `--lint-cmd` linter on edited files, then auto-commits ("Ran the linter"), and asks "Attempt to fix lint errors?" → auto-yes under `--yes-always` → reflection loop (max 3, `max_reflections`); `--auto-test` (default False) runs `--test-cmd` and asks "Attempt to fix test errors?" likewise (base_coder.py L1599-1623; `docs/usage/lint-test.md`). Disable with `--no-auto-lint`, `--no-auto-test`, `--no-suggest-shell-commands`.

### 3. Granularity

No option means "may write only files matching these globs/dirs". Closest pieces:

- **Explicit file list**: positional files / `--file` (`AIDER_FILE`, repeatable, YAML list `file:`) pre-populate `abs_fnames`; nonexistent ones are created ("Creating empty file {fname}", base_coder.py L459-461). This is a *starting* whitelist, not a ceiling (see §2a).
- **`--subtree-only`** (`AIDER_SUBTREE_ONLY`): "Only consider files in the current subtree of the git repository". Implemented in `GitRepo.ignored_file_raw` (repo.py): files outside cwd are treated as ignored for `get_tracked_files()` → affects repo map, `/add`, mention auto-add, `Coder.__init__` fnames filtering. It is **not** consulted by `allowed_to_edit`.
- **`.aiderignore` / `--aiderignore FILE`** (`AIDER_AIDERIGNORE`; gitignore syntax; FAQ shows an allow-list pattern `/*` + `!foo/` + `!foo/**`, `docs/faq.md` L60-84): same `ignored_file` path as subtree-only — filters repo map, tracked files, `/add` ("Skipping {fname} that matches aiderignore spec."), and initial `fnames`. **It does not stop LLM-emitted edit blocks**: `allowed_to_edit` only calls `git_ignored_file` (real `.gitignore`), never `ignored_file`. So `.gitignore` is the only ignore file that blocks writes.
- **Files outside the git root**: see §6.

Net: with `--yes-always` there is no enforceable path constraint; the harness must enforce it itself (e.g., inspect `git status`/`git diff --name-only` after the run, or run aider in an OS-level sandbox/container with a read-only mount for everything except the allowed roots).

### 4. Denial signal

All signals are plain text on stdout (and in the chat-history markdown):

- Edit to file not in chat, declined: `Skipping edits to {path}` (`tool_output`, not an error). Under `--yes-always` this never fires for existing/new files.
- Gitignored target: `Skipping edits to {path} that matches gitignore spec.` (`tool_warning`).
- Cannot create: `Unable to create {path}, skipping edits.` (`tool_error`).
- `/add` outside repo: `Can not add {abs} , which is not within {root}` (`commands.py` `cmd_add`).
- Failed SEARCH/REPLACE: `tool_error("The LLM did not conform to the edit format.")` + link + the `ValueError` text: `# {n} SEARCH/REPLACE block(s) failed to match!` / `## SearchReplaceNoExactMatch: This SEARCH block failed to exactly match lines in {path}` (`editblock_coder.py` L84-124; `docs/troubleshooting/edit-errors.md`: "Failed to apply edit to *filename*"). The error is fed back to the LLM as a reflection (up to 3).
- Applied: `Applied edit to {path}` per file; `Commit {hash} {message}` (repo.py `commit`).
- Structured signal: none; exit code stays 0.

**Auto-commits & attribution** (`docs/git.md`, `args.py`, `repo.py` `commit`): default one commit per edit round, with `-- <edited files>` pathspec and `--no-verify` unless `--git-commit-verify`. Before editing dirty files aider first commits them (`--dirty-commits`, default True) — with `--no-dirty-commits` and `--no-auto-commits` the working tree keeps aider's edits uncommitted, which is what a `git diff`-inspecting harness wants. Attribution defaults (v0.85.0+): `--attribute-co-authored-by` True → trailer `Co-authored-by: aider (<model>) <aider@aider.chat>`, author/committer names unmodified; `--attribute-author`/`--attribute-committer` (default None→True, only applied when co-authored-by is off or they are explicitly set) append ` (aider)` to `GIT_AUTHOR_NAME`/`GIT_COMMITTER_NAME`; `--attribute-commit-message-author`/`--attribute-commit-message-committer` (default False) prefix `aider: `. `coder.aider_commit_hashes` lists the commits aider made.

### 5. Injection per session

Precedence, lowest→highest: `.aider.conf.yml` (search order home → git root → cwd, "Files loaded last will take priority"; `--config FILE` loads only that file — `docs/config/aider_conf.md`; built in `main.py` L464-477), then `.env` files (same order; `--env-file`, `AIDER_ENV_FILE`; loaded with `override=True`, `main.py` `load_dotenv_files`), env vars `AIDER_<OPTION>` (configargparse `auto_env_var_prefix="AIDER_"`, `args.py` L41), then CLI flags. Every option in the table above has an env var and a YAML key (dashes→underscores are shown as YAML `yes-always: true`, `file:` lists, `read:` lists). `--config` itself has no env var (`is_config_file=True`). A clean per-session setup is: `--config <session.yml>` (bypasses home/repo/cwd files), `--env-file /dev/null` or a session `.env`, and `HOME` pointed at a scratch dir so `~/.aider.conf.yml`, `~/.aider/oauth-keys.env` and the `.aider.*` history files land there (`--chat-history-file`, `--input-history-file`, `--llm-history-file` are settable too). API keys go via env (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, or `--api-key provider=…`, `docs/config/dotenv.md`).

### 6. Additional directories / outside the git root

- The repo root comes from the positional file(s)/`--git-dname`/cwd via `git.Repo(..., search_parent_directories=True)`; mixed repos → `Files are in different git repos.` + `FileNotFoundError` (repo.py `GitRepo.__init__`). `guessed_wrong_repo` re-runs main if the config-time guess differs.
- `/add` of a path outside root: v0.86.2 refuses unconditionally (`Can not add …, which is not within …`); on `main` the refusal is skipped when `--no-auto-commits` (commands.py L849-857; HISTORY "main branch": "When auto-commits are disabled, aider can add files outside the git repo and promote read-only files to editable"). Unreleased as of 0.86.2.
- LLM-emitted edit paths are joined to root and not range-checked (`abs_root_path`), so `../outside/file` resolves outside the repo; `allowed_to_edit` would then ask "Create new file?" / "Allow edits…" → auto-yes. Whether `git add` of such a path fails is caught (`ANY_GIT_ERROR` → `tool_error`). Derived from code; not run.
- `--read` accepts absolute paths anywhere (test `test_read_option_with_external_file`, Context7 `/aider-ai/aider`).
- With `--no-git`, `root = utils.find_common_root(abs_fnames)` (base_coder.py L476) — no repo boundary at all.
- One extra directory outside the repo as an *editable* target: no supported mechanism; `--no-auto-commits` + pre-adding via `--file /abs/path` is the closest (works on `main`, unverified on 0.86.2 since `Coder.__init__` itself does no root check but `/add` does).

### 7. MCP

**No native MCP support in any release or in `main`** (GitHub code search for `mcp` in `Aider-AI/aider` path `aider/`: 0 results; HISTORY.md has no MCP entry; options.md has no `--mcp*`). Feature issues #2525 (opened 2024-12-03) and #3314 (2025-02-20) are still open. PRs: #3672 "Add MCP support" closed unmerged 2025-07-04; #3937 "Add MCP Support with LiteLLM" closed unmerged 2025-11-25 with no maintainer comment; #5539 "feat: add Model Context Protocol (MCP) support (opencode-compatible .mcp.json)" open since 2026-08-08; #5694 "feat: Native MCP client support" closed unmerged 2026-09-09 (https://github.com/Aider-AI/aider/pulls?q=is%3Apr+MCP and the individual PR pages). Third-party: AiderDesk (hotovo/aider-desk, Electron wrapper) has MCP servers in its own Agent Mode; `disler/aider-mcp-server` and `sengokudaikon/aider-mcp-server` expose *aider as* an MCP tool to other clients — neither adds MCP to the `aider` CLI. Note also PR #3937's thread reports the last official release was August 2025 and maintainer silence; PyPI shows 0.86.2 in Feb 2026, so the project is alive but slow.

### 8. Summary table

| Mechanism | Level | Path granularity | Denial visible in stream | Settable per session |
|---|---|---|---|---|
| Chat file list (positional, `--file`, `/add`) | file-list whitelist, **advisory only** under `--yes-always` (auto-approves "Create new file?" / "Allow edits to file not in chat?") | explicit file list | yes, text: `Skipping edits to {path}` — only when a confirm is declined (never under `--yes-always`) | flags / `AIDER_FILE` / YAML `file:` |
| `--read` / `/read-only` | none for writes (not checked in `allowed_to_edit`); context-only | explicit files/dirs, may be outside repo | no | `--read` / `AIDER_READ` / YAML `read:` |
| `.aiderignore` / `--aiderignore` | filters repo map, `/add`, mention auto-add; **does not gate LLM edit blocks** | gitignore globs (incl. allow-list pattern) | yes for `/add`: `Skipping … matches aiderignore spec.`; no for edits | `--aiderignore FILE` / `AIDER_AIDERIGNORE` |
| `.gitignore` | gate on edit path (`git_ignored_file`) | gitignore globs | yes: `Skipping edits to {path} that matches gitignore spec.` | repo file only (`--add-gitignore-files` relaxes `/add`, not edits) |
| `--subtree-only` | filters tracked-file view (map, `/add`, auto-add); not an edit gate | dir root = cwd | no | flag / `AIDER_SUBTREE_ONLY` |
| Git repo root | boundary for `/add` (refused outside root unless `--no-auto-commits` on `main`); LLM edit paths not range-checked | dir root | yes for `/add`: `Can not add …, which is not within …` | choice of cwd / positional dir |
| `--dry-run` | global write suppression | none | `Did not apply edit to {path} (--dry-run)` | flag / `AIDER_DRY_RUN` |
| Custom `io.confirm_ask` / `Coder` subclass (Python API) | tool-call-like gate per `(question, subject)` | anything you implement | whatever you print | in-process only; API "not officially supported" |
| Shell execution | `--yes-always` answers **no** to LLM-suggested commands (`explicit_yes_required`); `--no-suggest-shell-commands` removes them; `--auto-lint` (default on) and `--auto-test` run configured commands | n/a | `Running {cmd}` when run | flags / `AIDER_SUGGEST_SHELL_COMMANDS`, `AIDER_AUTO_LINT`, `AIDER_AUTO_TEST` |
| Filesystem sandbox | none | none | — | — |
| MCP | none (native); open/closed PRs only | — | — | — |

**Bottom line for a harness that must constrain writes:** aider offers no enforceable write boundary once `--yes-always` is on; the file list is a suggestion the model can exceed silently (and the SEARCH-block fallback can retarget edits to another chat file). The reliable pattern is external: run with `--no-auto-commits --no-dirty-commits --no-auto-lint --no-suggest-shell-commands --yes-always --no-pretty --no-stream --config <session.yml>` inside an OS-level sandbox, then derive the touched set from `git status --porcelain` / `git diff --name-only` (plus a watch on any extra allowed directory) and reject the run if anything falls outside the allowed roots; exit code and stdout are not a success signal.

Sources: [scripting.md](https://raw.githubusercontent.com/Aider-AI/aider/main/aider/website/docs/scripting.md), [aider_conf.md](https://raw.githubusercontent.com/Aider-AI/aider/main/aider/website/docs/config/aider_conf.md), [dotenv.md](https://raw.githubusercontent.com/Aider-AI/aider/main/aider/website/docs/config/dotenv.md), [git.md](https://raw.githubusercontent.com/Aider-AI/aider/main/aider/website/docs/git.md), [lint-test.md](https://raw.githubusercontent.com/Aider-AI/aider/main/aider/website/docs/usage/lint-test.md), [faq.md](https://raw.githubusercontent.com/Aider-AI/aider/main/aider/website/docs/faq.md), [tips.md](https://raw.githubusercontent.com/Aider-AI/aider/main/aider/website/docs/usage/tips.md), [edit-errors.md](https://raw.githubusercontent.com/Aider-AI/aider/main/aider/website/docs/troubleshooting/edit-errors.md), [options.md](https://raw.githubusercontent.com/Aider-AI/aider/main/aider/website/docs/config/options.md), [HISTORY.md](https://raw.githubusercontent.com/Aider-AI/aider/main/aider/website/HISTORY.md), [base_coder.py](https://raw.githubusercontent.com/Aider-AI/aider/main/aider/coders/base_coder.py), [editblock_coder.py](https://raw.githubusercontent.com/Aider-AI/aider/main/aider/coders/editblock_coder.py), [editblock_prompts.py](https://raw.githubusercontent.com/Aider-AI/aider/main/aider/coders/editblock_prompts.py), [io.py](https://raw.githubusercontent.com/Aider-AI/aider/main/aider/io.py), [main.py](https://raw.githubusercontent.com/Aider-AI/aider/main/aider/main.py), [repo.py](https://raw.githubusercontent.com/Aider-AI/aider/main/aider/repo.py), [args.py](https://raw.githubusercontent.com/Aider-AI/aider/main/aider/args.py), [commands.py](https://raw.githubusercontent.com/Aider-AI/aider/main/aider/commands.py), [commands.py@v0.86.2](https://raw.githubusercontent.com/Aider-AI/aider/v0.86.2/aider/commands.py), [PyPI aider-chat](https://pypi.org/pypi/aider-chat/json), [MCP PR list](https://github.com/Aider-AI/aider/pulls?q=is%3Apr+MCP), [PR #3672](https://github.com/Aider-AI/aider/pull/3672), [PR #3937](https://github.com/Aider-AI/aider/pull/3937), [PR #5539](https://github.com/Aider-AI/aider/pull/5539), [PR #5694](https://github.com/Aider-AI/aider/pull/5694), [issue #2525](https://github.com/Aider-AI/aider/issues/2525), [issue #3314](https://github.com/Aider-AI/aider/issues/3314), [disler/aider-mcp-server](https://github.com/disler/aider-mcp-server), [sengokudaikon/aider-mcp-server](https://github.com/sengokudaikon/aider-mcp-server), Context7 `/websites/aider_chat`, `/aider-ai/aider`, `/hotovo/aider-desk`.

---

## Goose (block/goose, 2026-09)

One caveat up front: the egress proxy blocks `block.github.io`, `goose-docs.ai`, `ampcode.com` and `ampcode.app`, so Goose facts come from the docs *source* in the `aaif-goose/goose` repo (the same markdown that `block.github.io/goose` renders) plus the Rust source, and Amp facts come from Context7's index of `ampcode.com/docs/*` plus two GitHub mirrors of the CLI manual. Every URL below is the page the fact was read from.

### 1. Headless mode

- **Command**: `goose run` with `-t, --text <TEXT>` ("Input text to provide to goose directly") or `-i, --instructions <FILE>` ("Path to instruction file... Use `-` for stdin"). Other relevant flags: `--no-session` ("Run goose commands without creating or storing a session file"), `-q, --quiet` (print only the model response), `--max-turns <N>` (default 1000), `--max-tool-repetitions <N>`, `--system <TEXT>`, `--provider`, `--model`, `--with-extension <COMMAND>`, `--with-streamable-http-extension <URL>`, `--with-builtin <name>`, `--container <id>`, `--debug`. Source: https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/docs/guides/goose-cli-commands.md
- **Output format**: `--output-format <FORMAT>` = `text` (default), `json` ("Complete JSON output after execution finishes"), `stream-json` ("Real-time structured output as events occur"). Source: same CLI reference and https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/docs/guides/running-tasks.md. The docs do **not** publish the event schema; it is only in source.
- **stream-json event shapes** (from `crates/goose-cli/src/session/mod.rs`, `#[serde(tag = "type", rename_all = "snake_case")] enum StreamEvent`): `message { message }`, `notification { extension_id, log{message} | progress{progress,total,message} }`, `error { error }`, `complete { total_tokens, input_tokens, output_tokens, cache_read_input_tokens, cache_write_input_tokens, cost_usd }`. `json` mode prints one object `{ messages: Vec<Message>, metadata: { ...tokens, cost, status: "completed"|"error" } }`. Source: https://raw.githubusercontent.com/aaif-goose/goose/main/crates/goose-cli/src/session/mod.rs
- **Per-tool-call events with name and arguments: yes**, nested inside `message` events. `Message.content` is `Vec<MessageContentBlock>` tagged `type` in camelCase: `text`, `toolRequest { id, toolCall: {name, arguments} }`, `toolResponse { id, toolResult: CallToolResult (is_error) }`, `toolConfirmationRequest { id, toolName, arguments, prompt }`, `error`, etc. Source: https://raw.githubusercontent.com/aaif-goose/goose/main/crates/goose-provider-types/src/conversation/message.rs
- **Exit code**: `main() -> anyhow::Result<()>`, no explicit `process::exit` on the run path, so 0 on success, 1 on any error. Source: https://raw.githubusercontent.com/aaif-goose/goose/main/crates/goose-cli/src/main.rs; the headless tutorial relies on `if ! goose run ...; then` (https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/docs/tutorials/headless-goose.md).

### 2. Write-control mechanisms

**(a) Hooks — yes, a real pre-tool gate.** Hooks live in *plugins*: `~/.agents/plugins/<name>/` (user), `<project>/.agents/plugins/<name>/` (project), or the install dir; each needs `plugin.json` (or `.plugin/plugin.json` / `.goose-plugin/plugin.json`) and `hooks/hooks.json`. Events: `SessionStart`, `SessionEnd`, `Stop`, `UserPromptSubmit`, `PreToolUse`, `PreToolUseResult`, `PostToolUse`, `PostToolUseFailure`, `BeforeReadFile`, `AfterFileEdit`, `BeforeShellExecution`, `AfterShellExecution`. Only `PreToolUse` and `Stop` can block. Rule fields: `matcher` (unanchored **regex**, matched against tool name for tool events), `hooks[]` with `type: "command"`, `command` (run via `sh -c`), `timeout` (default 30 s), `on_failure: "allow"|"block"` (PreToolUse only; default allow — broken hooks fail open). Payload on stdin: `event`, `session_id`, `matcher_context`, `tool_name`, `tool_input`, `tool_call_id`, `working_dir`. Block by exit code 2 (reason on stderr) or stdout `{"decision":"block","reason":"..."}`. Model then receives: `Tool call denied by policy hook \`<plugin>\`: <reason>. Do not retry; this is a policy denial, not a transient failure.` Sources: https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/docs/guides/context-engineering/hooks.md, https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/docs/guides/context-engineering/plugins.md, message string in https://raw.githubusercontent.com/aaif-goose/goose/main/crates/goose/src/hooks/mod.rs. Plugins are toggled by `disabledPlugins` in `~/.config/goose/settings.json`, `<project>/.config/goose/settings.json`, or `settings.local.json`.

**(b) Sandbox — none native.** `--container <id>` runs *extensions* inside an existing Docker container (goose itself stays on the host) and is not a path restriction. Source: https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/docs/guides/goose-cli-commands.md and goose-docs.ai/docs/tutorials/goose-in-docker via Context7. No path-scoped filesystem sandbox is documented. `.gooseignore` exists per third-party summaries (WebSearch, e.g. github.com/block/goose/issues/1065) but I could not find its doc page or enforcement code in the current repo tree — **unverified** for 2026.

**(c) Permission config — whole-tool only.** `GOOSE_MODE` ∈ `auto` (default), `approve`, `smart_approve`, `chat`, settable as env var or `config.yaml` key; "Environment variables take precedence over configuration files." Source: https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/docs/guides/environment-variables.md. Per-tool overrides (`Always Allow` / `Ask Before` / `Never Allow`) are keyed by `extension__tool` name and stored in `permission.yaml` under the config dir with shape (fenced as `text`: it is Goose's file, not a yunta document):
```text
user:
  always_allow: [developer__shell]
  ask_before: []
  never_allow: [developer__write]
smart_approve: { always_allow: [], ask_before: [], never_allow: [] }
```
Source: https://raw.githubusercontent.com/aaif-goose/goose/main/crates/goose/src/config/permission.rs (`PermissionConfig { always_allow, ask_before, never_allow }`), file list in https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/docs/guides/config-files.md. Docs only show configuring it via `goose configure` (https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/docs/guides/managing-tools/tool-permissions.md). No argument/path conditions exist.

**(d) Modes and headless.** In non-interactive runs, `approve`/`smart_approve` are refused outright: `"Tool approval required in non-interactive mode with GooseMode::{goose_mode}. This is an invalid configuration — Approve/SmartApprove modes require an interactive terminal. Use GooseMode::Auto for headless sessions."`; otherwise tool confirmations are auto-allowed (`Permission::AllowOnce`). Source: https://raw.githubusercontent.com/aaif-goose/goose/main/crates/goose-cli/src/session/mod.rs. So a harness must run `GOOSE_MODE=auto` and gate writes via hooks (or `never_allow` per tool).

### 3. Granularity
- **Path/glob granularity: only via a `PreToolUse` hook.** The hook receives `tool_name` + `tool_input`, so a script can allow writes whose path matches your globs (repo subdirs plus one outside dir) and block the rest. Example plugin (project-local, so the harness can drop it in per checkout):
```json
// <project>/.agents/plugins/yunta-guard/hooks/hooks.json
{ "hooks": { "PreToolUse": [ { "matcher": "^developer__(write|edit|text_editor|shell)$",
    "hooks": [ { "type": "command", "command": "${PLUGIN_ROOT}/guard.sh", "on_failure": "block" } ] } ] } }
```
`guard.sh` reads `jq -r '.tool_input.path'`, tests it against the allowed roots, and prints `{"decision":"block","reason":"..."}` otherwise. Caveats: matcher is regex, not glob; `developer__shell` writes can only be gated by inspecting `tool_input.command`. Current developer tool names per docs are `shell`, `write`, `edit`, `tree`, `read_image` (https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/docs/mcp/developer-mcp.md); the hooks doc and a test fixture still reference `developer__text_editor` with `command`/`path`/`file_text` args (https://raw.githubusercontent.com/aaif-goose/goose/main/crates/goose-provider-types/src/formats/ollama.rs) — exact argument names of `write`/`edit` are **unverified**.
- Built-in permissions and `GOOSE_MODE`: whole-tool only.

### 4. Denial signal
- Hook block: a `message` stream event whose content has a `toolResponse` with `toolResult.is_error` and the text `Tool call denied by policy hook ...` (test asserts `ToolResponse` carrying "denied by policy hook": https://raw.githubusercontent.com/aaif-goose/goose/main/crates/goose/src/agents/state_machine/tests/hooks_lifecycle.rs). The hook system also emits a `PreToolUseResult` payload (`decision: "deny"`, `blocked_by`, `reason`, `cause: "policy_denial"|"hook_failure"`) — but only to hooks, not to stdout.
- Permission denial (`never_allow`/deny): error tool result with constant `DECLINED_RESPONSE` = "The user has declined to run this tool. DO NOT attempt to call this tool again..." (https://raw.githubusercontent.com/aaif-goose/goose/main/crates/goose/src/agents/tool_execution.rs). So: distinguishable by `is_error` + prefix text, no dedicated event type.

### 5. Injection without touching global config
- `GOOSE_PATH_ROOT=<dir>`: "Override the root directory for all goose data, config, and state files" — an isolated config/data/state tree per invocation (docs show `GOOSE_PATH_ROOT="$(mktemp -d)" goose run ...`). Env vars override `config.yaml`. Source: https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/docs/guides/environment-variables.md
- `GOOSE_MODE`, `GOOSE_PROVIDER`, `GOOSE_MODEL`, `GOOSE_MAX_TURNS`, `GOOSE_DISABLE_SESSION_NAMING`, `GOOSE_CONTEXT_STRATEGY`, `GOOSE_CLI_MIN_PRIORITY` as env vars (headless tutorial).
- Per-project files: `<project>/.agents/plugins/` (hooks) and `<project>/.config/goose/settings.json` / `settings.local.json`. Whether `GOOSE_PATH_ROOT` also relocates `~/.agents/plugins` and `~/.config/goose/settings.json` is **unverified**.
- Flags: `--with-extension`, `--with-builtin`, `--with-streamable-http-extension`, `--provider`, `--model`, `--system`. No flag points at an alternate `config.yaml` or `permission.yaml`.

### 6. Additional directories
No `--add-dir` equivalent found in the CLI reference; only `--path` (legacy session file) and `session list -w` (filter). Goose is not documented as confining writes to cwd at all (community reports it writing above cwd: https://github.com/aaif-goose/goose/discussions/7069). Any "extra directory" policy therefore has to be enforced by the hook.

### 7. MCP
- Per-session server: `goose run --with-extension "<cmd> <args>"` (stdio, repeatable) or `--with-streamable-http-extension <URL>`; persistent config via `extensions:` in `config.yaml` (`type: stdio`, `cmd`, `args`, `envs`, `env_keys`, `timeout`). Sources: CLI reference and https://raw.githubusercontent.com/aaif-goose/goose/main/documentation/docs/guides/config-files.md
- Permissions target MCP tools by the same `extension__tool` key in `permission.yaml`, and hook matchers can regex them; `GOOSE_ALLOWLIST=<url>` restricts which extensions may load at all. No argument-level MCP rules.

### 8. Summary table — Goose

| Mechanism | Level | Path granularity | Denial visible in stream | Settable per session |
|---|---|---|---|---|
| `PreToolUse` hook (plugin `hooks/hooks.json`) | tool-call gate (fail-open unless `on_failure: block`) | any — script sees `tool_input.path` (regex matcher on tool name; globs are your script's job) | yes: `message` event with `toolResponse.is_error` and "Tool call denied by policy hook ..." text | yes: `<project>/.agents/plugins/`, toggled by `<project>/.config/goose/settings.json` |
| `GOOSE_MODE` | mode (auto/approve/smart_approve/chat) | none | approve modes error out headless | yes: env var |
| `permission.yaml` always/ask/never per tool | tool-call gate | none (whole tool) | yes: error tool result "The user has declined to run this tool" | only via `GOOSE_PATH_ROOT` pointing at a prepared config tree |
| `--container` | extensions run in Docker | none documented | n/a | yes: flag |
| Filesystem sandbox | none | — | — | — |

---

## Amp (Sourcegraph, 2026-09)

### 1. Headless mode
- `-x` / `--execute "<prompt>"` (or prompt on stdin): "sends the message... waits until the agent ends its turn, prints its final message, and exits"; execute mode also triggers automatically when stdout is redirected. `--stream-json` emits NDJSON; `--stream-json-input` reads `{"type":"user","message":{...}}` lines from stdin (requires `--stream-json`); `--no-archive-after-execute`; `-ox` runs in a remote orb with identical output. Source: ampcode.com/docs/cli/execute-mode and ampcode.com/docs/cli/streaming-json (via Context7 `/websites/ampcode`).
- `--dangerously-allow-all`: "Disable all command confirmation prompts". Note that Amp's current default is to *not* ask at all: "Amp does not ask for approval before running tools" unless permissions settings activate the legacy plugin (ampcode.com/docs/tools). The SDK forces `dangerouslyAllowAll=false` whenever `permissions` is supplied (ampcode.com/docs/sdk/python).
- **Event shapes** (`StreamJSONMessage`, ampcode.com/docs/cli/streaming-json): `{"type":"system","subtype":"init","cwd","session_id","tools":[...],"mcp_servers":[{name,status}]}`; `{"type":"assistant","message":{"content":[{"type":"text"}|{"type":"tool_use","id","name","input"}|thinking...],"stop_reason","usage"},"parent_tool_use_id","session_id"}`; `{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id","content","is_error"}]}}`; final `{"type":"result","subtype":"success","is_error":false,"num_turns","duration_ms","result","permission_denials?":string[]}` or `{"type":"result","subtype":"error_during_execution"|"error_max_turns","is_error":true,"error",...,"permission_denials?"}`. **Yes**, every tool call appears as `tool_use` with `name` and full `input`.
- Exit code: **unverified** (no doc page states it; the error `result` message is the documented failure signal).

### 2. Write-control mechanisms
**(a) Hooks.** Two generations:
- *Plugin API (current)*: a TS/JS plugin registers `amp.on('tool.call', async (event, ctx) => ...)` and returns `{action:'allow'}`, `{action:'reject-and-continue', message}`, `{action:'modify', ...}` or `{action:'synthesize', ...}`; `event.tool` is the tool name, and helpers such as `amp.helpers.shellCommandFromToolCall(event)` read the input. `tool.result` observes results. Loaded from project `.amp/plugins/`, system `~/.config/amp/plugins/` (or `$XDG_CONFIG_HOME/amp/plugins/`), personal and workspace settings; precedence project > system > personal > workspace. Source: ampcode.com/docs/customize/plugins (Context7). Exact `event.input` field name: **unverified**.
- *Legacy `amp.hooks` (settings array)*: entries with `compatibilityDate`, `id`, `on: {event: "tool:pre-execute"|"tool:post-execute", tool: [...], input matchers}`, `action: "send-user-message" | "redact-tool-input"`; `send-user-message` on `tool:pre-execute` interrupts the agent and cancels the call. Source: search-engine excerpt of https://ampcode.com/manual (page blocked here) — treat details as **partly unverified**.

**(b) Sandbox.** None documented for the local CLI; isolation is offered by running in an *orb* (remote environment, `-ox`) — not a path restriction. No path sandbox found.

**(c) Permission rules — `amp.permissions`** (settings array; activates the internal "legacy permissions plugin"; applies in both `amp` and `amp -x`). Rule keys: `tool` (glob: `Bash`, `mcp__playwright__*`, `**/my-tool`), `matches` (map argument → condition), `action` ∈ `allow|reject|ask|delegate`, `context` ∈ `thread|subagent`, `to` (delegate program), `message` (reject only: "Message returned to the model. If set, the rejection continues the conversation instead of halting it"). Match conditions: string = glob (`*` = any characters) or regex `/.../`; array = OR; scalar = literal; object = nested. Evaluation: first matching rule wins; if none, bundled defaults; add a catch-all `reject '*'`/`ask '*'` for default-deny. Delegate: program gets tool params as JSON on stdin, env `AMP_THREAD_ID`, `AGENT_TOOL_NAME`, `AGENT=amp`; exit 0 allow, 1 ask, ≥2 reject (stderr surfaced to the model). CLI: `amp permissions list [--builtin]`, `amp permissions add ...`, `amp permissions test edit_file --path "$PWD/README.md"` (prints action + matched rule). Sources: ampcode.com/docs/legacy-permissions-rules.txt, ampcode.com/docs/sdk/typescript, ampcode.com/docs/sdk/python (Context7). Also `amp.guardedFiles.allowlist` ("Array of file glob patterns that are allowed to be accessed without confirmation. Takes precedence over the built-in denylist") and `amp.dangerouslyAllowAll` (https://github.com/jdorfman/awesome-amp-code/blob/main/docs/amp_cli_docs.md, dated 2025-10-16, and ampcode.com/docs/tools).

**(d) Modes.** `--mode low|medium|high|ultra` / `--effort` are reasoning modes, not permission modes. Amp's only permission "mode" is `amp.dangerouslyAllowAll` vs. rules present.

### 3. Granularity
**Yes — path globs per file tool via `amp.permissions`.** `path` is a matchable argument for `edit_file`, `create_file`, `Read`, `Grep` (doc examples: `{"tool":"edit_file","matches":{"path":".*"},"action":"reject"}`, `{"tool":"Grep","matches":{"path":"$HOME/*"},"action":"ask"}`; `amp permissions test edit_file --path ...` shows the absolute path being matched; `$HOME`/`$PWD` expand). "Only under these directories plus one outside dir":
```jsonc
{ "amp.permissions": [
  { "tool": "create_file", "matches": { "path": ["$PWD/src/*", "$PWD/docs/*", "/srv/yunta-scratch/*"] }, "action": "allow" },
  { "tool": "edit_file",   "matches": { "path": ["$PWD/src/*", "$PWD/docs/*", "/srv/yunta-scratch/*"] }, "action": "allow" },
  { "tool": "create_file", "action": "reject", "message": "Writes are limited to src/, docs/ and /srv/yunta-scratch." },
  { "tool": "edit_file",   "action": "reject", "message": "Writes are limited to src/, docs/ and /srv/yunta-scratch." },
  { "tool": "undo_edit",   "action": "reject" },
  { "tool": "Bash", "matches": { "cmd": ["ls *", "cargo test*", "git status"] }, "action": "allow" },
  { "tool": "Bash", "action": "reject", "message": "Shell writes are not permitted." }
] }
```
Caveats: `*` is "any characters", so `$PWD/src/*` covers nested paths but I found no statement about `**` or path normalisation (`..`) — **unverified**; Bash cannot be path-scoped, only command-scoped, so a strict harness must reject/allowlist shell commands.

### 4. Denial signal
`reject` (with `message`) → the model gets the message and the run continues; the stream shows the `tool_use` followed by a `user` `tool_result` with `is_error: true` (the plugin returns `reject-and-continue`), and the final `result` carries `permission_denials?: string[]` — a distinct field. `reject` *without* `message` halts the conversation (legacy-permissions-rules.txt wording) — exact halted-stream shape **unverified** (presumably `result` with `is_error:true`). `ask` in `-x`: no doc states the outcome when no operator is present — **unverified**; avoid `ask` and use `reject`/`delegate` headless.

### 5. Injection
- `--settings-file <path>` ("Custom user settings: pass `--settings-file <path>` to point Amp at a different user settings file", ampcode.com/docs/cli/settings) and env `AMP_SETTINGS_FILE` ("Set settings file path (can also use --settings-file...)", awesome-amp-code mirror; also github.com/sourcegraph/amp-examples-and-guides/blob/main/guides/cli/README.md). Workspace `.amp/settings.json[c]` (nearest, searched upward to repo root) overrides user settings — so a per-checkout `.amp/settings.json` also works.
- `--mcp-config '<json or file path>'` ("JSON configuration or file path for MCP servers to merge with existing settings").
- SDK `execute({options:{permissions:[...], settingsFile, mcpConfig, env, cwd, enabledTools, dangerouslyAllowAll}})` maps 1:1 to CLI flags (ampcode.com/docs/sdk/typescript); whether a bare `amp` CLI flag for inline permissions exists is **unverified** (the SDK doc says options "map to Amp CLI flags" but no `--permissions` flag is documented).
- Other env: `AMP_API_KEY`, `AMP_URL`, `AMP_LOG_LEVEL`, `AMP_LOG_FILE`.

### 6. Additional directories
No `--add-dir` equivalent found in any doc (settings, execute-mode, SDK). Amp's file tools take absolute paths and are governed by permissions, not by a root list; the built-in default rules that gate paths outside cwd are only visible via `amp permissions list --builtin` — **unverified** what they contain.

### 7. MCP
- Register per session: `--mcp-config '{"srv":{"command":"npx","args":[...]}}'` or `amp.mcpServers` in the `--settings-file`/workspace settings (`command`/`args`/`env` for stdio, `url`/`headers` for remote, `${VAR}` expansion); `amp mcp add ...` for persistent. Source: ampcode.com/docs/customize/mcp, ampcode.com/docs/cli/execute-mode.
- Restrict: tools are named `mcp__<server>__<tool>`; `amp.permissions` rules match them by glob (`mcp__playwright__*`, with `matches` on their arguments); `amp.tools.disable` / `amp.tools.enable` accept the same names (`builtin:name` to disable only a built-in); `amp.mcpPermissions` allows/rejects whole servers by `command`/`args`/`url` pattern (first match wins, default allow). Source: ampcode.com/docs/cli/settings.

### 8. Summary table — Amp

| Mechanism | Level | Path granularity | Denial visible in stream | Settable per session |
|---|---|---|---|---|
| `amp.permissions` rules (legacy plugin) | tool-call gate | glob/regex on `path` arg of `edit_file`/`create_file`/`Read`/`Grep`; `cmd` for Bash; `$PWD`/`$HOME` expand | yes: `tool_result.is_error` + `result.permission_denials[]` (with `message`); halt without `message` | yes: `--settings-file`/`AMP_SETTINGS_FILE`, or `.amp/settings.json` in the checkout; SDK `permissions` |
| Plugin `tool.call` handler | tool-call gate (allow/reject-and-continue/modify/synthesize) | any — code sees the call | yes: same `tool_result.is_error` | yes: file in `<project>/.amp/plugins/` |
| Legacy `amp.hooks` (`tool:pre-execute` → `send-user-message`) | tool-call interrupt | tool + input matchers (details unverified) | prose only (a user message) | settings file |
| `amp.guardedFiles.allowlist` | confirm-before-edit list | file globs | n/a headless | settings file |
| `amp.tools.disable` / `.enable`, `amp.mcpPermissions` | tool/server visibility | none | tool absent from `system.init.tools` | settings file / `--mcp-config` |
| `--dangerously-allow-all` | disables all gates | — | — | flag |
| Filesystem sandbox | none locally (orbs are remote VMs) | — | — | `-ox` |

**Bottom line for the orchestrator**: Amp gives declarative path-glob write rules with a distinct machine-readable denial field, injectable per run via `--settings-file`; Goose has whole-tool permissions only, and path scoping requires shipping a `PreToolUse` hook plugin into the checkout and running `GOOSE_MODE=auto` (approve modes abort headless), with denials recognisable by the fixed "denied by policy hook" text in an `is_error` tool response.

---

## Kimi Code CLI (MoonshotAI/kimi-code v0.42.0, 2026-09-09)

### Scope note (which "Kimi CLI")

There are two projects. `MoonshotAI/kimi-cli` (Python) carries this banner: *"Kimi CLI is evolving into Kimi Code CLI (github.com/MoonshotAI/kimi-code) … This project will be gradually wound down"* (kimi-cli README line 12, https://github.com/MoonshotAI/kimi-cli/blob/main/README.md). The current product is **Kimi Code CLI** (`MoonshotAI/kimi-code`, TypeScript, `kimi` command, v0.42.0 released 2026-09-09; main @ `7d7e8de`, 2026-09-13). Everything below is about `kimi-code`; the `--input-format=stream-json`/stdin-JSONL feature surfaced by web search belongs to the legacy Python `kimi-cli` and does not exist in `kimi-code` (no `--input-format` option in `apps/kimi-code/src/cli/commands.ts`). Docs site `moonshotai.github.io/kimi-code` was egress-blocked here; I cite the same pages by their source paths under `docs/en/` and the engine source under `packages/agent-core-v2/src/`, all at https://github.com/MoonshotAI/kimi-code/blob/main/.

### 1. Headless mode

- **Flag**: `-p, --prompt <prompt>` — "Run a single prompt non-interactively and stream the Assistant output to stdout. This mode does not open the TUI" (`docs/en/reference/kimi-command.md`, options table). The prompt is an argument only; there is no stdin/`--input-format` option (`apps/kimi-code/src/cli/commands.ts` option list; no `process.stdin` read in `cli/v2/run-v2-print.ts`).
- **Output format**: `--output-format <text|stream-json>`, "Can only be used with `--prompt`; defaults to `text`" (same doc). `stream-json` = one JSON object per line on stdout.
- **Event shapes** (`apps/kimi-code/src/cli/prompt-render.ts`, `PromptJsonWriter`):
  - `{"role":"assistant","content"?:string,"tool_calls"?:[{"type":"function","id","function":{"name","arguments":string}}]}` — emitted (flushed) right before the tool result; **yes, tool name + JSON-stringified arguments are present**.
  - `{"role":"tool","tool_call_id","content":string}` — tool result. Note the interface has **no `is_error` field**; the engine's `isError` flag is dropped.
  - `{"role":"meta","type":"turn.step.retrying",...}`, `{"role":"meta","type":"session.resume_hint","session_id","command","content"}`, `{"role":"meta","type":"system.version","version"}`, plus a goal-summary JSON line when goal mode is used.
  - Hook results are written as `{"role":"assistant","content":"<Event> hook[ blocked]\n\n<body>"}` (`writeHookResult`/`formatHookResultPlain`) — same role as model text, distinguishable only by the content prefix.
  - Docs: "when the model calls a tool, an Assistant message with `tool_calls` is emitted first, followed by the corresponding Tool message… Thinking content is not written to JSONL; tool progress and 'resuming session' notices are still written to stderr" (`kimi-command.md` §Non-Interactive Execution).
- **Permission in `-p`**: "In `-p` mode, no human approval is requested — regular tool calls are handled under the `auto` permission policy, while static deny rules remain in effect" (`kimi-command.md`). Source: `run-v2-print.ts` calls `setMode('auto')` on the main agent (lines 413, 482). `--prompt` **cannot** be combined with `--yolo`, `--auto`, or `--plan` (`cli/options.ts` lines 79–83; `kimi-command.md` Flag Conflict Rules). (`docs/en/configuration/overrides.md` shows `kimi --yolo -p ...` as an example — that contradicts the conflict rule and the source; the source wins.)
- **ACP**: `kimi acp` exists — "communicates with an ACP client … via JSON-RPC over stdin/stdout" (`docs/en/reference/kimi-acp.md`). Implements `initialize/authenticate/logout`, `session/new|load|resume|list|fork|close|delete|prompt|cancel|set_mode|set_config_option`, reverse-RPC `session/update`, `session/request_permission`, `fs/read_text_file`, `fs/write_text_file`, `terminal/*`, `elicitation/create`. `session/new` accepts `cwd`, `mcpServers`, `additionalDirectories`. Modes: `default` (manual), `plan`, `auto`, `yolo`; initial mode is `default` (`packages/acp-server/src/modes.ts`, `DEFAULT_MODE_ID = 'default'`).
- **Exit code**: no result line is emitted. Success → process exits naturally with `process.exitCode || 0`; any thrown failure → `process.exitCode = 1` then `process.exit(1)` (`apps/kimi-code/src/main.ts` lines 231–261, `cli/headless-exit.ts`). Goal mode maps status to `complete: 0, blocked: 3, paused: 6` (`cli/goal-prompt.ts` `GOAL_EXIT_CODES`). A tool denial does **not** change the exit code.

### 2. Write-control mechanisms

**(a) Hooks** (`docs/en/customization/hooks.md`; engine `packages/agent-core-v2/src/features/externalHooks/`)
- Config: `[[hooks]]` array in `~/.kimi-code/config.toml` (i.e. `$KIMI_CODE_HOME/config.toml`), fields exactly `event`, `matcher` (regex on the tool name for `PreToolUse`), `command`, `timeout` (1–600 s, default 30); "`[[hooks]]` only allows these four fields; extra fields will cause the config file to fail to load" (schema is `.strict()`, `externalHooks/configSection.ts`). Plugins may also declare hooks in `kimi.plugin.json` (`docs/en/customization/plugins.md` §Hooks in Plugins).
- Event: `PreToolUse` — "Triggered before a tool call (before permission checks); the tool will not execute if blocked". Blockable events are only `PreToolUse`, `Stop`, `UserPromptSubmit`.
- Receives on stdin: base `{hook_event_name, session_id, session_title, client_type, cwd}` plus `tool_name`, `tool_input` (the full args object, e.g. `path`, `content`), `tool_call_id` (`agentExternalHooksService.ts` `runPreToolUse`; keys converted with `camelToSnake`, `internal/matchHooks.ts` line 127).
- Blocks by: exit code `2` (stderr = reason) **or** exit 0 with stdout JSON `{"hookSpecificOutput":{"permissionDecision":"deny","permissionDecisionReason":"..."}}`. Only `"deny"` is honoured; any other `permissionDecision` value is treated as allow (`internal/runHook.ts` line 191). Other non-zero exit, timeout or crash = **fail-open** ("Default allow"). Docs warn: "should not be used as the sole security barrier".
- What the model sees: the block reason becomes the tool result — `event.veto(denyToolExecution(reason))` → `{ output: reason, isError: true }` (`agentExternalHooksService.ts` `registerToolHooks`; `toolExecutor/beforeToolExecuteEvent.ts`). Docs: "Kimi Code CLI writes the blocking reason back into the context, and the model can use this to choose a safer alternative."

**(b) Sandbox**: **none**. No sandbox flag/config exists; no seatbelt/bubblewrap/landlock/firejail references anywhere in `packages/` or `apps/` (grep of source). The only "sandbox" in docs is `KIMI_CODE_HOME="$PWD/.kimi-sandbox" kimi` meaning an isolated *config/data* directory (`docs/en/configuration/overrides.md`). `Bash` spawns `$SHELL -c "cd <cwd> && <command>"` with no confinement (`agent/tools/os/bash/bashTool.ts` `spawn`).

**(c) Tool rules / policy** (`docs/en/configuration/config-files.md` §`permission`; engine `agent/permissionPolicy/`)
- `[[permission.rules]]` with `decision = "allow"|"deny"|"ask"`, `pattern = "ToolName"` or `"ToolName(arg-pattern)"`, optional `scope` (`turn-override|session-runtime|project|user`, default `user`), `reason`. "Rules are matched in order; the first matching rule takes effect." Tool-name part is itself a picomatch glob or `*` (`permissionRules/matchesRule.ts`).
- Per-tool argument subject: `Write(path-pattern)`, `Edit(path-pattern)`, `Read(path-pattern)`, `ReadMediaFile(path)` use `matchesPathRuleSubject` on the **canonical absolute path** with `cwd = workspaceDir` and `~` expansion, case-insensitive by default (`tools/os/write/writeTool.ts` line 61, `tools/edit/editTool.ts` line 68, `tool/rule-match.ts`). `Bash(command-glob)`, `Grep(pattern)`, `Glob(pattern)`, `FetchURL(url)`, `WebSearch(query)`, `Agent(profile)` use plain glob on that argument. "`AgentSwarm`, MCP tools, and custom tools can only be matched by tool name; argument patterns are not supported for them." A leading `!` negates the arg pattern (`rule-match.ts` `matchRuleSubjects`).
- `[tools] enabled = [...]` / `disabled = [...]` in config.toml removes tools from the model entirely (tool-name granularity; MCP globs) (`config-files.md` §`tools`). Agent files (`--agent-file`) carry `tools:` / `disallowedTools:` allow/deny lists, also tool-name granularity (`docs/en/customization/agents.md`).
- `--yolo` (`-y`; hidden aliases `--yes`, `--auto-approve`), `--auto`, `--plan`; `default_permission_mode = manual|yolo|auto` in config.toml.
- `[permission] dangerous_command_guard = true|false` (env `KIMI_CODE_DANGEROUS_COMMAND_GUARD`); the dangerous-command policy is **not instantiated at all in `-p`** (`permissionPolicyService.ts`: skipped when `bootstrap.args.nonInteractive`).

**(d) Approval modes** (`docs/en/guides/interaction.md`; policy order in `permissionPolicyService.ts`):
- `manual` ("Always Ask", default): read-only tools auto-allowed (`DefaultToolApprove` set: Read/Grep/Glob/ReadMediaFile/WebSearch/FetchURL/Agent/…); Write/Edit/Bash ask, except `Write`/`Edit` inside the workspace of a git work tree are auto-approved (`git-cwd-write-approve.ts`).
- `yolo` ("Ask When Needed"): approves regular calls; still asks for sensitive files, `.git` control paths, dangerous commands, plan exit.
- `auto` ("Never Ask"): approves everything after the deny rules; `AskUserQuestion` is denied. This is what `-p` forces.
- Evaluation order: `AutoModeAskUserQuestionDeny → UserConfiguredDeny → DangerousCommandAsk(interactive only) → AutoModeApprove → SessionApprovalHistory → UserConfiguredAsk → UserConfiguredAllow → SensitiveFileAccessAsk → GitControlPathAccessAsk → YoloModeApprove → DefaultToolApprove → GitCwdWriteApprove → FallbackAsk`. Consequence: in `-p`/auto only **`deny` rules and PreToolUse hooks** bite; `ask`/`allow` rules are moot.
- Independent of mode, `Write`/`Edit`/`Read` refuse sensitive files (`.env*`, `id_rsa*`, `credentials`, `.aws/credentials`, …) with `PATH_SENSITIVE` at path resolution (`tool/path-access.ts` `isSensitiveFile`, `resolvePathAccess`).

### 3. Granularity

**Yes — for the `Write` and `Edit` tools only**, via `[[permission.rules]]` path globs with negation. Because the subject is the canonical absolute path, and picomatch supports braces, one deny rule expresses "only these roots":

```toml
# $KIMI_CODE_HOME/config.toml
[[permission.rules]]
decision = "deny"
pattern  = "Write(!{/work/repo/src/**,/work/repo/tests/**,/tmp/yunta-out/**})"
reason   = "writes are limited to src/, tests/ and the harness output dir"

[[permission.rules]]
decision = "deny"
pattern  = "Edit(!{/work/repo/src/**,/work/repo/tests/**,/tmp/yunta-out/**})"
reason   = "writes are limited to src/, tests/ and the harness output dir"
```

Basis: pattern grammar `ToolName(arg-pattern)` (`config-files.md` §permission); `Write`/`Edit` pass the resolved absolute path with `cwd: workspace.workspaceDir` (`writeTool.ts` 61–66, `editTool.ts` 68–73); `pathGlobMatch` canonicalises both sides and uses picomatch (`rule-match.ts`); `!` negation in `matchRuleSubjects` (`rule-match.ts` 151–160); `UserConfiguredDeny` runs before `AutoModeApprove` so it holds in `-p`. Relative patterns are resolved against the workspace dir, so `Write(!{src/**,tests/**})` also works for in-repo roots, but an out-of-repo directory must be absolute. The `tests/` for this claim exist in the repo but I did not execute them — treat the brace+negation combination as **unverified by execution**; the individual pieces (negation, canonical absolute subject, picomatch) are verified in source.

Limits: **`Bash` is path-blind** — rules on it are command globs (`Bash(rm -rf*)`), so a shell `echo > file` is not covered; the only path-aware gate for shell output is a `PreToolUse` hook on `Bash` that parses the command yourself (docs explicitly say the example "is not a production-grade security parser"). MCP tools: tool-name only. Same glob mechanism can be duplicated in a `PreToolUse` hook (`tool_input.path`), which is equivalent in granularity but fail-open.

### 4. Denial signal

Only prose in the stream. A refused write is settled as a synthetic tool result (`toolExecutorService.ts`: `decision.veto → settleSynthetic(..., 'vetoed')`, `dispatchToolResult` → `ToolResultEvent{output, isError:true}`), which `PromptJsonWriter.writeToolResult` serialises as `{"role":"tool","tool_call_id":"…","content":"<text>"}` — the `isError` flag is **not** in the JSONL. The detectable strings are:
- rule deny: `Tool "Write" was denied by permission rule.` + ` Reason: <reason>` if set (`policies/user-configured-rule.ts` `defaultPermissionRuleDenyMessage`);
- policy deny without message: `Tool "<name>" was denied by permission policy.` (`toolApprovalService.ts`);
- hook block: exactly the hook's stderr / `permissionDecisionReason` (`denyToolExecution(reason)`), with no prefix — so choose a unique sentinel in the hook's reason;
- path guard: `"<path>" matches a sensitive-file pattern … Access is blocked` / `"<path>" is not an absolute path. You must provide an absolute path to write or edit a file outside the working directory.` (`path-access.ts`);
- interactive reject (TUI/ACP only): `Tool "<name>" was not run because the user rejected the approval request.[ Reason: …]`.
No distinct event type, no `permission.denied` line; `PermissionRequest`/`PermissionResult` hook events fire only for interactive asks, and `PostToolUseFailure` hook fires for blocked tools (`agentExternalHooksService.ts` `notifyPostToolUse`) — a hook is therefore the most reliable side channel for the harness (write your own log file), not stdout.

### 5. Injection per session

- **`KIMI_CODE_HOME=<dir>`** (env): relocates config.toml, mcp.json, sessions, logs, credentials — "Isolated test environment: `KIMI_CODE_HOME="$PWD/.kimi-sandbox" kimi`" (`overrides.md`; `env-vars.md`). This is the way to give one session its own `[[permission.rules]]`, `[[hooks]]`, `[tools]`, `default_permission_mode`, and `mcp.json` without touching `~/.kimi-code`. Caveat: provider credentials are read **only** from config.toml (`[providers.<name>]` / `[providers.<name>.env]`), "not from `process.env`" (`env-vars.md`), so the private home must also carry credentials or `KIMI_MODEL_*` vars must define a model (`KIMI_MODEL_NAME` synthesises a temporary provider, `env-vars.md` §Define a model from environment variables).
- **CLI flags**: `-p`, `--output-format`, `-m`, `--add-dir` (repeatable), `--agent-file <md>` (per-launch tool allow/deny list), `--skills-dir`, `-S/-c`. No `--config`, `--hooks`, `--permission-rule`, `--allowed-tools`, `--mcp-config` flags (`commands.ts`).
- **Env overrides** of config fields are limited to the documented set (`KIMI_CODE_DANGEROUS_COMMAND_GUARD`, `KIMI_LOOP_*`, `KIMI_MCP_*_TIMEOUT_MS`, `KIMI_CODE_BACKGROUND_*`, …); permission rules and hooks have **no** env/inline form.
- **Project-level files** in the working directory: `.kimi-code/mcp.json` (MCP servers), `.kimi-code/local.toml` (`[workspace] additional_dir`, written by `/add-dir`), `.kimi-code/AGENTS.md`. `overrides.md` states "no project-level config file mechanism" for config.toml — permission rules and hooks cannot be placed per project (the `scope = "project"` value on a rule is just a label consumed by the same user-config evaluator, `user-configured-rule.ts` `USER_CONFIGURED_SCOPES`).
- Which mechanisms can be set per session: hooks → `KIMI_CODE_HOME` only (or an enabled plugin); rules → `KIMI_CODE_HOME` only; tool allow/deny → `--agent-file` or `[tools]` in `KIMI_CODE_HOME`; mode → forced `auto` in `-p`, selectable via `session/set_mode` in ACP; extra dirs → `--add-dir` / ACP `additionalDirectories`; MCP → `.kimi-code/mcp.json`, `$KIMI_CODE_HOME/mcp.json`, or ACP `session/new.mcpServers`.

### 6. Additional directories

- `--add-dir <dir>`: "Add an extra workspace directory for this session. Relative paths resolve against the current working directory. Can be repeated" (`kimi-command.md`); honoured in `-p` (`run-v2-print.ts` line 474 `additionalDirs: opts.addDirs`). TUI `/add-dir`, persisted in `.kimi-code/local.toml` `[workspace] additional_dir`; ACP `session/new.additionalDirectories` (new sessions only); REST `POST /api/v1/workspaces/{id}/add-dir`.
- Semantics of "outside": workspace = cwd + additional dirs (`path-access.ts` `isWithinWorkspace`). Default guard mode is `absolute-outside-allowed`: a **relative** path that escapes the workspace is rejected (`PATH_OUTSIDE_WORKSPACE`), an **absolute** path outside the workspace is **allowed** and proceeds to the permission policies. In `manual` mode, an outside write is not covered by `git-cwd-write-approve` so it falls to `FallbackAsk`; in `yolo`/`auto` it is approved (changelog 0.19-era: "YOLO mode no longer asks before writing or editing files outside the working directory"). So `--add-dir` widens auto-approval and `Glob/Grep` scope; it is **not** a fence. The only fence is the deny-rule/hook from §3.

### 7. MCP

- Registration: `mcp.json` `{ "mcpServers": { ... } }` at `$KIMI_CODE_HOME/mcp.json` (user) or `<cwd>/.kimi-code/mcp.json` (project; project wins on name collision); transports stdio/http/sse; per-server `enabledTools`/`disabledTools`, `env`, `cwd`, `headers`, `bearerTokenEnvVar`, `startupTimeoutMs`, `toolTimeoutMs` (`docs/en/customization/mcp.md`; loader `app/mcpConfig/configLoader.ts` also probes `<projectRoot>/.mcp.json`). ACP: `session/new.mcpServers`. No CLI flag.
- Caveat for `-p`: project-level servers load only if the folder is trusted; "Print mode has no trust prompt, so the engine's workspace-trust gate would silently drop project-level MCP servers" — a stderr warning is printed (`run-v2-print.ts` 281–288). Trust is persisted per workspace in the home's document store (`workspaceTrustService.ts`), so with a fresh `KIMI_CODE_HOME` use `$KIMI_CODE_HOME/mcp.json` instead.
- Permissions: rules match `mcp__<server>__<tool>` with `*`/`**` globs, tool name only ("MCP tool parameters are not included in permission matching"); `[tools] enabled/disabled` and agent-file `tools`/`disallowedTools` accept the same globs. Unmatched MCP calls ask in manual mode and are auto-approved in yolo/auto.

### 8. Summary table

| Mechanism | Level | Path granularity | Denial visible in stream | Settable per session |
|---|---|---|---|---|
| `[[permission.rules]]` deny with `Write(...)`/`Edit(...)` | tool-call gate (pre-exec veto; active in `-p`) | glob on canonical absolute path, `!` negation, braces; **Write/Edit/Read only** — Bash is command-glob, MCP name-only | yes: `{"role":"tool",...,"content":"Tool \"Write\" was denied by permission rule. Reason: …"}` — prose, no flag | `$KIMI_CODE_HOME/config.toml` (env var to a private home); no flag/env/inline |
| `PreToolUse` hook | tool-call gate, before permission checks; **fail-open** | anything you compute from `tool_input` (path for Write/Edit, `command` for Bash) | yes: `role:"tool"` line whose `content` is your reason verbatim — prose | `[[hooks]]` in `$KIMI_CODE_HOME/config.toml` or an enabled plugin manifest |
| `[tools] enabled/disabled`, agent-file `tools`/`disallowedTools` | tool removal (model never sees it) | none (whole tool; MCP globs) | no (tool absent) | `--agent-file <md>` flag; `[tools]` in private home |
| Permission modes (`manual`/`yolo`/`auto`) | tool-call gate via approval prompt | none (whole tool; built-in sensitive-file and `.git` asks) | TUI/ACP only: `Tool "…" was not run because the user rejected…`; `-p` forces `auto` | `-p` (auto, fixed); ACP `session/set_mode`; `default_permission_mode` in private home |
| ACP `session/request_permission` (harness as ACP client, mode `default`) | tool-call gate — client answers `approve_once`/`approve_always`/`reject` per call with `toolCallId`, tool name and args | harness-defined (inspect the path in the request) | yes: the harness *is* the decider; engine emits `tool_call` updates via `session/update` | per `session/new` (`cwd`, `additionalDirectories`, `mcpServers`) + `session/set_mode` |
| Sensitive-file path guard | path resolution (always on) | fixed built-in list (`.env*`, keys, credentials) | yes: prose `matches a sensitive-file pattern` | not configurable |
| Filesystem sandbox | **none** | — | — | — |

Recommendation for the orchestrator: run `kimi -p … --output-format stream-json` with `KIMI_CODE_HOME` pointing at a per-run home containing credentials, the two `Write`/`Edit` negated-glob deny rules, a `PreToolUse` hook on `Bash` (and, if you want a second line, on `Write|Edit`) that also logs decisions to a harness-owned file, and `--add-dir` for the extra directory so relative-path escapes are still rejected. Detect denials by matching the `content` of `role:"tool"` lines against the sentinel reasons you set, since no structured flag exists. Where a hard guarantee is required, the ACP route is the only one in which the harness holds the decision instead of trusting fail-open hooks.
