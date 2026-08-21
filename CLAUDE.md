# CLAUDE.md — Yunta

Yunta es un workflow engine determinista en Rust para agentes de código: los flujos se
definen en YAML y el engine los ejecuta con verificación mecánica, estado externo y
auditoría total. Vos estás construyendo el engine — y las reglas que el engine le
impone a sus agentes también te aplican a vos como implementador.

## Fuentes de verdad (en este orden)

1. `docs/contrato-del-run.md` — documento normativo central del comportamiento del
   engine, con sus invariantes.
2. `docs/spec-adapter.md` — trait `Adapter`/`AgentSession`, capacidades y
   obligaciones de un adapter.
3. `internal/spec-ledger.md` — schema formal del ledger de tareas; se escribe a mano,
   así que precede leerla antes de tocar el ciclo de tareas.
4. `docs/adrs.md` — decisiones con racionales y alternativas descartadas. **Fuente de
   desempate: ante cualquier ambigüedad, buscá acá antes de decidir.**
5. `docs/rfc-0001-vision.md`, `docs/rfc-0002-packs.md`, `docs/rfc-0003-producto.md` y
   `docs/rfc-0004-distribucion.md` — arquitectura, packs, diferenciales de producto y
   distribución.
6. `docs/referencia-schema.md` — config y workflows canónicos (los fixtures de parseo
   salen de acá).
7. `docs/deuda.md` — lo deliberadamente NO resuelto.

La documentación canónica vive en Notion (página "Yunta", BD Docs); `docs/` es su
espejo en el repo. Si encontrás una contradicción entre código y docs, la doc gana y el
código se corrige — salvo que un ADR diga lo contrario.

## Cómo pensar (juicios que este proyecto exige)

- **Tu palabra no es evidencia.** El principio rector del engine te aplica: nada está
  "hecho" porque lo digas — está hecho cuando el criterio de aceptación de la tarea
  pasa en verde y lo verificaste ejecutándolo. Nunca reportes completado sin correr
  los tests.
- **Test-first en rojo.** Antes de implementar, escribí o identificá el test que hoy
  falla. Si el test ya pasa antes de tu cambio, no prueba nada: repensá el criterio.
- **Ante ambigüedad: ADRs → preguntar. Jamás inventar.** Si los docs no resuelven algo,
  no lo resuelvas vos silenciosamente: es un ítem para `docs/deuda.md` o una pregunta
  al humano. La deuda consciente NUNCA se resuelve implícitamente — cada ítem requiere
  decisión explícita registrada como ADR antes de codearse.
- **Degradación explícita, nunca emulación.** Si algo no se puede hacer (capacidad
  ausente, límite excedido, fuente caída), el sistema lo dice con evento y diagnóstico.
  Nunca simules la capacidad, nunca degrades en silencio, nunca tragues un error. Este
  juicio aplica al engine que construís y a cómo trabajás.
- **Scope chico y declarado.** Cada tarea toca lo suyo. Si encontrás un problema fuera
  de tu alcance, registralo (issue/nota), no lo parchees al pasar. Si tu cambio
  "necesita" tocar medio repo, la tarea está mal cortada: frená y replanteá.
- **Las specs no se "mejoran" al pasar.** Si implementando ves algo mejorable del
  diseño, proponélo como cambio de spec (ADR nuevo o modificación con racional) — no lo
  implementés distinto de lo escrito y lo dejés como sorpresa.

## Terminología obligatoria

| Usá | Nunca | Por qué |
|---|---|---|
| adapter | driver, backend | integración con un CLI |
| runner | agente (para bindings) | binding \{adapter, model, agent?\} |
| agente | subagente, persona | agente nombrado DEL adapter |
| `runner:` | `role:` | `role:` no existe en el schema |
| pack | plugin | "plugin" no existe en el vocabulario |
| executor | plugin | extensión de código del engine |

"Rol" solo como palabra descriptiva en prosa/docs, jamás como clave YAML.

## Reglas de código

- **Estructura**: workspace de Cargo — `crates/{core,storage,adapters,engine,cli}`,
  packages `yunta-core`, `yunta-storage`, `yunta-adapters`, `yunta-engine` y `yunta`
  (el binario). Dependencias unidireccionales: core ← storage/adapters ← engine ←
  cli; jamás ciclos, jamás dependencias hacia arriba. **Si te encontrás queriendo
  agregar una dependencia que rompe ese orden, el diseño de lo que estás escribiendo
  está mal, no el layout.** `yunta-engine` no depende de rusqlite/sqlx ni de ningún
  CLI concreto — lo impone el compilador, no el checklist. Sin features opcionales:
  no hay `serve` (proyecto separado) ni `postgres`.
- **Calidad**: `cargo clippy --workspace -- -D warnings` limpio, `cargo fmt` aplicado,
  errores con `thiserror` (nada de `unwrap()`/`expect()` fuera de tests),
  observabilidad con `tracing`. `#![forbid(unsafe_code)]` en todos los crates.
- **Tests**: cada crate tiene sus tests de integración en su propio `tests/`,
  ejercitando el engine con el adapter `mock` — nunca un LLM real en CI. Todo
  camino del engine debe ser ejercitable con mock; si no podés testear algo sin un
  LLM, el diseño de ese algo está mal.
- **Eventos**: todo evento y payload lleva `schema_version`. El event log es
  append-only; el estado se deriva por replay — si te encontrás guardando estado
  derivado como fuente de verdad, pará.
- **Secretos**: jamás en el event log, en eventos de adapter ni en fixtures. Solo
  env vars declaradas.

## Checklist de PR (definition of done)

1. El criterio de aceptación de la tarea pasa, ejecutado, no supuesto.
2. Tests verdes en CI con mock; clippy sin warnings; fmt aplicado.
3. Ningún invariante del Contrato del Run ni obligación del Adapter violado —
   repasá la lista completa en la fuente de verdad correspondiente, están para eso.
4. Terminología de la tabla respetada en código, docs y mensajes de commit.
5. Documentación de módulo actualizada si cambió comportamiento público.
6. Redacción nativa: sin referencias históricas, cada mecanismo justificado desde
   primeros principios.

## Comandos

```bash
cargo test --workspace                 # suite completa (usa adapter mock)
cargo clippy --workspace -- -D warnings   # obligatorio antes de commit
cargo fmt --all
cargo run -p yunta -- check <workflow>    # validación estática
cargo run -p yunta -- run <wf> --adapter mock   # correr un workflow sin LLM
```

## Qué NO hacer (resumen de trampas conocidas)

- No introducir `role:` ni "plugin" en schema, código o docs.
- No emular capacidades ausentes de un adapter — error en check o degradación con
  evento.
- No darle a ningún agente (ni al mock) una vía para marcar estado de tareas.
- No mutar artifacts ni manifests — el progreso son eventos.
- No resolver ítems de `docs/deuda.md` de facto.
- No agregar conocimiento específico de un CLI fuera de `yunta-adapters` — si
  `yunta-engine` necesita saber qué adapter tiene enfrente, falta una capacidad, no
  un branch.
- No dejar procesos huérfanos: todo camino de cancelación extermina el árbol
  completo.

## Diseño y patrones del codebase

- **Parse, don't validate.** Los estados inválidos deben ser irrepresentables por
  tipo, no rechazados por chequeos dispersos. Newtypes para todo identificador
  (`RunId`, `NodeId`, `TaskId`, `SessionId` — nunca `String` pelada); enums
  exhaustivos sin variante `Other`; el schema YAML se parsea a tipos de dominio una
  sola vez en la frontera y de ahí en más el código opera sobre tipos que ya no
  pueden estar mal.
- **Functional core, imperative shell.** La derivación de estado por replay
  (`fn derive(events: &[Event]) -> RunState`) es una función pura sin IO — es lo que
  hace el replay determinista y testeable por property tests. Lo mismo para
  decisiones del scheduler (`fn ready_nodes(state) -> Vec<NodeId>`): decidir es puro,
  ejecutar es la cáscara con tokio. Si una función de decisión necesita IO, está mal
  cortada.
- **Determinismo inyectado.** Reloj (`Clock` trait), generación de IDs y cualquier
  aleatoriedad se inyectan — jamás `SystemTime::now()` o entropía directa en el core.
  Es la diferencia entre tests reproducibles y tests flaky, y en un sistema
  event-sourced es innegociable.
- **Máquinas de estado como enums, no booleanos.** El estado de un nodo es
  `enum NodeState { Pending, Ready, Running, ... }` con transiciones como métodos que
  devuelven `Result` — nunca tres flags booleanos cuya combinación inválida nadie
  previó.
- **Traits solo en fronteras reales.** `Adapter`, `AgentSession`, `ContextSource`,
  `HumanInteraction`, `Clock`, storage. No abstraer "por las dudas": una abstracción
  sin segunda implementación real (o mock con propósito) es costo sin beneficio. La
  señal de que falta un trait es un `if adapter.id() == "claude-code"` en el engine —
  eso viola la frontera adapter/engine y se corrige con capacidad, no con branch.
- **Concurrencia estructurada.** Toda task de tokio tiene dueño (el scheduler retiene
  los `JoinHandle`); nada se spawnea y se olvida. Cancelación por `CancellationToken`
  propagado, y el camino de cancelación se testea con la misma seriedad que el camino
  feliz — la limpieza completa del árbol de procesos ante una cancelación depende
  de esto.
- **Errores tipados en la lib, contexto en el borde.** `thiserror` con enums por
  módulo; el `main` traduce a mensajes accionables. Un error debe decir qué hacer:
  "workflow `x` referencia el rol `planner` que ninguna capa de config define —
  agregalo a `runners:`" y no "invalid config".

## Higiene

- **Idioma: inglés en TODA superficie que toque al usuario.** Identificadores,
  comentarios, rustdoc, commits — y además README, documentación de uso, guías,
  textos de ayuda del CLI, mensajes de error, salida de `status`/`stats`/`--follow`/
  `graph`. El ecosistema al que Yunta aspira (packs compartidos, adapters
  de terceros) lo exige y los términos del glosario mapean 1:1: `Runner`, `Adapter`,
  `Pack`, `Ledger`, `Scope`. Única excepción: los documentos de diseño internos
  (corpus en Notion), que son del equipo y permanecen en español. Si escribís algo
  que un usuario de la herramienta va a leer, es en inglés — sin excepciones ad hoc.
- **`#![forbid(unsafe_code)]`** en todos los crates. No hay nada en Yunta que lo
  justifique; si algún día lo hay, es un ADR.
- **Dependencias con justificación, y en el crate correcto.** Cada crate nuevo se
  defiende en el PR (¿qué costo de compilación/binario/superficie de audit trae?,
  ¿alcanza std o algo ya presente?) y se agrega al crate que realmente lo necesita —
  nunca al workspace entero ni a `yunta-core` "para tenerlo a mano". `cargo-deny` en
  CI para licencias y duplicados. El binario estático chico es una feature del
  producto — cada dependencia la erosiona.
- **Tests que documentan.** Nombres que describen comportamiento
  (`resume_after_crash_mid_node_reaches_same_final_state`), fixtures golden en
  `tests/fixtures/` (los YAML de referencia de la doc SON fixtures), property tests
  para replay e idempotencia. Un bug corregido = un test que lo hubiera atrapado,
  siempre.
- **Sin código en suspenso.** Ni TODOs sin issue vinculado, ni código comentado, ni
  `#[allow(dead_code)]` "temporal". Lo que no se usa se borra — git lo recuerda.
- **Observabilidad desde el día uno.** `tracing` con spans por run y por nodo
  (run_id/node_id como campos estructurados); nunca `println!` fuera del CLI. Los
  tests pueden asertar sobre spans cuando el comportamiento observable es el
  contrato.
- **Archivos y funciones con techo blando.** Un archivo que pasa ~500 líneas o una
  función que pasa ~50 es una señal para repensar el corte, no una regla mecánica —
  pero la señal se atiende en el PR, no "después".
- **Commits atómicos con mensaje convencional** (`feat:`, `fix:`, `test:`, `docs:`,
  `refactor:`); un tema por commit; el cuerpo explica el porqué cuando no es obvio.
  El historial es documentación.
- **CI compila el workspace completo** y cada crate de forma aislada
  (`cargo check -p <crate>`) — un crate que no compila solo tiene una dependencia mal
  ubicada.
