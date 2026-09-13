---
number: D170
title: "Los umbrales de la superficie y del arnés son estos, y cambiarlos es revisar esta decisión"
status: accepted
revises: []
revised_by: []
---

# D170 — Los umbrales de la superficie y del arnés son estos, y cambiarlos es revisar esta decisión

| constante | valor | dónde | por qué |
|---|---|---|---|
| `WAIT_DEADLINE` | 10 s | `testkit/src/wait.rs` | techo de una espera explícita en tests: suficiente para un proceso real en CI cargado, corto para que un cuelgue no consuma el runner |
| `INTERRUPT_GRACE_PERIOD` | 200 ms | `engine/src/task_cycle/session.rs` | lo que un CLI que honra SIGINT tarda en salir antes de que el grupo reciba SIGKILL |
| `ENGINE_SHUTDOWN_POLL` | 200 ms | `cli/src/commands/cancel.rs` | cadencia con la que `cancel` mira si el engine soltó el registro |
| `QUEUE_DEPTH` | 1024 | `cli/src/surface/mod.rs` | profundidad de la cola engine→pintor: una ráfaga de nodos paralelos entra entera; un pintor atrasado vuelve al log en vez de replayar un minuto viejo |
| `REDRAW_CEILING_HZ` | 20 | `cli/src/surface/mod.rs` | techo de redibujos por segundo que los eventos piden entre latidos |
| `REDRAW_INTERVAL` | 1 s | `cli/src/surface/painter.rs` | latido propio del pintor: las duraciones se muestran en segundos enteros, más rápido redibuja lo mismo |
| `MIN_SAMPLES_FOR_ESTIMATION` | 3 | `engine/src/history.rs` | debajo de tres runs una mediana y un p90 no dicen nada |
| `SLOWEST` | 3 | `cli/src/surface/closing.rs` | nodos más lentos que el bloque de cierre nombra |
| `SHOWN` | 4 | `cli/src/surface/view.rs` | llamadas recientes que una fila de la región muestra |
| stagger del blackboard | 60 ms | `engine/tests/blackboard.rs` | separación entre dos posteos para probar que la consolidación ordena por contenido y no por llegada; se reemplaza por una propiedad sobre permutaciones de llegada en M21 |

Un `const` numérico nuevo en `src` lleva en su rustdoc la referencia a esta
decisión o a la que lo fije; el ratchet `numeric_const_without_adr` lo mide.

Racional: cada uno de estos valores tenía rustdoc con su porqué y ninguno
tenía decisión registrada; CLAUDE.md dice que un umbral sin decisión es una
decisión que alguien tomó solo.

Descartados: un ADR por umbral (dispersa lo que se decide junto); dejar los
valores solo en rustdoc (no es un registro).
