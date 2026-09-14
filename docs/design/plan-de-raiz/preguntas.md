# Un nodo que pregunta, pregunta — M26

Un nodo que declara `questions` entrega su documento de preguntas y termina
ahí: cierra como todos los nodos —hooks `after`, scope, artifacts— y en vez de
`node_finished` registra el hecho de haber preguntado, `questions_asked`, par
de `questions_answered`. Entre los dos el nodo espera como un gate interno;
después de la respuesta termina con el `node_finished` que el cierre difirió.
Lo que dependa de las respuestas es del nodo siguiente, que las monta como
contexto. Vuelve irrepresentable: un `node_failed` que la derivación lee como
espera, un cierre que salta `close_node`, un artifact que sólo podría
escribirse después de una respuesta que el nodo nunca recibe, y un flag
`interactive` que ninguna superficie lee.

Decisión: D173 (revisa D86). Prerequisito: W-11. Ítems: 2-01, 2-02, 2-03,
2-04, 3-01, 3-02, 3-05, 4-02, 4-03, 5-05, 5-06, 6-04. Defectos: EN-D27,
EN-D28, EN-D29, EV-D20, AR-D19, CO-21, DO-D45; inventario M24 I-01, I-07.

---

## 1. Vocabulario

| término | qué es | dónde vive |
|---|---|---|
| **nodo que pregunta** | un `kind: prompt` cuyo `artifacts.produces` es `[questions]` | `Node::asks` |
| **preguntar** (`questions_asked`) | el hecho de que el nodo cerró entero y dejó preguntas sin respuesta | kind del dominio `gates` |
| **responder** (`questions_answered`) | el hecho de que una persona contestó, por qué canal y quién | kind del dominio `gates`, existe |
| **ronda** | el par preguntar/responder de un intento del nodo | `GateLedger` |
| **respuestas** | el documento que el engine escribe con las respuestas, artifact del nodo que preguntó | `ArtifactKind::Answers` (4-02); hasta entonces `questions.answers.yaml` |
| **superficie** | quien pone las preguntas a una persona: la consola cuando está, la tool MCP, un pull request (A-14); sin ninguna, el run se estaciona con sus preguntas registradas | `HumanInteraction::ask` |
| **canal** (`Channel`) | por dónde llegó la respuesta: `tty`, `mcp`; `pr` es deuda A-14 | `QuestionsAnsweredPayload.channel` |

En prosa: nodo que pregunta, preguntar, responder, ronda, respuestas,
superficie. En YAML, JSON y código: `questions`, `questions_asked`,
`questions_answered`, `answers`. `interactive` deja de existir: un nodo que
declara `questions` pregunta, y la superficie disponible decide cómo.

---

## 2. La regla, en el tipo

Un nodo que pregunta, pregunta. `yunta check` rechaza lo demás nombrando el
arreglo:

```rust
// crates/core/src/workflow/node.rs
impl Node {
    /// Whether this node hands over a `questions` document and waits on its answers.
    /// The one copy of the predicate: the scheduler, the ask round and `check` all read it.
    pub fn asks(&self) -> bool;
}

// crates/engine/src/check/error.rs
/// A node that asks ends when it asks: its answers are the next node's context, so nothing it
/// declares beside `questions` could be written after them.
#[error("node `{node}` produces `questions` alongside {others} — a node that asks ends when it asks, and its answers reach the next node as context; keep `{node}` producing `questions` alone and move {others} to a node that follows it with `context: [{{ artifact: {{ node: {node}, {answers} }} }}]`",
        others = ArtifactSpec::listed(.others), answers = ReservedIdentity::Answers.reference())]
QuestionsAlongsideOtherArtifacts { node: NodeId, others: Vec<ArtifactSpec> },
/// Only a `prompt` node holds a session that hands questions over and a close that waits on them.
#[error("node `{node}` is `kind: {kind}` and produces `questions` — only a `prompt` node asks; put the questions in a `prompt` node and read its answers from here")]
QuestionsOnKind { node: NodeId, kind: &'static str },
/// The scheduler puts questions to a person one top-level node at a time; a group's child never reaches it.
#[error("node `{node}` produces `questions` inside parallel group `{group}` — a person answers one node at a time; ask before or after the group")]
QuestionsInsideParallel { node: NodeId, group: NodeId },

// crates/engine/src/check/declarations.rs
pub(crate) fn check_asking_nodes(workflow: &Workflow, errors: &mut Vec<CheckError>);   // alongside · kind
// crates/engine/src/check/gates.rs — junto a check_no_gate_in_parallel, misma recursión
pub(crate) fn check_no_questions_in_parallel(nodes: &[Node], parent_group: Option<&Node>, errors: &mut Vec<CheckError>);

// crates/core/src/workflow/artifacts.rs
impl ArtifactSpec { pub fn listed(specs: &[ArtifactSpec]) -> String; }   // "`brief.md`, `notes.md`" — una copia, la que ArtifactKind::listed ya tiene
impl ReservedIdentity { pub fn reference(self) -> String; }   // cómo una referencia de contexto nombra lo que el engine escribe: hoy "name: questions.answers.yaml"; con 4-02 "kind: answers"

// crates/core/src/workflow/node.rs — `interactive` se retira del struct y de la lista de claves: un YAML de autor que lo escriba
// recibe el rechazo de clave desconocida nombrándola (D110). crates/engine/src/human_interaction.rs:
pub trait HumanInteraction: Send + Sync {
    async fn resolve(&self, escalation: &GateWaitingPayload) -> Option<HumanChoice>;
    async fn ask(&self, questions: &QuestionsFile) -> Option<QuestionsReply> { None }   // sin `interactive`
}
```

Un `questions` con cero preguntas no pregunta: el nodo termina en el mismo
cierre y el engine acepta un documento de respuestas vacío con origen
`Derived`, para que el nodo siguiente monte siempre lo que declaró montar.
`ArtifactKind::Answers` (4-02, M12) es lo que el nodo siguiente monta por kind;
`AnswersFile::against(&QuestionsFile, Vec<Answer>) -> Result<Self, Report>` es
la puerta de una respuesta, porque `shape::accept` no ve las preguntas, y
`Document for AnswersFile` cubre sólo lo intra-documento. Con `Answers` como
kind llegan dos reglas más: `AnswersFromNodeThatNeverAsks { node, source }` y
`AnswersDeclaredAsProduced { node }` (`ArtifactKind::declarable()` es `false`
para `Answers`: lo escribe el engine).

---

## 3. El hecho del log

```rust
// crates/core/src/events/gates/payloads.rs  (hasta 2-01: crates/core/src/events/payloads.rs)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct QuestionsAskedPayload {
    /// The questions document the node handed over: what the answers answer.
    pub questions_hash: ContentHash,
    /// The ids awaiting an answer. Never empty: a node with nothing to ask finishes instead.
    pub questions: NonEmpty<QuestionId>,          // W-11: Vec<QuestionId>, y `new` rechaza la lista vacía
    /// What the session that asked spent. The `node_finished` after the answer carries zero.
    pub tokens_used: TokenUsage,
}
impl QuestionsAskedPayload { pub fn new(questions_hash: ContentHash, questions: NonEmpty<QuestionId>, tokens_used: TokenUsage) -> Self; }   // M03: QuestionsAsked::new
pub struct QuestionsAnsweredPayload { pub answers_hash: ContentHash, pub channel: Channel, pub responder: Option<Responder> }   // existe; M03: QuestionsAnswered::via
pub enum Channel { Tty, Mcp }   // existe, sin cambios (D167); `Pr` llega con A-14
// wire: {"kind":"questions_asked","questions_hash":"sha256:…","questions":["q1","q2"],"tokens_used":{…}}
// KINDS: `questions_asked` inmediatamente antes de `questions_answered`

// crates/core/src/events/gates/ledger.rs (M04, 2-03)
pub struct GateRecord { waiting: Option<(Escalation, Seq)>, resolved: Vec<(GateResolvedPayload, Seq)>, external_ref: Option<String>, approved_sha: Option<CommitSha>, rounds: Vec<QuestionRound> }
pub struct QuestionRound { pub asked: (QuestionsAskedPayload, Seq), pub answered: Option<(QuestionsAnsweredPayload, Seq)> }
impl GateLedger {
    pub fn pending_questions(&self, node: &NodeId) -> Option<&QuestionsAskedPayload>;   // la última ronda sin respuesta
    pub fn answered_unfinished(&self, node: &NodeId) -> bool;                            // respondida después del último node_started y sin terminal después
}
// W-11, hasta 2-03: RunState { …, pub answered_unfinished: BTreeSet<NodeId> } derivado en replay.rs::apply

// crates/core/src/events/gates/happening.rs (M19, 5-05)
pub enum Happening { Escalated(Escalation), Resolved(GateResolvedPayload), Asked { questions: NonEmpty<QuestionId> }, Answered { channel: Channel, responder: Option<Responder> } }
```

**Derivación** (`replay.rs::apply` en W-11; `NodeLedger::apply` en 2-03/2-04):

| evento | estado del nodo | además |
|---|---|---|
| `questions_asked` sobre `Running` | `Waiting { external_ref: None }`, guardando el estado previo como `gate_waiting` | `total_tokens += tokens_used`; cierra la contabilidad del intento |
| `questions_asked` sobre otro estado | `ReplayError::AskedWithoutStart { seq, node }` | |
| `questions_answered` sobre `Waiting` | restaura el estado previo (`Running`); `answered_unfinished` gana el nodo | |
| `questions_answered` sobre otro estado | `ReplayError::AnsweredWithoutAsk { seq, node }` | |
| `node_finished` después de responder | `Finished { tokens: questions_asked.tokens_used + node_finished.tokens_used }` | `answered_unfinished` pierde el nodo |
| `node_failed` | `Failed`, siempre | `Aux.pending_questions` y su cálculo se borran |
| `node_started` | `Running { attempt }` | `answered_unfinished` pierde el nodo |

La contabilidad de un intento cierra en el primero de `questions_asked`,
`node_finished` o `node_failed` después de su `node_started`, en un solo lugar
por lector: `replay.rs` (`total_tokens`), `live.rs::in_flight_tokens`,
`stats.rs::walk_attempts`. Con M04 los tres leen `NodeLedger`.

---

## 4. El cierre y la ronda

```rust
// crates/engine/src/run/node_exec.rs
pub(super) enum NodeEnd { Finished, Failed, Interrupted, ChildPaused { reason: String },
    /// Closed in full and waiting on its answers: no terminal event yet; the next `decide` sees `Waiting` and asks.
    Asked }

// crates/engine/src/run/node_close.rs
pub(super) async fn close_node(ctx: &RunCtx<'_>, node: &Node, close: Close<'_>) -> Result<NodeEnd, RunError>;
//   hooks after → scope → close_artifacts → record_artifacts → asked(verified):
//     Some(ids) no vacío → QuestionsAsked::new(hash, ids, tokens) · NodeEnd::Asked
//     Some(vacío)        → accept(AnswersFile { answers: [] }, RecordedOrigin::Derived) · finish_node
//     None               → finish_node
pub(super) async fn finish_node(ctx: &RunCtx<'_>, node: &Node, outcome: impl Into<String>, tokens: TokenUsage) -> Result<NodeEnd, RunError>;
//   el único emisor de node_finished (+ write_progress). W-11: lo llaman close_node y el paso FinishAnswered; 2-02 (M03) absorbe gate_exec.rs:188,490,570.
// crates/engine/src/run/node_artifacts.rs
pub(super) fn asked(verified: &[VerifiedArtifact]) -> Option<(ContentHash, Vec<QuestionId>)>;   // reemplaza pending_questions: None = no declara questions

// crates/engine/src/answers.rs — la única puerta por la que una respuesta entra al run
pub struct AnswersRecorded { pub answers_hash: ContentHash }
pub async fn record(log: &RunLog<'_>, run_dir: &Path, node: &NodeId, questions: &QuestionsFile, reply: QuestionsReply) -> Result<AnswersRecorded, Report>;
//   AnswersFile::against(questions, reply.answers)? → accept(answers, RecordedOrigin::Answered) → QuestionsAnswered::via(hash, reply.channel, reply.responder)
//   La llaman execute_ask (consola) y mcp::answer_questions (MCP). Todo o nada por respuesta: un Report no toca el log.

// crates/engine/src/run/questions_exec.rs — la ronda y nada más
pub(super) enum AskOutcome { Answered, Unanswered(PauseReason) }   // W-11: Unanswered { reason: String }, una sola frase en un solo sitio
pub(super) async fn execute_ask(ctx: &RunCtx<'_>, node: &Node) -> Result<AskOutcome, RunError>;
//   relee el documento questions que el run tiene (ArtifactLedger) y exige held.content_hash == asked.questions_hash, si no RunError::Broken;
//   ask(file) → None: Unanswered(Questions { node, pending }); Some → answers::record → Err(report): Unanswered(AnswersRefused { node, report }); Ok: Answered.
//   Nunca node_started, nunca node_finished, nunca hooks/scope/close_artifacts: ya corrieron.

// crates/engine/src/run/schedule.rs (W-11: next_step; 3-02: decide/waiting_step/answered_step)
pub enum ScheduleStep { …, AskQuestions { node: NodeId },
    /// Answered and owed its terminal: the ask round, a resume after a crash between the answer and the finish, and an MCP pre-seeded answer all land here.
    FinishAnswered { node: NodeId } }
//   0b. Waiting + node.asks() → AskQuestions · 0c. Running + state.answered_unfinished(node) → FinishAnswered (antes de la sección de huérfanos: un nodo respondido no es un huérfano)
//   `declares_questions` se borra: `Node::asks` es el predicado.
// crates/engine/src/run/steps.rs: FinishAnswered → node_close::finish_node(ctx, node, "questions answered", TokenUsage::default())
// crates/engine/src/run/parallel_exec.rs: NodeEnd::Asked de un hijo → RunError::Broken nombrando al hijo y la regla de check que lo impide

// crates/core/src/events/run/payloads.rs (M06, 3-01)
pub enum PauseReason { …, Questions { node: NodeId, pending: NonEmpty<QuestionId> }, AnswersRefused { node: NodeId, report: Report } }
// Display: "node `grill` asked 2 questions awaiting an answer: q1, q2" (text::counted, 4-03) · "node `grill`'s answers were refused: <report>"
```

**Cómo cierra un nodo que pregunta.** `grill` declara `[questions]`; su
sesión entrega q1, q2.

(a) con consola (TTY):

| seq | evento | `grill` derivado |
|---|---|---|
| 1 | `node_started { attempt: 1 }` | Running{1} |
| 2–4 | `agent_session_opened` … `artifact_submitted { questions }` · `artifact_accepted { Interpreted{Questions}, h_q, Submitted }` | Running |
| 5 | `hook_executed { after }` · `scope_checked` (el cierre entero) | Running |
| 6 | `questions_asked { questions_hash: h_q, questions: [q1, q2], tokens_used: T }` | Waiting; total += T |
| 7 | `artifact_accepted { answers, h_a, Answered }` | Waiting |
| 8 | `questions_answered { answers_hash: h_a, channel: tty, responder: eulke }` | Running{1}, respondido |
| 9 | `node_finished { outcome: "questions answered", tokens_used: 0 }` | Finished{tokens: T} |

(b) sin superficie (headless), después `yunta resume` con TTY: 1–6 iguales; 7 `run_paused { Questions { grill,
[q1, q2] } }`; el proceso termina. Resume: 8 `run_resumed { policies: [] }`
(`Waiting` no es huérfano) → `AskQuestions` → 9 `artifact_accepted { answers }`
· 10 `questions_answered` · 11 `node_finished`. Ningún `node_started` después
del 1.

(c) por MCP: el run está en (b) seq 7; `answer_questions` valida y pre-siembra
8 `artifact_accepted { answers }` y 9 `questions_answered { channel: mcp }`
desde el proceso de `yunta mcp`; `resume_run` → derive → Running respondido →
`FinishAnswered` → 10 `node_finished`.

(d) crash: después de 6 → `Waiting` → `AskQuestions` relee el documento y
pregunta de nuevo, sin estado conversacional y sin sesión; después de 8 →
Running respondido → `FinishAnswered`, sin sesión; después de 9 → Finished. En
ningún corte se vuelve a abrir una sesión: el nodo cerró cuando preguntó.

Una re-ejecución del nodo (`on_failure.goto` hacia él, `restart_node` sobre un
`Running` de verdad) abre otra sesión y otra ronda; la aceptación vigente de
`answers` es la última, como la de todo artifact, y `questions_asked.questions_hash`
ata cada ronda a su documento.

---

## 5. Las superficies

**Consola.** `ConsoleInteraction::ask(file)` pregunta en el lugar cuando la
consola está, pregunta por pregunta; sin consola, `None` es la convención que
ya significa "esta superficie no puede preguntar ahora" y el run se estaciona
con sus preguntas registradas. `interactive` se retira: era el resto del nodo
conversacional que D86 descartó, y un nodo que declara `questions` ya dijo
todo lo que hay que decir. `crates/cli/src/ask/form.rs` sigue validando
pregunta por pregunta con la misma puerta (`AnswersFile::against` sobre un
documento de una pregunta).

**MCP.** `answer_questions { run_id, node, answers: [{ id, value }] }`, la
segunda superficie de la misma puerta (`engine::answers::record`), como
`resolve_gate` es la segunda superficie de `HumanInteraction::resolve`:
lee las preguntas que el run tiene, valida, pre-siembra la aceptación y el
`questions_answered { channel: Mcp, responder }`, y devuelve el `Report`
cuando la respuesta no sirve, sin tocar el log. `resume_run` termina el nodo.
Cierra M24 I-01; `Channel::Mcp` queda como D167 lo fija. Ítem 5-06.

**Crónica y estado.** `questions_asked` → `Gates::Asked` → `? grill — waiting —
asked 2 questions: q1, q2`; `questions_answered` → `Gates::Answered` → `+ grill
— answered by eulke via tty`; el `node_finished` que sigue dice `finished`
como todos. `NodeDisplay` de un `Waiting` que preguntó muestra los ids desde
`GateLedger::pending_questions` (5-05); hasta entonces, `waiting`. `docs/concepts.md`:
`waiting` es un gate publicado o un nodo que preguntó.

---

## 6. El pack y los ejemplos

`grill` produce `[questions]`. Un nodo nuevo `brief` (`kind: prompt`, `runner:
planner`, `depends_on: [grill]`, `context: [{ artifact: { node: grill, kind:
questions } }, { artifact: { node: grill, name: questions.answers.yaml } }]`
—con 4-02, `kind: answers`—, `produces: [brief.md]`, prompt "Write the brief
from the questions and their answers.") escribe el brief; `plan` depende de
`brief` y monta `{ node: brief, name: brief.md }`; los tres `modes` incluyen
`brief`. El prompt de `grill` deja de decir "Once answered, write the brief".
El mismo corte en `crates/core/tests/fixtures/build-feature.yaml` (12 nodos;
`crates/core/tests/integration.rs` cuenta 12 e `implement` es `nodes[4]`) y en
`docs/design/referencia-schema.md`. El fixture del pack entrega `questions: []`
en la sesión de `grill` —así `yunta test` no espera a nadie y `brief` monta el
documento vacío— y una sesión de `brief` (`match_prompt_contains: "Write the
brief"`) escribe `brief.md`; los tres casos del pack esperan `brief: finished`.
`crates/engine/tests/factory_packs.rs` gana la sesión de `brief`.

---

## 7. Archivos

- nuevo: `crates/engine/src/answers.rs`; `crates/engine/tests/run_questions_close.rs`;
  `docs/design/adr/D173-un-nodo-que-pregunta-pregunta.md`; con 2-01, los
  brazos de `gates/{kinds,payloads,ledger,happening}.rs`; con 5-06,
  `mcp::tool_answer_questions`.
- modifica: `crates/core/src/workflow/node.rs` (`asks`; `interactive` fuera
  del struct y de la lista de claves); `crates/core/schemas/workflow.json`
  (regenerado); `crates/core/src/workflow/node_kind.rs:105` (rustdoc);
  `crates/core/tests/integration.rs` (sin `grill.interactive`);
  `crates/engine/tests/common/mod.rs` (`ScriptedAnswers::ask` sin el
  parámetro); `crates/core/src/workflow/artifacts.rs`
  (`ArtifactSpec::listed`, `ReservedIdentity::reference`);
  `crates/core/src/events/payloads.rs` (`QuestionsAskedPayload`, `new`);
  `crates/core/src/events/mod.rs` (variante, `KINDS`, `kind_name`);
  `crates/core/schemas/events.json` (regenerado); `crates/core/tests/events.rs`
  (37, la lista); `crates/engine/src/replay.rs` (brazos nuevos, `node_failed`
  siempre `Failed`, `answered_unfinished`, tokens, `ReplayError::{AskedWithoutStart,
  AnsweredWithoutAsk}`); `crates/engine/src/run/node_close.rs` (`finish_node`;
  `asked`; el `Derived` vacío); `crates/engine/src/run/node_artifacts.rs`
  (`asked`); `crates/engine/src/run/node_exec.rs` (`Asked`);
  `crates/engine/src/run/questions_exec.rs` (entero);
  `crates/engine/src/run/schedule.rs` (0c `FinishAnswered`; `Node::asks`);
  `crates/engine/src/run/steps.rs` (`FinishAnswered`, `Asked` sigue el loop);
  `crates/engine/src/run/parallel_exec.rs` (`Asked` → `Broken`);
  `crates/engine/src/live.rs:199-225` y `crates/engine/src/stats.rs:450-474`
  (la contabilidad cierra en `questions_asked`); `crates/engine/src/check/error.rs`
  (cuatro variantes; 4-02 dos más), `check/declarations.rs`
  (`check_asking_nodes`), `check/gates.rs` (`check_no_questions_in_parallel`),
  `check/mod.rs`; `crates/engine/src/human_interaction.rs` (`ask(&QuestionsFile)`);
  `crates/cli/src/human_interaction.rs:106` (misma firma);
  `crates/cli/src/surface/lines.rs:160-163` (`questions_asked` → "asked 2
  questions: q1, q2"); `crates/cli/src/commands/mcp.rs` (5-06);
  `crates/engine/tests/run_questions.rs` (las aserciones que hoy esperan
  `node_failed` y un segundo `node_started`); `crates/engine/tests/replay.rs:416-460`
  (ambos logs derivan `Failed`); `crates/engine/tests/properties.rs` (el
  generador produce el par); `crates/engine/tests/factory_packs.rs:135-183`;
  `packs/fragua/.yunta/workflows/build-feature.yaml`,
  `packs/fragua/.yunta/tests/fixtures/build-feature.yaml`,
  `packs/fragua/.yunta/tests/*.yaml`, `packs/fragua/README.md`;
  `crates/core/tests/fixtures/build-feature.yaml`;
  `crates/core/tests/integration.rs:56-61`; `docs/design/referencia-schema.md`;
  `docs/design/contrato-del-run.md` (§3 fila del par; §3.2; §4.1: el nodo
  declara `questions` y nada más, `check` rechaza lo demás, `interactive` no
  existe, las respuestas son artifact del nodo que
  preguntó, respondible por consola y por la tool MCP, por pull request A-14);
  `docs/design/spec-events.md` (§0 cuenta 37; §5.19 el par);
  `docs/concepts.md` (`waiting`); `docs/guide.md` (4-02: `answers` lo escribe
  el engine); `docs/design/adrs.md:95` (nota Revisada en D86);
  `docs/design/deuda-consciente.md` (A-14 cita D173);
  `docs/design/plan-de-raiz/cronica.md` (filas del par).
- borra (§0.15, reemplazado): `Aux.pending_questions` y el bloque
  `asks_questions` de `replay.rs:120-122,185-196`; la emisión de
  `node_started`/`node_finished` y el conteo de attempt de
  `questions_exec.rs:101-120,158-166`; el `fail_with_tokens("asked N…")` de
  `node_close.rs:165-180` (queda una sola frase, en `questions_exec.rs`, y con
  3-01 ninguna: `PauseReason` la produce); `node_artifacts.rs::pending_questions`;
  `schedule.rs::declares_questions`; con 4-02, `ReservedIdentity::Answers`,
  `answers_artifact()` y `validate_answers` (`AnswersFile::against` los
  reemplaza, también en `cli/src/ask/form.rs:135,157`).

---

## 8. Tests

Rojo primero, con la razón:

- `crates/engine/tests/run_questions_close.rs::a_node_that_asks_records_questions_asked_and_no_terminal_event`
  — hoy el log lleva `node_failed { Message }` y no existe el kind.
- `::a_node_failed_after_a_questions_artifact_derives_failed_not_waiting` —
  el log de L-07 (aceptación de `questions` + `node_failed { Artifacts
  [brief.md Missing] }`); hoy `replay.rs:242` deriva `Waiting`.
- `::the_answer_round_finishes_the_node_without_a_second_node_started` — hoy
  `questions_exec.rs:116-120` emite `node_started { attempt: 2 }`.
- `::a_node_that_asks_runs_its_after_hooks_and_scope_check_exactly_once` — un
  `hook_executed { after }` y un `scope_checked` antes del `questions_asked` y
  ninguno después; hoy la ronda no cierra por `close_node`.
- `::an_answered_node_owed_its_finish_is_finished_on_resume_without_a_session`
  — log cortado entre `questions_answered` y `node_finished`, resume con un
  fixture sin sesiones; hoy `resume_policies` lo reinicia como huérfano.
- `::the_finished_node_carries_what_the_asking_session_spent` — `Finished.tokens`
  es `questions_asked.tokens_used`; `live_total_tokens` no cuenta dos veces
  mientras espera.
- `::a_questions_document_that_asks_nothing_leaves_empty_derived_answers_and_never_waits`
  — hoy no existe artifact de respuestas y el nodo siguiente falla.
- `::the_node_that_follows_reads_the_questions_and_the_answers_of_the_node_that_asked`
  — el corte `grill`/`brief` con una respuesta; hoy `brief` no existe.
- `crates/core/tests/strict_keys.rs::interactive_is_not_a_node_key` — un
  workflow con `interactive: true` se rechaza nombrando la clave; hoy parsea.
- `crates/engine/tests/check.rs::a_node_that_asks_questions_declares_nothing_else`,
  `::the_refusal_spells_the_split_and_how_to_read_the_answers`,
  `::only_a_prompt_node_asks`, `::a_node_that_asks_is_refused_inside_a_parallel_group`
  — hoy `check` acepta los cuatro.
- `crates/engine/tests/replay.rs::a_log_written_before_origins_derives_the_artifacts_a_newer_one_does`
  — la aserción de `Waiting` (:455-459) pasa a `Failed`.
- `crates/engine/tests/properties.rs::an_ask_answered_after_any_crash_point_derives_one_finished_node`
  — para todo corte del log de (a), replay + los eventos restantes derivan
  `Finished` con exactamente un `node_finished` y ninguna sesión nueva; y
  `payload()` genera el par, con lo que `derive_is_deterministic`,
  `derive_is_prefix_monotonic` y `derive_is_deterministic_from_any_prefix`
  lo cubren.
- `crates/core/tests/events.rs::there_are_exactly_37_kinds_with_distinct_names`,
  `::kind_names_match_the_spec_exactly` — rojo hasta el kind.
- `crates/cli/tests/mcp.rs::answer_questions_pre_seeds_the_answer_and_resume_finishes_the_node`
  (5-06) — hoy no existe la tool.
- `crates/cli/tests/docs_sync.rs::every_yaml_example_in_the_docs_is_one_the_binary_accepts`
  — rojo con la regla nueva hasta que `referencia-schema.md` parta `grill`;
  `yunta test` en `packs/fragua` — rojo hasta el corte del pack.
- 4-02: `crates/engine/tests/check.rs::answers_read_from_a_node_that_never_asks_is_refused`,
  `::answers_cannot_be_declared_as_produced`; `crates/core/tests/shape.rs::answers_are_judged_against_their_questions`.

---

## 9. W-11 y las fases

**W-11, antes de la fase 0**, en los lugares de hoy y con los nombres de
arriba: `Node::asks`; las tres reglas de `check`; `interactive` retirado del
nodo, del trait y del schema; el kind `questions_asked`
por el flujo F1 tal como existe hoy —`payloads.rs`, `mod.rs`, `events.json`,
Contrato §3, spec-events §5.19, los literales 36→37— con `Vec<QuestionId>` y
un `new` que rechaza la lista vacía (2-01 lo migra a `gates/` como a los otros
36: un kind entra por F1 o no entra); `replay.rs` con la tabla de §3 y
`answered_unfinished`; `close_node` con `asked`, el `Derived` vacío,
`questions_asked` y `NodeEnd::Asked`; `finish_node` para `close_node` y
`FinishAnswered`; `engine::answers::record`; `execute_ask` sin ciclo de nodo;
`next_step` con 0c; `parallel_exec` con `Asked`; la contabilidad en `live.rs`
y `stats.rs`; la línea de `lines.rs`; el
corte `grill`/`brief` en pack, fixture canónico y `referencia-schema.md`;
Contrato §3/§4.1, spec-events, `concepts.md`; D173 y la nota en D86; todos
los tests de §8 salvo los marcados 4-02 y 5-06. Con eso el pack de referencia
corre de punta a punta. Bajo §0.10 el contador de `format!` que construye
`reason` baja en uno (`node_close.rs` pierde su copia).

**Espera a su fase:** `NonEmpty` y `QuestionsAsked::new` como constructor de
dominio, `finish_node` absorbiendo `gate_exec.rs:188,490,570` (2-02, M03);
`GateRecord.rounds`, `pending_questions`, `answered_unfinished` en `GateLedger`
(2-03, M04); `apply` exhaustivo por dominio (2-04, M05);
`PauseReason::{Questions, AnswersRefused}` (3-01, M06); `waiting_step` y
`answered_step` en `decide` (3-02, M07); `#[instrument]` en `execute_ask`
(3-05, M10); `ArtifactKind::Answers`, `AnswersFile::against`, el montaje por
kind y sus dos reglas (4-02, M12); `text::counted` en la frase (4-03, M13);
`Gates::{Asked, Answered}` y el modificador de `NodeDisplay` (5-05, M19);
`answer_questions` (5-06, M17/M18); el generador completo (6-04, M21).

---

## 10. Encastre

- **M03**: gana `QuestionsAsked::new(questions_hash, NonEmpty<QuestionId>, TokenUsage)`
  en `gates/payloads.rs`; la lista de emisores de `NodeFinished` pierde
  `questions_exec.rs:160` y dice "`node_close::finish_node`, único emisor".
- **M04**: `GateRecord` gana `rounds: Vec<QuestionRound>`; `GateLedger::{pending_questions,
  answered_unfinished}`; la fila "`questions_exec.rs:106-120` (attempt) pasa a
  leer `NodeLedger`" se borra: la ronda no cuenta intentos; `NodeRecord.tokens_closed`
  suma `questions_asked.tokens_used`.
- **M06**: `PauseReason` gana `Questions { node, pending }` y `AnswersRefused
  { node, report }`; la fila `validate_answers -> Vec<Diagnostic>` pasa a
  `AnswersFile::against -> Result<Self, Report>`.
- **M07**: `waiting_step` decide `AskQuestions` por `Node::asks`, y un paso
  `answered_step` decide `FinishAnswered` por `GateLedger::answered_unfinished`
  antes de `orphan_step`.
- **M12**: la fila `Vec<QuestionId>` cita `QuestionsAskedPayload.questions`
  como primer consumidor; `ArtifactKind::Answers` gana `declarable()`, el
  montaje `kind: answers`, `AnswersFromNodeThatNeverAsks`,
  `AnswersDeclaredAsProduced`; "`validate_answers` desaparece en favor de
  `shape::accept`" se corrige: la puerta es `AnswersFile::against`.
- **M13**: `answerer(_, Answers) = Answerer::Log`; `workflow::read` lleva las
  reglas de `check_asking_nodes` con las demás.
- **M17/M18**: `answer_questions` como tool MCP con `CliError` y una sola
  puerta (`engine::answers::record`), igual que `resolve_gate`.
- **M19**: `Gates::Asked`, `Gates::Answered` y sus filas.
- **M24**: I-01 se construye en 5-06; I-07 se retira aquí (`interactive`
  deja de existir, con la nota en D86).
- **D173 revisa D86.** Nota en D86: *(Revisada por D173: el hecho de preguntar
  es `questions_asked`, par de `questions_answered`, y el nodo espera entre los
  dos sin segundo `node_started`; un nodo que declara `questions` no declara
  otro artifact, e `interactive` se retira: la superficie disponible decide
  cómo se presentan las preguntas.)*

---

## 11. Cierra

EN-D27 `replay` deriva `Waiting` de cualquier `node_failed` tras un artifact
`questions`; EN-D28 la ronda de respuestas cierra el nodo fuera de
`close_node`, con `node_started` y `node_finished` propios; EN-D29 un hijo de
`parallel` que pregunta nunca es preguntado (`next_step` recorre sólo
`workflow.nodes`); EV-D20 el log registra la respuesta y no la pregunta;
AR-D19 un nodo que no preguntó nada no deja respuestas y el nodo siguiente
falla al montarlas; CO-21 `interactive: true` se acepta y ninguna superficie
lo lee; DO-D45 el pack, el fixture canónico y `referencia-schema.md` declaran
`[questions, brief.md]` con un prompt que espera un turno que D86 no da. M24
I-01 (5-06) e I-07. A-14 queda abierta y más barata: el par
`questions_asked`/`questions_answered` es la forma que una forja publica y
lee, como `gate_waiting`/`gate_resolved`.

---

## 12. Lo que descarta

- **Una segunda sesión con las respuestas en contexto** (L-07, alternativa 2;
  el diseño "continuación" del panel): contradice "termina" de D86, exige
  decidir contexto, `attempt`, tope de rondas y reanudación de esa sesión, y
  su W no es subconjunto de nada que una fase escriba; el nodo siguiente ya
  recibe las respuestas como contexto, con una sesión fresca y sin capacidad
  de adapter.
- **Derivar la espera de la aceptación de las respuestas, sin kind** (el
  diseño "tipo"): `node_finished` dejaría de significar terminado —la crónica
  diría `finished` de un nodo que `status` dice `waiting`—, la señal cruzaría
  otra vez del ledger de artifacts al estado del nodo, que es la forma del
  defecto, y un crash entre la aceptación y `questions_answered` perdería en
  silencio quién respondió.
- **`Failure::Questions` en `node_failed`**: preguntar no es fallar; `Failure`
  se conserva (§0.5), el scheduler tendría que excluir esa variante del
  re-ruteo, la crónica diría `x grill — failed`, y un hecho del dominio `gates`
  viajaría en un evento del dominio `node`.
- **Darle a `interactive` el consumidor que nunca tuvo** (la consola pregunta
  en el lugar sólo con el flag, y `check` lo rechaza sin `questions`): un
  flag más para declarar lo que `questions` ya dice, y un caso —"miro el run
  pero no quiero que me interrumpa"— que se resuelve no mirando.
- **Retirar `Channel::Mcp`** (los tres diseños del panel): D167 lo conserva
  y la tool que lo produce es una segunda superficie de una puerta que ya
  existe; se construye en 5-06.
