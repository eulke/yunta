# D131 — Ciclo de reparación de artifacts interpretados

## Contexto

Cuando un artifact interpretado no se puede leer, el nodo falla y no hay
reintento: `close_artifacts` devuelve sus errores y el cierre los registra con
`retryable: false`. La sesión que escribió el archivo ya murió cuando corre la
verificación, así que tampoco hay a quién devolverle el problema. El run pausa
y espera a una persona.

Es la única frontera del engine sin ciclo. El trabajo tiene el suyo desde el
principio: criterio en rojo, sesión, criterio en verde, y si sigue rojo se
reintenta con sesión nueva hasta un tope, y recién ahí escala a un gate. Una
declaración ilegible — que suele ser una clave de más o un `cmd:` faltante —
no tiene nada de eso.

El campo `retryable` que `node_failed` ya persiste no lo lee nadie: se escribe,
se deriva a `NodeState::Failed` y los dos sitios que hacen match lo descartan.
Es una promesa vacía, de las que D120 y D121 ya retiraron en otros lados.

Hay además una asimetría con el otro camino de entrada del mismo dato. Un
finding reportado en caliente por la tool `yunta_post_finding` se valida contra
su schema y el agente recibe el rechazo dentro de su sesión, a tiempo de
corregirlo; el mismo finding escrito como artifact falla el nodo sin apelación.
`contrato-del-run.md` §4.1 declara ese principio para la vía en caliente:
"reportar mal es un error visible, no un texto libre que después nadie puede
procesar". La vía del archivo no lo cumple.

## Decisión

Un artifact interpretado ilegible abre un ciclo de reparación: el nodo falla
`retryable: true` y el engine reabre una sesión con el mismo prompt más los
diagnósticos de D130 redactados para un agente, contra un tope propio. Agotado
el tope, el nodo falla como hoy y el run pausa o escala según su política.

El ciclo solo cubre lo que un agente puede arreglar reescribiendo el archivo:
YAML ilegible, claves desconocidas, tipos equivocados, reglas de registro
violadas. Un artifact ausente, vacío o por encima de `limits.max_artifact_bytes`
falla directo — no hay archivo que corregir, y un tope excedido es una guardia
contra accidentes, no una negociación.

Cada intento deja su evento, con los diagnósticos que lo motivaron, igual que
un intento del ciclo de tarea.

El tope se declara en `limits:`, junto al resto, y se congela en el manifest —
la regla de D117: un número que gobierna un run tiene que ser auditable desde
el recibo. **El default queda abierto**: `DEFAULT_MAX_RETRIES` vale 2 para
tareas y reusarlo es lo coherente, pero corregir una declaración es más barato
y más determinista que corregir trabajo, así que el número puede no ser el
mismo. Es una decisión de quien mantiene el producto, no del código.

## Racional

El engine ya paga la sesión cara — la que planificó, la que auditó — y la tira
entera por una clave de más. Reintentar una escritura con el problema explícito
es la corrección más barata del sistema: no hay que rehacer el análisis, solo
la transcripción, y el diagnóstico de D130 dice exactamente qué cambiar.

Que sea un ciclo y no un reintento a ciegas es lo que lo hace verificable: se
reintenta con el motivo en la mano, se registra cada intento, y el tope es
declarado y congelado. Un reintento sin el diagnóstico repetiría el mismo error
— que es, de hecho, lo que hace hoy el ciclo de tarea, cuyo comentario dice
"every attempt is a fresh session with the same request".

Y cierra la asimetría: el mismo dato, escrito por las dos vías que el sistema
ofrece, recibe el mismo trato.

## Descartado

**Reintentar sin diagnóstico**, reusando el prompt tal cual. Es lo que ya
existe en el ciclo de tarea y su rendimiento es el esperable: sin saber qué
falló, la segunda escritura repite la primera.

**Reanudar la sesión en vez de abrir una nueva.** Depende de una capacidad del
adapter (`resume_session`), y I8 exige que reejecutar sea siempre seguro
porque la rehidratación es completa. Una sesión nueva con el diagnóstico
cumple lo mismo sin capacidad de por medio.

**Un nodo correctivo declarado con `on_failure.goto`.** Ya se puede escribir y
no resuelve nada: el nodo destino recibe su propio prompt sin la causa, que
solo llega si el autor cablea a mano `context: [{run-events: {filter:
failed}}]`. Obliga a cada workflow del ecosistema a resolver por su cuenta algo
que es del engine.

**Tope fijo en el código.** Un límite que gobierna un run y no aparece en el
manifest no es auditable desde el recibo (D117).
