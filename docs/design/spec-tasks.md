# Spec — Schema del documento de tareas

**Estado:** normativo v0.1 · **Alcance:** schema formal del artifact `kind: tasks`,
sus reglas de validación y sus errores. Es el formato con el que se le da trabajo
al sistema, y una persona lo escribe a mano.

## 1. Estructura

Un documento de tareas es un documento YAML cuya única clave obligatoria de nivel
superior es `tasks:`:

```yaml
tasks:
  - id: graph-cmd
    title: "yunta graph emits the DAG as Mermaid"
    scope: ["crates/cli/src/graph.rs", "crates/cli/src/main.rs"]
    criteria:
      - cmd: "test -f crates/cli/src/graph.rs"
      - cmd: "cargo test -p yunta --test graph"
      - cmd: "cargo clippy --workspace -- -D warnings"
        type: guard
    notes: "Mermaid por default; aristas de re-ruta con estilo distinto."
```

Nada de referencias al brief, al modo o al run: todo ese contexto ya vive en el
manifest y en el event log. Lo que sí puede ir junto a `tasks:` es lo que lee la
persona que revisa el plan (§2.2): qué cambia y por qué, las formas que crea o
modifica, sus riesgos y lo que deja afuera. Es el mismo documento que el engine
ejecuta, así que lo que una persona aprueba es lo que corre (D195).

## 2. Campos

| Campo | Tipo | Obligatorio | Notas |
|---|---|---|---|
| `id` | string `^[A-Za-z][A-Za-z0-9_-]*$` | sí | el patrón es condición de lectura: lo verifica el tipo que lee el documento, así que un `id` que no lo cumple llega como problema de lectura y no como una de las reglas de §3, que exige la unicidad (`duplicate-id`). **Sin convención impuesta**: `T001` es convención, no regla — un id descriptivo (`graph-cmd`) sobrevive mejor a un re-plan que un número de orden. |
| `title` | string no vacío | sí | qué se hace, en una línea |
| `scope` | lista de globs, ≥1 | sí | qué puede tocar la tarea |
| `criteria` | lista de objetos, ≥1 | sí | ver la tabla de `criteria[]` más abajo |
| `depends_on` | lista de ids | no | default vacío |
| `notes` | string | no | contexto mínimo para un runner sin historial |
| `description` | Markdown | no; sí cuando un gate muestra el plan (§3.1) | qué hace la tarea y por qué, para quien revisa el plan; nombra las `shapes` que toca en vez de repetirlas. `yunta_task` se la devuelve a la sesión que la implementa |
| `changes` | lista de `{at, what}` | no; sí cuando un gate muestra el plan (§3.1) | cada lugar que la tarea cambia —un archivo, o un archivo y qué dentro de él (`src/theme.rs::Theme`)— y qué cambia ahí; cada uno dentro de su `scope` (`change-outside-scope`) |
| `outcome` | string | no; sí cuando un gate muestra el plan (§3.1) | qué va a observar una persona cuando la tarea esté hecha |
| `uses` | lista de nombres de `shapes` | no | las formas que la tarea usa sin construirlas; espera a la tarea dueña de cada una (`shape-used-before-its-owner`) |
| `invariants` | lista de strings | no | lo que el código que toca ya promete y la tarea mantiene |

### 2.1 `criteria[]`

| Campo | Tipo | Obligatorio | Notas |
|---|---|---|---|
| `cmd` | string no vacío | sí | comando ejecutable; exit 0 = pasa |
| `type` | enum `guard` | no | ausente = criterio normal (debe fallar en el pre-check); `guard` = línea de no-regresión (debe pasar antes y después) |
| `proves` | string | no; sí cuando un gate muestra el plan (§3.1) | qué muestra que pase, en palabras de quien revisa el plan |

Toda tarea necesita **al menos un criterio no-`guard`**: sin él no hay nada que pueda
estar en rojo antes del trabajo, y el pre-check pierde sentido.

La suite de no-regresión del proyecto no hace falta declararla: cuando el linaje la
midió en verde, el engine juzga cada tarea contra ella como `guard` (D196), sin
tocar el documento. Una tarea que la declara igual se juzga por su propia
declaración.

### 2.2 Para quien revisa el plan

| Campo | Tipo | Obligatorio | Notas |
|---|---|---|---|
| `summary` | string | no; sí cuando un gate muestra el plan (§3.1) | qué cambia el plan, en una línea |
| `description` | Markdown | no; sí cuando un gate muestra el plan (§3.1) | qué cambia, por qué y cómo se encara. Admite bloques de código —un ejemplo, cómo interactúan las piezas— y bloques `mermaid` para diagramas |
| `decisions` | lista de `{id, question, choice, alternatives?, why?}` | no; `why` sí cuando un gate muestra el plan (§3.1) | cada punto que el brief dejó abierto, cerrado acá y no por quien implementa: qué estaba abierto, qué se eligió, qué no, y por qué |
| `shapes` | lista de `{name, owner, file, code}` | no | cada forma que el plan crea o modifica —tipo, interfaz, schema, firma, formato— declarada una sola vez, entera, en el archivo donde vive, por la única tarea que la construye; el `scope` de esa tarea cubre el archivo (`shape-outside-owner-scope`) |
| `design` | Markdown | no | cómo encajan las partes, en prosa y ejemplos, alrededor de las `shapes` |
| `risks` | lista de strings | no | — |
| `out_of_scope` | lista de strings | no | — |

Al aceptar el documento, el engine escribe junto a su vista `tasks.yaml` una vista
Markdown, `tasks.md`, derivada de los mismos bytes: el resumen, la descripción y el
diseño tal cual, lo que el engine sabe del plan sin que se lo digan —cuántas tareas,
en qué orden, qué tocan, qué guards las sostienen, y un diagrama de dependencias— y
cada tarea con su descripción y una tabla de qué prueba cada criterio. Es lo que un
gate que muestra el plan señala para leerlo entero.

## 3. Validación al registrar

Quince reglas, cada una con su código estable: el mismo con el que el engine la
publica junto a la forma del documento, con el que la nombra el diagnóstico cuando
se rompe y con el que un reporte la cuenta. El engine rechaza el documento
completo — y falla el nodo que lo produjo — si alguna no se cumple:

1. `duplicate-id` — cada `id` se declara una sola vez en el documento.
2. `empty-title` — `title` dice qué hace la tarea, en una línea no vacía.
3. `empty-scope` — `scope` lista al menos un glob: los únicos paths que la tarea
   puede tocar.
4. `no-criteria` — toda tarea declara al menos un criterio.
5. `all-criteria-are-guards` — al menos un criterio no es `guard`, así que algo
   tiene que poder fallar antes del trabajo y pasar después.
6. `unknown-dependency` — `depends_on` nombra solo ids que este documento declara.
7. `dependency-cycle` — `depends_on` no forma ciclos. Los detecta el mismo
   recorrido que `check` corre sobre el grafo de nodos del workflow: un solo
   detector para los dos grafos, que reporta el ciclo como el camino que lo cierra.
8. `overlapping-scope` — dos tareas sin dependencia entre sí declaran scopes
   disjuntos; un solapamiento impide correrlas en paralelo y vuelve ambiguo el
   diff, así que o se separan los scopes o se declara la dependencia.
9. `duplicate-shape` — cada forma de `shapes` se declara una sola vez.
10. `duplicate-decision` — cada decisión tiene un `id` propio.
11. `unknown-shape-owner` — el `owner` de una forma es una tarea que este documento
    declara.
12. `shape-outside-owner-scope` — el `file` de una forma cae dentro del `scope` de
    su dueña: la tarea que construye una forma puede escribir su archivo.
13. `unknown-shape` — `uses` nombra solo formas que `shapes` declara.
14. `shape-used-before-its-owner` — una tarea que usa una forma de otra tarea
    espera a esa tarea, directamente o a través de otras.
15. `change-outside-scope` — cada lugar que una tarea dice cambiar cae dentro de
    su propio `scope`. Un plan que le pide a una tarea un cambio en un archivo que
    solo otra puede escribir se rechaza al entregarse, no cuando la tarea ya
    construyó otra cosa.

Estas reglas corren como parte de la lectura del documento, no como un paso aparte
que un llamador pueda saltear: quien obtiene un documento de tareas obtiene uno que las cumple.
Y se publican antes de que el documento se escriba: la lista que las aplica es la
misma que el contrato le entrega a la sesión, así que ninguna de las quince llega
por primera vez como un fallo (D143).

Lo que el engine **no** valida acá: que los comandos existan o sean correctos — eso
lo dice el pre-check en rojo al ejecutarlos, que es donde un criterio trivial o
roto se delata.

### 3.1 Cuando un gate muestra el plan

Si un gate que el run ejecuta —uno que el modo del run incluye, se llame como se
llame— muestra el documento (`shows:`), el engine también lo rechaza, al entregarlo
y al cerrar el nodo, si no cumple:

- `no-summary` — `summary` dice qué cambia el plan, en una línea no vacía.
- `no-description` — el plan y cada tarea llevan su `description`.
- `unexplained-criterion` — cada criterio dice qué `proves`.
- `no-outcome` — cada tarea dice qué se va a observar cuando esté hecha.
- `no-changes` — cada tarea dice qué cambia, lugar por lugar.
- `unexplained-decision` — cada decisión dice `why`.

`decisions`, `shapes`, `design`, `risks` y `out_of_scope` no se exigen: un plan
que solo toca documentación no crea formas ni tiene nada abierto, y uno puede no
tener riesgos. Los pide el prompt de quien planifica.

## 4. Errores

Cada rechazo nombra la tarea, el campo y la expectativa, en el vocabulario del
documento y nunca en el del parser:

```
artifacts/plan/tasks.yaml: 3 errors
  task `graph-cmd`: `scope` is empty; every task declares at least one glob, the only paths it may touch
  task `parse-events`: `depends_on` names `storage-init`, which no task in this file declares
  task `T004`: every criterion is a `guard`; at least one must be able to fail before the work, or there is nothing the work has to make pass
```

El encabezado nombra el archivo que se leyó y cuántos problemas tiene; debajo va una
línea por problema, con su sujeto adelante. Una tarea cuyo `id` es justamente lo que
no se pudo leer se nombra por su posición (`the first task`), nunca por un índice del
parser. Este bloque es el formato único con el que toda superficie muestra los
problemas de un documento —un documento de tareas, un artifact de findings, un workflow— y vive
en un solo lugar (D133).

Todos los problemas del documento se reportan juntos, no de a uno: quien lo escribió
corrige una vez, no siete veces.

## 5. Ejemplo: una tarea del propio plan de Yunta

```yaml
tasks:
  - id: context-sources
    title: "ContextSource trait with files, command and artifact builtins"
    scope: ["crates/engine/src/run/context_resolve/**"]
    criteria:
      - cmd: "cargo test -p yunta-engine --test run_context"
      - cmd: "! grep -rn 'todo!()' crates/engine/src/run/context_resolve/"
      - cmd: "cargo clippy --workspace -- -D warnings"
        type: guard
    notes: "Materializar el contenido efectivo en objects/<hash>; fuente caída = nodo failed."

  - id: context-assembly
    title: "Stable-first context assembly with per-segment hashes"
    depends_on: [context-sources]
    scope: ["crates/engine/src/run/context_resolve/**", "crates/core/src/events/node/**"]
    criteria:
      - cmd: "cargo test -p yunta-engine --test properties"
      - cmd: "cargo clippy --workspace -- -D warnings"
        type: guard
    notes: "Property test: dos rehidrataciones con distinto estado volátil comparten prefijo byte-idéntico."
```

## 6. Alcance y límites conocidos

El formato cubre bien las tareas con salida verificable por comando — la enorme
mayoría del plan de Yunta. Dos zonas donde no alcanza, reconocidas y sin
intento de forzarlas:

- **Tareas que exigen entorno externo** — los adapters reales necesitan
  un CLI instalado y autenticado; la distribución necesita cinco plataformas.
  No son verificables por un criterio local y se hacen a mano.
- **Tareas de juicio** — documentación, revisión de redacción. Lo que no cierra
  ningún comando no es una tarea: el autor del workflow lo pone detrás de un
  `gate`, donde una persona mira y decide.

Que lo no verificable por comando tenga una salida explícita es deliberado: sin
ella, esas tareas tentarían a inventar criterios falsos — exactamente los criterios
triviales que el pre-check en rojo existe para rechazar. La salida es el `gate`,
fuera del documento de tareas, y quien la cierra es una persona y no quien hizo
el trabajo.
