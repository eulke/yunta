# Deuda consciente y cuestiones abiertas

Lo deliberadamente no resuelto. Regla: **nada de esta lista se resuelve
implícitamente durante la implementación**: cada ítem requiere una decisión
explícita registrada en el registro de decisiones antes de codearse. Los
identificadores (A-01…) son **estables**: no se renumeran al cerrarse un ítem, para
que las referencias desde los ADRs nunca se rompan.

## Abiertos

**A-01 · Formato de snapshot para logs largos.** Optimización de replay; jamás
fuente de verdad (I2).

**A-02 · Notificaciones locales.** Canal de comando local y eventos suscribibles.
Los canales de equipo (Slack, dashboard) son del proyecto de servidor separado.

**A-03 · Perfiles de permisos de red y filesystem por nodo.** Más allá de
ReadOnly/Edit/Full, como extensión del modelo unificado de §6.1 del Contrato.

**A-04 · `yunta serve`.** Fuera de alcance (D77): las capacidades de equipo (estado
compartido, gates remotos, dashboard, notificaciones de equipo, triggers sin padre
común) pertenecen a un proyecto separado con su propio repositorio y modelo de
negocio. Yunta no tiene daemon ni feature `serve`.

**A-05 · Registry central de packs.** Búsqueda, ratings, `pack publish` con métricas
agregadas.

**A-06 · Firma criptográfica de packs, recibos y la cadena de eventos.** El hash del
lock y la cadena de hashes del event log (D102) dan integridad y orden —nadie alteró
lo escrito— nunca autoría: quién lo escribió. La firma es la capa separada que agrega
autenticidad. Forma propuesta: una firma detached sobre la cabeza de cadena que el
recibo ya publica (el `event_hash` final) y sobre el hash del lock del pack,
verificable sin reprocesar el contenido; la gestión de claves —qué identidad firma,
dónde viven las claves— queda fuera de v1 y entra con su propio ADR de activación.

**A-07 · Dependencias transitivas entre packs.** Deliberadamente fuera; si se
habilita, profundidad acotada y sin resolución de grafos de versiones estilo npm.

**A-08 · Pipeline por lotes (consumo incremental entre nodos concurrentes).** Pares
productor/consumidor solapados, manteniendo la mediación del engine (D26): cada parte
es un artifact o `finding_posted` con evento, jamás conversación en caliente. Formas
posibles: `kind: pipeline` o `until: upstream_done && queue_empty`. **Gatillo
medible:** que `yunta stats` sobre runs reales muestre la espera por dependencias
dominando el wall-clock (referencia >30 % en workflows de uso diario). Bordes a fijar
antes de diseñar: invalidación retroactiva (el consumidor rechaza la parte 3 cuando
el productor va por la 7), backpressure, resume y promoción a mitad de cola, y
simulabilidad en `mock`. Ahorra latencia, no tokens; antes de diseñarlo, verificar
que la secuencialidad dolorosa no sea granularidad de nodos mal cortada.

**A-09 · `isolation: container`.** Fuera del schema (D63) por no estar diseñado.
Para entrar necesita: definición de imagen y montajes, cómo cruza el run.dir la
frontera, interacción con `permissions.network` y con los adapters (el CLI del
agente, ¿adentro o afuera?). No confundir con sandboxing de seguridad: §6.1 del
Contrato es explícito en que el aislamiento real pertenece al entorno de ejecución.

**A-10 · Constructor visual de workflows.** Descartado (D75). Gatillo: demanda
demostrada del segmento no técnico. Si llega: web, y con el YAML como fuente de
verdad; cualquier estado que la interfaz guarde y el YAML no exprese rompe
versionado, packs y portabilidad. `yunta graph` cubre la necesidad de *ver* un DAG.

**A-11 · Captura de salida de nodos `executor` para `node-output:`.** Hoy solo
`kind: bash` deja su salida donde `node-output:` la lee: `execute_bash` la captura al
salir el proceso, éxito o fallo por igual. Un `kind: executor` corre por el mismo
contrato stdin/stdout, pero el engine parsea su stdout como resultado (`ExecutorOutput`)
y no lo materializa: un nodo correctivo que dependa de él no tiene qué leer. Forma
propuesta: escribir el stdout capturado con el mismo `write_node_output` que usa bash,
sin nueva superficie. Gatillo: el primer pack que encadene un executor con un nodo
correctivo.

**A-12 · Campos del contrato JSON de `kind: executor`.** El contrato fija la forma
(JSON por stdin, JSON por stdout, exit code como veredicto) y no nombra los campos:
`with` es entrada opaca definida por el executor, y la salida que el engine parsea
(`ExecutorOutput`, un `summary` opcional) la define el módulo que la lee, no la spec.
Para entrar necesita: el objeto de entrada mínimo (identidad del run y del nodo, `with`,
rutas del run.dir), el objeto de salida (veredicto, diagnóstico, artifacts producidos) y
un `schema_version` propio, versionado como los eventos (spec-events §2).

## Riesgos conocidos

- **Dependencia de flags headless de los CLIs.** Mitigada por diseño (todo flag
  vive en el adapter y se valida en `probe()` contra la versión instalada); las
  capacidades reales de resume, hooks y agentes por versión se relevan contra cada
  CLI instalado.
- **Nombre.** `yunta`, `yunta-core`, `yunta-storage`, `yunta-adapters` y
  `yunta-engine` deben estar libres en crates.io antes de publicar (D03).

## Cerrados

Se conservan para que nadie los reabra por accidente; el ADR citado manda.

- Schema de payloads por evento → **D70** (§3.1) y la spec de eventos campo por campo.
- Allowlist/denylist de comandos por capa org → **D51** (§6.1, modelo unificado de
  permisos).
- Comunicación entre nodos paralelos → **D49** (`coordination: independent` default,
  `blackboard` opt-in).
- Gate de escalación tipo cuestionario → **D50** (§5.3).
- Contrato de executors externos → **D47** (JSON stdin/stdout; WASM descartado).
- Locks de merge concurrente → **D48** (Yunta no mergea; crea PRs).
- Política org sobre executors y publishers de packs → **D51 + D72**.
- Prompt injection en packs de terceros → **D71** (audit inventaría, no juzga).
- Paralelismo de tareas del ledger → **D65** (§5.5).

## Ideas estacionadas (no comprometidas)

- Vocabulario interno temático ("tiros", "la yunta"): descartado para el schema;
  podría vivir solo en branding.
- Logo: yugo estilizado como dos nodos conectados.
