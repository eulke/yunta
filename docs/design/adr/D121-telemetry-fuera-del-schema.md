# D121 — `telemetry:` sale del schema hasta que exista el exportador

**Estado:** propuesta.

## Contexto

La config acepta `telemetry: { enabled, endpoint, protocol }` y no existe exportador: `enabled: true` no produce nada.

## Decisión propuesta

Retirar `telemetry:` del schema; vuelve junto con el exportador OTel (D96) y su propio ADR de activación.

## Racional

Una clave que no hace nada es una degradación silenciosa por diseño. La decisión de D96 sobre el formato de exportación no necesita una clave inerte para sobrevivir.

## Alternativas descartadas

- Conservarla documentada como inerte: la documentación de una no-funcionalidad es ruido para el usuario.
