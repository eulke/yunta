# Spec — Schema del documento de tareas

**Estado:** normativo v0.1 · **Alcance:** schema formal del artifact `kind: tasks`,
sus reglas de validación y sus errores. Se escribe antes del código que lo parsea,
por la misma razón que la spec de payloads de eventos precede a los tipos de Rust
del event log — es el formato con el que se le da trabajo al sistema, y va a
escribirse a mano desde el primer día.

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
| `id` | string `^[A-Za-z][A-Za-z0-9_-]*$` | sí | único en el documento. **Sin patrón impuesto**: `T001` es convención, no regla — un id descriptivo (`graph-cmd`) sobrevive mejor a un re-plan que un número de orden. |
| `title` | string no vacío | sí | qué se hace, en una línea |
| `scope` | lista de globs, ≥1 | sí | qué puede tocar la tarea |
| `criteria` | lista de objetos, ≥1 | sí | ver la tabla de `criteria[]` más abajo |
| `depends_on` | lista de ids | no | default vacío |
| `notes` | string | no | contexto mínimo para un runner sin historial |
| `manual_review` | bool | no | requiere `justification` |
| `justification` | string no vacío | solo si `manual_review` | por qué no es verificable por comando |

### 2.1 `criteria[]`

| Campo | Tipo | Obligatorio | Notas |
|---|---|---|---|
| `cmd` | string no vacío | sí | comando ejecutable; exit 0 = pasa |
| `type` | enum `guard` | no | ausente = criterio normal (debe fallar en el pre-check); `guard` = línea de no-regresión (debe pasar antes y después) |

Toda tarea necesita **al menos un criterio no-`guard`**: sin él no hay nada que pueda
estar en rojo antes del trabajo, y el pre-check pierde sentido.

## 3. Validación al registrar

El engine rechaza el documento completo — y falla el nodo que lo produjo — si:

1. Un `id` se repite, o no cumple el patrón.
2. Un `depends_on` referencia un id inexistente.
3. El grafo de `depends_on` tiene ciclos — detectados por el mismo recorrido que
   `check` corre sobre el grafo de nodos del workflow: un solo detector para los dos
   grafos, que reporta el ciclo como el camino que lo cierra.
4. Dos tareas sin dependencia entre sí declaran scopes que se solapan (impediría
   correr esas tareas en paralelo y hace ambiguo el diff).
5. Una tarea no tiene criterios, o todos son `guard`.
6. `manual_review: true` sin `justification`.
7. Un campo obligatorio falta o está vacío.

Estas reglas corren como parte de la lectura del documento, no como un paso aparte
que un llamador pueda saltear: quien obtiene un documento de tareas obtiene uno que las cumple.
Y se publican antes de que el documento se escriba: la lista que las aplica es la
misma que el contrato le entrega a la sesión, así que ninguna de estas siete llega
por primera vez como un fallo (D143).

Lo que el engine **no** valida acá: que los comandos existan o sean correctos — eso
lo dice el pre-check en rojo al ejecutarlos, que es donde un criterio trivial o
roto se delata.

## 4. Errores

Cada rechazo nombra la tarea, el campo y la expectativa, en el vocabulario del
documento y nunca en el del parser:

```
artifacts/plan.yaml: 3 errors
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
    scope: ["crates/engine/src/context/**"]
    criteria:
      - cmd: "cargo test -p yunta-engine context::"
      - cmd: "! grep -rn 'todo!()' crates/engine/src/context/"
      - cmd: "cargo clippy --workspace -- -D warnings"
        type: guard
    notes: "Materializar en context/<hash>/; fuente caída = nodo failed."

  - id: context-assembly
    title: "Stable-first context assembly with per-segment hashes"
    depends_on: [context-sources]
    scope: ["crates/engine/src/context/**", "crates/core/src/events.rs"]
    criteria:
      - cmd: "cargo test -p yunta-engine --test context_stability"
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
- **Tareas de juicio** — documentación, revisión de redacción. Ahí `manual_review` es
  la salida honesta.

Que el schema tenga una salida explícita para lo no verificable es deliberado: sin
ella, esas tareas tentarían a inventar criterios falsos — exactamente los criterios
triviales que el pre-check en rojo existe para rechazar.
