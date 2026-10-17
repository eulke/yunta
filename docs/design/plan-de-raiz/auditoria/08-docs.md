I have what I need. Here is the audit.

---

# FRONT: Documentation ↔ Code Coherence, ADR Corpus, Declared Debt

Repo `/home/user/yunta` @ `80abe93`. Governing rules read from `/home/user/yunta/CLAUDE.md`: *"La documentación gana al código cuando difieren, salvo decisión registrada en contra. Su silencio es un paso 2"* (line 70) and the **Expresión** block (lines 95–108).

---

## 1. DEBT LEDGER

`docs/design/deuda-consciente.md` in full, each item checked against the code.

### 1.1 Open items (A-01 … A-12)

| Id | Debt (doc line) | Why it's debt | What would resolve it | Still true? — evidence |
|---|---|---|---|---|
| **A-01** | Snapshot format for long logs (`deuda-consciente.md:14-15`) | Replay optimization; never a source of truth (I2) | An ADR fixing snapshot format + a reader that falls back to full replay | **Yes.** `grep -i snapshot crates/storage/src/*.rs` → nothing. No snapshot code anywhere. `crates/engine/src/replay.rs:1` derives from the full slice. |
| **A-02** | Local notifications (`:17-19`) | Local command channel + subscribable events; team channels belong to the separate server project | An ADR for the local channel | **Yes.** No notification surface; `RunObserver` (`crates/cli/src/surface/mod.rs:11-16`) is in-process only. |
| **A-03** | Per-node network/filesystem permission profiles (`:21-22`) | Beyond ReadOnly/Edit/Full, as an extension of §6.1 | Design of the profile model | **Yes.** `crates/core/src/workflow/node.rs:296-305` — `NodePermissions` is exactly `ReadOnly | Edit | Full`. |
| **A-04** | `yunta serve` (`:24-27`) | Out of scope by D77; team capabilities are a separate project | Nothing — closed by decision | **Yes (as a closed non-goal).** `crates/cli/src/cli.rs:31-300` has no `Serve` variant; no `serve` feature in `Cargo.toml`. |
| **A-05** | Central pack registry (`:29-30`) | Search, ratings, `pack publish` | Registry design + metrics publication format | **Yes.** `PackAction` (`cli.rs:250-303`) = add/update/new/remove/list/audit. No `publish`. |
| **A-06** | Cryptographic signature of packs, receipts and the event chain (`:32-38`) | Hash chain gives integrity+order, never authorship | Detached signature over the chain head + lock hash; key management out of v1 | **Yes.** `yunta verify` (`crates/cli/src/commands/verify.rs`) recomputes hashes only; no signing anywhere. |
| **A-07** | Transitive pack dependencies (`:40-41`) | Deliberately out; no npm-style version graphs | An ADR bounding depth | **Yes.** `crates/cli/src/commands/pack.rs` resolves one ref, no dep graph. |
| **A-08** | Batch pipeline / incremental consumption between concurrent nodes (`:43-51`) | Overlapping producer/consumer pairs, engine-mediated | `kind: pipeline` or `until: upstream_done && queue_empty`; measurable trigger = `yunta stats` showing >30 % wall-clock in dependency wait | **Yes.** No `pipeline` node kind (`crates/core/src/workflow/node_kind.rs`). |
| **A-09** | `isolation: container` (`:53-57`) | Out of the schema (D63) for lack of design | Image/mount definition, run.dir boundary crossing, interaction with `permissions.network` and adapters | **Yes.** `crates/core/src/config/sections.rs:124-128` — `Isolation { Worktree, None }` only; `:119` says so in rustdoc. |
| **A-10** | Visual workflow builder (`:59-63`) | Discarded (D75); trigger = demonstrated non-technical demand | Web UI with YAML as source of truth | **Yes.** `yunta graph` is the only visualization (`crates/cli/src/graph.rs`). |
| **A-11** | Capturing `kind: executor` output for `node-output:` (`:65-72`) | Only `kind: bash` leaves output where `node-output:` reads it; an executor's stdout is parsed as `ExecutorOutput` and never materialized | Write the captured stdout with the same `write_node_output` bash uses | **Yes — confirmed exactly.** `write_node_output` is called only at `crates/engine/src/run/bash_exec.rs:59` and `crates/engine/src/run/node_exec.rs:140`. `crates/engine/src/run/executor_exec.rs:145-149` parses stdout into `ExecutorOutput` and discards the bytes. |
| **A-12** | Field-level JSON contract of `kind: executor` (`:74-80`) | The spec fixes shape (JSON in/out, exit code = verdict) and names no fields | Minimal input object, output object, its own `schema_version` | **Yes — and the code says so.** `crates/engine/src/run/executor_exec.rs:1-7`: *"naming those fields in the spec is registered debt A-12"*. Fields live only in `build_stdin` (`:30-41`) and `struct ExecutorOutput { summary }` (`:50-52`). |

### 1.2 Known risks (`:82-86`)

| Risk | Still true? |
|---|---|
| Dependence on CLI headless flags — mitigated by design (every flag lives in the adapter, validated in `probe()`) | **Yes**, and unverified live: `docs/design/status.md:7-9` says `codex`, `claude-code`, GitHub forge, `yunta mcp` and per-run MCP are *"construidos contra documentación y ejemplos reales, sin corrida en vivo"*. `crates/adapters/src/codex/mod.rs:222-225` says the mapping is *"read off the CLI's own source rather than confirmed live"*. |
| Name availability on crates.io (D03) | **Yes.** `docs/design/status.md:13-19` still lists publishing as blocked. |

### 1.3 Closed items (`:88-100`) — spot-checked, all still closed

D70 (event payload schema), D51 (command allow/denylist), D49 (parallel-node coordination), D50 (escalation gate), D47 (executor contract), D48 (no merging), D51+D72 (org policy), D71 (pack audit inventories), D65 (tasks-document parallelism). All present in code and unreopened.

### 1.4 Additions — pending/deferred elsewhere, **not in the debt ledger**

| Source | Item | Status in code |
|---|---|---|
| `status.md:7-9` | Live verification of `codex`, `claude-code`, GitHub forge, `yunta mcp`, per-run MCP, `pack add` against a remote host; protocol in `smoke-checklist.md` | Pending; checklist sections A–D |
| `status.md:13-19` | crates.io publish, Homebrew tap repo + `update-tap` in `release.yml`, `setup-yunta` composite action repo, public docs site, splitting factory packs into their own repos, first `vX.Y.Z` tag | Pending |
| `rfc-0002.md:§8` | *"Firma criptográfica de packs y recibos (**planificada en M14**…)"* | **Dangling reference.** No milestone map exists anywhere in the corpus; the only milestone names are D90's `M-0`/`M0–M7`. This duplicates A-06 under a different, undefined identifier. |
| `rfc-0003.md:§3` | *"Cuando el registry exista (**deuda ⑪**)"* | **Broken debt reference.** The ledger uses `A-01…A-12` and states the ids are *"estables: no se renumeran"* (`deuda-consciente.md:5-7`). `⑪` matches nothing. The item is A-05. |
| `rfc-0003.md:§2` | `yunta replay --at` / `yunta diff` — "Post-v1 temprano" (D55) | Not built; not in the ledger, not in `status.md`. `cli.rs` has no `Replay`/`Diff`. |
| `contrato-del-run.md:445-455` (§8.8) | OTel export, "post-v1"; `telemetry:` retired from the schema (D121) | Consistent — no `telemetry` key in `crates/core/src/config/sections.rs`; no exporter. |
| **Unregistered** | Baseline capture is lazy, not at run creation (see §3 below) | Contradicts D18 and Contract §7.2; **absent from the debt ledger and from `status.md`** |
| **Unregistered** | `edit_hooks` is `false` on both real adapters | Contradicts spec-adapter §6; **absent from the debt ledger** |
| **Unregistered** | Short-circuit ordering learns per-invocation, not from the log | Contradicts D62 and Contract §5.4; **absent from the debt ledger** |
| **Unregistered** | `kind: questions` cannot be answered by pull request | Contract §3/§4.1 promise it; `Channel` has no `Pr`. **Absent from the ledger** |

---

## 2. ADR HEALTH

### 2(a) Numbering, gaps, duplicates, mis-citations

**Numbering is clean.** 163 ADRs, `D01`…`D163`, **no duplicates, no gaps** (verified by extracting every `^\*\*D\d+` and diffing against `seq 1 163`).

**Cross-references.** 75 ADRs cite other ADRs; every cited number was checked against the cited ADR's title. **No mis-pointing reference found.** Spot-checked the least obvious ones and all are correct:

| Citation | Looks wrong, is right |
|---|---|
| D132 → D124 (`adrs.md:142`) | Deriving kind strings from JSON Schema at runtime would pull `schemars` into the shipped binary, *"contra el techo de tamaño de D124"* ✓ |
| D137 → D120, D121 (`:147`) | *"D120 y D121 fijan el par de salidas que tiene una clave inerte: se implementa o se retira"* — D120 implements, D121 retires ✓ |
| D139/D140/D144 → D124 (`:149,150,155`) | All about binary size of runtime schema generation ✓ |

**Numbering defects elsewhere in the corpus (not in adrs.md):**

- `docs/design/spec-adapter.md` — the obligations are numbered **O1, O2, O3, O5, O4, O5, O6**. `O5` is used **twice** (`:193` "El prompt viaja por stdin" and `:201` "`edit_constraints` es best-effort declarado") and `O4` sits after the first `O5`. `spec-events.md:113` cites "O2" and `contrato-del-run.md:354` cites "O2 de la Spec" — both unambiguous, but O5 is not resolvable.
- `rfc-0003.md:§3` cites "deuda ⑪"; `rfc-0002.md:§8` cites "M14". Neither identifier exists anywhere in the repo.

### 2(b) ADRs describing mechanisms that no longer exist

| ADR | What it says | What the code does | Category |
|---|---|---|---|
| **D132** (`adrs.md:142`) | `label()` — *"cómo se nombra a un lector: **\"task ledger\"**"* | `crates/core/src/workflow/artifacts.rs:210-214`: `ArtifactKind::Tasks => "tasks document"` | ADR stale + banned vocabulary preserved in the register |
| **D139** (`adrs.md:149`) | *"`cargo xtask schema` escribe los **siete** archivos ahí"* | `crates/core/src/schema.rs:57` — `pub fn all() -> [(&'static str, Schema); **8**]`; `ls crates/core/schemas/` = 8 files (`config, events, findings, pack, questions, tasks, withdrawal, workflow`) | ADR stale (count) |
| **D142** (`adrs.md:152`) | Ceiling `21546800` bytes, derived from the day's measurement | `.github/workflows/ci.yml:99` — `ceiling=33554432` | **Correctly** superseded: the ADR carries *"(Revisada por D155: el techo pasa a 32 MiB…)"* ✓ |
| **D18** (`adrs.md:23`) | *"**Snapshot al abrir el run**; `baseline_compare` como gate"* | `crates/engine/src/run/check_exec.rs:77-82`: *"capture is **lazy**, on this builtin's own first invocation in the run, rather than eagerly at worktree creation"*, and *"The first `baseline_compare` node always passes (it has nothing yet to compare against)"* | **Code diverges — doc wins.** No `baseline/` directory is created anywhere (`grep -rn '"baseline"' crates/engine/src` → nothing), though `contrato-del-run.md:15` lists it in the run.dir anatomy |
| **D62** (`adrs.md:71`) | *"Short-circuit con orden aprendido **del log**"* | `crates/engine/src/task_cycle/criteria.rs:29-36` — durations are `Mutex<HashMap<String, Vec<u64>>>`, *"this invocation only"*; nothing reads `criteria_checked.results[].duration_ms` back off the log | **Code diverges — doc wins** |
| **D19** (`adrs.md:24`) | *"fuentes propias vía executors"* (custom `ContextSource`s) | `crates/core/src/workflow/context.rs:18-45` — `ContextSpec` is a closed 8-variant enum. No extension point | **Code diverges — doc wins**; `contrato-del-run.md:466` repeats the claim |
| **D120** (`adrs.md:130`) | `defaults.on_failure: abort | continue` **are implemented** | **True** — `crates/engine/src/run/schedule.rs:453-455, 569-580`. But `crates/core/src/config/sections.rs:133-135, 148-149, 166-168` still says *"Only `pause` is built — `check` refuses the others"* | **Rustdoc stale, ADR correct** |

### 2(c) `Revisada` chains

22 revision notes exist. Every revision note names its reviser; I checked the reverse direction (does each revising ADR's target carry a note?) and found:

| Reviser | Targets with a note | Targets that arguably lack one |
|---|---|---|
| D157 | D11, D19, D82, D86, D106, D107, D108, D134, D146, D149, D156 | **D152** (`adrs.md:171`) — D157 cites D152 in its own body, but D152 carries no `Revisada por D157` note. D152 is *"El directorio de artifacts del run se audita al cierre"*, and D157 moved the audit to the store + `ArtifactLedger`; either the note is missing or the citation is informational. Worth resolving. |
| D156 | D86, D129, D146, D149 | — |
| D155 | D124, D142 | — |
| D159 | D157 | — |
| D162 | D45, D46 | — (fixed by `80abe93`, the HEAD commit) |
| D77 | D02, D05 (implicit "Revisada:") | — |
| D53 | D07 (implicit) | — |

**One structural weakness:** eight notes say only *"(Revisada: …)"* with no reviser number (D02, D03, D05, D07, D45, D46). D03's note — *"la restricción original — el pipeline no soportaba workspaces — fue levantada"* — names **no reviser at all**, so the decision that lifted it is unfindable. That is also an Expresión violation (it narrates what was before).

### 2(d) One giant file vs `docs/design/adr/`

`docs/design/adrs.md` is **184,453 bytes in 172 non-empty lines** — one ADR per *line*.

| Measure | Value |
|---|---|
| Average line length | 1,071 chars |
| D157 (`:182`) | **14,188 chars on one line** |
| D159 (`:186`) | 7,381 |
| D158 (`:184`) | 5,411 |
| D156 (`:180`) | 4,294 |
| Headings | 7, **all at `#` level** (including the document title) — no `##` hierarchy, so no per-ADR anchor exists |

Consequences, all observable:

- **No addressable anchor.** Nothing in the repo can link to `adrs.md#d157`; every reference is the bare token `D157`, resolvable only by full-text search.
- **Diffs are useless.** A one-word edit to D157 rewrites a 14 KB line; `git diff` shows the whole ADR as changed. The commit `80abe93` ("las revisiones de D45 y D46 citan D162") touched two 3 KB lines for a two-word edit.
- **Section grouping has broken down.** `# Seguridad y operación` (`:40`) holds **D33–D109 — 77 of 163 ADRs**, including the Apache-2.0 licence (D67), the distribution channels (D68), the monetization model (D69), `yunta test` (D89) and the bootstrap plan (D90). `# Frontera de datos, procesos y distribución` (`:119`) holds D110–D163, including the live run view (D162) and escalation evidence (D163). Neither heading predicts its contents.
- **The `adr/` directory is empty.** `docs/design/adr/` contains only `README.md` (5 lines) stating that proposed decisions live there and are folded into the register on acceptance. It has never held a file. The intended two-tier system exists on paper only.

**Assessment:** the register's *content* is exceptional — rationale, discarded alternatives, revision chains, near-perfect cross-referencing. Its *container* is the weakest artifact in the corpus. One file per ADR under `docs/design/adr/DNNN-slug.md`, with the register reduced to a generated index (number, title, status, revised-by), would preserve everything the corpus does well and fix diffability, anchoring and grouping at once. Section 9 details this.

---

## 3. CONTRACT vs CODE

`docs/design/contrato-del-run.md`, §1–§15 (I1…**I30**, not I2x). The strongest claims, spot-checked.

| # | Claim (contract line) | Code location | Verdict | Who wins |
|---|---|---|---|---|
| §3:69 | *"Los **36** tipos de evento (**30 filas**; varias agrupan variantes emparentadas)"* | `crates/core/src/events/mod.rs:282` `/// All 36 event kinds`; enum = 36 variants; `KINDS` = 36 strings (`:339-434`) | **Agrees exactly.** Table has exactly 30 `|`-rows and names exactly the 36 kinds, set-identical to `KINDS` (verified by script). `spec-events.md:15-18` corroborates the 25+4×2+1×3 decomposition | — |
| §3:90 | `questions_answered` channel `(tty\|mcp\|**pr**)` | `crates/core/src/events/payloads.rs:105-108` — `enum Channel { Tty, Mcp }` | **Diverges** | **Doc wins** (or the promise is retracted) |
| §4.1:144 | Questions *"respondible por consola, tool MCP o **pull request**"* | `crates/engine/src/run/questions_exec.rs` routes only through `HumanInteraction::ask`; `crates/cli/src/ask/form.rs:50` sets `Channel::Tty`. No forge path | **Diverges.** `spec-events.md:334` already says `tty \| mcp` — so the contract also contradicts its own spec | **Doc wins** |
| §3.2:113 | Node states `pending → ready → running → **done** \| failed \| skipped`, plus `waiting` | `crates/engine/src/replay.rs:34-60` — `NodeState { Running, **Finished**, Failed, Waiting }`; `skipped` is a render-side reading (`crates/engine/src/view/node.rs:87`, `crates/engine/src/run/schedule.rs:12`) | **Vocabulary diverges** (`done` vs `finished`). README:189 and concepts.md both say `finished` | Code vocabulary is the one users see; **contract should be corrected** |
| §3.3:117-129 | `event_hash` chain, genesis `H0 = SHA-256(manifest_hash)`, persisted not recomputed, verified in receipt + `yunta verify` | `crates/storage/src/store.rs`; `yunta verify` (`cli.rs:174-179`) | **Agrees** — `spec-events.md:77-80` adds the concrete length-prefixed encoding | — |
| §4:132 | `limits.max_artifact_bytes` guards artifact size | `crates/engine/src/run_tools/host.rs:35-37` | **Agrees** | — |
| §5.1:165 | Every task needs ≥1 `cmd` criterion; `manual_review: true` with justification | `crates/core/src/tasks/rules.rs:41-67` (`NoCriteria`, `AllCriteriaAreGuards`, `ManualReviewWithoutJustification`) | **Agrees** | — |
| §5.3:176-194 | Escalation object with `summary`, `evidence`, `options[{id,label,tradeoff}]`, **`free_text: true`**, **`default_on_timeout: none`** | `GateWaitingPayload { summary, evidence, options, external_ref }` (`payloads.rs`); `free_text` lives on the *resolution* (`HumanChoice.free_text`, `payloads.rs:572-576`); `default_on_timeout` **does not exist as a field** — `crates/engine/src/human_interaction.rs:37-40`: *"enforced by this trait having no timeout parameter at all"* | **Structurally diverges.** The behaviour is right; the contract presents two non-fields as fields of the escalation, and omits `external_ref` | Contract's *normative structure* needs correcting |
| §5.3:195 | *"El mismo objeto se renderiza en toda superficie… vía el trait `HumanInteraction`"* | `crates/engine/src/human_interaction.rs:42-65` | **Agrees** | — |
| §5.4:199-204 | Memo key = `comando + tree_hash + **env declarado** + versión de config`; ordering *"aprendido **del log**"*; *"la memoización vive **dentro del run**"* | `criteria.rs:67-69` — key is `cmd \x00 tree_hash \x00 config_hash`; `:24-25` *"`declared env` drops out here because criteria have no `env:` field in the schema yet"*; `:29-36` durations are per-invocation; `:16-21` cache is per `execute_run`, so a resume starts cold | **Three divergences**: a key component that doesn't exist, learned order that isn't from the log (D62), and "within the run" that is actually "within the invocation" | Doc wins on the order (D62); the invocation-scoped cache is the *correct* reading of CLAUDE.md's Replay rule — **the contract's wording should change** |
| §5.5:211-218 | `concurrency: N`, default 1; disjoint-scope batching; isolate to work, serialize to verify, rebase + re-run criteria at integration; guards once per batch | `crates/engine/src/run/loop_exec/mod.rs`, `integrate.rs` | **Agrees** | — |
| §5.7:236-244 | `done` survives re-plan only on identical `id`+`criteria`+`scope`; cross-run `done` only when the commit is an ancestor of HEAD; `done` is the only state that travels | `crates/engine/src/tasks/mod.rs`, `crates/engine/src/tasks/crossing.rs`; `TaskStatusChangedPayload.commit: Option<CommitSha>` (`payloads.rs`) | **Agrees** — D159/D158 land exactly this | — |
| §5.8:260 | Sibling write collision: **error** in check when both declare overlapping scope; **warning** when neither declares | Reproduced verbatim by the CLI: *"parallel group `build`: two or more children can write and don't declare scope as disjoint…"* (observed running `yunta check` on `referencia-schema.md`'s `release-cycle`) | **Agrees** (I24) | — |
| §6.4:322 | Control plane tools: *"`list_workflows`, `run_workflow`, `resume_run`, `resolve_gate`, `workflow_status`"* — **five** | `crates/cli/src/commands/mcp.rs:79-91` dispatches **six**, including `document_shape`; `tool_definitions()` (`:136-194`) advertises it first | **Diverges — contract silent on a built tool.** D129 created it; README:175 lists all six correctly. `cli.rs:141-145` and `mcp.rs:1-2` rustdoc also still say "Five tools" | **Doc must be updated** — its silence is a paso 2 |
| §6.4:328-338 | Per-run tools: `yunta_post_finding`, `_update_finding`, `_withdraw_finding`, `yunta_check_artifact`, `yunta_request_scope_expansion`, `yunta_task_status`, `yunta_get_blackboard`, + `yunta_submit_<kind>` | `crates/engine/src/run_tools/` (catalog, findings, tasks, blackboard, submission, verdicts) | **Agrees** | — |
| §6.4:338 | `yunta_submit_tasks`/`_questions` take **`{ name, document }`** | `crates/core/src/workflow/artifacts.rs:241-242` + D157: *"la tool pierde su argumento, `yunta_submit_tasks { document }`"*; glosario.md:108-109 says *"`document` como único argumento"*; §4.1:139 itself says *"con `document` —su único argumento—"* | **Contract contradicts itself two sections apart.** §4.1 is right, §6.4 is stale | §4.1 wins |
| §6.5:344-356 | Loopback HTTP, ephemeral port, bearer token, per-session lifetime, `(run_id, node_id, attempt)` scoped by construction | `SessionRequest.run_tools_endpoint: Option<RunToolsEndpoint>` (`crates/adapters/src/session.rs:81`) and its doc | **Agrees** (I27) | — |
| §7.1:372-374 | Three closed builtins: `baseline_compare`, `coverage_gate`, `findings_gate{max_severity}` | `crates/core/src/workflow/node_kind.rs:324-331` | **Agrees exactly** | — |
| §7.2:380 | *"**Al crear el run** (después del worktree), el engine ejecuta la suite declarada (`baseline.suite`), persiste resultados y hash"* | `crates/engine/src/run/check_exec.rs:77-82` — capture is **lazy**, on the first `baseline_compare` invocation; the first such node always passes | **Diverges** (with D18 behind the doc). The code's stated reason — *"capturing eagerly would make `create_run` async across its four call sites for a builtin most workflows never use"* — is an ergonomics trade nobody registered | **Doc wins**; and this needed a paso 2 |
| §2:15 | run.dir contains `baseline/` | No such directory is created anywhere | **Diverges** (same root cause) | **Doc wins** |
| §7.3:385-389 | `isolation: worktree \| none`, `inherit` only for sub-runs; `none` demands a clean tree | `crates/core/src/config/sections.rs:112-128` | **Agrees** | — |
| §8.1:392-397 | Resume verifies artifact bytes by rehash **and** worktree identity + ancestry; content of the worktree never checked | `crates/engine/src/artifacts/integrity.rs`, `crates/engine/src/worktree/integrity.rs` | **Agrees** — D157/D158 landed both halves | — |
| §8.1:399-403 | `on_interrupt: restart_node \| resume_session \| fail_if_uncertain` | `crates/core/src/config/sections.rs` (`OnInterrupt`) | **Agrees** (I23) | — |
| §8.5:414-416 | Progress = counters not percentages; two levels (flow/task); surfaces = `status`, live view of `run` (default on a terminal), MCP `workflow_status`; `--quiet` → run id line; no terminal → append-only one line per event, first line names the degradation | `crates/cli/src/surface/view.rs:239-248` (`nodes X/Y · tasks A/B · N reroutes`); `crates/cli/src/surface/mod.rs:52-58` (`NOT_A_TERMINAL`, `DUMB_TERMINAL`, `COLOR_REFUSED`, `ROW_TEMPLATE`); `cli.rs:62-66` | **Agrees** — D162 landed | — |
| §8.6:419-423 | Estimation informative, suppressed by `--quiet`; travels in `list_workflows`; **budget warning survives `--quiet`**; silence below 3 runs | `crates/engine/src/history.rs:95` `MIN_SAMPLES_FOR_ESTIMATION = 3`; `budget_p90_warning` (`:106-120`); `cli.rs:62-66` *"A diagnostic and the budget warning are printed either way"*; `crates/cli/src/commands/list/mod.rs:40` | **Agrees fully.** Only the illustrative wording differs (contract: *"budget 200k is below the p90 of past runs (520k); this run will likely pause"*; code: *"`limits.max_tokens_per_run` (X) is below this workflow's historical p90 (Y tokens over N run(s)) — the run may pause on its budget"*) | — |
| §8.7:427-443 | Verification-effectiveness signals, suggests-never-acts, never suggests dropping `invariant: true` | `crates/engine/src/verification_effectiveness.rs` | **Agrees** | — |
| §8.8:445-455 | `telemetry:` retired from the schema until the exporter exists | No `telemetry` field in `crates/core/src/config/sections.rs` | **Agrees** (D121) | — |
| §9:466 | Eight builtin sources: `files, command, artifact, mcp, run-events, tasks, knowledge, node-output`; *"Los equipos agregan fuentes propias como executors"* | `crates/core/src/workflow/context.rs:18-45` — exactly those 8, as a **closed enum with no extension point** | **List agrees; extensibility claim diverges** (same as D19) | **Doc wins** |
| §9:469 | Effective content of every source materialized under `objects/<hash>` (I30) | `crates/engine/src/artifacts/store.rs:26` `OBJECTS_DIR` | **Agrees** | — |
| §2.3:63 | Input types `string, number, boolean, enum, path, document` | `crates/core/src/inputs.rs:24-84` — exactly six | **Agrees** | — |
| §10.1:512-514 | Modes are an open ordered map; `invariant: true` in all; mode-internal coherence is a `check` **error** | `crates/engine/src/modes.rs`, `crates/engine/src/check/modes.rs:51-58`, `crates/engine/src/check/error.rs:271` | **Agrees** (D76, D125) | — |
| §11.1/§11.2 | Hooks `before`/`after`, order context → before → session → after → verification; re-route edges with `max_reroutes` | `crates/core/src/workflow/hooks.rs`, `crates/engine/src/run/schedule.rs:440-470` | **Agrees** (I13, I14) | — |
| §12:565 | A `kind: workflow` node acquires what it declares from *"el **ledger** del hijo"* | `crates/engine/src/run/node_artifacts.rs:117-133` | Behaviour agrees; **the contract uses the banned word for the child's artifact set** — though glosario.md:100-105 licenses "ledger" for a log fold, so this is borderline | — |
| §13 | Adapter/runner/agent three levels; `runner:` never `role:` | `crates/core/src/workflow/node.rs`; only fixture `crates/cli/tests/fixtures/typo-keys.yaml:12` uses `role:`, as a negative test | **Agrees** | — |
| §14:617-634 | Test case format: `workflow, mode, inputs, fixture, expect{final_state, nodes{reroutes,runs}, tasks, events, never}` | `crates/cli/src/commands/test.rs` (D128) | **Agrees** | — |
| **Markup** | §2:9 renders as `Materialización:\`javascript`; the fence at `:19` is unterminated and the file ends with a stray ` ``` ` at `:673`. Throughout: `\{\{`, `\[`, `\]`, `\|`, and `[progress.md](http://progress.md)`, `[CONTEXT.md](http://CONTEXT.md)`, `[middleware.rs](http://middleware.rs)` auto-links | — | **Corrupt export artifact.** Every YAML example in the normative contract is un-copyable and un-parseable. Same corruption in `rfc-0001.md:§8,§9` (```javascript on a directory tree) and `rfc-0002.md:§4` | — |

**Invariants I1…I30** — all thirty are supported by code except the three already named: **I30** is satisfied; **I9** (explicit degradation) is satisfied; the weak points are I5/I7-adjacent (baseline laziness), and the questions-by-PR promise that no invariant covers.

---

## 4. SPECS vs CODE

### 4.1 `spec-events.md` vs `crates/core/src/events/payloads.rs`

Field-by-field. Every payload struct was extracted and diffed against the spec's §5.x tables.

| § | Kind | Divergence |
|---|---|---|
| 5.5 | `agent_session_opened` | Spec: `model \| string \| **sí**`. Code: `model: Option<ModelName>` (optional). `AgentEvent::SessionOpened` in `spec-adapter.md:159` itself says *"ausente cuando no reporta ninguno"* — **the two specs contradict each other** |
| 5.11 | `task_status_changed` | Spec lists `task_id, new_status, caused_by`. Code has a **fourth field `commit: Option<CommitSha>`** (D159, and `contrato-del-run.md:82` documents it). **Spec table is missing it** |
| 5.11 | `task_status_changed` | Spec note: `new_status … [**inferido, valores exactos a confirmar contra la implementación del scheduler**]`. `payloads.rs:62-69` has settled it: `Pending, Ready, Running, Done, Blocked, Failed`. **Stale inference marker** |
| 5.14 | `scope_expansion_granted` / `_denied` | One shared table. Code splits them: `Granted { task_id, decided_by, mode, count_this_run, **paths** }`, `Denied { …, denial_reason }`. **`paths` on `granted` is undocumented** |
| 5.18 | `gate_waiting` | Code has `external_ref: Option<String>` (the PR URL). **Not in the spec table** |
| 5.18 | `gate_resolved` | Spec shows a flat `{chosen_option, resolved_by, free_text}`. Code is an **enum of five shapes** — `Chosen(HumanChoice) \| Approved{by, sha} \| ChangesRequested{by} \| Closed \| Unrecognized` (`payloads.rs:548-565`) over a four-field wire struct `GateResolvedWire{chosen_option, resolved_by, free_text, **sha**}` (`:587-600`). **The `sha` field and the whole shape model are missing from the spec** — and §5.18 is the payload the run contract's external-gate evidence (§5.6:232) rests on |
| 5.19 | `questions_answered` | Spec `channel: tty \| mcp` ✓ matches code — but **contradicts `contrato-del-run.md:90`** (`tty\|mcp\|pr`) |
| 5.1 | `run_created` | `yunta_schema`, `base_branch`/`base_commit` still flagged `[inferido]`; all three exist in `RunCreatedPayload`. **Stale markers** |
| 5.6 | `agent_message` | Every field still flagged `[inferido]`; all seven exist verbatim in `AgentMessagePayload`. **Stale markers** |
| — | **Everything else** | 30 of 36 payloads match name-for-name and optionality-for-optionality. `finding_*`, `artifact_submitted`, `artifact_accepted` (§5.21.4/5.21.5) are exceptionally precise, including the `artifact`-not-`kind` naming rationale |

**Framing defects in `spec-events.md`:**

- `:5` *"**Precede** a los tipos de Rust"* — the types exist. Future tense for a completed thing.
- `:115-116` *"Si esta lectura no es la intención original, es exactamente el tipo de cosa a corregir con **una nota tuya** antes de que se convierta en tipos de Rust."* — addressed to a person in a past conversation, about work already done. Violates **Presente** and **Autocontenido**.
- `:75` heading *"(spec, no implementación)"* immediately followed by *"> Implementado: …"*.
- `:98` *"la firma criptográfica (deuda consciente, **sin ADR de diseño todavía**)"*.

### 4.2 `spec-adapter.md` vs the `Adapter` trait

| Item | Spec | Code | Verdict |
|---|---|---|---|
| **`Capabilities`** | **6 fields** (`spec-adapter.md:35-55`): resume_session, edit_hooks, permission_profiles, custom_agents, usage_reporting, run_tools | **8** (`crates/core/src/capabilities.rs:19-44`, `Capability` enum `:54-63`): adds **`skills`** and **`network_isolation`** | **Spec 2 short.** `network_isolation` is D119's; `skills` has no ADR at all. `spec-events.md:171` repeats the 6-field list |
| **Degradation table** | §5:215-223, 7 rows | No row for `skills` — but `crates/adapters/src/codex/mod.rs:206-210`: *"the engine degrades with `capability_degraded` when a node declares skills here"*. No row for `network_isolation` — but D119 mandates the event | **Spec incomplete** |
| **`SessionRequest`** | 12 fields including **`context: ResolvedContext`** (`:64`) | 13 fields, **no `context`** — plus **`artifact_dir`** (D149) and **`scratch_dir`** (D153), neither in the spec | **Diverges both directions** |
| **`AgentSession`** | 3 methods: `events`, `interrupt`, `kill` | 4 — adds `fn pgid(&self) -> Option<Pid>` (`crates/adapters/src/session.rs:302-304`) | **Spec 1 short** |
| **`Adapter::id`** | `-> &'static str` | `-> &'static AdapterId` (newtype) | Cosmetic, but the spec shows the un-newtyped form CLAUDE.md forbids |
| **§6 `claude-code`** | *"declara **todas** las capacidades… **`edit_hooks` se implementa** instalando hooks de pre-edición"* | `crates/adapters/src/claude_code/mod.rs:208` — **`edit_hooks: false`**; `:219` **`network_isolation: false`** | **Flatly false.** `docs/guide.md:78-81` is correct (*"no adapter today blocks an out-of-scope write while it happens"*), so the user doc and the normative spec contradict each other |
| **§6 `codex`** | *"`resume_session: false` salvo que `probe()` detecte soporte"* | `crates/adapters/src/codex/mod.rs:205` — **`resume_session: true`** unconditionally; also `custom_agents: false` and `skills: false`, neither mentioned | **Diverges** |
| **Obligations** | O1, O2, O3, **O5**, O4, **O5**, O6 | — | **Duplicate/out-of-order numbering** |
| **Invariants A1–A8** | — | All hold | ✓ |

`crates/adapters/src/session.rs:1-10` — the module rustdoc is **self-contradicting and wrong on three counts**:

```
//! `SessionRequest` drops `context: ResolvedContext` — the engine
//! inlines/references context in the prompt — and
//! `run_tools_endpoint: Option<Endpoint>` isn't wired for MCP yet.
//! ... even where nothing populates a field yet (`env`, `budget`, `adapter_settings`)
```

`run_tools_endpoint` **is** wired (D103/D147; the field's own doc 60 lines below at `:73-81` describes the live listener and token), and `env`, `budget` and `adapter_settings` are all populated at `crates/engine/src/run/prompt_exec.rs:211-214`. The whole paragraph is written as a **diff against the spec** rather than a description of the type — a changelog where a third-party reader needs a description.

### 4.3 `spec-ledger.md` vs the tasks document

| Item | Spec | Code | Verdict |
|---|---|---|---|
| Top-level key, fields, `criteria[]` | `:11-48` | `crates/core/src/tasks/shape.yaml`, `crates/core/src/tasks/mod.rs` | **Agrees** |
| **Validation rules** | §3: **seven** numbered items; `:71` *"ninguna de **estas siete**"* | `crates/core/src/tasks/rules.rs:29-68` — **nine** `RuleCode`s: DuplicateId, EmptyTitle, EmptyScope, NoCriteria, AllCriteriaAreGuards, UnknownDependency, DependencyCycle, OverlappingScope, ManualReviewWithoutJustification | **Diverges.** The spec merges NoCriteria+AllCriteriaAreGuards into item 5 and folds EmptyTitle+EmptyScope into item 7 ("un campo obligatorio falta o está vacío"). D143 says *"Una regla se enuncia donde se aplica, y desde ahí se publica"* — nine published demands against seven documented ones |
| Rule 1 *"o no cumple el patrón"* | §3.1 | The pattern is enforced by the `TaskId` newtype (`crates/core/src/ids.rs:403`, `NAME_RULE`/`is_name`), i.e. at **shape**, not as a rule | **Layer misattribution** |
| Error block format | §4:84-88 | `crates/core/src/text.rs:104` (*"The block `spec-ledger.md` §4 fixes"*) | **Agrees**, and is explicitly pinned to the spec by name |
| §5 example | `scope: ["crates/engine/src/context/**"]` | That directory does not exist (context lives at `crates/engine/src/run/context_resolve/`) | Illustrative, but stale against the repo it claims to be from |
| Framing | `:4-7` *"**Se escribe antes del código** que lo parsea"* | The parser exists | **Future tense for a completed thing** |

### 4.4 `referencia-schema.md` vs `crates/core/schemas`

`docs/design/README.md:7` states: *"config y workflows canónicos; **los fixtures de parseo del workspace salen de acá**."*

**The canonical config does not parse.** Copied verbatim into a project and run through the binary:

```
error: failed to parse config layer `.yunta/config.yaml`:
  `limits.max_tokens_per_run`: invalid type: string "2_000_000", expected u64 at line 76 column 23
```

Three keys use YAML-1.2-invalid underscore separators — `max_tokens_per_run: 2_000_000`, `max_artifact_bytes: 50_000_000`, `inline_context_bytes: 32_000` (`referencia-schema.md:82,86,87`) — the exact failure `crates/core/src/config/sections.rs:193-196` warns about: *"Canonical integer form is `2000000` — the YAML parser (YAML 1.2) resolves `2_000_000` as a string, which fails the parse loudly."*

With the underscores removed, everything else in the file is correct: `build-feature.yaml` → `yunta check: OK`; `release-cycle.yaml` fails only on two `use:` targets absent from the isolated fixture dir (expected). The `permissions`, `paths`, `pricing`, `skills.executors` and `mcp_servers` blocks all parse.

Schema files: **8** on disk, matching `crates/core/src/schema.rs:57`. `D139` says seven (§2b above). The eight are `config, events, findings, pack, questions, tasks, withdrawal, workflow`.

---

## 5. USER DOCS vs BEHAVIOUR

### 5.1 README command table vs `crates/cli/src/cli.rs`

The set of subcommands is mechanically pinned (`docs_sync.rs`) and **passes**. Flag-level and behaviour-level claims are not pinned; checked by hand:

| README row | Claim | Code | Verdict |
|---|---|---|---|
| `:167` | `yunta graph <workflow\|run_id>` | `cli.rs:156-165` — positional is **always** a workflow path; a run id goes to `--run`. Confirmed via `yunta graph --help`: `Usage: yunta graph [OPTIONS] <WORKFLOW>` | **Doc wrong.** Also omits `--format` from the signature while the prose mentions DOT |
| `:161` | `run` flags `--input --adapter --fixture --mode --quiet --detach --json` | `cli.rs:41-80` — exact match, no `--follow` (D162) | ✓ |
| `:161` | `--quiet` keeps the budget warning | `cli.rs:62-66` | ✓ |
| `:161` | No terminal → append-only lines, first line says why | `crates/cli/src/surface/mod.rs:52-58` | ✓ |
| `:163` | `status` shows the parked decision + the `resolve-gate` command | `crates/cli/src/commands/status/decision.rs`, `crates/cli/src/commands/advice.rs` | ✓ |
| `:164` | `resume` reports exactly as `run` does | `cli.rs:91-102`, both route through `commands/drive.rs` | ✓ |
| `:170` | `verify` reports the two guarantees apart | `commands/verify.rs:29-63` | ✓ |
| `:175` | `yunta mcp` serves **six** tools including `document_shape` | `commands/mcp.rs:79-91` | ✓ **README is the only doc that gets this right** |
| `:189` | `pending → ready → running → finished \| failed \| skipped \| waiting` | `replay.rs:34-60` + `view/node.rs:87` | ✓ (and diverges from the contract's `done`) |
| — | No `promote`, `replay`, `diff`, `serve` claimed | ✓ none exist | ✓ |

### 5.2 `docs/guide.md`, `concepts.md`, `adapters.md`, `packs.md`, `troubleshooting.md`, `compatibility.md`

These are the strongest documents in the repo. Every YAML block parses and every workflow passes `yunta check` (pinned by `docs_sync.rs`). Behaviour claims checked:

| Doc | Claim | Verdict |
|---|---|---|
| `guide.md:78-81` | *"That diff is the only thing enforcing scope: no adapter **today** blocks an out-of-scope write while it happens"* | **Correct** (`edit_hooks: false` on both real adapters) — **and contradicts `spec-adapter.md:227-232`**. "today" is a time marker |
| `guide.md:36-41` | `check` builtins are the three named | ✓ |
| `guide.md:28,325` | Links the tasks schema as `design/spec-ledger.md` | Resolves ✓, but sends an English-doc reader into the Spanish corpus, at a file whose *name* is the banned word |
| `concepts.md:101-103` | *"`waiting` means a `gate` is asking a person"* | **Incomplete**: `replay.rs:52-56` — `Waiting` also covers a `kind: questions` artifact with no `questions_answered` |
| `adapters.md:57-60` | `codex` reads `sandbox`; `claude-code` reads none; `budget.max_turns` → `--max-turns` on claude-code, no turn cap on codex | ✓ |
| `compatibility.md:188-193` | `task-ledger` reads as `tasks` in logs, manifests, **workflows** and the `ledger: {}` context source | **Behaviour confirmed** (`crates/core/src/workflow/artifacts.rs:194` `#[serde(alias = "task-ledger")]`) — see §6 for why this is a defect |
| `compatibility.md:393-395` | `yunta schema task-ledger` still answers, as an alias | Confirmed (`crates/core/tests/artifacts.rs:131-134`) |
| `troubleshooting.md:205-266` | `resume` broken-by-worktree and missing-checkout entries | ✓ matches `crates/engine/src/worktree/integrity.rs` and D158's promise that troubleshooting *"gana la entrada del caso"* |

### 5.3 Tense / plan language — grep results

**English user docs: essentially clean.** The only hits are legitimate (`compatibility.md:18` "used to accept" describes a semver policy; the `no longer` hits in `troubleshooting.md:174-209` describe run state, not project history).

**Rustdoc and comments — 13 real violations** of *"ningún texto del diff nombra la tarea, un plan, lo que había antes ni lo que vendrá"*:

| Location | Text | Kind |
|---|---|---|
| `crates/adapters/src/session.rs:3-10` | *"`SessionRequest` **drops** `context: ResolvedContext` … **isn't wired** for MCP **yet** … **nothing populates a field yet**"* | Diff-against-spec + wrong on all three |
| `crates/core/src/config/sections.rs:133-135, 148-149, 166-168` | *"Only `pause` (**today's behavior**) is built; `check` refuses the rest"* | Stale (D120 is built) + time marker |
| `crates/engine/src/check/declarations.rs:9` | *"Every `defaults.on_failure` value is **now** built, so none is refused here"* | Names what came before |
| `crates/core/src/config/sections.rs:119` | *"`container` isn't a schema value at all (**not yet designed**)"* | Plan (A-09 is registered; phrasing should state a limit) |
| `crates/core/src/config/sections.rs:286` | *"Closed at `binary` **today** — `wasm` is reserved as a **future** additive variant"* | Time marker + plan |
| `crates/engine/src/process_registry.rs:3` | *"a **future** `--detach` supervisor"* | **`--detach` exists** (`cli.rs:68-73`) |
| `crates/cli/src/commands/mcp.rs:1-2` | *"**Five tools** — list_workflows, run_workflow, workflow_status, resume_run, resolve_gate"* | Stale count; its own dispatch (`:86`) handles six |
| `crates/cli/src/cli.rs:141-145` | Same five-tool list in `--help` | Stale, and **user-visible** |
| `crates/engine/src/replay.rs:1-2, 7-8` | *"covers what the event schema can **currently** produce" … "It tracks what the schema can actually produce **today**"* | Time markers implying incompleteness |
| `crates/adapters/src/lib.rs:4` | *"`claude-code` and `codex` **are built**."* | Milestone framing |
| `crates/engine/src/lib.rs:6` | *"This crate **anchors the workspace dependency graph** …, **keeping it compiling and testable**"* | Bootstrap-plan framing |
| `crates/cli/src/project.rs:46` | *"falls back to this project's current root, **the old behavior**"* | Names what came before |
| `crates/engine/src/run/create.rs:113` | *"exactly the behavior **before modes existed**"* | Names what came before |
| `crates/engine/src/task_cycle/criteria.rs:25` | *"criteria have no `env:` field in the schema **yet** (nothing to declare **yet**)"* | Plan |
| `crates/engine/src/worktree/mod.rs:75` | *"would break **the old contract** silently"* | Names what came before |

**Spanish design corpus — 5 real violations** (`deuda-consciente.md` and `smoke-checklist.md` are excluded: forward-looking is their licensed register):

| Location | Text |
|---|---|
| `contrato-del-run.md:258` | *"formaliza el comportamiento que **antes era implícito**"* |
| `contrato-del-run.md:220` | *"el sustrato multi-persona que el engine **no provee en v1**"* |
| `contrato-del-run.md:445, 447` | *"(OpenTelemetry, **post-v1**)"*; *"Lo que sigue es el diseño que esa clave **gobernará cuando llegue**"* |
| `spec-events.md:5, 115-116` | *"**Precede** a los tipos de Rust"*; *"es exactamente el tipo de cosa a corregir con **una nota tuya antes de que se convierta en tipos de Rust**"* |
| `spec-ledger.md:4` | *"**Se escribe antes del código** que lo parsea"* |
| `adrs.md:7` (D03) | *"la restricción original — el pipeline no soportaba workspaces — **fue levantada**"* — and names no reviser |

---

## 6. GLOSSARY

`docs/design/glosario.md` defines **29 terms** across six sections. It is the best-maintained document in the corpus: every definition carries an `_Evitar_:` list, and every term I sampled maps 1:1 onto a type or function.

### 6.1 Terms defined, and whether the code honours them

| Term (line) | Definition | Code | ✓ |
|---|---|---|---|
| YAML de autor / de agente / persistido (`:14-33`) | The three authorship frontiers | `crates/core/src/shape/mod.rs`, D110 strict-key parsing, D70 tolerant reading | ✓ |
| Artifact opaco / interpretado (`:37-47`) | — | `ArtifactSpec::{Opaque, Interpreted}` (`crates/core/src/workflow/artifacts.rs`) | ✓ |
| **Identidad de artifact** (`:49-57`) | kind for interpreted, name for opaque; `(nodo, identidad)` | `ArtifactId::{Interpreted{kind}, Opaque{name}}` (`payloads.rs:809-814`) | ✓ |
| **Objeto** (`:59-65`) | bytes under `objects/<sha256>`, rehashed on read | `crates/engine/src/artifacts/store.rs:26,71` | ✓ |
| **Vista** (`:67-73`) | `artifacts/` projected from the store; no engine reader opens it | `ArtifactId::view_name()` | ✓ |
| **Staging** (`:75-81`) | `scratch/staging/<node_id>/`, of the session not the attempt | `SessionRequest.artifact_dir` (`session.rs:92`) | ✓ |
| Kind de artifact (`:83-88`) | One type across all four doors (D132) | `ArtifactKind` (`artifacts.rs:188-198`) | ✓ |
| **Tasks** (`:90-98`) | *"_Evitar_: **ledger, task-ledger**, plan"* | See 6.3 — **violated in 12 places** | ✗ |
| **Ledger** (`:100-105`) | *"Un pliegue del event log … **_Evitar_: usar la palabra para el documento de tareas**"* | `FindingLedger`, `ArtifactLedger`, `GrantLedger` are legitimate folds ✓; the tasks-document uses are not | partial |
| Entrega / Posteo (`:107-119`) | `yunta_submit_<kind>` / `yunta_post_finding` | `crates/engine/src/run_tools/submission.rs`, `findings.rs` | ✓ |
| Documento derivado (`:121-126`) | `findings` of a `prompt`/`loop` derived at close | `crates/engine/src/run/node_artifacts.rs:59` | ✓ |
| **Conjunto efectivo** (`:128-134`) | Last state per `(nodo, id)`, minus withdrawn | `events::findings::FindingLedger::effective()` | ✓ |
| Documento / Forma publicada / Regla (`:138-158`) | D136, D129, D135 | `crates/core/src/shape/mod.rs:55,162-175`, `tasks/rules.rs:29` | ✓ |
| Falla de nodo / de artifact (`:162-175`) | *"Hay **cuatro y solo cuatro**"* | `ArtifactFailure::{File, Content, Unheld, Undelivered}` (D157) | ✓ |
| Reporte / Diagnóstico / Sujeto / Bloque de problemas (`:177-201`) | D130, D133, D137 | `crates/core/src/diagnostic/mod.rs`, `crates/core/src/text.rs:104` | ✓ |
| Rechazo (`:203-207`) | Costs a call, not a session | `run_tools/verdicts.rs` | ✓ |
| Regla / Exigencia / Contrato / Cobertura (`:211-228`) | D143, D144 | `shape::contract(kind)`, `RULES` | ✓ |
| Verificación en sesión (`:230-235`) | `yunta_check_artifact`, consultative | D146 | ✓ |

### 6.2 Terms used in code/docs **without** a glossary entry

The brief named several; here is what the repo actually uses undefined:

| Term | Where used | Defined? |
|---|---|---|
| **frame** | `RunFrame`, `NodeFrame` (`crates/engine/src/view/mod.rs`, `crates/cli/src/surface/view.rs`) — the central type of the whole observation surface, and the one every surface presents | **No.** Not in `glosario.md`, not in the contract. §8.5 talks about "progreso" and "superficies" but never names the frame |
| **standing** | `NodeStanding::{Skipped, ToGo, Reached}` (`crates/engine/src/view/node.rs:87,182`) — a distinct concept from `NodeState` | **No** |
| **escalation** | Contract §5.3, `GateWaitingPayload`, `crates/cli/src/render/escalation.rs`, D163, I11, I14, I21 — used across the whole corpus | **No glossary entry.** Defined only structurally, in §5.3 |
| **offer / tradeoff** | `GateOption.tradeoff` (mandatory per D50) | **No** — `tradeoff` appears in §5.3 and in D50 but has no definition |
| **demand line** | `crates/cli/src/commands/advice.rs:4-7` — a named surface element | **No** |
| **crónica** | — | **Not used anywhere.** Dead term from the brief |
| **view** | ✓ defined (`glosario.md:67-73`) | ✓ |
| **staging** | ✓ defined (`:75-81`) | ✓ |
| **runner / agente / adapter / pack / executor** | ✓ D27, D28, D38, D87; glossary defers the substitution table to CLAUDE.md (`:8-10`) | ✓ |

`glosario.md:4-6` sets the bar itself: *"Un término entra acá cuando el corpus ya lo apoya en más de un lugar."* **frame**, **standing** and **escalation** each clear that bar and are missing.

### 6.3 Banned vocabulary still present

`CLAUDE.md:126-142` bans: `ledger`/`task-ledger` → `tasks`; `plugin` → `pack`/`executor`; `role:` → `runner:`; `driver`/`backend` → `adapter`; `subagente`/`persona` → `agente`.

**`ledger` used for the tasks document — the banned sense — 12 occurrences:**

| Location | Text |
|---|---|
| `crates/engine/src/view/mod.rs:46` | *"**ledger's tasks**"* |
| `crates/engine/src/view/mod.rs:78` | *"a mode narrows the graph, never a **ledger**"* |
| `crates/engine/src/view/mod.rs:100-101` | *"The **ledger's tasks** … a run with no **ledger** reports no **ledger** instead of `0/0`"* |
| `crates/engine/src/view/mod.rs:248-249` | *"The **ledger's** counter … a run with no **ledger** reports no **ledger**"* |
| `crates/engine/src/view/node.rs:62, 190` | *"The **ledger tasks** this node has in `running`"* |
| `crates/cli/src/surface/view.rs:230` | *"the **ledger's tasks**"* |
| `crates/cli/src/commands/status/progress.rs:46` | *"task (**ledger tasks** …)"* |
| **`crates/cli/src/ask/decision.rs:91`** | **`tradeoff: "Unblocks now; one more task on the ledger"` — a user-facing gate-menu string** |
| `crates/engine/tests/view.rs:326, 338, 343` | Test names: `tasks_are_absent_until_a_ledger_registers_one`, *"a run with no ledger reports no ledger"* |
| `crates/engine/tests/live_derivation.rs:3` | *"the node a **ledger task** belongs to"* |
| **`docs/design/adrs.md:142` (D132)** | **`label()` … "task ledger"** — and the code says `"tasks document"` |
| `docs/design/contrato-del-run.md:565` | *"esa identidad se busca en el **ledger** del hijo"* |

**`task-ledger` accepted as author-YAML input — an unregistered decision:**

`crates/core/src/workflow/artifacts.rs:194` — `#[serde(alias = "task-ledger")]` on `ArtifactKind`. The comment at `:189-193` explains it holds *"at all [doors] at once: a workflow, a frozen manifest or an event log."*

Two problems:
1. **D110** says *"Claves desconocidas: **rechazo en todo YAML de autor**, tolerancia **solo en lo persistido**"*, and CLAUDE.md restates it: *"La tolerancia vive solo en lo persistido y versionado."* A workflow's `kind: task-ledger` and the CLI argument `yunta schema task-ledger` (`crates/core/tests/artifacts.rs:131-134`) are author surfaces. The alias makes the retired, banned spelling **permanently valid input** on both.
2. **`grep -c 'task-ledger' docs/design/adrs.md` → 0.** No ADR registers this. It is documented only in `compatibility.md:188-193, 393-395` — a user doc, not the decision register. Per CLAUDE.md's *Levantar*, this is a decision someone took alone.

**File name:** `docs/design/spec-ledger.md` is the tasks-document spec. Its own title is *"Spec — Schema del documento de tareas"*. Referenced by `docs/design/README.md:5`, `docs/guide.md:28,325`, `crates/core/src/text.rs:104`, `crates/core/src/diagnostic/mod.rs`. The banned word is in the path every reference must spell.

**Other banned terms — all clean or defensible:**

| Term | Findings |
|---|---|
| `plugin` | Only in `CLAUDE.md:138-139` (the ban itself), `rfc-0002.md:9` and `D38` (`adrs.md:46`) — both explaining why it is banned. **`adrs.md:10` (D06)** says *"Estado del **plugin** bajo un único raíz `.yunta/`"* — a survival from before D38 renamed the concept. **1 real occurrence** |
| `role:` | Zero in production code/YAML/JSON. Present only as a negative-test fixture (`crates/cli/tests/fixtures/typo-keys.yaml:12`) and a rejection assertion (`crates/core/tests/strict_keys.rs:49`) — both correct uses. One Rust parameter name: `fn config_with_runner(role: &str, …)` (`crates/engine/tests/check.rs:189`) |
| `driver` | `crates/engine/src/run/exec.rs:1` *"The run execution **driver**"*; `crates/engine/src/run/promote.rs:7` *"both **drivers** of a chain"*; `crates/engine/tests/modes.rs:90` *"The one run **driver**"*. Not the adapter sense, but literally the banned word in rustdoc |
| `backend` | `crates/storage/src/lib.rs:1,3`, `error.rs:5-6`, `store.rs:110`, `crates/core/src/config/sections.rs:76`, **`crates/core/schemas/config.json:771`** (the published schema's own description), `referencia-schema.md:68`, D07, D53, D122, `rfc-0001.md:§5`. Storage-engine sense, not adapter sense — **defensible, but the ban is unqualified** |
| `subagente` | `adrs.md:45` (D37) — *"mismo runner, **subagente** distinto por nodo"*. **1 real occurrence**, and D27 is the ADR that banned it |
| `persona` | All occurrences are the Spanish word for "person" in Spanish prose. **Not a violation** |

---

## 7. DOCS SYNC TESTS

### 7.1 What `crates/cli/tests/docs_sync.rs` pins

Two tests, both passing (`cargo test -p yunta --test docs_sync` → `2 passed`):

| Test | What it pins | How |
|---|---|---|
| `the_readme_command_table_names_exactly_the_subcommands_the_binary_has` (`:56-64`) | README command table ≡ `yunta --help` `Commands:` list, **set equality both directions** | Parses the first word after `` | `yunta `` in each table row (`:45-54`) against `--help` output (`:20-41`) |
| `every_yaml_example_in_the_docs_is_one_the_binary_accepts` (`:182-227`) | Every ` ```yaml ` block in `README.md` and `docs/*.md` is valid, classified by top-level key: `nodes` → runs `yunta check` in a fixture project with five mock runners (`:100-136`); `publisher` → `PackManifest::validate()`; `workflow`+`expect` → executes via `yunta test` in a real git repo (`:138-180`); `node_defaults` → embedded in a minimal workflow and checked; else → parses as `ConfigLayer` | Floor assertion `seen >= 8` (`:223`) |

This is genuinely strong: no documented workflow, pack manifest, test case or config layer can drift from the parser, and no README row can name a command that does not exist.

### 7.2 What else pins docs to code

| Mechanism | Pins |
|---|---|
| `cargo xtask schema --check` (CI `ci.yml:34-35`) | The 8 committed JSON Schemas ≡ what the types emit (D139) |
| `cargo xtask smells --check` (`ci.yml:37-38`) | A ratchet on unwrap-outside-tests, `Result<_,String>`, wall-clock reads, oversized files/functions |
| `RUSTDOCFLAGS="-D warnings" cargo doc` (`ci.yml:40-41`) | Broken intra-doc links fail the build |
| `ci.yml:97-105` | Binary size ≤ 33554432 bytes ≡ D155's 32 MiB |
| `ci.yml:46-55` | The repo's own workflows and both factory packs pass `check` and their cases |
| D140/D144 tests (`crates/core/tests/`) | Diagnostic enumerations ≡ what the parser accepts; the published example writes every key the type accepts; every key has its own diagnostic |
| `crates/core/tests/artifacts.rs` | `ArtifactKind::as_str()` ≡ serde, round-trips through `FromStr` (D132) |
| `crates/core/src/text.rs:104` | Names `spec-ledger.md` §4 as the source of the problem-block format (by comment, not by test) |

### 7.3 What CLAUDE.md's rule would want pinned, and isn't

**`docs/design/` is entirely unpinned.** `docs_sync.rs:187-190` does a **non-recursive** `read_dir(docs/)`, so `docs/design/*.md` is never visited — which is why `referencia-schema.md`'s canonical config has been unparseable without anyone noticing, even though `docs/design/README.md:7` declares it the source of the workspace's parse fixtures.

| Claim class | Would catch | Currently |
|---|---|---|
| `referencia-schema.md` config/workflow blocks parse and `check` | The `2_000_000` bug (§4.4) | **Unpinned** |
| Contract §3 event table ≡ `EventPayload::KINDS` (name set + row/kind counts) | Any future drift of the 36/30 claim | **Unpinned** (currently correct) |
| `spec-events.md` §5.x field tables ≡ payload struct fields | All 8 mismatches in §4.1 | **Unpinned** |
| `spec-adapter.md` `Capabilities` list ≡ `Capability::ALL`; degradation table ≡ the capability set | `skills`, `network_isolation` missing | **Unpinned** |
| `spec-adapter.md` §6 per-adapter capability claims ≡ `capabilities()` | `claude-code` "declara todas las capacidades" vs `edit_hooks: false` | **Unpinned** |
| `spec-ledger.md` §3 rule list ≡ `TasksFile::RULES` demands | 7 documented vs 9 published | **Unpinned** — yet D143 already publishes `RULES` as strings, so this is a five-line test |
| MCP control-plane tool list ≡ `tool_definitions()` | Contract §6.4, `cli.rs:141-145`, `mcp.rs:1-2` all saying five | **Unpinned** |
| Contract §7.1 builtins ≡ `CheckBuiltin` variants; §9 sources ≡ `ContextSpec` variants; §2.3 input types ≡ `InputSpec` variants | Future drift | **Unpinned** (all three currently correct) |
| ADR numbering: no gaps, no duplicates, every `D\d+` citation resolves, every `Revisada por DN` has a reciprocal | D03's reviser-less note; a future mis-citation | **Unpinned** |
| Banned-vocabulary grep over `crates/**/src`, `docs/`, `*.yaml`, `*.json` | All 12 `ledger` uses and the user-facing string at `ask/decision.rs:91` | **Unpinned** — the `xtask smells` ratchet is the natural home and already has the machinery |
| Tense/plan markers in rustdoc and docs | All 18 findings in §5.3 | **Unpinned** — also a ratchet candidate |
| `docs_sync.rs:223` floor is `seen >= 8` | Nothing — the docs carry far more than 8 blocks; the floor cannot detect a doc whose examples were deleted | Weak |

---

## 8. DEFECTS

Categories: `doc stale` · `code diverges (doc wins)` · `ADR misnumbered` · `tense/plan` · `banned vocab` · `undefined term` · `unpinned claim`.

### 8.1 Code diverges — doc wins (CLAUDE.md: correct the code, or register a decision)

| # | Defect | Evidence | Severity |
|---|---|---|---|
| D1 | Baseline is captured **lazily**, on the first `baseline_compare` node, not at run creation; the first such node always passes; no `baseline/` directory exists | `crates/engine/src/run/check_exec.rs:77-82` vs `contrato-del-run.md:380` (§7.2), `:15` (§2), **D18** (`adrs.md:23`) | **High** — an unregistered weakening of the no-regressions guarantee, with the reason ("would make `create_run` async across its four call sites") stated in a comment rather than raised |
| D2 | `claude-code` declares `edit_hooks: false`; spec says it *"declara todas las capacidades"* and describes the pre-edit hook implementation | `crates/adapters/src/claude_code/mod.rs:208` vs `spec-adapter.md:227-232` | **High** — `guide.md:78-81` correctly tells users scope is post-hoc only, so the normative spec is the outlier |
| D3 | `codex` declares `resume_session: true` unconditionally; spec says `false` unless `probe()` detects support | `crates/adapters/src/codex/mod.rs:205` vs `spec-adapter.md:237-240` | **High** — a false capability claim is the one thing A2/A6 forbid |
| D4 | `kind: questions` cannot be answered by pull request | `Channel {Tty, Mcp}` (`payloads.rs:105-108`), `crates/engine/src/run/questions_exec.rs` vs `contrato-del-run.md:90, 144` | **Medium** — and the contract contradicts `spec-events.md:334` |
| D5 | Short-circuit ordering learns durations **in-memory per invocation**, never from the log | `crates/engine/src/task_cycle/criteria.rs:29-36` vs **D62** (`adrs.md:71`) and `contrato-del-run.md:204` | **Medium** |
| D6 | Memo key omits `env declarado` (criteria have no `env:` in the schema) | `criteria.rs:24-25, 67-69` vs `contrato-del-run.md:199` | **Low** — the doc names a key component that cannot exist |
| D7 | `ContextSpec` is a closed 8-variant enum; no executor-provided sources | `crates/core/src/workflow/context.rs:18-45` vs **D19** (`adrs.md:24`) and `contrato-del-run.md:466` | **Medium** |
| D8 | `task-ledger` is accepted as **author YAML** (`kind:`, the `ledger:` source) and as a CLI argument | `crates/core/src/workflow/artifacts.rs:194` vs **D110** (`adrs.md:120`) and CLAUDE.md's tolerance rule | **Medium** — and unregistered (§8.4/D22) |

### 8.2 Doc stale (correct the doc)

| # | Defect | Evidence |
|---|---|---|
| D9 | `referencia-schema.md`'s canonical config **does not parse** — `2_000_000`, `50_000_000`, `32_000` | `:82,86,87`; reproduced: `invalid type: string "2_000_000", expected u64`; `crates/core/src/config/sections.rs:193-196` documents exactly this trap |
| D10 | Contract §6.4:322 lists **five** MCP control-plane tools; six are served | `crates/cli/src/commands/mcp.rs:79-91, 136-194` |
| D11 | `crates/cli/src/cli.rs:141-145` (**user-visible `--help`**) and `crates/cli/src/commands/mcp.rs:1-2` both say "Five tools" |
| D12 | Contract §6.4:338 says `yunta_submit_tasks{name, document}`; the tool takes `{document}` only, as §4.1:139, D157 and `glosario.md:108` all say |
| D13 | `crates/adapters/src/session.rs:3-10` — `run_tools_endpoint` *"isn't wired for MCP yet"* (it is) and `env`/`budget`/`adapter_settings` *"nothing populates yet"* (all three do: `crates/engine/src/run/prompt_exec.rs:211-214`) |
| D14 | `crates/core/src/config/sections.rs:133-135, 148-149, 166-168` — *"Only `pause` is built; `check` refuses the rest"*; `abort` and `continue` are built (`schedule.rs:453-455, 569-580`) |
| D15 | `crates/engine/src/process_registry.rs:3` — *"a **future** `--detach` supervisor"*; `--detach` ships (`cli.rs:68-73`) |
| D16 | README:167 `yunta graph <workflow\|run_id>`; the positional is only a workflow path |
| D17 | `concepts.md:101` — `waiting` described as gates only; also covers pending questions (`replay.rs:52-56`) |
| D18 | `spec-events.md` §5.1/§5.6/§5.11 still carry `[inferido]` / *"valores exactos a confirmar contra la implementación"* markers for fields and enums that are settled in `payloads.rs` |
| D19 | `spec-adapter.md` `Capabilities` (6) vs `crates/core/src/capabilities.rs` (8: + `skills`, + `network_isolation`); no degradation-table rows for either |
| D20 | `spec-adapter.md` `SessionRequest` has `context: ResolvedContext` (gone) and lacks `artifact_dir`, `scratch_dir`; `AgentSession` lacks `pgid()` |
| D21 | `spec-events.md` §5.11 missing `commit`; §5.14 missing `paths` on `granted`; §5.18 missing `external_ref` and the entire `GateResolvedPayload` shape model incl. `sha`; §5.5 marks `model` mandatory (it is `Option`) |
| D22 | `spec-ledger.md` §3 documents **seven** rules and says *"estas siete"*; `tasks/rules.rs:29-68` publishes **nine** |
| D23 | Contract §3.2 says node states end in `done`; code and every user doc say `finished` |
| D24 | Contract §5.3 presents `free_text: true` and `default_on_timeout: none` as escalation fields; neither exists on `GateWaitingPayload`, and `external_ref` is missing from the doc |
| D25 | Contract, `rfc-0001.md`, `rfc-0002.md` are **markdown-corrupt**: escaped `\{\{ \[ \] \|`, ` ```javascript ` fences on YAML/trees, `[progress.md](http://progress.md)` auto-links, unterminated fence at `contrato-del-run.md:19` with a stray ` ``` ` at `:673`. Every YAML example in the normative contract is un-copyable |

### 8.3 ADR / numbering

| # | Defect | Evidence |
|---|---|---|
| D26 | **D132** says `label()` yields `"task ledger"`; code yields `"tasks document"` | `adrs.md:142` vs `crates/core/src/workflow/artifacts.rs:212` |
| D27 | **D139** says `xtask schema` writes *"los **siete** archivos"*; it writes eight | `adrs.md:149` vs `crates/core/src/schema.rs:57` |
| D28 | `spec-adapter.md` obligations run O1, O2, O3, **O5**, O4, **O5**, O6 — `O5` duplicated, `O4` out of order | `:183-205` |
| D29 | `rfc-0002.md:§8` cites milestone **"M14"**; no milestone map exists (only D90's `M-0`, `M0–M7`) |
| D30 | `rfc-0003.md:§3` cites **"deuda ⑪"**; the ledger uses `A-01…A-12` and declares its ids stable (`deuda-consciente.md:5-7`) |
| D31 | **D152** is cited in D157's body but carries no `Revisada por D157` note, unlike the eleven other D157 targets | `adrs.md:171` vs `:182` |
| D32 | Six revision notes name no reviser (`Revisada:` without a `DNNN`); **D03's names none at all** | `adrs.md:7` |
| D33 | Section grouping is degenerate: `# Seguridad y operación` holds D33–D109 (77 ADRs incl. licence, distribution, monetization, `yunta test`, bootstrap); all 7 headings are `#`-level, so no ADR has an anchor | `adrs.md:4,11,26,33,40,119` |
| D34 | `docs/design/adr/` (the proposed-decision tier) has never held a file | `ls docs/design/adr/` → `README.md` only |

### 8.4 Banned vocabulary

| # | Defect | Evidence |
|---|---|---|
| D35 | `ledger` names the **tasks document** in 12 places, including a **user-facing gate string** | `crates/cli/src/ask/decision.rs:91`; `crates/engine/src/view/mod.rs:46,78,100-101,248-249`; `view/node.rs:62,190`; `crates/cli/src/surface/view.rs:230`; `crates/cli/src/commands/status/progress.rs:46`; `crates/engine/tests/view.rs:326,338,343`; `crates/engine/tests/live_derivation.rs:3`; `contrato-del-run.md:565`; `adrs.md:142` |
| D36 | `task-ledger` alias on author YAML + CLI, **registered in no ADR** (`grep -c task-ledger docs/design/adrs.md` → 0) | `crates/core/src/workflow/artifacts.rs:194` |
| D37 | `docs/design/spec-ledger.md` — the banned word in the filename every reference must spell | `docs/design/README.md:5`, `docs/guide.md:28,325`, `crates/core/src/text.rs:104` |
| D38 | `plugin` for the pack concept survives in **D06** | `adrs.md:10` |
| D39 | `subagente` survives in **D37** | `adrs.md:45` |
| D40 | `driver` in rustdoc ×3 | `crates/engine/src/run/exec.rs:1`, `promote.rs:7`, `crates/engine/tests/modes.rs:90` |
| D41 | `backend` in rustdoc ×6 and in the **published schema description** | `crates/storage/src/lib.rs:1,3`, `error.rs:5-6`, `store.rs:110`, `crates/core/src/config/sections.rs:76`, **`crates/core/schemas/config.json:771`** |

### 8.5 Tense / plan language

**D42.** 15 rustdoc/comment sites + 6 design-corpus sites, enumerated in §5.3. The three worst are `crates/adapters/src/session.rs:3-10` (a spec-diff paragraph, wrong on three counts), `crates/core/src/config/sections.rs:133-135` (stale + "today's behavior"), and `spec-events.md:115-116` (addressed to a person about work already finished).

### 8.6 Undefined terms

**D43.** `frame` (`RunFrame`/`NodeFrame` — the central type of the observation surface), `standing` (`NodeStanding`), `escalation`, `tradeoff`, `demand line`. All clear the glossary's own admission bar (`glosario.md:4-6`).

### 8.7 Unpinned claims

**D44.** `docs/design/` is invisible to `docs_sync.rs` (non-recursive `read_dir(docs/)` at `:187-190`) — the direct cause of D9 going unnoticed. Everything in §7.3 is unpinned; the highest-value gaps are the spec-events field tables, the `Capabilities` list, the per-adapter capability claims, the MCP tool list, the `RULES` count, and a banned-vocabulary ratchet.

---

## 9. IDEAL

### 9.1 What is already right — keep it

1. **`docs_sync.rs` is the right idea.** Set-equality between the README table and `--help`, plus "every documented YAML is one the binary accepts", is exactly the mechanism CLAUDE.md's rule implies. It has held: all 8 blocks parse, every workflow checks, every documented test case runs.
2. **One normative source per topic, stated as such.** `docs/design/README.md` publishes an explicit **order of authority** (contract → adapter spec → tasks spec → events spec → ADRs → RFCs → schema reference → debt). That ordering is what let me adjudicate every conflict above without guessing.
3. **The glossary's `_Evitar_:` discipline.** Every term names both the word to use and the words it displaces. 26 of 29 terms map 1:1 onto a live type. This is better than most production codebases manage.
4. **The debt ledger's stable ids and hard rule.** *"Nada de esta lista se resuelve implícitamente durante la implementación"* plus non-renumbered ids means `A-12` is citable from `executor_exec.rs:1-7` and stays citable. Eleven of twelve items verified still true; the twelfth (A-04) is correctly closed-by-decision.
5. **ADR content quality.** Rationale, discarded alternatives with *reasons*, revision chains, and near-perfect cross-referencing across 163 decisions with zero gaps and zero duplicates.
6. **Schema generation is already pinned.** `cargo xtask schema --check` + `include_str!` means the published JSON Schema, the binary's output and the committed files cannot diverge. D139/D140/D144 extend this to diagnostic enumerations and the published examples.
7. **Degradation is honest where it counts.** Every `capabilities()` in the adapters carries a comment saying *why* a capability is false. That habit is the reason D2/D3 are findable at all.

### 9.2 How a greenfield version should be organized

**One ADR per file.** `docs/design/adr/DNNN-kebab-slug.md`, with a five-field front-matter block (`number`, `title`, `status: accepted|proposed|revised`, `revises: [DNNN]`, `revised_by: [DNNN]`). `adrs.md` becomes a **generated** index — number, title, status, revised-by, link — regenerated by `cargo xtask adr --check` and diffed in CI. This fixes, in one move: anchors (`adr/D157-...md`), diffability (a one-word edit is a one-line diff, not 14 KB), section grouping (front-matter tags replace prose headings that have drifted to 77 ADRs under "Seguridad"), and the reciprocal-note problem (`revises`/`revised_by` are derived from each other, so D03's reviser-less note and D152's missing note become CI failures). The existing `adr/` directory already expresses this intent; it has simply never been used.

**One normative source per topic, and nothing repeats it.** Today the event kind list lives in four places (contract §3 table, `spec-events.md` §0/§5, `EventPayload::KINDS`, `events.json`) and the capability list in three (spec-adapter §2, spec-events §5.5, `capabilities.rs`). The rule that already works for JSON Schemas — *the type is the source, everything else is an emission* — should extend to every closed set. Docs cite the emission; they do not restate it.

**Pin every closed set by test.** A single `docs/design/` sync test, recursive, asserting:

| Assertion | Effort |
|---|---|
| Contract §3 table row count and kind set ≡ `EventPayload::KINDS` | ~20 lines |
| `spec-events.md` §5.x field names ≡ payload struct fields, per kind | ~60 lines, catches all 8 of §4.1 |
| `spec-adapter.md` capability list ≡ `Capability` variants; degradation table has a row per variant | ~15 lines |
| `spec-adapter.md` §6 per-adapter claims ≡ `capabilities()` of each built adapter | ~20 lines, catches D2 and D3 |
| `spec-ledger.md` §3 ≡ `TasksFile::RULES` demands (already public strings per D143) | ~10 lines |
| Contract §6.4 tool list ≡ `tool_definitions()` and `run_tools::catalog` | ~15 lines |
| Contract §7.1/§9/§2.3 ≡ `CheckBuiltin`/`ContextSpec`/`InputSpec` variants | ~20 lines |
| Every YAML block in `docs/design/` parses (extend `docs_sync.rs` to recurse) | 2 lines, catches D9 |
| ADR index integrity: no gaps, no duplicates, every `D\d+` citation resolves, `revises`/`revised_by` reciprocal | ~40 lines in `xtask` |

**Two grep ratchets in `xtask smells`,** where the machinery already exists:
- **Banned vocabulary** over `crates/**/src`, `docs/`, `*.yaml`, `*.json`, with a baseline that can only go down. Seeded at today's count, it makes D35–D41 a decreasing number rather than a standing violation.
- **Tense/plan markers** (`for now`, `yet`, `today`, `now built`, `the old`, `will be`, `not yet`, `future <feature>`) in `//!`/`///` and in `docs/`. Same ratchet shape. This is the only mechanical enforcement possible for the **Expresión** rule, and §5.3 shows the rule is currently enforced by attention alone — which held for the English user docs and failed for rustdoc.

**Fix the corpus's source format.** The contract and RFCs carry escaped `\{\{ \[ \] \|`, ` ```javascript ` fences over YAML, and `http://progress.md` auto-links — the residue of an export from a WYSIWYG tool. Until that is undone, the normative contract's examples cannot be copied, cannot be parsed, and cannot be pinned by any test. Undoing it is a mechanical pass and it unlocks the §9.2 test for `docs/design/`.

**Where the three documented-but-unbuilt behaviours should land.** D1 (eager baseline), D2 (`edit_hooks`), D4 (questions by PR), D5 (log-learned criterion order) and D7 (executor context sources) each need one of two outcomes, never a third: **build it**, or **raise it and record the decision** — a new `A-NN` in the debt ledger plus a `Revisada` note on the ADR it retires (D18, D62, D19). What they must not do is keep living as a comment explaining why the shortcut was taken (`check_exec.rs:77-80`, `criteria.rs:29-36`, `session.rs:3-10`). CLAUDE.md names that exact failure mode: *"Un comentario que admite «no hay número en ningún lado» es una decisión que alguien tomó solo."*
