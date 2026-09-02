# D123 — Superficie pública del engine por área

**Estado:** propuesta.

## Contexto

`yunta_engine` re-exporta más de cien ítems planos desde `lib.rs`, muchos públicos solo porque un test de integración los usa.

## Decisión propuesta

Módulos públicos por área — `engine::{run, check, replay, receipt, stats, catalog, packs}` — con lo que el CLI y los adapters necesitan; el resto es `pub(crate)` y se prueba por comportamiento a través de la superficie pública.

## Racional

Lo que solo usan los tests no es API. Una superficie por área hace legible qué expone el engine y qué es detalle, y convierte en error de compilación cualquier uso del CLI que cruce la frontera por un atajo.

## Alternativas descartadas

- Conservar el re-export plano: cada ítem nuevo agranda una superficie que nadie diseñó.
