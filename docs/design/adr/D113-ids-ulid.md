# D113 — Ids: ULID para runs; hijos y sucesores con id propio y vínculo en el log

**Estado:** propuesta.

## Contexto

Los ids de run se derivan de un timestamp con formato propio y los runs hijos y sucesores codifican su relación en el nombre. Un nombre que codifica relaciones es estado derivado como fuente de verdad: si el vínculo existe solo en el nombre, no está en el log.

## Decisión propuesta

Todo run recibe un ULID generado por un `IdSource` inyectado (determinista en tests). La relación padre/hijo vive en `child_run_created` y la relación de promoción en `promoted_from`; el nombre no dice nada sobre ninguna de las dos.

## Racional

ULID ordena por tiempo, cabe en un path y no colisiona entre procesos concurrentes. Inyectar la fuente de ids cumple la regla de determinismo inyectado que ya rige para el reloj. La relación en el log es replayable y auditable; en el nombre no.

## Alternativas descartadas

- UUID v4: sin orden temporal, incómodo en listados.
- Conservar el formato propio: no colisiona hoy, pero acopla el nombre a relaciones que el log ya registra.
