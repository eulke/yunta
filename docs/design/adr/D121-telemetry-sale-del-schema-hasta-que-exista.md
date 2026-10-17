---
number: D121
title: "`telemetry:` sale del schema hasta que exista el exportador"
status: accepted
revises: []
revised_by: []
---

# D121 — `telemetry:` sale del schema hasta que exista el exportador

La config deja de aceptar `telemetry: { enabled, endpoint, protocol }`; la
clave vuelve junto con el exportador OTel (D96) y su propio ADR de activación.

Racional: una clave que no hace nada es una degradación silenciosa por diseño,
y la decisión de D96 sobre el formato de exportación no necesita una clave
inerte para sobrevivir.

Descartado: conservarla documentada como inerte (la documentación de una
no-funcionalidad es ruido para el usuario).
