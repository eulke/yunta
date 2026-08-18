# M-0 — Estado de implementación

Documento de seguimiento, no normativo — se actualiza en cada sesión de trabajo para
que retomar el bootstrap no dependa de memoria de conversación. La fuente de verdad
del *qué* sigue siendo el Plan de implementación (Notion, sección M-0); esto es
únicamente el *dónde estamos* y el *qué quedó deliberadamente afuera y por qué*.

## Alcance mínimo de M-0 (tal como lo define el Plan)

- [x] **T1.0** — spec del ledger → `docs/spec-ledger.md`
- [x] **T2.0** — spec de eventos → `docs/eventos.md` (31 kinds, no 30 — discrepancia
      del Contrato reportada, ver `docs/eventos.md` §0)
- [x] **T0.1** — workspace de Cargo
- [x] **T0.3** — tipos de error (`YuntaError`) + `tracing`
- [x] **T1.1** (recorte) — schema `prompt`/`bash`/`loop` en `yunta-core`
- [x] **T2.1** — storage: SQLite WAL, event log append/read/list (`yunta-storage`)
- [x] **T2.2** — los 31 tipos de payload de evento (`yunta_core::events`)
- [x] **T2.3** — derivación de estado por replay (`yunta_engine::derive`)
- [x] **T3.1** — trait `Adapter`/`AgentSession` (M-0 cut: sin `context`/`skills`/MCP en
      `SessionRequest`) en `yunta-adapters`
- [x] **T3.2** — adapter `mock`: fixtures YAML, sesión exitosa/fallida/colgada,
      ediciones fuera de scope con `edit_hooks`
- [x] **T5.1** — parseo y registro del ledger: las 7 reglas de validación de
      `docs/spec-ledger.md` §3, todos los errores juntos
- [x] **T5.2** — ciclo de la tarea: pre-check en rojo, dispatch, post-check,
      reintentos con sesión nueva, `blocked` (`yunta_engine::run_task`)
- [x] **T5.3** — scope check por `git diff` con glob-matching real (`globset`)
- [x] **T3.3** — enforcement de presupuesto en `dispatch` (`yunta_engine::task_cycle`):
      conteo de tokens vía eventos `Usage` contra `max_tokens`, timeout de wall-clock
      contra `budget.timeout`, corte con `interrupt()` → grace period → `kill()` (A4).
      `DispatchOutcome::BudgetExceeded` distingue el corte del engine de un `Failed`
      reportado por la sesión.
- [x] **T1.4** (recorte) — manifest congelado con hashes canónicos
      (`yunta_core::Manifest` + `yunta_engine::build_manifest`); sin `inputs`/`modo`.
- [x] **T7.1** (parcial) — `yunta check <workflow> [--config <config>]` real en el
      CLI, lee YAML de disco. `run`/`status`/`resume` **siguen sin conectar** —
      las decisiones de diseño ya están resueltas (ver abajo); falta el scheduler
      recortado (T4.1) y el ciclo run.dir/artifacts que lo sostiene.
- [ ] **T7.3** — adapter `claude-code` real (no ejercitable en este sandbox: necesita
      el binario `claude` instalado y autenticado)

## Hecho de más, no nombrado explícitamente en el alcance mínimo

- [x] **T0.2** — CI (`fmt` + `clippy -D warnings` + build/test, `.github/workflows/ci.yml`).
      Confirmado corriendo verde en GitHub Actions real (run `32094336162`, rama
      `claude/yunta-m0-bootstrap-60o1u2`) — ver nota de corrección abajo.
- [x] **T1.2** (recorte) — config en capas: `runners`/`adapters`/`storage`/`paths`.
- [x] **T1.3** (recorte) — `yunta check` como función pura en `yunta-engine`.
- [x] **Revisión de calidad autoiniciada** (regla de CLAUDE.md "nada de unwrap()/expect()
      fuera de tests"): 4 violaciones encontradas por grep en código de librería,
      las 4 corregidas — `ledger.rs`/`check.rs` (detección de ciclos: el enum `Color`
      ahora carga la posición en el stack en vez de buscarla con `.position().expect()`),
      `task_cycle.rs` (el branch de timeout ahora carga su propio `Duration` en vez de
      re-derivarlo con `budget.timeout.expect(...)`), `mock/mod.rs` (`events()` llamado
      dos veces ahora degrada a un stream vacío en vez de entrar en panic). Commit
      `7aad540`.

## Decisiones de recorte explícitas (qué quedó afuera y por qué)

- **T1.1**: solo nodos `prompt`/`bash`/`loop`. Sin `gate`/`check`/`parallel`/
  `executor`/`workflow`; sin `context:`, `skills:`, `modes:`, `inputs:`, fan-out de
  `runners:`, `agent:` a nivel nodo, `permissions:`, `scope_expansion:`,
  `coordination:`. Confirmado con el usuario.
- **T1.2**: solo `runners`/`adapters`/`storage`/`paths`. Sin `mcp_servers`, `skills`,
  `baseline`/`coverage`, `secrets`, `permissions` (con su merge invertido, D51).
  Confirmado con el usuario.
- **T1.3**: solo unicidad de `id`, referencias+aciclicidad de `depends_on` (excluye
  aristas de `on_failure.goto` por I14), targets de `goto` existentes, `runner:`
  resuelto contra `runners:`. Sin coherencia de modos, scopes disjuntos en
  `parallel`/`inherit`, templates, profundidad/aciclicidad del grafo de workflows,
  techos de `permissions`, warning de push a rama base — todo requiere schema fuera
  de alcance (modos, `parallel`, `context:`/templates, `permissions:`, composición).
- **T2.0**: documenta 31 event kinds, no los "30" que dice la prosa del Contrato
  (la tabla real tiene 31 — ver `docs/eventos.md` §0). Reportado; pendiente de que
  el usuario corrija Notion si coincide con la lectura.
- **CLAUDE.md**: dice "I1–I22" en un lugar e "I1–I30" en otro, dentro de la misma
  página de Notion. El usuario va a corregirlo en Notion; mientras tanto se trabaja
  con I1–I30 (el rango real del Contrato).
- **T2.2 corrección post-implementación**: varios payloads repetían `node_id` (o
  `from_node`/`author_node_id`) ya presente en el envelope del evento. Corregido en
  `yunta_core::events` y en `docs/eventos.md` — ver commit `a488b06`.
- **T2.3**: deriva nodos (lifecycle vía `node_started`/`finished`/`failed`), tareas
  (última `task_status_changed`) y tokens totales. No deriva gates ni presupuestos
  más allá del conteo de tokens — no hay `kind: gate` ni enforcement de `limits.*`
  todavía (T3.3), así que no hay nada que derivar de eso hasta que existan.
- **T3.1**: `SessionRequest` sin `context: ResolvedContext` (M6), `skills: Vec<PathBuf>`
  (fuera del recorte de T1.1) ni `run_tools_endpoint` (MCP, M8). `AgentOutcome` y
  `AgentError` quedan `[inferido]` — la spec los nombra sin detallar sus campos;
  minimalistas a propósito hasta que T7.3 (adapter real) muestre qué información hay
  de verdad disponible para reportar.
- **T3.2**: el mock enforce `edit_hooks` sobre un flag `blocked: bool` que el propio
  fixture declara por efecto, no con glob-matching real — evita traer una dependencia
  de globs solo para el mock; el matching real de scope (T5.3) sí lo va a necesitar.
- **T5.1**: la regla 4 (scopes solapados sin dependencia) usa una heurística
  deliberadamente conservadora — compara solo el prefijo literal de cada glob (todo
  antes del primer `*`/`?`/`[`) en vez de álgebra de globs real. Puede marcar como
  solapados dos globs que en la práctica no chocarían (p. ej. `src/*.rs` vs.
  `src/sub/mod.rs`), pero nunca deja pasar un solapamiento real — para un check de
  seguridad, la falsa alarma es el lado correcto del error.
- **T5.2/T5.3**: `run_task()` corre pre-check una sola vez (no se repite en cada
  reintento — solo dispatch+post-check+scope se repiten, coherente con que el
  árbol ya tiene estado de intentos previos). El scope check usa `git diff --name-only
  HEAD` + `git ls-files --others --exclude-standard` (tracked modificados/borrados +
  nuevos sin trackear); requiere que `cwd` sea un repo git con al menos un commit
  (`HEAD` válido) — sin eso, `ScopeCheckError::GitFailed`, nunca un panic ni un
  falso "todo limpio".
- **T7.1 (check)**: sin flags de formato de salida (`--json`) ni de verbosidad —
  agregar cuando algo los necesite.

## Decisiones de diseño resueltas (con el usuario, 2026-08-18)

Las tres preguntas que estaban abiertas se cerraron con la misma directiva:
**nada custom — se incluyen las piezas normativas que faltaban**:

1. **El `loop` encuentra su ledger por el mecanismo normativo (Contrato §5)**:
   un nodo anterior lo produce como artifact `kind: task-ledger`
   (`artifacts.produces`); al cerrar ese nodo el engine lo parsea, valida
   (`yunta_engine::register`, T5.1) y registra cada tarea (`task_registered`).
   El `loop` consulta el estado derivado — nunca un path propio en el schema.
   **El bootstrap incluye el nodo de planificación real** (`kind: prompt` que
   produce `plan.yaml` como `task-ledger`), por decisión explícita del usuario
   que amplía el recorte original del Plan ("las tareas se convierten a ledger
   a mano") — nada custom ni transitorio; el flujo es el final. Requiere entrar
   al recorte: verificación de artifacts al cierre de nodo (§4) + registro de
   `task-ledger` + el mínimo de templates de T6.3 que el bootstrap necesita
   (`{{run.dir}}`). *(Divergencia con la prosa del Plan en Notion — corregirla
   allá es tuyo; Notion es de solo lectura para mí.)*
2. **El ruteo de fixtures del mock es territorio de `yunta test` (T7.9 / §14)**:
   cada caso en `.yunta/tests/` declara su `fixture:`. Se incluye un recorte
   mínimo de T7.9 en M-0. `yunta run --adapter mock` sin fixture disponible
   degrada con error explícito que apunta a `yunta test` — jamás emulación.
3. **T1.4 manifest**: se construyó la versión normativa recortada (workflow
   resuelto + config mergeada + contenido de prompts por archivo + commit base,
   con hashes canónicos SHA-256). `inputs`/`modo` esperan a que existan en el
   schema. Hecho — ver commit `3e44a45`.

## Pendiente explícito (para retomar sin adivinar)

1. **T7.3** — adapter `claude-code` real: probe (binario, versión, auth), spawn
   headless streaming, `resume`, `permission_profiles`, `edit_hooks`,
   `custom_agents`, skills. Requiere el binario `claude` instalado y autenticado en
   el entorno donde se corra — no ejercitable en este sandbox.
2. **Grupos de config diferidos de T1.2**: `mcp_servers`, `skills`,
   `baseline`/`coverage`, `secrets`, `permissions` (merge invertido, org manda).
   Implementar recién cuando algo los consuma: `baseline`/`coverage` con T5.4
   (`baseline_compare`), `permissions` con T5.7, `skills` con M6, `mcp_servers` con
   M8.
3. **Node kinds diferidos de T1.1**: `gate`, `check`, `parallel`, `executor`,
   `workflow` (composición). Cada uno llega con su milestone correspondiente (M5
   para `check` builtins y gates, M9 para composición) — no antes.
4. **Reglas de `yunta check` diferidas de T1.3**: coherencia de modos, scopes
   disjuntos en `parallel`/`inherit`, templates, profundidad/aciclicidad del grafo
   de workflows, techos de `permissions`, warning de push directo a la rama base.
5. **Event kinds sin consumidor en replay (T2.3)**: `gate_waiting`/`gate_resolved`,
   `questions_answered`, `finding_posted`, `child_run_*`, ampliación de scope,
   `promotion_signaled` — existen como tipos (T2.2) pero `derive()` los ignora
   porque nada los emite todavía. Sumarlos a `RunState` cuando su schema/ciclo
   llegue (gates: M5/T7.2; composición: M9; findings: T5.12).
6. **T1.5** — validación de inputs al crear el run. Depende de `inputs:` en el
   schema, que T1.1 no implementó (fuera del recorte). Diferir hasta que algo
   necesite inputs reales — probablemente cuando el workflow de bootstrap necesite
   parametrizarse.
7. **`event_hash` (T2.5)**: política ya definida en `docs/eventos.md` §3, sin
   implementar — explícitamente fuera de M-0.
8. **El arco que cierra M-0** (orden de ejecución, según las decisiones de
   diseño resueltas arriba):
   1. Artifacts al cierre de nodo (§4): verificación de existencia/no-vacuidad,
      `artifact_written` con hash, y registro de `kind: task-ledger` vía
      `yunta_engine::register` → `task_registered`.
   2. Mínimo de templates (recorte de T6.3): `{{run.dir}}` en nodos `bash` —
      solo lo que el workflow de bootstrap necesita; el resto de T6.3 espera M6.
   3. Scheduler recortado (T4.1): `pending→ready→running→done|failed`,
      `depends_on`, despacho por `kind` (`prompt`/`bash`/`loop`), hooks
      `before`/`after`, re-rutas `on_failure.goto`, secuencial
      (`max_parallel_nodes` diferido). Reutiliza `run_task`/`dispatch` de
      `task_cycle.rs`.
   4. Creación de run: run.dir + manifest congelado en disco + `run_created`
      con `manifest_hash`; eventos al storage durante la ejecución.
   5. CLI: `yunta run`, `status`, `resume` (T7.1 parcial + T4.5 recortado).
   6. `yunta test` mínimo (recorte de T7.9): casos en `.yunta/tests/` con
      `fixture:` + `expect:` sobre estado derivado — el hogar del ruteo de
      fixtures del mock. `run --adapter mock` sin fixture → error explícito.
