---
number: D39
title: "Distribución de packs git-first con vendoring y lockfile; roles como contrato de portabilidad; `declares` como techo de permisos"
status: accepted
revises: []
revised_by: []
---

# D39 — Distribución de packs git-first con vendoring y lockfile; roles como contrato de portabilidad; `declares` como techo de permisos

`pack add` vendorea a `.yunta/packs/<publisher>/` y pinea por hash en
`yunta.lock`; nada se auto-actualiza (coherente con manifest inmutable). El
pack declara roles requeridos y jamás runners/modelos/secretos; el instalador
los resuelve con su config. `declares` es techo validado por check, no
descripción. Executors en packs = código: confirmación explícita y revisión
aparte. Sin registry central, sin deps transitivas ni firma en v1 (deuda).

Descartado: distribución estilo npm con resolución de grafos de versiones.
