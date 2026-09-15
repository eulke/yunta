# Deuda consciente y cuestiones abiertas

Lo deliberadamente no resuelto. Regla: **nada de esta lista se resuelve
implícitamente durante la implementación**: cada ítem requiere una decisión
explícita registrada en el registro de decisiones antes de codearse. La lista lleva
además lo construido y todavía no verificado contra el sistema real que describe,
donde lo que cierra el ítem es una corrida y no una decisión; cada entrada dice cuál
de las dos la resuelve. Los identificadores (A-01…) son **estables**: no se
renumeran al cerrarse un ítem, para que las referencias desde los ADRs nunca se
rompan.

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

**A-13 · El cerco en los adapters.** Cerrada por D172 (`plan-de-raiz/cerco.md`,
ítem 3-08): `yunta_core::fence::Fence::judge` es el único juez, `yunta fence
<adapter-id>` el hook que los CLIs ejecutan, `claude-code` lo instala como
`PreToolUse`, `codex` cerca por el sandbox de su proceso, y el mock juzga
cada efecto de su fixture por la misma función. Lo que el cerco no pudo
evitar sigue atrapándolo el diff del post-check, y una escritura que cruza un
cerco declarado exacto es además un `engine_finding`.

**A-14 · Preguntas respondibles por pull request.** `Channel` es `{tty, mcp}`;
un `kind: questions` se responde por consola. Lo resolvería una forja que
publique las preguntas y lea las respuestas, con su `Channel::Pr` (D167). El
par `questions_asked`/`questions_answered` (D173) es la forma que esa forja
publica y lee, como `gate_waiting`/`gate_resolved`; la tool MCP que responde
por el otro canal es el ítem 5-06.

**A-15 · Fuentes de contexto provistas por executors.** `ContextSpec` es una
enum cerrada de ocho fuentes. Lo resolvería un extension point con contrato
propio (entrada, salida, hash de lo materializado) y su ADR (D167).

**A-16 · Viaje en el tiempo: `yunta replay` y `yunta diff`.** RFC-0003 §2 promete
reconstruir lo que cualquier agente tenía delante en cualquier instante: `yunta
replay <run> --at <seq>` rinde el estado del run, el contexto montado y la evidencia
disponible en ese punto, y `yunta diff <run-a> <run-b>` reporta, entre dos corridas
del mismo workflow, el primer punto de divergencia y qué contexto difería. D55 las
registra como capacidades de producto post-v1 temprano y fija no recortar nada de esa
persistencia "por eficiencia", para no cerrar la puerta. Es deuda porque la data está
entera y ningún comando la lee de vuelta: el log está secuenciado y encadenado por
hash, cada `context_assembled` guarda el hash de cada segmento de contexto
(spec-events §5.8, D42) —el insumo directo del diff—, y el binario no tiene
subcomando `replay` ni `diff`. Lo resolvería la superficie de reconstrucción sobre
ese log: un comando que rinda estado, contexto y evidencia a un `seq` dado, y otro
que compare los hashes de segmento de dos corridas y nombre el primer segmento que
difiere.

**A-17 · Verificación en vivo de adapters, forja y MCP.** Los adapters `codex` y
`claude-code`, la forja de GitHub, `yunta mcp` montado en un cliente real, el MCP
por-run con un agente real y `pack add` contra un host remoto están construidos
contra documentación y ejemplos reales, sin una corrida contra el sistema que
describen (`status.md`). Los dos cercos son parte de eso: qué parte del stderr del
hook de Claude Code llega al `tool_result` del stream, y con qué marca `codex` un
proceso que su sandbox denegó. Es deuda porque lo único que confirma el contrato de
un sistema ajeno es una corrida contra él: hasta que ocurra, el comportamiento que el
adapter espera es el documentado y no el medido. La resuelve la checklist de
`smoke-checklist.md`, que describe cada corrida y su protocolo —cada divergencia se
corrige en su propia tarea, con el test de regresión que la hubiera atrapado, y el
resultado de cada corrida se registra en `status.md`—; pide binarios autenticados y
un token con repo descartable. No espera una decisión: espera credenciales y una
sesión.

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
- Paralelismo de tareas del documento de tareas → **D65** (§5.5).

## Ideas estacionadas (no comprometidas)

- Vocabulario interno temático ("tiros", "la yunta"): descartado para el schema;
  podría vivir solo en branding.
- Logo: yugo estilizado como dos nodos conectados.
