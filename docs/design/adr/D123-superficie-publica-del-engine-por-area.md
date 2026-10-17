---
number: D123
title: "Superficie pública del engine por área"
status: accepted
revises: []
revised_by: []
---

# D123 — Superficie pública del engine por área

`yunta-engine` expone módulos públicos por área — `run`, `check`, `replay`,
`receipt`, `stats`, `catalog`, `packs` — con lo que el CLI y los adapters
necesitan; el resto es `pub(crate)` y se prueba por comportamiento a través de
la superficie pública.

Racional: lo que solo usan los tests no es API; una superficie por área hace
legible qué expone el engine y qué es detalle, y convierte en error de
compilación cualquier uso del CLI que cruce la frontera por un atajo.

Descartado: el re-export plano de más de cien ítems desde `lib.rs` (cada ítem
nuevo agranda una superficie que nadie diseñó).
