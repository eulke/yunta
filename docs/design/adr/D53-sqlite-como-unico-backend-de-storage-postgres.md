---
number: D53
title: "SQLite como único backend de storage; Postgres deja de ser feature planificada"
status: accepted
revises: []
revised_by: []
---

# D53 — SQLite como único backend de storage; Postgres deja de ser feature planificada

Definitivo, no intermedio: (a) requerir Postgres contradice D05 (tooling sin
infraestructura) — el único escenario que lo justifica es `serve` compartido,
que ya está fuera de v1; (b) la carga es el caso ideal de SQLite: log
append-only, lecturas secuenciales por run_id, escritor local, WAL banca la
concurrencia de una máquina; (c) el miedo a la migración dolorosa está
desactivado por diseño — filas `(run_id, seq, kind, payload_json)` versionadas
son el formato más portable posible, la retención expira la mayoría de los
datos, y el estado es derivable por replay (I2): migrar es copiar eventos, no
destejer un schema. Condición que compra la opcionalidad futura: el módulo
`storage` detrás de una interfaz mínima (append, stream por run, snapshot) y
SQL sin dialectismos regados — Postgres futuro es un adapter de storage nuevo,
no una migración (misma jugada que los adapters de CLI). Camino intermedio sin
código: Litestream para replicar el SQLite si un equipo quiere durabilidad
compartida antes de serve.

Descartados: Postgres desde v1 (infra donde prometimos ninguna), feature
`postgres` vacía declarada "por las dudas" (compromiso especulativo), y
abstracción de storage genérica multi-motor (basta la interfaz mínima).
