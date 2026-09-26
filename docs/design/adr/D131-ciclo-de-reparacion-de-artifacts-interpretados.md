---
number: D131
title: "Ciclo de reparación de artifacts interpretados, con tope declarado"
status: retired
revises: []
revised_by: [D156]
---

# D131 — Ciclo de reparación de artifacts interpretados, con tope declarado

*(Retirada por D156: no hay ciclo de reparación; un rechazo se corrige en la
misma sesión.)* Un artifact interpretado cuyo contenido no se puede leer falla
el nodo `retryable: true` y el engine abre una sesión de corrección sobre el
runner del nodo, con la forma que ese nodo ya monta y los problemas de la
lectura, contra `limits.max_artifact_repairs` (default 1), declarado y
congelado en el manifest como todo límite que gobierna un run. Cada reparación
es un intento propio en el log: `node_failed` seguido de `node_started` es una
secuencia que la derivación ya aceptaba, con lo cual el intento se ve en
`status`, se cuenta en `stats` y se reproduce por replay sin eventos nuevos.
El ciclo cubre solo lo que una reescritura arregla — YAML ilegible, claves
desconocidas, tipos equivocados, reglas del documento violadas — y lo dice el
tipo de la falla, no un predicado: `ArtifactFailure::Content` se repara,
`ArtifactFailure::File` — ausente, vacío, por encima de `max_artifact_bytes`,
rechazado por el filesystem — falla directo (D134). El ciclo está escrito una
sola vez y lo tiene todo nodo que resuelve un runner, sea cual sea su kind; un
nodo sin runner lo dice en su tipo (D138).

Racional: era la única frontera del engine sin ciclo, mientras el trabajo
tiene el suyo desde el principio; el engine paga la sesión cara y la tiraba
entera por una clave de más, cuando reintentar la transcripción con el
problema en la mano no exige rehacer el análisis. Cierra además la asimetría
con `yunta_post_finding`, que valida en caliente y devuelve el rechazo dentro
de la sesión (Contrato §4.1), mientras el mismo dato escrito como artifact
fallaba sin apelación. El default es 1 y no los 2 del ciclo de tarea porque,
con la forma ya publicada (D129), una reescritura que tiene el diagnóstico
converge en el primer intento o no converge.

Descartados: reintentar sin diagnóstico (es lo que hace el ciclo de tarea, y
sin saber qué falló la segunda escritura repite la primera); reanudar la
sesión en vez de abrir una nueva (depende de una capability, y I8 exige que
reejecutar sea siempre seguro); un nodo correctivo por `on_failure.goto` (ya
se podía escribir y el destino no recibe la causa salvo que el autor cablee
`run-events` a mano); tope fijo en el código (no aparece en el manifest,
contra D117).
