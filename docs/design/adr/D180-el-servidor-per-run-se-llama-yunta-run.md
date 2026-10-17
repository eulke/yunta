---
number: D180
title: "El servidor per-run se llama `yunta-run`; una sesión que muere dice cómo salió; `doctor --session` abre una sesión real"
status: accepted
revises: [D147]
revised_by: []
---

# D180 — El servidor per-run se llama `yunta-run`; una sesión que muere dice cómo salió; `doctor --session` abre una sesión real

## Contexto

Reportado desde Codex: el usuario registra el control plane como
`[mcp_servers.yunta] command = "yunta" args = ["mcp"]`; el adapter inyecta
el servidor per-run como `-c mcp_servers.yunta.url=…` sobre la misma
tabla, y Codex rechaza `url is not supported for stdio`. La sesión muere
antes de su primera línea; el stderr que lo dice se drena a `tracing` y el
nodo falla con «session ended without a terminal event» y 0 tokens;
`doctor` dice sano porque `probe()` corre `--version` y nunca abre una
sesión (plan de raíz, §11 L-106).

## Decisión

1. **El servidor per-run tiene nombre propio.** `RunToolsEndpoint::SERVER_NAME`
   es `yunta-run`; el control plane lo registra el usuario con el nombre
   que quiera. D147 nombraba `mcp_servers.yunta`; esta decisión lo revisa.
2. **Una sesión que muere dice cómo salió.** `AgentSession::exit` responde,
   con el código o la señal de salida del proceso y las últimas
   `STDERR_TAIL_LINES = 20` líneas de stderr. Interrogar es matar primero:
   mata el grupo, cierra las cañerías y recoge la salida, en ese orden —el
   mismo que `kill()`—, así que la espera está acotada por construcción y
   ninguna sesión sobrevive a su run. Se pregunta sólo a la sesión cuyo
   stream terminó sin evento terminal: una que cerró su turno no paga nada,
   y una sin proceso propio responde `None`. Cómo salió es una unión
   cerrada, `SessionEnd { Code, Signal }`, tolerante a un `type` que un
   binario viejo no conoce; no dos opcionales que admitirían un proceso que
   no dice cómo murió. Lo que el hijo escribió en stderr entra redactado:
   su entorno es donde este sistema pone sus secretos, y el token de las
   run tools vive ahí. La muerte llega tipada por los dos caminos que abren
   sesiones: el nodo de prompt falla con `Failure::SessionDied { died:
   SessionDeath }` y el nodo `loop` con el mismo hecho, tomado de la
   primera tarea que bloqueó por una muerte; de ahí al log, a `status`, a
   `--json` y a la crónica. La prosa se produce en el borde y nombra cada
   forma que el tipo admite, la ausencia incluida.
3. **`doctor --session` abre una sesión real por binding.** Un run por
   binding —adapter, modelo y agente— que algún runner nombre, con un solo
   nodo `kind: prompt` y las run tools montadas, por el mismo camino que un
   workflow y en el sandbox de `yunta test`, con los adapters reales y el
   entorno de la invocación. El binding y no el nombre, porque una sesión
   ejercita un binding: dos runners que nombran el mismo no se prueban dos
   veces, y el que un runner tiene de reserva —el que un run alcanza
   justamente cuando el primero está caído— se prueba también. Un run por
   binding, y no un run con un nodo por binding, porque la salud del
   adapter rechaza la invocación entera: así el que muere se reporta como
   él mismo. Se intentan los bindings cuyo adapter ya probó sano; los demás
   los reporta el `doctor` de siempre. El run de una prueba no mide
   baseline: su veredicto es si la sesión abre, nunca qué medía el árbol.
   Gasta un prompt por binding, por eso es opt-in, y `doctor` sin la
   bandera dice qué garantiza —que el binario está, responde y autentica— y
   qué no.

## Racional

Frontera: dos servidores son dos nombres. Degradación explícita: una
sesión que muere al arrancar deja de llegar como «0 tokens». Un camino de
ejecución: `doctor --session` no abre sesiones por una puerta propia; corre
un run. Veinte líneas de stderr alcanzan para leer el error de arranque de
un CLI y no para que un log de sesión entero viaje en un evento.

## Alternativas descartadas

Avisar en la documentación que el control plane no se registre como
`yunta`: el nombre es el natural y la colisión es nuestra. Emitir un
evento terminal desde el adapter cuando el proceso muere: el adapter nunca
inventa un terminal —el contrato del stream es del engine—; el engine
pregunta y registra. Que `doctor`
pruebe la sesión siempre: gasta tokens sin que nadie lo pida.
