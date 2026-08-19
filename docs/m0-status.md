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
      (`yunta_core::Manifest` + `yunta_engine::build_manifest`), incluye
      `base_branch`/`base_commit` y contenido de prompts por archivo; sin
      `inputs`/`modo`.
- [x] **Artifacts al cierre de nodo (§4/§4.1)** — existencia, no-vacuidad y hash
      (`artifact_written`); `kind: task-ledger` se parsea, valida (todas las
      violaciones juntas) y registra (`task_registered`)
      (`yunta_engine::close_artifacts`).
- [x] **Templates (recorte de T6.3)** — sintaxis final `{{var}}` con error duro en
      variable indefinida (`render_template`/`template_variables`); M-0 provee
      `{{run.dir}}` (+ `{{worktree}}` en fixtures de `yunta test`).
- [x] **T4.1/T4.5 (recorte)** — `create_run` (run.dir + `manifest.yaml` +
      `run_created`) y `execute_run`: scheduler secuencial por replay
      (`pending→ready→running→done|failed`), despacho por kind
      (`bash`/`prompt`/`loop`), hooks `before`/`after` (`hook_executed`),
      re-rutas §11.2 (`node_rerouted`, retorno automático, `run_paused` al
      agotarse), commit por tarea verificada (§5.5), huérfanos `running`
      reinician al resumir (`run_resumed`, restart_node). `yunta run` y
      `yunta resume` son la misma función sobre el log.
- [x] **T7.1 (parcial)** — `yunta check`, `yunta run <workflow>`,
      `yunta status <run_id>`, `yunta resume <run_id>` reales en el CLI, con
      config en capas (repo > usuario > org, `YUNTA_HOME` como raíz de estado).
      `real_adapters()` construye `claude-code` cuando `runners:` lo nombra en
      la config mergeada; un workflow con `prompt`/`loop` que no resuelve a
      ningún adapter construido se rechaza **antes** de crear el run.
- [x] **T7.9 (recorte)** — `yunta test`: casos en `.yunta/tests/` con
      `workflow`/`fixture`/`expect` (final_state, nodes, tasks), sandbox por caso
      (worktree git, runs root y DB temporales), fixture renderizado con
      `{{run.dir}}`/`{{worktree}}`, el mock suplanta a todo adapter que la config
      nombre. Sin `mode`/`inputs`/`events`/`never` (esperan su schema o T7.9
      completo).
- [x] **T7.3 — adapter `claude-code` real.** Resultó sí ejercitable: el binario
      `claude` está instalado y autenticado en este sandbox (comparte
      infraestructura de sesión con el propio entorno de trabajo — ver nota de
      costo/seguridad abajo). Implementado en `yunta-adapters::claude_code`:
      - `probe()`: `claude --version`.
      - `spawn()`/`resume()`: `claude -p --output-format stream-json --verbose
        [--resume <id>] [--model] [--agent] <permission-args> <prompt>`.
      - Parser (`parse.rs`) puro: `system/init` → `SessionOpened`; bloques
        `text`/`tool_use` de mensajes `assistant` → `Note`/`ToolUse` (digest =
        `file_path`/`path`/`command`/`pattern`/`url` del input, o su hash);
        `thinking` deliberadamente no se expone. El `usage` de la línea
        `result` final (nunca el de líneas `assistant` intermedias, que
        subcuentan tokens de thinking aún no conciliados) es la única fuente
        de `Usage` — un total correcto importa más que poder cortar la sesión
        a mitad de camino, algo que este modo de ejecución por turnos atómicos
        no soporta bien de todas formas.
      - `capabilities()`: `resume_session`/`permission_profiles`/
        `custom_agents`/`usage_reporting` en `true`; `edit_hooks`/`run_tools`
        en `false` — honesto (A6): no hay bloqueo de ediciones en vivo para el
        CLI real todavía, el scope check post-hoc (T5.3) es el límite real.
      - `interrupt()`/`kill()`: `process_group(0)` al spawnear + `kill -SIGNAL
        -- -<pgid>` (A4, exterminio del árbol completo).
      - Mapeo de permisos (`permissions.rs`): las tres opciones obvias se
        probaron en vivo antes de elegir. `bypassPermissions`/
        `--dangerously-skip-permissions` — rechazadas por el CLI corriendo
        como root (contenedor). `dontAsk` — corre sin colgarse pero **deniega
        toda tool call** en vez de permitirla. `acceptEdits` — confirmada en
        vivo: permite Write y Bash sin prompt, corre como root, produce el
        archivo pedido. Es lo que usan los tres `PermissionProfile`;
        `ReadOnly` además restringe `--tools` a un set no-mutante.
      - Tests: 11 en `crates/adapters/tests/claude_code.rs` contra un binario
        `claude` simulado por script (`fixtures/claude_code_stub.sh`) — sin
        red, sin costo, nunca un LLM real en CI (A8). Cubren sesión exitosa,
        fallo con `retryable`, mapeo de `tool_use`, crash sin evento terminal,
        los tres perfiles de permiso, `--resume`, y **exterminio real del
        árbol de procesos** (el stub genera un nieto que `kill()` debe matar
        también). 1 test end-to-end en `crates/cli/tests/run_flow.rs` prueba
        que `yunta run` real dispara el adapter (mismo stub, vía
        `adapters.claude-code.binary` en config).
      - **Smoke test manual con el binario real** (el criterio de aceptación
        de T7.3): workflow de 3 nodos (`bash` → `prompt` con `claude-haiku-4-5`
        pidiendo crear `marker.txt` → `bash` verificando el archivo con
        `grep`) corrido con `yunta run` de verdad. Resultado: **run
        finished**, `marker.txt` creado con el contenido correcto, 18
        tokens de entrada / 262 de salida atribuidos en `status`. El primer
        intento con `--dangerously-skip-permissions` fue el que reveló el
        bloqueo por root; el segundo con `dontAsk` reveló la denegación
        silenciosa; el tercero con `acceptEdits` fue el que funcionó — los
        tres quedan documentados en el módulo como el rationale del mapeo.
      - **Nota de costo/seguridad**: el binario `claude` en este sandbox
        comparte `session_id` e infraestructura con la sesión de trabajo
        actual (no es una instalación aislada con su propia cuota) y cada
        invocación real cuesta dinero de la cuenta de Anthropic — confirmado
        con el usuario antes de gastar (~5 llamadas de prueba, entre
        US$0.004 y US$0.06 cada una, para diagnosticar el mapeo de permisos y
        el bug de `kill`).
      - **Bug real encontrado y corregido en el camino**: `kill -SIGNAL
        -<pgid>` (sin `--`) hace que `procps-ng`'s `kill` no envíe ninguna
        señal — sale con status 0 igual, sin tocar ningún proceso. Sin el
        test que verifica que un nieto del proceso muere de verdad, esto
        habría sido una violación silenciosa de A4 en producción. El fix
        (`kill <signal> -- -<pgid>`) y el test que lo atrapa quedaron en el
        mismo commit.

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

1. **T7.3 — hecho.** Ver el detalle en "Alcance mínimo de M-0" arriba. `skills`
   queda deliberadamente fuera (M6, `context:`/`skills:` no existen en el
   recorte de T1.1) — no es una falla, es scope.
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
8. **El arco que cerraba M-0 está COMPLETO** (los seis pasos: artifacts al
   cierre, templates mínimos, scheduler T4.1, creación de run, CLI
   run/status/resume, `yunta test`). Ver la lista de alcance arriba. Deudas
   menores que dejó, con su gatillo:
   - **T4.2 (worktrees)**: el run corre en el árbol actual del repo; aislamiento
     `worktree|none` con sus condiciones (§7.3) llega con T4.2. Gatillo: correr
     dos runs a la vez o el primer bootstrap real.
   - **T2.4 (paths congelados)**: `resume` busca run.dir bajo el `paths.runs`
     de la config *actual* — cambiarlo entre run y resume no está soportado
     hasta T2.4.
   - **Eventos `agent_session_opened`/`agent_message` sin emitir**: el ciclo
     nodo/tarea ya es auditable (runner_resolved, criteria_checked,
     scope_checked, artifact_written); emitir el detalle por sesión requiere un
     emitter dentro de `dispatch_session`. Gatillo: T7.3 o cuando `status`
     necesite mostrar la sesión viva.
   - **`node-output` como artifact (§11.2)**: el stderr de un `bash` fallido va
     hoy en el diagnóstico de `node_failed` (acotado a 20 líneas), no como
     artifact montable por `context:` — eso es de T4.4/M6.
   - **Subgrafo de corrección**: la re-ruta ejecuta solo el nodo destino; "su
     subgrafo" completo llega con T4.4.
   - **`run_paused` por límites de presupuesto de run (§8.3)**: los budgets por
     sesión (T3.3) están; `limits.max_tokens_per_run`/`max_loop_iterations`
     esperan a que `limits:` entre a la config (fuera del recorte de T1.2).
