---
number: D07
title: "Event sourcing append-only; estado derivado por replay"
status: revised
revises: []
revised_by: [D53]
---

# D07 — Event sourcing append-only; estado derivado por replay

*(Revisada por D53: SQLite es el único backend de storage, y Postgres no es
una feature planificada.)* Snapshots solo como optimización. Da resume tras
crash gratis y auditoría total. SQLite único backend. Eventos y payloads
versionados (`schema_version`) por tipo desde v0.1; migraciones de DB
embebidas en el binario.
