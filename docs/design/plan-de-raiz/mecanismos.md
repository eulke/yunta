# Los veintiséis mecanismos, uno por uno

Para cada mecanismo: qué vicio vuelve irrepresentable, las firmas exactas, los
archivos que toca (nuevo · modifica · borra), los tests que lo sostienen y los
defectos del índice que cierra. Un agente implementa lo que está acá escrito;
lo que no está, se levanta (README §0.2).

Evidencia de cada afirmación: [`auditoria/`](auditoria/), por frente.

---

## M01 · Puerto en core

**Vicio V9.** El engine importa su propia interfaz desde el crate de adapters
concretos (`crates/engine/Cargo.toml` depende de `yunta-adapters`;
`engine/src` usa `yunta_adapters::{Adapter, Budget, SessionRequest,
RunToolsEndpoint, PermissionProfile, signal::Liveness, process_start}`).

**Firmas.**

```rust
// crates/core/src/port/mod.rs
pub trait Adapter: Send + Sync { /* idéntico a adapters/src/session.rs:251-280 */ }
pub trait AgentSession: Send { /* idéntico a session.rs:282-305 */ }
pub struct SessionRequest { /* idéntico a session.rs:44-105; en 3-08 toma la forma de cerco.md §3 */ }
pub struct RunToolsEndpoint { pub url: String, pub token: Secret<String> }
pub const SERVER_NAME: &str = "yunta";
pub enum ProbeReport { Healthy { version: Option<String> }, Unhealthy { diagnostic: String } }
pub enum AgentEvent { /* idéntico; en 3-08 gana `fence` en SessionOpened y `WriteRefused` (cerco.md §3) */ }
pub enum AdapterError { /* idéntico */ }
pub struct Budget { /* idéntico */ }
pub enum PermissionProfile { /* idéntico */ }
pub static POLICY: [(Capability, Absence); Capability::ALL.len()];   // ver M09

// crates/core/src/process/{mod,subprocess,signal,process_start}.rs
//   contenido de adapters/src/{subprocess,signal,process_start}.rs, sin cambios de comportamiento

// crates/adapters/src/mock/fixture.rs
pub struct RunPaths<'a> { pub run_dir: &'a Path, pub worktree: &'a Path, pub staging: &'a Path }
impl MockFixture { pub fn parse(yaml: &str, paths: &RunPaths<'_>) -> Result<Self, FixtureError>; }
```

**Archivos.**
- nuevo: `crates/core/src/port/mod.rs`, `crates/core/src/process/{mod,subprocess,signal,process_start}.rs`, `crates/testkit-core/` (Cargo + `src/{lib,clock,ids,capture,log}.rs`), `crates/engine/tests/no_adapter_crate_in_engine.rs`.
- modifica: `crates/core/src/lib.rs` (exporta `port`, `process`); `crates/adapters/src/lib.rs` (implementa, no reexporta el puerto); `crates/adapters/src/mock/fixture.rs` (`parse` con `RunPaths`; el render usa `yunta_core::template` — el renderer de templates pasa de engine a core en este ítem si no está); `crates/engine/Cargo.toml` (quita `yunta-adapters`); todo `use yunta_adapters::` en `crates/engine/src` → `yunta_core::port::`; `crates/cli/src/commands/test.rs` (`load_mock_fixture` llama `MockFixture::parse`); `crates/testkit/src/bench.rs` (`from_yaml` → `parse` con los paths del bench); `crates/cli/src/commands/mod.rs::real_adapters` (registro `Vec<Arc<dyn Adapter>>` construido una vez; `refuse_unrunnable` y `doctor` listan `registry.iter().map(Adapter::id)`); `crates/cli/src/commands/init.rs` (usa los `ID` constantes de cada adapter); `Cargo.toml` workspace (miembro `testkit-core`); `crates/core/Cargo.toml`, `crates/adapters/Cargo.toml` (dev-dep `yunta-testkit-core`).
- borra: `crates/adapters/src/{subprocess,signal,process_start}.rs` (movidos), la definición de los traits en `adapters/src/session.rs` (queda solo la implementación compartida si hay).

**Tests.** `no_adapter_crate_in_engine` (lee `crates/engine/Cargo.toml`, asserta que no nombra `yunta-adapters`); `a_fixture_renders_its_run_paths_wherever_it_is_parsed` en `crates/adapters/tests/mock.rs` (mismo YAML con `{{staging}}` parsea igual desde el CLI y desde el bench); los tests de `adapters/tests/{claude_code,codex,mock}.rs` migran `request()`/`drain()`/`write_lines()`/`child_pid_fifo()`/`grandchild_pid()` a `yunta_testkit_core::adapter` y el test de secretos duplicado se vuelve uno parametrizado.

**Cierra.** CLI-D21, AD-D8, AD-D16, AD-D17, TE-D4.

---

## M02 · Kind declarado en su dominio

**Vicio V4.** Un kind se declara en 9 sitios + 2 docs (auditoría 01 §1).

**Estructura.**

```
crates/core/src/events/
  mod.rs          StoredEvent, EventDraft, EventBody, envelope — sin cambios
  wire.rs         EventPayloadWire + KINDS + JsonSchema a mano
  run/            kinds.rs payloads.rs ledger.rs happening.rs
  node/           …
  session/        …
  tasks/          …
  scope/          … (ledger.rs = GrantLedger, movido)
  findings/       … (ledger.rs = FindingLedger, movido; payloads y kinds nuevos)
  artifacts/      … (ledger.rs = ArtifactLedger, movido)
  gates/          …
  children/       …
  evidence.rs failure.rs   sin cambios
```

**Firmas.**

```rust
// core/src/events/<dominio>/kinds.rs — ejemplo findings
#[derive(Debug, Clone, PartialEq)]
pub enum FindingEvent { Posted(FindingPosted), Updated(FindingUpdated), Withdrawn(FindingWithdrawn), Refused(FindingRefused) }
impl FindingEvent {
    pub const KINDS: &'static [&'static str] = &["finding_posted", "finding_updated", "finding_withdrawn", "finding_refused"];
    pub fn kind_name(&self) -> &'static str;
    pub fn schema_version(&self) -> u32;      // por kind, hoy 1
    pub fn is_audit(&self) -> bool;           // false para todo kind que mueve estado
}

// core/src/events/mod.rs
#[derive(Debug, Clone, PartialEq)]
#[serde(from = "wire::EventPayloadWire", into = "wire::EventPayloadWire")]
pub enum EventPayload { Run(RunEvent), Node(NodeEvent), Session(SessionEvent), Tasks(TaskEvent), Scope(ScopeEvent), Findings(FindingEvent), Artifacts(ArtifactEvent), Gates(GateEvent), Children(ChildEvent) }
impl EventPayload {
    pub const KINDS: &'static [&'static str] = concat_kinds!(RunEvent, NodeEvent, …);   // orden = orden actual de `KINDS` en mod.rs:339-376
    pub fn kind_name(&self) -> &'static str;   // delega
    pub fn schema_version(&self) -> u32;       // delega
}

// core/src/events/wire.rs
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum EventPayloadWire { RunCreated(RunCreatedPayload), … /* 36 variantes planas, mismo orden */ }
impl From<EventPayloadWire> for EventPayload; impl From<EventPayload> for EventPayloadWire;
impl JsonSchema for EventPayload { /* delega en EventPayloadWire: mismo oneOf de 36 ramas, mismo $defs.EventPayload */ }
```

**Regla del wire.** `crates/core/schemas/events.json` no cambia ni un byte;
`cargo xtask schema --check` lo prueba. `kind_names_match_the_spec_exactly`
y `the_events_schema_is_one_stored_event_with_its_envelope_and_every_kind`
pasan sin editar sus literales. `EventBody::from_object` sigue decidiendo
por `EventPayload::KINDS`.

**Archivos.**
- nuevo: los 9 directorios con sus 4 archivos; `wire.rs`; macro `concat_kinds!` en `mod.rs` o `wire.rs`.
- modifica: `mod.rs` (enum de 9 brazos, `KINDS` derivado, `kind_name`/`schema_version` delegan); `core/tests/events.rs` (`all_kinds()` se deriva iterando un constructor de ejemplo por kind expuesto por cada dominio bajo `#[cfg(any(test, feature = "testkit"))]`; los "36" pasan a `EventPayload::KINDS.len()`); `mod.rs:282` doc sin número literal.
- borra: `payloads.rs` (su contenido se reparte); `findings.rs`, `artifacts.rs` de `core/src/events/` (se mueven a sus dominios).

**Tests.** Todos los de `core/tests/events.rs` y `schema_json.rs` sin cambio de aserción; nuevo `every_domain_declares_the_kinds_the_wire_carries` (unión de `<Dominio>::KINDS` == `EventPayload::KINDS` como conjunto y en orden).

**Cierra.** EV-D1, EV-D2, EV-D17, EV-D18, EV-D19, EN-D20 (parte).

---

## M03 · Constructor por hecho

**Vicio V1.** Auditoría 01 §2: `gate_waiting` 4 builders/5 emisores,
`capability_degraded` 8 sitios, `node_finished` 4, `node_started` 3,
`finding_posted` 4 bypass, `run_paused` 10 razones.

**Firmas.** Cada payload struct pierde sus campos `pub` de escritura (quedan
`pub` para lectura por `getter` o campos `pub` con constructor obligatorio
donde el invariante no lo prohíbe; la regla: **sin constructor no se
construye**).

```rust
// run/payloads.rs
impl RunPausedPayload { pub fn new(reason: PauseReason) -> Self; pub fn reason(&self) -> &PauseReason; }
impl RunResumedPayload { pub fn new(policy: OnInterrupt) -> Self; }
impl RunFinishedPayload { pub fn closed(terminal: TerminalState, state: &RunState) -> Self; }  // métricas derivadas
// node/payloads.rs
impl NodeStartedPayload { pub fn attempt(n: u32) -> Self; }
impl NodeFinishedPayload { pub fn new(outcome: impl Into<String>, tokens: TokenUsage) -> Self; }
impl NodeFailedPayload { pub fn new(failure: Failure, retryable: bool, tokens: TokenUsage) -> Self; }  // existe
impl NodeReroutedPayload { pub fn new(to: NodeId, cause: RerouteCause, origin: RerouteOrigin, attempt: Option<u32>, max: Option<u32>) -> Self; }
pub struct RerouteCause(pub Failure);   // Display = Failure's
// session/payloads.rs
impl CapabilityDegradedPayload { pub fn new(capability: Capability, adapter: AdapterId, policy: Policy) -> Self; }
pub enum Policy { PostCheckOnly, NoTokenBudget, NoSkills, NoRunTools, NetworkOpen, FreshSession }   // Display en el borde
impl AgentMessagePayload { pub fn tool_use(target: ToolTarget) -> Self; pub fn usage(…) -> Self; pub fn note(summary: NoteSummary) -> Self; }
// tasks/payloads.rs
impl TaskStatusChangedPayload { pub fn to(task: TaskId, status: TaskStatus, caused_by: Seq) -> Self; pub fn done(task: TaskId, caused_by: Seq, commit: CommitSha) -> Self; }
// gates/payloads.rs
pub struct Escalation(GateWaitingPayload);   // o GateWaitingPayload::new — el nombre público es `Escalation::new`
impl Escalation { pub fn new(summary: impl Into<String>, evidence: Evidence, options: NonEmpty<GateOption>) -> Result<Self, EscalationError>; }
pub enum EscalationError { SummaryRepeatsEvidence }
impl QuestionsAskedPayload { pub fn new(questions_hash: ContentHash, questions: NonEmpty<QuestionId>, tokens_used: TokenUsage) -> Self; }   // M26
impl QuestionsAnsweredPayload { pub fn via(hash: ContentHash, channel: Channel, responder: Option<Responder>) -> Self; }
// children/payloads.rs
impl ChildRunFinishedPayload { pub fn new(run: RunId, terminal: TerminalState, tokens: TokenUsage) -> Self; }
// artifacts/payloads.rs
impl ArtifactAcceptedPayload { pub fn new(artifact: ArtifactId, hash: ContentHash, origin: RecordedOrigin) -> Self; }
// ArtifactWrittenPayload: sin constructor; #[doc(hidden)]; solo Deserialize
```

`NonEmpty<T>` es un newtype en `core/src/nonempty.rs` con `NonEmpty::new(Vec<T>) -> Option<Self>` y `From<(T, Vec<T>)>`.

**Archivos.**
- modifica: cada `<dominio>/payloads.rs`; los emisores listados en auditoría 01 §2 pasan por el constructor: `run/escalation.rs:41,82`, `run/budget.rs:72,102`, `run/gate_exec.rs:117,544`, `loop_exec/escalate.rs:191` (Escalation); `task_cycle/attempt.rs:225`, `task_cycle/session.rs:116`, `loop_exec/mod.rs:272`, `runner_resolve.rs:259`, `prompt_exec.rs:82,158,181` (Degradation — en M08/M09 estos sitios se reducen a `open_session`); `node_exec.rs:75`, `questions_exec.rs:118`, `gate_exec.rs:514` (NodeStarted); `node_close.rs:184`, `gate_exec.rs:188,490,570` (NodeFinished: los tres pasan por `node_close::finish_node`, único emisor desde W-11 — `questions_exec.rs` ya no lo emite, M26); `steps.rs:146,212`, `gate_exec.rs:475`, `schedule.rs:435,442` (NodeRerouted con `RerouteCause`); `escalate.rs:93,274`, `gate_exec.rs:228`, `node_artifacts.rs:317` (por `engine_finding`); `steps.rs:43-54,116-127,276-287` (RunFinished::closed); `workflow_exec/mod.rs:554,589,649` (ChildRunFinished::new); `tasks/mod.rs:168`, `integrate.rs:108,151`, `escalate.rs:123`, `dispatch.rs:120` (TaskStatusChanged::to/done — `dispatch.rs:104-121` deja de escanear el log: el `caused_by` lo da `TaskLedger`).
- borra: `exec.rs:34-39` `run_paused()` helper (reemplazado por `RunPausedPayload::new`).

**Tests.** `an_escalation_with_no_options_cannot_be_built`, `an_escalation_whose_summary_repeats_a_fact_is_refused` (core/tests/events.rs); `a_done_task_carries_its_commit_and_nothing_else_does` (core/tests/events.rs); `every_engine_authored_finding_goes_through_engine_finding` (engine/tests/degradation.rs — recorre el log de un run que dispara los 4 sitios y asserta el esquema de id de `engine_finding`); `gate_exec` tests existentes sin cambio de aserción.

**Cierra.** EV-D6, EV-D7, EV-D8, EV-D9, EV-D10, EV-D15, EN-D15, AD-D10.

---

## M04 · Un ledger por dominio

**Vicio V2.** Auditoría 01 §4 y 02 §2: la tabla completa de pliegues ad hoc.

**Firmas.**

```rust
// core/src/events/<dominio>/ledger.rs — cada uno con la oración de cabecera de FindingLedger:
// "Every surface that … reads that set from here rather than folding … itself — a second fold is a second answer."
pub struct RunLedger { phase: RunPhaseRaw, mode: ModeName, closed: Option<(TerminalState, Seq)>, paused: Option<(PauseReason, Seq)>, resumed_after: Option<Seq> }
pub struct NodeLedger { per_node: BTreeMap<NodeId, NodeRecord> }
pub struct NodeRecord { attempts: u32, state: NodeState, open_since: Option<(Seq, DateTime<Utc>)>, last_terminal: Option<Seq>, last_failed: Option<Seq>, last_finished: Option<Seq>, reroutes: u32, last_reroute: Option<Reroute>, runner: Option<ResolvedRunner>, tokens_closed: TokenUsage, tokens_in_flight: TokenUsage, sessions: Vec<OpenSession>, calls: Vec<ToolCall>, last_event_at: Option<DateTime<Utc>> }
pub struct SessionLedger { /* sesiones por (node, attempt) */ }
pub struct DegradationLedger { pub all: Vec<Degradation> }
pub struct TaskLedger { per_task: BTreeMap<TaskId, TaskRecord> }   // status, owner node, registered_seq, identity (criteria+scope hash), attempt, commit
pub struct GateLedger { per_node: BTreeMap<NodeId, GateRecord> }   // waiting: Option<(Escalation, Seq)>, resolved: Vec<(GateResolvedPayload, Seq)>, external_ref: Option<String>, approved_sha: Option<CommitSha>, rounds: Vec<QuestionRound> (M26)
impl GateLedger { pub fn last_external_ref(&self, node: &NodeId) -> Option<&str>; pub fn pre_seeded(&self, node: &NodeId) -> Option<&GateResolvedPayload>; pub fn pending_questions(&self, node: &NodeId) -> Option<&QuestionsAskedPayload>; pub fn answered_unfinished(&self, node: &NodeId) -> bool; }   // M26: la ronda vive acá; `NodeRecord.tokens_closed` suma `questions_asked.tokens_used`
pub struct ChildLedger { pub links: Vec<ChildLink> }
impl ChildLedger { pub fn open_under(&self, node: &NodeId) -> Option<&ChildLink>; }
// cada ledger: pub fn apply(&mut self, event: &XEvent, envelope: &EventMeta) — exhaustivo (M05)
// EventMeta { seq, at, node: Option<&NodeId> }

// engine/src/replay.rs
impl RunState { pub fn apply(&mut self, event: &StoredEvent); }   // derive = fold(apply); chronicle lo usa evento a evento
pub struct RunState { pub run: RunLedger, pub nodes: NodeLedger, pub sessions: SessionLedger, pub degradations: DegradationLedger, pub tasks: TaskLedger, pub grants: GrantLedger, pub findings: FindingLedger, pub artifacts: ArtifactLedger, pub gates: GateLedger, pub children: ChildLedger, pub unknown: UnknownKinds, pub broken: Option<ReplayError>, pub effective_findings: Vec<Finding> /* calculado una vez al final */ }
```

**Archivos.**
- nuevo: `ledger.rs` en `run`, `node`, `session`, `tasks`, `gates`, `children`; `core/src/events/meta.rs` (`EventMeta`).
- modifica: `engine/src/replay.rs` (`derive` = despacho + `effective` al final; `RunState` con los ledgers; `dedup_findings` única regla); `engine/src/findings.rs::inherited_findings` (llama `dedup_findings`); `engine/src/run_tools/blackboard.rs:26-52,64-86` (por `FindingLedger`); `engine/src/run/schedule.rs` (borra `NodeHistory` 196-303, `last_external_ref` 187-194; lee `state.nodes`, `state.gates`); `engine/src/run/gate_exec.rs` (borra `last_external_ref` 365-375, `last_approved_sha` 352-362, el conteo de attempt 502-518; lee `state.gates`/`state.nodes`); `engine/src/run/questions_exec.rs` (la ronda no cuenta intentos: lee `state.gates.pending_questions`, M26); `engine/src/run/parallel_exec.rs:39-47` (ver M07); `engine/src/live.rs` (`running_since`, `last_event_age`, `open_sessions`, `recent_tool_calls`, `in_flight_tokens`, `since_last_terminal` → lecturas de `NodeLedger`; el módulo queda como fachada o desaparece); `engine/src/stats.rs::walk_attempts` (lee `NodeLedger`); `engine/src/view/mod.rs::walk_log` (borrado: `runner`, `reroute`, `reroutes`, `children`, `degraded` vienen de `RunState`); `engine/src/view/phase.rs` (lee `RunLedger`); `engine/src/receipt/mod.rs` (7 walks → lecturas de `RunState`); `engine/src/verification_effectiveness.rs` (un `derive` por log histórico, 5 pases → lecturas); `engine/src/tasks/{mod,crossing}.rs::{prior_registrations,standing_of}` (`TaskLedger`); `engine/src/run/loop_exec/dispatch.rs:19-46,104-121` (`TaskLedger`, `GrantLedger`); `engine/src/run/loop_exec/mod.rs:413` (`GrantLedger`); `engine/src/run/prompt_exec.rs:294-338::orphaned_session` (`SessionLedger`); `engine/src/run/workflow_exec/mod.rs:129-155` (`ChildLedger::open_under`); `engine/src/run/escalation.rs:259-290::pre_seeded_resolution` (`GateLedger::pre_seeded`); `engine/src/artifacts/mod.rs::RunArtifacts::of` con sus 4 callers (`gate_exec.rs:72`, `questions_exec.rs:32`, `distill.rs:159`, `promote.rs:145` leen `state.artifacts`); `engine/src/run/node_close.rs:190` (`progress.md` desde `RunState`, sin segundo replay); `engine/src/run/distill.rs` (`provenance.yaml` cuenta los findings desde `RunState::effective_findings`, deduplicados por la misma regla que el frame — hoy cuenta los vigentes sin deduplicar).

**Prerequisito cerrado.** W-04 (blackboard por `FindingLedger::effective`; una regla de dedup, la que colapsa espacios y mayúsculas, en `dedup_findings`).

**Tests.** Por ledger, en `core/tests/<dominio>_ledger.rs`: determinismo, monotonía de prefijo, y un test de comportamiento por lectura (`a_node_that_reroutes_and_finishes_reports_its_second_attempt`, `the_last_external_ref_is_the_one_the_latest_gate_published`, …). `crates/engine/tests/properties.rs::derive_is_deterministic` sin cambio. `the_frame_agrees_with_the_chronicle` (M19) ata `NodeLedger` a la crónica. `crates/engine/tests/blackboard.rs::a_withdrawn_finding_leaves_the_blackboard` (W-04).

**Cierra.** EV-D4 (parte), EV-D5, EV-D11, EV-D12, EV-D13, EV-D14, EN-D4, EN-D5, EN-D6, EN-D7 (parte), EN-D21, EN-D23, AR-D1, CLI-D20 (parte).

---

## M05 · Replay sin comodín

**Vicio V3.** `replay.rs:370` `_ => Ok(())`; `phase.rs:116`; 30+ comodines.

**Regla.** En `core/src/events/*/ledger.rs` y en `engine/src/replay.rs` no
existe `_ =>` sobre un `EventPayload` ni sobre un `<Dominio>Event`. Un kind
que no mueve estado se declara en su `kinds.rs` con `is_audit() == true` y el
`apply` del ledger lo nombra explícitamente (`FindingEvent::Refused(_) => {}`
con comentario de por qué es auditoría). Los lectores de presentación
(`stats`, `view`, `receipt`, `cli`) pueden usar comodín; cada uno declara en
su rustdoc de módulo qué dominios lee.

**Archivos.** `engine/src/replay.rs` (borra 352-370); cada `ledger.rs`;
`engine/src/view/phase.rs:101-117,137-150` (lee `RunLedger`, sin comodín);
ratchet `wildcard_in_ledger_apply` en M22.

**Tests.** `a_kind_that_moves_no_state_says_so_by_name` (core/tests/events.rs:
para cada kind, `is_audit()` XOR "algún ledger cambia al aplicarlo" — se
prueba aplicando el constructor de ejemplo a `RunState::default()` y
comparando).

**Cierra.** EV-D3, EV-D4, EV-D5.

---

## M06 · Hechos tipados, prosa en el borde

**Vicio V5.**

**Firmas.**

```rust
// core/src/events/run/payloads.rs
pub enum PauseReason { Escalation(Escalation), Cancelled, BudgetExhausted { spent: u64, cap: u64 }, ExternalGate { url: String }, UncertainOrphans(Vec<NodeId>), NodeFailed { node: NodeId, failure: Failure }, Blocked { node: NodeId, on: Vec<NodeId> }, Questions { node: NodeId, pending: NonEmpty<QuestionId> }, AnswersRefused { node: NodeId, report: Report } }   // las dos últimas: M26
impl Display for PauseReason;   // la única prosa; sentence() de Escalation la produce
// core/src/capabilities.rs: Capability::as_str() ya existe — se usa en cli/src/surface/lines.rs:151 y toda superficie
// engine/src/run/mod.rs
pub enum RunError { …, Git(#[source] GitError), Broken(#[source] ReplayError), ManifestWrite(#[source] std::io::Error), … }   // sin `detail: String`
// engine/src/worktree/mod.rs: WorktreeError::Git(#[source] GitError)
// adapters/src/claude_code/parse.rs
#[derive(Deserialize)] #[serde(tag = "type", rename_all = "snake_case")]
enum ClaudeLine { System(SystemLine), Assistant(AssistantLine), Result(ResultLine), #[serde(other)] Unknown }
// tokens: Option<u64>; ausente => None, nunca 0
// adapters/src/codex/parse.rs: ídem con el schema que su doc comment (:5-37) transcribe
// core/src/port: AgentError { kind: AgentErrorKind, #[source] cause: Option<Box<dyn Error + Send + Sync>> }
// core/src/questions/answers.rs: AnswersFile::against(&QuestionsFile, Vec<Answer>) -> Result<Self, Report>   // reemplaza validate_answers (M12, M26): la puerta ve las preguntas; `cli/src/ask/form.rs:135,157` la consume pregunta por pregunta
// core/src/config/permissions.rs: permission_layer_conflicts(...) -> Vec<Diagnostic>
```

**Archivos.** `run/payloads.rs`; los 10 sitios de `run_paused` (auditoría 01
§2); `schedule.rs:394-398,451,574,583`, `steps.rs:296-299`,
`gate_exec.rs:125,277`, `budget.rs:85-88,122-125`, `questions_exec.rs:92-97`
(construyen `PauseReason`); `run/mod.rs:107,165,168`;
`worktree/mod.rs:44-49`; `loop_exec/integrate.rs:316-319`;
`adapters/src/{claude_code,codex}/parse.rs`; `port` (`AgentError`);
`core/src/questions/mod.rs:76-113`; `core/src/config/permissions.rs:228-303`
y sus consumidores en `engine/src/check/`; `cli/src/surface/lines.rs`
(desaparece con M19 — hasta entonces, `as_str`).

**Tests.** `a_pause_reason_renders_once_at_the_border` (cli/src/render tests:
`PauseReason` → una cadena, la misma en `status`, closing y crónica);
`a_missing_token_count_is_absent_not_zero` (adapters/tests/claude_code.rs y
codex.rs); `an_unknown_stream_line_is_tolerated_and_named`
(adapters/tests/*); `a_git_failure_keeps_its_cause` (engine/tests/git.rs).

**Cierra.** EV-D8 (parte), EN-D26, CLI-D6, AD-D13, AD-D14, AD-D18, CO-10.

---

## M07 · decide / execute separados

**Vicio V8/V1.** `next_step` 370 líneas; `parallel_exec` re-decide;
`StillWaiting` ambiguo; `run_finished` ×3.

**Firmas.**

```rust
// engine/src/run/schedule.rs
pub fn decide(workflow: &Workflow, state: &RunState, policy: &Policy) -> Decision;   // antes next_step(events)
fn gate_step(…) -> Option<Decision>; fn waiting_step(…) -> Option<Decision>; fn answered_step(…) -> Option<Decision>; fn orphan_step(…) -> Option<Decision>; fn failure_step(…) -> Option<Decision>; fn ready_batch(…) -> Decision;
// waiting_step decide AskQuestions por Node::asks; answered_step decide FinishAnswered por GateLedger::answered_unfinished, antes de orphan_step (M26)
pub fn resume_policies(state: &RunState, workflow: &Workflow) -> Vec<(NodeId, OnInterrupt)>;   // ya existe; parallel_exec la llama
// engine/src/run/gate_exec.rs
pub enum GateStep { Resolved(Resolution), Waiting(PauseReason) }   // el gate nunca escribe run_paused
// engine/src/run/steps.rs
async fn finish(ctx, terminal: TerminalState) -> …   // único: RunFinished::closed + export + cleanup
// engine/src/run/escalation.rs
pub fn current_escalation(manifest, state: &RunState) -> Option<(NodeId, Escalation)>;   // sin segundo decide; mode = run_mode()
```

**Archivos.** `schedule.rs` (partir 208-578); `exec.rs:113-189` (llama
`decide(&workflow, &state, &policy)`; una lectura y un `derive` por
iteración; `ctx.run_view()` en los handlers terminales reutiliza el
`RunState` de la iteración); `steps.rs:317-338` (budget desde `state`, no
segundo `derive`); `steps.rs:38-54,114-127,222-293` (`finish`);
`gate_exec.rs:387-470,414,440` (`GateStep::Waiting`); `steps.rs:432-463`
(borra la compensación); `parallel_exec.rs:39-47`; `escalation.rs:116-177`
(borra `current_mode_name` 158-163; usa `run_mode()`).

**Prerequisito cerrado.** W-06 (`parallel_exec` por `resume_policies`).

**Tests.** `every_decision_is_a_function_of_state_alone`
(engine/tests/schedule.rs: mismo `RunState` → misma `Decision`, sin log);
`a_gate_that_waits_records_one_run_paused_and_the_gate_records_none`
(engine/tests/run_gates_limits.rs); `a_run_closes_with_one_run_finished_whatever_way_it_closes`
(engine/tests/run.rs); W-06 test.

**Cierra.** EN-D2, EN-D6 (parte), EN-D14, EN-D20 (parte), EN-D22.

---

## M08 · SessionPlan único

**Vicio V1/V7.** `SessionRequest` ×2 (auditoría 03 §5): `attempt.rs:263`
sin modelo, agente ni `artifact_dir`; gate de artifact tipado solo en prompt.

**Firmas.**

```rust
// engine/src/run/session_plan.rs
pub struct SessionPlan<'a> { pub node: &'a Node, pub task: Option<&'a TaskId>, pub prompt: String, pub chosen: &'a RunnerCandidate, pub profile: PermissionProfile, pub artifact_dir: Option<PathBuf>, pub resume: Option<SessionId> }
pub async fn open_session(ctx: &RunCtx<'_>, plan: SessionPlan<'_>, adapter: &dyn Adapter) -> Result<(SessionRequest, Vec<Degradation>), RunError>;
// único sitio del workspace que escribe `SessionRequest { … }`
```

`open_session`: resuelve skills (`require(Skills)`), abre run tools
(`require(RunTools)` con la refusal `TypedArtifactNeedsRunTools` /
`BlackboardNeedsRunTools` / `TypedArtifactListenerFailed`), `fence`
(`require(Fence)` y `Fence::for_session(profile, scope, artifact_dir)`, M25),
`agent` (`require(CustomAgents)`), `network`
(`require(NetworkIsolation)`), `budget.max_turns` (`require(UsageReporting)`);
`model = Some(chosen.model.clone())`, `agent = chosen.agent.clone()`,
`artifact_dir = plan.artifact_dir`, `scratch_dir` por `SessionSlot` (siempre),
`yunta_bin` de `RunEnv`, `env = secrets_env(…)`, `run_tools_endpoint`,
`skills`, `adapter_settings`.
Registra cada `Degradation` por `ctx.log().record` antes de devolver.

**Archivos.** nuevo `session_plan.rs`; modifica `prompt_exec.rs:122-278`
(arma el plan, llama `open_session`, `dispatch_session`); `task_cycle/attempt.rs:190-298`
(ídem; `SessionSetup` gana `chosen: RunnerCandidate` y `artifact_dir` — W-01
lo hace primero); `loop_exec/mod.rs:231-315` (`prepare_loop` deja de gatear:
lo hace `open_session` por tarea); `runner_resolve.rs:99-137,167-239`
(`open_run_tools` queda como función llamada solo por `open_session`;
`run_tools_allowed`, que W-01 introdujo como la decisión sin el bind, es lo
que `open_session` consulta antes de abrir el listener);
borra los gates inline de `prompt_exec.rs:142-194` y `loop_exec/mod.rs:252-315`.

**Prerequisito cerrado.** W-01 (`SessionSetup` con `chosen` y `artifact_dir`;
`run_tools_allowed`).

**Tests.** `a_prompt_session_and_a_task_session_are_opened_by_the_same_door`
(engine/tests/run_sessions.rs: dos runs, mismo runner, los `SessionRequest`
que el mock registra son iguales campo a campo salvo `prompt`, `cwd`,
`scratch_dir`); W-01 tests; `a_loop_task_on_a_run_tools_less_adapter_is_refused_before_any_session`.

**Cierra.** EN-D3, AR-D2, AR-D3, AR-D4, AD-D9.

---

## M09 · Capacidad → política como tabla

**Vicio V7.** Tres capacidades sin consulta; prosa ×7; `check` ciego.

**Firmas.**

```rust
// core/src/port/policy.rs
pub enum Absence { Resting, FailAtCheck, FailNode, DegradeWith(Policy) }
pub const POLICY: [(Capability, Absence); 8] = [
    (Capability::ResumeSession,      Absence::DegradeWith(Policy::FreshSession)),
    (Capability::Fence,              Absence::DegradeWith(Policy::PostCheckOnly)),   // FenceLevel::None; M25
    (Capability::PermissionProfiles, Absence::FailAtCheck),
    (Capability::CustomAgents,       Absence::FailAtCheck),
    (Capability::UsageReporting,     Absence::DegradeWith(Policy::NoTokenBudget)),
    (Capability::Skills,             Absence::DegradeWith(Policy::NoSkills)),
    (Capability::RunTools,           Absence::DegradeWith(Policy::NoRunTools)),   // FailNode lo decide `require` cuando el nodo declara artifact interpretado o blackboard
    (Capability::NetworkIsolation,   Absence::DegradeWith(Policy::NetworkOpen)),
];
pub fn absence_of(capability: Capability) -> &'static Absence;   // test: Capability::ALL.len() == POLICY.len() y cada variante aparece una vez
// engine/src/run/capability.rs
pub enum Decision { Granted, Degraded(Degradation), Refused(RunError) }
pub fn require(adapter: &dyn Adapter, capability: Capability, node: &Node, ctx: &RunCtx<'_>) -> Decision;   // único consumidor de POLICY en el engine
// engine/src/check/mod.rs
pub fn check(workflow: &Workflow, config: &ConfigLayer, adapters: &Adapters) -> Result<Vec<CheckWarning>, CheckError>;   // FailAtCheck se aplica acá
```

`Fence` y `UsageReporting` degradan **una vez por run** (la
`DegradationLedger` sabe si ya se registró). `check_warnings` toma adapters:
`read_only`/`edit` sin `permission_profiles` y `agent:` sin `custom_agents`
son errores de `check`.

**Archivos.** nuevo `core/src/port/policy.rs`, `engine/src/run/capability.rs`;
modifica `engine/src/check/mod.rs:241-245` y `cli/src/commands/run.rs::runnable`
(pasa `&adapters` a `check`); `session_plan.rs` (usa `require`);
`task_cycle/session.rs:372-379` (budget: si `NoTokenBudget` está registrado,
el run no finge presupuesto — y lo dice); `adapters/src/claude_code/permissions.rs:36-47`
(`read_only` con `Write`/`Edit` solo si hay archivos declarados, bajo el cerco — M25; o `permission_profiles: false`); `adapters/src/codex/mod.rs:143-157`
(`ReadOnly`+raíces → `AdapterError::FenceUnbuildable(SealedRoots)`, M25); `adapters/src/mock/fixture.rs:87-111`
(twin + test).

**Tests.** `every_capability_has_exactly_one_absence_policy`
(core/tests/capabilities.rs); `every_capability_round_trips_through_a_fixture`
(adapters/tests/mock.rs); `a_run_on_an_adapter_without_a_fence_says_so_once`
(engine/tests/degradation.rs); `a_run_on_an_adapter_without_usage_reporting_says_it_has_no_token_budget`;
`check_refuses_read_only_on_an_adapter_without_permission_profiles`
(engine/tests/check.rs); `check_refuses_an_agent_on_an_adapter_without_custom_agents`.

**Cierra.** EV-D10 (parte), AD-D2, AD-D3, AD-D4, AD-D7, AD-D9, AD-D10, AD-D15, AD-D19, AD-D20, DO-D2 (con P3).

---

## M10 · Una cáscara

**Vicio V8.**

**Regla.** Todo subproceso por `spawn_governed`; todo disco por `tokio::fs`
o `spawn_blocking`; reloj, entorno, ids y secretos inyectados; todo
`tokio::spawn` conserva su handle y propaga el span; toda degradación es un
evento.

**Firmas.**

```rust
// engine/src/git.rs
async fn git(ctx: &Shell<'_>, cwd: &Path, args: &[&str]) -> Result<Output, GitError>;   // por spawn_governed
pub struct Shell<'a> { pub registry: &'a ProcessRegistry, pub cancel: &'a CancellationToken, pub clock: &'a dyn Clock, pub env: &'a Env, pub secrets: &'a dyn SecretSource }
// core/src/config/env.rs
pub trait SecretSource: Send + Sync { fn get(&self, name: &str) -> Option<Secret<String>>; }
pub struct ProcessSecrets;   // impl en cli/main; RunEnv.secrets: Arc<dyn SecretSource>
// core/src/hash.rs
impl ContentHash { pub fn short(&self) -> &str /* 12 */ }
```

**Archivos.** `git.rs:116,132,145` (W-05: las tres funciones async que un
run llama); `git.rs:157,167` y `manifest.rs:73,139,305` (3-05: `build_manifest`
async —102 llamadas en 33 archivos, casi todas tests y `Bench`— y la pareja
sincrónica por `spawn_governed`; `cli/commands/init.rs:89,97` y
`cli/commands/test.rs:418` corren fuera de todo run y reciben un `Shell` sin
token de cancelación); `node_close.rs:255`;
`distill.rs:151,173,178,228,251`; `context_resolve/sources.rs:40,237`;
`context_resolve/knowledge.rs:171`; `workflow_exec/mod.rs:174`;
`lock.rs:158,178,185`; `worktree/mod.rs:159,230,210,379,424`;
`task_cycle/criteria.rs:108`; `process_registry.rs:72,82,91,96-103,157-160`;
`task_cycle/session.rs:66,131,244,246`; `context_resolve/mcp.rs:53`;
`gate_exec.rs:525` (`encode_ref`); `prompt_exec.rs:137`;
`loop_exec/mod.rs:244`; `gate_exec.rs` y `questions_exec.rs` (`#[instrument]`);
`adapters/src/subprocess.rs:107,142` y `mock/mod.rs:234,370` (`instrument`,
handle); `adapters/src/claude_code/mod.rs:72-78,272-296`,
`mock/mod.rs:146-167` (`tokio::fs`); `cli/src/main.rs` (construye
`ProcessSecrets`, `Env` una vez); `cli/src/project.rs:129`,
`cli/src/identity.rs:16`, `cli/src/render/glyphs.rs:53-57`,
`cli/src/surface/mod.rs:110-111`, `cli/src/commands/doctor.rs:21`
(reciben `&Env`).

**Prerequisitos.** W-05 (pendiente), W-07 (cerrado).

**Tests.** W-05, W-07; `no_engine_module_reads_the_process_clock_or_env`
(engine/tests/purity.rs: grep-test sobre `crates/engine/src` por
`SystemClock`, `Utc::now`, `std::env::var`, `std::fs::` fuera de la lista
blanca `process.rs`); `every_gate_and_questions_node_carries_a_node_span`
(engine/tests/spans.rs); `a_registry_write_that_fails_is_on_the_log`
(engine/tests/degradation.rs con un `run_dir` de solo lectura).

**Cierra.** EN-D1, EN-D7 (parte), EN-D8, EN-D9, EN-D10, EN-D12, EN-D13, EN-D16, EN-D17, EN-D18, EN-D19, AD-D1, AD-D11, AD-D12, AD-D21, CLI-D15, CO-20.

---

## M11 · Secreto

**Firmas.**

```rust
// core/src/events/session/payloads.rs
pub struct ToolTarget { pub display: Option<String>, pub digest: ContentHash }   // el hash entero; `abbreviated()` es cómo se muestra, nunca cómo se guarda — reemplaza el digest abreviado que W-03 escribe como paso intermedio
impl ToolTarget { pub fn of_path(path: &Path) -> Self /* display = path relativo */; pub fn opaque(input: &[u8]) -> Self /* display = None */; }
// engine/src/run_log.rs
impl RunLog<'_> { async fn record(…) { let draft = self.secrets.redact(draft); … } }   // redacta todo valor que SecretSource conozca en campos String del payload
// engine/src/run_tools/listener.rs: subtle::ConstantTimeEq sobre bytes; `expected: Secret<String>`
// engine/src/process_registry.rs: `clear()` borra también scratch/mcp.json
```

**Archivos.** `session/payloads.rs`; `adapters/src/claude_code/parse.rs:131-138`,
`codex/parse.rs:109-138` (`ToolTarget::opaque` para `command`/`url`,
`of_path` para `file_path`/`pattern`); `run_log.rs`; `listener.rs:109-125`;
`process_registry.rs:88-94`; `Cargo.toml` de engine (dep `subtle`,
defendida en el PR).

**Prerequisito cerrado.** W-03 (`target_digest` por `ContentHash::abbreviated()`).

**Tests.** W-03; `a_secret_the_config_names_never_reaches_the_log`
(engine/tests/degradation.rs: un `Note` que contiene el valor de un secreto
declarado; el log tiene `[redacted]`); `the_bearer_check_is_constant_time`
(no es testeable por tiempo; se testea que use `ConstantTimeEq` por tipo —
el `expected` es `Secret<String>` y no hay `==`).

**Cierra.** AD-D5, AD-D6.

---

## M12 · Parsear es validar, extendido

**Firmas.** Cada newtype por `string_id!` o por `impl` propio con `FromStr`,
`TryFrom<String>`, `Deserialize` por `checked`, `JsonSchema` con la regla.

```rust
// core/src/glob.rs
pub struct ScopeGlob(globset::Glob);   // Deserialize compila; Display = el patrón
// core/src/schema_range.rs
pub struct SchemaRange { … }   // FromStr = engine/src/check/declarations.rs:73 movido
// core/src/workflow/artifacts.rs
pub struct ArtifactName(String);
impl ArtifactName { pub fn parse(s: &str) -> Result<Self, Problem>; }   // relativo, sin `..`, sin absoluto, no en ReservedIdentity
pub enum ReservedIdentity { Kind(ArtifactKind) /* "<kind>.yaml" */, Answers /* "questions.answers.yaml" → desaparece con Answers como kind */ }
pub enum ArtifactKind { Tasks, Findings, Questions, Answers }
impl ArtifactKind { pub fn declarable(self) -> bool; }   // false sólo para Answers: lo escribe el engine (M26)
// core/src/questions/answers.rs: impl Document for AnswersFile (RULES intra-documento: id único) y
// AnswersFile::against(&QuestionsFile, Vec<Answer>) -> Result<Self, Report>: toda pregunta `required` tiene respuesta;
// un `choice` responde uno de sus `values`; un id responde una pregunta que existe; un `boolean` es true/false —
// publicadas con `code` y `demand` como las de tasks/findings/questions. `shape::accept` no ve las preguntas, así que
// `validate_answers` desaparece en favor de `against` (M26). El nodo siguiente monta `{ artifact: { node, kind: answers } }`;
// check: AnswersFromNodeThatNeverAsks { node, source }, AnswersDeclaredAsProduced { node } (preguntas.md §2).
// core/src/template.rs
pub enum TemplateVar { Input(InputName), RunDir, Worktree, Staging, RunnerName, NodeId, … }   // BTreeMap<TemplateVar, String>
// core/src/ids.rs (string_id!)
WorkflowName, SkillName, InputName, McpServerName
// core/src/events/artifacts/payloads.rs
pub enum RecordedOrigin { Submitted, Ingested, Derived, Answered, Input, Inherited { run: RunId, producer: Option<NodeId> } }
pub enum ArtifactOrigin { Recorded(RecordedOrigin), Legacy }   // solo el fold produce Legacy
// core/src/findings/mod.rs
pub struct Location { pub path: RelativePath, pub range: Option<LineRange> }
// core/src/diagnostic/problem.rs
pub enum DiagnosticCode { Rule(RuleCode), Parse(ParseCode), File(FileCode), Artifact(ArtifactCode) }
// engine/src/artifacts/ingest.rs
pub struct VerifiedArtifact { …, pub staged: Option<StagedHash> }   // el hash del store lo devuelve accept()
```

**Archivos.** `core/src/workflow/{node,artifacts,context,mod,node_kind}.rs`;
`core/src/tasks/mod.rs:60`; `core/src/pack.rs:52-57,153`; `core/src/manifest.rs:101`;
`engine/src/process_registry.rs:29`; `engine/src/check/declarations.rs:73,115-139`
(el chequeo del template se mueve a `ArtifactName` + re-validación en
`node_exec.rs:348-360`); `engine/src/template.rs:5`; `engine/src/run/node_exec.rs:264,277-279`
(`{{runner.name}}`); `engine/src/artifacts/mod.rs:48-59` (`Answers` como
kind; `questions_exec.rs:122-143` lo usa); `core/src/questions/` (shape y
`RULES` de `answers`); `engine/src/run/node_artifacts.rs:340-349`,
`questions_exec.rs:69,74,85` (`QuestionId`); `core/src/findings/mod.rs:40`,
`findings/rules.rs:26-29`; `core/src/diagnostic/{problem,artifact}.rs:171,91,190`;
`engine/src/receipt/mod.rs` (`DiagnosticCount.code: DiagnosticCode`);
`core/src/workflow/artifacts.rs:99-102` (`deny_unknown_fields` por
variante); `core/src/questions/mod.rs:56,64` (deny); `core/src/config/mod.rs:30-33`
(exporta `ScopeExpansionPermissions`); `core/src/workflow/artifacts.rs:194`
(alias según P4).

**Prerequisito cerrado.** W-02 (`ArtifactName::parse` después de renderizar; `ReservedIdentity`).

**Tests.** W-02; `an_invalid_glob_is_refused_at_parse` (core/tests/workflow.rs);
`a_pack_schema_range_that_does_not_parse_is_refused` (core/tests/pack.rs);
`answers_read_through_the_same_door_as_every_document` (core/tests/shape.rs);
`a_fresh_acceptance_cannot_be_legacy` (core/tests/artifact_ledger.rs — por
tipo, se prueba que `RecordedOrigin` no tiene `Legacy`);
`every_diagnostic_code_is_published` (core/tests/vocabulary.rs contra
`compatibility.md`).

**Cierra.** AR-D8, AR-D9, AR-D11, AR-D15, AR-D16, AR-D17, AR-D18, CO-1, CO-2, CO-3, CO-4, CO-12, CO-18, CO-19, DO-D8 (con P4).

---

## M13 · Puertas únicas en core

**Firmas.**

```rust
// core/src/workflow/read.rs
pub fn read(bytes: &str, path: &Path) -> Result<Workflow, Report>;   // parse + reglas de grafo (ids duplicados, refs rotas, scopes solapados, modos) en el mismo Report
// core/src/shape: impl Document for FindingEntry; impl Document for Withdrawal
// core/src/text.rs
pub fn counted(n: usize, noun: &str) -> String;
// engine/src/artifacts/mod.rs
pub enum Answerer { Log, Staging }
pub fn answerer(node_kind: &NodeKind, artifact: ArtifactKind) -> Answerer;   // único; close, record y verdict lo llaman; answerer(_, Answers) = Log (M26)
// engine/src/run_tools/catalog.rs
pub enum RunTool { CheckArtifact, TaskStatus, GetBlackboard, RequestScopeExpansion, PostFinding, UpdateFinding, WithdrawFinding, Submit(ArtifactKind) }
impl RunTool { pub const fn name(self) -> &'static str; pub fn describe(self) -> &'static str; pub fn schema(self) -> Schema; pub fn parse(name: &str) -> Option<Self>; }
// engine/src/run_dir.rs
pub fn manifest_path(run_dir: &Path) -> PathBuf; pub fn progress_path(..); pub fn sessions_root(..); pub fn task_worktrees(..);
```

**Archivos.** nuevo `core/src/workflow/read.rs`; modifica los 11
`yaml::parse::<Workflow>` (`cli/src/commands/list/mod.rs:91`,
`cli/src/graph.rs:39`, `cli/src/commands/test.rs:213`,
`engine/src/check/refs.rs:133`, `engine/src/run/workflow_exec/mod.rs:189`,
`engine/src/pack_audit.rs:110`, `cli/src/commands/run.rs:296-299`, …);
`engine/src/check/mod.rs:94-101,134` (reglas movidas a `read`; `check`
queda para lo que necesita config/adapters); `engine/src/run_tools/findings.rs:59,117,214`
(borra `parse_problem`); `cli/src/commands/mod.rs:296`,
`cli/src/surface/lines.rs:79`, `core/src/text.rs:118-121` (por
`text::counted`); los 25 `(s)` (auditoría 04 §3); `engine/src/artifacts/mod.rs:97-105`,
`ingest.rs:110`, `node_artifacts.rs:263`, `run_tools/submission.rs:67-76`
(`answerer`); `catalog.rs:39,61,100,109,128`, `session.rs:171-179`,
`notice.rs:119` (`RunTool`); los 15 `manifest.yaml` + `progress.md` +
`task-worktrees` + `scratch/sessions` + `"artifacts"` en `closing.rs:288`;
`engine/src/run/steps.rs:256-272` (por `canonical::derive_findings`).

**Tests.** `a_workflow_cannot_be_obtained_without_its_rules`
(core/tests/workflow.rs: un workflow con id duplicado no parsea por `read`);
`yunta_test_refuses_a_workflow_check_refuses` (cli/tests/run_flow.rs);
`a_finding_entry_reads_through_the_document_door` (core/tests/shape.rs);
`the_catalog_and_the_dispatch_name_the_same_tools` (engine/tests/run_tools.rs);
`every_path_under_the_run_dir_is_named_once` (engine/tests/run_dir.rs:
grep-test por literales).

**Cierra.** AR-D5, AR-D6, AR-D7, AR-D10, EN-D25, CLI-D7, CLI-D13 (parte), CO-8, CO-9, CO-11.

---

## M14 · PersistedDoc<T>

**Firmas.**

```rust
// core/src/persisted.rs
pub struct PersistedDoc<T> { pub schema_version: u32, pub doc: T, pub unknown: BTreeMap<String, serde_json::Value> }
impl<T: Persisted> PersistedDoc<T> { pub fn read(bytes: &[u8]) -> Result<Self, PersistedError>; pub fn write(&self) -> Vec<u8>; }
pub trait Persisted: Serialize + DeserializeOwned { const SCHEMA_VERSION: u32; const NAME: &'static str; }
pub enum PersistedError { NewerWriter { name, found, supported }, Unreadable { name, #[source] cause } }
// Manifest embebe PersistedWorkflow / PersistedConfig (tolerantes, sin deny_unknown_fields), no Workflow / ConfigLayer
```

**Archivos.** nuevo `core/src/persisted.rs`; modifica `core/src/manifest.rs:109-146`,
`engine/src/manifest.rs:19,102,129-155`, `engine/src/run/mod.rs:88`
(`read_manifest` compara versión y reporta desconocidos como diagnóstico);
`core/src/pack.rs:143-170` (`PackLock: Persisted`); `engine/src/process_registry.rs:23,157-160`;
`engine/src/lock.rs:46`; `engine/src/receipt/mod.rs:110` + `render.rs:160`;
`cli/src/commands/receipt.rs:32`.

**Tests.** `a_manifest_from_a_newer_writer_is_read_and_its_unknown_keys_are_named`
(engine/tests/manifest.rs); `a_manifest_the_binary_cannot_read_says_which_version_it_supports`;
`every_persisted_file_carries_its_version` (core/tests/persisted.rs sobre
los 5 tipos); `a_corrupt_registry_is_reported_as_corrupt_not_absent`
(engine/tests/process.rs).

**Cierra.** EN-D11, CLI-D17 (parte), CO-5, CO-6, CO-7, CO-17.

---

## M15 · Context::open_run

**Firmas.**

```rust
// cli/src/context.rs
pub struct Opened { pub run_id: RunId, pub run_dir: PathBuf, pub manifest: PersistedDoc<Manifest>, pub events: Vec<StoredEvent>, pub project: Project }
impl Context { pub async fn open_run(&self, id: &RunId) -> Result<Opened, CliError>; }
// cli/src/error.rs
CliError::RunNotFound { id: RunId, roots: Vec<PathBuf> }   // una oración: "no run `{id}` under {roots, separated}"
// cli/src/commands/stats.rs: collect_history(ctx) -> Vec<RunSummary> — una función, por open_run
```

**Archivos.** `context.rs`; `error.rs`; `status/mod.rs:31-47`, `stats.rs:50-65,125-214`,
`receipt.rs:49-62`, `cancel.rs:39-64`, `resolve_gate.rs:27-33`, `resume.rs:41-49`,
`verify.rs:40-74`, `graph.rs:70-77`, `list/runs.rs:225-251`, `gc.rs:88,159`,
`mcp.rs:266-284,358,387-395`, `drive.rs:299-310`, `run.rs:83-88`.

**Tests.** `every_command_that_opens_a_run_refuses_a_missing_one_with_the_same_sentence`
(cli/tests/run_flow.rs: `status`, `stats`, `receipt`, `verify`, `cancel`,
`gc`, `resume`, `resolve-gate` sobre un id inexistente → stderr idéntico);
`history_sees_a_run_under_the_default_state_root` (cli/tests/stats_cmd.rs).

**Cierra.** CLI-D1, CLI-D2, CLI-D3, CLI-D20, CLI-D23 (parte), CLI-D24.

---

## M16 · Un vocabulario

**Firmas.**

```rust
// cli/src/render/state.rs
pub(crate) enum RunWord { Created, Running, Paused, Finished, Failed, Cancelled, Promoted, Broken }
impl RunWord { pub(crate) fn of(phase: &RunPhase) -> Self; pub(crate) fn word(self) -> &'static str; pub(crate) fn token(self) -> &'static str /* --json */; }
impl From<RunWord> for Outcome;   // único mapeo; P5 decide Finished-con-bloqueantes
// cli/src/json.rs
pub struct RunDocument { schema_version, run_id, outcome: RunWordToken, waiting_on: Option<WaitingOnJson>, decision: Option<DecisionJson>, nodes, tasks, … }   // run/resume/status
// engine receipt: Receipt { schema_version: u32, … }
```

**Archivos.** `render/state.rs`; `status/progress.rs:79-97`,
`surface/closing.rs:94-99,157-201`, `list/runs.rs:65-85`, `drive.rs:377-425`,
`test.rs:77-101`, `surface/view.rs:41-49` (consumen `RunWord`);
`json.rs`, `drive.rs:388-406`, `status/mod.rs:163-192` (`RunDocument`);
`engine/src/receipt/mod.rs:110`; `ask/menu.rs:42-45,102`, `schema.rs:57`
(`width::`); `stats.rs:243-443` (render a `String`).

**Tests.** `a_parked_run_is_called_the_same_thing_on_every_surface`
(cli/tests/run_surface.rs: `status`, closing, `--json`, región);
`run_json_and_status_json_are_one_document` (cli/tests/status_cmd.rs);
`the_receipt_carries_the_document_version` (cli/tests/receipt_cmd.rs);
`stats_renders_to_a_string_a_test_can_read` (unit).

**Cierra.** CLI-D4, CLI-D5, CLI-D16 (con P5), CLI-D17, CLI-D22, CLI-D25.

---

## M17 · Un borde de texto

**Archivos.** `error.rs` (brazos `RunNotFound`, `NotPaused`, `NoMenu`,
`FixtureRefused`, …; `Message(String)` documentado como excepcional);
`mcp.rs:130,246,266-271,284,316,322,329-340,357-366,383-412` (tools
`-> Result<T, CliError>`; `tool_result(Result<T, CliError>) -> ToolResult`
único adaptador); `promote.rs:89,109`; `test.rs:217-419`;
`init.rs:269-282`, `new.rs:127-142` (`ask::Console::ask_line`,
`ask::choose`); `ask/mod.rs:164,311` (warnings por `Diagnostics`).

**Tests.** `an_mcp_client_answering_a_running_run_gets_the_advice_a_person_gets`
(cli/tests/mcp_flow.rs); `init_asks_through_the_console_and_honours_escape`
(cli/tests/console_interaction.rs).

**Cierra.** CLI-D10 (parte), CLI-D11, CLI-D12 (parte), CLI-D14, CLI-D26.

---

## M18 · Un camino de ejecución

**Archivos.** `test.rs:206-337` (`run_case` → `runnable` + `create_run_from`
+ `drive` con `ctx.clock`/`ctx.ids`; el sandbox es un `Checkout` propio);
`promote.rs:58-110` (`PromotionEnv { clock, ids }` desde `ctx`; `execute`
por `drive`); `mcp.rs:372-425` (borrado; `tool_resolve_gate` llama
`commands::resolve_gate::resolve(ctx, id, option)` y adapta);
`graph.rs:70`; `commands/mod.rs:339` (`check_or_refuse(&Path)`);
`check.rs:20,32-41,61` (un solo `Context::load`).

**Tests.** `yunta_test_and_yunta_run_execute_the_same_recipe`
(cli/tests/run_flow.rs: mismo workflow por ambos → mismos kinds en el log);
`a_promotion_successor_is_stamped_by_the_run_clock`
(engine/tests/promotion.rs con `FixedClock`).

**Cierra.** CLI-D10, CLI-D12, CLI-D13, CLI-D15 (parte), TE-D23.

---

## M19 · Crónica derivada

Especificación completa en [`cronica.md`](cronica.md).

**Cierra.** CLI-D8, CLI-D9, y el choque `Layout::aside`.

---

## M20 · Un arnés por capa

**Firmas.**

```rust
// testkit-core/src/log.rs
pub struct Log { … }
impl Log {
    pub fn for_run(id: &str) -> Self;                 // at = FIXED_NOW
    pub fn at(self, instant: DateTime<Utc>) -> Self;
    pub fn after(self, secs: i64) -> Self;            // avanza el reloj del builder
    pub fn event(self, payload: EventPayload) -> Self;
    pub fn node(self, node: &str, payload: EventPayload) -> Self;
    pub fn build(self) -> Vec<StoredEvent>;           // seq desde 1
}
// testkit/src/bench.rs
impl Bench {
    pub fn with_clock(self, clock: Arc<dyn Clock>) -> Self;
    pub fn with_ids(self, ids: SeqIdSource) -> Self;
    pub async fn run_sabotaged(&mut self, wf: &str, fx: &str, sabotage: impl FnOnce(&Path)) -> RunReport;
    pub async fn wake(&mut self) -> RunReport;
    pub async fn wake_with(&mut self, forge: Arc<dyn Forge>) -> RunReport;
    pub fn findings_by(&self, node: &str) -> Vec<Finding>;
    pub fn group_output(&self, group: &str) -> String;
    pub fn commit_subjects(&self) -> Vec<String>;
}
// testkit/src/bin.rs
pub fn hermetic(cmd: &mut Command, dir: &Path, home: &Path);   // YUNTA_HOME, YUNTA_ORG_CONFIG=<home>/org.yaml (vacío), HOME, USER=yunta-test, TERM=xterm-256color, env_remove NO_COLOR, GIT_CONFIG_*
impl Checkout { pub fn without_yunta_home(self) -> Self; pub fn with_org_config(self, yaml: &str) -> Self; }
// testkit-core/src/adapter.rs: request(), drain(), write_lines(), child_pid_fifo(), grandchild_pid()
```

**Archivos.** nuevo `testkit-core/src/{log,adapter}.rs`; modifica `bench.rs`,
`bin.rs`, `terminal.rs:81-88`, `checkout.rs`, `events.rs:62-70,111`
(borra `stored`; `SourceLog::record` con clock); los 10 `fn event()`
(auditoría 07 §2B), los 8 `Bench` sombra y `common/mod.rs` (§2A), los 17
grupos idénticos (§2C), los 12 `static IDS`; `run_tools.rs:547`;
`mcp_context.rs:261`; `check_keys_cmd.rs:19`; `docs_sync.rs:123-152`;
`factory_packs.rs:54`; `wait.rs` (`sleep(1ms)`).

**Prerequisito cerrado.** W-08 (`hermetic()` en `run_yunta` y `Terminal::open`).

**Tests.** Los existentes, migrados; el ratchet de M22 es el test.

**Cierra.** TE-D2, TE-D3, TE-D4 (parte), TE-D5, TE-D7, TE-D8, TE-D9, TE-D11.

---

## M21 · Cuatro propiedades

**Prerequisito cerrado.** W-09 (la propiedad tautológica renombrada a lo que
prueba, `derive_is_deterministic_from_any_prefix`, sin el `_crashed_at_k`).

**Archivos.** `crates/engine/tests/properties.rs`: generador `payload()` sobre
los 36 kinds vía los constructores de ejemplo de cada dominio (la misma
lista que `all_kinds()`), más `EventBody::Unknown`; propiedades:
`derive_is_deterministic`, `derive_is_prefix_monotonic`,
`events_round_trip_through_json`, `the_artifact_fold_…`,
`verifying_an_intact_store_…` (se conservan);
`a_run_cut_at_any_point_resumes_to_the_state_it_would_have_reached` (prefijo
→ `Bench::wake` desde ese log → `derive` final == ininterrumpido, sobre
workflows generados de 1–4 nodos bash con fixture determinista);
`redelivering_any_subsequence_of_events_changes_no_state`;
`a_node_rerun_after_a_crash_finishes_exactly_once_per_attempt`;
`any_append_sequence_verifies_and_any_flipped_byte_is_caught`
(storage/tests/store.rs, proptest sobre `verify_chain`).

**Cierra.** TE-D12, TE-D13, TE-D14, TE-D15.

---

## M22 · Ratchet que mide la regla

**Contadores nuevos en `xtask/src/smells.rs`**, cada uno una función
`count_<nombre>(root) -> usize` con la definición exacta, sobre `crates/*/src`
**y** `crates/*/tests` salvo que se indique:

| contador | cuenta | lista blanca |
|---|---|---|
| `execute_run_outside_testkit` | líneas con `execute_run(` | `crates/testkit/`, `crates/cli/src/commands/drive.rs` |
| `create_run_outside_testkit` | líneas con `create_run(` | `crates/testkit/`, `crates/cli/src/commands/run.rs` |
| `git_command_outside_git_rs` | líneas con `Command::new("git")` | `crates/engine/src/git.rs`, `crates/testkit*/src/repo.rs` |
| `system_clock_outside_boundary` | líneas con `SystemClock` o `Utc::now` | `crates/core/src/clock.rs`, `crates/cli/src/main.rs`, `crates/cli/src/context.rs` |
| `env_read_outside_boundary` | líneas con `std::env::var` | `crates/cli/src/main.rs`, `crates/core/src/config/env.rs` |
| `env_mutation_in_tests` | `env::set_var` / `env::remove_var` | — |
| `sleep_outside_mock_latency` | `tokio::time::sleep` / `thread::sleep` | `crates/adapters/src/mock/script.rs`, `crates/testkit*/src/wait.rs` |
| `event_builder_outside_testkit` | líneas `fn event(` | `crates/testkit-core/` |
| `bench_struct_outside_testkit` | `struct .*Bench` / `struct .*Clock` | `crates/testkit*/` |
| `session_request_literal_outside_plan` | `SessionRequest {` | `crates/engine/src/run/session_plan.rs`, tests de adapters |
| `wildcard_in_ledger_apply` | `_ =>` dentro de `fn apply` en `core/src/events/*/ledger.rs` y `engine/src/replay.rs` | — |
| `reason_built_by_format` | `reason: format!` / `summary: format!` / `cause: format!` / `policy_applied: format!` | — |
| `sync_fs_in_async` | `std::fs::` dentro de un cuerpo `async fn` (brace-matched) | — |
| `banned_vocabulary` | `ledger` seguido de `task`/`tasks`/`del documento` (y `plugin`, `role:`, `driver`, `backend` fuera de `crates/storage`, `subagente`) sobre `crates/**`, `docs/`, `*.yaml`, `*.json` | `CLAUDE.md`, `glosario.md`, `adrs.md` D27/D38 |
| `tense_markers` | `for now`, `not yet`, ` yet`, `today`, `now built`, `the old`, `will be`, `future ` en `//!`/`///`/`//` y en `docs/**/*.md` | `docs/design/plan-de-raiz/`, `deuda-consciente.md`, `smoke-checklist.md`, `adr/` |
| `test_files_over_500_lines` | archivos en `tests/` > 500 | — |
| `test_fns_over_50_lines` | fns en `tests/` > 50 | — |
| `numeric_const_without_adr` | `const NAME: <int> = <literal>;` sin `D\d+` en el rustdoc adyacente | según P7 |

`[workspace.lints.rust]` y `[workspace.lints.clippy]` en `Cargo.toml` con el
bloque de deny; los 5 `#![deny]` de los crate roots pasan a `[lints]
workspace = true`. `clippy.toml:3` dice la verdad.

**CI.** `release.yml` job `test` → `uses: ./.github/workflows/ci.yml`
(`workflow_call`); `ci.yml` gana job `test-macos` (`macos-latest`, `cargo test
--workspace --locked`); `for p in packs/*/; do $yunta test --dir "$p"; done`;
`concurrency: { group: ${{ github.workflow }}-${{ github.ref }},
cancel-in-progress: true }`; `timeout-minutes: 30` por job; step `cargo test
--workspace --release --locked -p yunta-core` (una vez, para el perfil);
`CONTRIBUTING.md` lista `cargo run -p xtask -- smells --check`.

**Cierra.** EN-D24, TE-D1, TE-D6, TE-D10, TE-D16, TE-D17, TE-D18, TE-D24, DO-D35–42 (ratchet).

---

## M23 · Docs atados

**Tests en `crates/cli/tests/docs_sync.rs`** (recursivo sobre `docs/`):

| test | compara |
|---|---|
| `every_yaml_example_in_the_docs_is_one_the_binary_accepts` | existente; recorre `docs/design/` desde 7-01, junto con el test de abajo que sostiene los bloques de `referencia-schema.md` cuyos `use:` (`design-review`, `qa-review`) sólo existen en ese documento |
| `the_contract_event_table_names_exactly_the_kinds_the_binary_writes` | filas de la tabla §3 de `contrato-del-run.md` (kinds entre backticks) ≡ `EventPayload::KINDS` como conjunto; cuenta de filas y de kinds ≡ las que el §0 de spec-events declara |
| `every_event_spec_section_lists_the_fields_its_payload_has` | por kind: tabla `campo \| tipo \| obligatorio` de spec-events §5.x ≡ campos del struct (nombre y `Option`) vía `schemars` sobre el schema generado |
| `the_adapter_spec_lists_every_capability_and_its_degradation` | lista §2 y tabla §5 de spec-adapter ≡ `Capability::ALL` |
| `every_built_adapter_declares_what_the_spec_says_it_declares` | §6 por adapter ≡ `<Adapter>::capabilities()` (tabla fija `capacidad \| valor`) |
| `the_tasks_spec_states_every_rule_the_engine_publishes` | ítems §3 de spec-tasks ≡ `TasksFile::RULES` (por `code`) |
| `the_contract_names_every_control_plane_tool` | §6.4 ≡ `tool_definitions()`; per-run ≡ `RunTool::ALL` |
| `the_contract_closed_sets_match_the_types` | §7.1 ≡ `CheckBuiltin`, §9 ≡ `ContextSpec`, §2.3 ≡ `InputSpec` |
| `the_reference_config_parses_and_its_workflows_check` | `referencia-schema.md` bloques: la config parsea, y cada workflow verifica en un proyecto que declara los tres que la composición `release-cycle` usa |

**`cargo xtask adr --check`**: lee `docs/design/adr/D*.md`, exige
front-matter `number,title,status,revises,revised_by`, numeración sin
huecos ni duplicados, toda cita `D\d+` en cualquier `docs/**/*.md` resuelve,
`revises`/`revised_by` recíprocos, y regenera `adrs.md` (índice: número,
título, estado, revisado-por, enlace) comparándolo byte a byte.

**Pase de corpus**: script único en `xtask` (`cargo xtask docs-unescape`,
corre una vez y se borra en el mismo PR) que deshace `\{\{`→`{{`,
`\[`→`[`, `\]`→`]`, `\|`→`|` fuera de tablas, `[x.md](http://x.md)`→`x.md`,
fences ` ```javascript ` sobre YAML/árboles → ` ```yaml `/` ```text `, y
cierra el fence de `contrato:19`. Se revisa a mano el diff.

**Prerequisito.** W-10 (pendiente): los números planos de `referencia-schema.md` (CO-14).

**Cierra.** EV-D16, AR-D12, AR-D13, AR-D14, CLI-D18, CLI-D19, AD-D22, AD-D23, AD-D24 (con P3), CO-13, CO-14, CO-15, CO-16, TE-D20, TE-D21, TE-D22, DO-D3, DO-D6, DO-D9–D44.

---

## M24 · Build-or-register

**Vicio V7.** Un comportamiento prometido por un documento, un tipo o un
campo y no construido; o construido y no conectado a la superficie que lo
consume. Las dos mitades del mismo vicio: la promesa sin mecanismo, y el
mecanismo sin consumidor.

**Regla.** Lo prometido y no construido se construye, o se retira con entrada
`A-NN` en `deuda-consciente.md` ("por qué es deuda / qué lo resolvería") y
nota `(Revisada por Dnnn: …)` en el ADR que lo describía. Lo construido y no
conectado se conecta, o se retira igual. Nunca un comentario que explique el
atajo, y nunca borrar lo inconcluso (§0.15). En ambos casos el comentario que
hoy explica el atajo se borra (`check_exec.rs:77-82`, `criteria.rs:29-36`,
`session.rs:3-10`, `context.rs` sobre extensibilidad).

**Los cinco de P3 (D167).** Se construyen el baseline eager en `create_run`
(D18, §7.2 del Contrato) y el orden de criterios aprendido del log desde
`TaskLedger` (D62): ítems 7-05 y 7-06, cada uno con su test. Se registraron
los hooks de edición (A-13, que M25 cierra), las preguntas por PR (A-14) y
las fuentes de contexto por executor (A-15).

**El inventario.** Lo que el barrido de §0.15 clasificó como inconcluso, con
evidencia en el commit que lo clasificó. Cada fila se cierra construyendo lo
que "falta" nombra, en el ítem de su mecanismo, o retirándolo con su `A-NN`
y su nota Revisada en 7-07. La columna "decisión" es la recomendación; quien
decide la confirma dejando la fila, o la cambia.

| # | qué | dónde | promete | falta | decisión | ítem |
|---|---|---|---|---|---|---|
| I-01 | `Channel::Mcp` | `core/src/events/payloads.rs:107` | Contrato §4.1, §6.4; `engine/src/human_interaction.rs:20-23`: preguntas respondibles por tool MCP | la tool MCP que implementa `HumanInteraction::ask` con `Channel::Mcp` | construir: la segunda superficie de la misma puerta, `answer_questions` (`preguntas.md` §5) | 5-06 |
| I-02 | `Task.manual_review` + `Task.justification` | `core/src/tasks/mod.rs:67,69` | D14; Contrato §5; `tasks/shape.yaml:31-34`: un nodo de auditoría juzga la completitud | el nodo de auditoría; hoy la tarea cierra mecánicamente y la justificación sólo la lee la regla de coherencia | registrar: A-18 (el juicio por rúbrica es un diseño propio y D14 lo describe como tal); los campos quedan y el shape dice qué los lee hoy | 7-07 |
| I-03 | `Task.notes` | `core/src/tasks/mod.rs:65` | `tasks/shape.yaml:22-23`, spec-tasks: "contexto para un runner sin historial" | llegar al brief de la sesión de tarea (`task_cycle/attempt.rs:245-247`) | construir: `SessionPlan.prompt` lleva `notes` debajo del título de la tarea | 3-03 |
| I-04 | `PackManifest.yunta_schema` | `core/src/pack.rs:57` | el pack declara qué schema exige | quién lo compara con `YUNTA_SCHEMA`: `pack add`/`pack update` antes de vendorear | construir: `SchemaRange` (M12) y el rechazo en `pack add` nombrando el rango y la versión | 4-01 |
| I-05 | `RunStats::{artifact_submissions, submissions_by_node, findings, findings_by_node, findings_effective}`, `Submissions`, `FindingActivity` | `engine/src/stats.rs:110-125,155-172,470-535` | un pase propio los calcula | la superficie: `RunStatsJson` y el texto de `yunta stats` | construir: `stats.rs` renderiza los dos conteos (M16) | 5-02 |
| I-06 | `EngineProcessFile.started_at` | `engine/src/process_registry.rs:29` | el instante en que el engine tomó el run | `cancel.rs` compara el arranque del pid contra `started_at` con la regla de `lock::holder_state` antes de señalar | construir: con `DateTime<Utc>` (M12) y la comparación en la cáscara (M10) | 3-05 |
| I-07 | `interactive:` del nodo hasta `HumanInteraction::ask(…, interactive)` | `engine/src/human_interaction.rs:57-64`; `cli/src/human_interaction.rs:106` | D86: dato de presentación | una superficie que lo lea | construir: la consola pregunta en el lugar sólo con `interactive: true` y `check` rechaza `interactive` sin `questions` (`preguntas.md` §2, §5) | W-11 |
| I-08 | `NodeFrame.group: Option<NodeId>` | `engine/src/view/node.rs:29-33` | el frame sabe a qué grupo pertenece un nodo | agrupar en `cli/src/surface/view.rs::node_rows` y en `status` | construir: la crónica y el frame sangran los hijos bajo su grupo (M19) | 5-05 |
| I-09 | la mitad-valor de `RunToolsHost.blackboard_members: HashMap<NodeId, Vec<NodeId>>` | `engine/src/run_tools/host.rs:30,59-71` | el host sabe los miembros de cada grupo | que `consolidate_blackboard` se los pida (`members_of(&NodeId) -> &[NodeId]`) y `node_exec.rs:136-137` deje de recalcularlos | construir: un lugar para los miembros (M04) | 2-03 |
| I-10 | `SessionRequest.adapter_settings` | `adapters/src/session.rs:66` | spec-adapter §4: el adapter recibe su config | que cada adapter lo lea en `spawn`/`resume` | construir: `open_session` lo arma (M08) y cada adapter consume el suyo por `typed_settings` (M09, 3-07) | 3-07 |
| I-11 | `MockAdapter::unconsumed(&self) -> Vec<usize>` | `adapters/src/mock/mod.rs:97` | un fixture dice qué sesiones pasan | que `yunta test` y el `Bench` fallen el caso con scripts sin reclamar | construir: un script sin reclamar falla el caso siempre —un fixture que describe sesiones que no ocurrieron miente— (M18) | 5-04 |
| I-12 | `Checkout::new()` y `_root: Option<TempDir>` | `testkit/src/checkout.rs:22,31-36,87-91` | un checkout dueño de su árbol | migrar los armados a mano (`docs_sync.rs:103`, `factory_packs_cmd.rs:45`) | construir: con `hermetic()` (M20) | 6-03 |
| I-13 | `FinalState::Promoted` | `cli/src/commands/test.rs:111-115,310` | un caso de `yunta test` puede terminar promovido | cómo un caso alcanza una promoción | construir: el caso declara `expect: promoted` y siembra la resolución del gate en el log del sandbox, que `steps.rs:186` ya lee (M18) | 5-04 |

**Cierra.** DO-D1, DO-D2, DO-D4, DO-D5, DO-D7, DO-D8, TE-D25, CLI-D16 (P5); I-01…I-13.

---

## M25 · El cerco

Especificación completa en [`cerco.md`](cerco.md): vocabulario, tipos
(`Fence`, `FenceLevel`, `Coverage`, `Fenced`, `Verdict`, `Refusal`,
`FenceHook`), las reglas del juez, el texto del rechazo, el comando
`yunta fence`, el engine (`Fence::for_session`, `fence_breach`), los tres
adapters builtin, la
muestra de ocho CLIs del mercado con las cinco reglas de escalado, archivos,
tests y ADR D172.

**Cierra.** AD-D2, AD-D7, AD-D20, AD-D24, DO-D2, A-13.

---

## M26 · Un nodo que pregunta, pregunta

Especificación completa en [`preguntas.md`](preguntas.md): vocabulario, la
regla en el tipo (`Node::asks` y las cuatro reglas de `check`), el hecho del
log (`questions_asked` par de `questions_answered`, `GateLedger::rounds`, la
derivación y la contabilidad de tokens), el cierre y la ronda (`close_node`
registra `questions_asked` y devuelve `NodeEnd::Asked`; `finish_node` único
emisor de `node_finished`; `engine::answers::record` única puerta de una
respuesta; `FinishAnswered` en el scheduler), las superficies (la consola lee
`interactive`; `answer_questions` por MCP; crónica y estado), el corte
`grill`/`brief` en el pack y los ejemplos, archivos, tests, W-11 y ADR D173.

**Prerequisito.** W-11 (pendiente): lo que hace correr el pack de referencia
de punta a punta hoy (`preguntas.md` §9).

**Cierra.** EN-D27, EN-D28, EN-D29, EV-D20, AR-D19, CO-21, DO-D45; M24 I-01
(5-06), I-07.
