# D122 — Acceso a storage desde código async con `spawn_blocking`

**Estado:** propuesta.

## Contexto

`Storage` es síncrono (rusqlite) y se invoca directamente desde código async del engine, bloqueando el hilo del runtime durante cada escritura.

## Decisión propuesta

Toda invocación a storage desde código async pasa por `spawn_blocking`, con una conexión por invocación abierta desde el path congelado en el manifest.

## Racional

El log es local y las escrituras son cortas: `spawn_blocking` es suficiente, no introduce un actor ni un canal, y conserva la interfaz síncrona que hace a `Storage` trivial de testear.

## Alternativas descartadas

- Hilo dedicado con canal: más piezas para el mismo resultado en un log local.
- Driver async: cambia el backend por un problema de scheduling.
