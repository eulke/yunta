---
number: D118
title: "Señales a process groups con el crate `nix`"
status: revised
revises: []
revised_by: [D165]
---

# D118 — Señales a process groups con el crate `nix`

Las señales a procesos y process groups — interrupción, exterminio del árbol,
liveness (`kill -0`) — se envían con `nix` (features `signal` y `process`, las
mínimas), desde un único helper tipado (*Revisada por D165: vive en
`yunta_core::process::signal`, junto al resto de la maquinaria de subprocesos,
que es el crate más bajo del workspace desde que el puerto está ahí*); el
engine, el CLI, los adapters y los tests lo consumen. Toda señal devuelve un
error tipado con el `errno` real, y `forbid(unsafe_code)` rige en el workspace
entero, tests incluidos.

Racional: una llamada de sistema envuelta con tipos es exactamente el caso en
que una dependencia se justifica: reemplaza un subproceso (`kill`) que
dependía de un binario externo en el `PATH` y perdía el código de error de la
llamada, y su superficie es mínima. El helper no vive en `yunta-engine` porque
los adapters también señalan sus propias sesiones y las dependencias solo van
hacia abajo.

Descartado: `libc` directo (exige `unsafe` en código propio); conservar el
binario `kill` (sin errores tipados y dependiente del `PATH`).
