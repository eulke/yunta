---
number: D175
title: "Una ubicación es un hecho con raíz, y las sesiones las acota el intento"
status: accepted
revises: []
revised_by: []
---

# D175 — Una ubicación es un hecho con raíz, y las sesiones las acota el intento

## Contexto

Dos contradicciones del plan de raíz que los ítems 3-08 y 4-02 cruzaron
(README §11 L-42, L-45, L-48; decisión P11).

**La ubicación de un finding.** M12 tipa `FindingEntry.location` como
`Location { path: RelativePath, range }` —el "path y rango opcional" del
Contrato §4.1— para el documento que un agente escribe. Los findings del
engine (M10: toda degradación es un `engine_finding`) ubican cosas que no
están en el trabajo: el registro de procesos (`exec.rs:228`), el store de
objetos (`exec.rs:272`), el worktree entero (`steps.rs:83,96`), y hoy lo
hacen con paths absolutos del host; la denegación de scope
(`escalate.rs:110,296`) ubica en una lista de globs. `inherited_findings`
deriva todos a un documento que el sucesor hereda y lee por la puerta:
con `Location` en la puerta, ese documento no se lee. Y un path absoluto
en un artifact que otra máquina hereda no es un hecho: es el accidente de
este host.

**El dueño de las sesiones.** M04 lista `NodeRecord.sessions:
Vec<OpenSession>` y a la vez `SessionLedger { sesiones por (node,
attempt) }` en `RunState`; README §7 manda el dominio `session` a
`SessionLedger`; `cerco.md` §3 lo hace dueño de la cobertura y los
rechazos. 2-03 construyó la mitad de `NodeLedger` sin levantar la otra, y
3-08 (L-42) plegó cobertura y rechazos sobre esa mitad.

## Decisión

1. **Una ubicación tiene raíz.** `Location { root: LocationRoot, path:
   RelativePath, range: Option<LineRange> }`, con `LocationRoot { Work,
   Run }`: el trabajo (`{{worktree}}`) o el registro del run
   (`{{run.dir}}`), las dos cosas que M12 ya nombra en `TemplateVar`.
   `RelativePath` es no vacío, no absoluto y sin `..`. Nunca un path
   absoluto: una ubicación es un hecho portable o no es una ubicación.
2. **Una sola forma escrita, un solo parser.** Un agente escribe
   `src/lib.rs:10-14` (raíz `Work`, sin prefijo: la única raíz que un
   finding de agente tiene). El engine escribe `run:scratch/engine.json`.
   El prefijo es un conjunto cerrado que `compatibility.md` publica; el
   rango se lee con el último `:` sólo cuando lo que sigue es un rango.
3. **En los dos lados.** `FindingEntry.location` y
   `events::Finding.location` llevan `Location`. La tolerancia de lo
   persistido (M02, M14) es sobre kinds y campos desconocidos, no sobre
   debilitar cada valor a `String`; M06 ya pone `PauseReason`,
   `ToolTarget` y `Coverage` tipados en los payloads, y con cero tags
   publicados (D141) el wire se reemplaza en el lugar. El string en el
   wire no cambia, así que la clave de dedup de findings tampoco.
   `From<events::Finding> for FindingEntry` sigue siendo total.
4. **El engine pasa por la misma puerta.** `RunCtx::engine_finding` toma
   `Location`, así el compilador impide que el engine escriba lo que la
   puerta rechaza. Cada finding del engine ubica lo que es: el registro
   → `run:scratch/engine.json`, el store → `run:objects`, un artifact
   declarado y no producido → `run:artifacts/<nodo>/<nombre>` (nombres
   de `run_dir::*`, M13), una denegación de scope y un breach del cerco →
   `Work` en el primer path pedido o cruzado (la lista entera va en
   `detail`), el distill → `Work` en `DISTILLED_DIR`, y el cleanup del
   worktree → `Work` en `.`: el trabajo entero, que es de lo que habla.
5. **`EmptyLocation` se borra.** Un `Location` no puede estar vacío; lo
   que la puerta rechaza es `parse` en `findings[i].location`, como
   `tasks[i].id` hoy (`compatibility.md` §problem). La cadena de
   `vocabulary.rs` no admite una regla que ningún documento pueda romper.
6. **`NodeLedger` es el dueño de las sesiones.** La identidad de una
   sesión es `(nodo, intento, id)` y su vida la acota el intento: no hay
   evento de cierre de sesión. `SessionLedger` y `RunState.sessions` se
   retiran del plan; el dominio `session` se pliega en `NodeLedger`
   (sesiones, cobertura, rechazos) y `DegradationLedger`. Si
   `NodeRecord` crece, la partición es por registro (`SessionRecord` con
   su `apply_session`), nunca por dominio.

## Racional

Hechos tipados, prosa en el borde (M06): la raíz de una ubicación es un
hecho que el recibo y `status` leen —"tres findings sobre el trabajo, uno
sobre el run"— en lugar de una convención sobre si el string empieza con
`/`. Un lugar (M13): las raíces son las dos que `TemplateVar` ya nombra y
los nombres bajo el run dir son los de `run_dir::*`. Replay: un artifact
heredado en otra máquina sigue diciendo lo mismo. Sesiones: un ledger que
sólo respondería "toda sesión abierta del run" es una iteración sobre los
registros de nodo, y su línea en M04 era el resto del mapeo uno a uno
dominio→ledger de §7, no un diseño.

## Alternativas descartadas

- **`Location.path` admite paths absolutos.** Resuelve el engine con una
  línea y deja la raíz como convención; un artifact heredado lleva el
  path de otra máquina.
- **`events::Finding.location: String` tolerante y `FindingEntry`
  tipado.** Vuelve falible la derivación del documento
  (`derive_findings -> Result`) para un caso que las dos puertas tipadas
  hacen imposible, y contradice cómo M06 tipa los demás payloads.
- **Los findings del engine no llevan `location`
  (`Option<Location>`).** Vacía la regla del Contrato §4.1 para la mitad
  de los findings, y sí tienen lugar: lo que el engine no pudo hacer
  está en alguna parte del run o del trabajo.
- **Construir `SessionLedger` y reabrir 2-03.** Duplica la contabilidad
  del intento que `NodeRecord` ya lleva, para responder una pregunta
  que una iteración responde.
