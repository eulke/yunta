All core tests pass (`cargo test -p yunta-core`: 0 failures) and `cargo xtask schema --check` exits 0, so the findings below are about design, not breakage.

# 1. MAP — the type inventory

## 1a. Newtyped identifiers (all validated; no unchecked production constructor)

Every string id is minted by one macro, `string_id!` (`crates/core/src/ids.rs:159-290`), which gives each type: `from_static` (const, compile-time panic on a bad literal, `ids.rs:172`), `FromStr`/`TryFrom<String>` through `checked` (`ids.rs:186`), a `Deserialize` that routes through `checked` (`ids.rs:257`), a `JsonSchema` whose description is generated from the rule (`ids.rs:265`), and `From<&str>` **only** under `#[cfg(any(test, feature = "testkit"))]` (`ids.rs:281-289`). That is exactly the shape CLAUDE.md asks for.

| Newtype | Declared | Rule | Constructor validates? |
|---|---|---|---|
| `NodeId` | `ids.rs:292` | `NODE_RULE` (`name`, or `base@runner` for fan-out) | yes; `fan_out`/`base`/`runner` build from already-valid halves (`ids.rs:299-340`) |
| `RunnerName` | `ids.rs:342` | `NAME_RULE` | yes |
| `AdapterId` | `ids.rs:349` | `NAME_RULE` | yes |
| `ModelName` | `ids.rs:355` | `TOKEN_RULE` (printable, no space) | yes |
| `AgentName` | `ids.rs:361` | `TOKEN_RULE` | yes |
| `ModeName` | `ids.rs:367` | `NAME_RULE`; `Default = "default"` via `from_static` (`ids.rs:373`) | yes |
| `ExecutorName` | `ids.rs:381` | `NAME_RULE` | yes |
| `RunId` | `ids.rs:387` | `SEGMENT_RULE`; total `From<Ulid>` (`ids.rs:393`) | yes |
| `TaskId` | `ids.rs:400` | `NAME_RULE` | yes |
| `FindingId` | `ids.rs:406` | `OPAQUE_RULE` | yes |
| `QuestionId` | `ids.rs:413` | `NAME_RULE` | yes |
| `OptionId` | `ids.rs:419` | `NAME_RULE` | yes |
| `Responder` | `ids.rs:426` | `OPAQUE_RULE` | yes |
| `SessionId` | `ids.rs:433` | `OPAQUE_RULE` | yes |
| `Publisher` / `PackName` | `ids.rs:439`, `:445` | `SEGMENT_RULE` | yes |
| `PackRef` | `ids.rs:455` | `publisher/name` | yes (`FromStr`, `ids.rs:477`) |
| `GitHubRepo` | `ids.rs:524` | `owner/name`, each a segment | yes (`ids.rs:546`) |
| `Pid` | `ids.rs:636` | positive, fits `i32` | yes (`TryFrom<u32>`/`<i32>`, `ids.rs:665`, `:679`) |
| `Seq` | `ids.rs:723` | positive | yes; `From<u64>` test-only (`ids.rs:806`) |
| `ContentHash` | `hash.rs:43` | 64 lowercase hex | yes; `ContentHash::sha256` total (`hash.rs:83`) |
| `CommitSha` | `hash.rs:49` | 7–64 lowercase hex | yes; `from_bytes` const-asserts length (`hash.rs:117`) |

No unchecked `From<&str>` escapes into production anywhere in the workspace — the only bypasses are `NodeId::fan_out` (`ids.rs:301`) and `NodeId::runner` (`ids.rs:322`), both building from substrings the node rule already validated.

## 1b. Closed sets that are enums (correct)

`NodeKind` (8 variants + `KINDS`/`keys` lists, `workflow/node_kind.rs:15,168,209`), `CheckBuiltin` (`node_kind.rs:318`), `ForgeKind` (`node_kind.rs:312`), `JoinPolicy` (`node_kind.rs:353`), `Coordination` (`node_kind.rs:339`), `WorkflowIsolation` (`node_kind.rs:279`), `PromptSource` (`node_kind.rs:367`), `NodePermissions` (`node.rs:292`), `OnInterrupt` (`node.rs:317`), `LoopUntil` (`node.rs:355`), `HookFailurePolicy` (`hooks.rs:42`), `CleanupTarget` (`workflow/mod.rs:236`), `ModeInclude` (`workflow/mod.rs:253`), `ArtifactKind` + alias (`artifacts.rs:186`), `ArtifactSpec`/`ArtifactRefId` (`artifacts.rs:35`, `:100`), `RunEventsFilter` (`context.rs:155`), `KnowledgeLayer` (`context.rs:181`), `Isolation` (`sections.rs:126`), `DefaultOnFailure` (`sections.rs:170`), `ExecutorKind` (`sections.rs:288`), `PackExecutorPolicy` (`permissions.rs:81`), `ScopeExpansionMode` (`policy.rs:14`), `AnswerType` (`questions/mod.rs:13`), `RuleCode` (macro-declared, `diagnostic/problem.rs:56`), `Problem` (`problem.rs:128`), `Subject` (`subject.rs:56`), `ArtifactFailure`/`FileProblem` (`diagnostic/artifact.rs:35`, `:174`), `Capability` (`capabilities.rs:52`), `InputSpec` (`inputs.rs:24`, one variant per type so `min` on a boolean is unrepresentable).

## 1c. String-typed fields that still stand in for a type

| Field | Type today | What it represents | Should it be a type? |
|---|---|---|---|
| `Workflow.yunta_schema` `workflow/mod.rs:78`; `PackManifest.yunta_schema` `pack.rs:57` | `Option<String>` | comparator range over the schema major | **Yes** — `SchemaRange` newtype; the parser exists but lives in the engine (`engine/src/check/declarations.rs:73`) and never runs on the pack's copy (`pack.rs:52-56` admits it) |
| `Node.scope` `node.rs:39`; `Task.scope` `tasks/mod.rs:60`; `ScopeExpansion.within` `context.rs:238` | `Vec<String>` | globs | **Yes** — `ScopeGlob` whose `Deserialize` calls `scope_glob` (`glob.rs:12`). Today a bad glob is caught neither at parse nor at `yunta check`, only mid-run (`engine/src/scope.rs:52`) |
| `NodeKind::Workflow.r#use` `node_kind.rs:142`; `Workflow.name` `workflow/mod.rs:44` | `String` | catalog workflow name (a path segment) | **Yes** — `WorkflowName`; `is_path_segment` already exists for exactly this (`ids.rs:139`) |
| `Node.skills` `node.rs:105`, `NodeDefaults.skills` `node.rs:23`, `SkillsConfig.always` `sections.rs:273`, `PackContents.skills` `pack.rs:124` | `Vec<String>` | skill names resolved against `skills.paths` | Yes — `SkillName` |
| `McpQueryParams.server` `context.rs:107` / `ConfigLayer.mcp_servers` key `config/mod.rs:57` | `String` | key of a closed config map | Yes — `McpServerName` (the workflow refers to a config key by bare string) |
| `NodeKind::Gate.assignee` `node_kind.rs:106` | `String` | who the escalation names | Borderline: `Responder` exists (`ids.rs:426`) and is what the log records for the same person |
| `ArtifactSpec::Opaque` `artifacts.rs:42`, `ArtifactRefId::Name` `artifacts.rs:101`, `MountArtifact.rename` `node_kind.rs:249` | `String` | artifact file name | Yes, with a template-aware rule — the rule exists but runs in the engine (`check/declarations.rs:134` calls `stays_inside`) |
| `PackProvenance.commit` `manifest.rs:101`; `PackLockEntry.commit` `pack.rs:153` | `Option<String>` / `String` | a git commit | **Yes — `CommitSha` already exists** and `Manifest.base_commit` uses it (`manifest.rs:130`) |
| `Manifest.base_branch` `manifest.rs:126`; `ExternalGate.branch` `node_kind.rs:303`; `ProjectConfig.base_branch`/`branch_prefix` `sections.rs:96-98` | `String` | git branch (the gate's is a template) | Branch name is a candidate newtype; the template ones are genuinely strings |
| `Manifest.yunta_version` `manifest.rs:113`; `PackManifest.version` `pack.rs:46` | `String` | semver | Documented as deliberate (`pack.rs:38-45`): nothing compares versions yet |
| `Workflow.inputs` keys `workflow/mod.rs:65`, `Manifest.inputs` `manifest.rs:120`, `NodeKind::Workflow.inputs` `node_kind.rs:148` | `BTreeMap<String, …>` | input names | Yes — `InputName` (the `{{inputs.x}}` namespace has a grammar) |
| `EngineProcessFile.started_at` `engine/src/process_registry.rs:29` | `String` | an instant | **Yes** — its sibling lock file uses `DateTime<Utc>` for the same concept (`engine/src/lock.rs:50`) |
| `RunTerminal::Paused/Failed.reason` `engine/src/run/mod.rs:218,226`; `RunPausedPayload.reason` `core/src/events/payloads.rs:956`; `RunResumedPayload.resume_policy_applied` `payloads.rs:965` | `String` | a closed cause / an `OnInterrupt` | **Yes** (events front): config prose already names the closed set — "`run_paused { reason: budget }`" (`sections.rs:201`) — and `OnInterrupt::as_str` exists (`node.rs:340`) |
| `external_ref` `payloads.rs:506` | `Option<String>` | forge reference | events front; the only stringly field left there — `decided_by`/`channel`/`by`/`sha` are all typed (`payloads.rs:71-76, 396, 557, 673`) |
| `DiagnosticCount.code` `engine/src/receipt/mod.rs` | `String` | a diagnostic code | Yes — see §7, the code vocabulary is half-typed |

# 2. AUTHORED YAML — strictness, diagnostics, one door

## 2a. `deny_unknown_fields` coverage

Every authored struct/enum in core carries it, or replaces it with a hand-written deserializer that names the offending key **and** lists the valid ones:

| Shape | Strict? | How |
|---|---|---|
| `Workflow` | yes | `workflow/mod.rs:41` |
| `ModeSpec`, `NodeDefaults`, `Hooks`, `HookStep`, `OnFailure`, `Artifacts` | yes | `workflow/mod.rs:246`, `node.rs:17`, `hooks.rs:10,24,56`, `artifacts.rs:22` |
| `Node` | yes (custom) | splits node keys from kind keys, names *all* unknown keys at once with the kind judged against, plus retired-key hints for `role`/`fresh_context` (`node.rs:195-257`; `RETIRED_NODE_KEYS` at `node.rs:148`) |
| `NodeKind`, `CheckBuiltin` | yes | internally tagged + deny (`node_kind.rs:14`, `:317`) |
| `MountSpec`, `ExternalGate`, `McpQueryParams`, `RunEventsParams`, `TasksParams`, `KnowledgeParams`, `NodeOutputParams`, `ScopeExpansion` | yes | `node_kind.rs:231,289`, `context.rs:105,145,171,197,213,234` |
| `ContextSpec`, `OnFinishStep` | yes (custom) | `keyed_entry` names the key and lists the legal ones (`workflow/parse.rs:14-46`); aliases read but never advertised (`context.rs:60`) |
| `DistillArtifact`, `MountArtifact`, `ArtifactContextRef` | yes (custom) | `ArtifactRefId::from_rest` names the unknown key against the whole reference's key set (`artifacts.rs:114-155`) |
| `InputSpec` | yes | via `AuthoredInputSpec` (`inputs.rs:175`) |
| `ConfigLayer` + every section + every permissions group | yes | `config/mod.rs:45`; `sections.rs` ×15; `permissions.rs` ×6 |
| `PackManifest`, `PackRequires`, `RequiredRunner`, `PackDeclares`, `PackContents` | yes | `pack.rs:33,71,87,100,120` |
| `TasksFile`/`Task`/`Criterion`, `FindingsFile`/`FindingEntry`/`Withdrawal`/`ProposedCriterionEntry`, `QuestionsFile`/`Question` | yes | `tasks/mod.rs:17,27,56`; `findings/mod.rs:18,34,53,68`; `questions/mod.rs:29,44` |

Gaps:
- **`ArtifactRefId`'s derived `Deserialize` is untagged with no per-variant `deny_unknown_fields`** (`artifacts.rs:99-102`). Unreachable today (all three containers go through `from_rest`), but it is a public type whose derive would silently swallow an extra key.
- `Answer`/`AnswersFile` are not strict (`questions/mod.rs:56,64`) — engine-written, never read back, so inert, but they are the one authored-looking pair without the rule.
- Tests bypass the single YAML door: `serde_norway::from_str` directly in `crates/core/tests/integration.rs:7` and `crates/core/tests/config.rs:575,659` where `yaml::parse` is the documented one door (`yaml.rs:1-10`).

## 2b. Diagnostics shape and D143

`Report { document: DocumentRef, diagnostics: Vec<Diagnostic> }` (`diagnostic/mod.rs:101,39`), `Diagnostic = Subject + Problem` (`:61`), `Problem = Parse { path, message } | Rule { code, detail }` (`problem.rs:128`), `Subject` in the document's own vocabulary with `Named<Id>` carrying id *and* position (`subject.rs:27,56`). `RuleCode` is declared by a macro so the variant, its serialized name and `ALL` cannot drift (`problem.rs:34-104`). Per-kind `RULES` sit beside the functions that enforce them (`tasks/rules.rs:29`, `findings/rules.rs:16`, `questions/rules.rs`), exactly as D143 (`docs/design/adrs.md:153`) requires, and three tests hold the chain: contract states every rule, every `RuleCode` belongs to some contract, every example reads back (`shape/mod.rs:216-268`).

**One door:** `shape::read` (`shape/mod.rs:71`) and `shape::accept` (`:110`) are the only ways to obtain an interpreted document, and both run `Document::check` in the same call; `Document` is sealed (`shape/mod.rs:57-63`). `render` (`:140`) and `contract` (`:151`) are the ways back out. Engine ingestion honours it (`engine/src/artifacts/canonical.rs:101,107`; `engine/src/artifacts/ingest.rs:30`).

Two doors that escape it:
1. **`FindingEntry` and `Withdrawal` are parsed by hand in the engine** — `serde_path_to_error::deserialize` + a local `parse_problem` that re-derives what `accept` already does (`engine/src/run_tools/findings.rs:59`, `:117`, helper at `:214`). Neither type is a `Document`, so core offers no door for them.
2. **A `Workflow` has no door at all.** Its cross-document rules live in `engine/src/check/` (duplicate ids at `check/mod.rs:94-101`, broken refs at `:134`), so "parse without validating" is available and taken: 11 sites call `yaml::parse::<Workflow>` directly (`cli/src/commands/list/mod.rs:91`, `cli/src/graph.rs:39`, `cli/src/commands/test.rs:213`, `engine/src/check/refs.rs:133`, `engine/src/run/workflow_exec/mod.rs:189`, `engine/src/pack_audit.rs:110`, …). Only `yunta run` routes through `resolve_and_check` (`cli/src/commands/run.rs:296-299`). This is precisely the vice D136 removed for artifacts, still standing for workflows.

## 2c. Validation done twice

No true double validation found. The candidates resolve cleanly: `TaskId`'s pattern is parse-only (no matching rule in `tasks/rules.rs`); `might_overlap` has one implementation with two callers (`glob.rs:44`, used by `tasks/rules.rs` and `check/scopes.rs:117`); `stays_inside` has one implementation with two callers (`pack.rs:181` → `pack.rs:208`, `check/declarations.rs:134`); the log-dependent findings rules are published from core and enforced once in the engine, and the split is documented (`findings/rules.rs:46-51`). The opposite problem exists instead: rules that run *only late* (globs, `yunta_schema` of a pack, artifact names).

# 3. TOLERANCE — what is persisted, versioned, and marked

| Persisted artifact | Version declared? | Tolerant reader? | Marks what it didn't understand? |
|---|---|---|---|
| event log rows | per-kind `schema_version` (`storage/src/store.rs:274-288`), read back at `events/mod.rs:253` | yes — `EventBody::Unknown` (`events/mod.rs:100`) | **yes** — unknown kinds surface in the receipt (`engine/src/receipt/mod.rs`, `unknown_kinds`) and in `status`/`stats` |
| `manifest.yaml` | `schema_version: u32` written (`core/src/manifest.rs:110`, value at `engine/src/manifest.rs:19,102`) | yes (no `deny`), `paths`/`pack` documented as tolerant (`manifest.rs:136-146`) | **no** — the field is written and **never read**: no comparison anywhere (`read_manifest`, `engine/src/run/mod.rs:88`, just parses). A newer manifest is read silently, or dies on a raw serde error because `Manifest` embeds the strict `Workflow`/`ConfigLayer` |
| `yunta.lock` (`PackLock`/`PackLockEntry`) | **no version at all** (`pack.rs:143-170`) | yes, and a test pins it (`crates/core/tests/strict_keys.rs:181`) | no |
| `run.dir/scratch/engine.json` (`EngineProcessFile`) | **no** (`engine/src/process_registry.rs:23`) | tolerant by default | no |
| isolation lock (`LockOwner`) | **no** (`engine/src/lock.rs:46`) | tolerant by default | no |
| `receipt.json` | **no** — `Receipt` has no version field (`engine/src/receipt/mod.rs:110`; rendered raw at `receipt/render.rs:160`, saved at `cli/src/commands/receipt.rs:32`) | n/a (`Serialize` only) | no |
| `stats/status/run --json` | `SCHEMA_VERSION: u32 = 3`, one constant (`cli/src/json.rs:17`) | n/a | n/a |
| config layers | `version:` refused when unknown (`cli/src/project.rs:137-143`) — the only version actually enforced | n/a (authored, strict) | n/a |

**Tolerance is confined where it belongs** — no `#[serde(default)]`-swallowing in authored types; the two `default`s on `Capabilities` fields (`capabilities.rs:33,43`) sit under a container-level `#[serde(default)]` (`capabilities.rs:16`) on a type that is an event payload. But the rule's other half is unmet: `core/src/manifest.rs:109-110` states "everything persisted is versioned from the first commit", and four persisted files falsify it, while the one version that *is* written (the manifest's) is never consulted.

# 4. TEXT — produced once?

`core/src/text.rs` is the single layout module: `LINE_WIDTH:26`, `one_line:30`, `hanging:36`, `indent:52`, `detailed:73`, `aside:96`, `problems:116`, each with a doc explaining why it is there and not in a surface. Consumption is genuinely centralised — `Report`'s `Display` goes through `text::problems` (`diagnostic/mod.rs:119`), `shape::contract` through `text::one_line` (`shape/mod.rs:180`), and ~30 call sites across engine and cli use the helpers (`engine/src/git.rs:18,49,69`, `engine/src/run/steps.rs:237,296`, `cli/src/commands/mod.rs:249,257,348`, `cli/src/commands/test.rs:94,97,155`, `cli/src/render/width.rs:7` re-exporting `LINE_WIDTH`).

Duplicates found:

| Second copy | Where | Note |
|---|---|---|
| pluralized counting | `cli/src/commands/mod.rs:296` (`counted(n, noun)`) **and** `cli/src/surface/lines.rs:79` (`counted_problems`) **and** `text::problems`'s own `error/errors` (`text.rs:118-121`) | three writings of one rule; `counted_problems(n)` is literally `counted(n, "problem")` |
| `one_line` wrappers | `cli/src/commands/status/decision.rs:219`, `cli/src/surface/closing.rs:329` | thin delegations to core — harmless, but two private aliases of one name |
| serde-error → `Diagnostic` | `engine/src/run_tools/findings.rs:214` (`parse_problem`) vs the same mapping inside `shape::accept` (`shape/mod.rs:118-127`) | a real second copy of the frontier's wording |
| human text built in core | `questions::validate_answers → Vec<String>` (`questions/mod.rs:76-113`) and `permission_layer_conflicts → Vec<String>` (`config/permissions.rs:228-303`) | prose assembled in core rather than data rendered at the edge — the exact thing `Diagnostic` exists to avoid (`diagnostic/mod.rs:1-15`) |

No duplicate of `indent`/`wrap`/`detailed`/`aside` exists; `cli/src/render/width.rs:26,57,109` (`indent(depth)`, `wrap`, `truncate`) are different concepts (cell-width aware), correctly local to the terminal surface.

# 5. SCHEMAS

`schema::all()` emits **eight** roots — workflow, config, pack, tasks, findings, questions, withdrawal, events (`core/src/schema.rs:63-75`) — written/checked by `cargo xtask schema [--check]` (`xtask/src/main.rs:23-24,64-97`) into `crates/core/schemas/`, and embedded with `include_str!` for the three interpreted kinds (`schema.rs:85-88`). I ran `cargo xtask schema --check`: **exit 0, no drift**.

D144's first half is implemented and exhaustive: `example_writes_every_key` derives the key list from the type's own generated schema and asserts the published example writes each one, for `Task`, `Criterion`, `FindingEntry`, `ProposedCriterionEntry`, `Question` (`shape/mod.rs:205-237`). Its second half ("every key of the example, given a wrong-typed value, produces a diagnostic naming it, never a `Problem::Unreadable`", `docs/design/adrs.md:155`) exists only as hand-picked cases (`crates/core/tests/shape.rs:98,109,120,134,146`) — and `Problem::Unreadable` no longer exists (D156 replaced the walk, `adrs.md:180`). D140 likewise still describes "el recorrido de forma" (`adrs.md:150`) without a `(Revisada por D156)` marker, unlike D129/D146/D156 which carry theirs.

Published examples out of date:
- **`docs/compatibility.md:380-383` lists seven schema files and omits `withdrawal.json`**, which `schema::all()` emits (`schema.rs:71`) and the repo holds (`crates/core/schemas/withdrawal.json`). D139 (`adrs.md:149`) says "los siete archivos" too.
- **`docs/design/referencia-schema.md:85,89,90` publishes `max_tokens_per_run: 2_000_000`, `max_artifact_bytes: 50_000_000`, `inline_context_bytes: 32_000`** — literals the parser refuses, as the type's own doc says (`config/sections.rs:193-196`) and a test pins (`crates/core/tests/config.rs:570-576`). The reference config as published does not parse.
- `docs/compatibility.md:240` enumerates the artifact codes as "`artifact-missing`, `artifact-undelivered`, `artifact-unheld`", omitting `artifact-empty`/`artifact-oversized`/`artifact-unreadable` (`diagnostic/artifact.rs:190-197`). Unlike `RuleCode`, these codes have no test binding them to any published list.

# 6. CONFIG

| Concern | Where read | Notes |
|---|---|---|
| layer precedence | `ConfigLayer::merge_layers` folds `[org, user, repo]` (`config/mod.rs:161`); field-by-field in `config/merge.rs:12-62` | arrays replace, maps merge per key — documented at `config/mod.rs:10-16` |
| permissions inversion | `merge_permissions` (`permissions.rs:120`), conflicts reported by `permission_layer_conflicts` (`permissions.rs:228`) | org is the ceiling; strictest wins per group |
| storage path / `paths.runs` / `paths.worktrees` | `StorageConfig` `sections.rs:80`, `PathsConfig` `sections.rs:107`; `~` expanded once in `expand_home` (`config/mod.rs:98-136`) | frozen per run by `FrozenPaths::new`, the only constructor, absolute-only (`manifest.rs:59-77`) |
| runners / adapters | `RunnerCandidate` `sections.rs:14`, `AdapterSettings` `sections.rs:70` | keys are `RunnerName`/`AdapterId` newtypes |
| defaults & limits | resolved in one place each: `resolved_isolation/…/resolved_inline_context_bytes` (`config/mod.rs:139-231`) | every literal default lives there and nowhere else |
| secrets | names only: `ConfigLayer.secrets: Vec<String>` (`config/mod.rs:88`), `McpServerConfig.auth_env` (`sections.rs:32`), `GitHubForgeConfig.token_env` (`sections.rs:58`) — **no value ever in config**; values wrapped in `Secret<T>` whose `Debug` redacts and which has no `Display`/`Serialize` (`secret.rs:11-38`) | correct |
| env vars | `Env` is the injected boundary (`config/env.rs:41-57`), filled once in `cli/src/project.rs:95-101` | **but** the process is read in five more places |

Env reads outside the one function: `cli/src/project.rs:129` re-reads `HOME` inside `load_layer` (contradicting its own comment at `:91-94`); `cli/src/identity.rs:16` reads `USER`; `cli/src/commands/mod.rs:202` reads the forge token (edge, by design); `engine/src/task_cycle/session.rs:66` (`secrets_env`) and `engine/src/run/context_resolve/mcp.rs:53` read secret values from inside the engine — defensible at spawn time, but they contradict `config/env.rs:43-46` ("captured once at a shell boundary so no code below the boundary reads the process itself"). Presentation reads (`TERM`, `NO_COLOR`) in `cli/src/render/glyphs.rs:53-57` and `cli/src/surface/mod.rs:110-111` are two copies of the same probe.

One dead-ish edge: `ScopeExpansionPermissions` is `pub` (`permissions.rs:43`) but re-exported by neither `config/mod.rs:30-33` nor `lib.rs:48-56`, so the public field `PermissionsConfig.scope_expansion` names a type no consumer can spell (the engine only pattern-matches it, `check/mod.rs:187-194`).

# 7. DEFECTS

| # | Defect | Evidence | Category |
|---|---|---|---|
| 1 | `scope:`/`within:` globs never compiled at the frontier — an invalid glob survives parse *and* `yunta check`, failing only mid-run after tokens are spent | `workflow/node.rs:39`, `tasks/mod.rs:60`, `context.rs:238` vs `engine/src/scope.rs:52`; no glob check in `engine/src/check/*` | stringly-typed |
| 2 | `yunta_schema` range is a bare `String`; the pack's copy is never parsed at all | `workflow/mod.rs:78`, `pack.rs:52-57`, parser at `engine/src/check/declarations.rs:73` | stringly-typed |
| 3 | Commits as `String` where `CommitSha` exists and is used a few lines away | `manifest.rs:101` and `pack.rs:153` vs `manifest.rs:130` | stringly-typed |
| 4 | Timestamp as `String` in one persisted file and `DateTime<Utc>` in its sibling | `engine/src/process_registry.rs:29` vs `engine/src/lock.rs:50` | stringly-typed / un lugar |
| 5 | `Manifest.schema_version` written and never read; a newer manifest is neither refused nor marked | written `engine/src/manifest.rs:19,102`; `read_manifest` `engine/src/run/mod.rs:88` has no check; no reader in the workspace | tolerance leak |
| 6 | Persisted files with no version at all, against core's own claim that "everything persisted is versioned from the first commit" | `pack.rs:143` (lock), `process_registry.rs:23`, `lock.rs:46`, `receipt/mod.rs:110` vs `core/src/manifest.rs:109-110` | tolerance leak / doc-code divergence |
| 7 | `Manifest` (tolerant) embeds `Workflow` and `ConfigLayer` (both `deny_unknown_fields`), so a manifest from a newer writer cannot be read by an older binary | `manifest.rs:115-116` + `workflow/mod.rs:41` + `config/mod.rs:45` | tolerance leak |
| 8 | A second parse door for agent input: `FindingEntry`/`Withdrawal` hand-parsed in the engine with a local copy of `accept`'s error mapping | `engine/src/run_tools/findings.rs:59,117,214` vs `shape/mod.rs:110-127` | duplicated helper / one-door bypass |
| 9 | A workflow can be obtained without its rules — 11 direct `yaml::parse::<Workflow>` sites, rules living in `engine/src/check/` | `cli/src/commands/list/mod.rs:91`, `cli/src/graph.rs:39`, `engine/src/run/workflow_exec/mod.rs:189`, … vs `shape::read` `shape/mod.rs:71` | one-door bypass |
| 10 | Human prose assembled in core instead of typed data rendered at the edge | `questions/mod.rs:76-113`, `config/permissions.rs:228-303` (both `Vec<String>`) | stringly-typed / text produced twice |
| 11 | Pluralization written three times | `cli/src/commands/mod.rs:296`, `cli/src/surface/lines.rs:79`, `text.rs:118` | duplicated helper |
| 12 | The "stable code" vocabulary is half-typed: `RuleCode` is an enum, but parse/file/artifact codes are `&'static str` literals and the receipt stores `String` | `problem.rs:171`, `diagnostic/artifact.rs:91,190`, `engine/src/receipt/mod.rs` (`DiagnosticCount.code: String`) | stringly-typed |
| 13 | `Task`'s rustdoc says the id pattern is not enforced by the type; the field is `TaskId`, which enforces it at parse, and the test asserts the parse behaviour | `tasks/mod.rs:51-54` vs `:58` and `crates/core/tests/shape.rs:134-144` | doc/code divergence |
| 14 | Published reference config does not parse (underscored numeric literals) and has drifted from its own fixture (`baseline.suite`) | `docs/design/referencia-schema.md:85,89,90,40` vs `crates/core/tests/fixtures/reference-config.yaml:74,78,79,39` and `crates/core/tests/config.rs:570` | doc/code divergence + un lugar (two copies, nothing ties them) |
| 15 | Docs list seven schema files; there are eight | `docs/compatibility.md:380`, `adrs.md:149` vs `core/src/schema.rs:63-75` | doc/code divergence |
| 16 | D140/D144 still describe "el recorrido de forma" and `Problem::Unreadable`, removed by D156, without a revision marker | `adrs.md:150,155` vs `problem.rs:128` and `adrs.md:180` | doc/code divergence |
| 17 | An unreadable `pack.yaml`/`yunta.lock` at freeze time is swallowed with `.ok()?` — no event, no diagnostic | `engine/src/manifest.rs:151-155` (documented as deliberate at `:129-138`) | degradación no explícita |
| 18 | `ArtifactRefId`'s derived untagged `Deserialize` would accept unknown keys if ever reached | `artifacts.rs:99-102` | missing deny_unknown_fields (latent) |
| 19 | `ScopeExpansionPermissions` is public but unexported; `Answer`/`AnswersFile` are the only non-strict document-shaped types | `permissions.rs:43`; `questions/mod.rs:56,64` | api hygiene / missing deny |
| 20 | `HOME`/`TERM` read in more than one place despite `Env` being the declared single boundary | `cli/src/project.rs:96` vs `:129`; `render/glyphs.rs:57` vs `surface/mod.rs:110` | un lugar |

# 8. IDEAL — greenfield core, and what must be kept

## Keep exactly as is

- **`ids.rs`'s `string_id!` machine** (`ids.rs:159`): one macro that yields `from_static` (compile-time), `FromStr`/`TryFrom`, a `Deserialize` that cannot skip the rule, a schema description generated from the rule, and `From<&str>` gated behind `testkit`. This is the reference implementation of "parsear es validar" and `hash.rs` correctly reuses it (`hash.rs:43,49`).
- **`shape::read`/`accept` + sealed `Document` + per-kind `RULES`** (`shape/mod.rs:34-170`, `tasks/rules.rs:29`, `findings/rules.rs:16`): rules beside enforcement, published before writing, and three self-checking tests (`shape/mod.rs:216-268`, `crates/core/tests/vocabulary.rs:136-175`).
- **`Report`/`Diagnostic`/`Subject`/`Problem`/`RuleCode`**: diagnostics as data, subject in the document's vocabulary, `Named<Id>` fusing id and position (`subject.rs:27-53`), `rule_codes!` making a code unmintable by typing a string (`problem.rs:34`).
- **`text.rs`** as the one layout module, and `yaml.rs` as the one YAML door with path-located errors (`yaml.rs:44`, `from_value` at `:60` preserving nested paths).
- **`ArtifactKind`** as the single type for the kind set with alias, `label`, `as_str`, `listed`, `submit_tool` (`artifacts.rs:186-292`), and **`InputSpec`**'s one-variant-per-type shape plus `AuthoredInputSpec` → `TryFrom` contradiction check (`inputs.rs:24,175,255`).
- **`FrozenPaths::new`** as the only constructor refusing a relative root (`manifest.rs:59`), **`Secret<T>`** (`secret.rs:11`), **`Clock`/`IdSource`** injection (`clock.rs`, `id_source.rs`), and the node deserializer that names every unknown key at once with retired-key hints (`node.rs:195-257`).

## What becomes a type

1. `ScopeGlob` (compiles through `glob.rs:12` in `Deserialize`) replacing `Vec<String>` in `Node.scope`, `Task.scope`, `ScopeExpansion.within` — kills defect 1 and moves the check before the first token.
2. `SchemaRange` in core, parsed at the frontier, consumed by both `Workflow.yunta_schema` and `PackManifest.yunta_schema`; `engine/src/check/declarations.rs:73` becomes its `FromStr`.
3. `CommitSha` for `PackProvenance.commit` and `PackLockEntry.commit`; `DateTime<Utc>` for `EngineProcessFile.started_at`.
4. `WorkflowName`, `SkillName`, `InputName`, `McpServerName`, `ArtifactName` (template-aware, absorbing `stays_inside`) — each a `string_id!` line.
5. `PauseCause` / typed `RunTerminal` reasons, and `OnInterrupt` (not `String`) for `resume_policy_applied` — the events front's remaining prose-as-payload.
6. One `DiagnosticCode` enum covering rule, parse, file and artifact codes, so `Problem::code`, `FileProblem::code`, `ArtifactFailure::code` and the receipt's `DiagnosticCount.code` are one vocabulary with one test.
7. `PersistedDoc<T>` discipline: every persisted file carries `schema_version`, a reader that compares it, and an `unknown` bucket that surfaces like `EventBody::Unknown` already does — applied to manifest, lock, `engine.json`, `receipt.json`.

## What becomes a single door

1. **`Document` widened to the agent's unit of submission**: `FindingEntry` and `Withdrawal` become `Document`s (or `shape::accept_entry`), deleting `engine/src/run_tools/findings.rs:214` and both hand-rolled parses.
2. **`workflow::read(bytes, path) -> Result<Workflow, Report>`** in core, running today's `engine/src/check/` graph rules through the same `Report`, so no caller can hold an unchecked workflow — closing the last hole D136 left.
3. **One reference config**: `docs/design/referencia-schema.md` includes the fixture (or the fixture is generated from the doc) with a test that parses it, so defect 14 cannot recur.
4. **One counting helper**: `text::counted(n, noun)` in core, consumed by `text::problems`, `cli/src/commands/mod.rs:296` and `cli/src/surface/lines.rs:79`.
5. **One env boundary**: `Env` grows `user`, `term`, `no_color` and the secret lookup becomes a single injected `SecretSource`, so `std::env` appears exactly once outside tests.
