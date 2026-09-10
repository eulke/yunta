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

1. **La forma se publica donde sea que alguien escriba**, sin que un humano
   tenga que transmitirla. Ver abajo.
2. **Un archivo ilegible se corrige, no mata el nodo.** Ver D131.

**YAML persistido** — eventos, manifest, lock de packs. Lector tolerante, sin
cambios respecto de D70.

### Dónde se publica la forma

Un agente escribe un documento de Yunta en cuatro momentos, y ninguno puede
depender de que una persona le haya explicado el formato antes.

**Dentro de un run.** Un nodo que declara `artifacts.produces: [{kind: <k>}]`
recibe la forma de `<k>` como fuente de contexto derivada de esa declaración:
no una clave nueva que el autor deba recordar, sino la consecuencia directa de
la que ya escribió. Entra en el segmento `stable`, con lo cual no toca el
prefijo byte-estable que §9 exige para el cache del proveedor.

**Fuera de un run, por MCP.** El plano de control gana la tool
`document_shape`, cuyo enum de `kind` enumera todos los documentos que Yunta
lee. Un cliente MCP ve esa tool y su enum al conectarse, sin llamar a nada y
sin que nadie se lo cuente: la lista de tools es la superficie que se anuncia
sola, y es por eso la puerta principal para un agente que trabaja en el repo
sin entrar a ningún run.

**Fuera de un run, por shell.** `yunta schema <kind>` imprime la forma;
`yunta schema` sin argumentos lista los kinds; `--json` emite el JSON Schema
para un editor. Es la misma puerta para una persona que quiere escribir el
documento a mano, que hoy no tiene ninguna: los archivos de `schemas/` los
genera `cargo xtask`, una herramienta de desarrollo del repo, no algo que
alcance a quien instaló el binario.

**Después de haber escrito mal.** El diagnóstico de D130 redactado para un
agente incluye la forma. Así, quien no encontró ninguna de las puertas
anteriores converge igual en el intento siguiente.

Las cuatro rinden desde una única fuente: el esqueleto derivado de los tipos,
más un ejemplo válido declarado al lado de cada tipo, con un test por kind que
lee ese ejemplo y exige cero diagnósticos. Cuatro textos escritos por separado
se desincronizan en el primer cambio de schema; uno con cuatro consumidores, no.

El registro de schemas de `yunta-core` cubre las tres kinds interpretadas:
`findings` y `questions` hoy no emiten schema y `ledger` sí, una asimetría sin
razón.

Lo que **no** se hace: escribir en `.claude/` ni en `CLAUDE.md` para que un
agente cliente encuentre la forma. `init` ya trata ese archivo como ajeno,
sugiere una línea y nunca la escribe, y la skill que instala vive en
`.yunta/skills/`, que el engine monta para sus propias sesiones y ningún
cliente externo lee. Por eso el peso de la puerta automática lo lleva MCP.

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

**Publicar las formas como MCP resources en vez de una tool.** Semánticamente
es lo correcto — son documentos, no acciones — pero el soporte de resources
entre clientes es desparejo, y una forma que el cliente no lista es una puerta
cerrada. La tool se ve en todos. Vuelve a discutirse cuando resources sea
universal.

**Publicar el JSON Schema en vez de un ejemplo comentado.** El schema ya
existe y se le podría pasar tal cual a un agente. Un modelo copia una forma
mucho mejor de lo que la deriva de una gramática, y el schema pesa varias
veces más en tokens dentro de un bloque que viaja en cada sesión del nodo. El
schema sigue siendo la salida de `--json`, para editores y herramientas.
