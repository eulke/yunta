---
number: D153
title: "Cada sesión nombra su propio directorio de scratch, desde la identidad que el engine ya tiene"
status: accepted
revises: []
revised_by: []
---

# D153 — Cada sesión nombra su propio directorio de scratch, desde la identidad que el engine ya tiene

`SessionSlot` — el nodo, su reparación, o una tarea de un loop — deriva
`scratch/sessions/…` y `SessionRequest.scratch_dir` pasa a ser de la sesión,
no del run.

Racional: era del run, y las sesiones que pueden estar vivas a la vez lo
compartían: dos intentos concurrentes de tareas de un mismo loop escribían su
config MCP sobre el del otro, cada uno con el token de su propia sesión. El
adapter lo esquivaba derivando el nombre del archivo del puerto de la URL, lo
que ataba la identidad de una sesión a un detalle de transporte — y el
fallback de ese parseo no podía dispararse nunca, porque una URL sin puerto
devuelve cadena vacía, no ausencia. Con el directorio por sesión el archivo se
llama `mcp.json` y no hay nada que parsear. `SessionSetup` pierde su `Default`
y gana el nodo: un setup que se construye sin el nodo al que pertenece es un
estado inválido representable.

Descartados: derivar el nombre de un contador (no es identidad, y no sobrevive
a un resume); un subdirectorio por adapter (el que colisiona es la sesión, no
el adapter).
