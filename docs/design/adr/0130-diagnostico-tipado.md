# D130 — El diagnóstico es un tipo, no una cadena

## Contexto

"Con diagnóstico" es la coletilla de I11 y aparece más de una docena de veces
en el corpus: un límite excedido, una fuente caída, una cadena de hashes rota,
un artifact ilegible fallan "con diagnóstico". El corpus nunca dice qué es un
diagnóstico, qué debe contener ni quién lo lee. Un concepto sin definición no
recibe un tipo, y lo que quedó en su lugar es una cadena de texto.

`close_artifacts` produce `Vec<ArtifactError>`: cada violación tipada, con el
nodo, el nombre del artifact y la causa conservada. Su único consumidor las
convierte en una sola cadena con `.join("; ")`, y esa cadena entra al event log
como `node_failed.outcome: String`. Desde ahí no se puede volver atrás.

Las consecuencias son visibles en toda superficie:

- La prosa cruda de serde llega al usuario. `tasks[0].criteria[0]: invalid
  type: string "cargo test", expected struct Criterion at line 6 column 9` es
  lo que ve alguien que escribió un plan. `tasks[0]` no es vocabulario de este
  dominio, `struct Criterion` es un nombre de Rust y `expected` describe lo que
  quería un deserializador, no lo que el sistema necesita.
- El detalle que sí es de dominio se descarta. `ledger.rs` produce siete reglas
  con mensajes precisos, y `ArtifactError::InvalidLedger` guarda ese vector y
  su `Display` imprime solo el conteo: `failed validation with 4 error(s)`.
  `spec-ledger.md` §4 fija normativamente el formato contrario, una violación
  por línea con tarea, campo y expectativa. El CLI ya tiene esa función
  (`error_block`), y la usa para `check` y nunca para esto.
- La cadena se anida. Un nodo `kind: workflow` envuelve el motivo del hijo en
  el suyo, así que tres niveles de composición producen `child run c failed:
  node n failed: child run d failed: node m failed: <n errores unidos por ";">`
  en una sola línea.
- Nadie puede volver a formatearla. `yunta graph` tiene su propio saneador
  privado, duplicado en dos funciones, cuyo comentario dice que un outcome
  puede traer un salto de línea. `progress.md` la interpola cruda en un bullet
  de Markdown, sin escapar, y ese archivo alimenta el contexto del nodo
  siguiente: la prosa de serde termina llegando a otro agente. `yunta test` la
  imprime con `Debug`, que le agrega comillas escapadas. `yunta run --json` la
  descarta entera. El recibo la evita a propósito, y su comentario lo dice:
  "read from the event kind and the envelope's node id, never the node's
  outcome text".

Ese comentario es el diagnóstico del problema: la superficie que más necesita
ser confiable ya decidió que `outcome` no lo es.

## Decisión

Un diagnóstico es un valor estructurado y vive en `yunta-core`, junto a la
frontera de parseo. Lleva un código de un enum cerrado, el sujeto en el
vocabulario del dominio (`la tarea \`t1\``, `el criterio 1 de la tarea \`t1\``,
nunca `tasks[0].criteria[0]`), la ubicación en el archivo cuando existe, lo que
se esperaba y qué hacer.

Los diagnósticos viajan en grupo: una lectura fallida produce todos los
problemas del archivo de una vez, no el primero.

`node_failed` gana un campo aditivo con los diagnósticos en forma, junto al
`outcome` que ya tiene; el `outcome` queda como la redacción para una persona,
derivada de ellos. La regla de compatibilidad es la de D70: campo nuevo
opcional, lector viejo lo ignora, ningún `_v2`.

El texto para humanos se produce una sola vez, en el borde, con la forma que
`spec-ledger.md` §4 ya fija y que `error_block` ya implementa. La redacción
para un agente que va a reintentar (D131) sale del mismo valor por otro camino.

Ninguna superficie interpola un diagnóstico crudo: el saneado de saltos de
línea que `yunta graph` hace por su cuenta pasa a ser parte del renderizador,
en un solo lugar.

## Racional

El engine ya hace el trabajo caro — distinguir siete clases de violación de
ledger, ubicar el valor que falló, conservar la causa — y lo tira justo antes
de que sirva. Un tipo no agrega análisis: conserva el que ya existe hasta el
borde, que es donde se decide para quién se redacta.

Y sin un tipo no hay forma de tener dos lectores. El ciclo de reparación de
D131 necesita decirle a un agente qué corregir, y una persona necesita leer
otra cosa; una cadena redactada en el punto de falla ya eligió por los dos, y
eligió mal para ambos.

Que un diagnóstico sea dato y no prosa también lo vuelve contable: cuántos
runs fallan por un ledger ilegible es una pregunta que hoy solo se responde
haciendo `grep` sobre texto libre.

## Descartado

**Limpiar la cadena en cada superficie.** Es el estado actual llevado a su
conclusión: cada lugar reimplementa el saneado, como `graph.rs` ya hace dos
veces, y ninguno puede recuperar lo que se perdió en el `join`.

**Reemplazar `outcome` por el campo estructurado.** Rompe los logs escritos y
contradice la regla de evolución de D70, que existe justamente para que un
cambio de formato no invalide evidencia ya persistida.

**Traducir la prosa de serde por coincidencia de texto.** Ata el vocabulario
del producto a los mensajes internos de una dependencia, que cambian sin aviso
en un parche.
