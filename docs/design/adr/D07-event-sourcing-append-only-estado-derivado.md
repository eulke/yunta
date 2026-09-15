---
number: D07
title: "Event sourcing append-only; estado derivado por replay"
status: revised
revises: []
revised_by: []
---

# D07 — Event sourcing append-only; estado derivado por replay

*(Revisada: D53 fijó SQLite como único backend, definitivo — Postgres ya no es
feature planificada.)* Snapshots solo como optimización. Da resume tras crash
gratis y auditoría total. SQLite único backend. Eventos y payloads versionados
(`schema_version`) por tipo desde v0.1; migraciones de DB embebidas en el
binario.
