I have the full picture. Writing up the audit.

---

# FRONT audit — the adapter boundary (`crates/adapters`, engine dispatch, CLI wiring)

## 1. MAP

### 1.1 `Adapter` trait — `crates/adapters/src/session.rs:251-280`

| Method | Signature | Notes |
|---|---|---|
| `id` | `fn id(&self) -> &'static AdapterId` | sync; `AdapterId` is a validated newtype (`crates/core/src/ids.rs:352`) |
| `capabilities` | `fn capabilities(&self) -> Capabilities` | sync, constant after construction |
| `probe` | `async fn probe(&self) -> Result<ProbeReport>` | |
| `staged_paths` | `fn staged_paths(&self, &SessionRequest) -> Vec<PathBuf>` | default `Vec::new()`; worktree-relative paths the scope check excludes |
| `spawn` | `async fn spawn(&self, SessionRequest) -> Result<Box<dyn AgentSession>>` | |
| `resume` | `async fn resume(&self, &SessionId, SessionRequest)` | default `Err(AdapterError::Unsupported { what: Capability::ResumeSession })` — typed, not stringly (`session.rs:275-278`) |

### 1.2 `AgentSession` trait — `session.rs:282-305`

`events() -> BoxStream<'_, AgentEvent>`, `interrupt()`, `kill()`, `pgid() -> Option<Pid>` (default `None`, for mock).

### 1.3 `SessionRequest` — `session.rs:44-105`

`prompt`, `cwd`, `model`, `agent`, `permissions`, `env: HashMap<String, Secret<String>>`, `edit_constraints: Option<Vec<Glob>>`, `budget`, `adapter_settings`, `skills: Vec<PathBuf>`, `run_tools_endpoint: Option<RunToolsEndpoint>`, `artifact_dir: Option<PathBuf>`, `scratch_dir: Option<PathBuf>`.

`RunToolsEndpoint { url: String, token: Secret<String> }` + `SERVER_NAME = "yunta"` (`session.rs:113-126`) — the one shared name, and the only `mcp__` knowledge in the tree lives inside `crates/adapters` (verified by grep: `claude_code/mod.rs:147`, `claude_code/parse.rs:58`, `codex/mod.rs:159`).

### 1.4 `Capabilities` — `crates/core/src/capabilities.rs:17-45`

8 flags: `resume_session`, `edit_hooks`, `permission_profiles`, `custom_agents`, `usage_reporting`, `skills`, `run_tools`, `network_isolation`. `Capability` enum (`:54-63`) is the closed twin with canonical snake_case spelling (`as_str`, `:67`), and `Capabilities::declares` (`:92`) is the single lookup.

### 1.5 `ProbeReport` — `session.rs:185-191`: `Healthy { version: Option<String> } | Unhealthy { diagnostic: String }`.

### 1.6 Runner → session, step by step

| # | Step | Site |
|---|---|---|
| 1 | node's `runner:` (or `defaults.runner`) resolved against `runners:` candidates; `available` is injected, so the resolver is pure | `crates/engine/src/runner.rs:67-120` |
| 2 | node's own `agent:` overrides the candidate's; `CustomAgents` gate fails the node | `run/runner_resolve.rs:51-79` |
| 3 | `runner_resolved` emitted with chosen + every discarded candidate | `runner_resolve.rs:80-88` |
| 4 | `NetworkIsolation` → `capability_degraded` when `network: false` unenforceable | `runner_resolve.rs:246-269` |
| 5 | `Skills` gate → degrade to `Vec::new()` | `run/prompt_exec.rs:151-170` (prompt) / `run/loop_exec/mod.rs:265-284` (loop) |
| 6 | `RunTools` gate → listener or fail or degrade | `runner_resolve.rs:167-239` (prompt) / `loop_exec/mod.rs:288-316` (loop) |
| 7 | `submission_notice` appended to the prompt only when a listener exists | `run_tools/notice.rs:25-40` |
| 8 | `SessionRequest` built | `prompt_exec.rs:205-219` / `task_cycle/attempt.rs:263-283` |
| 9 | `staged_paths(&request)` captured before dispatch | `prompt_exec.rs:238`, `attempt.rs:284` |
| 10 | `dispatch_session` → `resume()` or `spawn()`, pgid registered | `task_cycle/session.rs:150-173` |
| 11 | stream folded into log; budget raced | `session.rs:184-254` |
| 12 | cancel/budget → `interrupt` → 200 ms grace → `kill` | `session.rs:138`, `:240-247` |

### 1.7 `SessionObserver::emit_session_event` — `task_cycle/session.rs:80-91`

`async fn emit_session_event(&self, &NodeId, EventPayload) -> Result<(), StorageError>`. Mapping in `apply_agent_event` (`:277-392`):

| `AgentEvent` | Log event |
|---|---|
| `SessionOpened` | `AgentSessionOpened { session_id, agent, model, capabilities: adapter.capabilities() }` |
| `RunToolsMounted { count }` | nothing, unless offered && `count == 0` → `CapabilityDegraded(RunTools)` (`:115-124`, `:303-309`) |
| `ToolUse` | `AgentMessage { ToolUse, tool_name, target_digest }` |
| `Note` | `AgentMessage { Note, text: note_summary(text) }` — size + 12 hex of sha256 only (`:129-132`) |
| `Usage` | `AgentMessage { Usage, … }` + budget fold |
| `Completed`/`Failed` | terminal `DispatchOutcome`, no direct event |

Returning `Result` is right and is used: a failed append becomes `DispatchError::Audit` and fails the node (`:301`, `attempt.rs:287-296`) rather than silently thinning the trail.

---

## 2. FRONTERA — adapter knowledge outside `crates/adapters`

Grep results for adapter ids, binary names, CLI flags and CLI paths across `core`, `engine`, `cli`:

| Hit | Verdict |
|---|---|
| `crates/engine/**` — **zero** hits for `"claude"`, `"codex"`, `--print`, `--mcp-config`, `.claude`, `mcp__`, `binary` | **Clean.** The engine genuinely knows adapters only by `AdapterId` + `Capabilities`. This is the strongest part of the boundary. |
| `crates/core/src/ids.rs:846-852` | test-only literal. Fine. |
| `crates/cli/src/commands/mod.rs:31-34,175-186` `real_adapters` | Legitimate: the CLI is the composition root. But it is a hand-written `if`-chain — a third adapter means editing three places. |
| `crates/cli/src/commands/mod.rs:226-229` `refuse_unrunnable` | Hard-codes the sentence *"only `claude-code` and `codex` are built"* instead of listing `adapters.keys()`. Drifts the moment a third adapter lands. |
| `crates/cli/src/commands/init.rs:112-118` `ProbedAdapter { id: &'static str }` with `("claude-code", …)`, `("codex", …)`; fallback literal at `:180` | **Frontera leak (mild).** The id is declared by the adapter (`Adapter::id()` / `CLAUDE_CODE_ID`) and re-spelled here as a bare `&'static str`. `real_adapters` uses the constants; `init` does not — two spellings of one name. |
| `crates/cli/src/commands/run.rs:158-170`, `run/attached.rs:82` — `MOCK_ID` special-cased | Acceptable: `--adapter mock --fixture` is a surface concept, and the refusals are explicit. |
| `crates/cli/src/surface/lines.rs:297` | test fixture only. |

**Capabilities that should be declared instead of named:** none found in the engine. Two near-misses worth naming:

- `run_tools/notice.rs` tells the session to call `yunta_submit_tasks` (`crates/core/src/workflow/artifacts.rs:241`), but `claude-code` mounts it as `mcp__yunta__yunta_submit_tasks` (`claude_code/mod.rs:147`). The engine writes the prompt sentence; no adapter gets to render the mounted name. Today it works because agents resolve by suffix, but the *name a session is told to call* is adapter-specific knowledge the adapter never declares.
- `PROBE_TIMEOUT`/`MAX_LINE_BYTES` (`subprocess.rs:29,33`) are shared-adapter constants, correctly inside the crate.

### 2b. Fixture rendering in the CLI — wrong layer

`load_mock_fixture` (`crates/cli/src/commands/test.rs:348-371`) reads the fixture, builds `{run.dir, worktree, staging}` and calls **`yunta_engine::render_template`** on the mock adapter's own authored YAML before `MockAdapter::from_yaml`.

Three things are wrong with the layer:

1. The fixture format belongs to `mock` (`crates/adapters/src/mock/fixture.rs`), and its variables are the mock's contract with the run. The CLI is the only place that knows they exist.
2. It reaches through two boundaries at once: the CLI borrows the **engine's workflow template renderer** to render an **adapter's** file.
3. It is already duplicated-by-omission: `crates/testkit/src/bench.rs:284` calls `MockAdapter::from_yaml(fixture_yaml)` directly with **no rendering**, so a fixture that works under `yunta test` is unusable from the engine's own bench. The second copy did not get created — the capability simply vanishes below the CLI.

`staging_root` is engine knowledge (`yunta_engine::run_dir::staging_root`), which is exactly why the renderer belongs in the engine's mock-driving seam or in `mock` with the paths passed in — not in `commands/test.rs`.

---

## 3. CAPABILITIES — declared, consumed, degraded

| Capability | Declared by | Consumed at | Degradation |
|---|---|---|---|
| `resume_session` | cc ✓ `claude_code/mod.rs:205`, codex ✓ `codex/mod.rs:206`, mock=fixture | `prompt_exec.rs:88-104` | `capability_degraded` + fresh session ✓ |
| `edit_hooks` | cc ✗ `:208`, codex ✗ `:214`, mock=fixture | **nowhere** | **none** ✗ |
| `permission_profiles` | cc ✓ `:210`, codex ✓ `:216`, mock=fixture | **nowhere** | **none** ✗ |
| `custom_agents` | cc ✓ `:211`, codex ✗ `:220` | `runner_resolve.rs:56-79` | node **fails** (spec says check-time error) |
| `usage_reporting` | cc ✓ `:212`, codex ✓ `:228` | **nowhere** | **none** ✗ |
| `skills` | cc ✓ `:218`, codex ✗ `:210` | `prompt_exec.rs:151-170`, `loop_exec/mod.rs:265-284` | `capability_degraded` ✓, but **two copies** |
| `run_tools` | cc ✓ `:215`, codex ✓ `:226` | `runner_resolve.rs:182-238`, `loop_exec/mod.rs:288-316`, `attempt.rs:209-241`, `session.rs:303-309` | fail (blackboard / typed artifact) or `capability_degraded` ✓, **four sites, four wordings** |
| `network_isolation` | cc ✗ `:222`, codex ✗ `:230` | `runner_resolve.rs:252-268` | `capability_degraded` ✓ |

**Is every consumption site going through one check?** Yes for the *predicate* — every site calls `Capabilities::declares(Capability::X)` (`capabilities.rs:92`), nothing reads a bool field directly, and nothing infers a capability from an `AdapterId`. That part is right and rare.

**No.** for the *policy*. `policy_applied` is a free `String` (`crates/core/src/events/payloads.rs:951`) written inline at each of the seven emit sites, so `RunTools` alone degrades with three different sentences (`runner_resolve.rs:235`, `prompt_exec.rs:186`, `attempt.rs:228`, `session.rs:119-122`) and `Skills` with two near-identical ones differing only in "the session" vs "task sessions". The prompt path and the loop path are structurally duplicated checks, not one shared gate.

**Three capabilities are declared and never consulted** — `edit_hooks`, `permission_profiles`, `usage_reporting`. For each, the contract states a degradation that does not exist:

- `contrato-del-run.md:274` and `spec-adapter.md:218`: absent `edit_hooks` → *"degradación explícita a solo-post-check con warning"*. `edit_constraints` is populated unconditionally (`prompt_exec.rs:212`, `attempt.rs:270`) and no event is emitted. Both real adapters declare `false`, so **every real run silently degrades here**.
- `spec-adapter.md:219`: `read_only` on an adapter without `permission_profiles` → check error. `check_warnings` takes no adapter registry and says so (`crates/engine/src/check/mod.rs:241-245`); `check` is not capability-aware at all.
- `spec-adapter.md:221`: absent `usage_reporting` → token budget unenforceable, warning per run. The engine enforces `max_tokens` purely by counting `Usage` events (`session.rs:372-379`), so an adapter that reports none silently never trips the budget — the exact silent degradation the design forbids.

---

## 4. DUEÑO

### 4.1 `subprocess.rs` — correct, and the model to keep

| Concern | Evidence |
|---|---|
| Process group | `std_cmd.process_group(0)` before spawn (`:74-78`); pgid = child pid (`:87-96`), typed `Pid` |
| `kill_on_drop` | `:80` — reaps the leader; the group is handled separately |
| Registration | engine-side: `process_registry::register(observer.process_registry(), session.pgid())` RAII guard for the whole dispatch (`task_cycle/session.rs:170-173`), persisted to `run.dir/scratch/engine.json` (`process_registry.rs:1-13`, `:96-103`) |
| `kill()` | `SIGKILL` to the **group**, abort readers, `wait()`, set `reaped` (`:239-252`) |
| `interrupt()` | `SIGINT` to the group (`:235-237`) |
| Drop | `kill_group()` + `close_pipes()` — the abandoned-session path (`:261-268`) |
| `ESRCH` is success | `signal.rs:72-81` — a group already gone is the desired state |
| Prompt via stdin | `write_prompt` after readers start, `shutdown()`, `BrokenPipe` tolerated (`session.rs:158-180`); `claude` uses `-p` with no positional (`claude_code/mod.rs:153-155`), `codex` uses `-` (`codex/mod.rs:105-106`) |
| stdout/stderr | `LineReader` — non-UTF-8 → replacement chars, `MAX_LINE_BYTES` 1 MiB, oversize line dropped with a warning and reading continues (`:275-329`); out-of-order events before `SessionOpened` become non-retryable `Failed` (`:110-140`) |

Two small gaps: `interrupt()` does not consult `reaped` (a post-kill interrupt could signal a recycled pgid — unreachable on today's paths), and `Drop` does not `wait()` (descendants reparent to init after SIGKILL; acceptable).

### 4.2 Dropped `JoinHandle`s (grep `tokio::spawn(` across all crates)

| Site | Handle | Consequence |
|---|---|---|
| `crates/adapters/src/mock/mod.rs:234` `tokio::spawn(script::play(…))` | **dropped** | **Real defect.** `MockSession` (`:370-374`) holds only the two `Notify`s and has **no `Drop`**. If the session is dropped without `kill()` — e.g. `dispatch_session` returning early on `DispatchError::Audit` mid-stream, or any future abort path — a `Hang` script's player parks on `stops.awaited()` (`script.rs:76-81`) forever, holding the sender and its `Script`. One leaked task per such session for the life of the process. The real adapter kills its tree on drop; the mock does not stop its player. Asymmetry in the one direction that matters: the mock is *weaker* than the contract it stands in for. |
| `crates/adapters/src/subprocess.rs:107,142` | kept in `SubprocessSession` (`:201-202`), `abort()`ed in `close_pipes` | ✓ |
| `crates/engine/src/run_tools/listener.rs:74` | kept in `RunToolsSession.server`, `Drop` cancels + aborts (`:27-38`) | ✓ exemplary |
| `crates/engine/src/process.rs:237,304` | kept: `stdin_task` awaited (`:271-273`), `read_to_end` handles drained (`:274-275`) | ✓ |
| `crates/cli/src/commands/mod.rs:56` Ctrl-C bridge | dropped | Acceptable — process-lifetime daemon, documented `:41-52`. |
| `crates/cli/src/commands/mod.rs:139` detached-resume reaper | dropped | Deliberate and documented (`:103-108`): it exists precisely to reap. Worth keeping the handle anyway so the MCP server can drop it with the run. |
| `crates/cli/src/surface/mod.rs:205`, `surface/turns.rs:235` | kept in `painter` fields | ✓ |
| `crates/storage/src/async_storage.rs:52`, `cli/src/human_interaction.rs:66` | `spawn_blocking`, awaited | ✓ |

### 4.3 Blocking I/O on the async path (CLAUDE.md *Código*: disk goes async or via `spawn_blocking`)

- `claude_code/mod.rs:72-78` (`create_dir_all`, `write`, `set_permissions`) and `:272-296` (`stage_skills`: `create_dir_all`, `remove_file`, `symlink` per skill) — all `std::fs`, called from `async fn launch` (`:163-164`).
- `mock/mod.rs:146-167` `apply_effects` — `std::fs::create_dir_all` + `write` per effect, called from `async fn spawn` (`:220`).
- `process_registry.rs:96-103` `persist()` — `std::fs::write` + `rename`, called from `add`/`remove` on the dispatch path (`task_cycle/session.rs:170`).

### 4.4 Span attribution

`tokio::spawn` does not carry the current span, so the stderr drain (`subprocess.rs:142-147`), the stdout reader and the mock player emit outside the node span opened at `run/node_exec.rs:63` / `task_cycle/mod.rs:227` — their `tracing` lines carry `adapter` but no `run_id`/`node_id`, against CLAUDE.md's "un span por run y por nodo".

---

## 5. SECRETO

| Link | Evidence | Verdict |
|---|---|---|
| Config names variables only | `config/mod.rs:84-88` `secrets: Vec<String>` | ✓ |
| Resolution | `SessionSetup::secrets_env` (`task_cycle/session.rs:59-71`) — declared name ∩ present in engine env → `Secret::from` | ✓ one place, called from `prompt_exec.rs:211` and `loop_exec/mod.rs:320` |
| Transport | `SessionRequest.env: HashMap<String, Secret<String>>` | ✓ |
| Child env, one exposure | `subprocess.rs:62-67` `.envs(… value.expose())` | ✓ the single `expose()` on this path |
| `Debug` | `Secret` has no `Display`, no `Serialize`, `Debug` = `[redacted]` (`core/src/secret.rs:28-32`); asserted at `adapters/tests/claude_code.rs:545` and `tests/codex.rs:661` (both tokens redacted in a whole-request dump) | ✓ |
| Prompt | stdin only, both adapters (§4.1) | ✓ |
| Run-tools token | `RunToolsEndpoint.token: Secret<String>`; **claude** writes it into `scratch/mcp.json` at `0o600`, explicitly to keep it out of `argv` (`claude_code/mod.rs:26-36`, `:74-78`); **codex** passes it as `YUNTA_RUN_TOOLS_TOKEN` in the child env and only the *variable name* on the command line (`codex/mod.rs:54-57`, `:117-121`, `:165-169`) | ✓ genuinely well done |
| Notes in the log | `note_summary` — bytes + 12 hex (`task_cycle/session.rs:129-132`) | ✓ |

### Exposure risks found

1. **`target_digest` carries raw content, not a digest.** `claude_code/parse.rs:131-138` returns the literal `command`, `url`, `file_path` or `pattern` from the tool input and only falls back to sha256 when none is present; `codex/parse.rs:109-138` does the same for `command`/`query`. This string is persisted verbatim (`AgentMessagePayload.target_digest`, `payloads.rs:293`). A `Bash` tool use whose command line embeds a credential lands in the event log in clear, against spec O3/A5 (`spec-adapter.md:199-203`, `:262`).
2. **The engine-side redaction pass the spec promises does not exist.** `spec-adapter.md:201` — *"El engine además redacta todo valor de secreto conocido antes de persistir (I12): defensa en profundidad"*. Grep for `redact` finds only `Secret`'s own `Debug`. There is no filter between a payload and storage, so #1 has no second line of defence.
3. **CLI stderr → trace log.** `subprocess.rs:142-147` debug-logs every stderr line of the child. A CLI that echoes a token in an auth error writes it to the trace log.
4. **Bearer comparison is not constant-time and the expected value leaves `Secret`.** `run_tools/listener.rs:109-125` builds `expected: String` and compares with `==`. Loopback-only and the token is 244 bits, so the timing channel is theoretical — but the value is a plain `String` in a closure for the listener's lifetime.
5. **Stale token file.** `scratch/mcp.json` survives the run (only `engine.json` is cleared, `process_registry.rs:88-94`). Harmless because the listener dies with the session, but it is a credential at rest with no owner.

---

## 6. PARSING

| Aspect | `claude_code/parse.rs` | `codex/parse.rs` |
|---|---|---|
| Entry | `serde_json::from_str::<Value>` then `value.get("type").and_then(Value::as_str)` | same (`:46-59`) |
| Typed vs stringly | **stringly** — every field is `Value::get(&str)` + `as_str`/`as_u64`; no `#[derive(Deserialize)]` struct anywhere in either parser | same |
| Unknown tolerance | unknown `type` → `Vec::new()` (`:30`); unknown content block → `None` (`:127`) | unknown event/item → `Vec::new()` (`:58`, `:105`) |
| Where it is strict | a missing/invalid `session_id` or `model`, or a `result` line with no `is_error`, becomes `Failed { retryable: false }` with a message naming the field (`:74-102`, `:150-153`) | invalid `thread_id` → `Failed { retryable: false }` (`:72-78`) |
| Errors typed with cause | **no** — these are `AgentError { message: String }` (`session.rs:203-207`), a flat string. Preserved cause exists only for the I/O layer (`AdapterError::AdapterIo { source }`) |
| Retryability | `failure::classify(&message)` — string matching on the CLI's prose | same (`:173`) |
| Purity | both modules are pure and total, documented as such | ✓ |

The tolerant-reader stance is correct for a persisted/foreign stream and matches CLAUDE.md ("la tolerancia vive solo en lo persistido y versionado"). But "Parsear es validar" asks for the inverse of what is here: a `#[serde(tag = "type")]` enum with `#[serde(other)] Unknown` would make the recognized shapes irrepresentable-if-invalid while keeping the tolerance, and `codex/parse.rs`'s own doc comment (`:5-37`) already transcribes the exact Rust types from the CLI's source — the type is known, it just is not written down. `field_u64` silently defaulting a missing token count to `0` (`claude:179-181`, `codex:180-182`) is the concrete cost: an absent `output_tokens` becomes a reported zero, which is a count the engine then bills against `max_tokens`.

By contrast the *authored* surfaces are exemplary: `typed_settings` (`session.rs:132-149`) is the single unknown-key check with a typed `AdapterError::UnknownSetting` naming the known keys, `ConfigOverride` (`codex/config.rs`) makes an unquoted TOML value unrepresentable with round-trip tests for quotes, control chars and non-ASCII, and the whole fixture format denies unknown fields (§7).

---

## 7. MOCK

### Is it a genuine client of the run-tools endpoint? **Yes.**

`mock/run_tool.rs:43-96` opens a real `rmcp` streamable-HTTP client against `endpoint.url` with the bearer header, places a real `tools/call`, and reduces the answer to `tool:sha256[..12]` for the audit stream. `ToolExpectation` (`fixture.rs:193-205`) makes both outcomes assertable and makes *both* mismatches fail the session loudly (`script.rs:176-197`). A `run_tool` step on a session with no endpoint is a loud authoring error, never a skip (`run_tool.rs:16-21`). This is the strongest piece of the mock — it tests the engine's listener over the wire, not a stub of it.

### Fixture strictness

`deny_unknown_fields` on every shape: `Multi` (`fixture.rs:46`), `FixtureCapabilities` (`:86`), `SessionScript` (`:118`), `MockStep` (`:145`), `MockEffect` (`:226`), `MockOutcome` (`:237`). The two forms are told apart **structurally** by the presence of `sessions:` (`:40-43`), never by "whatever parses". Exhaustion is an explicit error naming the count (`mod.rs:258-267`); an ambiguous `match_prompt_contains` is an error, not a coin flip (`:299-310`).

`FixtureCapabilities` (`:87-96`) is a hand-maintained twin of `Capabilities` with `From` (`:98-111`) — the right call (authored YAML must refuse an unknown flag; the log must tolerate one), and the doc comment says exactly why (`:81-84`). **But nothing keeps the twin in sync**: the `From` impl is exhaustive-by-field, not by destructuring, so adding a 9th capability to core compiles fine and the fixture silently cannot express it.

### What the mock can do that no real adapter can

| Behaviour | Mock | claude-code | codex |
|---|---|---|---|
| `edit_hooks: true` → effects blocked pre-write, reported as `ToolUse blocked:<path>` (`mod.rs:134-139`, `script.rs:98-108`) | ✅ fixture can declare it | ❌ `:208` | ❌ `:214` |
| `network_isolation: true` | ✅ fixture can declare it | ❌ `:222` | ❌ `:230` |
| `permission_profiles` honoured per profile | recorded only | `--tools` + `acceptEdits` | `--sandbox` |
| `pgid()` | `None` (no registry entry, no group kill) | `Some` | `Some` |
| `RunToolsMounted` | scriptable | emitted from the init line's tool set (`parse.rs:57-68`) | **never emitted** |

So an engine test can prove hot scope enforcement and network isolation against `mock` that **cannot happen on any shipped adapter** — `crates/adapters/tests/mock.rs:146` (`capabilities: { edit_hooks: true }`) is exactly such a test. And `mock_adapters` (`cli/commands/test.rs:374-389`) installs the mock under *every* adapter id the config names, so a `yunta test` case declaring `runner: … adapter: claude-code` runs against fixture-declared capabilities, not claude-code's real ones — a green case is no evidence about the real adapter's capability set.

Conversely the mock is missing a real adapter's ownership: no `Drop`, no process group, no player shutdown (§4.2).

---

## 8. DEFECTS

| # | Defect | Evidence | Category | Severity |
|---|---|---|---|---|
| D1 | Mock's player task handle dropped; `MockSession` has no `Drop` — a session dropped without `kill()` leaks a parked task holding the script | `mock/mod.rs:234`, `:370-374`; `script.rs:76-81` | dropped handle | high |
| D2 | `edit_hooks` never consulted: `edit_constraints` populated unconditionally, no `capability_degraded`, though both real adapters declare `false` | `prompt_exec.rs:212`, `attempt.rs:270`; contract `contrato-del-run.md:274`, `spec-adapter.md:218` | capability declared-not-consumed / silent degradation | high |
| D3 | `usage_reporting` never consulted: `max_tokens` is enforced only by counting `Usage`, so an adapter that reports none silently has no token budget | `session.rs:372-379` vs `spec-adapter.md:221` | silent degradation | high |
| D4 | `permission_profiles` never consulted; `check` is not capability-aware at all | `check/mod.rs:241-245` vs `spec-adapter.md:211,219` | doc/code divergence | high |
| D5 | `target_digest` persists the raw `command`/`url`/`file_path`, not a digest — a secret in a command line reaches the log | `claude_code/parse.rs:131-138`, `codex/parse.rs:109-138`, `payloads.rs:293` | secret exposure risk | high |
| D6 | The engine-side "redact every known secret before persisting" pass the spec promises does not exist | grep `redact`; `spec-adapter.md:201` | doc/code divergence + defence-in-depth gap | high |
| D7 | `claude-code` `ReadOnly` profile includes `Write` with nothing confining it to `artifact_dir`, while `permission_profiles: true` is declared | `claude_code/permissions.rs:36-47` (the comment admits it) | capability over-declared | high |
| D8 | Fixture template rendering lives in the CLI and is absent from every other consumer | `cli/commands/test.rs:348-371` vs `testkit/src/bench.rs:284` | layering | high |
| D9 | Skills + run-tools capability gating duplicated between the prompt path and the loop path, with divergent messages | `prompt_exec.rs:151-194` vs `loop_exec/mod.rs:265-316`; `runner_resolve.rs:99-137` vs `loop_exec/mod.rs:302-313` | "un lugar" | medium |
| D10 | `policy_applied` is free text written at seven sites; `RunTools` degrades with three different sentences | `runner_resolve.rs:235`, `prompt_exec.rs:186`, `attempt.rs:228`, `session.rs:119-122` | "un lugar" | medium |
| D11 | `codex` `build_args` swallows a settings error and falls back to the default sandbox instead of failing; only `probe()` reports it | `codex/mod.rs:85-90` vs `:234-241` | silent degradation | medium |
| D12 | Blocking `std::fs` on the async spawn path (mcp config, skill symlinks, mock effects, registry persist) | `claude_code/mod.rs:72-78,283-294`; `mock/mod.rs:158-164`; `process_registry.rs:96-103` | async discipline | medium |
| D13 | Adapter stream parsers are entirely stringly-typed `serde_json::Value` walks; `field_u64` turns a missing count into `0` | `claude_code/parse.rs`, `codex/parse.rs:180-182` | stringly parsing | medium |
| D14 | `AgentError { message: String }` — a parse failure keeps no typed cause | `session.rs:203-207` | error typing | medium |
| D15 | `FixtureCapabilities` twin has no compile-time or test guard against drifting from `Capabilities` | `fixture.rs:87-111` | latent divergence | medium |
| D16 | `init.rs` re-spells `"claude-code"`/`"codex"` as bare `&'static str` instead of the adapters' own `ID` constants | `cli/commands/init.rs:112-118,180` | frontera leak (mild) | low |
| D17 | `refuse_unrunnable` hard-codes "only claude-code and codex are built" instead of listing the registry | `cli/commands/mod.rs:226-229` | frontera leak (mild) | low |
| D18 | Surface prints `RunTools` (`{:?}`) where the log and spec say `run_tools`; `Capability::as_str` exists for this | `cli/surface/lines.rs:151`, test asserts the wrong spelling at `:300`; `capabilities.rs:67` | "un lugar" / expression | low |
| D19 | `codex` never emits `RunToolsMounted`, so the "mounted and holding nothing" degradation (`spec-adapter.md:222`) can never fire on codex | `codex/parse.rs:45-59` | coverage gap | low |
| D20 | `codex` `ReadOnly` + `artifact_dir` sets `sandbox_workspace_write.writable_roots` under a `read-only` sandbox, where it has no effect — node fails at close with no event | `codex/mod.rs:143-157`, `permissions.rs:29-33` | silent degradation | low |
| D21 | Spawned readers/players lose the run/node span; stderr lines carry no `run_id`/`node_id` | `subprocess.rs:142-147` vs `node_exec.rs:63` | observability | low |
| D22 | `spec-events.md:171` lists 6 capabilities (no `skills`, no `network_isolation`) and `model` as mandatory; the code has 8 and `Option<ModelName>` | `capabilities.rs:17-45`, `session.rs:215-218` | doc/code divergence | low |
| D23 | `docs/adapters.md:30` says the engine "picks the first whose adapter is healthy"; it picks the first *constructed*, and `probe_or_refuse` refuses the whole run on any unhealthy adapter rather than falling through | `runner.rs:9-10,95`, `cli/commands/mod.rs:241-250` | doc/code divergence | low |
| D24 | `spec-adapter.md:230-236` says `claude-code` "declara todas las capacidades" and implements `edit_hooks`, and that `probe()` verifies agent existence; none of that is built | `claude_code/mod.rs:201-223`, `:226-234` | doc/code divergence (documentation wins per CLAUDE.md) | medium |

---

## 9. IDEAL

### Already right — keep, do not refactor away

| Piece | Why |
|---|---|
| **Engine carries zero CLI knowledge** — no binary, flag, path or `mcp__` prefix anywhere in `crates/engine` | This is the invariant the whole spec exists for (A1), and it holds today. Any redesign must preserve it as the first constraint. |
| **`Capabilities` in `core`, one `declares()` lookup, `Capability` as a closed enum with canonical spelling** | Makes "a degradation can never cite a capability no adapter has" true by type. Nothing infers a capability from an adapter id. |
| **`Capabilities` twin for fixtures** (`FixtureCapabilities`, `deny_unknown_fields` + `From`) | Exactly the right split: authored YAML is strict, the persisted event is tolerant. Keep the twin; add the sync guard (below). |
| **`RunToolsEndpoint` with `Secret<String>` + `SERVER_NAME`** | One name both adapters translate natively; the token never reaches `argv` in either translation, by two different correct mechanisms. |
| **`staged_paths`** | The clean answer to "the adapter wrote in the worktree and the scope check must not read it as agent work": computed once, declared by the adapter, consumed by the engine (`prompt_exec.rs:238`, `attempt.rs:284`). |
| **`SessionObserver::emit_session_event -> Result<(), StorageError>`** | A lost audit event fails the node instead of thinning the trail — the replay discipline enforced at the type. |
| **Process-group creation + group kill on every path, incl. `Drop`** | `subprocess.rs:74-78`, `:239-252`, `:261-268` plus `RunToolsSession::Drop` (`listener.rs:33-38`). This is the "Dueño" model done right; the mock should be brought up to it, not the reverse. |
| **`typed_settings`, `ConfigOverride`, `LineReader`** | One place each, typed error, round-trip tested. |
| **`note_summary`** | The log carries size + hash, never the text. |
| **Mock as a real MCP client** | `run_tool.rs` proves the listener over the wire. |

### What a greenfield boundary declares

1. **Capability → policy is data, not prose.** One table in `core` mapping each `Capability` to its absence policy (`FailNode`, `DegradeWith(&'static str)`, `Resting`), and one engine-side `fn require(adapter, capability, ctx) -> Decision` that every site calls. That kills D9, D10, and makes D2/D3/D4 impossible to forget: a capability with no policy row does not compile.
2. **One dispatch path for prompt and task sessions.** `prompt_exec.rs:122-278` and `attempt.rs:190-298` build the same `SessionRequest` with the same gates in two hand-written orders. A single `SessionPlan { node, task: Option<TaskId>, prompt, profile, artifact_dir, resume }` → `open_session(plan) -> (SessionRequest, Vec<CapabilityDegraded>)` gives one place where `skills`, `run_tools`, `network`, `agent` and `scratch_dir` are decided, with `dispatch_session` unchanged below it. The loop path then stops needing its own copy of the blackboard refusal.
3. **The fixture renderer moves down.** `MockFixture::parse(yaml, &RunPaths { run_dir, worktree, staging })` in `crates/adapters/src/mock/fixture.rs`, with `RunPaths` handed in by whoever drives the run. `commands/test.rs` shrinks to reading a file; `testkit/bench.rs` gets the same rendering for free; the engine's workflow template renderer stops being reached for by the CLI.
4. **`edit_constraints`, `skills`, `agent`, `run_tools_endpoint`, `budget.max_turns` are populated only behind their capability** — enforced at construction, not by convention. Today `SessionRequest`'s doc comments state the rule (`session.rs:58-61`, `:69-72`, `:77-80`) and three of the five are honoured; a smart constructor taking `Capabilities` makes all five true.
5. **`target_digest` is a digest.** Either hash unconditionally, or introduce a `ToolTarget { display: Option<String>, digest: ContentHash }` where `display` is admitted only for shapes that cannot carry a credential (a worktree-relative path), and never for `command`/`url`.
6. **Adapter stream events are typed at the edge.** `#[serde(tag = "type")]` enums with an `Unknown` catch-all variant per adapter, `Option<u64>` for counts that may be absent. The tolerance stays; the guessing goes. `codex/parse.rs`'s doc comment already contains the schema.
7. **A capability-aware `check`.** `check(workflow, config, &Adapters)` so `read_only` on an adapter without `permission_profiles`, and `agent:` on one without `custom_agents`, fail before a worktree exists — which is what `spec-adapter.md:209-212` describes and what `runner_resolve.rs:56-79` currently does too late.
8. **Mock reaches parity on ownership**: keep the player's `JoinHandle` in `MockSession` and `abort()` it in `Drop`, mirroring `SubprocessSession`.
9. **Twin sync guard**: a test that round-trips every `Capability::ALL` through `FixtureCapabilities` so a 9th flag breaks the build rather than silently becoming unexpressible.

### One item for the human (CLAUDE.md step 2, *Levantar*)

Four defects (D2, D3, D4, D24) are the same shape: **the documentation describes behaviour the code does not have**, and CLAUDE.md says the documentation wins absent a registered decision. I found no ADR covering them — `docs/design/adr/` holds only a `README.md`, and `deuda-consciente.md` registers neither. So either the three unconsumed capabilities get their degradations built (and `check` becomes capability-aware), or `spec-adapter.md §5`/`contrato-del-run.md:274`/I9 get amended by a recorded decision. That choice — build the degradations vs. narrow the contract — is not mine to make; it needs a decision before any of D2/D3/D4 is "fixed" in either direction.
