# Spec — Schema del documento de tareas

**Estado:** normativo v0.1 · **Alcance:** schema formal del artifact `kind: tasks`,
sus reglas de validación y sus errores. Es el formato con el que se le da trabajo
al sistema, y una persona lo escribe a mano.

## 1. Estructura

Un documento de tareas es un documento YAML con una única clave de nivel superior:

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

Sin metadatos de cabecera: nada de referencias al brief, al modo o al run. Cuanto más
chico el schema, menos hay que validar — y todo ese contexto ya vive en el manifest y
en el event log.

## 2. Campos

| Campo | Tipo | Obligatorio | Notas |
|---|---|---|---|
| `id` | string `^[A-Za-z][A-Za-z0-9_-]*$` | sí | el patrón es condición de lectura: lo verifica el tipo que lee el documento, así que un `id` que no lo cumple llega como problema de lectura y no como una de las reglas de §3, que exige la unicidad (`duplicate-id`). **Sin convención impuesta**: `T001` es convención, no regla — un id descriptivo (`graph-cmd`) sobrevive mejor a un re-plan que un número de orden. |
| `title` | string no vacío | sí | qué se hace, en una línea |
| `scope` | lista de globs, ≥1 | sí | qué puede tocar la tarea |
| `criteria` | lista de objetos, ≥1 | sí | ver la tabla de `criteria[]` más abajo |
| `depends_on` | lista de ids | no | default vacío |
| `notes` | string | no | contexto mínimo para un runner sin historial |

### 2.1 `criteria[]`

| Campo | Tipo | Obligatorio | Notas |
|---|---|---|---|
| `cmd` | string no vacío | sí | comando ejecutable; exit 0 = pasa |
| `type` | enum `guard` | no | ausente = criterio normal (debe fallar en el pre-check); `guard` = línea de no-regresión (debe pasar antes y después) |

Toda tarea necesita **al menos un criterio no-`guard`**: sin él no hay nada que pueda
estar en rojo antes del trabajo, y el pre-check pierde sentido.

## 3. Validación al registrar

Ocho reglas, cada una con su código estable: el mismo con el que el engine la
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

Estas reglas corren como parte de la lectura del documento, no como un paso aparte
que un llamador pueda saltear: quien obtiene un documento de tareas obtiene uno que las cumple.
Y se publican antes de que el documento se escriba: la lista que las aplica es la
misma que el contrato le entrega a la sesión, así que ninguna de las ocho llega
por primera vez como un fallo (D143).

Lo que el engine **no** valida acá: que los comandos existan o sean correctos — eso
lo dice el pre-check en rojo al ejecutarlos, que es donde un criterio trivial o
roto se delata.

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
