---
number: D113
title: "Ids: ULID para runs; hijos y sucesores con id propio y vínculo en el log"
status: accepted
revises: []
revised_by: []
---

# D113 — Ids: ULID para runs; hijos y sucesores con id propio y vínculo en el log

Todo run recibe un ULID acuñado por un `IdSource` inyectado (`SystemIdSource`
en producción, una fuente secuencial en tests), del mismo modo que el reloj
entra por `Clock`. La relación padre/hijo vive en `child_run_created` y la
relación de promoción en `promoted_from`; el nombre de un run no codifica
ninguna de las dos.

Racional: un nombre que codifica relaciones es estado derivado usado como
fuente de verdad — si el vínculo existe solo en el nombre, no está en el log
ni es replayable. ULID ordena por tiempo, cabe en un path y no colisiona entre
procesos concurrentes; inyectar la fuente cumple la regla de determinismo
inyectado que ya rige para el reloj.

Descartado: UUID v4 (sin orden temporal, incómodo en listados); conservar el
formato `run-<fecha>-<pid>` y los sufijos `-<nodo>`/`-promoted` (no colisiona
hoy, pero acopla el nombre a relaciones que el log ya registra).
