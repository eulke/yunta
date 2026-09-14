# Plan de raíz

El plan vigente de corrección arquitectónica de Yunta. Es la especificación de
todo el trabajo que sigue en la rama `feature/improve-ux`: cada ítem se
implementa tal como está escrito acá, con los nombres, los archivos y los tests
que acá se nombran. Lo que este documento no dice, no se inventa: se levanta
(§0) y decide un humano.

Fuente: ocho auditorías independientes sobre `80abe93` —eventos, engine,
artifacts, CLI, adapters, core, tests, documentación— con 198 defectos citados
por archivo y línea, reducidos a doce vicios y veintiséis mecanismos.

## Qué hay en este directorio

| archivo | qué es | quién lo lee |
|---|---|---|
| [`README.md`](README.md) (este) | el plan: régimen, diagnóstico, vicios, arquitectura, flujos, decisiones, fases, tablero, levantamientos, índice | todos, entero, antes de tocar nada |
| [`mecanismos.md`](mecanismos.md) | los 26 mecanismos con firmas exactas, archivos que tocan (nuevo · modifica · borra), tests y defectos que cierran | quien implementa un ítem, la sección del mecanismo que el ítem nombra |
| [`cronica.md`](cronica.md) | M19 completo: tipos, tabla kind→momento→kept, palabras, disposiciones, pase del pintor, archivos, tests, ADR D164 | quien implementa 5-05 |
| [`cerco.md`](cerco.md) | M25 completo: vocabulario, tipos, juez, codec, `yunta fence`, engine, adapters builtin, la muestra del mercado, archivos, tests, ADR D172 | quien implementa 3-08 |
| [`preguntas.md`](preguntas.md) | M26 completo: la regla en el tipo, el par `questions_asked`/`questions_answered`, el cierre y la ronda, las superficies, el corte del pack, archivos, tests, W-11, ADR D173 | quien implementa W-11 o un ítem que M26 nombra |
| [`artefactos/yunta-de-raiz.html`](artefactos/yunta-de-raiz.html), [`artefactos/cronica-del-run.html`](artefactos/cronica-del-run.html) | las dos propuestas tal como fueron aprobadas, con sus diagramas; el README y `mecanismos.md` son su forma normativa | quien quiera la versión legible |
| [`auditoria/01-eventos.md`](auditoria/01-eventos.md) … [`08-docs.md`](auditoria/08-docs.md) | las ocho auditorías, textuales, con toda la evidencia archivo:línea; están en inglés porque son evidencia y se conservan como se produjeron | quien implementa un ítem, la auditoría de su frente, para no re-auditar ni adivinar |
| [`auditoria/09-mercado-de-clis.md`](auditoria/09-mercado-de-clis.md) | los ocho CLIs relevados para el cerco (Gemini, Copilot, Cursor, OpenCode, Aider, Goose, Amp, Kimi), textuales, con URLs y lo no verificado marcado | quien implementa 3-08 o un adapter nuevo |

Ningún ítem se empieza sin haber leído este README entero, el mecanismo que el
ítem nombra en `mecanismos.md`, y la auditoría del frente. Lo que esos tres no
dicen, no existe: se levanta.

---

## 0. Régimen de ejecución

Estas reglas rigen para todo agente o persona que implemente un ítem de este
plan. No admiten interpretación.

1. **El plan es la spec.** Un ítem se implementa con los nombres de tipos,
   funciones, módulos, archivos y tests que el plan nombra. No se renombra, no
   se reubica, no se "mejora" el nombre. Si el plan dice `Escalation::new`, el
   constructor se llama `Escalation::new`.
2. **Lo que el plan no dice, se levanta.** Un agente que encuentra una
   contradicción, una imposibilidad, un diseño mejor, una dependencia oculta o
   un alcance mayor **se detiene**, escribe el levantamiento en §11 con
   evidencia (archivo:línea), alternativas y recomendación, y espera. No
   improvisa una solución "mientras tanto". No implementa una parte y deja una
   nota. No decide solo.
3. **Nada fuera del ítem.** Un PR cierra ítems del tablero (§10) y nada más.
   No se agregan mecanismos, tipos, archivos, dependencias ni flags que el plan
   no nombre. No se aprovecha "ya que estoy". Un refactor adyacente que el
   ítem no exige es drift.
4. **Las decisiones P1–P10 son bloqueantes.** Un ítem marcado como dependiente
   de una decisión no registrada en `adrs.md` no se empieza. Un agente no toma
   una decisión P por defecto, ni "provisoriamente".
5. **Lo marcado "se conserva" no se toca.** `RunLog`, el observer, `RunFrame`,
   `Failure`, `reserved::offer`, `FindingLedger`, `ArtifactLedger`,
   `GateResolvedPayload`, `string_id!`, `shape::read`, `spawn_governed`,
   `Delivery::choose`, `Curtain`, `Folded`, `render::state`, `advice`,
   `Layout`, `wait.rs`, `Terminal`, la nomenclatura de tests. Un mecanismo
   nuevo los generaliza; nunca los reimplementa ni los reemplaza.
6. **Cada ítem cierra con el gate completo ejecutado**, en este orden, por
   quien implementa: `cargo fmt --all --check`, `cargo clippy --workspace
   --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc
   --workspace --no-deps`, `cargo run -p xtask -- schema --check`, `cargo run
   -p xtask -- smells --check`, `cargo deny check`, `cargo test --workspace`,
   `yunta check` sobre todo workflow del repo y los packs, `yunta test` en el
   repo y en cada pack, `cargo check -p <crate>` por crate. Un resultado no
   ejecutado no existe.
7. **Primero en rojo.** Cada ítem nombra su test. El test se escribe primero,
   corre, falla por la razón exacta del ítem, y recién entonces se escribe el
   código. Un ítem sin su test en rojo no se considera empezado.
8. **El texto queda en presente.** Ningún rustdoc, ayuda, mensaje ni comentario
   del diff nombra este plan, una fase, un ítem, lo que había antes ni lo que
   vendrá. Este archivo es el único lugar del repo que planifica.
9. **El tablero se actualiza en el mismo PR.** Un ítem cerrado va en su
   commit y el tablero lo sigue en otro commit del mismo PR, citando el hash
   verdadero del que lo cierra —un commit no puede llevar su propio hash. Un
   ítem levantado cambia a `levantado` y apunta a §11 en el mismo commit.
10. **Prohibiciones que el ratchet mide** (§7, M22) y que un PR no puede subir:
    `_ =>` en un `apply` de ledger; un `format!` que construya `reason`,
    `summary`, `cause` o `policy_applied`; un `SessionRequest { … }` literal
    fuera de `open_session`; `Command::new("git")` fuera de `git.rs` y
    `repo.rs`; `std::fs` dentro de `async fn`; `SystemClock` o `Utc::now`
    fuera de `clock.rs` y `main.rs`; `std::env` fuera del boundary de `Env`;
    `execute_run(RunEnv` fuera del testkit; `fn event(` fuera del testkit;
    `sleep` en tests; vocabulario prohibido (tabla de CLAUDE.md); marcadores
    de tiempo o plan en texto (`for now`, `yet`, `today`, `the old`, `will`,
    `future`, `not yet`).
11. **Vocabulario.** La tabla de CLAUDE.md rige. `ledger` solo nombra un
    pliegue del log. El documento de tareas es `tasks`.
12. **Orden.** Los ítems de un PR respetan las dependencias de §10. Una fase
    no empieza hasta que la anterior de la que depende está cerrada.
13. **Leer antes de tocar.** README entero, el mecanismo del ítem en
    `mecanismos.md`, la auditoría del frente en `auditoria/`. Un agente que no
    puede citar la línea de la auditoría que motiva el ítem no lo empezó.
14. **Un ítem, un PR** (o una serie corta que el tablero enumera). El título
    del PR nombra el ítem (`W-03`, `2-01`); la descripción lista los defectos
    del índice que cierra por id y pega la salida del gate.
15. **Lo que deja de usarse se borra; lo que falta terminar se termina.**
    Sin uso no prueba que sobre. Antes de borrar algo hay que decir en cuál
    de estos tres cae, y sólo el primero se borra:
    - **Reemplazado**: otro mecanismo ocupó su lugar y nadie puede volver a
      consumirlo. Se borra en el mismo commit que lo dejó sin uso — el tipo,
      la función, la constante, el campo, el archivo, el test y el párrafo
      que lo describía. Un envoltorio que sólo delega y un alias "por
      compatibilidad" caen acá.
    - **Inconcluso**: está construido y todavía no conectado, y el sistema
      lo necesita conectado. No se borra: se termina en el ítem que lo
      nombra, o se levanta en §11 si ningún ítem lo nombra. Borrarlo
      esconde trabajo que falta.
    - **Planificado**: su consumidor llega en una fase que el plan nombra
      (§7, §10, `mecanismos.md`). No se toca hasta esa fase, y el plan es
      la prueba de que tiene destino.

    Lo que además se conserva a propósito lo dice §8 o una decisión
    registrada. Un barrido que no clasifica antes de borrar no es limpieza:
    es pérdida.

### Cómo se cierra un ítem

- [ ] Leídos README, mecanismo y auditoría del frente; citada la evidencia.
- [ ] Toda decisión P de la que depende está registrada como ADR.
- [ ] El test nombrado por el ítem existe, corrió y falló por la razón del ítem.
- [ ] El código usa los nombres del plan; no hay tipo, archivo, mecanismo ni
      dependencia que el plan no nombre.
- [ ] Nada marcado "se conserva" fue reimplementado ni reemplazado.
- [ ] Ningún texto del diff nombra el plan, una fase, un ítem, lo que había o
      lo que vendrá.
- [ ] Gate completo (§0.6) ejecutado, con salida en el PR.
- [ ] `xtask/smells.baseline` regenerado por medición; ningún contador subió.
- [ ] Tablero (§10) actualizado en el mismo commit, con el hash.
- [ ] Índice (§12): los defectos que este ítem cierra siguen apuntando a él.

---

## 1. Diagnóstico

| medida | valor |
|---|---|
| lugares donde se declara un kind de evento | 9 en el workspace + 2 docs; el compilador defiende 3 |
| literales de `GateWaitingPayload` sin constructor | 7, en 4 módulos |
| módulos que pliegan el ciclo de vida de un nodo | 19 |
| replays completos del log por iteración del scheduler | ≥ 3 (y ≥ 2 lecturas de storage) |
| subprocesos git con process group, registro o cancelación | 0 |
| capacidades declaradas que el engine nunca consulta | 3 (`edit_hooks`, `usage_reporting`, `permission_profiles`) |
| copias de "buscar el run dir" en el CLI / redacciones de "no run" | 6 / 10 |
| `execute_run(RunEnv{…})` a mano en tests / en el testkit | 46 / 1 |
| divergencias documentación↔código | 44; `docs/design/` no está atado por ningún test |
| tags publicados | 0 — por D141, reestructurar payloads hoy no cuesta ningún `_v2` |

Lo que está bien y se conserva (regla 5 de §0): el engine tiene cero
conocimiento de CLIs concretos; `RunLog` es una costura única con el observer
colgado; `Failure` y `reserved::offer` son hechos tipados con constructor
único; `FindingLedger` y `ArtifactLedger` son pliegues únicos; `GateResolvedPayload`
separa forma Rust de forma wire; `string_id!` valida todo id; `shape::read`
fusiona parsear y chequear; `spawn_governed` es el modelo de propiedad;
`Delivery::choose`, `Curtain`, `Folded`, `render::state`, `advice`, `Layout`
en el CLI; `wait.rs` y `Terminal` en el testkit; la calidad de los ADRs.

---

## 2. Los doce vicios y los veintiséis mecanismos

| vicio | síntoma principal | mecanismos |
|---|---|---|
| V1 un hecho se construye en muchos lugares | `GateWaiting` ×7, `capability_degraded` ×8, `SessionRequest` ×2 divergentes, lo escribible ×4 | M03 M08 M25 |
| V2 una pregunta se responde en muchos lugares | `last_external_ref` ×2, attempt ×6, dedup ×2, run dir ×6, `RunPhase`→palabras ×5 | M04 M15 M16 |
| V3 el catch-all silencioso | `replay::apply` `_ => Ok(())`, `phase.rs` `_ => Created`, `Waiting` derivado de cualquier `node_failed` | M05 M26 |
| V4 la declaración dispersa | un kind = 9+2 lugares | M02 |
| V5 prosa congelada en el log, texto en la capa equivocada | `run_paused.reason`, `{:?}` al usuario ×9, MCP re-bordea ×17 | M06 M17 |
| V6 la disciplina que el tipo no impone | globs `String`, `Legacy` fresco, nombre de artifact que escapa, versión no leída, un nodo que pregunta y debe otra cosa | M12 M13 M14 M26 |
| V7 la capacidad declarada y no consultada | 3 capacidades sin consulta, 5 comportamientos prometidos-no-construidos | M09 M24 M25 |
| V8 la cáscara que no gobierna | git sin grupo, 15 `std::fs` en async, 3 `SystemClock`, `target_digest` crudo | M10 M11 |
| V9 el puerto del lado equivocado | el engine importa su interfaz desde `yunta-adapters` | M01 |
| V10 los caminos duplicados | `yunta test` sin `check`, `mcp::tool_resolve_gate`, 8 `Bench` sombra, la ronda de preguntas cierra sin `close_node` | M18 M19 M20 M26 |
| V11 la documentación sin atar | `docs/design/` invisible a `docs_sync`; la config de referencia no parsea | M23 |
| V12 el ratchet que mide el síntoma | `copied_test_helpers 0` falso; propiedad de resume tautológica | M21 M22 |

### Los mecanismos

- **M01 · Puerto en core.** `yunta_core::port` define `Adapter`, `AgentSession`,
  `SessionRequest`, `RunToolsEndpoint`, `Budget`, `PermissionProfile`,
  `ProbeReport`, `AgentEvent`, `AdapterError`. `yunta_core::process` recibe
  `subprocess.rs`, `signal.rs`, `process_start.rs`. `yunta-adapters` implementa
  el puerto y no lo exporta. El engine depende de core y storage únicamente.
  `MockFixture::parse(yaml, &RunPaths)` en `adapters/src/mock/fixture.rs`.
  `yunta-testkit-core` (solo depende de core) + `yunta-testkit`. Test de
  frontera `crates/engine/tests/no_adapter_crate_in_engine.rs`. El registro de
  adapters concretos en el CLI se deriva de `adapters.keys()`, no de prosa.
- **M02 · Kind declarado en su dominio.** `core/src/events/{run,node,session,
  tasks,scope,findings,artifacts,gates,children}/` con `kinds.rs`,
  `payloads.rs`, `ledger.rs`, `happening.rs`. `EventPayload` de 9 brazos
  (`Run(RunEvent)`, `Node(NodeEvent)`, …). `wire.rs`: `EventPayloadWire` plano
  por `#[serde(from/into)]`; `KINDS` concatenado de los dominios; `JsonSchema`
  a mano que emite el mismo `oneOf` de 36 ramas en el mismo orden.
  `events.json` no cambia un byte (`xtask schema --check` lo guarda).
  `kind_name`, `all_kinds()` y los "36" se derivan de `KINDS`. Versión por
  dominio.
- **M03 · Constructor por hecho.** Un constructor público por kind, en su
  dominio, que fija el invariante: `RunPaused::new(PauseReason)`,
  `RunResumed::new(OnInterrupt)`, `RunFinished::closed(terminal, &RunState)`,
  `NodeStarted::attempt(n)`, `NodeFinished::new(outcome, tokens)`,
  `NodeRerouted::new(to, RerouteCause(Failure), origin)`,
  `Degradation::new(Capability, AdapterId, Policy)`,
  `TaskStatusChanged::to(task, status, caused_by)` / `::done(task, caused_by,
  commit)`, `Escalation::new(summary, Evidence, NonEmpty<GateOption>)`,
  `QuestionsAnswered::via(Channel, Responder)`, `ChildClosed::new(run,
  terminal, tokens)`, `ArtifactAccepted::new(id, hash, RecordedOrigin)`.
  `artifact_written` sin constructor y `#[doc(hidden)]`. `RunCtx::engine_finding`
  es el único autor de findings del engine.
- **M04 · Un ledger por dominio.** `TaskLedger`, `GateLedger`, `NodeLedger`,
  `SessionLedger`, `ChildLedger`, `DegradationLedger`, `RunLedger` junto a
  `FindingLedger`, `ArtifactLedger`, `GrantLedger`. `RunState` los sostiene a
  todos en un pase O(e). `NodeHistory`, `last_external_ref` ×2, los 6 conteos
  de attempt, las 5 lecturas de tasks, `RunArtifacts::of` ×4, los scans de
  hijos ×5 desaparecen. `blackboard.rs` consume
  `FindingLedger::effective_of`. Una sola regla de dedup (`dedup_findings`),
  consumida por `inherited_findings`. `FindingLedger::effective` se llama una
  vez al final de `derive`, no por evento.
- **M05 · Replay sin comodín.** `derive` despacha por dominio; cada
  `<dominio>::apply` es exhaustivo. Lo que no mueve estado se declara
  `Audit` por nombre en su `kinds.rs`. `phase.rs` lee `RunLedger`.
- **M06 · Hechos tipados, prosa en el borde.** `PauseReason { Escalation,
  Cancelled, BudgetExhausted{spent,cap}, ExternalGate(Url),
  UncertainOrphans(Vec<NodeId>), NodeFailed{node,failure} }`, `Policy` enum,
  `RerouteCause(Failure)`, `Capability::as_str` en vez de `{:?}`;
  `RunError::Git(#[source] GitError)`; parsers de adapters como
  `#[serde(tag = "type")]` con `#[serde(other)] Unknown` y `Option<u64>` para
  conteos ausentes; `AgentError` con causa; `Diagnostic` en vez de
  `Vec<String>` en `questions::validate_answers` y
  `permission_layer_conflicts`.
- **M07 · decide/execute separados.** `schedule::next_step` en `gate_step`,
  `waiting_step`, `orphan_step`, `failure_step`, `ready_batch` compuestas por
  `decide(&Workflow, &RunState, &Policy)`. `parallel_exec` llama
  `schedule::resume_policies`. `GateStep::Waiting(PauseReason)`: `steps`
  registra `run_paused`, el gate nunca. `RunFinished::closed` único en
  `finish(terminal)`. `current_escalation` sin segundo `next_step`;
  `current_mode_name` es `run_mode()`.
- **M08 · SessionPlan único.** `SessionPlan { node, task: Option<TaskId>,
  prompt, chosen: RunnerCandidate, profile, artifact_dir, resume:
  Option<SessionId> }`; `open_session(plan, adapter) -> (SessionRequest,
  Vec<Degradation>)` en `engine/src/run/session_plan.rs`, único lugar que
  construye un `SessionRequest`, y con él la cerca de la sesión (M25).
  `prompt_exec` y `attempt.rs` lo llaman. `TypedArtifactNeedsRunTools` rige
  en ambos.
- **M09 · Capacidad→política como tabla.** `core::port::POLICY:
  [(Capability, Absence)]` con `Absence { Resting, FailAtCheck, FailNode,
  DegradeWith(Policy) }`, una fila por variante (test que recorre
  `Capability::ALL`). `require(adapter, capability, ctx) -> Decision` en el
  engine, único consumidor. `check(workflow, config, &Adapters)`. Test
  `every_capability_round_trips_through_a_fixture` para el twin
  `FixtureCapabilities`.
- **M10 · Una cáscara.** `git.rs` por `spawn_governed`; `tokio::fs` en
  `node_close`, `distill`, `context_resolve/{sources,knowledge}`,
  `workflow_exec`, `lock.rs`, `worktree`, `criteria.rs`, `process_registry`,
  `claude_code/mod.rs`, `mock/mod.rs`; `Clock` inyectado en `worktree/mod.rs`
  (3 sitios); `SecretSource` en `RunEnv` consumido por `secrets_env` y
  `context_resolve/mcp.rs`; `Env` leído una vez en `main`; `JoinHandle`
  conservado en `MockSession`; `#[instrument(fields(run_id, node_id))]` en
  `publish_gate`, `poll_gate`, `resolve_internal_gate`, `execute_ask`;
  `tokio::spawn(fut.instrument(Span::current()))` en readers y players;
  `ctx.adapters.get(&id).ok_or(RunError::AdapterMissing)`;
  `ContentHash::short()`; toda falla de `process_registry`, `interrupt`/`kill`,
  `encode_ref`, `git::success` es un `engine_finding`; `read_registry`
  distingue corrupto de ausente.
- **M11 · Secreto.** `ToolTarget { display: Option<String>, digest:
  ContentHash }` con `display` solo para paths relativos al worktree, nunca
  `command`/`url`; pase de redacción en `RunLog::record` contra el
  `SecretSource`; `scratch/mcp.json` se borra con el registro; comparación de
  bearer en tiempo constante sobre `Secret`.
- **M12 · Parsear es validar, extendido.** `ScopeGlob`, `SchemaRange`,
  `ArtifactName` + `ReservedIdentity`, `TemplateVar` (enum; `{{runner.role}}`
  → `{{runner.name}}`), `CommitSha` en `PackProvenance`/`PackLockEntry`,
  `DateTime<Utc>` en `EngineProcessFile.started_at`, `WorkflowName`,
  `SkillName`, `InputName`, `McpServerName`, `RecordedOrigin` →
  `ArtifactOrigin`, `ArtifactKind::Answers`, `Location { path, range }`,
  `Vec<QuestionId>`, `DiagnosticCode` (una enum), `StagedHash` distinto del
  hash del store, `ArtifactRefId` con `deny_unknown_fields` por variante,
  `Answer`/`AnswersFile` estrictos, `ScopeExpansionPermissions` exportado.
- **M13 · Puertas únicas en core.** `workflow::read(bytes, path) ->
  Result<Workflow, Report>` con las reglas de `engine/src/check/` en el mismo
  `Report`; `Document` para `FindingEntry` y `Withdrawal`; `text::counted(n,
  noun)`; `Answerer { Log, Staging }`; `RunTool` enum con `name()`,
  `describe()`, `schema()` y dispatch exhaustivo; `run_dir::{manifest_path,
  progress_path, sessions_root, task_worktrees}`; `steps.rs:256` por
  `canonical::derive_findings`.
- **M14 · PersistedDoc<T>.** `schema_version` escrito **y leído**, balde
  `unknown` que aflora, en `manifest.yaml`, `yunta.lock`,
  `scratch/engine.json`, el lock de aislamiento, `receipt.json`. `Manifest`
  embebe formas persistidas tolerantes, no `Workflow`/`ConfigLayer`
  estrictos. `manifest.rs:151` reporta un `pack.yaml` ilegible.
- **M15 · Context::open_run.** `Opened { run_id, run_dir, manifest:
  PersistedDoc<Manifest>, events, project }`; `CliError::RunNotFound { id,
  roots }` con una sola oración. La usan `status`, `stats`, `receipt`,
  `verify`, `cancel`, `gc`, `graph`, `resume`, `resolve-gate`, `list --runs`,
  `drive::settle`, los 4 tools MCP y `collect_history` (que deja de tener dos
  copias).
- **M16 · Un vocabulario.** `render::state::RunWord` para el estado de un run;
  `progress::summary`, `Closing::verdict`, `Standing::heading`,
  `RunJson.outcome`, `demand_line` lo consumen; `--json` emite el mismo token
  que el texto. `Outcome` deriva de `RunWord` en un solo lugar. `RunDocument`
  versionado para run/resume/status; `receipt.json` con `schema_version`.
  `width::` en `ask/menu.rs` y `schema.rs`. `stats.rs` renderiza a `String`.
- **M17 · Un borde de texto.** `CliError` con brazos tipados; `Message(String)`
  solo para lo único; `mcp.rs` devuelve `Result<T, CliError>` y un adaptador lo
  vuelve tool result; `promote.rs` y `test.rs` tipados; `init`/`new` preguntan
  por `ask::Console`; warnings de `Console::open` por `Diagnostics`.
- **M18 · Un camino de ejecución.** `test::run_case` y `promote` entran por
  `runnable` + `drive` con `ctx.clock` y `ctx.ids`; `mcp::tool_resolve_gate`
  es `resolve_gate` con otro medio; `graph` abre storage por `ctx.storage()`;
  `check_or_refuse(&Path)`.
- **M19 · Crónica derivada.** `engine/src/view/chronicle.rs`: `Moment { seq,
  at, elapsed, node, happening }`, `Happening` de 9 brazos por dominio (cada
  dominio implementa `From<&XEvent> for x::Happening`), `chronicle(events) ->
  Vec<Moment>` pura, uno por evento, monótona. `cli/src/surface/chronicle.rs`:
  `Said`, `say`, `kept`, `graduation`. `Lines::moment`, `Region::record`.
  Desaparecen `Scrollback::{gone,leaving,restart}`, `Region::restart`,
  `view::{graduation,settled_nodes,working_nodes}`, `lines::{detail,
  resolution,counted_problems}`. `Painter` recibe `glyphs`. `Layout::aside` →
  `Layout::advice`. Tests: `every_event_is_one_moment`,
  `the_chronicle_of_a_prefix_is_a_prefix_of_the_chronicle`,
  `the_frame_agrees_with_the_chronicle`,
  `a_settled_node_carries_how_long_it_worked_and_the_children_it_bore`,
  `a_node_that_settles_twice_is_two_moments`,
  `what_a_watched_terminal_keeps_above_its_region_is_what_the_append_only_surface_writes`,
  `a_run_read_back_from_a_pipe_says_what_a_watched_terminal_kept`.
- **M20 · Un arnés por capa.** `testkit_core::Log::for_run(id).at(t).node(n,
  p).after(s).event(p).build()`; un `Bench` con `run_sabotaged`, `wake`,
  `wake_with(forge)`, `with_clock`, `with_ids`, `findings_by`, `group_output`,
  `commit_subjects`; `hermetic(cmd, dir, home)` compartido por `run_yunta` y
  `Terminal::open` (fija `YUNTA_HOME`, `YUNTA_ORG_CONFIG` a un archivo vacío,
  `HOME`, `USER`, `TERM`, quita `NO_COLOR`); `Checkout::without_yunta_home()`;
  `common/mod.rs` reducido a literales; `testkit::adapter` con `request`,
  `drain`, `write_lines`, `child_pid_fifo`, `grandchild_pid` y el test de
  secretos parametrizado; `SourceLog::record` con clock inyectado;
  `SeqIdSource` por bench; `wait.rs` cede con `sleep(1ms)`.
- **M21 · Cuatro propiedades.** Generador sobre los 36 kinds + `Unknown`
  (comparte la lista con `all_kinds()`): replay (se conserva), resume real
  (prefijo → camino de resume → estado final igual), idempotencia
  (re-entrega de subsecuencia; re-corrida tras crash = un `node_finished` por
  attempt), cadena (todo append verifica; toda mutación de un byte falla).
- **M22 · Ratchet que mide la regla.** Contadores nuevos en `xtask smells`,
  sobre `src` y `tests`: `execute_run_outside_testkit`,
  `git_command_outside_git_rs`, `system_clock_in_tests`, `sleep_in_tests`,
  `env_set_in_tests`, `event_builder_outside_testkit`,
  `bench_struct_outside_testkit`, `banned_vocabulary`, `tense_markers`,
  `test_files_over_500_lines`, `test_fns_over_50_lines`; `[workspace.lints]`
  en `Cargo.toml` con el bloque de deny (5 copias → 1); `release.yml` reusa
  `ci` por `workflow_call`; job macOS `cargo test --workspace`; `for p in
  packs/*/`; `concurrency` + `timeout-minutes`; `cargo test --release` una
  vez; CONTRIBUTING lista `smells --check`.
- **M23 · Docs atados.** `docs_sync.rs` recursivo sobre `docs/design/`: todo
  YAML parsea y chequea; Contrato §3 ≡ `KINDS`; spec-events §5.x ≡ structs;
  spec-adapter caps ≡ `Capability::ALL`, tabla de degradación completa, §6 ≡
  `capabilities()` por adapter; spec-ledger §3 ≡ `RULES`; Contrato §6.4 ≡
  `tool_definitions()` + catálogo; §7.1/§9/§2.3 ≡ variantes.
  `docs/design/adr/DNNN-slug.md` con front-matter `number, title, status,
  revises, revised_by`; `adrs.md` generado por `cargo xtask adr --check`
  (huecos, citas, recíprocos). Pase que deshace la corrupción del corpus
  (`\{\{`, `\[`, `\|`, fences `javascript`, autolinks `http://x.md`, fence sin
  cerrar en `contrato:19`). Correcciones que los tests exigen (§9). Glosario:
  `frame`, `standing`, `escalation`, `tradeoff`, `demand line`,
  `crónica`/`momento`, `puerto`. `spec-ledger.md` → `spec-tasks.md`.
- **M24 · Build-or-register.** Cada comportamiento prometido por la
  documentación y no construido se construye, o se retira con entrada `A-NN`
  en `deuda-consciente.md` y nota `Revisada` en el ADR que lo describía.
  Nunca un comentario que explique el atajo.
- **M25 · El cerco.** `yunta_core::fence::Fence { allowed: Vec<ScopeGlob>,
  roots: Vec<PathBuf> }` con `judge(worktree, target) -> Verdict`, el único
  juez de lo que una sesión escribe; `Capabilities::fence: FenceLevel { None,
  ToolCalls, Filesystem }` reemplaza `edit_hooks`; `Coverage { Exact,
  WidenedToRoots, ToolsOnly }` en `agent_session_opened`, el canal más
  débil; kind `write_refused { session_id, target }`; `FenceHook` lo arma el
  CLI y `yunta fence <adapter>` es el hook, cada adapter escribe solo el
  `FenceCodec`; las raíces salen de `fence.roots`; el cerco vive en
  `scratch_dir`, nunca bajo `cwd`; `read_only` = `allowed` vacío con raíces;
  una escritura que llega al diff bajo `Exact` es `engine_finding`.
  `edit_constraints`, `Glob`, `blocked:<path>` se borran. Especificación
  completa y la muestra de ocho CLIs del mercado: `cerco.md`.
- **M26 · Un nodo que pregunta, pregunta.** `Node::asks`; un nodo que declara
  `questions` no declara otro artifact, es `kind: prompt` y no vive en un
  `parallel` (`check` lo rechaza nombrando el corte); el hecho de preguntar es
  el kind `questions_asked` (`questions_hash`, `questions`, `tokens_used`),
  par de `questions_answered`, y el nodo espera entre los dos como un gate
  interno, sin segundo `node_started`; `close_node` lo registra y devuelve
  `NodeEnd::Asked`; `finish_node` es el único emisor de `node_finished`;
  `engine::answers::record` la única puerta de una respuesta, para la consola
  y para la tool MCP `answer_questions`; `FinishAnswered` termina un nodo
  respondido sin sesión; `node_failed` es siempre `Failed`; `interactive` se
  retira del nodo, del trait y del schema; `PauseReason::{Questions,
  AnswersRefused}`; un `questions` vacío deja respuestas vacías `Derived`.
  Especificación completa: `preguntas.md`.

---

## 3. Arquitectura objetivo

```
RunState  ← derive(events)                        // un pase, un dueño por lectura
Decision  ← decide(&Workflow, &RunState, &Policy)  // pura, total, no toca el log
Effects   ← execute(Decision, &mut Shell)          // solo I/O; produce hechos
Fact      ← <dominio>::Constructor(...)            // uno por kind
seq       ← RunLog::record(fact)                   // una costura, observer colgado
                    ↓
        run_frame(&RunState)  chronicle(events)     // dos derivaciones, un vocabulario
                    ↓
        Region · Scrollback · Lines · Closing · status · --json · MCP   // disposiciones
```

### Diagrama

```mermaid
flowchart LR
  subgraph storage
    L[(Event log<br/>wire plano · hash chain)]
  end
  subgraph core/events · por dominio
    LD[Ledgers<br/>Run Node Session Tasks Grant<br/>Finding Artifact Gate Child Degradation]
    RS[RunState<br/>= todos los ledgers, un pase O e]
  end
  subgraph engine · puro
    D[decide<br/>gate · waiting · orphan · failure · ready_batch]
    RF[run_frame<br/>estado en un instante]
    CH[chronicle<br/>qué pasó, en orden]
  end
  subgraph engine · cáscara
    EX[execute<br/>Shell: spawn_governed · tokio::fs · Clock/Env/Secrets inyectados]
    SP[SessionPlan → open_session → dispatch]
    RQ[require cap ← POLICY]
  end
  subgraph de vuelta al log
    CT[Constructores<br/>Escalation::new · Degradation::new · PauseReason …]
    RL[RunLog::record<br/>una costura, observer colgado]
  end
  L --> LD --> RS --> D --> EX --> CT --> RL --> L
  RS --> RF
  L --> CH
  EX --> SP --> RQ
  RL -.observer.-> RF
  RL -.observer.-> CH
  RF --> UI[Region · Closing · status · --json · MCP]
  CH --> UI2[Scrollback · Lines]
```

### Crates

```
yunta (cli)  → engine, adapters, core, storage        raíz de composición
yunta-engine → core, storage                          conoce el puerto, no el crate
yunta-adapters → core                                 implementa core::port
yunta-storage → core
yunta-core                                            + port + process + events/<dominio>
yunta-testkit-core → core                             FixedClock, Log, ids, Captured
yunta-testkit → core, storage, adapters, engine, cli  Bench, Checkout, Terminal
```

```mermaid
flowchart TB
  subgraph hoy
    A1[yunta cli] --> A2[yunta-engine]
    A2 -->|importa Adapter, SessionRequest, Budget, signal, process_start| A3[yunta-adapters]
    A2 --> A4[yunta-storage]
    A3 --> A5[yunta-core]
    A4 --> A5
    A6[yunta-testkit] --> A2
    A6 --> A3
  end
  subgraph objetivo
    B1[yunta cli · raíz de composición] --> B2[yunta-engine]
    B1 --> B3[yunta-adapters]
    B2 --> B4[yunta-storage]
    B2 --> B5[yunta-core · + port + process + events/dominio]
    B3 -->|implementa core::port| B5
    B4 --> B5
    B6[yunta-testkit-core] --> B5
    B7[yunta-testkit] --> B2
    B7 --> B3
    B7 --> B6
  end
```

Test de frontera: `crates/engine/tests/no_adapter_crate_in_engine.rs`.

### Eventos: nueve dominios

| dominio | kinds | ledger |
|---|---|---|
| `run` | run_created run_paused run_resumed run_finished promotion_signaled | `RunLedger` |
| `node` | node_started node_finished node_failed node_rerouted hook_executed context_assembled criteria_checked scope_checked baseline_captured | `NodeLedger` |
| `session` | agent_session_opened agent_message capability_degraded write_refused (este último nace en 3-08, no en 2-01) | `SessionLedger`, `DegradationLedger` |
| `tasks` | task_registered task_status_changed | `TaskLedger` |
| `scope` | scope_expansion_requested/granted/denied | `GrantLedger` (existe) |
| `findings` | finding_posted/updated/withdrawn/refused | `FindingLedger` (existe) |
| `artifacts` | artifact_accepted artifact_submitted artifact_written | `ArtifactLedger` (existe) |
| `gates` | gate_waiting gate_resolved questions_asked questions_answered (`questions_asked` nace en W-11) | `GateLedger` |
| `children` | child_run_created child_run_finished loop_iteration | `ChildLedger` |

Lo que un módulo de dominio es dueño de, y lo que se deriva:

```mermaid
flowchart LR
  subgraph core/events/findings/ · dueño de
    K[kinds.rs<br/>enum FindingEvent · KINDS · kind_name · schema_version · is_audit]
    P[payloads.rs<br/>structs + constructores]
    LG[ledger.rs<br/>FindingLedger::apply exhaustivo]
    H[happening.rs<br/>From&lt;&amp;FindingEvent&gt; for Happening]
  end
  subgraph core/events/ · derivado
    EP[EventPayload<br/>9 brazos]
    W[wire.rs<br/>EventPayloadWire plano · KINDS · JsonSchema]
  end
  subgraph ya no se escribe a mano
    D1[kind_name]
    D2[all_kinds]
    D3["36" = KINDS.len]
    D4[events.json]
    D5[lines::detail → chronicle::say]
  end
  K --> EP --> W --> D1 & D2 & D3 & D4
  H --> D5
```

Constructores por dominio, con el invariante que fijan: `mecanismos.md#m03`.

### Tipos: lo inválido, irrepresentable

| tipo nuevo | reemplaza | qué vuelve irrepresentable |
|---|---|---|
| `ScopeGlob` | `Node.scope`, `Task.scope`, `ScopeExpansion.within` (`Vec<String>`) | un glob inválido que pasa `check` y falla a mitad del run |
| `SchemaRange` | `Workflow.yunta_schema`, `PackManifest.yunta_schema` (`String`) | un rango que se parsea en el engine y no en el pack |
| `ArtifactName` + `ReservedIdentity` | `ArtifactSpec::Opaque(String)`, `ArtifactRefId::Name`, `MountArtifact.rename` | `..`, absoluto, o colisión con un nombre del engine; se re-valida después de renderizar |
| `TemplateVar` (enum) | `BTreeMap<String,String>` de `template_vars` | un input del usuario expandido donde un path no lo admite; `{{runner.role}}` → `{{runner.name}}` |
| `CommitSha` (existe) | `PackProvenance.commit`, `PackLockEntry.commit` | un commit que no es hex |
| `DateTime<Utc>` | `EngineProcessFile.started_at: String` | dos representaciones del instante en archivos hermanos |
| `WorkflowName` `SkillName` `InputName` `McpServerName` | `NodeKind::Workflow.use`, `Node.skills`, claves de `inputs`, `McpQueryParams.server` | una referencia que no puede resolver |
| `RecordedOrigin` → `ArtifactOrigin` | `ArtifactAcceptedPayload.origin` | `Legacy` en una aceptación fresca |
| `ArtifactKind::Answers` | `ANSWERS_SUFFIX` + `Opaque` | un documento que el engine escribe y se niega a leer |
| `Location { path, range }` | `FindingEntry.location: String` | "path y rango opcional" solo por convención |
| `Vec<QuestionId>` | `pending_questions: Vec<String>` | un id y una violación en la misma lista |
| `DiagnosticCode` | `RuleCode` + literales de parse/file/artifact + `DiagnosticCount.code: String` | un código escrito a mano sin test que lo ate |
| `Answerer { Log, Staging }` | `answered_by_the_log` + su re-derivación | una tercera lectura de quién responde por un artifact |
| `RunTool` (enum) | literales en `catalog.rs` y `session.rs` | catálogo y dispatch que se olvidan uno del otro |
| `StagedHash` | `VerifiedArtifact.content_hash` con dos significados | un campo que es dos hashes |
| `PauseReason` `Policy` `RerouteCause` | `reason: String`, `policy_applied: String`, `cause: String` | prosa del engine congelada en el log |
| `PersistedDoc<T>` | archivos persistidos sin versión leída | un lector viejo que no marca lo que no entendió |
| `FenceLevel` | `Capabilities.edit_hooks: bool` | un sandbox y un hook dichos con la misma palabra |
| `Fence { allowed: Vec<ScopeGlob>, roots }` | `edit_constraints: Option<Vec<String>>` + `artifact_dir` como permiso | un glob inválido en una sesión; dos fuentes para lo escribible |
| `Coverage` en `agent_session_opened` | nada (la sesión no decía qué cercó) | una cobertura declarada y no construida |
| `questions_asked` (kind) + `Node::asks` + tres reglas de `check` | `node_failed { Message "asked N…" }` leído como espera; `produces: [questions, brief.md]`; `Node.interactive` | una espera deducida de un fallo; un nodo que pregunta y debe otra cosa; un flag que nadie lee |

### Sesiones y capacidades

```mermaid
flowchart LR
  SP[SessionPlan<br/>node · task · prompt · chosen · profile · artifact_dir · resume] --> OS[open_session plan, adapter]
  PT[core::port::POLICY<br/>una fila por Capability] --> RQ[require cap]
  OS --> RQ
  RQ -->|Granted| SR[SessionRequest<br/>modelo y agente de chosen]
  RQ -->|Degraded| DG[Degradation::new → RunLog]
  RQ -->|Refused| RE[RunError → nodo falla]
  SR --> DS[dispatch_session · sin cambios]
```


`SessionPlan` → `open_session` → `require(cap)` por cada campo gobernado
(`fence`, `skills`, `agent`, `run_tools_endpoint`, `budget.max_turns`,
`network`) → `SessionRequest` + `Vec<Degradation>` → `dispatch_session`
(sin cambios). El cerco lo arma `Fence::for_session(profile, scope,
artifact_dir)` (M25, `cerco.md` §5).

`POLICY` (valores iniciales; una fila por variante):

| capacidad | ausencia |
|---|---|
| `resume_session` | `Resting` (sesión fresca; ya emite `capability_degraded`) |
| `fence` (`FenceLevel::None`) | `DegradeWith(PostCheckOnly)` una vez por run |
| `permission_profiles` | `FailAtCheck` cuando un nodo pide `read_only`/`edit` |
| `custom_agents` | `FailAtCheck` cuando un nodo declara `agent:` |
| `usage_reporting` | `DegradeWith(NoTokenBudget)` una vez por run |
| `skills` | `DegradeWith(NoSkills)` |
| `run_tools` | `FailNode` si el nodo declara artifact interpretado o blackboard; `DegradeWith(NoRunTools)` si no |
| `network_isolation` | `DegradeWith(NetworkOpen)` |

---

## 4. Prerequisitos: los bugs de comportamiento

Defectos que cambian lo que un run hace hoy. Los que tienen un fix propio que
es un subconjunto estricto de su mecanismo —código que la fase igual
escribiría, en el mismo lugar— son los **prerequisitos** del plan: fase W, cada
uno un ítem del tablero (W-01…W-11) con su test en rojo primero, antes de la
fase 0, para que un usuario de hoy corra. No son un plan aparte: cada fila
nombra el mecanismo del que es la primera parte, y el mecanismo la cuenta como
ya hecha. Los que no tienen fix propio van por fase o por decisión, y la fila
lo dice.

| # | bug | hoy | mecanismo | fix | test |
|---|---|---|---|---|---|
| W-01 | sesiones de `loop` en el modelo default | `task_cycle/attempt.rs:266-267` pasa `model: None, agent: None, artifact_dir: None`; `runner_resolved` registra otro modelo | M08 | `SessionSetup` gana `chosen: RunnerCandidate` y `artifact_dir: Option<PathBuf>`; `attempt.rs` copia `chosen.model`, `chosen.agent` y `artifact_dir` de ahí; `run_tools_allowed(ctx, node, adapter, id) -> Result<bool, RunToolsSetupError>` en `runner_resolve.rs` responde si el nodo puede montar las tools (y con eso rige `TypedArtifactNeedsRunTools`); `open_run_tools` y `prepare_loop` la consumen, y sólo `open_run_tools` abre un listener | `crates/engine/tests/run_concurrency.rs::a_task_session_runs_on_the_model_and_agent_the_runner_resolved`; `::a_loop_node_declaring_an_interpreted_artifact_is_refused_without_run_tools` |
| W-02 | nombre de artifact que escapa del run dir | `check/declarations.rs:133` valida el template sin renderizar; `node_exec.rs:348` renderiza `{{inputs.*}}`; `store.rs:147` une el nombre a un `PathBuf` | M12 | `core::workflow::ArtifactName::parse(&str) -> Result<Self, Problem>` (segmentos relativos, sin `..`, sin absoluto, no reservado por `ReservedIdentity`); `render_artifact_names` lo aplica **después** de renderizar y falla el nodo con `Failure::message` | `crates/engine/tests/artifacts.rs::a_rendered_artifact_name_that_leaves_the_run_dir_fails_the_node`; `crates/core/tests/artifacts.rs::an_artifact_name_with_a_parent_segment_is_refused` |
| W-03 | `target_digest` persiste comandos crudos | `claude_code/parse.rs:131-138`, `codex/parse.rs:109-138` guardan `command`/`url`/`file_path` literal | M11 | ambos parsers producen siempre `ContentHash::abbreviated()` del input; nada literal. Es el paso intermedio: M11 guarda el `ContentHash` entero y la abreviatura queda en el borde que lo muestra | `crates/adapters/tests/claude_code.rs::a_tool_use_never_persists_the_command_it_ran`; ídem en `codex.rs` |
| W-04 | blackboard muestra findings retirados | `run_tools/blackboard.rs:26-52,64-86` pliegan `finding_posted` a mano | M04 | `consolidate_blackboard` y `get_blackboard` leen `FindingLedger::of(events).effective()` filtrado por grupo; `findings::inherited_findings` llama `replay::dedup_findings` —una regla, la que colapsa espacios y mayúsculas— | `crates/engine/tests/blackboard.rs::a_withdrawn_finding_leaves_the_blackboard`; `::an_updated_finding_shows_its_last_content`; `crates/engine/tests/promotion.rs::inherited_findings_dedup_the_way_the_frame_counts_them` |
| W-05 | git sin process group ni cancelación | `git.rs:116,132,145,157,167` usan `Command::new("git")` directo | M10 | las tres funciones que un run llama (`output_bytes`, `output`, `success`) construyen `GovernedCommand` y llaman `spawn_governed` con el registro y el `CancellationToken` del run; la pareja sincrónica (`output_blocking`, `success_blocking`) corre fuera de todo run y pasa a async con `build_manifest` en 3-05 | `crates/engine/tests/process.rs::a_cancelled_run_kills_the_git_it_spawned` (git envuelto por un stub en `PATH` inyectado que espera un marcador) |
| W-06 | `parallel_exec` ignora `on_interrupt` | `parallel_exec.rs:39-47` reinicia siempre | M07 | `execute_parallel` llama `schedule::resume_policies` y honra `fail_if_uncertain`/`resume_session` | `crates/engine/tests/run_concurrency.rs::a_parallel_child_with_fail_if_uncertain_fails_instead_of_restarting` |
| W-07 | mock pierde el handle del player | `mock/mod.rs:234` `tokio::spawn` descartado; `MockSession` sin `Drop` | M10 | `MockSession { player: JoinHandle<()> }` + `impl Drop` que aborta | `crates/adapters/tests/mock.rs::a_dropped_session_stops_its_player` |
| W-08 | tests heredan `/etc/yunta/config.yaml` | `testkit/src/bin.rs:11-19` y `terminal.rs:81-88` no fijan `YUNTA_ORG_CONFIG` | M20 | `run_yunta` y `Terminal::open` fijan `YUNTA_ORG_CONFIG` a un archivo vacío bajo `home`, `USER=yunta-test`, `TERM` y quitan `NO_COLOR` — el núcleo de `hermetic()` | `crates/cli/tests/run_flow.rs::a_run_under_test_reads_no_org_config_from_the_host` |
| W-11 | un nodo con preguntas contestadas cierra debiendo lo que declaró además; el pack de referencia no corre de punta a punta | `replay.rs:242` deriva `Waiting` de cualquier `node_failed` tras un artifact `questions`; `questions_exec.rs:106-165` cierra sin `close_node` con `node_started` y `node_finished` propios; `packs/fragua`, el fixture canónico y `referencia-schema.md` declaran `[questions, brief.md]` con un prompt que espera un turno que D86 no da | M26 | `preguntas.md` §9: `Node::asks` y las tres reglas de `check`; `interactive` retirado; el kind `questions_asked` por F1; `Waiting` sólo desde `questions_asked` y `node_failed` siempre `Failed`; `close_node` registra `questions_asked`, `finish_node` único emisor, la ronda sin ciclo de nodo, `FinishAnswered`; el corte `grill`/`brief` en pack, fixture y referencia; D173 | `crates/engine/tests/run_questions_close.rs::a_node_failed_after_a_questions_artifact_derives_failed_not_waiting`; `crates/engine/tests/check.rs::a_node_that_asks_questions_declares_nothing_else` |
| — | tres capacidades nunca consultadas | AD-D2 D3 D4 | M09 · fase 3 | un quinto gate inline sería V7; depende de P3 | — |
| — | findings bloqueantes salen con 0 | CLI-D16 | M16 · fase 5 | depende de P5 | — |
| — | propiedad de resume tautológica | TE-D12 | M21 · fase 6 | ahora: renombrar a `derive_is_deterministic_from_any_prefix` y borrar el `_crashed_at_k`; la real es M21 | W-09 |
| — | `loop` sin gate de artifact tipado | EN-D3 | M08 | cae con W-01 | — |
| — | config de referencia no parsea | CO-14 | M23 · fase 0 | W-10: los números planos en `referencia-schema.md`; el recorrido recursivo de `docs_sync` es de 7-01, con `the_reference_config_parses_and_its_workflows_check` | — |

---

### El CLI: puertas, vocabulario, borde

```mermaid
flowchart LR
  C[Context::load · Env una vez] --> O[Context::open_run id<br/>Opened o RunNotFound]
  O --> D[run_frame · derive · chronicle]
  D --> V[RunWord · NodeDisplay · advice]
  V --> B[CliError · un borde<br/>MCP renderiza CliError]
  B --> X[Outcome ← RunWord → main]
  A[ask::Console<br/>init · new · gates · questions] --> B
```

### Documentación atada

```mermaid
flowchart LR
  T[Los tipos<br/>KINDS · Capability::ALL · RULES · CheckBuiltin · ContextSpec · InputSpec · tool_definitions] --> S[schemas/*.json<br/>xtask schema --check]
  T --> DS[docs_sync recursivo<br/>9 comparaciones]
  DD[docs/design/*.md<br/>citan, no restatean] --> DS
  AD[adr/DNNN-slug.md<br/>front-matter revises/revised_by] --> AI[adrs.md generado<br/>xtask adr --check]
  R[ratchets<br/>banned_vocabulary · tense_markers] --> DD
```

## 5. Los siete flujos

**F1 · agregar un kind.** Variante en `<dominio>/kinds.rs`; struct y
constructor en `<dominio>/payloads.rs`; `ledger.rs::apply` no compila sin
brazo (o `Audit` por nombre); `happening.rs` no compila sin lectura; `KINDS`,
`kind_name`, `all_kinds()`, `events.json`, los "36" se derivan; el test de docs
exige fila del Contrato y sección del spec.

**F2 · un nodo corre.** `exec` lee el log una vez → `derive` → `decide` →
`Execute(node, attempt)` (attempt de `NodeLedger`) → `NodeStarted::attempt(n)`
por `RunLog::record` → por kind: `spawn_governed`, o `SessionPlan` (F3), o
`Escalation::new` + `GateStep::Waiting(PauseReason)` → `close_node` con
`Answerer` → `NodeFinished::new`/`NodeFailed::new` → `progress.md` por
`tokio::fs` desde `RunState` → observer → `Folded` → `run_frame` +
`chronicle`.

**F3 · una sesión se abre.** `resolve_node_runner` → `SessionPlan` →
`open_session` con `require(cap)` por campo → `SessionRequest` con modelo y
agente de `chosen` → `dispatch_session` → audit events con `Result`,
`ToolTarget` digest → cancel/budget: interrupt → gracia → kill; un kill fallido
es `engine_finding`.

**F4 · un artifact vive.** `ArtifactSpec::{Interpreted(kind),
Opaque(ArtifactName)}` validado como escrito y después de renderizar →
entrega por `RunTool` (`yunta_submit_<kind> {document}`) con `shape::accept`
+ `canonical` + `accept(…, RecordedOrigin::Submitted)`, o archivo en staging →
cierre por `Answerer` → `held_document`/`verify_one`, mismo camino para
`yunta_check_artifact` → `ArtifactLedger` única respuesta → `Answers` es un
kind.

**F5 · un comando abre un run.** `Context::load()` con `Env` una vez →
`ctx.open_run(&id)?` → `Opened` o `RunNotFound` → `PersistedDoc<Manifest>` →
deriva → `RunWord`/`NodeDisplay`/`advice` → `Outcome ← RunWord` → `main` mapea
una vez.

**F6 · una capacidad degrada.** `POLICY` → `require()` →
`capability_degraded(Policy)` por `RunLog` → `DegradationLedger` →
`RunFrame.degraded`, receipt, `stats`, `verify`, crónica → `check` refuta antes
lo `FailAtCheck`.

**F7 · una afirmación queda atada.** Tipo declara → doc lista en tabla fija →
`docs_sync` compara → ratchets corren → ADR con `revises` → `xtask adr
--check` exige recíproco.

---

## 6. Decisiones que van antes (paso 2)

Registradas el 2026-09-13, con la recomendación como decisión, por
aprobación explícita del dueño del repo: D165 (P1), D166 (P2), D167 (P3),
D168 (P4), D169 (P5), D164 (P6, dentro de la crónica), D170 (P7), D171 (P8),
D172 (P9, el cerco, con la muestra de ocho CLIs del mercado), D173 (P10, un
nodo que pregunta, pregunta; registrada el 2026-09-14 con el panel de tres
diseños que la respalda).
Viven en `docs/design/adr/` y las indexa `adrs.md`. Un ítem que quiera
apartarse de una de ellas la revisa con un ADR nuevo; no la reinterpreta.

| id | pregunta | decisión | ADR · desbloquea |
|---|---|---|---|
| P1 | ¿El puerto vive en `yunta_core::port` o en un crate `yunta-port`? | `core::port` | D165 · fase 1 |
| P2 | ¿La reestructura de eventos se hace antes del primer tag? | sí, y es lo primero después de P1 (D141) | D166 · fase 2 |
| P3 | Build-or-register para: baseline al crear el run (D18, §7.2); hooks de edición (spec-adapter §6); preguntas por PR (§3, §4.1); orden de criterios aprendido del log (D62); fuentes de contexto por executor (D19) | construir baseline eager y orden desde el log; registrar como deuda A-13/A-14/A-15 los otros tres | D167 · M09, M24, fase 3 |
| P4 | `#[serde(alias = "task-ledger")]` en YAML de autor y CLI | alias solo al leer lo persistido; rechazo con diagnóstico en YAML de autor | D168 · M12, fase 4 |
| P5 | exit code de un run "finished, holding N blocking findings" | `Reported` (1) | D169 · M16, fase 5 |
| P6 | qué conserva una terminal observada (`kept`) | lo que cierra algo o pide algo a una persona | D164 · M19, fase 5 |
| P7 | umbrales sin ADR: `WAIT_DEADLINE`, stagger 60 ms, `QUEUE_DEPTH`, `REDRAW_CEILING_HZ`, `MIN_SAMPLES_FOR_ESTIMATION` | un ADR "umbrales de superficie y arnés"; el ratchet rechaza `const` numérico nuevo sin referencia a ADR | M22, fase 6 |
| P8 | los ocho fixes de §4 antes de la fase 0 | sí, cada uno como subconjunto estricto de su mecanismo | D171 · W-01…W-08 |
| P9 | ¿Cómo se cerca lo que una sesión escribe, y escala a cualquier adapter futuro? | un juez en core, un nivel por adapter, una cobertura por sesión, un rechazo como kind; el post-check sigue siendo la garantía | D172 · M25, 3-08 |
| P10 | ¿Un nodo con preguntas debe una segunda sesión con las respuestas, o declarar `questions` excluye declarar otra cosa? | excluye: un nodo que pregunta, pregunta; el hecho es `questions_asked`, par de `questions_answered`; `interactive` se retira | D173 · M26, W-11 |

---

## 7. Fases

| fase | ítems | desbloquea | depende de |
|---|---|---|---|
| W | W-01…W-11: los prerequisitos de §4 —los ocho fixes, el rename de la propiedad, la config de referencia, el nodo que pregunta | un usuario de hoy | P8, P10 |
| 0 | P1–P8 registrados como ADR por archivo; corpus des-corrompido; `docs_sync` recursivo; ratchets nuevos sembrados | que la documentación pueda perder | — |
| 1 | M01 | tabla de política en core; arnés único | P1 |
| 2 | M02 M03 M04 M05 | todo lo que deriva | P2, fase 1 |
| 3 | M06 M07 M08 M09 M10 M11 M25 | un engine que el compilador defiende | P3, P9, fase 2; 3-08 además 4-01 |
| 4 | M12 M13 M14 | `check` atrapa antes del primer token | P4, fase 2 |
| 5 | M15 M16 M17 M18 M19 | la misma palabra en cada superficie | P5, P6, fase 2 |
| 6 | M20 M21 M22 | que el ratchet signifique lo que dice | P7, fase 2 |
| 7 | M23 M24 | el primer tag | acompaña 2–6 |
| M26 | W-11 ahora; el resto reparte en 2-01…2-04, 3-01, 3-02, 3-05, 4-02, 4-03, 5-05, 5-06, 6-04 | que un nodo que pregunta corra de punta a punta en toda superficie | P10 |

Orden estricto W → 0 → 1 → 2 → 3; 4, 5 y 6 dependen de 2 y pueden ir en
paralelo entre sí; 7 acompaña. M26 no es una fase: su prerequisito es W-11 y
cada parte restante entra en el ítem de su mecanismo. El primer tag se publica después de la 7.

---

## 8. Lo que no cambia

`RunLog`, `observer.rs`, `run_frame`/`view/`, `lock` + `hand_over` (salvo su
`std::fs`), `worktree` (integridad, `branch -d`, guard en `Drop`),
`reserved::offers`, los dos builders de `escalation`, `pre_seeded_resolution`,
`process.rs::spawn_governed`, `node_close::fail_with`, `create_run`'s
`tokio::fs`; `Failure`, `Evidence`/`Fact`, `Report`/`Diagnostic`,
`GateResolvedPayload`, `EventDraft`/`StoredEvent`, `EventBody::Unknown`,
`FindingLedger`, `ArtifactLedger`, `run_mode()`, `string_id!`, `shape::read`
+ `RULES`, `FrozenPaths::new`, `Secret<T>`, `text.rs`, `yaml.rs`,
`ArtifactKind`, `InputSpec`; hash store + view, `ArtifactId`, la entrega por
tools, el split estricto/tolerante; `Capabilities::declares`,
`RunToolsEndpoint`, `staged_paths`, `SessionObserver -> Result`,
`typed_settings`, `ConfigOverride`, `LineReader`, `note_summary`, el mock como
cliente MCP real; `Delivery::choose`, `Curtain`, la política de drop del
`Feed`, `Folded`, `Region` sin raw mode, `render::state`, `advice`, `Layout`,
`render::escalation`, `json::SCHEMA_VERSION` como regla, `error::{warn,note}`
+ `Diagnostics`; `wait.rs`, `Terminal`, `repo.rs`, `Bench`/`Checkout` como
par, `RecordingObserver`, `frames.rs`, `Env::subprocess_vars`, `yunta test`
como segunda superficie, la nomenclatura de tests, la forma del ratchet.

---

## 9. Correcciones documentales que los tests de M23 van a exigir

Contrato: 6 tools MCP en §6.4; `{document}` en §6.4; `finished` en §3.2;
§5.3 sin `free_text`/`default_on_timeout` como campos y con `external_ref`;
§5.4 con la clave real del memo y la cache por invocación; §9 sin fuentes por
executor (o P3); §2 sin `baseline/` (o P3); §12 sin `ledger` del hijo.
spec-events: `commit` en §5.11, `paths` en §5.14, `external_ref` y el modelo
de `gate_resolved` con `sha` en §5.18, `model` opcional en §5.5, sin
`[inferido]`, artifacts fuera de §5.21.x, sin "precede a los tipos".
spec-adapter: 8 capacidades, tabla de degradación completa, `SessionRequest`
real, `pgid()`, `AdapterId`, §6 verdadero por adapter, O1–O6 sin duplicar.
spec-ledger → spec-tasks: 9 reglas, regla 1 en su capa, ejemplo con path
real, sin "se escribe antes del código". referencia-schema: `2000000`,
`50000000`, `32000`, `baseline.suite` igual al fixture. compatibility: 8
schemas, todos los códigos de artifact. adrs: D132 "tasks document", D139
"ocho", D152 `Revisada por D157`, D03 con reviser, D140/D144 `Revisada por
D156`, D06 sin `plugin`, D37 sin `subagente`. rfc-0002 `M14` → A-06;
rfc-0003 `deuda ⑪` → A-05. README: `graph <workflow> [--run <id>]`, `list`
sin "modes". concepts: `waiting` incluye preguntas. Rustdoc:
`session.rs:3-10`, `sections.rs:133-168`, `declarations.rs:9`,
`process_registry.rs:3`, `mcp.rs:1-2`, `cli.rs:141-145`, `replay.rs:1-8`,
`lib.rs:4,6`, `project.rs:46`, `create.rs:113`, `criteria.rs:25`,
`worktree/mod.rs:75`, `Task.id`. Comentarios de dev-dep: `storage/Cargo.toml`
(fixed clock), `cli/Cargo.toml` (`nix`). 19 tests sin `//!`.

---

## 10. Tablero

Estados: `pendiente` · `bloqueado(Pn)` · `en curso` · `levantado(§11)` ·
`cerrado(hash)`. Se actualiza en el mismo commit que cambia el estado.

Cada ítem `N-xx` implementa los mecanismos que su fase nombra en §7; la
especificación de cada mecanismo —firmas, archivos, tests— es
`mecanismos.md#mNN`, para 5-05 es `cronica.md` y para 3-08 es `cerco.md`. Los
ítems W-xx tienen su especificación completa en §4.

| ítem | qué | depende de | estado |
|---|---|---|---|
| W-01 | `SessionSetup` con `chosen` y `artifact_dir`; `run_tools_allowed` consumida por `open_run_tools` y `prepare_loop` | P8 | cerrado(a45e19a) |
| W-02 | `ArtifactName::parse` después de renderizar | P8 | cerrado(aa437f7) |
| W-03 | `target_digest` siempre `ContentHash::abbreviated()` | P8 | cerrado(56ad092) |
| W-04 | blackboard por `FindingLedger`; una regla de dedup, la que colapsa espacios y mayúsculas | P8 | cerrado(6bf7baa) |
| W-05 | `git.rs`: las tres funciones async por `spawn_governed` | P8 | cerrado(98de13f) |
| W-06 | `parallel_exec` por `resume_policies` | P8 | cerrado(1dd753b) |
| W-07 | `MockSession` con handle y `Drop` | P8 | cerrado(48b93e3) |
| W-08 | `run_yunta`/`Terminal::open` herméticos | P8 | cerrado(99f148a) |
| W-09 | renombrar la propiedad tautológica a lo que prueba | — | cerrado(afaf173) |
| W-10 | `referencia-schema.md` parsea (números planos, CO-14) | — | cerrado(fa9d791) |
| W-11 | un nodo que pregunta, pregunta (`preguntas.md` §9): `Node::asks`, las reglas de `check`, `interactive` retirado, `questions_asked`, la derivación, `close_node`/`finish_node`/`FinishAnswered`, `answers::record`, el corte `grill`/`brief` | P10 | cerrado(45aa682) |
| 0-01 | `cargo xtask adr --check` (índice generado, huecos, citas, recíprocos); D164–D171 ya escritos | — | cerrado(ebd4d16) |
| 0-02 | corpus des-corrompido (Contrato, rfc-0001, rfc-0002, rfc-0003) | — | cerrado(d9306bf) |
| 0-03 | ratchets `banned_vocabulary` y `tense_markers` sembrados | — | cerrado(8edcdeb) |
| 1-01 | `core::port` + `core::process`; engine sin `yunta-adapters`; test de frontera | P1 | cerrado(38f99dc) |
| 1-02 | `MockFixture::parse(yaml, &RunPaths)`; `commands/test.rs` y `Bench` lo usan | 1-01 | cerrado(874ea82) |
| 1-03 | `testkit-core`; `core` y `adapters` lo enlazan; `testkit::adapter` | 1-01 | cerrado(6290870) |
| 1-04 | registro de adapters derivado en `refuse_unrunnable`, `doctor`, `init` | 1-01 | cerrado(5313f8c) |
| 2-01 | dominios `run` `node` `session` `tasks` `scope` `findings` `artifacts` `gates` `children` con `kinds`/`payloads`/`ledger`/`happening`; `wire.rs`; `events.json` idéntico (37 kinds con `questions_asked`) | P2, 1-01 | pendiente |
| 2-02 | constructores M03 en cada dominio; todos los emisores los usan; `QuestionsAsked::new` con `NonEmpty`; `finish_node` absorbe los tres `node_finished` de `gate_exec` (M26) | 2-01 | pendiente |
| 2-03 | ledgers nuevos; `RunState` los sostiene; `NodeHistory` y los pliegues ad hoc borrados; `members_of` en el host del blackboard (M24 I-09); `GateLedger::rounds`, `pending_questions`, `answered_unfinished` (M26) | 2-01 | pendiente |
| 2-04 | `derive` por dominio, `apply` exhaustivo, `Audit` por nombre; `phase.rs` por `RunLedger` | 2-03 | pendiente |
| 3-01 | `PauseReason` (con `Questions` y `AnswersRefused`, M26), `Policy`, `RerouteCause`; `Capability::as_str`; `RunError::Git(#[source])` | 2-02 | pendiente |
| 3-02 | `decide` en seis (con `answered_step`, M26); `GateStep::Waiting`; `RunFinished::closed` único; `current_escalation` sin doble derive | 2-03 | pendiente |
| 3-03 | `SessionPlan` + `open_session`; `attempt.rs` y `prompt_exec` lo llaman; el brief de tarea lleva `notes` (M24 I-03) | 2-02 | pendiente |
| 3-04 | `POLICY` + `require()`; `check(…, &Adapters)`; twin test | 1-01, 3-03 | pendiente |
| 3-05 | Shell: `tokio::fs` ×15+, `Clock` en worktree, `SecretSource`, spans, `get()`, degradaciones como `engine_finding`; `build_manifest` async y la pareja sincrónica de `git.rs` por `spawn_governed`; `cancel` compara el arranque del pid con `started_at` (M24 I-06) | 2-02 | pendiente |
| 3-06 | `ToolTarget`; pase de redacción; `mcp.json` limpiado; bearer constante | 3-05 | pendiente |
| 3-07 | parsers tagged con `Unknown`; `AgentError` con causa; codex falla en settings; claude `read_only` con `Write`/`Edit` solo si hay archivos declarados (`cerco.md` §6); cada adapter lee `adapter_settings` (M24 I-10) | 1-01 | pendiente |
| 3-08 | el cerco (`cerco.md`): `core::fence`, `FenceLevel`, `Coverage`, `FenceHook`, `write_refused`, `yunta fence`, codec claude-code, sandbox codex, mock por el juez, `fence_breach`, docs y glosario | 3-03, 3-04, 3-06, 3-07, 4-01 | pendiente |
| 4-01 | `ScopeGlob`, `SchemaRange`, `WorkflowName`, `SkillName`, `InputName`, `McpServerName`, `CommitSha`, `DateTime`; `pack add` rechaza un rango que excluye la versión (M24 I-04) | 2-01 | pendiente |
| 4-02 | `ReservedIdentity`, `TemplateVar`, `ArtifactKind::Answers` (con `declarable`, `AnswersFile::against`, el montaje `kind: answers` y sus dos reglas de `check`, M26), `RecordedOrigin`, `Location`, `QuestionId`, `DiagnosticCode`, `StagedHash` | 4-01 | pendiente |
| 4-03 | `workflow::read`; `Document` para `FindingEntry`/`Withdrawal`; `text::counted`; `Answerer`; `RunTool`; `run_dir::*`; `steps.rs:256` por canonical | 4-01 | pendiente |
| 4-04 | `PersistedDoc<T>` en manifest, lock, engine.json, lock de aislamiento, receipt | 4-01 | pendiente |
| 5-01 | `Context::open_run`; `collect_history` único | 4-04 | pendiente |
| 5-02 | `RunWord`; `Outcome` de `RunWord`; `RunDocument`; receipt versionado; `width::`; `stats` publica entregas y findings (M24 I-05) | 5-01 | pendiente |
| 5-03 | `CliError` en MCP/promote/test; `ask::Console` en init/new; `Diagnostics` en `Console::open` | 5-01 | pendiente |
| 5-04 | `test`/`promote` por `runnable`+`drive`; `mcp::resolve_gate` único; `graph` por `ctx.storage()`; `Env` una vez; un script sin reclamar falla el caso y `expect: promoted` (M24 I-11, I-13) | 5-01 | pendiente |
| 5-05 | crónica: `view/chronicle.rs`, `surface/chronicle.rs`, `Lines::moment`, `Region::record`, borrados, `Layout::advice`, tests; los hijos bajo su grupo (M24 I-08); `Gates::{Asked, Answered}` y el modificador de `NodeDisplay` (M26) | 2-01, P6 | pendiente |
| 5-06 | `answer_questions` por MCP: la segunda superficie de `engine::answers::record`, como `resolve_gate` (M26, M24 I-01) | 5-04, W-11 | pendiente |
| 6-01 | `Log` builder; 10 `fn event()` borrados; `SourceLog` con clock | 1-03 | pendiente |
| 6-02 | un `Bench` con las cinco capacidades; 46 `execute_run` y 8 sombra migrados; `common/mod.rs` a literales | 6-01 | pendiente |
| 6-03 | `hermetic()`; `Checkout::without_yunta_home`; `SeqIdSource` por bench; `sleep`→`wait_until_async`; los armados a mano por `Checkout` (M24 I-12) | 6-01 | pendiente |
| 6-04 | cuatro propiedades sobre generador completo; `an_ask_answered_after_any_crash_point_derives_one_finished_node` (M26) | 2-01 | pendiente |
| 6-05 | ratchet: 11 contadores nuevos sobre `src`+`tests`; `[workspace.lints]`; CI `workflow_call`, macOS, glob de packs, timeouts, `--release`; CONTRIBUTING | — | pendiente |
| 7-01 | `docs_sync` recorre `docs/design/` y ata los conjuntos cerrados (§2 M23); `the_reference_config_parses_and_its_workflows_check` | 2-01, 3-04 | pendiente |
| 7-02 | ADR por archivo, índice generado, recíprocos | 0-01 | pendiente |
| 7-03 | correcciones de §9 | 7-01 | pendiente |
| 7-04 | glosario; deuda: `yunta replay/diff` (rfc-0003 §2) y la verificación en vivo (status.md) entran como A-16/A-17 con ids estables; `spec-tasks.md` | 7-03 | pendiente |
| 7-05 | baseline eager en `create_run` (M24, D167) con su test | 3-05 | pendiente |
| 7-06 | orden de criterios aprendido del log desde `TaskLedger` (M24, D167) con su test | 2-03 | pendiente |
| 7-07 | el inventario de M24: `manual_review` y `justification` se retiran con D174 (I-02); lo que se construye cierra en el ítem de su mecanismo | 7-04 | pendiente |

Ya cerrado en esta rama, antes del plan: merge de `main` con la costura del
observer en `RunLog` (`6fe9ccc`), `Evidence` como hechos etiquetados
(`365aa41`), ofertas con tradeoff por constructor (`reserved::offers`),
`hand_over` del lock en `--detach`, la vista viva como default (D162),
correcciones de docs (`8582c01`, `80abe93`).

---

## 11. Levantamientos

Un agente que se detiene por la regla 2 de §0 escribe acá, con fecha, ítem,
evidencia (archivo:línea), alternativas y recomendación, y espera. El humano
responde en el mismo lugar. Lo decidido se escribe donde rige —la fila de §4 o
§10, el mecanismo de `mecanismos.md`, la regla de §0, el ADR— y la entrada
queda reducida a una fila de esta tabla: qué se levantó, qué se decidió y dónde
vive ahora. Un levantamiento no es un plan aparte: abierto, bloquea su ítem;
resuelto, ya está en el plan. La evidencia y las alternativas completas de cada
uno están en el commit que lo escribió.

| id | ítem | qué se levantó | decisión | dónde vive ahora |
|---|---|---|---|---|
| L-01 | W-01 | `open_run_tools` decide y además abre el listener; llamarla desde `prepare_loop` abría uno por nodo `loop` y un bind fallido dejaba sin tools a todos los intentos | partir la decisión del bind: `run_tools_allowed(ctx, node, adapter, id) -> Result<bool, RunToolsSetupError>` en `runner_resolve.rs`, consumida por `open_run_tools` y `prepare_loop` | filas W-01 (§4, §10); M08 |
| L-02 | §0.9 | un commit no puede llevar su propio hash | el ítem va en su commit y el tablero lo sigue en otro del mismo PR, con el hash verdadero | §0.9 |
| L-03 | W-05 | gobernar `output_blocking`/`success_blocking` vuelve `build_manifest` async: 102 llamadas en 33 archivos, un alcance mayor que "subconjunto estricto" | W-05 gobierna las tres funciones async que un run llama; la pareja sincrónica y `build_manifest` async son de 3-05 | filas W-05 y 3-05 (§4, §10); M10 |
| L-04 | W-10 | el recorrido recursivo de `docs_sync` arrastra la composición `release-cycle` de `referencia-schema.md`, cuyos `use:` no existen en ningún documento | W-10 corrige sólo los números de CO-14; el recorrido recursivo entra en 7-01 con `the_reference_config_parses_and_its_workflows_check`, que sostiene esos bloques | filas W-10 y 7-01 (§4, §10); M23 |
| L-05 | W-03 | `sha256_hex(input)[..12]` trunca en el productor y M11 fija `digest: ContentHash` entero | el log guarda el hash entero y doce dígitos son cómo se lee: W-03 escribe `ContentHash::abbreviated()` como paso intermedio y M11 lo reemplaza | fila W-03 (§4, §10); M11 |
| L-06 | W-04 | dos normalizaciones de dedup, y la fila no decía cuál sobrevive | la que colapsa espacios y mayúsculas, en `dedup_findings`, para las dos lecturas; `provenance.yaml` pasa a contar desde el mismo pliegue | fila W-04 (§4, §10); M04 |
| L-07 | §4 | el nodo con preguntas contestadas cierra debiendo `brief.md`: la derivación borra el `node_failed` que nombra el faltante, la ronda de respuestas cierra sin `close_node`, y el prompt de `grill` supone un turno después de las respuestas que D86 no da | un nodo que pregunta, pregunta: M26, con W-11 como el subconjunto que hace correr el pack de referencia hoy, y D173 que revisa D86 | M26 (`mecanismos.md#m26`); filas W-11 y las de M26 (§4, §10); P10 (§6) |
| L-08 | §0.15 | D26 llama "append-only" al blackboard que W-04 dejó plegado; ningún ADR revisa esa palabra | como propiedad del canal sigue siendo verdad y un ADR no se reescribe: D26 queda, el Contrato dice el pliegue en presente | Contrato §5.9 y §6.4 |
| L-09 | 0-02 | `rfc-0003.md` carga el mismo tag `javascript` sobre un bloque de texto que el ítem nombra en los otros tres, y el pase de corpus se borra en el commit que lo corre | el ítem des-corrompe los cuatro documentos: el pase corre una vez, y lo que no arregle queda sin herramienta que lo arregle | fila 0-02 (§10); M23 |
| L-10 | 1-01 | el engine importa `Forge` y sus tipos de `yunta_adapters`, así que "quita `yunta-adapters`" los mueve al puerto; `ForgeError::{Transport,Response}` llevan `reqwest::Error` (`adapters/src/forge/mod.rs:35,60`), y moverlos tal cual mete un cliente HTTP en `yunta-core` | el puerto lleva `Forge` con las dos causas como `Box<dyn Error + Send + Sync>`: nadie matchea el tipo concreto (`engine/src/run/mod.rs:164` sólo la encadena) y la causa se conserva; `GitHubForge` la envuelve al construirla | fila 1-01 (§10); M01 |
| L-11 | 1-03 | "el test de secretos duplicado se vuelve uno parametrizado" supone que es un test del adapter; las dos copias son idénticas byte a byte y lo que afirman es el `Debug` de `SessionRequest`, que desde 1-01 es un tipo de core (`adapters/tests/{claude_code.rs:524,codex.rs:645}`) | un solo test en `core/tests/port.rs`, donde vive el tipo; parametrizar dos entradas idénticas no prueba nada de ningún adapter | fila 1-03 (§10); M01 |

---

## 12. Índice: cada defecto, su mecanismo

Ids de las auditorías: EV eventos, EN engine, AR artifacts, CLI, AD adapters,
CO core, TE tests, DO docs.

| id | defecto | mecanismo | fase |
|---|---|---|---|
| EV-D1 | kind declarado en 9 sitios, 3 defendidos | M02 | 2 |
| EV-D2 | `KINDS` sin vínculo de compilación con la enum | M02 | 2 |
| EV-D3 | `replay::apply` `_ => Ok(())` silencioso | M05 | 2 |
| EV-D4 | 30+ brazos comodín en 13 módulos | M04 M05 | 2 |
| EV-D5 | `phase.rs` responde `Created` a lo desconocido | M04 | 2 |
| EV-D6 | `GateWaitingPayload` sin constructor, 7 literales | M03 | 2 |
| EV-D7 | escalación con menú vacío representable | M03 | 2 |
| EV-D8 | `run_paused.reason` con 10 strings | M03 M06 | 2 |
| EV-D9 | 4 bypass de `engine_finding` | M03 | 2 |
| EV-D10 | `capability_degraded` sin constructor, prosa ×8 | M03 M09 | 2–3 |
| EV-D11 | blackboard repliega findings | M04 · W-04 | W |
| EV-D12 | `last_external_ref` ×2 idéntica | M04 | 2 |
| EV-D13 | tasks plegadas en 5 lugares | M04 | 2 |
| EV-D14 | ciclo de nodo en 19 módulos; dos "sesión" | M04 | 2 |
| EV-D15 | `artifact_written` escribible sin emisor | M03 | 2 |
| EV-D16 | spec-events numera artifacts bajo findings | M23 | 7 |
| EV-D17 | `schema_version` constante | M02 | 2 |
| EV-D18 | tres "36" a mano | M02 M23 | 2 |
| EV-D19 | `payloads.rs` 989 líneas | M02 | 2 |
| EV-D20 | el log registra la respuesta y no la pregunta: `questions_answered` sin par | M26 · W-11 | W |
| EN-D1 | git sin process group/registro/cancel | M10 · W-05 | W |
| EN-D2 | `parallel_exec` ignora `on_interrupt` | M07 · W-06 | W |
| EN-D3 | loop sin gate `TypedArtifactNeedsRunTools` | M08 · W-01 | W |
| EN-D4 | dos dedup de findings divergentes | M04 · W-04 | W |
| EN-D5 | `derive` O(e+F²) | M04 | 2 |
| EN-D6 | ≥2 lecturas, ≥3 replays por iteración | M04 M07 | 2–3 |
| EN-D7 | `progress.md` con replay + fs síncrono | M04 M10 | 3 |
| EN-D8 | índice de HashMap ×2 | M10 | 3 |
| EN-D9 | `encode_ref` traga error | M10 | 3 |
| EN-D10 | registro de procesos a tracing | M10 | 3 |
| EN-D11 | registro corrupto = ausente | M14 | 4 |
| EN-D12 | kill fallido descartado | M10 | 3 |
| EN-D13 | `git::success.unwrap_or(false)` | M10 | 3 |
| EN-D14 | `StillWaiting` ambiguo sobre `run_paused` | M07 | 3 |
| EN-D15 | `node_finished` ×4, `node_started` ×3 | M03 | 2 |
| EN-D16 | gate/questions sin span | M10 | 3 |
| EN-D17 | 15 `std::fs` en async | M10 | 3 |
| EN-D18 | 3 `SystemClock` en el engine | M10 | 3 |
| EN-D19 | `std::env` en el engine ×2 | M10 | 3 |
| EN-D20 | 5 archivos >500, 56 fns >50 | M07 M02 | 2–3 |
| EN-D21 | `verification_effectiveness` 5 pases | M04 | 3 |
| EN-D22 | `current_escalation` deriva ×2 | M07 | 3 |
| EN-D23 | `RunArtifacts::of` ×4 | M04 | 2 |
| EN-D24 | `clippy.toml` apunta a lints inexistentes; deny ×5 | M22 | 6 |
| EN-D25 | literales `distilled`/`manifest.yaml` | M13 | 4 |
| EN-D26 | `GitError` stringificado | M06 | 3 |
| EN-D27 | `replay` deriva `Waiting` de cualquier `node_failed` tras un artifact `questions` | M26 · W-11 | W |
| EN-D28 | la ronda de preguntas cierra fuera de `close_node`, con `node_started` y `node_finished` propios | M26 · W-11 | W |
| EN-D29 | un hijo de `parallel` que pregunta nunca es preguntado: `next_step` recorre sólo `workflow.nodes` | M26 · W-11 | W |
| AR-D1 | blackboard repliega (confirmación) | M04 · W-04 | W |
| AR-D2 | loop en modelo default, sin agente | M08 · W-01 | W |
| AR-D3 | `SessionRequest` ×2 campo a campo | M08 | 3 |
| AR-D4 | gate de artifact tipado solo en prompt | M08 · W-01 | W |
| AR-D5 | `answered_by_the_log` re-derivado | M13 | 4 |
| AR-D6 | nombres de tools como literales | M13 | 4 |
| AR-D7 | `manifest.yaml` ×15, paths fuera de `run_dir` | M13 | 4 |
| AR-D8 | nombre de artifact escapa el run dir | M12 · W-02 | W |
| AR-D9 | `ANSWERS_SUFFIX` no reservado | M12 | 4 |
| AR-D10 | findings heredados fuera de `canonical` | M13 | 4 |
| AR-D11 | `Legacy` representable en fresco | M12 | 4 |
| AR-D12 | Contrato §6.4 `{name, document}` | M23 | 7 |
| AR-D13 | Contrato §5.2 brief con ruta | M23 | 7 |
| AR-D14 | spec-ledger regla 1 en dos capas | M23 | 7 |
| AR-D15 | `Vec<String>` por `QuestionId` | M12 | 4 |
| AR-D16 | `location: String` | M12 | 4 |
| AR-D17 | `content_hash` con dos significados | M12 | 4 |
| AR-D18 | `{{runner.role}}` | M12 | 4 |
| AR-D19 | un nodo que no preguntó nada no deja respuestas; el nodo siguiente falla al montarlas | M26 · W-11 | W |
| CLI-D1 | sin puerta `open_run` | M15 | 5 |
| CLI-D2 | `collect_history` saltea `Project::run_dir` | M15 | 5 |
| CLI-D3 | `collect_history` ×2 | M15 | 5 |
| CLI-D4 | run parado con 4 palabras | M16 | 5 |
| CLI-D5 | `RunPhase`→palabras ×5 | M16 | 5 |
| CLI-D6 | `{:?}` al usuario ×9 | M06 | 5 |
| CLI-D7 | 25 `(s)`, 3 pluralizadores | M13 | 4 |
| CLI-D8 | dos derivaciones para las superficies | M19 | 5 |
| CLI-D9 | `Scrollback::gone` | M19 | 5 |
| CLI-D10 | `mcp::tool_resolve_gate` duplicado | M18 | 5 |
| CLI-D11 | MCP re-bordea 17 errores | M17 | 5 |
| CLI-D12 | `promote` aplana errores, `SystemClock` | M17 M18 | 5 |
| CLI-D13 | `test` cuarto camino, sin `check` | M18 M13 | 4–5 |
| CLI-D14 | `init`/`new` fuera de `ask` | M17 | 5 |
| CLI-D15 | cwd/HOME leídos ×2 | M10 | 5 |
| CLI-D16 | dos exit codes; findings bloqueantes = 0 | M16 · P5 | 5 |
| CLI-D17 | receipt JSON sin versión | M14 M16 | 4–5 |
| CLI-D18 | `--quiet` imprime más que el id | M23 | 5 |
| CLI-D19 | README `list` modes, `graph` firma | M23 | 7 |
| CLI-D20 | scans de terminal a mano ×3 | M04 M15 | 5 |
| CLI-D21 | "only claude-code and codex" ×2 | M01 | 1 |
| CLI-D22 | `stats.rs` 25 `println!` | M16 | 5 |
| CLI-D23 | tamaños | M15 M17 M18 | 5 |
| CLI-D24 | `graph` abre storage solo | M15 | 5 |
| CLI-D25 | `MARKER`/`GAP`, `{:<12}` | M16 | 5 |
| CLI-D26 | warnings de `Console::open` sin `Diagnostics` | M17 | 5 |
| AD-D1 | mock pierde handle del player | M10 · W-07 | W |
| AD-D2 | `edit_hooks` nunca consultada | M09 M25 | 3 |
| AD-D3 | `usage_reporting` nunca consultada | M09 | 3 |
| AD-D4 | `permission_profiles` nunca; `check` ciego | M09 | 3 |
| AD-D5 | `target_digest` crudo | M11 · W-03 | W |
| AD-D6 | sin pase de redacción | M11 | 3 |
| AD-D7 | claude `ReadOnly` con `Write` | M09 M25 | 3 |
| AD-D8 | fixture renderer en el CLI | M01 | 1 |
| AD-D9 | gates de skills/run_tools duplicados | M08 M09 | 3 |
| AD-D10 | `policy_applied` prosa ×7 | M03 M09 | 2–3 |
| AD-D11 | codex traga error de settings | M10 | 3 |
| AD-D12 | `std::fs` en spawn async | M10 | 3 |
| AD-D13 | parsers stringly; conteos ausentes = 0 | M06 | 3 |
| AD-D14 | `AgentError` sin causa | M06 | 3 |
| AD-D15 | `FixtureCapabilities` sin guard | M09 | 3 |
| AD-D16 | `init.rs` re-deletrea ids | M01 | 1 |
| AD-D17 | `refuse_unrunnable` prosa | M01 | 1 |
| AD-D18 | `RunTools` vs `run_tools` | M06 | 5 |
| AD-D19 | codex sin `RunToolsMounted` | M09 | 3 |
| AD-D20 | codex `ReadOnly`+`artifact_dir` sin evento | M25 | 3 |
| AD-D21 | readers sin span | M10 | 3 |
| AD-D22 | spec-events 6 caps, `model` obligatorio | M23 | 7 |
| AD-D23 | adapters.md "primer sano" | M23 | 7 |
| AD-D24 | spec-adapter §6 falso | M23 M25 · P9 | 3–7 |
| CO-1 | globs sin compilar | M12 | 4 |
| CO-2 | `yunta_schema` String | M12 | 4 |
| CO-3 | commits String | M12 | 4 |
| CO-4 | `started_at` String | M12 | 4 |
| CO-5 | `manifest.schema_version` no leído | M14 | 4 |
| CO-6 | 4 persistidos sin versión | M14 | 4 |
| CO-7 | `Manifest` embebe tipos estrictos | M14 | 4 |
| CO-8 | `FindingEntry` parseada a mano | M13 | 4 |
| CO-9 | `Workflow` sin reglas ×11 | M13 | 4 |
| CO-10 | prosa en core (`Vec<String>`) | M06 | 4 |
| CO-11 | pluralización ×3 | M13 | 4 |
| CO-12 | códigos half-typed | M12 | 4 |
| CO-13 | rustdoc de `Task.id` contradice el tipo | M23 | 4 |
| CO-14 | referencia-schema no parsea | M23 · W-10 | W |
| CO-15 | 7 schemas documentados, 8 reales | M23 | 7 |
| CO-16 | D140/D144 sin nota de D156 | M23 | 7 |
| CO-17 | `pack.yaml` ilegible tragado | M14 | 4 |
| CO-18 | `ArtifactRefId` untagged | M12 | 4 |
| CO-19 | `ScopeExpansionPermissions` sin export; `Answer` sin deny | M12 | 4 |
| CO-20 | HOME/TERM ×2 | M10 | 5 |
| CO-21 | `interactive: true` se acepta y ninguna superficie lo lee | M26 · W-11 (se retira) | W |
| TE-D1 | `copied_test_helpers 0` falso | M22 | 6 |
| TE-D2 | 46 `execute_run` a mano | M20 | 6 |
| TE-D3 | `stored` sin usuarios; 10 builders | M20 | 6 |
| TE-D4 | core/adapters sin testkit | M01 M20 | 1 |
| TE-D5 | `common/mod.rs` segundo testkit | M20 | 6 |
| TE-D6 | `sleep(100ms)` | M22 | 6 |
| TE-D7 | `run_yunta` hereda org config, TERM, USER | M20 · W-08 | W |
| TE-D8 | `check_keys_cmd` lee config real | M20 | 6 |
| TE-D9 | `docs_sync` sin HOME/git pin | M20 | 6 |
| TE-D10 | `remove_var` global | M22 | 6 |
| TE-D11 | `static IDS` ×12 | M20 | 6 |
| TE-D12 | propiedad de resume tautológica | M21 · W-09 | W–6 |
| TE-D13 | sin idempotencia | M21 | 6 |
| TE-D14 | sin propiedad de cadena | M21 | 6 |
| TE-D15 | generador 8/36 | M21 | 6 |
| TE-D16 | release nunca testeado | M22 | 6 |
| TE-D17 | sin macOS | M22 | 6 |
| TE-D18 | release gate débil | M22 | 6 |
| TE-D19 | nombres — sin defecto | se conserva | — |
| TE-D20 | 19 tests sin `//!` | M23 | 6 |
| TE-D21 | comentario de dev-dep falso | M23 | 6 |
| TE-D22 | `nix` dev-dep obsoleto | M23 | 6 |
| TE-D23 | `yunta test` "determinista" con SystemClock | M18 | 5 |
| TE-D24 | 34 tests >500, 145 fns >50 | M22 | 6 |
| TE-D25 | 60 ms sin decisión | M24 · P7 | 0 |
| DO-D1 | baseline lazy vs D18/§7.2 | M24 · P3 | 0–3 |
| DO-D2 | claude-code `edit_hooks` vs spec | M25 · P9 | 3 |
| DO-D3 | codex `resume_session: true` vs spec | M23 | 7 |
| DO-D4 | preguntas por PR | M24 · P3 | 0 |
| DO-D5 | orden de criterios por invocación vs D62 | M24 · P3 | 0–3 |
| DO-D6 | memo key con `env` inexistente | M23 | 7 |
| DO-D7 | fuentes por executor vs D19 | M24 · P3 | 0 |
| DO-D8 | `task-ledger` en YAML de autor | M24 · P4 | 0–4 |
| DO-D9 | referencia-schema no parsea | M23 · W-10 | W |
| DO-D10–11 | 5 tools MCP en Contrato, `--help`, rustdoc | M23 | 7 |
| DO-D12 | §6.4 `{name, document}` | M23 | 7 |
| DO-D13 | `session.rs:3-10` rustdoc falso ×3 | M23 | 1 |
| DO-D14 | "Only pause is built" | M23 | 7 |
| DO-D15 | "future --detach" | M23 | 7 |
| DO-D16 | README `graph` | M23 | 7 |
| DO-D17 | `waiting` incompleto en concepts | M23 | 7 |
| DO-D18 | marcadores `[inferido]` obsoletos | M23 | 7 |
| DO-D19–21 | spec-adapter/spec-events campos desfasados | M23 | 7 |
| DO-D22 | 7 reglas documentadas, 9 publicadas | M23 | 7 |
| DO-D23 | `done` vs `finished` | M23 | 7 |
| DO-D24 | §5.3 campos inexistentes | M23 | 7 |
| DO-D25 | markdown corrupto | M23 | 0 |
| DO-D26–27 | D132 "task ledger", D139 "siete" | M23 | 7 |
| DO-D28–30 | O5 duplicado, M14, deuda ⑪ | M23 | 7 |
| DO-D31–32 | D152 sin nota; D03 sin reviser | M23 | 7 |
| DO-D33–34 | agrupación degenerada; `adr/` vacío | M23 | 0 |
| DO-D35–41 | vocabulario prohibido (12 + 6) | M23 | 0–7 |
| DO-D42 | 21 marcadores de tiempo/plan | M23 | 0–7 |
| DO-D43 | términos sin glosario | M23 | 7 |
| DO-D44 | `docs/design` invisible a `docs_sync` | M23 · W-10 | W |
| DO-D45 | pack, fixture canónico y `referencia-schema.md` declaran `[questions, brief.md]` con un prompt que espera un turno que D86 no da | M26 · W-11 | W |
