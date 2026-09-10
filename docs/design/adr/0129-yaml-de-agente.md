# D129 — Tercera categoría en la frontera: el YAML de agente

## Contexto

D110 parte el YAML del sistema en dos: el **de autor**, que rechaza claves
desconocidas, y el **persistido**, que las tolera. Entre lo que llama "de
autor" enumera "los ledgers y los artifacts de preguntas", y los describe como
"escrito por una persona".

No lo son. El pack de referencia `yunta/fragua` produce su ledger en un nodo
`kind: prompt` con `runner: planner`, y el esqueleto que `yunta new --shape
ledger` escribe tiene como prompt entero `"Write a task ledger to
{{run.dir}}/artifacts/ledger.yaml."`. `contrato-del-run.md` §5 dice lo mismo:
"un nodo de planificación lo declara". El escritor de un artifact interpretado
es un agente.

La diferencia no es cosmética. Un YAML de autor existe antes del run, así que
`yunta check` lo alcanza antes de gastar un token, y su lector es una persona
que puede abrir el archivo y corregirlo. Un YAML de agente nace a mitad del
run, ningún `check` lo ve nunca, y su primer lector es el agente que lo
escribió. Tratarlos igual deja una categoría con la mitad del contrato: el
rigor sin la gramática y sin la corrección.

Lo que se observa hoy es exactamente esa mitad. Un agente que escribe
`description:` dentro de una tarea, `criteria: [cargo test]` en vez de
`[{cmd: ...}]`, `severity: high` en un finding o `question:` en vez de `text:`
falla el nodo, y ninguna de esas formas es irrazonable para quien nunca vio el
schema. El engine no se lo mostró en ningún momento: el prompt que llega a la
sesión es el bloque de contexto más el prompt del autor, y nada más.

## Decisión

La frontera tiene tres categorías, no dos.

**YAML de autor** — config, workflows, `pack.yaml`, casos de test, fixtures del
mock. Rechaza claves desconocidas. `check` lo alcanza. Su lector es una
persona.

**YAML de agente** — el contenido de todo artifact interpretado (`task-ledger`,
`findings`, `questions`). Rechaza claves desconocidas igual, y suma las dos
obligaciones que la categoría trae consigo:

1. **La forma se publica a quien la escribe.** Un nodo que declara
   `artifacts.produces: [{kind: <k>}]` recibe la forma de `<k>` como fuente de
   contexto derivada de esa declaración — no una clave nueva que el autor deba
   recordar, sino la consecuencia directa de la que ya escribió. Se genera
   desde los mismos tipos que después la parsean, como el JSON Schema de D70,
   y entra en el segmento `stable` del contexto, con lo cual no toca el
   prefijo byte-estable que §9 exige para el cache del proveedor.
2. **Un archivo ilegible se corrige, no mata el nodo.** Ver D131.

**YAML persistido** — eventos, manifest, lock de packs. Lector tolerante, sin
cambios respecto de D70.

El registro de schemas de `yunta-core` cubre las tres kinds interpretadas:
`findings` y `questions` hoy no emiten schema y `ledger` sí, una asimetría sin
razón.

## Racional

Parsear es validar, y esa regla no se relaja: una clave mal escrita que el
tipo ignora en silencio sigue siendo un estado inválido entrando por la puerta
de atrás. Lo que cambia es de quién es la culpa cuando el archivo sale mal. A
un autor se le puede exigir que lea la referencia del schema antes de escribir
un workflow, porque escribe una vez y `check` lo corrige gratis. A un agente
se le está exigiendo adivinar una gramática que el engine tiene en tipos de
Rust y nunca le muestra, en una sola oportunidad, a mitad de un run que ya
gastó tokens.

Publicar la forma es además lo más barato del sistema: los tipos ya llevan
`schemars`, `cargo xtask schema` ya emite los archivos, y el mecanismo de
fuentes de contexto con clases de estabilidad ya existe y está probado. No hay
mecanismo nuevo, solo una fuente derivada de una declaración que ya se escribe.

## Descartado

**Relajar la validación de los artifacts de agente** (tolerar claves
desconocidas, aceptar `criteria` como lista de strings). Convierte cada
divergencia en un dato perdido en silencio: un `description:` ignorado es
información que el agente creyó registrar y nadie leyó, y una tarea sin
`criteria` legible es una tarea sin verificación. Es la puerta de atrás que
D110 cerró.

**Dejar la gramática en manos del autor del workflow**, documentando que el
prompt debe incluirla. Es lo que pasa hoy de facto, y el resultado es que el
esqueleto que el propio producto genera no funciona. Además duplica en cada
workflow del ecosistema una forma que el engine ya conoce, y la desincroniza
en el primer cambio de schema.

**Una clave nueva en el nodo** (`artifacts.publish_schema: true`). Es una
segunda declaración del mismo hecho: el nodo ya dijo `kind: task-ledger`.
