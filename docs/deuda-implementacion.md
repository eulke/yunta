# Deuda de implementación — registro y plan de resolución

**Estado:** vivo — se actualiza al abrir o cerrar cada ítem.
**Alcance:** deuda acumulada durante la implementación de M-0–M9 — gaps
dentro de tareas ya marcadas como hechas, decisiones "documentado, no
resuelto", y gatillos que ya se cumplieron. **No** cubre:

- La "Deuda consciente" de diseño (A-01–A-10, página de Notion): esa es
  deuda de *producto*, decidida antes de codear, y cada ítem espera su
  ADR. Este documento es deuda de *implementación*: cosas que el plan sí
  pide (o que una norma del Contrato promete) y que quedaron a medio
  camino por recortes de alcance legítimos.
- Tareas del plan con dueño (T9.3, T9.4, T2.5, M8, M10–M12): tienen su
  slot y su criterio de aceptación en el plan; se listan al final solo
  cuando un ítem de acá depende de ellas o las alimenta.

## Cómo se consume este documento

1. **Un ítem = una tarea.** Cada DI-NN se implementa como tarea propia,
   test-first en rojo, con los ✓ de su sección como criterio de
   aceptación. Jamás "de paso" dentro de otra tarea (CLAUDE.md: "scope
   chico y declarado").
2. **La solución propuesta acá es el diseño de referencia.** Si al
   implementar aparece algo mejor, se corrige **este documento primero**
   (con racional, como un mini-ADR) y recién después el código — nunca se
   implementa distinto de lo escrito dejándolo como sorpresa.
3. **El orden de niveles es el orden de ataque.** Dentro de un nivel, el
   orden listado ya considera dependencias entre ítems.
4. Al cerrar un ítem: marcarlo `[x]` acá, actualizar la entrada
   correspondiente de `docs/m0-status.md`, y borrar el comentario de
   deuda del código que lo nombraba.

## Índice por prioridad

| ID | Título | Origen | Nivel | Tamaño |
|---|---|---|---|---|
| DI-01 | Escalación de scope expansion → gate real | T5.11 | 1 | M |
| DI-02 | `kind: questions` → superficie interactiva | T5.14 | 1 | M |
| DI-03 | `NodeState::Waiting` para gates en replay/status | T7.7 | 1 | M |
| DI-04 | Gate interno genérico (`message`/`options`/`on`) | T7.2/T9.1 | 1 | M |
| DI-05 | `limits:` + presupuestos declarables | T1.2/T3.3/T7.5 | 1 | L |
| DI-06 | Señales de modo en rendimiento de verificación | T7.10 | 1 | S |
| DI-07 | Paths congelados en el manifest (T2.4) | M-0 | 2 | S |
| DI-08 | Canal cross-process: `cancel` real, Ctrl-C, lock huérfano | T7.1/T4.2 | 2 | L |
| DI-09 | Eventos de sesión (`agent_session_opened`/`agent_message`) | M-0 | 2 | M |
| DI-10 | Findings del engine sobreviven la promoción | T9.2 | 2 | S |
| DI-11 | Cancelación de `loop`/`check`/`executor` bajo `join: any` | T4.6 | 2 | M |
| DI-12 | Colisión de escritura en el fan-out implícito del DAG | T4.1 | 2 | M |
| DI-13 | Schema completo de T1.1: round-trip de los YAML de referencia | T1.1 | 2 | L |
| DI-14 | Retención a nivel de base de datos | T7.1 | 2 | S |
| DI-15 | Orden aprendido de criterios (duración histórica) | T5.9 | 3 | M |
| DI-16 | Race de `max_per_run` bajo concurrencia | T5.10/T5.11 | 3 | S |
| DI-17 | `context:` a nivel loop/tarea | T6.1 | 3 | M |
| DI-18 | Reglas menores de `check` (`max_parallel_nodes`, push a base) | T1.3 | 3 | S |
| DI-19 | Higiene: helpers duplicados, params de `create_run` | varios | 3 | S |
| DI-20 | Techo en capas para `scope_expansion` | T5.11 | 3 | S |
| DI-21 | `events.jsonl` en el camino `Broken` | T5.8 | 3 | S |
| DI-22 | Smoke tests en vivo pendientes (codex, GitHubForge) | T7.4/T7.7 | 3 | S |
| DI-23 | `on_interrupt: resume_session` | T4.5/D99 | 3 | M |
| DI-24 | `on_finish.distill` — mecanismo completo | T5.8 | 2 | M |

---

## Nivel 1 — gatillo cumplido: hoy incumplen una promesa normativa

Estos seis ítems quedaron en pausa esperando una pieza que **ya existe**.
Mantenerlos abiertos ya no es recorte de alcance: es una promesa del
Contrato que el binario actual no cumple pudiendo cumplirla.

### DI-01 — Escalación de scope expansion → gate real `[x]`

- **Origen:** T5.11 (§6.2). Entrada en `m0-status.md`: "`ask` degrada
  siempre a pausa, nunca a consulta real: no existe `kind: gate`/T7.2 en
  este codebase todavía". Código: `scope_expansion.rs::Decision::Escalate`,
  `loop_exec.rs` (pausa ante `needs_human_decision`).
- **Gatillo, ya cumplido:** T7.2 construyó `HumanInteraction` +
  `ConsoleInteraction`; el objeto de escalación §5.3 ya se arma y
  renderiza para re-rutas agotadas. La razón por la que `ask` degradaba
  desapareció.
- **Síntoma hoy:** `scope_expansion.mode: ask` con TTY disponible pausa
  el run igual que sin TTY — el humano tiene que `yunta resume` y aun así
  nadie le pregunta nada; la solicitud queda en el limbo.
- **Solución propuesta:**
  1. `RunCtx` gana `human_interaction: &'a dyn HumanInteraction` (hoy es
     parámetro suelto de `execute_run`; moverlo al ctx no cambia ningún
     call-site externo). `loop_exec`/`task_cycle` lo alcanzan vía ctx.
  2. En el punto donde hoy se pausa por `needs_human_decision`, el engine
     arma el objeto §5.3 **con evidencia mecánica del log**:
     - `summary`: `"task `{task_id}` requests scope expansion: {reason}"`.
     - `evidence`: paths pedidos, criterio propuesto + exit code de su
       pre-check (ya está en `ScopeExpansionOutcome.precheck_exit`),
       modo efectivo, y estado del cap (`count_this_run`/`max_per_run`).
     - `options`: `grant` (tradeoff: "the task's final diff is evaluated
       against scope + these paths; consumes 1 of max_per_run") y `deny`
       (tradeoff: "denial becomes a finding (D80); the task retries
       within its original scope"). `free_text` siempre (§5.3).
  3. Resolución:
     - `grant` → `scope_expansion_granted { decided_by: Person{id},
       paths }` — **agregar `paths: Vec<String>` al payload** (aditivo,
       lector tolerante D70): un grant que no dice qué concedió obliga a
       correlacionar con el `requested` anterior por orden de log; la
       auditoría debe ser autocontenida. La tarea vuelve a `ready` y su
       próximo intento deriva `effective_scope = task.scope + paths de
       todos los granted de ese task en el log` (I2: el log decide, no
       estado en memoria).
     - `deny` → `scope_expansion_denied { decided_by: Person{id} }` +
       conversión a `finding_posted` (D80) — misma vía que la denegación
       por regla ya usa en `loop_exec.rs`.
     - `None` (sin superficie) → pausa, exactamente como hoy. Sin
       cambio de comportamiento headless.
  4. M8 reutiliza el mismo objeto vía `resolve_gate` sin lógica nueva —
     esa es la razón de armar el payload completo acá y no un prompt
     ad-hoc de consola.
- **✓ Criterios:**
  - `mode: ask` + `ScriptedInteraction` que concede → la tarea re-corre y
    su diff dentro de scope+paths pasa; el log tiene
    `requested → granted{decided_by: Person}` y ningún `run_paused`.
  - La misma interacción que deniega → `denied{Person}` +
    `finding_posted` con el criterio propuesto adjunto; la tarea
    reintenta dentro del scope original.
  - `NoInteraction` → pausa idéntica a la actual (test existente sigue
    verde sin tocar).
  - El objeto `gate_waiting` de esta escalación lleva evidencia con el
    exit code real del pre-check del criterio propuesto.
- **No hacer:** no inventar un renderer de consola aparte — es el mismo
  `HumanInteraction.resolve()`; no persistir la decisión fuera del log.

### DI-02 — `kind: questions` → superficie interactiva `[x]`

- **Origen:** T5.14 (§4.1/D86). Entrada: "sigue sin superficie
  interactiva — deuda ya nombrada". Gatillo cumplido: T7.1 (TTY) y T7.2
  (trait) existen.
- **Síntoma hoy:** un nodo que produce `kind: questions` con un humano
  sentado frente a la terminal igual deja el run `waiting`; el humano
  tiene que editar `answers.yaml` a mano.
- **Solución propuesta:**
  1. Extender el trait con un método con default (compatibilidad hacia
     atrás sin tocar `NoInteraction` ni los mocks existentes):
     ```rust
     #[async_trait]
     pub trait HumanInteraction: Send + Sync {
         async fn resolve(&self, escalation: &GateWaitingPayload)
             -> Option<GateResolvedPayload>;
         /// §4.1/D86 — pregunta por pregunta. None = sin superficie.
         async fn ask(&self, questions: &QuestionsFile)
             -> Option<Vec<Answer>> { let _ = questions; None }
     }
     pub struct Answer { pub id: String, pub value: String }
     ```
  2. `ConsoleInteraction::ask`: TTY pregunta por pregunta respetando
     `answer_type` (`text` libre; `choice` valida contra `values`, con
     reintento ante valor inválido; `boolean` acepta `y/n`), `required`
     (una pregunta no requerida acepta línea vacía = sin respuesta).
     EOF/sin TTY → `None` (mismo convenio que `resolve`).
  3. En el punto donde T5.14 hoy deja el run `waiting` (sesión del nodo
     ya cerrada, `questions.yaml` parseado): llamar `ask()`. `Some` →
     materializar el artifact de respuestas + `questions_answered
     { hash, channel: Console, respondent }` y seguir; `None` → waiting,
     como hoy. Resume re-renderiza desde el artifact (comportamiento
     T5.14 existente, sin estado conversacional — se preserva).
  4. `interactive: true` del nodo (cuando DI-13 lo agregue al schema) es
     dato de **presentación**: con `false`/ausente, la superficie puede
     mostrar el cuestionario como objeto §5.3 de una sola pieza en vez de
     pregunta por pregunta. No cambia la semántica de las respuestas.
- **✓ Criterios:**
  - Con `ScriptedInteraction` que responde: el run continúa en la misma
    invocación, `questions_answered` en el log con canal y respondiente,
    el nodo siguiente monta las respuestas.
  - `choice` con valor fuera de `values` re-pregunta (test de consola con
    stdin scriptado o unit test del validador extraído puro).
  - `required: false` sin respuesta → artifact sin esa entrada, sin error.
  - Headless: idéntico a hoy (waiting), tests existentes sin tocar.
- **No hacer:** no meter las preguntas por `resolve()` disfrazadas de
  gate — son dos formas distintas del Contrato (§4.1 vs §5.3) y
  aplastarlas en un solo método genera el vicio de payloads ambiguos.

### DI-03 — `NodeState::Waiting` para gates en replay/status `[x]`

- **Origen:** T7.7 + "Pendiente explícito #5". El gatillo original decía
  "cuando un gate pueda resolverse desde otra invocación" — T7.7 creó
  exactamente eso (el gate externo queda publicado y sin resolver entre
  invocaciones).
- **Síntoma hoy:** un run pausado esperando un PR muestra el nodo gate
  como **inexistente** en `status` (nunca emitió `node_started`), cuando
  §3.2 define `waiting` como estado derivable y §8.5 exige "waiting
  distinguido" en el progreso. El scheduler compensa escaneando eventos
  crudos (`was_published`/`last_external_ref`) — lógica de derivación
  fuera de `derive()`, el vicio exacto que I2 quiere evitar.
- **Solución propuesta:**
  1. `replay::NodeState` gana variante `Waiting { external_ref:
     Option<String> }`. Regla de derivación: un nodo está `Waiting` si
     su último evento de gate es `gate_waiting` **sin** `gate_resolved`
     posterior, y no hay `node_started`/terminal posterior a ese
     `gate_waiting`. (El par síncrono de T7.2 — waiting+resolved juntos —
     nunca produce `Waiting`, por construcción de la regla.)
  2. `schedule.rs` secciones 0 y 3 pasan a preguntar
     `state.nodes.get(id) == Waiting{external_ref}` en vez de escanear
     eventos — `was_published`/`last_external_ref` se borran (git los
     recuerda). Comportamiento idéntico; property test de equivalencia
     sobre los logs de los tests de T7.7 existentes.
  3. `status`/`progress_summary`/`progress.md` muestran `waiting` como
     categoría propia (§8.5): `"approve: waiting — PR #12"` usando
     `external_ref`.
  4. **Alcance completo de `waiting` (§3.2):** el texto normativo cubre
     gates **y** "nodos cuyas preguntas pendientes esperan respuesta" —
     un nodo `questions` cuya sesión cerró con `questions.yaml` escrito
     y sin `questions_answered` posterior también deriva `Waiting`
     (regla análoga: último evento relevante sin su resolución).
  5. **`skipped` para nodos excluidos por modo (§3.2):** `derive()` es
     pura sobre eventos y no conoce el workflow, así que la exclusión
     por modo no puede (ni debe) entrar ahí — pero `status`/`progress`,
     que SÍ tienen el manifest y el modo del `run_created`, renderizan
     los nodos excluidos como `skipped (mode: quick)` en vez de
     omitirlos: el denominador visible cambia con el modo y D45 exige
     que ese cambio sea atribuible, no silencioso.
- **✓ Criterios:**
  - Sobre el log real del test T7.7 "publica y pausa": `derive()` da
    `Waiting` con el `external_ref` del PR; tras aprobar y re-despertar,
    `Finished` (tests T7.7 existentes siguen verdes).
  - `yunta status` de un run pausado en gate imprime el nodo como
    `waiting`, distinguido de `running` y de "nunca corrió".
  - Property test existente de determinismo de replay cubre la variante.
- **No hacer:** no emitir eventos nuevos para esto (D45: derivación
  pura, cero eventos).

### DI-04 — Gate interno genérico: `message`/`options`/`on` `[x]`

- **Origen:** T7.2 construyó el mecanismo pero solo lo conectó a re-rutas
  agotadas; T7.7 agregó `kind: gate` pero con `external:` obligatorio;
  T9.1 documentó que "ninguna tarea pide `message`/`options`/`on`". Esa
  lectura era correcta tarea por tarea pero el agregado la refuta:
  - Los YAML de "Config y workflows de referencia" **son fixtures de
    parseo** (CLAUDE.md, y el ✓ de T1.1 exige round-trip) y
    `build-feature.yaml` usa `approve-plan`/`ship` como gates internos
    con `message`, `options` y `on: { ajustar: plan }`.
  - T1.3 exige validar "coherencia interna de cada modo (…) cuyo `goto`
    **u opción de gate** apunta a un nodo excluido" — imposible sin que
    las opciones de gate existan en el schema.
- **Solución propuesta:**
  1. Schema:
     ```rust
     NodeKind::Gate {
         assignee: String,
         #[serde(default)] message: Option<String>,
         #[serde(default)] options: Vec<String>,        // ids libres
         #[serde(default)] on: IndexMap<String, NodeId>, // opción → re-ruta
         #[serde(default)] external: Option<ExternalGate>, // pasa a Option
     }
     ```
     `external: Some` = gate por PR (T7.7, sin cambios). `external: None`
     = gate interno: se resuelve por `HumanInteraction` (consola hoy,
     `resolve_gate` MCP en M8).
  2. Semántica del gate interno (scheduler lo intercepta ready, igual que
     el externo, uno a la vez):
     - Arma `GateWaitingPayload`: `summary` = `message` (o un default
       derivado del id), `evidence` = lista de artifacts producidos por
       sus `depends_on` (mecánica, del log), `options` = las declaradas +
       una opción `abort` **agregada por el engine** (mismo convenio que
       la escalación de T7.2 ya usa; §5.3: abortar siempre es una salida
       válida). Tradeoffs derivados: opción mapeada en `on` → "re-routes
       to `{target}` and re-asks when it completes"; no mapeada →
       "resolves this gate and continues".
     - Resolución: opción en `on` → `node_rerouted { from: gate, to:
       target }` con la semántica §11.2 completa (el destino y su
       subgrafo completan → el gate vuelve a `ready` y **re-pregunta**).
       Sin `max_reroutes`: el ciclo lo conduce un humano en cada vuelta,
       no es un ciclo automático que haya que acotar. Opción no mapeada →
       `gate_resolved { chosen_option }` + `node_finished` (outcome = la
       opción). `abort` → `run_paused`, como T7.2.
     - Sin superficie (`None`) → `gate_waiting` en el log + pausa; DI-03
       lo muestra como `waiting`; un `resume` con TTY re-pregunta.
  3. `check`: `on` ⊆ `options`; targets de `on` existen; **extensión de
     la regla de modos de T9.1**: target de `on` excluido del modo que
     incluye al gate = error con las dos salidas (el texto exacto que
     T1.3 pide). Gate interno no exige `forge` configurado
     (`ExternalGateWithoutForge` solo aplica con `external: Some`).
  4. Con esto, la "clasificación por nodo temprano + gate" de §10.1 queda
     completamente expresable como composición: nodo temprano propone
     (artifact), gate interno confirma, y la escalera de promoción (T9.2)
     mueve de modo — sin mutar el modo del run en curso, que D22
     descarta explícitamente.
- **✓ Criterios:**
  - El fragmento `approve-plan` de `build-feature.yaml` parsea round-trip
    tal cual está escrito en la referencia.
  - Interacción scriptada "ajustar" → `plan` re-corre → el gate
    re-pregunta → "aprobar" → el DAG sigue. Todo derivado del log en un
    `resume` posterior (property de resumibilidad).
  - Modo que incluye el gate y excluye el target de `on` falla `check`
    nombrando las dos salidas.
  - Headless: pausa con `gate_waiting` registrado; `status` (con DI-03)
    lo muestra `waiting`.
- **Dependencias:** DI-03 (estado waiting) primero; ambos alimentan M8.
- **No hacer:** no inventar aristas condicionales generales ("si opción X
  entonces rama Y" como feature de DAG) — el único branching es la
  re-ruta declarada en `on`, igual que `on_failure.goto`.

### DI-05 — `limits:` + presupuestos declarables `[x]`

- **Origen:** el grupo `limits:` está en la config de referencia desde el
  día uno y quedó fuera del recorte de T1.2. Hoy son consumidores
  huérfanos: la advertencia presupuesto-vs-p90 (§8.6, deuda de T7.5), el
  `run_paused` por presupuesto de run (§8.3, "Pendiente explícito #8"),
  `Budget` del trait Adapter que `node_exec.rs` construye siempre en
  blanco (T3.3 a medias), el cap de iteraciones de loop, y el umbral de
  contexto inline hardcodeado en 4096 bytes (`context_resolve.rs`).
- **Solución propuesta (por etapas, cada una con valor propio):**
  1. **Schema + merge + manifest** — `LimitsConfig` en `yunta-core`:
     ```rust
     pub struct LimitsConfig {
         pub max_tokens_per_run: Option<u64>,
         pub max_loop_iterations: Option<u32>,
         pub max_concurrent_runs: Option<u32>,
         pub max_workflow_depth: Option<u32>,   // consumidor: T9.3
         pub max_artifact_bytes: Option<u64>,
         pub inline_context_bytes: Option<u64>,
     }
     ```
     Merge por clave con precedencia repo > usuario > org (merge normal
     de T1.2 — el merge invertido es exclusivo de `permissions`, §6.1).
     Congelado en el manifest. **Verificado:** serde_yaml 0.9 (YAML
     1.2) parsea `2_000_000` como *string*, no como entero — la forma
     canónica es `2000000` sin separadores, y el guion bajo falla en
     el parseo con error de tipo (ruidoso, no silencioso: correcto).
  2. **Presupuesto de run (§8.3):** el loop de `execute_run` compara
     `state.total_tokens` contra `max_tokens_per_run` en cada iteración
     del scheduler. Excedido → escalación §5.3 (no pausa muda): summary
     con tokens gastados vs. cap, `evidence` mecánica del log, opciones
     `continue` (tradeoff: "lifts the cap for this invocation; you
     will be asked again if the run pauses and resumes") y `abort`.
     Sin superficie → `run_paused { reason: budget }`.
     **La autorización es por invocación, en memoria — jamás derivada
     del log.** Racional (corrige la versión anterior de este punto,
     que proponía la regla "cap levantado si existe `gate_resolved`
     con `chosen_option: continue`"): identificar *cuál* escalación
     fue la de presupuesto exigiría o bien string-matching sobre
     `summary` (vicio: acopla el replay a texto humano) o bien un
     marcador nuevo en el schema de eventos (crecimiento de schema
     para un solo consumidor); y semánticamente cada invocación nueva
     gasta dinero nuevo — que un humano haya dicho "continue" hace
     tres días no autoriza el gasto de hoy. Un `resume` posterior
     re-escala: la decisión queda auditada en el log
     (`gate_waiting`/`gate_resolved` a nivel de run, `node_id: None`),
     pero solo la invocación que la obtuvo la consume.
  3. **Budget por sesión (T3.3):** `node_exec` construye
     `Budget.max_tokens` como una fracción del cap de run restante —
     política simple y documentada: `min(restante, max_tokens_per_run /
     nodos_no_terminales)`; sin cap → `Budget` ilimitado como hoy.
     Si un humano ya autorizó `continue` en esta invocación, las
     sesiones vuelven a ilimitado (capearlas a `restante = 0`
     contradiría la autorización). **Corrección:** la versión anterior
     decía "desde `defaults.timeout_minutes` (ya existe)" — no existe:
     el recorte de T1.2 lo dejó fuera (documentado en `config.rs`), así
     que `Budget.timeout` queda `None` hasta que ese campo entre por su
     propia tarea.
  4. **Advertencia p90 (§8.6, cierra la deuda de T7.5):** en `yunta run`,
     si hay estimación (≥3 corridas) y `max_tokens_per_run <
     estimation.tokens.p90` → una línea de advertencia antes de crear el
     run (informativa, jamás bloqueante).
  5. **`max_loop_iterations`:** hoy **no existe cap alguno** —
     verificado en `loop_exec.rs`: el loop solo termina por `until`
     satisfecho o por tareas bloqueadas; un ledger cuyo estado oscila
     (p. ej. re-plan que re-invalida) podría iterar sin techo. El cap de
     config es la única red: agotado → la escalación §5.3 que §8.3 ya
     describe (opciones `continue`/`abort`, mismo mecanismo que el
     punto 2). Default sin declarar: el valor de referencia (12).
  6. **`inline_context_bytes`:** reemplaza la constante 4096 de
     `context_resolve.rs`; default si no se declara = el valor de
     referencia (32_000).
  7. **`max_concurrent_runs`:** en `create_run` (CLI), contar runs no
     terminales en storage; excedido → error accionable. Best-effort
     (dos `yunta run` simultáneos pueden colarse — documentar, mismo
     criterio que la race de DI-16: cap blando, no límite de seguridad).
  8. **`max_artifact_bytes`:** guardia en `close_artifacts` — artifact
     que excede → nodo failed con diagnóstico (§4 "guardia contra
     accidentes").
- **✓ Criterios:** cada etapa con el suyo; los centrales:
  - Config con `max_tokens_per_run` bajo + mock que gasta tokens → el
    run escala/pausa con la razón exacta; sin límite → sin cambio.
  - `yunta run` con presupuesto < p90 histórico imprime la advertencia;
    con <3 corridas, nunca.
  - El umbral inline es configurable y el default es el de referencia.
  - Precedencia repo>org verificada por test de merge.
- **No hacer:** no convertir ningún límite en enforcement de OS (D105:
  eso no existe); no inventar campos fuera de los seis de referencia.
- **Nota de cierre:** las etapas 2 y 3 interactúan más de lo que este
  diseño anticipaba: con el Budget por sesión activo (etapa 3), una
  sesión *obediente* nunca puede empujar el total del run por encima del
  cap — su propia cuota `min(restante, cap/nodos_no_terminales)` la
  corta antes. El chequeo a nivel de run (etapa 2) es la red para el
  caso real: un *overshoot* (un solo evento de usage que revienta cuota
  y cap a la vez — exactamente lo que hace un CLI que reporta usage a
  posteriori), tras el cual el scheduler todavía quiere ejecutar más
  (re-ruta, corrección, retry). Los tests lo ejercitan así. Además,
  `continue` autorizado vuelve las sesiones a ilimitado — caparlas a
  `restante = 0` contradiría la autorización. `max_workflow_depth`
  queda en schema sin consumidor hasta T9.3, como este ítem ya
  declaraba.

### DI-06 — Señales de modo en rendimiento de verificación `[x]`

- **Origen:** T7.10 dejó fuera "modo sin uso" y el test estructural
  "nunca sugiere quitar nodos `invariant: true`" porque `modes:` no
  existía. T9.1 lo creó — gatillo cumplido.
- **Solución propuesta:**
  1. `analyze_verification_effectiveness` recibe también el historial de
     modos usados (el `mode` de cada `run_created`, que
     `collect_raw_history` ya trae dentro de los logs crudos — solo hay
     que leerlo).
  2. Señal nueva `UnusedMode { name, runs_observed }`: un modo declarado
     que ningún run del historial usó, con `runs_observed >= MIN_SAMPLES`
     (mismo piso; la evidencia es "hubo suficientes corridas y ninguna lo
     eligió", no "existe hace mucho").
  3. Test estructural del ✓ de T7.10: ningún hallazgo del análisis
     (re-ruta nunca disparada, gate siempre aprobado) se emite sobre un
     nodo `invariant: true` **con redacción de quitar el nodo** — la
     regla concreta: los hallazgos sobre nodos invariantes se filtran de
     `never_triggered_reroutes`/`always_approved_gates` (un nodo de
     verificación que nunca falla está haciendo su trabajo; sugerir
     revisarlo es exactamente el error que §8.7 prohíbe).
  4. Render en `check`/`stats` con la misma función compartida.
- **✓ Criterios:**
  - Workflow con 3 modos, historial de ≥3 runs todos en el primero → los
    otros dos aparecen como sin uso; con 2 runs, nada.
  - Un nodo `invariant: true` con `on_failure` nunca disparada NO aparece
    como hallazgo (test estructural del ✓ original).
  - Los 13 tests existentes de T7.10 sin tocar.
- **No hacer:** no sugerir jamás la eliminación de un modo — la señal es
  "sin uso", la decisión es humana (§8.7: "sugiere, jamás actúa").

---

## Nivel 2 — robustez del núcleo

### DI-07 — Paths congelados en el manifest (T2.4) `[x]`

- **Origen:** M-0, "Pendiente explícito #8": "`resume` busca run.dir bajo
  el `paths.runs` de la config *actual*".
- **Síntoma:** cambiar `paths.runs`/`paths.worktrees` (o `YUNTA_HOME`)
  entre `run` y `resume` pierde el run — violación directa del ✓ de
  T2.4 ("crear un run, cambiar `paths.runs`, `yunta resume` lo encuentra
  donde nació").
- **Solución propuesta:**
  1. `Manifest` gana `paths: FrozenPaths { runs_root, worktrees_root }`
     — los paths **resueltos y absolutos** al crear el run (post
     `YUNTA_HOME`, post capas).
  2. `resume`/`status`/`stats`/`gc` sobre un run existente leen del
     manifest, nunca de la config actual. Problema del huevo y la
     gallina (encontrar el manifest necesita saber dónde buscar):
     `resume` busca en orden — (a) `paths.runs` actual, (b) el default
     — y una vez abierto el manifest, **todo lo demás** (worktree,
     artifacts) sale de los paths congelados. Un run creado bajo paths
     viejos que ya no figuran en ninguna capa requiere `YUNTA_HOME`
     apuntando ahí — documentado como límite (encontrar lo inhallable no
     es resoluble sin un índice global, que sería estado derivado como
     fuente de verdad: I2 lo prohíbe).
  3. Campos nuevos del manifest = bump de `schema_version` del manifest
     con lector tolerante (manifest viejo sin `paths` → fallback a
     config actual, comportamiento de hoy, sin romper runs existentes).
- **✓ Criterios:** el ✓ original de T2.4, ejecutado: crear run, cambiar
  `paths.worktrees` en la config, `resume` completa usando el worktree
  original; manifest pre-DI-07 sigue resumible.

### DI-08 — Canal cross-process: `cancel` real, Ctrl-C, lock huérfano `[x]`

- **Origen:** T7.1 ("Pendiente explícito #9") + T4.2 (staleness del lock
  de `none`). Tres síntomas, una causa: nada identifica los procesos de
  un run vivo desde fuera del proceso `yunta` que lo corre.
- **Síntomas:** (a) `yunta cancel` solo reporta, jamás cancela un run
  vivo; (b) Ctrl-C sobre `yunta run` no extermina el árbol (las sesiones
  corren en process groups propios que no reciben el SIGINT del
  foreground); (c) un crash con `isolation: none` deja
  `yunta-none.lock` huérfano que un humano borra a mano. A4 ("todo
  camino de cancelación extermina el árbol completo") está incumplido
  para los caminos externos.
- **Solución propuesta:**
  1. **Registro de proceso por run** — `run.dir/scratch/engine.json`
     (scratch: no es artifact ni estado del log; es efímero del proceso):
     ```json
     { "engine_pid": 12345, "started_at": "...",
       "process_groups": [12401, 12455] }
     ```
     `execute_run` lo escribe al arrancar; `dispatch_session`/hooks/
     executors agregan su pgid al spawnear y lo quitan al cerrar
     (append-rewrite atómico vía tempfile+rename); se borra al terminal.
  2. **Liveness sin unsafe ni dependencia nueva:** `kill -0 <pid>` vía
     `std::process::Command` (`kill` es POSIX; Windows ya está fuera de
     alcance por D94). Helper único `process_alive(pid) -> bool` en el
     shell imperativo del CLI/engine — jamás en el core puro.
  3. **Ctrl-C (mismo proceso):** `main` instala `tokio::signal::ctrl_c`
     → dispara el `CancellationToken` raíz del run → el camino
     interrupt→kill **ya construido** (T3.3/T4.6) extermina el árbol,
     emite `run_paused { reason: "cancelled by user" }`, libera el lock
     de `none`, borra `engine.json`, sale. El único código nuevo es la
     señal → token; la cancelación en sí ya existe y ya está testeada.
  4. **`yunta cancel <run_id>` (proceso separado):** lee `engine.json`.
     Engine vivo → `SIGINT` al `engine_pid` (que ahora lo maneja, punto
     3) y espera el terminal en el log con timeout; escalación `SIGKILL`
     a los `process_groups` si no cierra. Engine muerto con pgids
     registrados (crash) → mata pgids huérfanos directamente, emite
     `run_paused { reason: "cancelled after crash" }` y limpia. Sin
     `engine.json` → el comportamiento actual (reportar lo que el log
     dice).
  5. **Lock de `none` con dueño:** el lock file pasa de vacío a
     `{ "pid": ... }`. **Corrección al implementar:** la versión
     anterior incluía `created_at` — se quita porque ninguna decisión lo
     consume (la vida del dueño se decide con `kill -0`, jamás por
     antigüedad) y poblarlo exigiría plumbing de `Clock` a
     `prepare_worktree` solo para un campo informativo; el engine hoy no
     tiene ni un `now()` directo y eso vale conservarlo.
     `prepare_worktree` ante lock
     existente: dueño vivo → `Locked` (como hoy); dueño muerto → lo roba
     con un warning explícito por stderr (degradación explícita, nunca
     silenciosa). Lock legacy vacío → tratarlo como sin dueño
     verificable: conservador, `Locked` con mensaje que explica cómo
     borrar a mano (no romper el contrato viejo adivinando).
  6. `--detach` (M8/D101) consumirá el mismo `engine.json` — este diseño
     es su prerequisito, no un rival.
- **✓ Criterios:**
  - Test con proceso hijo que ignora SIGINT (el patrón de T3.3): Ctrl-C
    simulado (enviar SIGINT al proceso `yunta` de test) → cero zombies,
    `run_paused` en el log, lock de `none` liberado.
  - `yunta cancel` desde otro proceso sobre un run vivo con una sesión
    mock lenta → la sesión muere, el log queda terminal.
  - Crash simulado (SIGKILL al engine) → `cancel` limpia pgids
    huérfanos; un `run` nuevo bajo `none` roba el lock del muerto con
    warning.
- **No hacer:** ni daemon, ni socket, ni base de datos de procesos — el
  archivo por run alcanza y muere con el run. No usar `libc`/unsafe
  (forbid está en todos los crates y no se toca).

### DI-09 — Eventos de sesión: `agent_session_opened` / `agent_message` `[x]`

- **Origen:** M-0 "Pendiente explícito #8": "emitir el detalle por sesión
  requiere un emitter dentro de `dispatch_session`. Gatillo: T7.3 o
  cuando `status` necesite mostrar la sesión viva" — T7.3 pasó hace rato.
- **Síntoma:** el ciclo es auditable a nivel tarea pero una sesión de
  agente no deja rastro propio en el log: ni qué modelo abrió, ni el
  session_id del adapter (imprescindible para DI-23 `resume_session`),
  ni el resumen de mensajes que §3 cataloga.
- **Solución propuesta:**
  1. Emitter en `dispatch_session`: al abrir, `agent_session_opened
     { session_id, adapter, model, agent? }` (los campos ya
     especificados por T2.0 en `docs/eventos.md` — implementarlos tal
     cual, no rediseñarlos); al recibir eventos del adapter, un
     `agent_message` **acotado** (digest/kind + tamaño, jamás contenido
     completo — I12/O3: el payload no puede portar secretos ni contenido
     íntegro). **Precisión al implementar:** para `note`, "acotado"
     significa contenido cero — `text` lleva `N bytes, sha256 <prefijo>`
     y nada más; un truncado seguiría pudiendo portar un secreto corto,
     así que no alcanza (eventos.md §5.6 actualizado con el racional).
  2. `derive()` los ignora para el estado de nodos (no cambian
     transiciones) pero `stats` gana tokens por sesión y `status` puede
     mostrar "session open (model X)" para el nodo `running`.
  3. Es prerequisito de DI-23: `resume_session` necesita leer el último
     `session_id` del log.
- **✓ Criterios:** un run mock deja en el log un `agent_session_opened`
  por sesión con `session_id` no vacío; test de redacción (I12) sobre
  `agent_message`: un secreto inyectado en el stream del mock jamás
  aparece en el payload persistido; replay determinista sin cambios de
  estado.

### DI-10 — Findings del engine sobreviven la promoción `[x]`

- **Origen:** T9.2 — "un finding emitido por el engine (p. ej. una
  ampliación denegada) no sobrevive a esta copia: vive solo en el event
  log del padre. Documentado, no resuelto."
- **Síntoma:** §10.2 promete "artifacts, ledger y **findings** del
  antecesor" en el contexto inicial del sucesor; los findings sin
  artifact (denegaciones D80, findings en caliente futuros) se pierden.
- **Solución propuesta:** en el cierre por promoción (el único punto que
  lo necesita), el engine **deriva del log** un
  `artifacts/findings-inherited.yaml` con schema `kind: findings`
  (T5.12), aplicando la deduplicación normativa (location + título
  normalizado, sin perder autorías). Derivación pura log→archivo
  (función en el functional core, IO en el shell); la copia de
  directorio de `drive_promotions` ya lo arrastra sin tocarse. Si no hay
  findings, no se escribe archivo (cero ruido).
- **✓ Criterios:** run que promueve con una denegación de scope en el
  log → el sucesor tiene `findings-inherited.yaml` válido contra el
  parser de T5.12, con ese finding; sin findings → sin archivo; la
  derivación es determinista (mismo log → mismos bytes).
- **No hacer:** no escribirlo en todo cierre "por las dudas" — solo
  promoción lo consume; generalizarlo es territorio de T9.3.

### DI-11 — Cancelación de `loop`/`check`/`executor` bajo `join: any` `[x]`

- **Origen:** T4.6 — "un hijo `kind: loop` no es cancelable (…) gatillo:
  alguien necesita de verdad un loop dentro de un grupo parallel". El
  gatillo formal no llegó, pero A4 es invariante ("todo camino de
  cancelación extermina el árbol completo") y DI-08 agrega caminos de
  cancelación nuevos que lo pisan: postergarlo ahora acumula vicio.
- **Solución propuesta:** `run_task`/`dispatch_session` (loop),
  `execute_check` y `execute_executor` aceptan el `CancellationToken`
  que `execute_node` ya recibe y lo consultan en sus puntos de espera
  (`tokio::select!` sobre la sesión/comando vs. `token.cancelled()`).
  Cancelado → interrupt→kill del proceso en curso (mecanismo T3.3
  existente). **Precisión al implementar — el destino del nodo depende
  de QUIÉN canceló, y son dos semánticas distintas:** (a) carrera
  `join: any` (un hermano ganó) → el perdedor SÍ registra
  `node_failed` "interrupted: a sibling…" — el grupo no puede cerrar
  sobre un huérfano (un run `finished` con un nodo `running` eterno
  rompería status y el propio invariante de cierre); (b) cancelación
  del usuario (token raíz de DI-08) → sin evento terminal fabricado:
  el nodo queda huérfano, el run pausa "cancelled by user", y el
  resume lo trata por `on_interrupt` exactamente como un crash — que
  es lo que hace la cancelación resumible (un `node_failed` fabricado
  acá dejaría el run pausado sin salida). La versión anterior de este
  ítem prescribía (b) para todo; la distinción sale de implementarlo.
- **✓ Criterios:** grupo `join: any` con un hijo `loop` lento (mock con
  latencia) y un hermano rápido → el loop muere antes de su propio final,
  cero zombies, y el resume posterior lo re-trata por `on_interrupt`;
  ídem con `check` y `executor` (test parametrizado).

### DI-12 — Colisión de escritura en el fan-out implícito del DAG `[x]`

- **Origen:** T4.1 — "el riesgo físico es idéntico (mismo worktree
  compartido) pero acá no hay warning de check".
- **Solución propuesta:** extender D100 a nodos top-level: para cada par
  de nodos escribibles **sin camino de dependencia entre sí** (clausura
  de `depends_on` — pueden quedar `ready` juntos) y solo cuando el
  `max_parallel_nodes` resuelto es > 1 (con 1, la ejecución es secuencial
  y escrituras sucesivas al mismo worktree son legítimas): scope
  declarado solapado → **error** (verificable de antemano, mismo rango
  que en `parallel`); 2+ escribibles sin scope declarado → **warning**
  una sola vez por componente conexa (no por par: el ruido mata la
  señal).
- **✓ Criterios:** dos nodos independientes con scope solapado +
  `max_parallel_nodes: 2` → error citando ambos; mismos nodos con
  `max_parallel_nodes: 1` → limpio; sin scope declarado → warning único;
  workflow secuencial (cadena de depends_on) → jamás afectado.
- **No hacer:** no simular el interleaving del scheduler en `check` —
  la aproximación estática (par sin orden relativo) es la regla, y se
  documenta como tal.

### DI-13 — Schema completo de T1.1: round-trip de los YAML de referencia `[x]`

- **Origen:** el ✓ de T1.1 ("los tres YAML de referencia parsean sin
  pérdida, round-trip") sigue sin cumplirse. Faltantes confirmados
  contra el schema actual: `skills:` (nodo), `interactive:`,
  `fresh_context:`, `yunta_schema:` (workflow), `on_finish:`, y el gate
  interno (DI-04). `coordination:` (D49) es de T8.2 y los `kind:
  workflow` de T9.3 — esos dos llegan con su tarea.
- **Por qué es Nivel 2 y no cosmético:** los YAML de referencia son los
  fixtures normativos; cada campo que no parsea es un workflow de la doc
  oficial que revienta en el primer `yunta check` de un usuario.
- **Solución propuesta — campo por campo, cada uno con su consumidor o
  su degradación explícita (A6: jamás aceptar-e-ignorar en silencio):**
  1. `skills: [nombres]` en nodo (+ `node_defaults`) — **la pieza más
     grande de este ítem**, verificado: el recorte de M-0 dropeó
     `skills: Vec<PathBuf>` de `SessionRequest` explícitamente
     (`session.rs`'s propio doc comment), así que T7.3 NO construyó el
     montaje. Falta la cadena completa: (a) campo en el schema del nodo
     + `node_defaults` + `skills.paths`/`skills.always` de config (la
     referencia los declara); (b) resolución de nombre → directorio de
     skill contra `skills.paths` (repo primero, mismo orden que
     knowledge), error accionable si no existe; (c) restaurar
     `SessionRequest.skills: Vec<PathBuf>` per Spec Adapter v0.2; (d)
     montaje en `claude-code` por su mecanismo nativo de skills; el
     mock lo registra en el fixture para testear sin LLM (A8); (e)
     adapter sin mecanismo nativo → `capability_degraded` en el log
     (A6), nunca error fatal (una skill es instrucción adicional, no
     correctness) — exige agregar la capacidad `skills` a
     `Capabilities`.
  2. `interactive: bool` en nodo: dato de presentación para DI-02
     (§4.1). Parsear + pasarlo a la superficie; sin superficie no cambia
     nada.
  3. `fresh_context: bool` en nodo (§8.2): `true` = la sesión del nodo se
     abre sin `resume` de sesión previa (rehidratación pura). Hoy TODO
     nodo es efectivamente fresh — parsear y validar que `false` sin
     DI-23 construido sea un error de `check` accionable ("declares
     `fresh_context: false` but session resume is not supported yet"),
     no una aceptación muda.
  4. `yunta_schema: ">=1 <2"` en workflow (§2.1): parsear (string de
     rango semver), congelar en manifest, y `check` compara contra el
     schema del binario (constante `YUNTA_SCHEMA: u32 = 1`) — fuera de
     rango = error accionable. Sin declarar → se infiere del binario
     (texto de referencia).
  5. `on_finish:` en workflow: `Vec<OnFinishStep>` con
     `Cleanup { cleanup: CleanupTarget::Worktree }` y
     `Distill { distill: Vec<String> }`. **`cleanup: worktree` se
     implementa** (cierra la deuda de T4.2): al `Finish` — nunca al
     `Paused` — `git worktree remove`, y la rama del run se borra solo
     si está mergeada o pusheada a upstream (`git branch -d`, jamás
     `-D`; si git se niega, la rama queda y no es error — un distill
     recién commiteado ahí, DI-24, jamás debe morir por el cleanup).
     Siempre después del export de `events.jsonl` (T5.8). **`distill`
     se diseña e implementa completo en DI-24** — el "mecanismo sin
     especificar" de T5.8 queda resuelto ahí, no con una degradación
     placeholder.
  6. Test de cierre: los tres YAML de referencia completos (menos los
     bloques de T8.2/T9.3 **y el fan-out `runners: []`/`agent:` por
     nodo de T9.4**, marcados) como fixtures reales en
     `crates/core/tests/fixtures/`, round-trip byte-comparable a nivel
     de árbol serde.
  7. **Ampliación al implementar (el ✓ del round-trip de config.yaml la
     exige — faltantes confirmados contra la referencia real, no
     listados arriba):** `version: 1` (validado == 1 al cargar la capa),
     `defaults.runner` (consumidor: nodo sin `runner:`),
     `defaults.timeout_minutes` (consumidor: `Budget.timeout` — cierra
     la corrección de DI-05 etapa 3), `defaults.on_failure` (solo
     `pause` implementado: otro valor es error de check accionable,
     jamás aceptación muda), `adapters.*.adapter_settings` (passthrough
     opaco a `SessionRequest.adapter_settings`), `secrets:` (nombres de
     env vars; consumidor: `SessionRequest.env` se puebla SOLO con las
     declaradas presentes en el ambiente — I12), `telemetry:` (parse y
     nada más: la propia referencia lo declara inerte hasta T13.3), y
     `pricing` con la forma de la referencia
     (`{modelo: {cost_per_1k_tokens}}` — la doc gana sobre el
     `{modelo: f64}` que T7.5 implementó). Los `2_000_000` con guión
     bajo de la referencia no son enteros en YAML 1.2: el fixture usa
     la forma canónica sin separador (decisión ya registrada en DI-05).
- **✓ Criterios:** `build-feature.yaml` y `config.yaml` de referencia
  parsean round-trip; `cleanup: worktree` deja el disco sin el worktree
  tras un run terminado (y el run.dir intacto); `distill` declarado
  produce la degradación explícita y nada más; `fresh_context: false`
  falla `check` con mensaje accionable.
- **Dependencias:** DI-04 (gate interno) es parte de este cierre; DI-02
  consume `interactive`; DI-23 destraba `fresh_context: false`.

### DI-14 — Retención a nivel de base de datos `[ ]`

- **Origen:** T7.1 ("Pendiente explícito #10"): `gc` borra archivos pero
  §8.3 implica purga de filas del event log según `retention_days`.
- **Solución propuesta:** `Storage::purge_run(run_id)` (borra las filas
  de ese run; la interfaz de storage se mantiene mínima, D53 — un método,
  no un query language). `gc` lo llama **solo** cuando: el run es
  terminal, superó `retention_days`, y su `events.jsonl` existe en el
  run.dir archivado… con una decisión explícita: si `gc` también borra el
  run.dir (que es lo que hace hoy), purgar la DB elimina el último
  rastro. Regla propuesta: `gc` purga filas únicamente de runs cuyo
  run.dir **ya no existe** (borrado por un gc anterior o a mano) — la DB
  nunca es la primera copia en morir; el orden de muerte es
  run.dir-con-jsonl primero (que es autocontenido, §8.3), filas después,
  en la corrida siguiente. `--dry-run` lo reporta.
- **✓ Criterios:** run terminal viejo con run.dir presente → primera
  corrida de `gc` borra run.dir (comportamiento actual), segunda corrida
  purga filas; run no terminal jamás se purga; `verify`/`status` sobre
  un run purgado da "unknown run", no un estado corrupto.

### DI-24 — `on_finish.distill`: mecanismo completo `[x]` (ADR D107)

- **Origen:** T5.8 dejó `distill` sin implementar con la pregunta
  abierta "¿sesión de agente o transformación determinista?" y la nota
  de que su mecanismo "no está documentado en Notion". Releyendo las
  fuentes con el resto del sistema ya construido, **la pregunta tiene
  más respuesta normativa de la que parecía** — lo que falta es juntarla
  y cerrar los huecos con decisiones explícitas. Esta sección es esa
  propuesta de mecanismo; al implementarse **se registra como ADR nuevo
  en Notion** (resuelve formalmente la cuestión abierta de T5.8, y la
  regla es que la deuda no se cierra sin ADR).
- **Lo que las fuentes SÍ fijan (no negociable):**
  - **Destino:** `.yunta/knowledge/` — §8.3: "destila el conocimiento
    durable (ADRs, CONTEXT.md **bajo `.yunta/knowledge/`**)"; §9.2: la
    capa `repo` de knowledge es "`.yunta/knowledge/`, **lo destilado
    acá**". El destilado alimenta directamente la fuente `knowledge`
    (T6.5) y, después, el workflow `promote-knowledge` (T10.5) que
    cura repo → org.
  - **Orden:** antes de cualquier cleanup (§8.3/D20, "antes de
    cualquier cleanup") — el engine impone la fase, el orden de
    declaración en el YAML no manda.
  - **Criterio de selección:** D20 — "lo fetcheado/generado va a la
    respuesta del nodo, lo durable a knowledge; specs efímeras,
    decisiones durables". `distill: [paths]` nombra artifacts del run
    que el workflow declara durables.
  - **Findings:** §4.1 — "sobreviven al run para el gate de promoción
    o la destilación", pero "el engine **no impone** un artifact de
    cierre: qué hacer con los hallazgos (consolidar, promover,
    destilar, ignorar) es decisión del workflow". Insumo disponible,
    jamás auto-destilado.
- **Decisión central: transformación determinista, nunca sesión de
  agente.** Racional: (a) §11.1 fija para los hooks del ciclo de nodo
  "solo comandos, nunca IA (para eso existen los nodos)" — `on_finish`
  es exactamente la misma clase de pegamento de cierre; (b) un
  destilado escrito por LLM en el cierre sería contenido no verificable
  entrando a la capa de conocimiento sin pasar por ningún criterio ni
  scope — la puerta trasera perfecta contra "la palabra del agente no
  es evidencia"; (c) A8: el cierre debe correr con mock. **Si un equipo
  quiere un resumen redactado por agente, lo produce un nodo `prompt`
  como artifact** (con su runner, su presupuesto, su verificación) y
  `distill` exporta ese artifact — composición, no un mecanismo nuevo.
  Lo mismo para findings: un nodo consolidador los escribe como
  artifact `kind: findings` y se lo nombra en `distill`.
- **Mecanismo propuesto:**
  1. **Qué hace:** para cada path declarado en `distill:` (relativo a
     `run.dir/artifacts/`, la misma convención de T7.7), el engine
     copia el archivo a
     `<worktree>/.yunta/knowledge/distilled/<workflow>/<run_id>/<name>`
     y escribe al lado un `provenance.yaml` derivado por función pura
     de (log, manifest):
     ```yaml
     source_run: run-20260820-...
     workflow: build-feature
     workflow_hash: "sha256:..."
     mode: standard
     distilled_at: "..."            # del Clock inyectado, jamás SystemTime directo
     artifacts:
       - { name: plan.yaml, content_hash: "sha256:..." }
     verification:                   # derivado del log — evidencia, no palabra
       criteria: { executed: 12, green: 12, reused: 3 }
       findings: { blocking: 0, minor: 2 }
     ```
     Un subdirectorio por run — **jamás un índice compartido mutable**
     (dos PRs concurrentes destilando al mismo índice = conflicto de
     merge garantizado; ese vicio se evita por construcción).
  2. **Cómo llega al repo:** bajo `isolation: worktree`, el engine
     commitea los archivos destilados a la rama del run con mensaje
     convencional (`docs(knowledge): distill from <run_id>`) — el
     conocimiento viaja en el mismo PR que el trabajo y pasa por la
     misma revisión humana; si la rama ya tiene upstream (el nodo `pr`
     hizo `push -u`), el engine pushea ese commit; si no, el commit
     queda en la rama local (y `cleanup` usa `-d`, que se niega a
     borrar ramas no mergeadas — DI-13 ya lo fija — así que nunca se
     pierde). Bajo `isolation: none`, los archivos quedan **sin
     commitear** en el checkout del usuario: el engine jamás commitea
     la rama del usuario; el árbol queda sucio a la vista y el próximo
     run bajo `none` se negará hasta que el humano commitee o descarte
     — fricción visible y correcta, no un bug (documentar en la guía).
  3. **Secuencia de cierre resultante (reemplaza a la actual):**
     `distill` (+ sus eventos) → `run_finished` → export
     `events.jsonl` → `cleanup`. El export va después de
     `run_finished` porque snapshotea el log completo; distill va antes
     porque nada se emite después de `run_finished` (I3).
  4. **Cuándo corre:** solo en cierres reales — `Finish` (Done) y
     promoción (`Promoted`: el conocimiento del intento corto es
     conocimiento; además el sucesor lo hereda vía la capa repo, que
     complementa a DI-10). Jamás en `Paused` (el run no cerró) ni en
     cancelación.
  5. **Degradación explícita:** un path declarado que ningún nodo
     produjo → `finding_posted` (severity `minor`, title "distill:
     declared artifact `X` was never produced") — el canal general de
     "registrado, nunca perdido" que §4.1 ya da, sin inventar un
     event kind nuevo; el resto de los paths se destila igual. El
     `provenance.yaml` lista también los ausentes con `missing: true`.
  6. **`check` estático:** cada path de `distill:` debe coincidir con
     algún `artifacts.produces` declarado en el workflow — error de
     check si no (la versión estática del punto 5; el punto 5 cubre el
     caso "declarado pero no producido en runtime").
- **✓ Criterios:**
  - Run mock con `distill: [plan.yaml]` → el worktree termina con
    `.yunta/knowledge/distilled/<wf>/<run>/plan.yaml` + provenance
    válido y un commit en la rama del run; `derive` del log intacto;
    el mismo run re-derivado da provenance byte-idéntico (pura).
  - Un nodo siguiente (otro run) con `context: [{knowledge: {}}]`
    monta lo destilado — el ciclo §8.3→§9.2 cerrado end-to-end.
  - Path no producido → finding en el log + el resto destilado;
    `check` rechaza un path que ningún nodo declara producir.
  - Bajo `none`: archivos presentes sin commit; el run siguiente bajo
    `none` se rehúsa por árbol sucio (test que documenta la fricción).
  - `Paused` → no destila nada.
- **Dependencias:** DI-13 (schema de `on_finish` y `cleanup` con
  `-d`); compone con DI-10 (findings de promoción) sin solaparse.
- **No hacer:** ninguna sesión de agente en el cierre; ningún índice
  global compartido; jamás commitear el checkout del usuario bajo
  `none`; no auto-destilar findings sin declaración del workflow.

---

## Nivel 3 — calidad, optimización e higiene

### DI-15 — Orden aprendido de criterios (duración histórica) `[ ]`

- **Origen:** T5.9 — el short-circuit por duración histórica (§5.4/D62)
  no se construyó porque el log no registra duraciones por criterio.
- **Solución propuesta:** (1) `CriterionResult` gana `duration_ms:
  Option<u64>` (aditivo, D70, lector tolerante; `reused: true` →
  `None`). (2) El pre-check ordena por mediana histórica ascendente
  (fallan-rápido primero es el objetivo de D62; los sin historial van al
  final en orden declarado). La mediana se computa del log del run
  actual + historial del workflow si está disponible barato; si no, solo
  intra-run — el Contrato solo exige que el orden jamás altere el
  veredicto. (3) Property test: para cualquier permutación, el conjunto
  de resultados es idéntico (ya es un principio de T5.9 — extender el
  test existente).
- **✓ Criterios:** el ✓ original de T5.9 pendiente, ejecutado; payloads
  viejos sin `duration_ms` parsean.

### DI-16 — Race de `max_per_run` bajo concurrencia `[ ]`

- **Origen:** T5.10/T5.11 — "el cap puede excederse hasta en
  `concurrency - 1` dentro de un lote". Aceptada entonces porque
  serializar la evaluación anulaba T5.10.
- **Solución propuesta (cierra la race sin serializar el dispatch):** el
  pre-check del criterio propuesto (lo caro) corre **fuera** de toda
  sección crítica; solo la ventana `leer granted_count del log → decidir
  cap → emitir granted/denied` se protege con un `tokio::Mutex` del
  run. Las sesiones siguen 100% concurrentes; solo la contabilidad del
  cap es atómica.
- **✓ Criterios:** test con `concurrency: 4` y 4 tareas que piden
  ampliación simultánea bajo `rules` con `max_per_run: 2` → exactamente
  2 granted, 2 escaladas, determinista en el conteo (no en el orden).
  Borrar el párrafo de "soft race documentada" de `m0-status.md` y el
  doc comment de `granted_count`.

### DI-17 — `context:` a nivel loop/tarea `[ ]`

- **Origen:** T6.1 — `context:` solo se resuelve para `kind: prompt`;
  `check` lo rechaza en cualquier otro kind. El workflow de referencia
  no lo usa en loops, pero §9 no lo restringe y el brief de tarea (§5.2)
  se enriquecería con las mismas fuentes.
- **Solución propuesta:** permitir `context:` en `kind: loop`: se
  resuelve **una vez por tarea** (no por nodo) al armar el brief, con la
  misma materialización `context/<hash>/` y el mismo
  `context_assembled`; las fuentes volátiles (`command`, `run-events`)
  se re-resuelven por tarea, las estables se memoizan por hash (D42 ya
  da el criterio de clases de estabilidad). `check` deja de rechazarlo
  para `loop`; sigue rechazándolo para `bash`/`check`/`executor`/`gate`
  (sin sesión que lo consuma).
- **✓ Criterios:** loop con `context: [{files: …}]` → cada brief de
  tarea lo incluye; `context_assembled` por tarea en el log; en `bash`
  sigue siendo error de `check`.

### DI-18 — Reglas menores de `check` `[ ]`

- **Origen:** T1.3 diferidas + nota de `schedule.rs`.
- **Solución propuesta:** (1) `max_parallel_nodes >= 1` como error de
  `check` cuando la config lo resuelve en 0 (el clamp del scheduler
  queda como defensa en profundidad, con su comentario actualizado).
  (2) Warning D48: scan estático de nodos `bash`/hooks cuyo comando
  contiene `git push` referenciando la rama base resuelta
  (`{{project.base_branch}}` o el literal) sin pasar por un gate previo
  en el DAG — warning, no error (el texto de T1.3 dice warning).
- **✓ Criterios:** fixture con `max_parallel_nodes: 0` → error; workflow
  de referencia (`pr` hace push a `{{run.branch}}`, no a base) → limpio;
  un `git push origin main` directo → warning que nombra D48.

### DI-19 — Higiene: helpers duplicados, params de `create_run` `[ ]`

- **Origen:** acumulado. (a) `flatten`/walk de nodos duplicado en
  `progress.rs`, `stats.rs`, `verification_effectiveness.rs`, `check.rs`
  (`collect_ids`); (b) `mode_of` en `stats.rs` vs. lectura de
  `run_created` en `run/mod.rs`; (c) `create_run` ya lleva 7 parámetros
  posicionales (señal de CLAUDE.md sobre el techo blando).
- **Solución propuesta:** (a) `Workflow::iter_nodes(&self) ->
  impl Iterator<Item = &Node>` en `yunta-core` (pre-orden, hijos de
  `parallel` incluidos) — un solo dueño del recorrido; los cuatro sitios
  migran. (b) `RunCreatedPayload` ya es la fuente; helper
  `events::run_mode(&[Event]) -> &str` en core. (c)
  `CreateRunParams { run_id, manifest, runs_root, mode, promoted_from }`
  + `storage`/`clock` como argumentos (los dos con lifetime distinto del
  resto). Sin cambio de comportamiento — refactor puro, tests existentes
  como red.
- **✓ Criterios:** cero duplicados de recorrido (grep estructural);
  clippy/tests verdes sin cambio de golden outputs.

### DI-20 — Techo en capas para `scope_expansion` `[ ]`

- **Origen:** T5.11 gap #1 — §6.2 dice que el modo sigue el modelo de
  techo de §6.1 pero no da forma YAML. **Requiere decisión de schema
  (mini-ADR en este doc antes de codear), propuesta:**
  ```yaml
  permissions:
    scope_expansion:
      max_mode: ask        # rules | ask | deny — techo; capas inferiores solo endurecen
  ```
  Orden de severidad `rules < ask < deny` (más permisivo → menos). Merge
  invertido como el resto de `permissions` (§6.1): una capa inferior
  puede declarar un modo **más duro** que el techo, jamás más blando; un
  nodo que declara `mode: rules` bajo un techo `ask` falla `check`
  citando la capa (mismo formato de error que T5.7).
- **✓ Criterios:** org con `max_mode: ask` + nodo `rules` → error de
  check citando la capa org; nodo `deny` → limpio; sin techo declarado →
  comportamiento actual intacto.

### DI-21 — `events.jsonl` en el camino `Broken` `[ ]`

- **Origen:** T5.8 — el retorno temprano de `ScheduleStep::Broken` no
  exporta. Un log corrupto es exactamente el que más querés tener
  exportado para el forense.
- **Solución propuesta:** best-effort antes del `Err`: exportar lo que
  el log tenga (la serialización de eventos individuales no depende de
  que la *secuencia* sea coherente); si la propia exportación falla, el
  error original de `Broken` gana (no se enmascara). Un comentario en el
  código deja claro el orden de prioridad de errores.
- **✓ Criterios:** run con log truncado artificialmente → `execute_run`
  devuelve `Broken` **y** `events.jsonl` existe con los eventos legibles.

### DI-22 — Smoke tests en vivo pendientes `[ ]`

- **Origen:** T7.4 (`codex`) y T7.7 (`GitHubForge`) — construidos contra
  documentación/fuente real, sin corrida en vivo (este sandbox no tiene
  ni el binario `codex` ni token+repo descartable de GitHub).
- **Solución propuesta:** no es código: es una **checklist ejecutable
  documentada** para la primera sesión con credenciales — (a) el
  workflow de 3 nodos de T7.3 contra `codex` real (mapeo de sandbox +
  parser; corregir ahí lo que difiera, nunca de paso en otra tarea);
  (b) el ciclo publish→approve→poll de T7.7 contra un repo descartable
  (endpoints, permisos del token, shape de `reviews`). Resultado de cada
  corrida → actualizar la entrada correspondiente de `m0-status.md`.
- **✓ Criterios:** ambas checklists corridas y sus entradas de deuda
  cerradas; cualquier corrección hecha con test de regresión.

### DI-23 — `on_interrupt: resume_session` `[ ]`

- **Origen:** T4.5/D99 — la tercera variante existe en el Contrato pero
  no en el schema ("no consumer: nothing resumes a session on crash
  recovery yet"). El gatillo real es DI-09: sin `agent_session_opened`
  en el log no hay `session_id` que retomar.
- **Solución propuesta:** (1) depende de DI-09. (2) Variante
  `ResumeSession` en `OnInterrupt`; en el resume, un nodo huérfano con
  esa política busca el último `agent_session_opened` de su intento en
  el log y llama `adapter.resume(session_id)` (capacidad
  `resume_session` — claude-code la declara; adapter sin la capacidad →
  error de `check` al declarar la política sobre un rol cuyo candidato
  resuelto no la tiene, A6/I17). Sin sesión registrada (crash antes de
  abrir) → degrada a `restart_node` con evento explícito. (3)
  `fresh_context: false` (DI-13) comparte esta mecánica.
- **✓ Criterios:** mock con soporte de resume scriptado: matar el engine
  a mitad de sesión, resume con `resume_session` → la sesión continúa
  (el fixture lo demuestra por efectos), no se abre una nueva; sin
  capacidad → check lo rechaza; sin sesión previa → restart con evento.

---

## Posturas cerradas (decisión registrada — no son deuda)

Estos puntos aparecieron como "deuda/límite" en `m0-status.md` pero la
resolución correcta es **mantener la restricción y documentarla**, no
construir algo. Si alguna vez duelen de verdad, reabrirlos requiere ADR.

- **P-01 — Gate dentro de `parallel`: rechazado por diseño.** La
  resolución de un gate es un round-trip humano/forja uno-a-la-vez;
  ninguna semántica de `join` está definida para eso y no hay caso de
  uso real. El error de `check` ES la feature.
- **P-02 — Consenso multi-reviewer: fuera del engine.** "Última revisión
  decisiva gana"; cuántas aprobaciones hacen falta es política de la
  forja (branch protection). Duplicarlo en Yunta crearía dos fuentes de
  verdad sobre la misma pregunta.
- **P-03 — `ForgeKind` con un solo variante.** El punto de extensión
  (trait + enum cerrado) ya existe; un segundo forge (GitLab) se agrega
  con demanda real, no especulativamente (CLAUDE.md: una abstracción sin
  segunda implementación real es costo sin beneficio — acá el trait ya
  se paga con `MockForge`).
- **P-04 — `include:` de modos solo nombra nodos top-level.** Un grupo
  `parallel` entra o sale entero. Nombrar un hijo ya es error de `check`
  (cae en `ModeReferencesUnknownNode`) — comportamiento correcto;
  documentarlo en la guía de usuario (M10).
- **P-05 — Convención de paths de artifacts en gates externos**
  (relativos a `run.dir/artifacts/`, mismo path relativo en el branch):
  se documenta en la guía (M10), no se cambia.
- **P-06 — Clasificación de modo "nodo temprano + gate" = composición
  T9.1+T9.2+DI-04.** No existe un tercer mecanismo que mute el modo de
  un run en curso — D22 lo descarta explícitamente ("Descartado: mutar
  modo/manifest del run en curso"). El patrón se expresa como: arrancar
  en el modo piso, nodo temprano propone, gate interno (DI-04) confirma,
  promoción (T9.2) escala. Con DI-04 cerrado, el patrón es 100%
  expresable.

## Pendientes con dueño en el plan (referencia, sin duplicar)

- **T2.5** — cadena de hashes del event log + `yunta verify` (política ya
  escrita en `docs/eventos.md` §3). Prioridad alta de facto: T10.4
  (recibo) la exige y cada evento nuevo que se emite sin hash agranda la
  migración. Recomendación: primera tarea del plan a retomar tras el
  Nivel 1 de este doc.
- **T9.3** — `kind: workflow` (composición). Absorbe: la fuente
  `artifact` cross-run general por vínculos (§12) que DI-10 resuelve de
  forma mínima; `max_workflow_depth` (DI-05); el "árbol para
  composición" de `status` (§8.5).
- **T9.4** — fan-out `runners: []` (§13.2) + `agent:` a nivel nodo
  (§13.3) — cierra M9.
- **M8 (T8.1/T8.2)** — `yunta mcp` + MCP por-run. Consumidor natural de
  DI-01/DI-02/DI-03/DI-04 (la superficie `resolve_gate` reutiliza los
  mismos objetos) y de DI-08 (`--detach` usa `engine.json`).
- **A-01–A-10** — deuda consciente de producto (Notion): sin cambio; ese
  documento manda sobre lo suyo.

## Orden de ataque recomendado

1. **Nivel 1 completo** (DI-01 → DI-06): son promesas normativas con
   gatillo cumplido; cada semana que pasan abiertas es una semana en que
   el producto contradice su propia documentación. DI-03 antes que
   DI-04 (el estado `waiting` es prerequisito limpio del gate interno).
2. **T2.5** (plan): antes de que el volumen de eventos siga creciendo.
3. **Nivel 2** en el orden listado — DI-08 y DI-09 primero (desbloquean
   DI-23 y `--detach` de M8); DI-13 puede avanzar en paralelo porque es
   mayormente schema, y DI-24 (distill) inmediatamente después de DI-13,
   que le da el schema de `on_finish` — juntos cierran el ciclo completo
   §8.3 → §9.2 (destilar → montar como knowledge) que hoy está cortado.
4. **T9.3/T9.4** para cerrar M9, ya con DI-10 resuelto de forma mínima
   (T9.3 lo generaliza).
5. **Nivel 3** intercalado como tareas chicas entre las grandes, nunca
   "de paso".
