---
number: D24
title: "Re-rutas `on_failure.goto` como segundo conjunto de aristas"
status: accepted
revises: []
revised_by: []
---

# D24 — Re-rutas `on_failure.goto` como segundo conjunto de aristas

Con `max_reroutes` obligatorio y retorno automático (destino completa → nodo
fallido re-corre). El nodo correctivo no sabe quién lo invocó → reutilizable.
Escalera de corrección: hook mecánico → criteria del ledger → re-ruta.
