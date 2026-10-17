---
number: D122
title: "Acceso a storage desde código async con `spawn_blocking`"
status: accepted
revises: []
revised_by: []
---

# D122 — Acceso a storage desde código async con `spawn_blocking`

Toda invocación a storage desde código async pasa por `spawn_blocking`, con
una conexión por invocación abierta desde el path congelado en el manifest;
`Storage` conserva su interfaz síncrona.

Racional: el log es local y las escrituras son cortas; `spawn_blocking` es
suficiente, no introduce un actor ni un canal y conserva la interfaz síncrona
que hace a `Storage` trivial de testear. Invocar rusqlite directamente desde
el runtime bloqueaba el hilo durante cada escritura.

Descartado: hilo dedicado con canal (más piezas para el mismo resultado en un
log local); driver async (cambia el backend por un problema de scheduling).
