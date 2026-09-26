I've completed the audit. Here are the findings.

---

# 1. MAP — the life of an artifact, end to end

| Stage | Function / site | Notes |
|---|---|---|
| **Declared** | `Artifacts { produces: Vec<ArtifactSpec> }` — `crates/core/src/workflow/artifacts.rs:23`; `ArtifactSpec::{Interpreted(ArtifactKind), Opaque(String)}` :37; custom `Deserialize` :49 ("a string that names a kind *is* that kind") | `ArtifactKind` :188 (`Tasks`/`Findings`/`Questions`, alias `task-ledger`) |
| **Declaration → identity** | `impl From<&ArtifactSpec> for ArtifactId` — `crates/core/src/events/payloads.rs:834`; `ArtifactId` :787 | The one place a declaration becomes the question the log answers |
| **Names rendered** | `render_artifact_names` — `crates/engine/src/run/node_exec.rs:348`; `declared_artifacts` :316; `artifact_dir` :334 | Only opaque names template |
| **Shape published** | `artifact_shapes` — `crates/engine/src/run/context_resolve/shapes.rs:69` → `shape::contract` — `crates/core/src/shape/mod.rs:151` | Mounted as `stable` context |
| **Tools mounted** | capability gate `open_run_tools` — `crates/engine/src/run/runner_resolve.rs:167`; catalog `mounted` — `crates/engine/src/run_tools/catalog.rs:19` | |
| **Produced — submitted** | `SessionTools::submit` — `run_tools/submission.rs:97` → `artifacts::submit` — `artifacts/canonical.rs:84` (validates via `shape::accept`, renders canonical) → `record` :136 emits `artifact_submitted` then `accept(…, Submitted)` | |
| **Produced — derived (findings)** | `node_artifacts::derive_findings` — `run/node_artifacts.rs:47` → `FindingLedger::effective_of` → `artifacts::derive_findings` — `canonical.rs:125` → `accept(…, Derived)` | Runs **before** the close asks the log |
| **Produced — file in staging** | node writes under `run_dir::staging` — `run_dir.rs:36`; read back by `verify_one` — `artifacts/ingest.rs:171` via `run_dir::staged_path` :42 | Command nodes, and opaque artifacts of session nodes |
| **Produced — answered** | `questions_exec::execute_ask` — `run/questions_exec.rs:26`; `answers_artifact()` — `artifacts/mod.rs:55` → `accept(…, Answered)` :139 | |
| **Produced — from a child run** | `acquire_from_child` — `node_artifacts.rs:201` → `accept(…, Inherited{run,producer})` :222 | |
| **Produced — at birth** | `inputs::read_document` — `engine/src/inputs.rs:192` → `canonical_document` — `canonical.rs:206`; mounts — `workflow_exec/mounts.rs:124`; promotion — `run/promote.rs:131` → `register_birth_documents` — `run/create.rs:313` | Origins `Input`/`Inherited` |
| **Verified** | `close_artifacts` — `ingest.rs:96`, dispatching on `answered_by_the_log` — `artifacts/mod.rs:100` → `held_document` :141 or `verify_one` :171; both end in `interpret` :270 → `shape::read` | Collects every failure |
| **Accepted** | `artifacts::accept` — `artifacts/mod.rs:119`: `ObjectStore::put` → `artifact_accepted` → `ObjectStore::project` | Order is load-bearing and documented |
| **Recorded as meaning** | `record_artifacts` — `node_artifacts.rs:252`; `record_content` :285 → `tasks::register` — `engine/src/tasks/mod.rs:131` (`task_registered`) / re-emits `finding_posted` for a `workflow` node :313 | |
| **Stored / viewed** | `ObjectStore` — `artifacts/store.rs:55`; `objects/<sha256>` :26; `view_path` :147 (`artifacts/<node>/<name>`, root when producerless) | Read verifies hash :103 |
| **Folded** | `ArtifactLedger::of` — `crates/core/src/events/artifacts.rs:62`; `latest` :121, `of_kind` :134, `by_producer` :141, `every` :148 | |
| **Consumed** | context source `sources.rs:135`; loop `load_registered_tasks` — `loop_exec/mod.rs:438`; mounts `mounts.rs:187`; promotion `promote.rs:145`; distill `distill.rs:160`; gate attachments `gate_exec.rs:72`; view `view/node.rs:154`; integrity `integrity.rs:64` | All through `RunArtifacts`/`ArtifactLedger` — **nothing `read_dir`s `artifacts/`** |

---

# 2. ONE DOOR CHECK

| # | Question | Verdict | Evidence |
|---|---|---|---|
| a | Read a document's rules/problems | **One door** ✅ | `shape::read` — `core/src/shape/mod.rs:71` (parse + `Document::check` in one function, by design :11-14); `shape::accept` :110 is its structured twin, same type, same rules. Every reader routes through `interpret` — `ingest.rs:270`. `Document` is sealed :34 |
| b | Write the view | **One door** ✅ | `ObjectStore::project` — `store.rs:120`, called only from `accept` — `artifacts/mod.rs:147`. `view_path` :147 is the only layout statement |
| c | Where a file is read from | **One door each** ✅ | `staged_path` — `run_dir.rs:42` (one caller: `ingest.rs:181`); `store::view_path` (`mod.rs:203`, `ingest.rs:209/223`, `questions_exec.rs:132`) |
| d | Content hash | **One door** ✅ | `sha256_hex` — `core/src/hash.rs:120`, `ContentHash` newtype :44. **Caveat:** `VerifiedArtifact.content_hash` from `verify_one` (`ingest.rs:190`) is the hash of the *staged* bytes, not of what the store holds (canonical) — documented at `ingest.rs:50-53`, but two hashes under one field name |
| e | Document → events | **Tasks: one door** ✅ `tasks::register` — `tasks/mod.rs:131` (callers: `node_artifacts.rs:306`, `create.rs:326`). **Findings: two doors** ❌ | `FindingLedger` (`core/src/events/findings.rs:57`) is the declared single fold, but `run_tools/blackboard.rs:27-39` and `:70-79` fold `FindingPosted` by hand. **Questions:** one round — `questions_exec.rs:26`, documented as the ONE ask site |
| f | "Answered by the log vs by a file" | **Two doors** ❌ | `answered_by_the_log(node_kind, kind)` — `artifacts/mod.rs:100`, whose own rustdoc says "so it is answered once here", has exactly two callers (`ingest.rs:110`, `node_artifacts.rs:263`). `run_tools/submission.rs:67-76` re-derives the same decision from `spec.kind()` alone. They agree only because run tools are session-only — an accident, not a type |

**Additional duplicates (Un lugar):**

- **Tool-name literals in dispatch vs catalog.** `catalog.rs:39/100/109/128` and `session.rs:171/177/178/179` each spell `yunta_check_artifact`, `yunta_task_status`, `yunta_get_blackboard`, `yunta_request_scope_expansion`. Worse, it is *inconsistent*: `session.rs:173-176` matches `UPDATE_FINDING_TOOL`/`WITHDRAW_FINDING_TOOL` through the constant, while `session.rs:172` hardcodes `"yunta_post_finding"` next to `catalog.rs:61` which uses `ArtifactKind::POST_FINDING_TOOL`. `notice.rs:119` hardcodes `yunta_check_artifact` a third time. Only the five names in `core/src/workflow/artifacts.rs:241-260` live in one place.
- **"Does this node declare kind K"** written four ways: `schedule.rs:174`, `questions_exec.rs:36-41`, `node_artifacts.rs:26-33`, `submission.rs:186`, plus `node_exec.rs:337` for the opaque case. No `Node::declares(kind)`.
- **Canonical rendering of a findings document** has a second door: `run/steps.rs:256-272` renders with `yaml::to_string` and accepts directly, bypassing `canonical::derive_findings` — and therefore bypassing the `max_artifact_bytes` guard that `rendered_document` (`canonical.rs:169`) exists to enforce.

---

# 3. LEGACY & TOLERANCE

**Verdict: correctly confined, with one type-level hole.**

| Item | Where | Judgement |
|---|---|---|
| `ArtifactWritten(ArtifactWrittenPayload)` | `core/src/events/mod.rs:292`, payload `payloads.rs:305` | **Read-only.** No production site constructs it — the only uses are the fold `events/artifacts.rs:92` and the CLI line renderer `cli/src/surface/lines.rs:94`. ✅ |
| `legacy_identity` / `artifact_name` | `core/src/events/artifacts.rs:158,167` | Rebuilds the identity from a persisted path. Confined to the fold. ✅ |
| `ArtifactOrigin::Legacy` | `payloads.rs:908` | Produced only by the fold (`artifacts.rs:95`); consumed only by `integrity.rs:65` to count `unverifiable`. Degradación explícita is exemplary: `integrity.rs:12-24` and `unverifiable_detail` :106 name what could not be checked and why. ✅ |
| `#[serde(alias = "task-ledger")]` | `workflow/artifacts.rs:194` | Alias declared once, on the derive, with a comment explaining why it is not rustdoc. ✅ |
| `ArtifactWrittenPayload.artifact_kind: Option<…>` | `payloads.rs:328` | Tolerant reader on persisted data. ✅ |

**Hole:** `ArtifactAcceptedPayload.origin` (`payloads.rs:769-773`) is typed `ArtifactOrigin`, so `Legacy` is representable in a *fresh* acceptance. The invariant "Legacy only ever comes from an `artifact_written`" is upheld by convention, not by type. The clean shape is a `RecordedOrigin` (what `accept` may write) widening into an `ArtifactOrigin` (what a fold may produce) — legacy tolerance then cannot leak into the authored path even by accident.

---

# 4. RUN TOOLS

**Call → events.** `SessionTools::call_tool` (`session.rs:164`) matches the name, runs a handler, and renders the typed `RunToolError` once at the boundary :189-194. Handlers reach the log only through `SessionTools::append` :142, which stamps `(run, node)` and the run's injected clock :131. Events written:

| Tool | Events |
|---|---|
| `yunta_submit_*` | `artifact_submitted` (`submission.rs:164`) **and**, on acceptance, `artifact_accepted` via `accept` :171 — two facts, deliberately (:130-135) |
| `yunta_post/update/withdraw_finding` | `finding_posted`/`finding_updated`/`finding_withdrawn` (`findings.rs:33,46,74`); every refusal is a `finding_refused` :188 |
| `yunta_check_artifact` | none — consultative |
| `yunta_task_status`, `yunta_get_blackboard` | none — reads |
| `yunta_request_scope_expansion` | **none** — writes a file (`tasks.rs:64`) that the post-attempt evaluation consumes |

**Is `yunta_check_artifact` the same code path as the close?** Substantially yes — `held_document` and `verify_one` are `pub(crate)` precisely so both callers share them (`ingest.rs:138-140`, `:167-170`), and `render_verdict` (`submission.rs:212`) reuses the close's `ArtifactFailure`. **But the dispatch above them is duplicated** (§2f): `close_artifacts` asks `answered_by_the_log(node.kind, …)`; `verdict` asks `spec.kind()`. Same answer today, one refactor away from the false confidence D146 exists to prevent.

**Which tools a node gets.** Two independent axes, both correct in principle:
- *Whether any:* the resolved runner's declared `Capability::RunTools` (`runner_resolve.rs:182`, `loop_exec/mod.rs:288`) — never an adapter name.
- *Which ones:* `catalog::mounted` (`catalog.rs:19`) reads only `SessionTools` fields — `declared` (the node's rendered `produces`), `task.is_some()`, `in_blackboard_group()`. Scoping by construction; no tool takes a run id.

**Frontera:** clean. No CLI name, flag or path appears in `run_tools/`; the endpoint crosses as `yunta_adapters::RunToolsEndpoint` (`listener.rs:17`) and the adapter translates it. One tension worth naming: `ArtifactKind::submit_tool()` / `POST_FINDING_TOOL` (`core/src/workflow/artifacts.rs:239-260`) put MCP transport vocabulary on a *workflow* type in `core`. It buys "Un lugar" at the cost of the workflow layer knowing the run-tools layer's names.

---

# 5. TASK CYCLE

**Ownership is clean.** `task_cycle::run_task` (`task_cycle/mod.rs:227`) decides — pre-check, attempts, post-check, scope — and emits **no** task-lifecycle events; it returns a `TaskCycleReport` :152. `loop_exec` owns all three verbs: dispatch (`loop_exec/dispatch.rs:72`), integrate (`integrate.rs:27`), escalate (`escalate.rs:24`). The split is stated at `task_cycle/mod.rs:134-138` and honoured.

**Duplicated session-request building — yes, and it has already diverged.**

`SessionRequest` is constructed field-by-field in two places, 14 fields each:

| Field | `run/prompt_exec.rs:205` | `task_cycle/attempt.rs:263` |
|---|---|---|
| `model` | `Some(chosen.model)` :210 | **`None`** :266 |
| `agent` | `chosen.agent` :211 | **`None`** :267 |
| `artifact_dir` | `node_exec::artifact_dir(…)` :219 | **`None`** :278 |
| rest | equivalent | equivalent |

Both loop and prompt nodes resolve a runner through `resolve_node_runner` (`runner_resolve.rs:16`) and emit `runner_resolved` naming `{adapter, model, agent}` (`RunnerCandidate` — `core/src/config/sections.rs:15`). But `SessionSetup` (`task_cycle/session.rs:24`) carries no model and no agent, so a `loop` node's task sessions run on **the CLI's default model**, silently: the adapters only pass `--model` when `req.model.is_some()` (`adapters/src/claude_code/mod.rs:117`, `codex/mod.rs:81`). The log says one thing; the session does another.

Second consequence of the same duplication: `artifact_dir: None` means `notice::files_to_write` returns `None` (`notice.rs:109`), so a `loop` node declaring an **opaque** artifact tells no session where to write it — while `close_artifacts` will still look for it in `scratch/staging/<loop-node>/` and fail the node. Silent, not explicit, degradation.

Third: `CapabilityDegraded{RunTools}` is emitted from three sites with three different prose bodies — `attempt.rs:225`, `prompt_exec.rs:183`, `task_cycle/session.rs:115` — plus a fourth policy string built in `open_run_tools`.

**A gate that exists on one door only.** `open_run_tools` refuses a node that declares an interpreted artifact when the adapter lacks `run_tools` (`runner_resolve.rs:192-198`, `TypedArtifactNeedsRunTools`) — "fails before a session opens rather than after one produced nothing". `loop_exec/mod.rs:286-315` re-implements the capability gate inline and **omits that refusal**: a loop node with `produces: [tasks]` on a capability-less adapter burns every session and fails at close.

---

# 6. STAGING / RUN DIR

`run_dir.rs` declares itself "the names the engine writes under it, **in one place**" (`run_dir.rs:1-2`). It holds `SCRATCH_DIR` :19, `STAGING_DIR` :22, `staging_root` :25, `staging` :36, `staged_path` :42, `open_staging` :71 with the `Opening` policy :62 — that part is exemplary. `ARTIFACTS_DIR` lives in `core/src/workflow/artifacts.rs:16` and `OBJECTS_DIR` in `artifacts/store.rs:26`, each with one consumer set.

**Everything else under the run dir is a literal somewhere else:**

| Path | Literal at | Should be |
|---|---|---|
| `scratch/sessions/` | `session_dir.rs:32` | `run_dir.rs` |
| `progress.md` | `run/node_close.rs:255` (and the message :256) | `run_dir.rs` |
| `task-worktrees/` | `run/loop_exec/dispatch.rs:91` | `run_dir.rs` |
| `manifest.yaml` | **15 sites**: `run/create.rs:189`, `workflow_exec/mounts.rs:167`, `workflow_exec/mod.rs:426`, and 12 in `crates/cli/src/` (`project.rs:40`, `commands/{stats,mcp,resolve_gate,resume,receipt,gc}.rs`, `commands/list/runs.rs:239`, `commands/status/mod.rs:46`) | one `run_dir::manifest_path()` |
| `"artifacts"` | `cli/src/surface/closing.rs:288` — literal instead of the exported `yunta_core::ARTIFACTS_DIR` | the constant |

---

# 7. DEFECTS

| # | Defect | Category | Evidence |
|---|---|---|---|
| D1 | Blackboard folds `finding_posted` by hand, twice, ignoring update and withdrawal — a withdrawn finding still shows in `yunta_get_blackboard` and in the group's consolidated output, contradicting §6.4 "Queda fuera de todo conteo, archivo y vista" and `findings.rs:1-8` ("un segundo pliegue es una segunda respuesta") | two doors / replay / doc-code | `run_tools/blackboard.rs:27-39`, `:70-79` vs `core/src/events/findings.rs:57,118` |
| D2 | A `loop` node's resolved runner model and agent never reach its task sessions; `runner_resolved` records a model the session does not run on | layering / two doors | `task_cycle/attempt.rs:266-267` vs `run/prompt_exec.rs:210-211`; `SessionSetup` — `task_cycle/session.rs:24-42` |
| D3 | `SessionRequest` built field-by-field in two places (14 fields), which is what let D2 and the `artifact_dir` gap open | two doors | `prompt_exec.rs:205`, `attempt.rs:263` |
| D4 | `TypedArtifactNeedsRunTools` refusal exists on the prompt door and not on the loop door | two doors | `runner_resolve.rs:192-198` vs `loop_exec/mod.rs:286-315` |
| D5 | `answered_by_the_log` re-derived in the run tools instead of called | two doors | `artifacts/mod.rs:97-105` vs `run_tools/submission.rs:67-76` |
| D6 | Tool names as literals in dispatch, inconsistently with the constants used two lines away | string in two places | `session.rs:171-179` vs `catalog.rs:39,61,100,109,128`; `notice.rs:119` |
| D7 | `"manifest.yaml"` in 15 places; `progress.md`, `task-worktrees`, `scratch/sessions` outside `run_dir.rs`; `"artifacts"` literal in the CLI | string in two places | §6 table |
| D8 | Opaque artifact names are a bare `String` with containment checked only by `yunta check`, **before** templating; `template_vars` injects user-supplied workflow inputs (`inputs.*`), so `produces: ["{{inputs.x}}"]` with `x=../../…` escapes the run dir after render and nothing re-checks | missing type / parse-is-validate | `workflow/artifacts.rs:42,49-57`; `check/declarations.rs:133-139` (comment :115-116 "templates in a name are checked as written"); `node_exec.rs:277-279,348-360`; `store.rs:147-153` pushes the name onto a `PathBuf` |
| D9 | `ANSWERS_SUFFIX` builds an engine-owned identity (`questions.answers.yaml`) that nothing reserves; `check_reserved_artifact_names` covers context sources, mounts and distill — **not `produces:`** — so a node can declare that exact name and collide with the engine's own acceptance under the same `(node, identity)` key | missing type | `artifacts/mod.rs:48,55-59`; `check/declarations.rs:213-253` |
| D10 | Promotion's inherited-findings artifact renders and accepts outside `canonical::derive_findings`, skipping `max_artifact_bytes` — a guard the run enforces everywhere else | two doors | `run/steps.rs:256-272` vs `canonical.rs:125,169-187` |
| D11 | `ArtifactOrigin::Legacy` is representable in a fresh `artifact_accepted` | missing type | `payloads.rs:769-773,908` |
| D12 | contrato §6.4 specifies `yunta_submit_*` as `{ name, document }` with `name` an enum of declared names; the code takes `document` only — and the same paragraph contradicts itself one line earlier ("con el documento como único argumento") | doc/code divergence | `docs/design/contrato-del-run.md` §6.4 vs `catalog.rs:162-178`, `submission.rs:102-112`; `docs/guide.md:158-164` and `docs/concepts.md:66-70` are correct |
| D13 | contrato §5.2 step 3: the task brief is "ruta al documento de tareas + task_id"; the code sends no path | doc/code divergence | `attempt.rs:244-247` |
| D14 | spec-ledger §3 rule 1 states two rules ("un `id` se repite, **o no cumple el patrón**"); only duplication is a published `Rule`. The pattern is enforced by the `TaskId` newtype as a parse `Problem`, so a writer meets it as a parse error rather than a published demand — the exact gap D143 exists to close | doc/code divergence | `docs/design/spec-ledger.md:57`; `core/src/tasks/rules.rs:29-69`; `core/src/ids.rs:403` |
| D15 | `pending_questions` returns `Vec<String>` though `QuestionId` exists; `unanswered` in the ask round is also `Vec<String>` mixing question ids and validation violations | missing type | `node_artifacts.rs:340-349`; `questions_exec.rs:69,74,85` |
| D16 | `FindingEntry.location: String` though the contract specifies "path y rango opcional" and the rule demands "a non-empty path, with its range when there is one" | missing type | `core/src/findings/mod.rs:40`; `core/src/findings/rules.rs:26-29`; contrato §4.1 |
| D17 | `VerifiedArtifact.content_hash` means the staged-file hash on the ingest path and the canonical hash on the submit path — one field, two meanings (documented, never wrong today, but a trap) | missing type | `ingest.rs:50-55,190` vs `canonical.rs:191` |
| D18 | `{{runner.role}}` is the author-facing template variable; CLAUDE.md's vocabulary table says `runner:` in place of `role:` in YAML and code | doc/code divergence | `node_exec.rs:264`; `template.rs:5`; `check/declarations.rs:116` |

---

# 8. IDEAL

### What is already right and must be kept

| Piece | Keep? | Why |
|---|---|---|
| **Hash store + view** (`store.rs`) | **Keep unchanged.** | `objects/<sha256>`, write-through-`scratch`-and-rename :73-86, read-verifies :103-109, `project` regenerates :120. The invariant "`artifacts/` is written and never read back" is *actually* upheld — no `read_dir` of it exists anywhere. This is the strongest thing in the subsystem. |
| **`ArtifactId`** (`payloads.rs:787`) | **Keep.** | Closed two-case identity, `From<&ArtifactSpec>` :834 as the one declaration→identity door, `view_name()` :825 as the one naming door. Kind-as-identity is what removes the file-name question entirely. |
| **`ArtifactLedger` fold** (`core/src/events/artifacts.rs`) | **Keep.** | Total `apply` :80, first-acceptance ordering :56, `latest`/`of_kind`/`by_producer`/`every`. Ten consumers, zero second folds. Replay is honoured exactly. |
| **`shape::read`** (`core/src/shape/mod.rs:71`) | **Keep.** | Parse-and-check fused in one function "because a caller that could get one without the other would eventually be written" :12-14; sealed trait :34; `EXAMPLE` + `RULES` with three tests holding the chain (:211,227,239,255). D143 is fully realized. |
| **Submission through tools** (`submission.rs`, `canonical.rs`) | **Keep the mechanism; fix the dispatch.** | Document-never-a-file, canonical re-render, refusal-costs-a-call, `artifact_submitted` recording refusals too — all correct. What needs fixing is D5: the session verdict must call `answered_by_the_log`, not re-derive it. |
| **Strict authored / tolerant persisted split** | **Keep.** | `FindingEntry` `deny_unknown_fields` (`findings/mod.rs:35`) vs `events::Finding`; stated at `findings/mod.rs:1-4` and `run_tools/findings.rs:108-114`. Textbook. |
| **`answered_by_the_log`** | **Keep the idea, promote it to a type.** | See below. |

### Greenfield model

**Types.**

1. `ArtifactName(String)` — a validated newtype: one relative path segment sequence, no `..`, no absolute, and *not* an engine-reserved identity. Constructed at the frontier, re-validated after templating. Kills D8 and D9 by making the escape unrepresentable instead of linted.
2. `ReservedIdentity` — the engine's own names (`<kind>.yaml`, `<kind>.answers.yaml`) enumerated in one place that both `ArtifactName`'s constructor and `check` consume. `ANSWERS_SUFFIX` today is a private constant nothing defends.
3. `Answer` as a fourth `ArtifactKind`, not an `Opaque` with a suffix. Today the answers document is engine-written YAML carried as opaque bytes (`artifacts/mod.rs:50-59`) — the engine both writes and refuses to read its own document. A kind gives it `shape::read`, a published contract, and removes the suffix-collision class entirely.
4. `RecordedOrigin` (what `accept` may write) widening into `ArtifactOrigin` (what a fold may produce), so `Legacy` is unrepresentable on the authored path (D11).
5. `StagedHash` vs `StoredHash`, or simply drop `content_hash` from `VerifiedArtifact` and let `accept`'s return be the only hash anyone names (D17).
6. `Location { path, range: Option<Range> }` for findings (D16); `Vec<QuestionId>` for pending questions (D15); a closed `TemplateVar` enum in place of `BTreeMap<String,String>` (D18 and the escape surface in D8).

**Doors.** Exactly one of each, and each named by a function that nothing may bypass:

| Door | Today | Ideal |
|---|---|---|
| declaration → identity | `From<&ArtifactSpec>` ✅ | same |
| bytes → run | `accept` ✅ | same |
| document → bytes | `canonical` / `derive_findings` | plus `steps.rs:256` routed through it (D10) |
| bytes → document | `shape::read` ✅ | same |
| "who answers for this artifact" | `answered_by_the_log` + a re-derivation | `enum Answerer { Log, Staging }` returned by one function, consumed by close, record and the session verdict — the *type* forbids a third reading (D5) |
| document → events | `tasks::register` ✅ / `FindingLedger` (bypassed) | `FindingLedger::effective*` the only source for every *set*; raw folds only for explicit history views (D1) |
| tool name → handler | literals in two files | one `enum RunTool` with `name()`, `describe()`, `schema()` and an exhaustive `match` in dispatch — catalog and dispatch become two folds over the same enum, and adding a tool cannot forget either (D6) |
| run-dir path | `run_dir.rs` + 18 literals elsewhere | every path a `run_dir::` function; `manifest.yaml` included (D7) |
| session request | two literal structs | one `SessionRequest::for(runner, node_or_task, setup)` builder taking the resolved `RunnerCandidate`, so model and agent cannot be dropped (D2, D3) |
| capability gate | `open_run_tools` + an inline copy | `open_run_tools` only (D4) |

**Folds.** Two, both pure, both in `core`, both already written: `ArtifactLedger` and `FindingLedger`. The work is not to build them — it is to close the last bypass (`blackboard.rs`) and to state in the type system that they are the only answer, e.g. by making `FindingPosted`'s payload reachable for *set* questions only through the ledger.
