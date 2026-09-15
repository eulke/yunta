# Los treinta y un mecanismos, uno por uno

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
pub const SERVER_NAME: &str = "yunta";   // M31 (fase 8) lo fija en "yunta-run"
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
pub struct NodeRecord { attempts: u32, state: NodeState, open_since: Option<(Seq, DateTime<Utc>)>, last_terminal: Option<Seq>, last_failed: Option<Seq>, last_finished: Option<Seq>, reroutes: u32, last_reroute: Option<Reroute>, runner: Option<ResolvedRunner>, tokens_closed: TokenUsage, tokens_in_flight: TokenUsage, sessions: Vec<OpenSession> /* cada una con su `fence: Option<Coverage>` */, refused: Vec<RefusedWrite>, calls: Vec<ToolCall>, last_event_at: Option<DateTime<Utc>> }   // el dueño de las sesiones: el intento las acota (D175)
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
pub struct RunState { pub run: RunLedger, pub nodes: NodeLedger, pub degradations: DegradationLedger, pub tasks: TaskLedger, pub grants: GrantLedger, pub findings: FindingLedger, pub artifacts: ArtifactLedger, pub gates: GateLedger, pub children: ChildLedger, pub unknown: UnknownKinds, pub broken: Option<ReplayError>, pub effective_findings: Vec<Finding> /* calculado una vez al final */ }
```

**Archivos.**
- nuevo: `ledger.rs` en `run`, `node`, `session`, `tasks`, `gates`, `children`; `core/src/events/meta.rs` (`EventMeta`).
- modifica: `engine/src/replay.rs` (`derive` = despacho + `effective` al final; `RunState` con los ledgers; `dedup_findings` única regla); `engine/src/findings.rs::inherited_findings` (llama `dedup_findings`); `engine/src/run_tools/blackboard.rs:26-52,64-86` (por `FindingLedger`); `engine/src/run/schedule.rs` (borra `NodeHistory` 196-303, `last_external_ref` 187-194; lee `state.nodes`, `state.gates`); `engine/src/run/gate_exec.rs` (borra `last_external_ref` 365-375, `last_approved_sha` 352-362, el conteo de attempt 502-518; lee `state.gates`/`state.nodes`); `engine/src/run/questions_exec.rs` (la ronda no cuenta intentos: lee `state.gates.pending_questions`, M26); `engine/src/run/parallel_exec.rs:39-47` (ver M07); `engine/src/live.rs` (`running_since`, `last_event_age`, `open_sessions`, `recent_tool_calls`, `in_flight_tokens`, `since_last_terminal` → lecturas de `NodeLedger`; el módulo queda como fachada o desaparece); `engine/src/stats.rs::walk_attempts` (lee `NodeLedger`); `engine/src/view/mod.rs::walk_log` (borrado: `runner`, `reroute`, `reroutes`, `children`, `degraded` vienen de `RunState`); `engine/src/view/phase.rs` (lee `RunLedger`); `engine/src/receipt/mod.rs` (7 walks → lecturas de `RunState`); `engine/src/verification_effectiveness.rs` (un `derive` por log histórico, 5 pases → lecturas); `engine/src/tasks/{mod,crossing}.rs::{prior_registrations,standing_of}` (`TaskLedger`); `engine/src/run/loop_exec/dispatch.rs:19-46,104-121` (`TaskLedger`, `GrantLedger`); `engine/src/run/loop_exec/mod.rs:413` (`GrantLedger`); `engine/src/run/prompt_exec.rs:294-338::orphaned_session` (`NodeLedger`, D175); `engine/src/run/workflow_exec/mod.rs:129-155` (`ChildLedger::open_under`); `engine/src/run/escalation.rs:259-290::pre_seeded_resolution` (`GateLedger::pre_seeded`); `engine/src/artifacts/mod.rs::RunArtifacts::of` con sus 4 callers (`gate_exec.rs:72`, `questions_exec.rs:32`, `distill.rs:159`, `promote.rs:145` leen `state.artifacts`); `engine/src/run/node_close.rs:190` (`progress.md` desde `RunState`, sin segundo replay); `engine/src/run/distill.rs` (`provenance.yaml` cuenta los findings desde `RunState::effective_findings`, deduplicados por la misma regla que el frame — hoy cuenta los vigentes sin deduplicar).

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

**Prerequisitos.** W-05 (cerrado), W-07 (cerrado).

**Tests.** W-05, W-07; `no_engine_module_reads_the_process_clock_or_env`
(engine/tests/purity.rs: grep-test sobre `crates/engine/src` por
`SystemClock`, `Utc::now`, `std::env::var`, `std::fs::` fuera de la lista
blanca `process.rs`); `every_gate_and_questions_node_carries_a_node_span`
(engine/tests/spans.rs); `a_registry_write_that_fails_is_on_the_log`
(engine/tests/degradation.rs con un `run_dir` de solo lectura).

**Cierra.** EN-D1, EN-D7 (parte), EN-D8, EN-D9, EN-D10, EN-D12, EN-D13, EN-D16, EN-D17, EN-D18, EN-D19, AD-D1, AD-D11, AD-D12, AD-D21, CLI-D15, CO-20.

**Lo que M27 termina (fase 8).** Lo que 3-05 construyó es `Supervision`
(`registry: Option`, `env: &[(String, String)]`, sin `secrets`, que viajan
por `RunEnv.secrets`), no el `Shell` firmado arriba; M27 le quita el
`Option` a `cancel` y `clock`, borra `Supervision::none` y la pareja
sincrónica de git, y le da a `init` y a `yunta test` el token que este
mecanismo les dejaba sin dar.

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
// core/src/findings/location.rs (D175): en FindingEntry.location y en events::Finding.location, el mismo tipo
pub struct Location { pub root: LocationRoot, pub path: RelativePath, pub range: Option<LineRange> }
pub enum LocationRoot { Work, Run }   // `src/lib.rs:10-14` es Work; `run:scratch/engine.json` es Run; nunca un path absoluto
// engine/src/run/ctx.rs: engine_finding(node, id, severity, title, location: Location, detail)
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
`findings/rules.rs:26-29` (la regla `EmptyLocation` y su `RuleCode` se borran:
un `Location` no puede estar vacío, y lo que la puerta rechaza es `parse` en
`findings[i].location`, como `tasks[i].id` hoy — `compatibility.md` §problem);
`core/src/events/findings/payloads.rs` (`Finding.location: Location`, D175 §3);
`engine/src/run/ctx.rs::engine_finding` (toma `Location`, así el engine no
escribe lo que la puerta rechaza), y cada sitio con su raíz (D175 §4):
`exec.rs:228` → `run:scratch/engine.json`, `exec.rs:272` → `run:objects`,
`distill.rs:200` → `run:artifacts/<nodo>/<nombre>`, `distill.rs:272,285,306` →
`Work` en `DISTILLED_DIR`, `steps.rs:83,96` → `Work` en `.`,
`loop_exec/escalate.rs:110,296` → `Work` en el primer path pedido (la lista
entera ya va en `detail`), `scope.rs::Breach::location` → `Work` en el primer
path cruzado; `docs/compatibility.md` §problem (el prefijo `run:` y que un
`location` que no lee es `parse`);
`core/src/tasks/mod.rs:52` (el doc de `Task` dice que `id` no se valida al
parsear y `TaskId` lo valida); `core/src/diagnostic/{problem,artifact}.rs:171,91,190`;
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
`compatibility.md`);
`a_location_that_does_not_read_is_a_parse_problem_at_its_path` (core/tests/shape.rs:
un `location` vacío o que sube con `..` es `parse` en `findings[0].location`, nunca
una regla); `an_engine_finding_locates_where_the_door_can_read`
(engine/tests/degradation.rs: cada `engine_finding` del run, derivado por
`derive_findings`, vuelve a leerse por `shape::accept`).

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
`engine/src/run/steps.rs:256-272` (por `canonical::derive_findings(events) ->
FindingsFile`: total, porque `Finding.location` ya es `Location` en el log, D175 §3).

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

**Sembrado.** 0-03: `banned_vocabulary` y `tense_markers` en
`xtask/src/smells/prose.rs`, con el corpus que cada uno lee y su lista
blanca; el ratchet los mide desde la fase 0 para que ningún ítem de las
fases siguientes los suba. `smells/shape.rs` separa la lectura de forma
(módulos de test, literales, cuerpos de función) de los contadores.

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
front-matter `number,title,status,revises,revised_by` —con el estado
concorde con los revisores: `accepted` sin revisor, `revised` y `retired`
con al menos uno (M29)—, numeración sin
huecos ni duplicados, toda cita `D\d+` en cualquier `docs/**/*.md` resuelve,
`revises`/`revised_by` recíprocos, y regenera `adrs.md` (índice: número,
título, estado, revisado-por, enlace) comparándolo byte a byte.

**Pase de corpus**: script único en `xtask` (`cargo xtask docs-unescape`,
corre una vez y se borra en el mismo PR) que deshace `\{\{`→`{{`,
`\[`→`[`, `\]`→`]`, `\|`→`|` fuera de tablas, `[x.md](http://x.md)`→`x.md`,
fences ` ```javascript ` sobre YAML/árboles → ` ```yaml `/` ```text `, y
cierra el fence de `contrato:19`. Se revisa a mano el diff.

**Prerequisito.** W-10 (cerrado): los números planos de `referencia-schema.md` (CO-14).

**Cierra.** EV-D16, AR-D12, AR-D13, AR-D14, CLI-D18, CLI-D19, AD-D22, AD-D23, AD-D24 (con P3), CO-13, CO-14, CO-15, CO-16, TE-D20, TE-D21, TE-D22, DO-D3, DO-D6, DO-D9–D44.

---

## M24 · Build-or-register

**Vicio V7.** Un comportamiento prometido por un documento, un tipo o un
campo y no construido; o construido y no conectado a la superficie que lo
consume. Las dos mitades del mismo vicio: la promesa sin mecanismo, y el
mecanismo sin consumidor.

**Regla.** Lo prometido y no construido se construye, o se retira con entrada
`A-NN` en `deuda-consciente.md` ("por qué es deuda / qué lo resolvería") y
nota `(Revisada por Dnnn: …)` en el ADR que lo describía. Una promesa que
contradice un invariante no es deuda: es un error de la promesa, y se
retira por decisión, sin `A-NN` (D177, M29). Lo construido y no
conectado se conecta, o se retira igual. Nunca un comentario que explique el
atajo, y nunca borrar lo inconcluso (§0.15). En ambos casos el comentario que
hoy explica el atajo se borra (`check_exec.rs:77-82`, `criteria.rs:29-36`,
`session.rs:3-10`, `context.rs` sobre extensibilidad).

**Los cinco de P3 (D167).** Se construyen el baseline eager en `create_run`
(D18, §7.2 del Contrato) y el orden de criterios aprendido del log desde
`TaskLedger` (D62): ítems 7-05 y 7-06, cada uno con su test; M28 (fase 8,
D176) reemplaza la captura al nacer por la medición en el primer despertar
y la herencia por linaje. Se registraron
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
| I-02 | `Task.manual_review` + `Task.justification` | `core/src/tasks/mod.rs:67,69` | D14; Contrato §5; `tasks/shape.yaml:31-34`: un nodo de auditoría juzga la completitud | el nodo de auditoría; hoy la tarea cierra mecánicamente y la justificación sólo la lee la regla de coherencia | retirar (D174): los dos campos, la regla `ManualReviewWithoutJustification` con su código publicado, `shape.yaml:31-34`, spec-tasks §2 y §3.6 y su sección de tareas de juicio, Contrato §5 ("excepcionalmente se marca `manual_review`"), `schemas/tasks.json`, `core/tests/tasks_rules.rs:177-192`, `strict_keys.rs:162`, `engine/tests/{submit.rs:271-281,task_cycle.rs:45}`; una tarea que un comando no puede cerrar es un `gate` | 7-07 |
| I-03 | `Task.notes` | `core/src/tasks/mod.rs:65` | `tasks/shape.yaml:22-23`, spec-tasks: "contexto para un runner sin historial" | llegar al brief de la sesión de tarea (`task_cycle/attempt.rs:245-247`) | construir: `SessionPlan.prompt` lleva `notes` debajo del título de la tarea | 3-03 |
| I-04 | `PackManifest.yunta_schema` | `core/src/pack.rs:57` | el pack declara qué schema exige | quién lo compara con `YUNTA_SCHEMA`: `pack add`/`pack update` antes de vendorear | construir: `SchemaRange` (M12) y el rechazo en `pack add` nombrando el rango y la versión | 4-01 |
| I-05 | `RunStats::{artifact_submissions, submissions_by_node, findings, findings_by_node, findings_effective}`, `Submissions`, `FindingActivity` | `engine/src/stats.rs:110-125,155-172,470-535` | un pase propio los calcula | la superficie: `RunStatsJson` y el texto de `yunta stats` | construir: `stats.rs` renderiza los dos conteos (M16) | 5-02 |
| I-06 | `EngineProcessFile.started_at` | `engine/src/process_registry.rs:29` | el instante en que el engine tomó el run | `cancel.rs` compara el arranque del pid contra `started_at` con la regla de `lock::holder_state` antes de señalar | construir: con `DateTime<Utc>` (M12) y la comparación en la cáscara (M10) | 3-05 |
| I-07 | `interactive:` del nodo hasta `HumanInteraction::ask(…, interactive)` | `engine/src/human_interaction.rs:57-64`; `cli/src/human_interaction.rs:106` | D86: dato de presentación | una superficie que lo lea | retirar (D173): del nodo, del trait y del schema; un nodo que declara `questions` ya dijo todo (`preguntas.md` §2, §5) | W-11 |
| I-08 | `NodeFrame.group: Option<NodeId>` | `engine/src/view/node.rs:29-33` | el frame sabe a qué grupo pertenece un nodo | agrupar en `cli/src/surface/view.rs::node_rows` y en `status` | construir: la crónica y el frame sangran los hijos bajo su grupo (M19) | 8-04 |
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
respuesta; `FinishAnswered` en el scheduler), las superficies (la consola;
`answer_questions` por MCP; `interactive` retirado; crónica y estado), el corte
`grill`/`brief` en el pack y los ejemplos, archivos, tests, W-11 y ADR D173.

**Prerequisito.** W-11 (cerrado): lo que hace correr el pack de referencia
de punta a punta hoy (`preguntas.md` §9).

**Cierra.** EN-D27, EN-D28, EN-D29, EV-D20, AR-D19, CO-21, DO-D45; M24 I-01
(5-06), I-07.

---

## M27 · La cáscara nace con el proceso

**Vicio V8**, la mitad que M10 dejó: el tipo `Supervision` admite «sin
dueño» —`#[derive(Default)]` en `process.rs:26` y `none()` en `:49-54`:
`registry: None`, `cancel: None`, `clock: None`— y producción lo usa ocho
veces: `engine/src/run/create.rs:274` (la suite del baseline, que M28 se
lleva) y `:398` (el git de `carried_into`); `cli/src/commands/run.rs:355`
(`prepare_worktree`), `run/detach.rs:145` (`hand_over_worktree`) y `:154`
(`release_worktree`), `drive.rs:388` (`release_worktree` en `released`),
`promote.rs:98` (`CallerInfra` de `drive_promotions`: `head_commit`,
`prepare_worktree` y el `create_run` del sucesor), `pack.rs:82` (`run_git`,
que sirven `clone_pack`, `head_commit` y `current_branch` para `pack add` y
`pack update`). La pareja sincrónica de git —`output_blocking` y
`success_blocking`, `git.rs:212-230`— lanza `Command::new("git")` sin
gobierno alguno desde `build_manifest` (`manifest.rs:203`, `git_line`; corre
también dentro de `workflow_exec` para el manifest del hijo), `init.rs:90,98`
y `test/mod.rs:201`. El token de Ctrl-C del CLI nace en `drive.rs:221`
(`cancel_on_ctrl_c`), después del worktree y del nacimiento; `RunEnv.cancel`
es `Option` (`run/mod.rs:284`) y `build_ctx` inventa un token que nadie
dispara (`exec.rs:378`); `Executing.cancel` (`drive.rs:159`) y
`PromotionEnv.cancel` (`promote.rs:43`) son `Option` y `yunta test` pasa
`None` (`test/case.rs:191,313`); `AttemptEnv.clock` es `Option<&dyn Clock>`
(`task_cycle/mod.rs:242`) y el literal de `task_cycle/mod.rs:282-287` lo
desenvuelve; el reloj de respaldo `SystemClock` vive en `process.rs:45`, fuera
de `clock.rs`/`main.rs` (§0.10). Evidencia: L-91 (§11); EN-D30, EN-D31,
EN-D32, CLI-D27 (§12).

**Regla.** Ningún subproceso de producción nace sin `Supervision`, y una
`Supervision` siempre tiene token y reloj: los dos nacen con el proceso —el
CLI arma uno por invocación, el engine lo recibe en `RunEnv`— y bajan por
parámetro hasta el spawn. El registro es opcional porque sólo un run tiene
uno. El CLI arma la interrupción del proceso al cargar su `Context`, una
vez por invocación, desde una fuente inyectada; todo comando la ve por el
mismo getter, y los que esperan —`cancel`, `mcp`— la observan. Una
cancelación es un hecho tipado en toda puerta que spawnea: llega al log
como `run_paused { cancelled by user }`, nunca como un `Err` que se
escapa. La línea «interrupt received» la escribe quien mira el run, por la
puerta que ya usa.

**Firmas.**

```rust
// engine/src/process.rs — `#[derive(Clone, Copy)]`, sin Default
pub struct Supervision<'a> {
    pub registry: Option<&'a ProcessRegistry>,   // sólo un run tiene uno
    pub cancel: &'a CancellationToken,           // antes Option
    pub env: &'a [(String, String)],
    pub clock: &'a dyn Clock,                    // antes Option
}
impl<'a> Supervision<'a> {
    /// A supervision outside any run: the caller's token and clock, no registry and no env overrides — what a CLI command and a test spawn under.
    pub fn outside_any_run(cancel: &'a CancellationToken, clock: &'a dyn Clock) -> Self;
    pub fn with_env(self, env: &'a [(String, String)]) -> Self;
}
// se borran: `Default` del derive, `Supervision::none()`, `Supervision::clock()` y su `SystemClock`; `worktree/mod.rs:176,222,303,466` pasan de `.clock()` a `.clock`

// engine/src/git.rs — la pareja sincrónica se borra; `build_manifest`, `init` y `yunta test` usan `output`/`success`
pub async fn build_manifest(workflow: &Workflow, config: &ConfigLayer, repo: &Path, workflow_dir: &Path, inputs: &HashMap<InputName, String>, supervision: Supervision<'_>) -> Result<Built, ManifestError>;   // `git_line` por `git::output`
// engine/src/run/create.rs — la supervisión del llamador es infraestructura, como storage y clock
pub async fn create_run(params: CreateRunParams<'_>, storage: &AsyncStorage, supervision: Supervision<'_>) -> Result<PathBuf, RunError>;   // un reloj por nacimiento: `RunLog::new(storage, run_id, supervision.clock, …)`
pub struct CallerInfra<'a> { pub storage: &'a AsyncStorage, pub ids: &'a dyn IdSource, pub supervision: Supervision<'a> }   // `ids.mint_run_id(supervision.clock.now())`
//   workflow_exec/mod.rs:372 pasa `ctx.supervision(cancel)` (el token del nodo); run/promote.rs:126 pasa `supervision` de `CallerInfra`
// engine/src/run/mod.rs — la cancelación es un hecho tipado en toda puerta que spawnea
pub enum RunError { /* … */ Cancelled, /* … */ }   // lo que `create_run` y `prepare_worktree` devuelven cuando su git fue detenido por el token (`GitError::Cancelled`, que `git::stopped` produce)
//   workflow_exec/mod.rs:317-335 y :372-385 mapean `RunError::Cancelled` a `node_exec::cancelled_end(ctx, node)`: el padre escribe `run_paused { cancelled by user }` y el CLI corre `released()`
//   worktree::prepare_worktree deshace lo que empezó cuando su git fue cancelado: el worktree a medio agregar y su rama
// engine/src/run/mod.rs
pub struct RunEnv<'a> { /* … */ pub cancel: &'a CancellationToken, /* … */ }   // antes Option; `build_ctx` clona, no inventa
// engine/src/task_cycle/mod.rs — «baja por parámetro hasta el spawn», también en el ciclo de tareas
pub struct AttemptEnv<'a> { pub adapter, pub node, pub cwd, pub max_retries, pub budget, pub memo, pub history, pub supervision: Supervision<'a> }   // reemplaza `registry`, `clock` y el parámetro `cancel` de `run_task`
//   loop_exec/dispatch.rs:115-125 la arma con `ctx.supervision(cancel)`; el literal de mod.rs:282-287 se borra y un criterio corre con los `subprocess_vars` del run
// engine/src/run/ctx.rs:105 — `supervision(&self, cancel)` sin cambio de forma: `cancel` y `clock` dejan de envolverse en Some

// cli/src/interrupt.rs (nuevo)
/// What trips the invocation's cancellation, in two stages: the first Ctrl-C stops the work, the second aborts what stopping still holds and hands the signal back to the process default (D181). Ctrl-C on a real command, nothing on a test's, the server's own on a `yunta mcp` request. A value the composition root is handed, never a process global.
pub(crate) struct Interrupt { stop: CancellationToken, abort: CancellationToken, listener: Option<JoinHandle<()>> }   // el handle se conserva (M10)
impl Interrupt {
    /// Installs the SIGINT stream synchronously — `tokio::signal::unix::signal(SignalKind::interrupt())` — so the handler exists when `Context::load` returns, and spawns the listener that trips the stages.
    pub(crate) fn ctrl_c() -> std::io::Result<Self>;
    pub(crate) fn never() -> Self;
    /// The same two tokens for a `Context` derived from this one — a sandbox, a request the server handles — with no listener of its own: only the root installs the stream.
    pub(crate) fn shared(&self) -> Self;      // los dos `CancellationToken` clonados, `listener: None`
    pub(crate) fn stop(&self) -> &CancellationToken;
    pub(crate) fn abort(&self) -> &CancellationToken;
}
// cli/src/context.rs
pub struct Context { /* … */ pub env: yunta_core::Env /* `process_env()` una vez, en `resolve_in` */, interrupt: Interrupt }
impl Context {
    pub fn load() -> Result<Self, CliError>;                                          // `Interrupt::ctrl_c()?`
    pub fn resolve_in(cwd: PathBuf, interrupt: Interrupt) -> Result<Self, CliError>;   // `Context::sandboxed` y los seis `resolve_in` de `mcp.rs` (:299,:308,:354,:381,:408,:425) arman el suyo con `self.interrupt.shared()`; el listener vive en el `Context` que `load` devolvió
    /// The token the work of this invocation answers to: the first Ctrl-C.
    pub fn cancellation(&self) -> &CancellationToken;
    /// The supervision the work runs under: no registry, the first stage, this invocation's env and clock.
    pub fn supervision(&self) -> Supervision<'_>;    // Supervision::outside_any_run(self.cancellation(), &self.clock).with_env(&self.env.subprocess_vars)
    /// The supervision for what gives a take back once the work stopped — `released`, `hand_over` — answering only to the second Ctrl-C.
    pub fn teardown(&self) -> Supervision<'_>;       // Supervision::outside_any_run(self.interrupt.abort(), &self.clock)
}
//   drive.rs:101, promote.rs:102 y cli.rs:428 dejan de leer `process_env()`: `Executing.ambient` y `PromotionEnv` toman `&ctx.env`.
//   `cancel.rs` espera la muerte del engine con `select!` sobre `ctx.cancellation()`; `mcp.rs` cierra el servidor cuando dispara.
//   `released` (drive.rs:379) y `hand_over_worktree` (detach.rs:145) corren bajo `ctx.teardown()`: lo que devuelve una toma no puede responder al token que la pidió.
// cli/src/commands/mod.rs:52 — `cancel_on_ctrl_c(diagnostics)` se borra; `drive::watch` toma `ctx.cancellation()` y lanza el watcher que escribe
//   "interrupt received — stopping the run (sessions get interrupt, then kill)" por la `Diagnostics` de la superficie o por stderr sin ella (`--json`), como hoy; su handle vive en `Watching`.
//   `Watching`, `watch` y `close` se mudan a `cli/src/commands/drive/watch.rs`: `drive.rs` está a ocho líneas del techo y el contador de archivos no sube.
// cli/src/commands/drive.rs — `Executing.cancel: &CancellationToken`; `released` (:379) usa `settling.ctx.teardown()`
// cli/src/commands/promote.rs — `PromotionEnv { ctx: &'a Context, storage, adapters, forge, human_interaction, observer }`: un dueño para reloj, token, env y fence_hook, como `Driving` y `Settling`; `:98` usa `env.ctx.supervision()`, `:114` `env.ctx.cancellation()`
// cli/src/commands/run.rs:355,384; run/detach.rs:154 — `ctx.supervision()`; detach.rs:145 — `ctx.teardown()`
// cli/src/commands/pack.rs::{add, update} — construyen `Context::load()`; `pack.rs::{clone_pack, head_commit, current_branch, run_git}` toman `Supervision<'_>`
// cli/src/commands/init.rs:90,98 y test/mod.rs:201 — `git::output`/`git::success` con `ctx.supervision()`
// cli/src/commands/mcp.rs — el servidor observa `ctx.cancellation()`: un SIGINT cancela lo que está naciendo y cierra el servidor, como hoy lo cerraba el proceso
// cli/src/commands/test/case.rs:151,191,313 — `Interrupt::shared` para el `Context` del sandbox y `cancel: ctx.cancellation()`

// testkit/src/owner.rs (nuevo; L-113)
/// What a test's subprocesses answer to: a token the test may trip and the fixed clock — the supervision outside any run, owned so the borrows have somewhere to live.
pub struct Owner { cancellation: CancellationToken, clock: FixedClock }
impl Owner { pub fn new() -> Self; pub fn cancellation(&self) -> &CancellationToken; pub fn supervision(&self) -> Supervision<'_>; }   // `Supervision::outside_any_run(&self.cancellation, &self.clock)`
// testkit/src/bench/mod.rs — `cancel: CancellationToken` (nace con el bench; `with_cancel` lo reemplaza); `Bench::supervision(&self) -> Supervision<'_>` = `Supervision::outside_any_run(&self.cancel, self.clock.as_ref()).with_env(..)` para lo que el bench spawnea al nacer y al congelar
// testkit/src/bench/driving.rs:259 — `create_run(.., self.supervision())`; `:326` — `cancel: &self.cancel`; `freeze` pasa `self.supervision()` a `build_manifest`
```

**Archivos.** Nuevo: `cli/src/interrupt.rs`, `testkit/src/owner.rs`,
`testkit/src/stubs.rs` (`git()`, el stub que M31 amplía) y
`testkit/stubs/git_stub.sh`. Modifica: `testkit/src/checkout.rs`
(`Checkout::with_stubs`), `cli/src/commands/cancel.rs`, `cli/src/cli.rs:428`, `engine/src/process.rs`, `engine/src/git.rs`, `engine/src/manifest.rs`,
`engine/src/worktree/mod.rs` (sólo `.clock()` → `.clock`; §8 lo conserva),
`engine/src/run/{create.rs, ctx.rs, exec.rs, mod.rs, workflow_exec/mod.rs,
promote.rs, loop_exec/dispatch.rs}`, `engine/src/task_cycle/mod.rs`,
`engine/src/lib.rs`, `cli/src/{context.rs, pack.rs, main.rs}`,
`cli/src/commands/{mod.rs, drive.rs, run.rs, run/detach.rs, promote.rs,
pack.rs, init.rs, mcp.rs, test/mod.rs, test/case.rs}` (y `drive/watch.rs`,
nuevo), `docs/design/adr/D181-*.md` (nuevo),
`testkit-core/src/lib.rs`, `testkit/src/bench/{mod.rs, driving.rs}`, los
tests que construían una `Supervision` a mano —`engine/tests/process.rs:41,79-84,104,155-160`,
`engine/src/tasks/crossing.rs` ×5, `engine/tests/{promotion.rs ×6, scope.rs ×9,
worktree.rs ×20, scope_expansion.rs ×9, task_cycle.rs ×10}`, y todo `RunEnv`,
`CallerInfra` o `build_manifest` literal de un test— por `Owner`;
`engine/tests/purity.rs` (deja de exceptuar `process.rs`); el texto que
describía la ausencia: `create.rs:263-267,395-397`, `exec.rs:344-349`,
`process.rs:21-25,35-39,58-66`, `run/mod.rs:266-270`, `drive.rs:91-92,201-203`,
`purity.rs:11-14,93-101`, `git.rs:210-211,221`; `xtask/smells.baseline`
(`system_clock_outside_boundary` baja por medición). Borra: `Supervision::none`,
`Supervision::clock`, `Default` del derive, `git::{output_blocking,
success_blocking}`, `manifest::git_line` sincrónico, `commands::cancel_on_ctrl_c`.
Lo que M10 firmó como `Shell` (`registry` sin `Option`, `&Env`, `secrets`)
queda en la forma que 3-05 construyó: `Supervision` con `registry: Option`
—un comando fuera de un run no tiene registro— y sin `secrets`, que ya
viajan por `RunEnv.secrets`; M10 decía que `init` y `yunta test` corren «sin
token de cancelación», y desde acá lo tienen.

**Prerequisitos.** Ninguno.

**Tests.** `engine/tests/process.rs`:
`a_supervision_outside_any_run_still_answers_to_its_token` (rojo: el tipo no
tiene `outside_any_run` ni exige token);
`engine/tests/cancel.rs`: `a_cancelled_token_stops_the_git_a_birth_runs`
(rojo: `create_run` no toma supervisión y el git de `carried_into` corre
igual) —un run que nace teniendo un documento de tareas con un `done` que
cruza, con el token ya disparado: `create_run` devuelve
`RunError::Cancelled`, y como `birth_registrations` corre antes del
directorio y de `run_created`, no queda run alguno—;
`engine/tests/workflow_compose.rs`:
`a_parent_whose_child_birth_is_interrupted_pauses_as_cancelled_by_user`
(rojo: hoy el `Err` del nacimiento sale de `execute_run` sin `run_paused`;
verde: el log del padre termina en `run_paused` y el hijo no tiene
`run_created`); `engine/tests/purity.rs`:
`no_engine_module_reads_the_process_clock` sin la excepción de `process.rs`
(rojo hasta borrar el respaldo) y `no_engine_module_spawns_git_outside_the_shell`
(grep sobre `crates/engine/src` por `Command::new("git")` fuera de `git.rs`,
rojo por la pareja sincrónica); `cli/src/interrupt.rs` (unit):
`a_fired_interrupt_cancels_the_contexts_token` (una fuente falsa, disparada,
y `ctx.cancellation().is_cancelled()`); `cli/tests/run_flow.rs`:
`an_interrupt_while_the_worktree_is_being_prepared_kills_the_git_and_frees_the_lock`
(el stub `git` del testkit en el `PATH` del comando —`Checkout::with_stubs`—
delega en el git real salvo en `worktree add`, donde publica su pid por
rename y bloquea; tras SIGINT ese git está `Liveness::Dead`, el lock del
checkout no tiene holder y no hay `run_created`; rojo: hoy el git sobrevive
en su propio grupo y el lock nombra un pid muerto),
`an_interrupt_during_a_resume_pauses_the_run_as_cancelled_by_user` (hoy
`resume` no armaba nada antes de `drive`),
`yunta_cancel_on_a_detached_run_pauses_it_as_cancelled_by_user` (hoy el
`resume` desacoplado no tiene listener y `yunta cancel` lo mata a SIGKILL
tras esperar el timeout),
`an_interrupt_during_yunta_cancel_stops_the_wait` y
`a_second_interrupt_aborts_a_release_the_first_left_running` (con la
fuente falsa: el primer disparo deja `teardown` viva, el segundo la
cancela).

**Cierra.** EN-D30, EN-D31, EN-D32, CLI-D27; L-91 (git, worktree y
manifest; la suite es de M28).

**Decisión.** D181: dos Ctrl-C —el primero detiene el trabajo, el
segundo aborta lo que detenerlo todavía sostiene y devuelve la señal al
proceso—, el mismo «interrupt, then kill» que el repo ya aplica a las
sesiones (L-108).

**Encastre.** Dos formas de una cancelación, cada una donde corresponde:
un nacimiento la devuelve a su llamador como `RunError::Cancelled` —no
tiene log propio todavía—, y un paso de un run vivo devuelve `Ok(())` sin
evento, porque la vuelta siguiente del loop ve el token disparado y escribe
`run_paused` (M28, `steps::measure_baseline`). M10: `spawn_governed` se
conserva; `Supervision` es el
`Shell` de M10 en la forma que 3-05 le dio, ahora sin `Option` en token y
reloj. M28: la suite del baseline corre bajo `RunCtx::root_supervision`
porque la mide `execute_run`, no el nacimiento. M20: `Bench` ya llevaba
`with_cancel`; el token existe siempre, y `Owner` es la infraestructura que
antes cada test copiaba. M22: `system_clock_outside_boundary` baja. §8:
`spawn_governed`, `worktree` (integridad, `branch -d`, guard en `Drop`),
`Env::subprocess_vars`, `wait.rs`, `Terminal` sin cambio de conducta.

---

## M28 · Un baseline por linaje

**Vicio V2** (una pregunta —qué pasaba antes— respondida por run y no por
linaje) y **V7** (D61 y Contrato §7.2 ¶2 prometen una memoización sin
construir). Un hijo `kind: workflow` mide de nuevo al nacer
(`workflow_exec/mod.rs:372` → `create.rs:249`) sobre un árbol que el padre
ya tocó, así que una regresión del padre le queda invisible al compare del
hijo; un sucesor de promoción mide de nuevo (`run/promote.rs:126`, el mismo
`create_run`); `yunta run --detach` y `run_workflow` pagan la suite antes de
devolver el id (L-93); un nacimiento interrumpido deja un run sin
`baseline_captured` que ningún despertar repone; y como el nacimiento
escribe más de un evento, `start()` lee el primer despertar de todo run
nacido con una suite o con documentos como una reanudación
(`exec.rs:214-218`, «anything beyond `run_created` means a previous
invocation worked on this run»), escribe `run_resumed` y verifica una
historia que no existe. `baseline_compare` y `coverage_gate` corren su
comando cada vez (`check_exec.rs:103,163`) y el `Memo` del run sólo sirve
a los criterios (`task_cycle/criteria.rs:17-25`). El hecho está en el
dominio equivocado: `baseline_captured` «carries no node» y vive en
`NodeEvent` (`node/kinds.rs:9`), `NodeLedger` no lo pliega (`is_audit`,
`node/kinds.rs:61`) y cada lector lo busca con un pliegue propio
(`check_exec.rs:136`, `receipt/mod.rs:348`). `.yunta/config.yaml:29-30`
de este repo declara `cargo test --workspace` y ninguno de sus dos
workflows (`lint-fix`, `run-tasks`) ni los dos del pack `starter` que CI
verifica bajo la misma config comparan. Evidencia: L-91, L-92, L-93,
L-107 (§11); EN-D33, EN-D34, EN-D37, CLI-D31, DO-D48, DO-D49 (§12).

**Regla.** El baseline es del linaje. La raíz lo mide una vez, en su
primer despertar, antes de su primer nodo; medirlo es una decisión del
scheduler y un paso de la cáscara, no un paso ad hoc del despertar. Todo
run que nace de otro —hijo `kind: workflow`, sucesor de una promoción—
nace teniendo la medición de la raíz, en su propio log, con origen
`inherited` que nombra a la raíz; su propia config no se consulta y los
bytes de la suite quedan con el run que midió. El hecho es del run:
`RunEvent::BaselineCaptured`, plegado por `RunLedger`, y toda lectura sale
del estado. Toda `baseline_compare` del linaje compara contra esa única
medición; dentro de una invocación reutiliza, por el `Memo` de §5.4, el
resultado de la suite sobre un árbol que no cambió desde otra comparación
y lo dice en su cierre; `coverage_gate` mide cada vez, porque su veredicto
lee la salida y no el código. Se mide si y sólo si la config declara
`baseline.suite`; `yunta check` avisa cuando ni el workflow ni los
workflows que compone comparan. Un primer despertar se reconoce por lo que
el log tiene, no por cuántos eventos tiene.

**Firmas.**

```rust
// core/src/events/run/{kinds.rs, payloads.rs, ledger.rs, happening.rs} — el kind cambia de dominio; el wire no cambia un byte
pub enum RunEvent { Created(..), PromotionSignaled(..), Paused(..), Resumed(..), Finished(..), BaselineCaptured(BaselineCapturedPayload) }   // is_audit: false — mueve `RunLedger`
pub struct BaselineCapturedPayload {
    pub command: String,
    pub results: BaselineResults,
    pub hash: ContentHash,
    /// Whose measurement this is. A log written before the field reads `measured`.
    #[serde(default)]
    pub origin: BaselineOrigin,
}
#[derive(Default)] #[serde(tag = "type", rename_all = "snake_case")]
pub enum BaselineOrigin { #[default] Measured, Inherited { run: RunId } }   // `run` es la raíz que midió, nunca el padre inmediato
impl RunLedger {
    /// The measurement this run holds — its own, or the one it was born holding. `None` for a lineage whose root declared no suite.
    pub fn baseline(&self) -> Option<&BaselineCapturedPayload>;
    /// Whether the run's own events say an invocation woke it: a pause, a resume, or a `baseline_captured { origin: measured }`.
    pub fn woken(&self) -> bool;
}
impl RunState {
    /// Whether an invocation already woke this run: the run's own events say so, nodes an invocation that died without pausing left behind, or a replay that stopped. What separates a first wake from a resume — a birth writes any number of events, the measurement a run is born holding among them. Un ledger por dominio no ve otro dominio, así que la pregunta vive donde se componen (L-112).
    pub fn woken(&self) -> bool;
}
pub enum run::happening::Happening { /* … */ BaselineCaptured(BaselineOrigin) }   // words.rs:101: `baseline measured` | `baseline inherited from run <root>`
//   node/{kinds.rs:9,39,61, happening.rs:40,95,121, ledger.rs:354}, tasks/ledger.rs:142, replay.rs:251, wire.rs:88,134, testkit-core kinds.rs:36 pierden o mueven el brazo; core/tests/events.rs:25 y `all_kinds()` siguen al dominio nuevo

// engine/src/run/schedule.rs — decidir es puro
pub struct Policy { /* … */ pub baseline_suite: Option<String> }     // Policy::of: manifest.config.baseline.as_ref().map(|b| b.suite.clone())
pub enum Decision { /* … */ MeasureBaseline { suite: String }, /* … */ }
//   decide(): antes de cualquier `Execute`, `policy.baseline_suite` es `Some` y `state.run.baseline()` es `None` → `MeasureBaseline`; un run que nació teniéndola o que ya midió nunca la ve
// engine/src/run/baseline.rs — ejecutar es la cáscara, y medir vive con lo demás del baseline
/// Measures the suite the scheduler decided this run owes, under the run's own supervision: keeps its output under `baseline/`, records `baseline_captured { origin: measured }`. A suite the cancellation stops records nothing and answers `Ok(())` — a step is not a node, so there is no `cancelled_end` to write; the loop's next turn sees the token fired and pauses the run.
pub(super) async fn measure(ctx: &RunCtx<'_>, suite: String) -> Result<(), RunError>;   // exec.rs:128 gana el brazo `Decision::MeasureBaseline { suite } => baseline::measure(&ctx, suite).await?`
// engine/src/run/exec.rs:214-218 — `if view.state.woken() { resume(&ctx, &view).await?; }` reemplaza `events.len() > 1`

// engine/src/run/baseline.rs (nuevo) — lo que un nacimiento hereda y lo que la suite deja
/// What a run born of another holds: the root's measurement, named by the run that took it. Pure: the parent's own or inherited capture, with the root resolved.
pub struct BirthBaseline { pub measured_by: RunId, pub command: String, pub results: BaselineResults, pub hash: ContentHash }
pub fn inherited(from: &RunId, from_state: &RunState) -> Option<BirthBaseline>;   // `from` cuando el padre midió; `origin.run` cuando heredó
/// Keeps everything the suite wrote under the measuring run's `baseline/` (`run_dir::baseline_capture`), the file the receipt's hash names. A run born holding a measurement has no `baseline/`: the bytes are under the run its origin names.
pub(super) async fn keep_capture(run_dir: &Path, output: &[u8]) -> Result<(), RunError>;   // mudada desde create.rs
// engine/src/run/create.rs
pub struct CreateRunParams<'a> { /* … */ pub baseline: Option<&'a BirthBaseline> }
//   `None`: la raíz — nace sin medición. `Some`: nace teniéndola: `baseline_captured { origin: inherited { run: measured_by } }` después de los birth artifacts. `capture_baseline` se borra.
//   workflow_exec/mod.rs:372 — `baseline: baseline::inherited(ctx.run_id, &derive(&events)).as_ref()` sobre los `events` ya cargados en :132; run/promote.rs:126 — sobre el estado del predecesor;
//   cli run.rs:385, cli promote.rs:222 (test), testkit driving.rs:260 — `baseline: None`.

// engine/src/task_cycle/criteria.rs — el memo del run sirve a un lector más
impl Memo {
    /// The exit code of `cmd` on `cwd` as this invocation already knows it, or by running it now under `supervision`: what a criterion and a `baseline_compare` share.
    pub(crate) async fn exit_code(&self, cmd: &str, cwd: &Path, supervision: Supervision<'_>) -> Result<Memoized, TaskCycleError>;
}
pub struct Memoized { pub exit_code: i32, pub reused: bool }
//   el lazo de criterios sigue con su huella compartida (L-110); check_exec::execute_baseline_compare compara `ctx.run_view().await?.state.run.baseline()` contra `ctx.memo.exit_code(..)` y cierra con
//   "no regression vs baseline (exit 0)" o "no regression vs baseline (exit 0, reused: same tree since an earlier compare)"; coverage_gate sigue por `run_command`.

// engine/src/receipt/mod.rs:66-73 — `BaselineSummary { suite, hash, compared, regressions, origin: BaselineOrigin }`, aditivo, `Receipt::SCHEMA_VERSION` queda en 1;
//   `baseline_summary` lee `state.run.baseline()` del estado que ya deriva; receipt/render.rs:74-83 imprime «(suite `{}`, hash `{}`, measured by run {})» cuando el origen es heredado.

// engine/src/check/warning.rs
CheckWarning::BaselineNeverCompared { suite: String }
//   "config declares `baseline.suite` (`{suite}`) and no node of this workflow or of the workflows it composes is a `baseline_compare`: run on its own, this workflow measures the suite before its first node and nothing reads the measurement — add the check, or drop the suite"
// engine/src/check/refs.rs — la caminata contesta lo que sólo ella puede contestar; `check` y `check_warnings` siguen sin leer archivos
pub struct RefsCheck { pub errors: Vec<CheckError>, pub warnings: Vec<CheckWarning> }
pub fn check_workflow_refs(workflow: &Workflow, config: &ConfigLayer, repo_root: &Path, origin: &WorkflowOrigin) -> RefsCheck;
//   `walk_workflow_refs` gana `compares: &mut bool`, cierto cuando algún workflow recorrido tiene `NodeKind::Check(CheckBuiltin::BaselineCompare)`; al final, `config.baseline.is_some() && !compares` → el warning.
//   cli/src/commands/check.rs:52-55 y cli/src/commands/mod.rs:355-364 imprimen `refs.warnings` junto a los demás.
```

**Decisión.** D176 revisa D18 («snapshot al abrir el run»: el primer
despertar), D61 (`baseline_compare` reutiliza por `Memo`; `coverage_gate`
mide cada vez) y D167 («`create_run` captura»: el nacimiento hereda, el
despertar mide), y con ello la mitad de baseline de P3 (§6). Contrato §7.2
reescrito —¶1 «antes de su primer nodo, en su primer despertar … un run que
nace de otro nace teniendo la medición de la raíz», ¶2 «un linaje mide la
suite una vez; cada `baseline_compare` la vuelve a correr sobre su árbol,
y una invocación no la repite sobre un árbol que no cambió; coverage se
mide en cada gate»—; §2 línea 16 («snapshot de suite al primer
despertar»), §2 línea 66 y §3 línea 75 (la fila de `baseline_captured`,
que pasa al dominio del run); README del plan §3 (la tabla de dominios:
`baseline_captured` sale de `node` y entra en `run`). spec-events §5.3
gana la fila `origin` —`{type: measured}` \| `{type: inherited, run}`,
Oblig. «sí», «un log sin el campo se lee `measured`»—; `schemas/events.json`
por `xtask schema`. L-107 (§11): `baseline:` sale de `.yunta/config.yaml`
de este repo —ningún workflow del repo ni del pack `starter` compara, así
que con la suite CI avisaría cuatro veces por corrida, y `yunta test` sobre
este repo deja de medir nada—.

**Archivos.** Nuevo: `engine/src/run/baseline.rs`, `docs/design/adr/D176-*.md`.
Modifica: `core/src/events/run/{kinds.rs, payloads.rs, ledger.rs, happening.rs}`,
`core/src/events/node/{kinds.rs, payloads.rs, happening.rs, ledger.rs}`,
`core/src/events/tasks/ledger.rs:142`, `core/src/events/wire.rs`,
`core/schemas/events.json`, `core/tests/events.rs`, `engine/src/replay.rs:251`,
`engine/src/run/{create.rs, exec.rs, schedule.rs, steps.rs, check_exec.rs
(módulo doc :5, rustdoc :76-82, diagnóstico :94-96 — «this run holds no
baseline: its lineage's root declared no `baseline.suite`»),
workflow_exec/mod.rs, promote.rs, mod.rs, run_dir.rs:47-53}`,
`engine/src/observer.rs:21-23` (el `baseline_captured` de la raíz pasa por
`ctx.emit` y se observa; el nacimiento sólo escribe el heredado),
`engine/src/task_cycle/criteria.rs`, `engine/src/receipt/{mod.rs, render.rs}`,
`engine/src/check/{refs.rs, warning.rs}`, `engine/src/lib.rs`,
`cli/src/commands/{check.rs, mod.rs, run.rs, promote.rs}`,
`cli/src/surface/chronicle/words.rs`, `testkit-core/src/kinds.rs:36`,
`testkit/src/bench/driving.rs:30,260`, `docs/design/contrato-del-run.md`,
`docs/design/spec-events.md:148,153`, `docs/design/adr/{D18, D61, D167}`
(nota y `revised_by`), `docs/design/adrs.md`, `.yunta/config.yaml`, README
del plan §3 y §6; los tests que fijan la forma vieja:
`engine/tests/run_checks.rs:11-59,63-64,90`, `engine/tests/observer.rs:92-94`,
`engine/tests/receipt.rs:58-63,103,170,313`, `cli/tests/receipt_cmd.rs:51`,
`engine/tests/schedule.rs`, y los llamadores de `check_workflow_refs` en
`engine/tests/{check.rs, catalog.rs, pack_permissions_ceiling.rs}`. Borra:
`create.rs::capture_baseline`, `check_exec.rs::captured_baseline`, el pliegue
inline de `receipt::baseline_summary`, `NodeEvent::BaselineCaptured`.

**Prerequisitos.** Ninguno: la suite corre bajo `execute_run`, que ya
está gobernado.

**Tests.** `engine/tests/schedule.rs`:
`a_run_owing_a_baseline_is_told_to_measure_it_before_any_node` y
`a_run_born_holding_a_baseline_is_never_told_to_measure` (rojo: `Decision`
no tiene el brazo); `engine/tests/run_checks.rs`:
`a_run_born_and_not_yet_woken_holds_no_baseline` (rojo: hoy nace midiendo),
`the_first_wake_measures_the_baseline_before_any_node` (reemplaza
`a_run_captures_its_baseline_when_it_is_created`: tras `try_create` el log
no tiene `baseline_captured`; tras el primer despertar tiene exactamente
uno, después de `run_created`, sin `run_resumed` antes —rojo: hoy hay uno
al nacer y el primer despertar escribe `run_resumed`—),
`a_run_born_holding_documents_is_not_resumed_on_its_first_wake` (rojo),
`a_resumed_run_measures_nothing_again` (un stub contador por ruta absoluta
en `baseline.suite`, dos despertares, una medición),
`two_baseline_compares_on_one_tree_run_the_suite_once_and_the_second_says_so`;
`engine/tests/cancel.rs`: `a_suite_the_cancellation_stops_leaves_no_measurement_and_the_run_pauses`
(la sincronización es el registro: `wait_until_async` hasta que
`engine.json` liste el grupo de la suite, y recién entonces el token);
`engine/tests/workflow_compose.rs`:
`a_child_is_born_holding_the_roots_measurement` (rojo: hoy mide de nuevo),
`a_child_born_holding_a_baseline_is_not_resumed_on_its_first_wake`,
`a_grandchild_names_the_root_and_not_its_parent`,
`a_childs_baseline_compare_sees_a_regression_its_parent_made` (rojo: hoy
el hijo mide sobre el árbol roto y pasa), `a_lineage_measures_once`
(el stub contador por ruta absoluta: hoy dos, después una);
`engine/tests/promotion.rs`: `a_successor_is_born_holding_its_predecessors_measurement`;
`engine/tests/check.rs`: `a_suite_nothing_compares_is_a_warning` (con un
workflow de pack verificado bajo una config de proyecto que declara la
suite, la forma que CI ejercita), `a_suite_a_composed_workflow_compares_is_not`;
`core/tests/events.rs`: `a_baseline_captured_written_without_an_origin_reads_as_measured`
y el par de `is_audit` para el kind en su dominio nuevo;
`engine/tests/receipt.rs`: `the_receipt_names_the_run_that_measured_an_inherited_baseline`;
`cli/tests/check.rs`: `check_warns_about_a_suite_nothing_compares`.

**Cierra.** EN-D33, EN-D34, EN-D37, CLI-D31, DO-D48, DO-D49, y la forma
final de DO-D1; L-91 (la suite), L-92, L-93, L-107.

**Encastre.** M27: la suite corre bajo `RunCtx::root_supervision`
—registro, token y `subprocess_vars` del run—; `yunta cancel` la encuentra
por `engine.json`, que `build_ctx` escribe antes. M07: medir es una
`Decision` y un paso de `steps`, como todo lo que el run hace. M02/M04/M05:
el kind vive en su dominio, un ledger lo pliega, `is_audit` dice la
verdad, y ningún lector pliega por su cuenta. M03: `origin` es un campo
con `default`, tolerante en lo persistido; `BaselineOrigin` es exhaustivo
y `all_kinds()` lo ejemplifica. M24: cierra la mitad de D61 que 7-05 dejó;
`--detach` y `run_workflow` vuelven a devolver el id sin esperar (L-93).
M19: la crónica dice `measured`/`inherited`. M23: spec-events §5.3 queda
atado por `every_event_spec_section_lists_the_fields_its_payload_has`;
§7.2 es prosa sin test. §8: `create_run`'s `tokio::fs` sin cambio;
`shape::read` y `RULES` sin cambio.
---

## M29 · Una decisión dice lo que el código hace

**Vicio V7** (la promesa sin mecanismo) y **V11** (documentación sin atar),
en el registro que 7-02 construyó. `Decision::parse` acepta `status:
revised` con `revised_by: []` —D147 fue el único de las 33 revisadas y 4
retiradas sin revisor: su nota no nombraba a nadie y el commit `fcb956e`
enmendó el cuerpo en su lugar (`--allowedTools mcp__yunta` →
`mcp__yunta__*`), así que la decisión tal como se tomó sobrevivía sólo en
git; el plan de la fase 8 (`00fd102`) registró D178 y D180, devolvió al
cuerpo lo que decidió y puso los revisores, pero el parser sigue
aceptando la combinación—. D62 (`adr/D62:11-13`) y D59 (en el título)
prometían un corto-circuito del pre-check que el Contrato repite en §5.2
(línea 171) y §5.4 (línea 206) y que contradice I6 (línea 650) y §8.7
(líneas 431-437); `pre_check` corre todos los criterios
(`task_cycle/criteria.rs:183-191`) pero devuelve la primera sorpresa en el
orden aprendido (`criteria.rs:199-217`): con un criterio trivial y un guard
roto a la vez, qué variante y qué `cmd` vuelven depende del orden, y el
veredicto se aplana a `TaskOutcome::Blocked { reason: String }`
(`mod.rs:156-160,328-340`) antes de llegar a nadie. Las cuatro retiradas
dicen «Retirada por» en el cuerpo y el índice generado dice «Revisada por»
para todas (`xtask/src/adr.rs:130-140`). Evidencia: L-87, L-95, L-105
(§11); EN-D35, DO-D46, DO-D47 (§12).

**Regla.** Una revisión es una decisión: el cuerpo de un ADR nunca se
enmienda; lo que cambia lo dice un ADR nuevo que lo revisa, el revisado
lleva la nota «(Revisada por Dnnn: …)» y el front-matter lleva la
reciprocidad. El estado y los revisores son un solo hecho que el tipo no
deja disentir: `accepted` no tiene revisor; `revised` y `retired` llevan al
menos uno, y `Decision::parse` rechaza el archivo que diga otra cosa. Una
promesa de comportamiento que el código contradice se resuelve como M24
lo distingue: la que contradice un invariante se retira por decisión; la
sólo no construida es deuda `A-NN`. Un veredicto sobre un conjunto es una
función pura de lo que corrió, nombra todo lo que encontró en el orden de
declaración, y viaja tipado hasta el borde que lo dice.

**Firmas.**

```rust
// xtask/src/adr/decision.rs — el front-matter sigue siendo de cinco campos; `revised_by` se pliega en el estado al parsear
pub enum Status { Accepted, Revised { by: NonEmpty<u32> }, Retired { by: NonEmpty<u32> } }
impl Decision { pub fn revisers(&self) -> &[u32]; }        // lo que `reciprocals` y `index()` leen
//   `Decision::parse` rechaza «D147: `status: revised` and `revised_by` names no decision» y «`status: accepted` and `revised_by` names D178», como rechaza sus hermanos;
//   `index()` escribe «Retirada por» para `Retired` y «Revisada por» para `Revised`, por `match` sobre el mismo enum.

// engine/src/task_cycle/mod.rs — el veredicto es una función de lo que corrió
/// A criterion the pre-check found wrong before any work: the criteria need fixing, not the task.
pub enum Surprise { TrivialCriterion { cmd: String }, BrokenGuard { cmd: String } }
/// Everything the pre-check found, in the order the task declares its criteria; empty when every non-guard is red and every guard green. A function of what ran, so replay derives the same verdict from `criteria_checked`.
pub fn surprises(task: &Task, runs: &[CriterionRun]) -> Vec<Surprise>;
/// Why a task stopped without being done. One type for cada respuesta que el ciclo da (L-115) y la que M31 agrega.
pub enum BlockedCause { PreCheck(NonEmpty<Surprise>), Unmet { attempts: u32 }, ScopeDecisionOwed, NonRetryable, CommandDenied { rule: String } }
pub enum TaskOutcome { Done, Blocked { cause: BlockedCause }, Interrupted, /* … */ }
//   `pre_check` devuelve `Vec<CriterionRun>` como `post_check` y `PreCheckOutcome` se borra; `run_task` (mod.rs:328-340) hace
//   `match NonEmpty::new(surprises(task, &pre_runs)) { Some(found) => bloquea con BlockedCause::PreCheck(found), None => sigue al intento }`
//   —`NonEmpty::new` devuelve `Option` (`core/src/nonempty.rs:20`) y el caso vacío es el normal— y `mod.rs:400-405` bloquea con `BlockedCause::Unmet { attempts }`, la frase que hoy arma un `format!`;
//   la oración se produce una vez, por `Display` de `BlockedCause` y de `Surprise` —«criterion `true` already passes before any work — the criteria need fixing, not the task» / «guard `false` is already red before any work started»—,
//   una línea por sorpresa, donde se escribe la causa del `task_status_changed` (loop_exec/integrate.rs:111-122) y donde `status` la muestra; ningún `reason:` se arma con `format!` (M22).
```

**Decisiones.** D177 (registrada con el plan) revisa D62 y D59: el
pre-check evalúa el conjunto entero y su veredicto nombra cada criterio
trivial y cada guard roto, en orden de declaración; el orden aprendido
decide cuándo llega la evidencia, nunca qué se verifica ni qué se reporta.
D178 (registrada con el plan) revisa D147: la regla de permiso nombra al
servidor entero, `mcp__<servidor>__*`. Lo que el ítem construye: el
parser que rechaza el estado sin revisor, el índice que dice «Retirada
por», el veredicto tipado, y el Contrato §5.2 (línea 171: «con
memoización y en el orden aprendido, §5.4») y §5.4 último párrafo:
«Complemento del pre-check: **orden aprendido**. El engine evalúa todos
los criterios, de menor a mayor duración histórica (dato que el log ya
tiene), de modo que la evidencia barata llega primero; el veredicto es
sobre el conjunto completo, nombra cada sorpresa en orden de declaración,
y no depende del orden de ejecución». L-105: §9 ya dice lo que se hizo
(«D152 `Retirada por D157`»); al cerrar, los marcadores «(8-03)» de esa
línea salen y el índice dice lo mismo que los cuatro cuerpos. L-87: la
mitad de D147 que 7-03 no cerró, cerró con el plan (D178, D180).

**Archivos.** Modifica: `xtask/src/adr.rs` (`:1-8` módulo doc, `:16`,
`:130-140` `index()`, tests `:207-243`), `xtask/src/adr/decision.rs`
(`Status`, `parse`, `revisers`, rustdoc `:25-35`, helper y tests
`:272-281,355-417`), `xtask/Cargo.toml` si `NonEmpty` no está al alcance,
`docs/design/adr/README.md:27,35-38`, `docs/design/adrs.md` (regenerado:
«Retirada por» en cuatro filas), `docs/design/contrato-del-run.md:171,206`,
`engine/src/task_cycle/{mod.rs, criteria.rs}`, `engine/src/run/loop_exec/integrate.rs:111-122`,
`engine/tests/task_cycle.rs:218-306,505-525,780-812`, `mecanismos.md` M23
(el párrafo de `adr --check`) y M24 (la distinción entre retirar por
decisión y registrar deuda), README del plan §9 (los marcadores «(8-03)»).
Borra: `PreCheckOutcome`, el `format!` de `mod.rs:334-340`.

**Prerequisitos.** Ninguno.

**Tests.** `xtask/src/adr/decision.rs`:
`a_status_that_disagrees_with_its_revisers_is_refused` (rojo: `parse`
acepta `revised`/`[]`, `retired`/`[]` y `accepted`/`[Dn]`; verde: los
rechaza y acepta las tres formas concordes);
`xtask/src/adr.rs::the_index_names_a_decision_its_status_its_revisers_and_its_file`
gana una fila `retired` y espera «`retired` *(Retirada por D157.)*» (rojo:
dice «Revisada»); `cargo run -p xtask -- adr --check` verde antes y después
salvo esa fila del índice; `engine/tests/task_cycle.rs`:
`surprises_names_every_trivial_criterion_and_every_broken_guard_in_declaration_order`
(unitario sobre runs sintéticos, sin subprocesos; rojo: la función no
existe y hoy vuelve la primera), `pre_check_and_post_check_run_every_criterion`
sostiene D177, `a_trivial_criterion_blocks_before_any_attempt_runs` y
`a_broken_guard_blocks_before_any_attempt_runs` conservan su oración byte a
byte para una sola sorpresa; `adapters/tests/claude_code.rs:804-808`
sostiene D178.

**Cierra.** EN-D35, DO-D46, DO-D47; L-95, L-105; L-87 (cerrado con el plan).

**Encastre.** 7-02 (M23): el checker sigue con sus tres reglas de
conjunto —numeración, reciprocidad, citas— y la cuarta vive donde vive
todo lo que es de un archivo: en `parse`. M24: la regla de la promesa no
construida es una y M29 la cita. M06: `Surprise` es el hecho tipado y la
prosa se produce en el borde por `Display`. M05/Replay: `surprises` es una
función de `criteria_checked`, así que un `status` tras un resume deriva
el mismo porqué. M22: ningún contador sube —`reason_built_by_format`
queda en su valor—. D167: su fila de D62 queda revisada por D177 sin tocar
D167.

---

## M30 · Una superficie, un frame

**Vicio V2** (una pregunta respondida en muchos lugares) y **V10** (caminos
duplicados): qué nodos tiene un run, en qué orden y con qué palabra lo
contestan varios caminos. La vista viva lee el `RunFrame` —orden de
declaración, cada grupo `parallel` seguido de sus hijos (`view/mod.rs:114-116`),
`NodeStanding::{Skipped, ToGo, Reached}`, y `NodeFrame.group`, escrito en
`view/node.rs:115` y que ninguna superficie lee (I-08)—; `status` deriva el
frame y vuelve a derivar el estado para listar `RunState.nodes` en orden
alfabético y sólo los que el log nombra (`status/mod.rs:51-71`;
`print_failures` igual, `:122-137`; auditoría `04-cli.md:43-44`, «two
passes»); `--json` igual, en un `BTreeMap` que no puede decir orden ni
grupo (`json.rs:87,133-137`); `graph --run` recorre el workflow de disco y
no el manifest congelado del run (`graph.rs:37,72-86`; auditoría
`04-cli.md:51`), sin los hijos de un `parallel`; `stats` toma la palabra de
`RunState` (`stats.rs:318`; auditoría `04-cli.md:98` lista los cinco
consumidores de `NodeDisplay`). Y el hecho «qué espera un nodo» está
aplastado en el origen: `NodeState::Waiting { external_ref }`
(`node/ledger.rs:43-50`) no distingue un gate de unas preguntas sin
responder, `replay.rs:305` descarta `p.questions` al escribir `Waiting {
external_ref: None }`, y la oración «asked N questions: …» se arma a mano
en tres sitios con dos ortografías —la crónica (`words.rs:235-246`, con
`counted`), `PauseReason::Questions` (`run/payloads.rs:167-172`, con
«question(s)») y los veredictos de las tools (`run_tools/verdicts.rs:71-80`,
con «question(s)»)—. Un `parallel` dentro de un `parallel` es
representable (`node_kind.rs:52`), ningún check lo rechaza
(`check/error.rs:212,266,319`) y el iterador empareja con el grupo
inmediato (`workflow/mod.rs:141-150`). Evidencia: L-67, L-97, L-109 (§11);
M24 I-08; CLI-D28, CLI-D29, CLI-D30 (§12).

**Regla.** El frame es la única derivación que una superficie lee para
listar los nodos de un run: todo `NodeFrame` del frame —del manifest
congelado del run, en orden de declaración, cada grupo `parallel` con sus
hijos un paso debajo, los que el modo excluye incluidos y etiquetados
`skipped`, como `graph --run` ya imprime—. Lo que un nodo espera vive en
su estado: `NodeState::Waiting { on: NodeWait }`, y de ahí lo leen
`NodeDisplay::of(state)` —una sola entrada, la que se conserva—, el
scheduler y la pausa del run. Una oración, un productor:
`text::asked_questions`. Un `parallel` no anida otro: `check` lo rechaza,
y un paso de sangría es exacto. Una superficie que lista nodos en orden los
lista como lista; las colecciones que un lector indexa por id —`tasks`,
`diagnostics`— siguen siendo mapas.

**Firmas.**

```rust
// core/src/events/node/ledger.rs — la espera tiene forma
pub enum NodeState { /* … */ Waiting { on: NodeWait } }
/// What a waiting node waits on. A gate's kind — internal or external — is a property of the declaration, never of the wait: the scheduler reads it from the workflow and the state says only whether the gate has a handle yet.
pub enum NodeWait {
    /// A published, unresolved gate; `external_ref` is the forge's handle once recorded.
    Gate { external_ref: Option<String> },
    /// The questions the node asked and nobody answered.
    Questions { asked: NonEmpty<QuestionId> },
    //   `QuestionsAskedPayload.questions` es un `Vec` en el wire (`gates/payloads.rs:358`) y su constructor es el que lo hace no vacío, así que un
    //   `questions_asked` sin ids —de un escritor viejo o ajeno— no es una espera: `replay::apply` deja el nodo como estaba y
    //   `RunState.broken` nombra el evento, la degradación explícita que un pliegue de producción debe en vez de un pánico
}
// engine/src/replay.rs:276 → `Waiting { on: NodeWait::Gate { external_ref } }`; :305 → `Waiting { on: NodeWait::Questions { asked } }`
// engine/src/run/schedule.rs:386-417 — `waiting_step` decide sobre `on`: `Questions { .. }` no es un gate y nunca llega a `PublishGate`/`PollGate`; `Gate { external_ref }` sigue eligiendo por `is_external_gate(node)`, que es de la declaración
// engine/src/view/phase.rs:82-86,157-161 — `WaitingOn::Node { node, on: NodeWait, reason }` en vez de `external_ref`: un solo vocabulario para «qué espera» en el run y en el nodo
// core/src/text.rs
/// Identifiers as a reader sees a list of them, each in its own backticks — the one joiner every sentence about a set of ids uses.
pub fn listed<'a>(ids: impl IntoIterator<Item = &'a str>) -> String;    // mudada desde `run/payloads.rs:122`, que la tenía privada
/// The one sentence for questions awaiting an answer: `asked 2 questions: q1, q2`.
pub fn asked_questions(asked: &[QuestionId]) -> String;                 // `counted` + `listed`
//   la consumen `NodeDisplay::of`, `PauseReason::Questions`'s Display y la crónica; `run_tools/verdicts.rs:60-80` pasa a `counted` + `listed` para sus cuatro veredictos, de modo que «question(s)» desaparece del repo
// cli/src/render/state.rs — `NodeDisplay::of(Option<&NodeState>)` conserva su firma y gana el brazo
//   `Some(NodeState::Waiting { on: NodeWait::Questions { asked } }) => Some(text::asked_questions(asked))`; `skipped()` se conserva
// cli/src/surface/chronicle/words.rs:235-246 — `H::Asked { questions }` dice `NodeDisplay::of(Some(&NodeState::Waiting { on: NodeWait::Questions { asked } })).label()`: la crónica y `status` son los mismos bytes por construcción; el `format!` se borra
// cli/src/render/width.rs — `pub(crate) const CHILD_DEPTH: usize = 1;` (D179), consumida por `chronicle::graduation` y por `view::node_rows`; la copia privada de `chronicle/mod.rs:26` se borra
// cli/src/surface/view.rs:69 — `node_rows` sangra `CHILD_DEPTH` las filas de un nodo con `group: Some(_)`; `standing` (:221-227) se conserva; `working` (:273-287) sigue filtrando lo que corre: la región muestra el trabajo en curso, no la lista
// cli/src/commands/status/mod.rs — `print_derived(frame: &RunFrame, state: &RunState)`: los nodos salen de `frame.nodes`, `{id}: {label}` como hoy, los hijos sangrados `CHILD_DEPTH` bajo su grupo; `print_failures` recorre `frame.nodes` en ese mismo orden; las tareas siguen saliendo de `state.tasks`; `status` deriva una vez y pasa la misma lectura a las tres partes de la página
// cli/src/json.rs — con el salto a `SCHEMA_VERSION = 5`, `nodes` deja de ser un mapa:
pub struct NodeJson { id: String, state: StateWord /* serializa `word()`, como RunWord */, #[serde(skip_serializing_if = "Option::is_none")] detail: Option<String>, #[serde(skip_serializing_if = "Option::is_none")] group: Option<String>, #[serde(skip_serializing_if = "Option::is_none")] waiting_on: Option<NodeWaitJson> }
#[serde(tag = "on", rename_all = "snake_case")] pub enum NodeWaitJson { Gate { external_ref: Option<String> }, Questions { asked: Vec<String> } }
//   `nodes: Vec<NodeJson>` en orden de declaración; `impl Serialize for StateWord` junto al de `RunWord` (state.rs:299-304); `WaitingOnJson::Node` lleva el mismo `NodeWaitJson`, así que el documento tiene un solo vocabulario de espera
// cli/src/cli.rs:157-166 — `Graph { workflow: Option<PathBuf>, run: Option<RunId>, format }`: exactamente una fuente. Sin `--run`, el positional es obligatorio y se lee de disco como hoy; con `--run`, el workflow es el que el run congeló y un positional además del id se rechaza nombrando las dos fuentes.
// cli/src/graph.rs — `pub async fn graph(..)`: con `--run` abre el run por `Context::open_run` y dibuja `open.manifest.doc.workflow`; `render_mermaid` emite un `subgraph` por nodo `parallel` con sus hijos adentro y `render_dot` un `cluster_<id>`; deja de calcular `mode_included_nodes`
// cli/src/commands/stats.rs:306-323 — `render_nodes(stats, state, glyphs)` sin cambio de forma: una fila por nodo que arrancó, la palabra por `NodeDisplay::of(state.nodes.state(..))`, que ahora sabe qué preguntó
// engine/src/check/error.rs — `CheckError::ParallelInsideParallel { group, node }` junto a sus tres hermanos `*InsideParallel`
// engine/src/lib.rs:107 — `mode_included_nodes` deja de exportarse: su único consumidor externo era `graph.rs`; sigue en `view/mod.rs` y `schedule.rs`
// core/src/events/gates/ledger.rs:91-95 — `pending_questions` y `QuestionRound::pending` se borran (§0.15, reemplazado): lo que preguntó un nodo se lee del estado del nodo, no de un segundo pliegue
```

**Decisión.** D179 (registrada con el plan): `status`, `--json` y `graph
--run` listan los nodos como el frame; lo que un nodo espera vive en su
estado y `NodeDisplay::of` lo dice; `--json` sube a `schema_version: 5`
con `nodes` como lista ordenada; `graph` toma el workflow de una sola
fuente; un `parallel` no anida otro (L-109). Cambio visible:
`docs/compatibility.md:278-321` (qué contiene `nodes`, su forma, el número,
y que `tasks`/`diagnostics` siguen siendo mapas), `:193-198` (la lista de
reglas de `check` gana el anidado), `:353-357` (`waiting_on`),
`README.md:134-135,164` (la fila de `graph`), `docs/concepts.md:99-101` sin
cambio. Un `run_paused.reason` escrito antes del cambio conserva su
oración: la prosa del pausado es lo persistido (§0.15, planificado) y la
página lo muestra tal como se escribió.

**Archivos.** Modifica: `core/src/events/node/ledger.rs`, `core/src/text.rs`,
`core/src/events/run/payloads.rs:118-175`, `core/src/events/gates/ledger.rs`,
`engine/src/replay.rs:271-310`, `engine/src/view/{phase.rs, mod.rs}`,
`engine/src/run/schedule.rs:386-417`, `engine/src/run_tools/verdicts.rs:60-90`,
`engine/src/check/{error.rs, graph.rs}`, `engine/src/lib.rs`,
`cli/src/render/{state.rs (:87-101,110-122 rustdoc y brazo; tests
:147-201), width.rs}`, `cli/src/surface/{view.rs, region.rs,
chronicle/mod.rs, chronicle/words.rs, closing.rs}`,
`cli/src/commands/{status/mod.rs, advice.rs, stats.rs}`, `cli/src/json.rs`,
`cli/src/cli.rs:157-166,380`, `cli/src/graph.rs`, `docs/compatibility.md`,
`README.md`, y los tests y fixtures que fijan la forma vieja:
`engine/tests/{external_gate.rs:78-79, properties.rs:369, view.rs:955-1057
(`RunPhase::Waiting`) y :973 (la oración «question(s)»), check.rs}`,
`cli/tests/{status_cmd.rs, parked_runs.rs:410,418,545-590, graph_cmd.rs,
run_flow.rs:62-63,2078, mcp_flow.rs:1176-1180, console_interaction.rs}`,
`cli/src/commands/mcp.rs:303-316` y `drive.rs:468` (consumidores del
documento), y el plan: `preguntas.md:118,131,252-254,413,430` (el
modificador sale del estado; `pending_questions` sale de la firma de
`GateLedger` y la fila de derivación pasa a `Waiting { on:
NodeWait::Questions { asked } }`), `mecanismos.md` M04 `:192,:205` (la
misma firma y la cláusula de `questions_exec.rs`), `cronica.md:225-226` (`--json` y la tool
`workflow_status` cambian de forma), `README.md` §11 L-97 y §10 filas 5-05
y 7-07 (I-08 cierra acá). Borra: la exportación de `mode_included_nodes`,
el `format!` de `words.rs:237-245`, `chronicle/mod.rs:26`,
`GateLedger::pending_questions`, `QuestionRound::pending`, el `listed`
privado de `payloads.rs:122`. La clasificación de §0.15 de las dos
primeras es «reemplazado»: no tienen llamador, y lo que `questions_exec` y
`NodeDisplay` leen en su lugar es el estado del nodo. `frames.rs`, `NodeFrame` y `StateWord::of` no
cambian; `NodeState` no aparece en `core/schemas/events.json`, así que no
hay schema que regenerar.

**Prerequisitos.** Ninguno.

**Tests.** `core/tests/events.rs`:
`a_node_that_asked_waits_on_its_questions_in_its_own_state` (rojo:
`Waiting` no tiene forma); `cli/tests/status_nodes_cmd.rs` (nuevo, para
que `status_cmd.rs` no cruce las 500 líneas):
`status_lists_every_declared_node_in_declaration_order_with_children_under_their_group`
(rojo: hoy alfabético y sólo los que el log nombra),
`status_says_which_questions_a_node_is_waiting_on` (rojo),
`status_lists_the_nodes_the_frame_declares_in_its_order` (ids y orden
contra `frame.nodes`),
`status_json_lists_every_declared_node_in_order_under_schema_version_five`,
`the_chronicle_and_status_say_a_node_that_asked_with_the_same_bytes`;
`status_attributes_each_problem_to_the_document_it_came_from` sigue verde
(`{id}: {label}` no cambia); `cli/tests/run_surface.rs`:
`the_live_view_indents_a_groups_children_under_it`; `cli/tests/graph_cmd.rs`:
`graph_with_a_run_id_draws_the_runs_frozen_workflow_with_its_groups_as_subgraphs`
y `graph_refuses_a_workflow_and_a_run_at_once`; `engine/tests/check.rs`:
`a_parallel_inside_a_parallel_is_refused`; `engine/tests/schedule.rs`:
`a_node_waiting_on_questions_is_never_taken_for_a_gate`;
`cli/src/render/state.rs` (unit): `a_waiting_node_that_asked_names_its_questions`.

**Cierra.** CLI-D28, CLI-D29, CLI-D30; L-67, L-97, L-109; M24 I-08.

**Encastre.** M19 (`cronica.md`): «toda palabra de estado sale de
`NodeDisplay::of(state).label()`» sigue siendo verdad con una sola entrada;
la región sigue mostrando el trabajo en curso —lo que corre y lo que
espera— y `status` la lista entera, con las mismas palabras y el mismo
orden del frame. M26 (`preguntas.md` §5): el modificador del nodo que
preguntó, leído del estado. M06: la espera es un hecho tipado y la oración
se produce en un lugar. M16: la palabra de `expect.nodes` no cambia
(`StateWord::of`). M15: `graph` deja de enmarcar otro documento que el run.
M22: `CHILD_DEPTH` cita D179; `status_nodes_cmd.rs` deja el contador de
archivos donde está. §8: `render::state` se generaliza (el mismo
vocabulario, la misma entrada, un brazo más); `run_frame`/`view/` y
`frames.rs` sin cambio; `json::SCHEMA_VERSION` sube por su propia regla.
M31: `print_failures` y `RunDocument` cambian acá primero; 8-05 les agrega
la muerte de una sesión.

---

## M31 · Una sesión que muere dice por qué

**Vicio V2** (dos servidores con un nombre) y **V5** (una degradación sin
evidencia). Reportado desde Codex: el usuario registra el control plane
como `[mcp_servers.yunta] command = "yunta" args = ["mcp"]` en
`~/.codex/config.toml`; el adapter inyecta el servidor per-run como `-c
mcp_servers.yunta.url=…` (`adapters/src/codex/mod.rs:185-196`,
`RunToolsEndpoint::SERVER_NAME = "yunta"` en `core/src/port/session.rs:133`);
Codex hace merge sobre la misma tabla y rechaza `url is not supported for
stdio`. El stderr que lo dice se drena a `tracing::debug!`
(`core/src/process/subprocess.rs:171-174`) y el nodo falla con
`Failure::message("session ended without a terminal event")` reintentable
(`engine/src/run/prompt_exec.rs:222-231`, `task_cycle/session.rs:289`) sin
código de salida; en un nodo `kind: loop` ni eso: el intento guarda
`dispatch: Crashed` en `AttemptRecord` (`task_cycle/attempt.rs:137-146`) y
nadie lo lee, la tarea bloquea con una frase (`task_cycle/mod.rs:400-405`)
que `integrate.rs:175-182` empuja a `state.blocked_reasons: Vec<String>`
(`loop_exec/mod.rs:220`) y `loop_exec/mod.rs:78-86` concatena en un
`Failure::message` no reintentable. `probe()` de codex corre `--version`
(`adapters/src/codex/mod.rs:263-271`), así que `doctor` dice sano —y
`doctor` sólo conoce adapters (`cli/src/commands/doctor.rs:29-63`), nunca
las candidaturas que un runner nombra (`core/src/config/mod.rs:54`:
`BTreeMap<RunnerName, Vec<RunnerCandidate>>`)—, y
`docs/troubleshooting.md:40-44` promete que «a healthy `doctor` means a run
won't fail on setup for that adapter». Evidencia: L-106 (§11); AD-D25,
EN-D36, EN-D38, CLI-D32 (§12).

**Regla.** El servidor per-run tiene nombre propio, `yunta-run`, distinto
del control plane que un usuario registra con el nombre que quiera. Una
sesión que termina sin evento terminal dice cómo salió su proceso y qué
escribió último en stderr, y ese hecho tipado llega al `node_failed` de los
dos caminos que abren sesiones —el nodo de prompt y el nodo `loop`— y de
ahí a `status`, a `--json` y a la crónica. Interrogar a un proceso es
matarlo primero: `exit()` mata el grupo, cierra las cañerías y recoge la
salida, en ese orden, de modo que la espera está acotada por construcción y
ninguna sesión sobrevive a su run. Se pregunta sólo a la sesión cuyo stream
terminó sin decir nada. Lo que el hijo escribió en stderr entra al log
redactado: el entorno del hijo es donde este sistema pone sus secretos.
`yunta doctor --session` abre una sesión real por binding —adapter, modelo
y agente— que algún runner nombre, no por nombre de runner, porque una
sesión ejercita un binding y el que un runner tiene de reserva vale lo que
el primero; gasta un prompt por binding, por eso es opt-in, y `doctor` sin
la bandera dice qué garantiza y qué no.

**Firmas.**

```rust
// core/src/port/session.rs
impl RunToolsEndpoint { pub const SERVER_NAME: &'static str = "yunta-run"; }   // claude_code/mod.rs:49,161 y parse.rs:136, codex/mod.rs:185 lo siguen sin cambio de texto
pub trait AgentSession {
    /* … */
    /// How the process ended, asked only of a session whose stream ended without a terminal event. The session is over by the time it is asked, so the group dies first and the status is collected after: the wait is bounded by construction and nothing outlives the run. A session with no process of its own answers `None`.
    async fn exit(&mut self) -> Option<SessionExit> { None }     // el mock hereda el default
}
// core/src/process/subprocess.rs — `SubprocessSession::exit`: `kill_group()` → `close_pipes()` → `child.wait()` → `reaped = true`, el mismo orden que `kill()` (:268-278), que es lo que conserva la invariante de `Drop` (:287-296): `kill_group` es un no-op una vez reaped (:236-241)
/// How many stderr lines a session keeps for its exit (D180).
pub const STDERR_TAIL_LINES: usize = 20;
/// A stderr line as the log keeps it: every value this session's `env` carried replaced by `[redacted]`, the promise `Secret`'s own `Debug` makes and `RunToolsEndpoint` states for its token.
fn redacted(line: String, secrets: &[Secret<String>]) -> String;
//   el drenaje guarda las últimas `STDERR_TAIL_LINES` líneas redactadas en un `Mutex<VecDeque<String>>` compartido; el cierre captura los valores de `launch.env` (`:77-79`) junto a la cola

// core/src/events/failure.rs — el hecho es del log, así que vive con los hechos y el puerto lo usa
/// How the process ended: the status it exited with, or the signal that ended it. A stored `type` this build does not know reads back as `Unknown`, the tolerance every persisted union here gives.
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEnd { Code { code: i32 }, Signal { signal: i32 }, #[serde(other)] Unknown }
pub struct SessionExit { pub end: SessionEnd, pub stderr_tail: Vec<String> }
pub struct SessionDeath { pub adapter: AdapterId, pub exit: Option<SessionExit> }
pub enum Failure { Artifacts { artifacts: Vec<ArtifactFailure> }, SessionDied { died: SessionDeath }, Message { outcome: String } }   // untagged; `died` es el discriminador, un campo como `artifacts`, antes de `Message`
impl Failure { pub fn session_died(adapter: AdapterId, exit: Option<SessionExit>) -> Self; }
//   `Display` dice cada forma que el tipo admite, porque la ausencia se nombra: con salida y con cola, «session `codex` exited with code 2 before any terminal event — url is not supported for stdio» (la última línea);
//   con señal, «…was killed by signal 9 before any terminal event»; con la cola vacía, la oración sin el guión; sin salida —una sesión sin proceso propio, el mock—, «session `mock` ended without a terminal event».
//   `Failure::failures()` gana el brazo vacío. §8 conserva `Failure` y `node_close::fail_with`: este ítem los generaliza —un constructor y un brazo más, el mismo `fail_with`— y no los reimplementa.
//   `receipt/mod.rs:216-224` cuenta artifacts que no cerraron: una sesión muerta no nombra ninguno y no entra en esa cuenta; el `let-else` queda dicho en su rustdoc en vez de ser un descarte accidental.

// engine/src/task_cycle/mod.rs — los dos caminos dicen la muerte
pub enum DispatchOutcome { /* … */ Crashed { exit: Option<SessionExit> }, /* … */ }
pub enum BlockedCause { PreCheck(NonEmpty<Surprise>), SessionDied(SessionDeath), Unmet { attempts: u32 } }   // `PreCheck` y `Unmet` son de M29
//   session.rs:287-291 — `match terminal { Some(outcome) => outcome, None => DispatchOutcome::Crashed { exit: session.exit().await } }`: sólo se pregunta al que murió;
//   prompt_exec.rs:223 — `fail_with(ctx, node, Failure::session_died(adapter.id().clone(), exit), true, tokens)`;
//   task_cycle/mod.rs:400-405 — un intento que murió bloquea la tarea con `BlockedCause::SessionDied`
// engine/src/run/loop_exec/{integrate.rs, mod.rs} — la causa deja de ser texto antes de llegar al log
//   `integrate.rs:175-182` empuja `(TaskId, BlockedCause)` y `mod.rs:220` lo guarda tipado; `mod.rs:78-86` cierra el nodo con
//   `Failure::session_died(..)` de la primera tarea cuya causa es una muerte —el hecho accionable— y con `Failure::message` cuando ninguna lo es,
//   `retryable: true` en el primer caso, como el camino de prompt, y `false` en el segundo, como hoy; la oración sigue nombrando cada tarea bloqueada por `Display` de `BlockedCause`

// cli/src/cli.rs:138-140 — la ayuda de `Doctor` dice qué chequea sin la bandera y qué agrega con ella
Doctor {
    /// Also opens one real session per binding any runner names — the smallest run there is, through the same machinery a workflow uses, run tools mounted — and reports how each ended. Spends one prompt per binding.
    #[arg(long)] session: bool,
}
// cli/src/commands/doctor.rs
pub async fn doctor(session: bool) -> Result<Outcome, CliError>;
/// One probe run per distinct binding (`adapter`/`model`/`agent`) any runner names, whichever runners reach it: a session exercises the binding, and one a runner falls back to is worth as much as its first. Only bindings whose adapter already probed healthy; the rest the plain `doctor` reports.
async fn session_probe(ctx: &Context, candidate: &RunnerCandidate, named_by: &[RunnerName]) -> SessionProbe;
//   `claude-code/claude-sonnet-4-6 (executor, reviewer): ok — 812 tokens`
//   `codex/gpt-5-codex (planner fallback, reviewer-alt): session died — exit 2: url is not supported for stdio`
/// The context a probe run drives in: `yunta test`'s sandbox over this project's real config with `baseline:` removed — a probe asks whether a session opens, never what the tree measured.
fn probe_context(ctx: &Context, checkout: &SandboxedCheckout) -> Context;
//   un run por binding, no un run con un nodo por binding, porque `probe_or_refuse` rechaza la invocación entera: así el que muere se reporta como él mismo. `Outcome::Reported` si alguno murió o algún adapter probó enfermo.
// cli/src/commands/test/mod.rs — el armado del sandbox deja de ser privado de `run_case`
/// A checkout of its own for a run that must not touch the project: a temp root, its worktree seeded from `.yunta`, a git repo, and the context rooted there.
pub(crate) fn sandboxed_checkout(cwd: &Path) -> Result<SandboxedCheckout, CliError>;   // lo que `case.rs:135-151` arma hoy con `copy_dir_all` (:176) e `init_git` (:193), que pasan a `pub(crate)`
// cli/src/json.rs — la muerte va donde ya está el nodo: `NodeJson` (8-04) gana
//   `#[serde(skip_serializing_if = "Option::is_none")] session_death: Option<SessionDeathJson>`, aditivo, y `SCHEMA_VERSION` queda en el 5 que 8-04 fijó.
//   `diagnostics` no gana entrada —una muerte no nombra artifact, igual que una frase (`node_diagnostics` :368-372)— y su rustdoc (:88-94) sigue siendo verdad.
// cli/src/commands/status/mod.rs — `print_failures` gana el bloque: el nodo, cómo salió, y cada línea de la cola

// testkit-core/src/stubs.rs (nuevo) — el crate que adapters y cli comparten (`yunta-testkit` depende de `yunta-adapters`, así que los stubs no pueden vivir ahí)
/// The stub CLIs the adapter and CLI tests drive, by absolute path. Each honours `<NAME>_STUB_EXIT` and `<NAME>_STUB_STDERR`: what to exit with, and the lines to write to stderr before exiting.
pub fn codex() -> PathBuf;
pub fn claude_code() -> PathBuf;
// testkit-core/src/adapter.rs:41-49 — junto a `drain`, `drain_for_exit(session) -> (Vec<AgentEvent>, Option<SessionExit>)`: lo que la sesión dijo y cómo salió, sin que el test escriba un segundo drenaje
```

**Decisión.** D180 revisa D147 (el nombre) y fija `STDERR_TAIL_LINES`: el
servidor per-run se llama `yunta-run`; `exit()` mata antes de recoger; la
muerte llega tipada por los dos caminos que abren sesiones; el stderr entra
redactado; `doctor --session` abre una sesión por binding sano, una por
binding. D147 queda `revised_by: [D178, D180]`. Docs: `docs/adapters.md`
(§doctor gana `--session`; los dos servidores y sus nombres),
`docs/compatibility.md` §The MCP servers (los nombra), §The JSON surfaces
(`session_death` en el nodo, y que `diagnostics` no gana entrada) y §What
isn't covered (qué chequea `doctor`), `docs/troubleshooting.md:40-44` (qué
garantiza `doctor` sin `--session` y qué sólo con ella) y su entrada
«session died», README tabla de comandos, spec-events §5.15 (la falla toma
**tres** formas; la fila nombra `died` y el párrafo «una de dos formas»
pasa a tres), spec-adapter O2 (`:217-219`: el engine registra la muerte con
su salida, no sintetiza una frase) y el `exit` del trait,
`contrato-del-run.md:356` (lo mismo), `mecanismos.md` M01 (`SERVER_NAME`
con su valor nuevo).

**Archivos.** Nuevo: `testkit-core/src/stubs.rs`, `testkit-core/stubs/{codex_stub.sh,
claude_code_stub.sh}` (mudados desde `adapters/tests/fixtures/`, con la
variable de stderr). Modifica: `core/src/port/session.rs` (`SERVER_NAME`,
`exit`, el rustdoc de `events` `:308-310`, el re-export de
`port/mod.rs:17-20`), `core/src/process/subprocess.rs`
(`:171-174,226-241,268-278,287-296`), `core/src/events/failure.rs`
(`:3-8,18-23,24-32,57-62`), `core/schemas/events.json`,
`engine/src/task_cycle/{mod.rs, session.rs, attempt.rs}`,
`engine/src/run/{prompt_exec.rs, node_close.rs:335-350,
loop_exec/integrate.rs:172-186, loop_exec/mod.rs:78-86,220}`,
`engine/src/receipt/mod.rs:216-224`, `engine/tests/task_cycle.rs:504`,
`adapters/tests/{codex.rs, claude_code.rs}` enteros (cada uno resuelve el
stub por `stub_path()` y lo nombra en su doc de módulo; las fixtures
`codex.rs:195` y `claude_code.rs:828` dicen `yunta` donde el CLI real dirá
`yunta-run`), `testkit-core/src/{lib.rs, adapter.rs}`, `cli/src/{cli.rs,
json.rs}`, `cli/src/commands/{doctor.rs, status/mod.rs, test/mod.rs,
test/case.rs}`, `cli/tests/pack_requires_doctor_cmd.rs` (los casos de
`doctor` viven donde ya viven), `docs/adapters.md`,
`docs/compatibility.md`, `docs/troubleshooting.md`, `README.md`,
`docs/design/{spec-events.md, spec-adapter.md, contrato-del-run.md}`,
`docs/design/adr/D180-*.md` (la forma de `Failure::SessionDied`),
`mecanismos.md` M01. Borra: `adapters/tests/fixtures/*_stub.sh`.

**Prerequisitos.** 8-01 (`Context` con su `Interrupt` y el camino que
`drive::execute` recorre: `doctor --session` construye un `Context` y
maneja un run) y 8-03 (`BlockedCause`, que este ítem extiende). 8-04 no es
prerequisito sino orden: `print_failures` y `NodeJson` cambian allá
primero.

**Tests.** `adapters/tests/codex.rs`:
`the_per_run_server_never_shares_the_control_planes_name` (rojo: los args
dicen `mcp_servers.yunta.`; verde: `mcp_servers.yunta-run.url` y ningún
`mcp_servers.yunta.`), `a_session_that_dies_before_its_first_event_reports_its_exit_and_its_last_stderr_lines`
(rojo: el trait no tiene `exit`; el stub escribe la línea por
`CODEX_STUB_STDERR` y sale con 2, y el test lee la salida por
`drain_for_exit`), `a_dead_sessions_stderr_tail_never_carries_a_value_from_its_env`
(el stub escribe el token que su entorno lleva; la cola dice `[redacted]`),
`a_dead_sessions_process_group_is_gone_once_its_exit_is_collected` (el
stub deja un nieto vivo; tras `exit()` el grupo está `Liveness::Dead`);
`adapters/tests/claude_code.rs`: el allow-rule y el prefijo dicen
`yunta-run`; `engine/tests/run_sessions.rs`:
`a_node_whose_session_died_fails_naming_the_adapter_and_the_exit` y
`a_session_that_finished_its_turn_is_never_asked_how_it_exited` (sobre una
sesión del mock que registra si se la interrogó: la regla es del engine);
`core/tests/events.rs`: `a_node_failed_by_a_dead_session_round_trips_with_its_exit`
y `a_session_end_this_build_does_not_know_reads_back_as_unknown`;
`engine/tests/task_cycle.rs`: `a_task_whose_session_died_blocks_naming_the_exit`;
`engine/tests/run.rs`: `a_loop_node_whose_session_died_fails_naming_the_adapter_and_the_exit`
(rojo: hoy dice «no task is ready and not all are done»);
`cli/tests/status_cmd.rs`: `status_prints_the_stderr_a_dead_session_left`,
`status_json_publishes_a_session_death_on_its_node`;
`cli/tests/pack_requires_doctor_cmd.rs`:
`doctor_session_reports_a_binding_whose_cli_dies_at_startup_with_its_stderr`,
`doctor_session_names_every_runner_that_reaches_a_binding`,
`doctor_session_never_measures_the_projects_baseline`,
`doctor_without_session_opens_none` (los stubs de `testkit-core` nombrados
por `binary:` en la config del sandbox, sin tocar `PATH`).

**Cierra.** AD-D25, EN-D36, EN-D38, CLI-D32; L-106.

**Encastre.** M01: `SERVER_NAME` sigue siendo el único nombre y los
adapters lo siguen. M06: el hecho es tipado y la prosa se produce en el
borde, en `Failure::Display`; `BlockedCause::Display` dice la tarea y
delega la muerte en ella. M08: `doctor --session` abre sesiones por
`open_session`, la única puerta, porque corre un run. M10/M27: `exit()`
mata el grupo como todo lo que este repo gobierna, y por eso no necesita un
umbral nuevo. M11/§Secreto: el stderr entra redactado, que es lo que
`RunToolsEndpoint` promete de su token. M18: un solo camino de ejecución
—el de `yunta test`, con su armado de sandbox ahora compartido— con
adapters reales. M20: los stubs y el helper que los drena viven en
`testkit-core`, el crate que adapters y cli comparten. M22:
`STDERR_TAIL_LINES` cita D180. M19: la crónica dice el `Failure` como a
cualquier otro. M28: el run de una prueba no mide baseline, porque su
veredicto no es sobre el árbol. M29: `BlockedCause` reúne lo que bloquea
una tarea; D178 y D180 revisan D147 en ese orden. §8: `Failure` y
`node_close::fail_with` se generalizan, nunca se reimplementan.
