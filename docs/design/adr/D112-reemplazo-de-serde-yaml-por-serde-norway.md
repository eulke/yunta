---
number: D112
title: "Reemplazo de `serde_yaml` por `serde_norway`, un fork mantenido con la misma API"
status: accepted
revises: []
revised_by: []
---

# D112 — Reemplazo de `serde_yaml` por `serde_norway`, un fork mantenido con la misma API

Todo YAML de autor y los artifacts interpretados se parsean con
`serde_norway`, que continúa la línea `0.9` de `serde_yaml` sin cambios de
API, bajo `MIT OR Apache-2.0` y sin dependencias transitivas nuevas; el cambio
se valida con los fixtures de round-trip de `crates/core/tests/fixtures/`, que
definen el comportamiento que el reemplazo conserva byte a byte en
serialización.

Racional: `serde_yaml` está archivado por su autor; un parser sin
mantenimiento en la frontera de entrada de todo el sistema es un pasivo de
seguridad que crece con el tiempo y cuyos advisories futuros no tendrán fix.
Los fixtures de referencia existen precisamente para que un cambio así sea
verificable mecánicamente.

Descartado: mantener `serde_yaml` con `ignore` en cargo-deny (pospone el costo
y lo agranda); `serde_yaml_ng` (misma solución, pero cambia la API en su línea
`0.10` y licencia solo MIT); cambiar el formato de autoría (el YAML es el
contrato con los usuarios y los packs).
