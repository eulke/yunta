---
number: D164
title: "El run se lee como una crónica derivada, y cada superficie la dispone"
status: accepted
revises: [D162, D45]
revised_by: []
---

# D164 — El run se lee como una crónica derivada, y cada superficie la dispone

El engine deriva del log, junto al frame, una *crónica*: un momento por
evento, en el orden del log, en los mismos tipos que el frame usa para decir
dónde está cada cosa (`NodeState`, `Reroute`, `ChildLink`, `Degradation`,
`ToolCall`, `OpenSession`, `ResolvedRunner`). `Happening` tiene la forma de
`EventPayload`: nueve brazos, uno por dominio, y cada dominio es dueño de la
lectura de sus kinds. Es pura y monótona en el log: la crónica de un prefijo es
prefijo de la crónica. El CLI elige las palabras de un momento una sola vez,
con el mismo vocabulario de estados que el frame, y cada superficie solo las
dispone: la región dibuja del frame lo que está abierto; el scrollback
conserva de la crónica lo que cerró algo o pidió algo a una persona (un
`node_finished`/`node_failed`, un `node_rerouted`, un `gate_waiting`, un
`gate_resolved`, un `questions_answered`, un `run_paused`, un `run_resumed`,
un `promotion_signaled`, un `child_run_finished`, un `capability_degraded`, un
`finding_posted`/`finding_withdrawn`, un kind desconocido); la superficie
append-only escribe todos los momentos con el tiempo del run adelante. Un
test de propiedad sostiene que el frame concuerda con la crónica, y otro que
lo que una terminal observada conserva es lo que la superficie append-only
escribe.

Especificación: `docs/design/plan-de-raiz/cronica.md`.

Racional: dos derivaciones con dos vocabularios se pagan dos veces por cada
kind nuevo y divergen sin que ningún test lo diga; un scrollback que deduce
"qué cerró" comparando frames necesita memoria por nodo (`Scrollback::gone`),
donde vivió un defecto real.

Descartados: que el modo append-only dibuje frames (un frame no es una línea,
y un lector después del hecho quiere el orden); declarar las dos superficies
deliberadamente distintas y corregir D162 (barato, pero deja dos vocabularios
sin nada que los ate); un `--format json` como salida de máquina (queda
habilitado por el tipo, no lo reemplaza); conservar más en el scrollback
(tareas que se mueven, artifacts aceptados) — un nodo `loop` ya muestra sus
tareas corriendo en la región, y lo que un lector reencuentra arriba es lo que
cambió el rumbo del run; conservar solo nodos que cierran (deja fuera lo que
pidió a una persona).
